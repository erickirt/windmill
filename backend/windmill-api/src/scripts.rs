/*
 * Author: Ruben Fiszel
 * Copyright: Windmill Labs, Inc 2022
 * This file and its contents are licensed under the AGPLv3 License.
 * Please see the included NOTICE for copyright information and
 * LICENSE-AGPL for a copy of the license.
 */

use crate::{
    auth::AuthCache,
    db::{ApiAuthed, DB},
    schedule::clear_schedule,
    triggers::{
        get_triggers_count_internal, list_tokens_internal, TriggersCount, TruncatedTokenWithEmail,
    },
    users::{maybe_refresh_folders, require_owner_of_path},
    utils::{check_scopes, WithStarredInfoQuery},
    webhook_util::{WebhookMessage, WebhookShared},
    HTTP_CLIENT,
};
use axum::extract::Multipart;

use axum::{
    extract::{Extension, Path, Query},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use hyper::StatusCode;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::value::RawValue;
use sql_builder::prelude::*;
use sqlx::{FromRow, Postgres, Transaction};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::Arc,
};
use windmill_audit::audit_oss::audit_log;
use windmill_audit::ActionKind;
use windmill_worker::process_relative_imports;

use windmill_common::{
    assets::{clear_asset_usage, insert_asset_usage, AssetUsageKind, AssetWithAltAccessType},
    error::to_anyhow,
    worker::CLOUD_HOSTED,
};

use windmill_common::{
    db::UserDB,
    error::{Error, JsonResult, Result},
    jobs::JobPayload,
    schedule::Schedule,
    schema::should_validate_schema,
    scripts::{
        to_i64, HubScript, ListScriptQuery, ListableScript, NewScript, Schema, Script, ScriptHash,
        ScriptHistory, ScriptHistoryUpdate, ScriptKind, ScriptLang, ScriptWithStarred,
    },
    users::username_to_permissioned_as,
    utils::{
        not_found_if_none, paginate, query_elems_from_hub, require_admin, Pagination, StripPath,
    },
    worker::to_raw_value,
    HUB_BASE_URL,
};
use windmill_git_sync::{handle_deployment_metadata, DeployedObject};
use windmill_parser_ts::remove_pinned_imports;
use windmill_queue::{schedule::push_scheduled_job, PushIsolationLevel};

const MAX_HASH_HISTORY_LENGTH_STORED: usize = 20;

#[derive(Serialize, sqlx::FromRow)]
pub struct ScriptWDraft {
    pub hash: ScriptHash,
    pub path: String,
    pub summary: String,
    pub description: String,
    pub content: String,
    pub language: ScriptLang,
    pub kind: ScriptKind,
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<sqlx::types::Json<Box<RawValue>>>,
    pub schema: Option<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub envs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrent_limit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency_time_window_s: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_ttl: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedicated_worker: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_error_handler_muted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_unless_cancelled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_after_use: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_to_runner_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_main_func: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_preprocessor: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_behalf_of_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(json(nullable))]
    pub assets: Option<Vec<AssetWithAltAccessType>>,
}

pub fn global_service() -> Router {
    Router::new()
        .route("/hub/top", get(get_top_hub_scripts))
        .route("/hub/get/*path", get(get_hub_script_by_path))
        .route("/hub/get_full/*path", get(get_full_hub_script_by_path))
}

pub fn global_unauthed_service() -> Router {
    Router::new()
        .route(
            "/tokened_raw/:workspace/:token/*path",
            get(get_tokened_raw_script_by_path),
        )
        .route("/empty_ts/*path", get(get_empty_ts_script_by_path))
}

pub fn workspaced_service() -> Router {
    Router::new()
        .route("/list", get(list_scripts))
        .route("/list_search", get(list_search_scripts))
        .route("/create", post(create_script))
        .route("/create_snapshot", post(create_snapshot_script))
        .route("/archive/p/*path", post(archive_script_by_path))
        .route("/get/draft/*path", get(get_script_by_path_w_draft))
        .route("/get/p/*path", get(get_script_by_path))
        .route("/get_triggers_count/*path", get(get_triggers_count))
        .route("/list_tokens/*path", get(list_tokens))
        .route("/raw/p/*path", get(raw_script_by_path))
        .route("/raw_unpinned/p/*path", get(raw_script_by_path_unpinned))
        .route("/exists/p/*path", get(exists_script_by_path))
        .route("/archive/h/:hash", post(archive_script_by_hash))
        .route("/delete/h/:hash", post(delete_script_by_hash))
        .route("/delete/p/*path", post(delete_script_by_path))
        .route("/get/h/:hash", get(get_script_by_hash))
        .route("/raw/h/:hash", get(raw_script_by_hash))
        .route("/deployment_status/h/:hash", get(get_deployment_status))
        .route("/list_paths", get(list_paths))
        .route(
            "/toggle_workspace_error_handler/p/*path",
            post(toggle_workspace_error_handler),
        )
        .route("/history/p/*path", get(get_script_history))
        .route("/get_latest_version/*path", get(get_latest_version))
        .route(
            "/list_paths_from_workspace_runnable/*path",
            get(list_paths_from_workspace_runnable),
        )
        .route(
            "/history_update/h/:hash/p/*path",
            post(update_script_history),
        )
}

#[derive(Serialize, FromRow)]
pub struct SearchScript {
    path: String,
    content: String,
}
async fn list_search_scripts(
    authed: ApiAuthed,
    Path(w_id): Path<String>,
    Extension(user_db): Extension<UserDB>,
) -> JsonResult<Vec<SearchScript>> {
    let mut tx = user_db.begin(&authed).await?;
    #[cfg(feature = "enterprise")]
    let n = 1000;

    #[cfg(not(feature = "enterprise"))]
    let n = 10;

    let rows = sqlx::query_as!(
        SearchScript,
        "SELECT path, content from script WHERE workspace_id = $1 AND archived = false LIMIT $2",
        &w_id,
        n
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect::<Vec<_>>();
    tx.commit().await?;
    Ok(Json(rows))
}

async fn list_scripts(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path(w_id): Path<String>,
    Query(pagination): Query<Pagination>,
    Query(lq): Query<ListScriptQuery>,
) -> JsonResult<Vec<ListableScript>> {
    let (per_page, offset) = paginate(pagination);
    let mut sqlb = SqlBuilder::select_from("script as o")
        .fields(&[
            "hash",
            "o.path",
            "summary",
            "COALESCE(draft.created_at, o.created_at) as created_at",
            "archived",
            "extra_perms",
            "CASE WHEN lock_error_logs IS NOT NULL THEN true ELSE false END as has_deploy_errors",
            "language",
            "favorite.path IS NOT NULL as starred",
            "tag",
            "draft.path IS NOT NULL as has_draft",
            "draft_only",
            "ws_error_handler_muted",
            "no_main_func",
            "codebase IS NOT NULL as use_codebase",
            "kind"
        ])
        .left()
        .join("favorite")
        .on(
            "favorite.favorite_kind = 'script' AND favorite.workspace_id = o.workspace_id AND favorite.path = o.path AND favorite.usr = ?"
                .bind(&authed.username),
        )
        .left()
        .join("draft")
        .on(
            "draft.path = o.path AND draft.workspace_id = o.workspace_id AND draft.typ = 'script'"
        )
        .order_desc("favorite.path IS NOT NULL")
        .order_by("created_at", lq.order_desc.unwrap_or(true))
        .and_where("o.workspace_id = ?".bind(&w_id))
        .offset(offset)
        .limit(per_page)
        .clone();

    let lowercased_kinds: Option<Vec<String>> = lq
        .kinds
        .map(|x| x.split(",").map(&str::to_lowercase).collect());

    if (!lq.include_without_main.unwrap_or(false)
        && lowercased_kinds
            .as_ref()
            .map(|x| !x.contains(&"preprocessor".to_string()))
            .unwrap_or(true))
        || authed.is_operator
    {
        // only include scripts that have a main function
        // do not hide scripts without main if preprocessor is in the kinds
        sqlb.and_where("o.no_main_func IS NOT TRUE");
    }

    if !lq.include_draft_only.unwrap_or(false) || authed.is_operator {
        sqlb.and_where("draft_only IS NOT TRUE");
    }

    if lq.show_archived.unwrap_or(false) {
        sqlb.and_where_eq(
            "o.ctid",
            "(SELECT ctid FROM script 
              WHERE path = o.path 
                AND workspace_id = ? 
              ORDER BY created_at DESC 
              LIMIT 1)"
                .bind(&w_id),
        );
        sqlb.and_where_eq("archived", true);
    } else {
        sqlb.and_where_eq("archived", false);
    }
    if let Some(ps) = &lq.path_start {
        sqlb.and_where_like_left("o.path", ps);
    }
    if let Some(p) = &lq.path_exact {
        sqlb.and_where_eq("o.path", "?".bind(p));
    }
    if let Some(cb) = &lq.created_by {
        sqlb.and_where_eq("created_by", "?".bind(cb));
    }
    if let Some(ph) = &lq.first_parent_hash {
        sqlb.and_where_eq("parent_hashes[1]", &ph.0);
    }
    if let Some(ph) = &lq.last_parent_hash {
        sqlb.and_where_eq("parent_hashes[array_upper(parent_hashes, 1)]", &ph.0);
    }
    if let Some(ph) = &lq.parent_hash {
        sqlb.and_where_eq("any(parent_hashes)", &ph.0);
    }
    if let Some(it) = &lq.is_template {
        sqlb.and_where_eq("is_template", it);
    }
    if authed.is_operator {
        sqlb.and_where_eq("kind", quote("script"));
    } else if let Some(lowercased_kinds) = lowercased_kinds {
        let safe_kinds = lowercased_kinds
            .into_iter()
            .map(sql_builder::quote)
            .collect_vec();
        if safe_kinds.len() > 0 {
            sqlb.and_where_in("kind", safe_kinds.as_slice());
        }
    }
    if lq.starred_only.unwrap_or(false) {
        sqlb.and_where_is_not_null("favorite.path");
    }

    if lq.with_deployment_msg.unwrap_or(false) {
        sqlb.join("deployment_metadata dm")
            .left()
            .on("dm.script_hash = o.hash")
            .fields(&["dm.deployment_msg"]);
    }

    if let Some(languages) = lq.languages {
        sqlb.and_where_in(
            "language",
            &languages
                .iter()
                .map(|language| quote(language.as_str()))
                .collect_vec(),
        );
    }

    let sql = sqlb.sql().map_err(|e| Error::internal_err(e.to_string()))?;
    let mut tx = user_db.begin(&authed).await?;
    let rows = sqlx::query_as::<_, ListableScript>(&sql)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct TopHubScriptsQuery {
    limit: Option<i64>,
    app: Option<String>,
    kind: Option<String>,
}

async fn get_top_hub_scripts(
    Query(query): Query<TopHubScriptsQuery>,
    Extension(db): Extension<DB>,
) -> impl IntoResponse {
    let mut query_params = vec![];
    if let Some(query_limit) = query.limit {
        query_params.push(("limit", query_limit.to_string().clone()));
    }
    if let Some(query_app) = query.app {
        query_params.push(("app", query_app.to_string().clone()));
    }
    if let Some(query_kind) = query.kind {
        query_params.push(("kind", query_kind.to_string().clone()));
    }

    let (status_code, headers, response) = query_elems_from_hub(
        &HTTP_CLIENT,
        &format!("{}/scripts/top", *HUB_BASE_URL.read().await),
        Some(query_params),
        &db,
    )
    .await?;
    Ok::<_, Error>((status_code, headers, response))
}

fn hash_script(ns: &NewScript) -> i64 {
    let mut dh = DefaultHasher::new();
    ns.hash(&mut dh);
    dh.finish() as i64
}

async fn create_snapshot_script(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(webhook): Extension<WebhookShared>,
    Extension(db): Extension<DB>,
    Path(w_id): Path<String>,
    mut multipart: Multipart,
) -> Result<(StatusCode, String)> {
    let mut script_hash = None;
    let mut tx = None;
    let mut uploaded = false;
    let mut handle_deployment_metadata = None;
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        let data = field.bytes().await.unwrap();
        if name == "script" {
            let ns: NewScript = Some(serde_json::from_slice(&data).map_err(to_anyhow)?).unwrap();
            let is_tar = ns.codebase.as_ref().is_some_and(|x| x.ends_with(".tar"));

            let (new_hash, ntx, hdm) = create_script_internal(
                ns,
                w_id.clone(),
                authed.clone(),
                db.clone(),
                user_db.clone(),
                webhook.clone(),
            )
            .await?;
            let nh = new_hash.to_string();
            script_hash = Some(if is_tar { format!("{nh}.tar") } else { nh });
            tx = Some(ntx);
            handle_deployment_metadata = hdm;
        }
        if name == "file" {
            let hash = script_hash.as_ref().ok_or_else(|| {
                Error::BadRequest(
                    "script need to be passed first in the multipart upload".to_string(),
                )
            })?;

            uploaded = true;

            #[cfg(all(feature = "enterprise", feature = "parquet"))]
            let object_store = windmill_common::s3_helpers::get_object_store().await;

            #[cfg(not(all(feature = "enterprise", feature = "parquet")))]
            let object_store: Option<()> = None;

            if &windmill_common::utils::MODE_AND_ADDONS.mode
                == &windmill_common::utils::Mode::Standalone
                && object_store.is_none()
            {
                std::fs::create_dir_all(
                    windmill_common::worker::ROOT_STANDALONE_BUNDLE_DIR.clone(),
                )?;
                windmill_common::worker::write_file_bytes(
                    &windmill_common::worker::ROOT_STANDALONE_BUNDLE_DIR,
                    &hash,
                    &data,
                )?;
            } else {
                #[cfg(not(all(feature = "enterprise", feature = "parquet")))]
                {
                    return Err(Error::ExecutionErr("codebase is an EE feature".to_string()));
                }

                #[cfg(all(feature = "enterprise", feature = "parquet"))]
                if let Some(os) = object_store {
                    let path = windmill_common::s3_helpers::bundle(&w_id, &hash);

                    if let Err(e) = os
                        .put(&object_store::path::Path::from(path.clone()), data.into())
                        .await
                    {
                        tracing::info!("Failed to put snapshot to s3 at {path}: {:?}", e);
                        return Err(Error::ExecutionErr(format!("Failed to put {path} to s3")));
                    }
                } else {
                    return Err(Error::BadConfig("Object store is required for snapshot script and is not configured for servers".to_string()));
                }
            }
        }
        // println!("Length of `{}` is {} bytes", name, data.len());
    }
    if !uploaded {
        return Err(Error::BadRequest("No file uploaded".to_string()));
    }
    if script_hash.is_none() {
        return Err(Error::BadRequest(
            "No script found in the uploaded file".to_string(),
        ));
    }

    tx.unwrap().commit().await?;
    if let Some(hdm) = handle_deployment_metadata {
        hdm.handle(&db).await?;
    }
    return Ok((StatusCode::CREATED, format!("{}", script_hash.unwrap())));
}

async fn list_paths_from_workspace_runnable(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> JsonResult<Vec<String>> {
    let mut tx = user_db.begin(&authed).await?;
    let runnables = sqlx::query_scalar!(
        r#"SELECT importer_path FROM dependency_map 
            WHERE workspace_id = $1 AND imported_path = $2"#,
        w_id,
        path.to_path(),
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(runnables))
}

async fn create_script(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(webhook): Extension<WebhookShared>,
    Extension(db): Extension<DB>,
    Path(w_id): Path<String>,
    Json(ns): Json<NewScript>,
) -> Result<(StatusCode, String)> {
    let (hash, tx, hdm) =
        create_script_internal(ns, w_id, authed, db.clone(), user_db, webhook).await?;
    tx.commit().await?;
    if let Some(hdm) = hdm {
        hdm.handle(&db).await?;
    }
    Ok((StatusCode::CREATED, format!("{}", hash)))
}

struct HandleDeploymentMetadata {
    email: String,
    created_by: String,
    w_id: String,
    obj: DeployedObject,
    deployment_message: Option<String>,
}

impl HandleDeploymentMetadata {
    async fn handle(self, db: &DB) -> Result<()> {
        handle_deployment_metadata(
            &self.email,
            &self.created_by,
            &db,
            &self.w_id,
            self.obj,
            self.deployment_message,
            false,
        )
        .await
    }
}

async fn create_script_internal<'c>(
    ns: NewScript,
    w_id: String,
    authed: ApiAuthed,
    db: sqlx::Pool<Postgres>,
    user_db: UserDB,
    webhook: WebhookShared,
) -> Result<(
    ScriptHash,
    Transaction<'c, Postgres>,
    Option<HandleDeploymentMetadata>,
)> {
    check_scopes(&authed, || format!("scripts:write:{}", ns.path))?;

    let codebase = ns.codebase.as_ref();
    #[cfg(not(feature = "enterprise"))]
    if ns.ws_error_handler_muted.is_some_and(|val| val) {
        return Err(Error::BadRequest(
            "Muting the error handler for certain script is only available in enterprise version"
                .to_string(),
        ));
    }
    if *CLOUD_HOSTED {
        let nb_scripts =
            sqlx::query_scalar!("SELECT COUNT(*) FROM script WHERE workspace_id = $1", &w_id)
                .fetch_one(&db)
                .await?;
        if nb_scripts.unwrap_or(0) >= 5000 {
            return Err(Error::BadRequest(
                    "You have reached the maximum number of scripts (5000) on cloud. Contact support@windmill.dev to increase the limit"
                        .to_string(),
                ));
        }

        if ns.summary.len() > 300 {
            return Err(Error::BadRequest(
                "Summary must be less than 300 characters on cloud".to_string(),
            ));
        }
        if ns.description.len() > 3000 {
            return Err(Error::BadRequest(
                "Description must be less than 3000 characters on cloud".to_string(),
            ));
        }
    }
    let script_path = ns.path.clone();
    let hash = ScriptHash(hash_script(&ns));
    let authed = maybe_refresh_folders(&ns.path, &w_id, authed, &db).await;
    let mut tx: Transaction<'_, Postgres> = user_db.begin(&authed).await?;
    if sqlx::query_scalar!(
        "SELECT 1 FROM script WHERE hash = $1 AND workspace_id = $2",
        hash.0,
        &w_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .is_some()
    {
        return Err(Error::BadRequest(
            "A script with same hash (hence same path, description, summary, content) already \
             exists!"
                .to_owned(),
        ));
    };
    let clashing_script = sqlx::query_as::<_, Script>(
        "SELECT * FROM script WHERE path = $1 AND archived = false AND workspace_id = $2",
    )
    .bind(&ns.path)
    .bind(&w_id)
    .fetch_optional(&mut *tx)
    .await?;
    struct ParentInfo {
        p_hashes: Vec<i64>,
        perms: serde_json::Value,
        p_path: String,
    }
    let parent_hashes_and_perms: Option<ParentInfo> = match (&ns.parent_hash, clashing_script) {
        (None, None) => Ok(None),
        (None, Some(s)) if !s.draft_only.unwrap_or(false) => Err(Error::BadRequest(format!(
            "Path conflict for {} with non-archived hash {}",
            &ns.path, &s.hash
        ))),
        (None, Some(s)) => {
            sqlx::query!(
                "DELETE FROM script WHERE hash = $1 AND workspace_id = $2",
                s.hash.0,
                &w_id
            )
            .execute(&mut *tx)
            .await?;
            Ok(None)
        }
        (Some(p_hash), o) => {
            if sqlx::query_scalar!(
                "SELECT 1 FROM script WHERE hash = $1 AND workspace_id = $2",
                p_hash.0,
                &w_id
            )
            .fetch_optional(&mut *tx)
            .await?
            .is_none()
            {
                return Err(Error::BadRequest(
                    "The parent hash does not seem to exist".to_owned(),
                ));
            };

            let clashing_hash_o = sqlx::query_scalar!(
                "SELECT hash FROM script WHERE parent_hashes[1] = $1 AND workspace_id = $2",
                p_hash.0,
                &w_id
            )
            .fetch_optional(&mut *tx)
            .await?;

            if let Some(clashing_hash) = clashing_hash_o {
                return Err(Error::BadRequest(format!(
                    "A script with hash {} with same parent_hash has been found. However, the \
                         lineage must be linear: no 2 scripts can have the same parent",
                    ScriptHash(clashing_hash)
                )));
            };

            let ScriptWithStarred { script: ps, .. } =
                get_script_by_hash_internal(&mut tx, &w_id, p_hash, None).await?;

            if ps.path != ns.path {
                require_owner_of_path(&authed, &ps.path)?;
            }

            let ph = {
                let v = ps.parent_hashes.map(|x| x.0).unwrap_or_default();
                let mut v: Vec<i64> = v
                    .into_iter()
                    .take(MAX_HASH_HISTORY_LENGTH_STORED - 1)
                    .collect();
                v.insert(0, p_hash.0);
                v
            };
            let r: Result<Option<ParentInfo>> = match o {
                Some(clashing_script)
                    if clashing_script.path == ns.path && clashing_script.hash.0 != p_hash.0 =>
                {
                    Err(Error::BadRequest(format!(
                        "Path conflict for {} with non-archived hash {}",
                        &ns.path, &clashing_script.hash
                    )))
                }
                Some(_) | None => Ok(Some(ParentInfo {
                    p_hashes: ph,
                    perms: ps.extra_perms,
                    p_path: ps.path,
                })),
            };
            sqlx::query!(
                "UPDATE script SET archived = true WHERE hash = $1 AND workspace_id = $2",
                p_hash.0,
                &w_id
            )
            .execute(&mut *tx)
            .await?;

            sqlx::query!(
                "DELETE FROM asset WHERE workspace_id = $1 AND usage_kind = 'script' AND usage_path = (SELECT path FROM script WHERE hash = $2 AND workspace_id = $1)",
                &w_id,
                p_hash.0
            )
            .execute(&mut *tx)
            .await?;

            r
        }
    }?;
    let p_hashes = parent_hashes_and_perms.as_ref().map(|v| &v.p_hashes[..]);
    let extra_perms = parent_hashes_and_perms
        .as_ref()
        .map(|v| v.perms.clone())
        .unwrap_or(json!({}));
    let lock = if ns.codebase.is_some() {
        Some(String::new())
    } else if !(
        ns.language == ScriptLang::Python3
            || ns.language == ScriptLang::Go
            || ns.language == ScriptLang::Bun
            || ns.language == ScriptLang::Bunnative
            || ns.language == ScriptLang::Deno
            || ns.language == ScriptLang::Rust
            || ns.language == ScriptLang::Ansible
            || ns.language == ScriptLang::CSharp
            || ns.language == ScriptLang::Nu
            || ns.language == ScriptLang::Php
            || ns.language == ScriptLang::Java
        // for related places search: ADD_NEW_LANG
    ) {
        Some(String::new())
    } else {
        ns.lock.as_ref().and_then(|e| {
            if e.is_empty() {
                None
            } else {
                Some(e.to_string())
            }
        })
    };

    let needs_lock_gen = lock.is_none() && codebase.is_none();
    let envs = ns.envs.as_ref().map(|x| x.as_slice());
    let envs = if ns.envs.is_none() || ns.envs.as_ref().unwrap().is_empty() {
        None
    } else {
        envs
    };

    let lang = if &ns.language == &ScriptLang::Bun || &ns.language == &ScriptLang::Bunnative {
        let anns = windmill_common::worker::TypeScriptAnnotations::parse(&ns.content);
        if anns.native {
            ScriptLang::Bunnative
        } else {
            ScriptLang::Bun
        }
    } else {
        ns.language.clone()
    };

    let validate_schema = should_validate_schema(&ns.content, &ns.language);

    let (no_main_func, has_preprocessor) = if matches!(ns.kind, Some(ScriptKind::Preprocessor)) {
        (ns.no_main_func, ns.has_preprocessor)
    } else {
        match lang {
            ScriptLang::Bun | ScriptLang::Bunnative | ScriptLang::Deno | ScriptLang::Nativets => {
                let args = windmill_parser_ts::parse_deno_signature(&ns.content, true, true, None);
                match args {
                    Ok(args) => (args.no_main_func, args.has_preprocessor),
                    Err(e) => {
                        tracing::warn!(
                            "Error parsing deno signature when deploying script {}: {:?}",
                            ns.path,
                            e
                        );
                        (None, None)
                    }
                }
            }
            ScriptLang::Python3 => {
                let args = windmill_parser_py::parse_python_signature(&ns.content, None, true);
                match args {
                    Ok(args) => (args.no_main_func, args.has_preprocessor),
                    Err(e) => {
                        tracing::warn!(
                            "Error parsing python signature when deploying script {}: {:?}",
                            ns.path,
                            e
                        );
                        (None, None)
                    }
                }
            }
            _ => (ns.no_main_func, ns.has_preprocessor),
        }
    };

    sqlx::query!(
        "INSERT INTO script (workspace_id, hash, path, parent_hashes, summary, description, \
         content, created_by, schema, is_template, extra_perms, lock, language, kind, tag, \
         draft_only, envs, concurrent_limit, concurrency_time_window_s, cache_ttl, \
         dedicated_worker, ws_error_handler_muted, priority, restart_unless_cancelled, \
         delete_after_use, timeout, concurrency_key, visible_to_runner_only, no_main_func, codebase, has_preprocessor, on_behalf_of_email, schema_validation, assets) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::text::json, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34)",
        &w_id,
        &hash.0,
        ns.path,
        p_hashes,
        ns.summary,
        ns.description,
        &ns.content,
        &authed.username,
        ns.schema.and_then(|x| serde_json::to_string(&x.0).ok()),
        ns.is_template.unwrap_or(false),
        extra_perms,
        lock,
        lang as ScriptLang,
        ns.kind.unwrap_or(ScriptKind::Script) as ScriptKind,
        ns.tag,
        ns.draft_only,
        envs,
        ns.concurrent_limit,
        ns.concurrency_time_window_s,
        ns.cache_ttl,
        ns.dedicated_worker,
        ns.ws_error_handler_muted.unwrap_or(false),
        ns.priority,
        ns.restart_unless_cancelled,
        ns.delete_after_use,
        ns.timeout,
        ns.concurrency_key,
        ns.visible_to_runner_only,
        no_main_func.filter(|x| *x), // should be Some(true) or None
        codebase,
        has_preprocessor.filter(|x| *x), // should be Some(true) or None
        if ns.on_behalf_of_email.is_some() {
            Some(&authed.email)
        } else {
            None
        },
        validate_schema,
        ns.assets.as_ref().and_then(|a| serde_json::to_value(a).ok())
    )
    .execute(&mut *tx)
    .await?;
    let p_path_opt = parent_hashes_and_perms.as_ref().map(|x| x.p_path.clone());
    if let Some(ref p_path) = p_path_opt {
        sqlx::query!(
            "DELETE FROM draft WHERE path = $1 AND workspace_id = $2 AND typ = 'script'",
            p_path,
            &w_id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            "UPDATE capture_config SET path = $1 WHERE path = $2 AND workspace_id = $3 AND is_flow IS FALSE",
            ns.path,
            p_path,
            w_id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            "UPDATE capture SET path = $1 WHERE path = $2 AND workspace_id = $3 AND is_flow IS FALSE",
            ns.path,
            p_path,
            w_id
        )
        .execute(&mut *tx)
        .await?;

        let mut schedulables = sqlx::query_as::<_, Schedule>(
            "UPDATE schedule SET script_path = $1 WHERE script_path = $2 AND path != $2 AND workspace_id = $3 AND is_flow IS false RETURNING *")
            .bind(&ns.path)
            .bind(&p_path)
            .bind(&w_id)
        .fetch_all(&mut *tx)
        .await?;

        let schedule = sqlx::query_as::<_, Schedule>(
            "UPDATE schedule SET path = $1, script_path = $1 WHERE path = $2 AND workspace_id = $3 AND is_flow IS false RETURNING *")
            .bind(&ns.path)
            .bind(&p_path)
            .bind(&w_id)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(schedule) = schedule {
            schedulables.push(schedule);
        }

        for schedule in schedulables {
            clear_schedule(&mut tx, &schedule.path, &w_id).await?;

            if schedule.enabled {
                tx = push_scheduled_job(&db, tx, &schedule, None).await?;
            }
        }
    } else {
        sqlx::query!(
            "DELETE FROM draft WHERE path = $1 AND workspace_id = $2 AND typ = 'script'",
            ns.path,
            &w_id
        )
        .execute(&mut *tx)
        .await?;
    }
    if p_hashes.is_some() && !p_hashes.unwrap().is_empty() {
        audit_log(
            &mut *tx,
            &authed,
            "scripts.update",
            ActionKind::Update,
            &w_id,
            Some(&ns.path),
            Some([("hash", hash.to_string().as_str())].into()),
        )
        .await?;
        webhook.send_message(
            w_id.clone(),
            WebhookMessage::UpdateScript {
                workspace: w_id.clone(),
                path: ns.path.clone(),
                hash: hash.to_string(),
            },
        );
    } else {
        audit_log(
            &mut *tx,
            &authed,
            "scripts.create",
            ActionKind::Create,
            &w_id,
            Some(&ns.path),
            Some(
                [
                    ("workspace", w_id.as_str()),
                    ("hash", hash.to_string().as_str()),
                ]
                .into(),
            ),
        )
        .await?;
        webhook.send_message(
            w_id.clone(),
            WebhookMessage::CreateScript {
                workspace: w_id.clone(),
                path: ns.path.clone(),
                hash: hash.to_string(),
            },
        );
    }

    clear_asset_usage(&mut *tx, &w_id, &script_path, AssetUsageKind::Script).await?;
    for asset in ns.assets.as_ref().into_iter().flatten() {
        insert_asset_usage(&mut *tx, &w_id, &asset, &ns.path, AssetUsageKind::Script).await?;
    }

    let permissioned_as = username_to_permissioned_as(&authed.username);
    if let Some(parent_hash) = ns.parent_hash {
        tracing::info!(
            "creating script {hash:?} at path {script_path} with parent {parent_hash} on workspace {w_id}",
        );
    } else {
        tracing::info!("creating script {hash:?} at path {script_path} on workspace {w_id}",);
    }
    if needs_lock_gen {
        let tag = if ns.dedicated_worker.is_some_and(|x| x) {
            Some(format!("{}:{}", &w_id, &ns.path,))
        } else if ns.tag.as_ref().is_some_and(|x| x.contains("$args[")) {
            None
        } else {
            ns.tag
        };

        let mut args: HashMap<String, Box<serde_json::value::RawValue>> = HashMap::new();
        if let Some(dm) = ns.deployment_message {
            args.insert("deployment_message".to_string(), to_raw_value(&dm));
        }
        if let Some(ref p_path) = p_path_opt {
            args.insert("parent_path".to_string(), to_raw_value(&p_path));
        }

        let tx = PushIsolationLevel::Transaction(tx);
        let (_, new_tx) = windmill_queue::push(
            &db,
            tx,
            &w_id,
            JobPayload::Dependencies {
                hash,
                language: ns.language,
                path: ns.path,
                dedicated_worker: ns.dedicated_worker,
            },
            windmill_queue::PushArgs::from(&args),
            &authed.username,
            &authed.email,
            permissioned_as,
            authed.token_prefix.as_deref(),
            None,
            None,
            None,
            None,
            None,
            false,
            false,
            None,
            true,
            tag,
            None,
            None,
            None,
            Some(&authed.clone().into()),
        )
        .await?;
        Ok((hash, new_tx, None))
    } else {
        if codebase.is_none() {
            let db2 = db.clone();
            let w_id2 = w_id.clone();
            let authed2 = authed.clone();
            let permissioned_as2 = permissioned_as.clone();
            let script_path2 = script_path.clone();
            let parent_path = p_path_opt.clone();
            let lock = ns.lock.clone();
            let deployment_message = ns.deployment_message.clone();
            let content = ns.content.clone();
            let language = ns.language.clone();
            tokio::spawn(async move {
                // wait for 10 seconds to make sure the script is deployed and that the CLI sync that pushed it (f one) is complete
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                if let Err(e) = process_relative_imports(
                    &db2,
                    None,
                    None,
                    &w_id2,
                    &script_path2,
                    parent_path,
                    deployment_message,
                    &content,
                    &Some(language),
                    &authed2.email,
                    &authed2.username,
                    &permissioned_as2,
                    lock,
                )
                .await
                {
                    tracing::error!(%e, "error processing relative imports");
                }
            });
        }

        // handle_deployment_metadata(
        //     &authed.email,
        //     &authed.username,
        //     &db,
        //     &w_id,
        //     DeployedObject::Script {
        //         hash: hash.clone(),
        //         path: script_path.clone(),
        //         parent_path: p_path_opt,
        //     },
        //     ns.deployment_message,
        //     false,
        // )
        // .await?;

        Ok((
            hash,
            tx,
            Some(HandleDeploymentMetadata {
                email: authed.email,
                created_by: authed.username,
                w_id,
                obj: DeployedObject::Script {
                    hash: hash.clone(),
                    path: script_path.clone(),
                    parent_path: p_path_opt,
                },
                deployment_message: ns.deployment_message,
            }),
        ))
    }
}

pub async fn get_hub_script_by_path(
    Path(path): Path<StripPath>,
    Extension(db): Extension<DB>,
) -> Result<String> {
    windmill_common::scripts::get_hub_script_by_path(path, &HTTP_CLIENT, &db).await
}

pub async fn get_full_hub_script_by_path(
    Path(path): Path<StripPath>,
    Extension(db): Extension<DB>,
) -> JsonResult<HubScript> {
    Ok(Json(
        windmill_common::scripts::get_full_hub_script_by_path(path, &HTTP_CLIENT, Some(&db))
            .await?,
    ))
}

async fn get_script_by_path(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, path)): Path<(String, StripPath)>,
    Query(query): Query<WithStarredInfoQuery>,
) -> JsonResult<ScriptWithStarred> {
    let path = path.to_path();
    check_scopes(&authed, || format!("scripts:read:{}", path))?;
    let mut tx = user_db.begin(&authed).await?;

    let script_o = if query.with_starred_info.unwrap_or(false) {
        sqlx::query_as::<_, ScriptWithStarred>(
            "SELECT s.*, favorite.path IS NOT NULL as starred
            FROM script s
            LEFT JOIN favorite
            ON favorite.favorite_kind = 'script' 
                AND favorite.workspace_id = s.workspace_id 
                AND favorite.path = s.path 
                AND favorite.usr = $3
            WHERE s.path = $1
                AND s.workspace_id = $2
            ORDER BY s.created_at DESC LIMIT 1",
        )
        .bind(path)
        .bind(w_id)
        .bind(&authed.username)
        .fetch_optional(&mut *tx)
        .await?
    } else {
        sqlx::query_as::<_, ScriptWithStarred>(
            "SELECT *, NULL as starred FROM script WHERE path = $1 AND workspace_id = $2 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(path)
        .bind(w_id)
        .fetch_optional(&mut *tx)
        .await?
    };
    tx.commit().await?;

    let script = not_found_if_none(script_o, "Script", path)?;
    Ok(Json(script))
}

async fn list_tokens(
    Extension(db): Extension<DB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> JsonResult<Vec<TruncatedTokenWithEmail>> {
    let path = path.to_path();
    list_tokens_internal(&db, &w_id, &path, false).await
}

async fn get_triggers_count(
    Extension(db): Extension<DB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> JsonResult<TriggersCount> {
    let path = path.to_path();
    get_triggers_count_internal(&db, &w_id, &path, false).await
}

async fn get_script_by_path_w_draft(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> JsonResult<ScriptWDraft> {
    let path = path.to_path();
    check_scopes(&authed, || format!("scripts:read:{}", path))?;
    let mut tx = user_db.begin(&authed).await?;

    let script_o = sqlx::query_as::<_, ScriptWDraft>(
        "SELECT hash, script.path, summary, description, content, language, kind, tag, schema, draft_only, envs, concurrent_limit, concurrency_time_window_s, cache_ttl, ws_error_handler_muted, draft.value as draft, dedicated_worker, priority, restart_unless_cancelled, delete_after_use, timeout, concurrency_key, visible_to_runner_only, no_main_func, has_preprocessor, on_behalf_of_email, assets FROM script LEFT JOIN draft ON 
         script.path = draft.path AND script.workspace_id = draft.workspace_id AND draft.typ = 'script'
         WHERE script.path = $1 AND script.workspace_id = $2
         ORDER BY script.created_at DESC LIMIT 1",
    )
    .bind(path)
    .bind(w_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;

    let script = not_found_if_none(script_o, "Script", path)?;
    Ok(Json(script))
}

async fn get_script_history(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> JsonResult<Vec<ScriptHistory>> {
    let path = path.to_path();
    check_scopes(&authed, || format!("scripts:read:{}", path))?;
    let mut tx = user_db.begin(&authed).await?;
    let query_result = sqlx::query!(
        "SELECT s.hash as hash, dm.deployment_msg as deployment_msg 
        FROM script s LEFT JOIN deployment_metadata dm ON s.hash = dm.script_hash
        WHERE s.workspace_id = $1 AND s.path = $2
        ORDER by s.created_at DESC",
        w_id,
        path,
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    let result: Vec<ScriptHistory> = query_result
        .into_iter()
        .map(|row| ScriptHistory {
            script_hash: ScriptHash(row.hash),
            deployment_msg: row.deployment_msg,
        })
        .collect();
    return Ok(Json(result));
}

async fn get_latest_version(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> JsonResult<Option<ScriptHistory>> {
    let path = path.to_path();
    check_scopes(&authed, || format!("scripts:read:{}", path))?;
    let mut tx = user_db.begin(&authed).await?;
    let row_o = sqlx::query!(
        "SELECT s.hash as hash, dm.deployment_msg as deployment_msg 
        FROM script s LEFT JOIN deployment_metadata dm ON s.hash = dm.script_hash
        WHERE s.workspace_id = $1 AND s.path = $2
        ORDER by s.created_at DESC LIMIT 1",
        w_id,
        path,
    )
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;

    if let Some(row) = row_o {
        let result = ScriptHistory {
            script_hash: ScriptHash(row.hash),
            deployment_msg: row.deployment_msg, //
        };
        return Ok(Json(Some(result)));
    } else {
        return Ok(Json(None));
    }
}

async fn update_script_history(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, script_hash, script_path)): Path<(String, ScriptHash, StripPath)>,
    Json(script_history_update): Json<ScriptHistoryUpdate>,
) -> Result<()> {
    let script_path = script_path.to_path();
    check_scopes(&authed, || format!("scripts:write:{}", script_path))?;

    let mut tx = user_db.begin(&authed).await?;
    sqlx::query!(
        "INSERT INTO deployment_metadata (workspace_id, path, script_hash, deployment_msg) VALUES ($1, $2, $3, $4) ON CONFLICT (workspace_id, script_hash) WHERE script_hash IS NOT NULL DO UPDATE SET deployment_msg = $4",
        w_id,
        script_path,
        script_hash.0,
        script_history_update.deployment_msg,
    )
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    return Ok(());
}

async fn list_paths(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path(w_id): Path<String>,
) -> JsonResult<Vec<String>> {
    let mut tx = user_db.begin(&authed).await?;

    let scripts = sqlx::query_scalar!(
        "SELECT distinct(path) FROM script WHERE  workspace_id = $1",
        w_id
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Json(scripts))
}

#[derive(Deserialize)]
pub struct ToggleWorkspaceErrorHandler {
    #[cfg(feature = "enterprise")]
    pub muted: Option<bool>,
}

#[cfg(not(feature = "enterprise"))]
async fn toggle_workspace_error_handler(
    _authed: ApiAuthed,
    Extension(_user_db): Extension<UserDB>,
    Path((_w_id, _path)): Path<(String, StripPath)>,
    Json(_req): Json<ToggleWorkspaceErrorHandler>,
) -> Result<String> {
    return Err(Error::BadRequest(
        "Muting the error handler for certain script is only available in enterprise version"
            .to_string(),
    ));
}

#[cfg(feature = "enterprise")]
async fn toggle_workspace_error_handler(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, path)): Path<(String, StripPath)>,
    Json(req): Json<ToggleWorkspaceErrorHandler>,
) -> Result<String> {
    let mut tx = user_db.begin(&authed).await?;

    let error_handler_maybe: Option<String> = sqlx::query_scalar!(
        "SELECT error_handler FROM workspace_settings WHERE workspace_id = $1",
        w_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or(None);

    match error_handler_maybe {
        Some(_) => {
            sqlx::query_scalar!(
                "UPDATE script 
                SET ws_error_handler_muted = $3 
                WHERE ctid = (
                    SELECT ctid FROM script
                    WHERE path = $1 AND workspace_id = $2
                    ORDER BY created_at DESC
                    LIMIT 1
                )
",
                path.to_path(),
                w_id,
                req.muted,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok("".to_string())
        }
        None => {
            tx.commit().await?;
            Err(Error::ExecutionErr(
                "Workspace error handler needs to be defined".to_string(),
            ))
        }
    }
}

async fn get_tokened_raw_script_by_path(
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, token, path)): Path<(String, String, StripPath)>,
    Extension(cache): Extension<Arc<AuthCache>>,
) -> Result<String> {
    let authed = cache
        .get_authed(Some(w_id.clone()), &token)
        .await
        .ok_or_else(|| Error::NotAuthorized("Invalid token".to_string()))?;
    return raw_script_by_path(
        authed,
        Extension(user_db),
        Extension(db),
        Path((w_id, path)),
    )
    .await;
}

async fn get_empty_ts_script_by_path() -> String {
    return String::new();
}

async fn raw_script_by_path(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> Result<String> {
    raw_script_by_path_internal(path, user_db, db, authed, w_id, false).await
}

async fn raw_script_by_path_unpinned(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> Result<String> {
    raw_script_by_path_internal(path, user_db, db, authed, w_id, true).await
}

lazy_static::lazy_static! {
    static ref DEBUG_RAW_SCRIPT_ENDPOINTS: bool =
        std::env::var("DEBUG_RAW_SCRIPT_ENDPOINTS").is_ok();
}

async fn raw_script_by_path_internal(
    path: StripPath,
    user_db: UserDB,
    db: DB,
    authed: ApiAuthed,
    w_id: String,
    unpin: bool,
) -> Result<String> {
    let path = path.to_path();
    check_scopes(&authed, || format!("scripts:read:{}", path))?;
    if !path.ends_with(".py")
        && !path.ends_with(".ts")
        && !path.ends_with(".go")
        && !path.ends_with(".sh")
    {
        return Err(Error::BadRequest(format!(
            "Path must ends with a .py, .ts, .go. or .sh extension: {}",
            path
        )));
    }
    let path = path
        .trim_end_matches(".py")
        .trim_end_matches(".bun.ts")
        .trim_end_matches(".deno.ts")
        .trim_end_matches(".ts")
        .trim_end_matches(".go")
        .trim_end_matches(".sh");
    let mut tx = user_db.begin(&authed).await?;

    let content_o = sqlx::query_scalar!(
        "SELECT content FROM script WHERE path = $1 AND workspace_id = $2 AND archived = false ORDER BY created_at DESC LIMIT 1",
        path,
        w_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;

    if content_o.is_none() {
        let exists = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM script WHERE path = $1 AND workspace_id = $2 AND archived = false ORDER BY created_at DESC LIMIT 1)",
            path,
            w_id
        )
        .fetch_one(&db)
        .await?
        .unwrap_or(false);

        if exists {
            return Err(Error::NotFound(format!(
                "Script {path} exists but {} does not have permissions to access it",
                authed.username
            )));
        } else {
            if *DEBUG_RAW_SCRIPT_ENDPOINTS {
                let other_script_o = sqlx::query_scalar!(
                    "SELECT path FROM script WHERE workspace_id = $1 AND archived = false",
                    w_id
                )
                .fetch_all(&db)
                .await?;
                let other_script_archived = sqlx::query_scalar!(
                    "SELECT distinct(path) FROM script WHERE workspace_id = $1 AND archived = true",
                    w_id
                )
                .fetch_all(&db)
                .await?;
                tracing::warn!(
                    "Script {path} does not exist in workspace {w_id} but these paths do, non-archived: {:?} | archived: {:?}",
                    other_script_o.join(", "),
                    other_script_archived.join(", ")
                )
            }
        }
    }

    let content = not_found_if_none(content_o, "Script", path)?;

    if unpin {
        return Ok(remove_pinned_imports(&content)?);
    } else {
        return Ok(content);
    }
}

async fn exists_script_by_path(
    Extension(db): Extension<DB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> JsonResult<bool> {
    let path = path.to_path();

    let exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM script WHERE path = $1 AND workspace_id = $2 ORDER BY created_at DESC LIMIT 1)",
        path,
        w_id
    )
    .fetch_one(&db)
    .await?
    .unwrap_or(false);

    Ok(Json(exists))
}

async fn get_script_by_hash_internal<'c>(
    db: &mut Transaction<'c, Postgres>,
    workspace_id: &str,
    hash: &ScriptHash,
    with_starred_info_for_username: Option<&str>,
) -> Result<ScriptWithStarred> {
    let script_o = if let Some(username) = with_starred_info_for_username {
        sqlx::query_as::<_, ScriptWithStarred>(
            "SELECT s.*, favorite.path IS NOT NULL as starred
            FROM script s
            LEFT JOIN favorite 
            ON favorite.favorite_kind = 'script' 
                AND favorite.workspace_id = s.workspace_id 
                AND favorite.path = s.path 
                AND favorite.usr = $1 
            WHERE s.hash = $2 AND s.workspace_id = $3",
        )
        .bind(&username)
        .bind(hash)
        .bind(workspace_id)
        .fetch_optional(&mut **db)
        .await?
    } else {
        sqlx::query_as::<_, ScriptWithStarred>(
            "SELECT *, NULL as starred FROM script WHERE hash = $1 AND workspace_id = $2",
        )
        .bind(hash)
        .bind(workspace_id)
        .fetch_optional(&mut **db)
        .await?
    };

    let script = not_found_if_none(script_o, "Script", hash.to_string())?;
    Ok(script)
}

#[derive(Deserialize)]
struct GetScriptByHashQuery {
    authed: Option<bool>,
}
async fn get_script_by_hash(
    Extension(db): Extension<DB>,
    Extension(user_db): Extension<UserDB>,
    Path((w_id, hash)): Path<(String, ScriptHash)>,
    Query(query): Query<WithStarredInfoQuery>,
    Query(query_auth): Query<GetScriptByHashQuery>,
    Extension(authed): Extension<ApiAuthed>,
) -> JsonResult<ScriptWithStarred> {
    let mut tx = if query_auth.authed.is_some_and(|x| x) {
        user_db.begin(&authed).await?
    } else {
        db.begin().await?
    };
    let r = get_script_by_hash_internal(
        &mut tx,
        &w_id,
        &hash,
        query.with_starred_info.and_then(|x| {
            if x {
                Some(authed.username.as_str())
            } else {
                None
            }
        }),
    )
    .await?;

    check_scopes(&authed, || format!("scripts:read:{}", &r.script.path))?;

    tx.commit().await?;

    Ok(Json(r))
}

async fn raw_script_by_hash(
    Extension(db): Extension<DB>,
    Path((w_id, hash_str)): Path<(String, String)>,
) -> Result<String> {
    let mut tx = db.begin().await?;
    let hash = ScriptHash(to_i64(hash_str.strip_suffix(".ts").ok_or_else(|| {
        Error::BadRequest("Raw script path must end with .ts".to_string())
    })?)?);
    let r = get_script_by_hash_internal(&mut tx, &w_id, &hash, None).await?;
    tx.commit().await?;

    Ok(r.script.content)
}

#[derive(FromRow, Serialize)]
struct DeploymentStatus {
    lock: Option<String>,
    lock_error_logs: Option<String>,
}
async fn get_deployment_status(
    Extension(db): Extension<DB>,
    Path((w_id, hash)): Path<(String, ScriptHash)>,
) -> JsonResult<DeploymentStatus> {
    let mut tx = db.begin().await?;
    let status_o: Option<DeploymentStatus> = sqlx::query_as!(
        DeploymentStatus,
        "SELECT lock, lock_error_logs FROM script WHERE hash = $1 AND workspace_id = $2",
        hash.0,
        w_id,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let status = not_found_if_none(status_o, "DeploymentStatus", hash.to_string())?;

    tx.commit().await?;
    Ok(Json(status))
}

pub async fn require_is_writer(authed: &ApiAuthed, path: &str, w_id: &str, db: DB) -> Result<()> {
    return crate::users::require_is_writer(
        authed,
        path,
        w_id,
        db,
        "SELECT extra_perms FROM script WHERE path = $1 AND workspace_id = $2 ORDER BY created_at DESC LIMIT 1",
        "script",
    )
    .await;
}

async fn archive_script_by_path(
    authed: ApiAuthed,
    Extension(webhook): Extension<WebhookShared>,
    Extension(user_db): Extension<UserDB>,
    Extension(db): Extension<DB>,
    Path((w_id, path)): Path<(String, StripPath)>,
) -> Result<()> {
    let path = path.to_path();
    check_scopes(&authed, || format!("scripts:write:{}", path))?;
    let mut tx = user_db.begin(&authed).await?;

    require_owner_of_path(&authed, path)?;

    let hash: i64 = sqlx::query_scalar!(
        "UPDATE script SET archived = true WHERE path = $1 AND workspace_id = $2 RETURNING hash",
        path,
        &w_id
    )
    .fetch_one(&db)
    .await
    .map_err(|e| Error::internal_err(format!("archiving script in {w_id}: {e:#}")))?;

    sqlx::query!(
        "DELETE FROM asset WHERE workspace_id = $1 AND usage_kind = 'script' AND usage_path = $2",
        &w_id,
        path
    )
    .execute(&mut *tx)
    .await?;

    audit_log(
        &mut *tx,
        &authed,
        "scripts.archive",
        ActionKind::Delete,
        &w_id,
        Some(&ScriptHash(hash).to_string()),
        Some([("workspace", w_id.as_str())].into()),
    )
    .await?;
    tx.commit().await?;

    handle_deployment_metadata(
        &authed.email,
        &authed.username,
        &db,
        &w_id,
        DeployedObject::Script {
            hash: ScriptHash(0), // dummy hash as it will not get inserted in db
            path: path.to_string(),
            parent_path: Some(path.to_string()),
        },
        Some(format!("Script '{}' archived", path)),
        true,
    )
    .await?;

    webhook.send_message(
        w_id.clone(),
        WebhookMessage::DeleteScript { workspace: w_id, hash: hash.to_string() },
    );

    Ok(())
}

async fn archive_script_by_hash(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(webhook): Extension<WebhookShared>,
    Path((w_id, hash)): Path<(String, ScriptHash)>,
) -> JsonResult<Script> {
    let mut tx = user_db.begin(&authed).await?;

    let script = sqlx::query_as::<_, Script>(
        "UPDATE script SET archived = true WHERE hash = $1 RETURNING *",
    )
    .bind(&hash.0)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| Error::internal_err(format!("archiving script in {w_id}: {e:#}")))?;

    check_scopes(&authed, || format!("scripts:write:{}", &script.path))?;
    sqlx::query!(
        "DELETE FROM asset WHERE workspace_id = $1 AND usage_kind = 'script' AND usage_path = (SELECT path FROM script WHERE hash = $2 AND workspace_id = $1)",
        &w_id,
        &hash.0
    )
    .execute(&mut *tx)
    .await?;

    audit_log(
        &mut *tx,
        &authed,
        "scripts.archive",
        ActionKind::Delete,
        &w_id,
        Some(&hash.to_string()),
        Some([("workspace", w_id.as_str())].into()),
    )
    .await?;
    tx.commit().await?;

    webhook.send_message(
        w_id.clone(),
        WebhookMessage::DeleteScript { workspace: w_id, hash: hash.to_string() },
    );

    Ok(Json(script))
}

async fn delete_script_by_hash(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(webhook): Extension<WebhookShared>,
    Extension(db): Extension<DB>,
    Path((w_id, hash)): Path<(String, ScriptHash)>,
) -> JsonResult<Script> {
    let mut tx = user_db.begin(&authed).await?;

    require_admin(authed.is_admin, &authed.username)?;
    let script = sqlx::query_as::<_, Script>(
        "UPDATE script SET content = '', archived = true, deleted = true, lock = '', schema = null WHERE hash = $1 AND \
         workspace_id = $2 RETURNING *",
    )
    .bind(&hash.0)
    .bind(&w_id)
    .fetch_one(&db)
    .await
    .map_err(|e| Error::internal_err(format!("deleting script by hash {w_id}: {e:#}")))?;

    check_scopes(&authed, || format!("scripts:write:{}", &script.path))?;
    sqlx::query!(
        "DELETE FROM asset WHERE workspace_id = $1 AND usage_kind = 'script' AND usage_path = (SELECT path FROM script WHERE hash = $2 AND workspace_id = $1)",
        &w_id,
        hash.0
    )
    .execute(&mut *tx)
    .await?;

    audit_log(
        &mut *tx,
        &authed,
        "scripts.delete",
        ActionKind::Delete,
        &w_id,
        Some(&hash.to_string()),
        Some([("workspace", w_id.as_str())].into()),
    )
    .await?;
    tx.commit().await?;

    webhook.send_message(
        w_id.clone(),
        WebhookMessage::DeleteScript { workspace: w_id, hash: hash.to_string() },
    );

    Ok(Json(script))
}

#[derive(Deserialize)]
struct DeleteScriptQuery {
    keep_captures: Option<bool>,
}

async fn delete_script_by_path(
    authed: ApiAuthed,
    Extension(user_db): Extension<UserDB>,
    Extension(webhook): Extension<WebhookShared>,
    Extension(db): Extension<DB>,
    Path((w_id, path)): Path<(String, StripPath)>,
    Query(query): Query<DeleteScriptQuery>,
) -> JsonResult<String> {
    let path = path.to_path();
    check_scopes(&authed, || format!("scripts:write:{}", path))?;

    if path == "u/admin/hub_sync" && w_id == "admins" {
        return Err(Error::BadRequest(
            "Cannot delete the global setup app".to_string(),
        ));
    }
    let mut tx = user_db.begin(&authed).await?;

    let draft_only = sqlx::query_scalar!(
        "SELECT draft_only FROM script WHERE path = $1 AND workspace_id = $2",
        path,
        w_id
    )
    .fetch_one(&db)
    .await?
    .unwrap_or(false);

    if !draft_only {
        require_admin(authed.is_admin, &authed.username)?;
    }

    let script = if !draft_only {
        require_admin(authed.is_admin, &authed.username)?;
        sqlx::query_scalar!(
            "DELETE FROM script WHERE path = $1 AND workspace_id = $2 RETURNING path",
            path,
            w_id
        )
        .fetch_one(&db)
        .await
        .map_err(|e| Error::internal_err(format!("deleting script by path {w_id}: {e:#}")))?
    } else {
        // If the script is draft only, we can delete it without admin permissions but we still need write permissions
        sqlx::query_scalar!(
            "DELETE FROM script WHERE path = $1 AND workspace_id = $2 RETURNING path",
            path,
            w_id
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| Error::internal_err(format!("deleting script by path {w_id}: {e:#}")))?
    };

    sqlx::query!(
        "DELETE FROM draft WHERE path = $1 AND workspace_id = $2 AND typ = 'script'",
        path,
        w_id
    )
    .execute(&db)
    .await?;

    if !query.keep_captures.unwrap_or(false) {
        sqlx::query!(
            "DELETE FROM capture_config WHERE path = $1 AND workspace_id = $2 AND is_flow IS FALSE",
            path,
            w_id
        )
        .execute(&db)
        .await?;

        sqlx::query!(
            "DELETE FROM capture WHERE path = $1 AND workspace_id = $2 AND is_flow IS FALSE",
            path,
            w_id
        )
        .execute(&db)
        .await?;
    }

    audit_log(
        &mut *tx,
        &authed,
        "scripts.delete",
        ActionKind::Delete,
        &w_id,
        Some(&path),
        Some([("workspace", w_id.as_str())].into()),
    )
    .await?;
    tx.commit().await?;

    handle_deployment_metadata(
        &authed.email,
        &authed.username,
        &db,
        &w_id,
        DeployedObject::Script {
            hash: ScriptHash(0), // Temporary value as it will get removed right after
            path: path.to_string(),
            parent_path: Some(path.to_string()),
        },
        Some(format!("Script '{}' deleted", path)),
        true,
    )
    .await?;

    sqlx::query!(
        "DELETE FROM deployment_metadata WHERE path = $1 AND workspace_id = $2 AND script_hash IS NOT NULL",
        path,
        w_id
    )
    .execute(&db)
    .await
    .map_err(|e| {
        Error::internal_err(format!(
            "error deleting deployment metadata for script with path {path} in workspace {w_id}: {e:#}"
        ))
    })?;

    webhook.send_message(
        w_id.clone(),
        WebhookMessage::DeleteScriptPath { workspace: w_id, path: path.to_string() },
    );

    Ok(Json(script))
}
