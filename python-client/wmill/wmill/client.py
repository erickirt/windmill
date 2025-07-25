from __future__ import annotations

import atexit
import datetime as dt
import functools
from io import BufferedReader, BytesIO
import logging
import os
import random
import time
import warnings
import json
from json import JSONDecodeError
from typing import Dict, Any, Union, Literal, Optional
import re

import httpx

from .s3_reader import S3BufferedReader, bytes_generator
from .s3_types import (
    Boto3ConnectionSettings,
    DuckDbConnectionSettings,
    PolarsConnectionSettings,
    S3Object,
)

_client: "Windmill | None" = None

logger = logging.getLogger("windmill_client")

JobStatus = Literal["RUNNING", "WAITING", "COMPLETED"]


class Windmill:
    def __init__(self, base_url=None, token=None, workspace=None, verify=True):
        base = (
            base_url
            or os.environ.get("BASE_INTERNAL_URL")
            or os.environ.get("WM_BASE_URL")
        )

        self.base_url = f"{base}/api"
        self.token = token or os.environ.get("WM_TOKEN")
        self.headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self.token}",
        }
        self.verify = verify
        self.client = self.get_client()
        self.workspace = workspace or os.environ.get("WM_WORKSPACE")
        self.path = os.environ.get("WM_JOB_PATH")

        self.mocked_api = self.get_mocked_api()

        assert self.workspace, (
            f"workspace required as an argument or as WM_WORKSPACE environment variable"
        )

    def get_mocked_api(self) -> Optional[dict]:
        mocked_path = os.environ.get("WM_MOCKED_API_FILE")
        if not mocked_path:
            return None
        logger.info("Using mocked API from %s", mocked_path)
        mocked_api = {"variables": {}, "resources": {}}
        try:
            with open(mocked_path, "r") as f:
                incoming_mocked_api = json.load(f)
            mocked_api = {**mocked_api, **incoming_mocked_api}
        except Exception as e:
            logger.warning(
                "Error parsing mocked API file at path %s Using empty mocked API.",
                mocked_path,
            )
            logger.debug(e)
        return mocked_api

    def get_client(self) -> httpx.Client:
        return httpx.Client(
            base_url=self.base_url,
            headers=self.headers,
            verify=self.verify,
        )

    def get(self, endpoint, raise_for_status=True, **kwargs) -> httpx.Response:
        endpoint = endpoint.lstrip("/")
        resp = self.client.get(f"/{endpoint}", **kwargs)
        if raise_for_status:
            try:
                resp.raise_for_status()
            except httpx.HTTPStatusError as err:
                error = f"{err.request.url}: {err.response.status_code}, {err.response.text}"
                logger.error(error)
                raise Exception(error)
        return resp

    def post(self, endpoint, raise_for_status=True, **kwargs) -> httpx.Response:
        endpoint = endpoint.lstrip("/")
        resp = self.client.post(f"/{endpoint}", **kwargs)
        if raise_for_status:
            try:
                resp.raise_for_status()
            except httpx.HTTPStatusError as err:
                error = f"{err.request.url}: {err.response.status_code}, {err.response.text}"
                logger.error(error)
                raise Exception(error)
        return resp

    def create_token(self, duration=dt.timedelta(days=1)) -> str:
        endpoint = "/users/tokens/create"
        payload = {
            "label": f"refresh {time.time()}",
            "expiration": (dt.datetime.now() + duration).strftime("%Y-%m-%dT%H:%M:%SZ"),
        }
        return self.post(endpoint, json=payload).text

    def run_script_async(
        self,
        path: str = None,
        hash_: str = None,
        args: dict = None,
        scheduled_in_secs: int = None,
    ) -> str:
        """Create a script job and return its job id.
        
        .. deprecated:: Use run_script_by_path_async or run_script_by_hash_async instead.
        """
        logging.warning(
            "run_script_async is deprecated. Use run_script_by_path_async or run_script_by_hash_async instead.",
        )
        assert not (path and hash_), "path and hash_ are mutually exclusive"
        return self._run_script_async_internal(path=path, hash_=hash_, args=args, scheduled_in_secs=scheduled_in_secs)

    def _run_script_async_internal(
        self,
        path: str = None,
        hash_: str = None,
        args: dict = None,
        scheduled_in_secs: int = None,
    ) -> str:
        """Internal helper for running scripts asynchronously."""
        args = args or {}
        params = {"scheduled_in_secs": scheduled_in_secs} if scheduled_in_secs else {}
        if os.environ.get("WM_JOB_ID"):
            params["parent_job"] = os.environ.get("WM_JOB_ID")
        if os.environ.get("WM_ROOT_FLOW_JOB_ID"):
            params["root_job"] = os.environ.get("WM_ROOT_FLOW_JOB_ID")
        
        if path:
            endpoint = f"/w/{self.workspace}/jobs/run/p/{path}"
        elif hash_:
            endpoint = f"/w/{self.workspace}/jobs/run/h/{hash_}"
        else:
            raise Exception("path or hash_ must be provided")
        
        return self.post(endpoint, json=args, params=params).text

    def run_script_by_path_async(
        self,
        path: str,
        args: dict = None,
        scheduled_in_secs: int = None,
    ) -> str:
        """Create a script job by path and return its job id."""
        return self._run_script_async_internal(path=path, args=args, scheduled_in_secs=scheduled_in_secs)

    def run_script_by_hash_async(
        self,
        hash_: str,
        args: dict = None,
        scheduled_in_secs: int = None,
    ) -> str:
        """Create a script job by hash and return its job id."""
        return self._run_script_async_internal(hash_=hash_, args=args, scheduled_in_secs=scheduled_in_secs)

    def run_flow_async(
        self,
        path: str,
        args: dict = None,
        scheduled_in_secs: int = None,
        # can only be set to false if this the job will be fully await and not concurrent with any other job
        # as otherwise the child flow and its own child will store their state in the parent job which will
        # lead to incorrectness and failures
        do_not_track_in_parent: bool = True,
    ) -> str:
        """Create a flow job and return its job id."""
        args = args or {}
        params = {"scheduled_in_secs": scheduled_in_secs} if scheduled_in_secs else {}
        if not do_not_track_in_parent:
            if os.environ.get("WM_JOB_ID"):
                params["parent_job"] = os.environ.get("WM_JOB_ID")
            if os.environ.get("WM_ROOT_FLOW_JOB_ID"):
                params["root_job"] = os.environ.get("WM_ROOT_FLOW_JOB_ID")
        if path:
            endpoint = f"/w/{self.workspace}/jobs/run/f/{path}"
        else:
            raise Exception("path must be provided")
        return self.post(endpoint, json=args, params=params).text

    def run_script(
        self,
        path: str = None,
        hash_: str = None,
        args: dict = None,
        timeout: dt.timedelta | int | float | None = None,
        verbose: bool = False,
        cleanup: bool = True,
        assert_result_is_not_none: bool = False,
    ) -> Any:
        """Run script synchronously and return its result.
        
        .. deprecated:: Use run_script_by_path or run_script_by_hash instead.
        """
        logging.warning(
            "run_script is deprecated. Use run_script_by_path or run_script_by_hash instead.",
        )
        assert not (path and hash_), "path and hash_ are mutually exclusive"
        return self._run_script_internal(
            path=path, hash_=hash_, args=args, timeout=timeout, verbose=verbose,
            cleanup=cleanup, assert_result_is_not_none=assert_result_is_not_none
        )

    def _run_script_internal(
        self,
        path: str = None,
        hash_: str = None,
        args: dict = None,
        timeout: dt.timedelta | int | float | None = None,
        verbose: bool = False,
        cleanup: bool = True,
        assert_result_is_not_none: bool = False,
    ) -> Any:
        """Internal helper for running scripts synchronously."""
        args = args or {}

        if verbose:
            if path:
                logger.info(f"running `{path}` synchronously with {args = }")
            elif hash_:
                logger.info(f"running script with hash `{hash_}` synchronously with {args = }")

        if isinstance(timeout, dt.timedelta):
            timeout = timeout.total_seconds()

        job_id = self._run_script_async_internal(path=path, hash_=hash_, args=args)
        return self.wait_job(
            job_id, timeout, verbose, cleanup, assert_result_is_not_none
        )

    def run_script_by_path(
        self,
        path: str,
        args: dict = None,
        timeout: dt.timedelta | int | float | None = None,
        verbose: bool = False,
        cleanup: bool = True,
        assert_result_is_not_none: bool = False,
    ) -> Any:
        """Run script by path synchronously and return its result."""
        return self._run_script_internal(
            path=path, args=args, timeout=timeout, verbose=verbose,
            cleanup=cleanup, assert_result_is_not_none=assert_result_is_not_none
        )

    def run_script_by_hash(
        self,
        hash_: str,
        args: dict = None,
        timeout: dt.timedelta | int | float | None = None,
        verbose: bool = False,
        cleanup: bool = True,
        assert_result_is_not_none: bool = False,
    ) -> Any:
        """Run script by hash synchronously and return its result."""
        return self._run_script_internal(
            hash_=hash_, args=args, timeout=timeout, verbose=verbose,
            cleanup=cleanup, assert_result_is_not_none=assert_result_is_not_none
        )

    def wait_job(
        self,
        job_id,
        timeout: dt.timedelta | int | float | None = None,
        verbose: bool = False,
        cleanup: bool = True,
        assert_result_is_not_none: bool = False,
    ):
        def cancel_job():
            logger.warning(f"cancelling job: {job_id}")
            self.post(
                f"/w/{self.workspace}/jobs_u/queue/cancel/{job_id}",
                json={"reason": "parent script cancelled"},
            ).raise_for_status()

        if cleanup:
            atexit.register(cancel_job)

        start_time = time.time()

        if isinstance(timeout, dt.timedelta):
            timeout = timeout.total_seconds()

        while True:
            result_res = self.get(
                f"/w/{self.workspace}/jobs_u/completed/get_result_maybe/{job_id}", True
            ).json()

            started = result_res["started"]
            completed = result_res["completed"]
            success = result_res["success"]

            if not started and verbose:
                logger.info(f"job {job_id} has not started yet")

            if cleanup and completed:
                atexit.unregister(cancel_job)

            if completed:
                result = result_res["result"]
                if success:
                    if result is None and assert_result_is_not_none:
                        raise Exception("Result was none")
                    return result
                else:
                    error = result["error"]
                    raise Exception(f"Job {job_id} was not successful: {str(error)}")

            if timeout and ((time.time() - start_time) > timeout):
                msg = "reached timeout"
                logger.warning(msg)
                self.post(
                    f"/w/{self.workspace}/jobs_u/queue/cancel/{job_id}",
                    json={"reason": msg},
                )
                raise TimeoutError(msg)
            if verbose:
                logger.info(f"sleeping 0.5 seconds for {job_id = }")

            time.sleep(0.5)

    def cancel_running(self) -> dict:
        """Cancel currently running executions of the same script."""
        logger.info("canceling running executions of this script")

        jobs = self.get(
            f"/w/{self.workspace}/jobs/list",
            params={
                "running": "true",
                "script_path_exact": self.path,
            },
        ).json()

        current_job_id = os.environ.get("WM_JOB_ID")

        logger.debug(f"{current_job_id = }")

        job_ids = [j["id"] for j in jobs if j["id"] != current_job_id]

        if job_ids:
            logger.info(f"cancelling the following job ids: {job_ids}")
        else:
            logger.info("no previous executions to cancel")

        result = {}

        for id_ in job_ids:
            result[id_] = self.post(
                f"/w/{self.workspace}/jobs_u/queue/cancel/{id_}",
                json={"reason": "killed by `cancel_running` method"},
            )

        return result

    def get_job(self, job_id: str) -> dict:
        return self.get(f"/w/{self.workspace}/jobs_u/get/{job_id}").json()

    def get_root_job_id(self, job_id: str | None = None) -> dict:
        job_id = job_id or os.environ.get("WM_JOB_ID")
        return self.get(f"/w/{self.workspace}/jobs_u/get_root_job_id/{job_id}").json()

    def get_id_token(self, audience: str) -> str:
        return self.post(f"/w/{self.workspace}/oidc/token/{audience}").text

    def get_job_status(self, job_id: str) -> JobStatus:
        job = self.get_job(job_id)
        job_type = job.get("type", "")
        assert job_type, f"{job} is not a valid job"
        if job_type.lower() == "completedjob":
            return "COMPLETED"
        if job.get("running"):
            return "RUNNING"
        return "WAITING"

    def get_result(
        self,
        job_id: str,
        assert_result_is_not_none: bool = True,
    ) -> Any:
        result = self.get(f"/w/{self.workspace}/jobs_u/completed/get_result/{job_id}")
        result_text = result.text
        if assert_result_is_not_none and result_text is None:
            raise Exception(f"result is None for {job_id = }")
        try:
            return result.json()
        except JSONDecodeError:
            return result_text

    def get_variable(self, path: str) -> str:
        path = parse_variable_syntax(path) or path
        if self.mocked_api is not None:
            variables = self.mocked_api["variables"]
            try:
                result = variables[path]
                return result
            except KeyError:
                logger.info(
                    f"MockedAPI present, but variable not found at {path}, falling back to real API"
                )

        """Get variable from Windmill"""
        return self.get(f"/w/{self.workspace}/variables/get_value/{path}").json()

    def set_variable(self, path: str, value: str, is_secret: bool = False) -> None:
        path = parse_variable_syntax(path) or path
        if self.mocked_api is not None:
            self.mocked_api["variables"][path] = value
            return

        """Set variable from Windmill"""
        # check if variable exists
        r = self.get(
            f"/w/{self.workspace}/variables/get/{path}", raise_for_status=False
        )
        if r.status_code == 404:
            # create variable
            self.post(
                f"/w/{self.workspace}/variables/create",
                json={
                    "path": path,
                    "value": value,
                    "is_secret": is_secret,
                    "description": "",
                },
            )
        else:
            # update variable
            self.post(
                f"/w/{self.workspace}/variables/update/{path}",
                json={"value": value},
            )

    def get_resource(
        self,
        path: str,
        none_if_undefined: bool = False,
    ) -> dict | None:
        path = parse_resource_syntax(path) or path
        if self.mocked_api is not None:
            resources = self.mocked_api["resources"]
            try:
                result = resources[path]
                return result
            except KeyError:
                # NOTE: should mocked_api respect `none_if_undefined`?
                if none_if_undefined:
                    logger.info(
                        f"resource not found at ${path}, but none_if_undefined is True, so returning None"
                    )
                    return None
                logger.info(
                    f"MockedAPI present, but resource not found at ${path}, falling back to real API"
                )

        """Get resource from Windmill"""
        try:
            return self.get(
                f"/w/{self.workspace}/resources/get_value_interpolated/{path}"
            ).json()
        except Exception as e:
            if none_if_undefined:
                return None
            logger.error(e)
            raise e

    def set_resource(
        self,
        value: Any,
        path: str,
        resource_type: str,
    ):
        path = parse_resource_syntax(path) or path
        if self.mocked_api is not None:
            self.mocked_api["resources"][path] = value
            return

        # check if resource exists
        r = self.get(
            f"/w/{self.workspace}/resources/get/{path}", raise_for_status=False
        )
        if r.status_code == 404:
            # create resource
            self.post(
                f"/w/{self.workspace}/resources/create",
                json={
                    "path": path,
                    "value": value,
                    "resource_type": resource_type,
                },
            )
        else:
            # update resource
            self.post(
                f"/w/{self.workspace}/resources/update_value/{path}",
                json={"value": value},
            )

    def set_state(self, value: Any):
        self.set_resource(value, path=self.state_path, resource_type="state")

    def set_progress(self, value: int, job_id: Optional[str] = None):
        workspace = get_workspace()
        flow_id = os.environ.get("WM_FLOW_JOB_ID")
        job_id = job_id or os.environ.get("WM_JOB_ID")

        if job_id != None:
            job = self.get_job(job_id)
            flow_id = job.get("parent_job")

        self.post(
            f"/w/{workspace}/job_metrics/set_progress/{job_id}",
            json={
                "percent": value,
                "flow_job_id": flow_id or None,
            },
        )

    def get_progress(self, job_id: Optional[str] = None) -> Any:
        workspace = get_workspace()
        job_id = job_id or os.environ.get("WM_JOB_ID")

        r = self.get(
            f"/w/{workspace}/job_metrics/get_progress/{job_id}",
        )
        if r.status_code == 404:
            print(f"Job {job_id} does not exist")
            return None
        else:
            return r.json()

    def set_flow_user_state(self, key: str, value: Any) -> None:
        """Set the user state of a flow at a given key"""
        flow_id = self.get_root_job_id()
        r = self.post(
            f"/w/{self.workspace}/jobs/flow/user_states/{flow_id}/{key}",
            json=value,
            raise_for_status=False,
        )
        if r.status_code == 404:
            print(f"Job {flow_id} does not exist or is not a flow")

    def get_flow_user_state(self, key: str) -> Any:
        """Get the user state of a flow at a given key"""
        flow_id = self.get_root_job_id()
        r = self.get(
            f"/w/{self.workspace}/jobs/flow/user_states/{flow_id}/{key}",
            raise_for_status=False,
        )
        if r.status_code == 404:
            print(f"Job {flow_id} does not exist or is not a flow")
            return None
        else:
            return r.json()

    @property
    def version(self):
        return self.get("version").text

    def get_duckdb_connection_settings(
        self,
        s3_resource_path: str = "",
    ) -> DuckDbConnectionSettings | None:
        """
        Convenient helpers that takes an S3 resource as input and returns the settings necessary to
        initiate an S3 connection from DuckDB
        """
        s3_resource_path = parse_resource_syntax(s3_resource_path) or s3_resource_path
        try:
            raw_obj = self.post(
                f"/w/{self.workspace}/job_helpers/v2/duckdb_connection_settings",
                json={}
                if s3_resource_path == ""
                else {"s3_resource_path": s3_resource_path},
            ).json()
            return DuckDbConnectionSettings(raw_obj)
        except JSONDecodeError as e:
            raise Exception(
                "Could not generate DuckDB S3 connection settings from the provided resource"
            ) from e

    def get_polars_connection_settings(
        self,
        s3_resource_path: str = "",
    ) -> PolarsConnectionSettings:
        """
        Convenient helpers that takes an S3 resource as input and returns the settings necessary to
        initiate an S3 connection from Polars
        """
        s3_resource_path = parse_resource_syntax(s3_resource_path) or s3_resource_path
        try:
            raw_obj = self.post(
                f"/w/{self.workspace}/job_helpers/v2/polars_connection_settings",
                json={}
                if s3_resource_path == ""
                else {"s3_resource_path": s3_resource_path},
            ).json()
            return PolarsConnectionSettings(raw_obj)
        except JSONDecodeError as e:
            raise Exception(
                "Could not generate Polars S3 connection settings from the provided resource"
            ) from e

    def get_boto3_connection_settings(
        self,
        s3_resource_path: str = "",
    ) -> Boto3ConnectionSettings:
        """
        Convenient helpers that takes an S3 resource as input and returns the settings necessary to
        initiate an S3 connection using boto3
        """
        s3_resource_path = parse_resource_syntax(s3_resource_path) or s3_resource_path
        try:
            s3_resource = self.post(
                f"/w/{self.workspace}/job_helpers/v2/s3_resource_info",
                json={}
                if s3_resource_path == ""
                else {"s3_resource_path": s3_resource_path},
            ).json()
            return self.__boto3_connection_settings(s3_resource)
        except JSONDecodeError as e:
            raise Exception(
                "Could not generate Boto3 S3 connection settings from the provided resource"
            ) from e

    def load_s3_file(self, s3object: S3Object | str, s3_resource_path: str | None) -> bytes:
        """
        Load a file from the workspace s3 bucket and returns its content as bytes.

        '''python
        from wmill import S3Object

        s3_obj = S3Object(s3="/path/to/my_file.txt")
        my_obj_content = client.load_s3_file(s3_obj)
        file_content = my_obj_content.decode("utf-8")
        '''
        """
        s3object = parse_s3_object(s3object)
        with self.load_s3_file_reader(s3object, s3_resource_path) as file_reader:
            return file_reader.read()

    def load_s3_file_reader(
        self, s3object: S3Object | str, s3_resource_path: str | None
    ) -> BufferedReader:
        """
        Load a file from the workspace s3 bucket and returns the bytes stream.

        '''python
        from wmill import S3Object

        s3_obj = S3Object(s3="/path/to/my_file.txt")
        with wmill.load_s3_file(s3object, s3_resource_path) as file_reader:
            print(file_reader.read())
        '''
        """
        s3object = parse_s3_object(s3object)
        reader = S3BufferedReader(
            f"{self.workspace}",
            self.client,
            s3object["s3"],
            s3_resource_path,
            s3object["storage"] if "storage" in s3object else None,
        )
        return reader

    def write_s3_file(
        self,
        s3object: S3Object | str | None,
        file_content: BufferedReader | bytes,
        s3_resource_path: str | None,
        content_type: str | None = None,
        content_disposition: str | None = None,
    ) -> S3Object:
        """
        Write a file to the workspace S3 bucket

        '''python
        from wmill import S3Object

        s3_obj = S3Object(s3="/path/to/my_file.txt")

        # for an in memory bytes array:
        file_content = b'Hello Windmill!'
        client.write_s3_file(s3_obj, file_content)

        # for a file:
        with open("my_file.txt", "rb") as my_file:
            client.write_s3_file(s3_obj, my_file)
        '''
        """
        s3object = parse_s3_object(s3object)
        # httpx accepts either bytes or "a bytes generator" as content. If it's a BufferedReader, we need to convert it to a generator
        if isinstance(file_content, BufferedReader):
            content_payload = bytes_generator(file_content)
        elif isinstance(file_content, bytes):
            content_payload = file_content
        else:
            raise Exception("Type of file_content not supported")

        query_params = {}
        if s3object is not None and s3object["s3"] != "":
            query_params["file_key"] = s3object["s3"]
        if s3_resource_path is not None and s3_resource_path != "":
            query_params["s3_resource_path"] = s3_resource_path
        if (
            s3object is not None
            and "storage" in s3object
            and s3object["storage"] is not None
        ):
            query_params["storage"] = s3object["storage"]
        if content_type is not None:
            query_params["content_type"] = content_type
        if content_disposition is not None:
            query_params["content_disposition"] = content_disposition

        try:
            # need a vanilla client b/c content-type is not application/json here
            response = httpx.post(
                f"{self.base_url}/w/{self.workspace}/job_helpers/upload_s3_file",
                headers={
                    "Authorization": f"Bearer {self.token}",
                    "Content-Type": "application/octet-stream",
                },
                params=query_params,
                content=content_payload,
                verify=self.verify,
                timeout=None,
            ).json()
        except Exception as e:
            raise Exception("Could not write file to S3") from e
        return S3Object(s3=response["file_key"])

    def sign_s3_objects(self, s3_objects: list[S3Object | str]) -> list[S3Object]:
        return self.post(
            f"/w/{self.workspace}/apps/sign_s3_objects", json={"s3_objects": list(map(parse_s3_object, s3_objects))}
        ).json()

    def sign_s3_object(self, s3_object: S3Object | str) -> S3Object:
        return self.post(
            f"/w/{self.workspace}/apps/sign_s3_objects",
            json={"s3_objects": [s3_object]},
        ).json()[0]

    def __boto3_connection_settings(self, s3_resource) -> Boto3ConnectionSettings:
        endpoint_url_prefix = "https://" if s3_resource["useSSL"] else "http://"
        return Boto3ConnectionSettings(
            {
                "endpoint_url": "{}{}".format(
                    endpoint_url_prefix, s3_resource["endPoint"]
                ),
                "region_name": s3_resource["region"],
                "use_ssl": s3_resource["useSSL"],
                "aws_access_key_id": s3_resource["accessKey"],
                "aws_secret_access_key": s3_resource["secretKey"],
                # no need for path_style here as boto3 is clever enough to determine which one to use
            }
        )

    def whoami(self) -> dict:
        return self.get("/users/whoami").json()

    @property
    def user(self) -> dict:
        return self.whoami()

    @property
    def state_path(self) -> str:
        state_path = os.environ.get(
            "WM_STATE_PATH_NEW", os.environ.get("WM_STATE_PATH")
        )
        if state_path is None:
            raise Exception("State path not found")
        return state_path

    @property
    def state(self) -> Any:
        return self.get_resource(path=self.state_path, none_if_undefined=True)

    @state.setter
    def state(self, value: Any) -> None:
        self.set_state(value)

    @staticmethod
    def set_shared_state_pickle(value: Any, path: str = "state.pickle") -> None:
        """
        Set the state in the shared folder using pickle
        """
        import pickle

        with open(f"/shared/{path}", "wb") as handle:
            pickle.dump(value, handle, protocol=pickle.HIGHEST_PROTOCOL)

    @staticmethod
    def get_shared_state_pickle(path: str = "state.pickle") -> Any:
        """
        Get the state in the shared folder using pickle
        """
        import pickle

        with open(f"/shared/{path}", "rb") as handle:
            return pickle.load(handle)

    @staticmethod
    def set_shared_state(value: Any, path: str = "state.json") -> None:
        """
        Set the state in the shared folder using pickle
        """
        import json

        with open(f"/shared/{path}", "w", encoding="utf-8") as f:
            json.dump(value, f, ensure_ascii=False, indent=4)

    @staticmethod
    def get_shared_state(path: str = "state.json") -> None:
        """
        Get the state in the shared folder using pickle
        """
        import json

        with open(f"/shared/{path}", "r", encoding="utf-8") as f:
            return json.load(f)

    def get_resume_urls(self, approver: str = None) -> dict:
        nonce = random.randint(0, 1000000000)
        job_id = os.environ.get("WM_JOB_ID") or "NO_ID"
        return self.get(
            f"/w/{self.workspace}/jobs/resume_urls/{job_id}/{nonce}",
            params={"approver": approver},
        ).json()

    def request_interactive_slack_approval(
        self,
        slack_resource_path: str,
        channel_id: str,
        message: str = None,
        approver: str = None,
        default_args_json: dict = None,
        dynamic_enums_json: dict = None,
    ) -> None:
        """
        Sends an interactive approval request via Slack, allowing optional customization of the message, approver, and form fields.

        **[Enterprise Edition Only]** To include form fields in the Slack approval request, use the "Advanced -> Suspend -> Form" functionality.
        Learn more at: https://www.windmill.dev/docs/flows/flow_approval#form

        :param slack_resource_path: The path to the Slack resource in Windmill.
        :type slack_resource_path: str
        :param channel_id: The Slack channel ID where the approval request will be sent.
        :type channel_id: str
        :param message: Optional custom message to include in the Slack approval request.
        :type message: str, optional
        :param approver: Optional user ID or name of the approver for the request.
        :type approver: str, optional
        :param default_args_json: Optional dictionary defining or overriding the default arguments for form fields.
        :type default_args_json: dict, optional
        :param dynamic_enums_json: Optional dictionary overriding the enum default values of enum form fields.
        :type dynamic_enums_json: dict, optional

        :raises Exception: If the function is not called within a flow or flow preview.
        :raises Exception: If the required flow job or flow step environment variables are not set.

        :return: None

        **Usage Example:**
            >>> client.request_interactive_slack_approval(
            ...     slack_resource_path="/u/alex/my_slack_resource",
            ...     channel_id="admins-slack-channel",
            ...     message="Please approve this request",
            ...     approver="approver123",
            ...     default_args_json={"key1": "value1", "key2": 42},
            ...     dynamic_enums_json={"foo": ["choice1", "choice2"], "bar": ["optionA", "optionB"]},
            ... )

        **Notes:**
        - This function must be executed within a Windmill flow or flow preview.
        - The function checks for required environment variables (`WM_FLOW_JOB_ID`, `WM_FLOW_STEP_ID`) to ensure it is run in the appropriate context.
        """
        workspace = self.workspace
        flow_job_id = os.environ.get("WM_FLOW_JOB_ID")

        if not flow_job_id:
            raise Exception(
                "You can't use 'request_interactive_slack_approval' function in a standalone script or flow step preview. Please use it in a flow or a flow preview."
            )

        # Only include non-empty parameters
        params = {}
        if message:
            params["message"] = message
        if approver:
            params["approver"] = approver
        if slack_resource_path:
            params["slack_resource_path"] = slack_resource_path
        if channel_id:
            params["channel_id"] = channel_id
        if os.environ.get("WM_FLOW_STEP_ID"):
            params["flow_step_id"] = os.environ.get("WM_FLOW_STEP_ID")
        if default_args_json:
            params["default_args_json"] = json.dumps(default_args_json)
        if dynamic_enums_json:
            params["dynamic_enums_json"] = json.dumps(dynamic_enums_json)

        self.get(
            f"/w/{workspace}/jobs/slack_approval/{os.environ.get('WM_JOB_ID', 'NO_JOB_ID')}",
            params=params,
        )

    def username_to_email(self, username: str) -> str:
        """
        Get email from workspace username
        This method is particularly useful for apps that require the email address of the viewer.
        Indeed, in the viewer context WM_USERNAME is set to the username of the viewer but WM_EMAIL is set to the email of the creator of the app.
        """
        return self.get(f"/w/{self.workspace}/users/username_to_email/{username}").text

    def send_teams_message(
        self,
        conversation_id: str,
        text: str,
        success: bool = True,
        card_block: dict = None,
    ):
        """
        Send a message to a Microsoft Teams conversation with conversation_id, where success is used to style the message
        """
        return self.post(
            f"/teams/activities",
            json={
                "conversation_id": conversation_id,
                "text": text,
                "success": success,
                "card_block": card_block,
            },
        )


def init_global_client(f):
    @functools.wraps(f)
    def wrapper(*args, **kwargs):
        global _client
        if _client is None:
            _client = Windmill()
        return f(*args, **kwargs)

    return wrapper


def deprecate(in_favor_of: str):
    def decorator(f):
        @functools.wraps(f)
        def wrapper(*args, **kwargs):
            warnings.warn(
                (
                    f"The '{f.__name__}' method is deprecated and may be removed in the future. "
                    f"Consider {in_favor_of}"
                ),
                DeprecationWarning,
            )
            return f(*args, **kwargs)

        return wrapper

    return decorator


@init_global_client
def get_workspace() -> str:
    return _client.workspace


@init_global_client
def get_root_job_id(job_id: str | None = None) -> str:
    return _client.get_root_job_id(job_id)


@init_global_client
@deprecate("Windmill().version")
def get_version() -> str:
    return _client.version


@init_global_client
def run_script_async(
    hash_or_path: str,
    args: Dict[str, Any] = None,
    scheduled_in_secs: int = None,
) -> str:
    is_path = "/" in hash_or_path
    hash_ = None if is_path else hash_or_path
    path = hash_or_path if is_path else None
    return _client.run_script_async(
        hash_=hash_,
        path=path,
        args=args,
        scheduled_in_secs=scheduled_in_secs,
    )


@init_global_client
def run_flow_async(
    path: str,
    args: Dict[str, Any] = None,
    scheduled_in_secs: int = None,
    # can only be set to false if this the job will be fully await and not concurrent with any other job
    # as otherwise the child flow and its own child will store their state in the parent job which will
    # lead to incorrectness and failures
    do_not_track_in_parent: bool = True,
) -> str:
    return _client.run_flow_async(
        path=path,
        args=args,
        scheduled_in_secs=scheduled_in_secs,
        do_not_track_in_parent=do_not_track_in_parent,
    )


@init_global_client
def run_script_sync(
    hash: str,
    args: Dict[str, Any] = None,
    verbose: bool = False,
    assert_result_is_not_none: bool = True,
    cleanup: bool = True,
    timeout: dt.timedelta = None,
) -> Any:
    return _client.run_script(
        hash_=hash,
        args=args,
        verbose=verbose,
        assert_result_is_not_none=assert_result_is_not_none,
        cleanup=cleanup,
        timeout=timeout,
    )


@init_global_client
def run_script_by_path_async(
    path: str,
    args: Dict[str, Any] = None,
    scheduled_in_secs: Union[None, int] = None,
) -> str:
    return _client.run_script_by_path_async(
        path=path,
        args=args,
        scheduled_in_secs=scheduled_in_secs,
    )


@init_global_client
def run_script_by_hash_async(
    hash_: str,
    args: Dict[str, Any] = None,
    scheduled_in_secs: Union[None, int] = None,
) -> str:
    return _client.run_script_by_hash_async(
        hash_=hash_,
        args=args,
        scheduled_in_secs=scheduled_in_secs,
    )


@init_global_client
def run_script_by_path_sync(
    path: str,
    args: Dict[str, Any] = None,
    verbose: bool = False,
    assert_result_is_not_none: bool = True,
    cleanup: bool = True,
    timeout: dt.timedelta = None,
) -> Any:
    return _client.run_script(
        path=path,
        args=args,
        verbose=verbose,
        assert_result_is_not_none=assert_result_is_not_none,
        cleanup=cleanup,
        timeout=timeout,
    )


@init_global_client
def get_id_token(audience: str) -> str:
    """
    Get a JWT token for the given audience for OIDC purposes to login into third parties like AWS, Vault, GCP, etc.
    """
    return _client.get_id_token(audience)


@init_global_client
def get_job_status(job_id: str) -> JobStatus:
    return _client.get_job_status(job_id)


@init_global_client
def get_result(job_id: str, assert_result_is_not_none=True) -> Dict[str, Any]:
    return _client.get_result(
        job_id=job_id, assert_result_is_not_none=assert_result_is_not_none
    )


@init_global_client
def duckdb_connection_settings(s3_resource_path: str = "") -> DuckDbConnectionSettings:
    """
    Convenient helpers that takes an S3 resource as input and returns the settings necessary to
    initiate an S3 connection from DuckDB
    """
    return _client.get_duckdb_connection_settings(s3_resource_path)


@init_global_client
def polars_connection_settings(s3_resource_path: str = "") -> PolarsConnectionSettings:
    """
    Convenient helpers that takes an S3 resource as input and returns the settings necessary to
    initiate an S3 connection from Polars
    """
    return _client.get_polars_connection_settings(s3_resource_path)


@init_global_client
def boto3_connection_settings(s3_resource_path: str = "") -> Boto3ConnectionSettings:
    """
    Convenient helpers that takes an S3 resource as input and returns the settings necessary to
    initiate an S3 connection using boto3
    """
    return _client.get_boto3_connection_settings(s3_resource_path)


@init_global_client
def load_s3_file(s3object: S3Object | str, s3_resource_path: str | None = None) -> bytes:
    """
    Load the entire content of a file stored in S3 as bytes
    """
    return _client.load_s3_file(
        s3object, s3_resource_path if s3_resource_path != "" else None
    )


@init_global_client
def load_s3_file_reader(
    s3object: S3Object | str, s3_resource_path: str | None = None
) -> BufferedReader:
    """
    Load the content of a file stored in S3
    """
    return _client.load_s3_file_reader(
        s3object, s3_resource_path if s3_resource_path != "" else None
    )


@init_global_client
def write_s3_file(
    s3object: S3Object | str | None,
    file_content: BufferedReader | bytes,
    s3_resource_path: str | None = None,
    content_type: str | None = None,
    content_disposition: str | None = None,
) -> S3Object:
    """
    Upload a file to S3

    Content type will be automatically guessed from path extension if left empty

    See MDN for content_disposition: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Content-Disposition
    and content_type: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Content-Type

    """
    return _client.write_s3_file(
        s3object,
        file_content,
        s3_resource_path if s3_resource_path != "" else None,
        content_type,
        content_disposition,
    )


@init_global_client
def sign_s3_objects(s3_objects: list[S3Object | str]) -> list[S3Object]:
    """
    Sign S3 objects to be used by anonymous users in public apps
    Returns a list of signed s3 tokens
    """
    return _client.sign_s3_objects(s3_objects)


@init_global_client
def sign_s3_object(s3_object: S3Object| str) -> S3Object:
    """
    Sign S3 object to be used by anonymous users in public apps
    Returns a signed s3 object
    """
    return _client.sign_s3_object(s3_object)


@init_global_client
def whoami() -> dict:
    """
    Returns the current user
    """
    return _client.user


@init_global_client
@deprecate("Windmill().state")
def get_state() -> Any:
    """
    Get the state
    """
    return _client.state


@init_global_client
def get_resource(
    path: str,
    none_if_undefined: bool = False,
) -> dict | None:
    """Get resource from Windmill"""
    return _client.get_resource(path, none_if_undefined)


@init_global_client
def set_resource(path: str, value: Any, resource_type: str = "any") -> None:
    """
    Set the resource at a given path as a string, creating it if it does not exist
    """
    return _client.set_resource(value=value, path=path, resource_type=resource_type)


@init_global_client
def set_state(value: Any) -> None:
    """
    Set the state
    """
    return _client.set_state(value)


@init_global_client
def set_progress(value: int, job_id: Optional[str] = None) -> None:
    """
    Set the progress
    """
    return _client.set_progress(value, job_id)


@init_global_client
def get_progress(job_id: Optional[str] = None) -> Any:
    """
    Get the progress
    """
    return _client.get_progress(job_id)


def set_shared_state_pickle(value: Any, path="state.pickle") -> None:
    """
    Set the state in the shared folder using pickle
    """
    return Windmill.set_shared_state_pickle(value=value, path=path)


@deprecate("Windmill.get_shared_state_pickle(...)")
def get_shared_state_pickle(path="state.pickle") -> Any:
    """
    Get the state in the shared folder using pickle
    """
    return Windmill.get_shared_state_pickle(path=path)


def set_shared_state(value: Any, path="state.json") -> None:
    """
    Set the state in the shared folder using pickle
    """
    return Windmill.set_shared_state(value=value, path=path)


def get_shared_state(path="state.json") -> None:
    """
    Get the state in the shared folder using pickle
    """
    return Windmill.get_shared_state(path=path)


@init_global_client
def get_variable(path: str) -> str:
    """
    Returns the variable at a given path as a string
    """
    return _client.get_variable(path)


@init_global_client
def set_variable(path: str, value: str, is_secret: bool = False) -> None:
    """
    Set the variable at a given path as a string, creating it if it does not exist
    """
    return _client.set_variable(path, value, is_secret)


@init_global_client
def get_flow_user_state(key: str) -> Any:
    """
    Get the user state of a flow at a given key
    """
    return _client.get_flow_user_state(key)


@init_global_client
def set_flow_user_state(key: str, value: Any) -> None:
    """
    Set the user state of a flow at a given key
    """
    return _client.set_flow_user_state(key, value)


@init_global_client
def get_state_path() -> str:
    return _client.state_path


@init_global_client
def get_resume_urls(approver: str = None) -> dict:
    return _client.get_resume_urls(approver)


@init_global_client
def request_interactive_slack_approval(
    slack_resource_path: str,
    channel_id: str,
    message: str = None,
    approver: str = None,
    default_args_json: dict = None,
    dynamic_enums_json: dict = None,
) -> None:
    return _client.request_interactive_slack_approval(
        slack_resource_path=slack_resource_path,
        channel_id=channel_id,
        message=message,
        approver=approver,
        default_args_json=default_args_json,
        dynamic_enums_json=dynamic_enums_json,
    )


@init_global_client
def send_teams_message(
    conversation_id: str, text: str, success: bool, card_block: dict = None
):
    return _client.send_teams_message(conversation_id, text, success, card_block)


@init_global_client
def cancel_running() -> dict:
    """Cancel currently running executions of the same script."""
    return _client.cancel_running()


@init_global_client
def run_script(
    path: str = None,
    hash_: str = None,
    args: dict = None,
    timeout: dt.timedelta | int | float = None,
    verbose: bool = False,
    cleanup: bool = True,
    assert_result_is_not_none: bool = True,
) -> Any:
    """Run script synchronously and return its result.
    
    .. deprecated:: Use run_script_by_path or run_script_by_hash instead.
    """
    return _client.run_script(
        path=path,
        hash_=hash_,
        args=args,
        verbose=verbose,
        assert_result_is_not_none=assert_result_is_not_none,
        cleanup=cleanup,
        timeout=timeout,
    )


@init_global_client
def run_script_by_path(
    path: str,
    args: dict = None,
    timeout: dt.timedelta | int | float = None,
    verbose: bool = False,
    cleanup: bool = True,
    assert_result_is_not_none: bool = True,
) -> Any:
    """Run script by path synchronously and return its result."""
    return _client.run_script_by_path(
        path=path,
        args=args,
        verbose=verbose,
        assert_result_is_not_none=assert_result_is_not_none,
        cleanup=cleanup,
        timeout=timeout,
    )


@init_global_client
def run_script_by_hash(
    hash_: str,
    args: dict = None,
    timeout: dt.timedelta | int | float = None,
    verbose: bool = False,
    cleanup: bool = True,
    assert_result_is_not_none: bool = True,
) -> Any:
    """Run script by hash synchronously and return its result."""
    return _client.run_script_by_hash(
        hash_=hash_,
        args=args,
        verbose=verbose,
        assert_result_is_not_none=assert_result_is_not_none,
        cleanup=cleanup,
        timeout=timeout,
    )


@init_global_client
def username_to_email(username: str) -> str:
    """
    Get email from workspace username
    This method is particularly useful for apps that require the email address of the viewer.
    Indeed, in the viewer context WM_USERNAME is set to the username of the viewer but WM_EMAIL is set to the email of the creator of the app.
    """
    return _client.username_to_email(username)


def task(*args, **kwargs):
    from inspect import signature

    def f(func, tag: str | None = None):
        if (
            os.environ.get("WM_JOB_ID") is None
            or os.environ.get("MAIN_OVERRIDE") == func.__name__
        ):

            def inner(*args, **kwargs):
                return func(*args, **kwargs)

            return inner
        else:

            def inner(*args, **kwargs):
                global _client
                if _client is None:
                    _client = Windmill()
                w_id = os.environ.get("WM_WORKSPACE")
                job_id = os.environ.get("WM_JOB_ID")
                f_name = func.__name__
                json = kwargs
                params = list(signature(func).parameters)
                for i, arg in enumerate(args):
                    if i < len(params):
                        p = params[i]
                        key = p
                        if key not in kwargs:
                            json[key] = arg

                params = {}
                if tag is not None:
                    params["tag"] = tag
                r = _client.post(
                    f"/w/{w_id}/jobs/run/workflow_as_code/{job_id}/{f_name}",
                    json={"args": json},
                    params=params,
                )
                job_id = r.text
                print(f"Executing task {func.__name__} on job {job_id}")
                r = _client.wait_job(job_id)
                print(f"Task {func.__name__} ({job_id}) completed")
                return r

            return inner

    if len(args) == 1 and len(kwargs) == 0 and callable(args[0]):
        return f(args[0], None)
    else:
        return lambda x: f(x, kwargs.get("tag"))

def parse_resource_syntax(s: str) -> Optional[str]:
    """Parse resource syntax from string."""
    if s is None:
        return None
    if s.startswith("$res:"):
        return s[5:]
    if s.startswith("res://"):
        return s[6:]
    return None

def parse_s3_object(s3_object: S3Object | str) -> S3Object:
    """Parse S3 object from string or S3Object format."""
    if isinstance(s3_object, str):
        match = re.match(r'^s3://([^/]*)/(.*)$', s3_object)
        if match:
            return S3Object(s3=match.group(2) or "", storage=match.group(1) or None)
        return S3Object(s3="")
    else:
        return s3_object

    

def parse_variable_syntax(s: str) -> Optional[str]:
    """Parse variable syntax from string."""
    if s.startswith("var://"):
        return s[6:]
    return None
