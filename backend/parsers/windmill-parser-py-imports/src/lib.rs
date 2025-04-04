/*
 * Author: Ruben Fiszel
 * Copyright: Windmill Labs, Inc 2022
 * This file and its contents are licensed under the AGPLv3 License.
 * Please see the included NOTICE for copyright information and
 * LICENSE-AGPL for a copy of the license.
 */

mod mapping;

use async_recursion::async_recursion;
use itertools::Itertools;
use lazy_static::lazy_static;

use mapping::{FULL_IMPORTS_MAP, SHORT_IMPORTS_MAP};
#[cfg(not(target_arch = "wasm32"))]
use regex::Regex;
#[cfg(target_arch = "wasm32")]
use regex_lite::Regex;

use rustpython_parser::{
    ast::{Stmt, StmtImport, StmtImportFrom, Suite},
    Parse,
};
use sqlx::{Pool, Postgres};
use windmill_common::{error, worker::PythonAnnotations};

const DEF_MAIN: &str = "def main(";

fn replace_import(x: String) -> String {
    SHORT_IMPORTS_MAP
        .get(&x)
        .map(|x| x.to_owned())
        .unwrap_or(&x)
        .to_string()
}

fn replace_full_import(x: &str) -> Option<String> {
    FULL_IMPORTS_MAP.get(x).map(|x| (*x).to_owned())
}

lazy_static! {
    static ref RE: Regex = Regex::new(r"^\#\s?(\S+)\s*$").unwrap();
}

fn process_import(module: Option<String>, path: &str, level: usize) -> Vec<String> {
    if level > 0 {
        let mut imports = vec![];
        let splitted_path = path.split("/");
        let base = splitted_path
            .clone()
            .take(splitted_path.count() - level)
            .join("/");
        if let Some(m) = module {
            imports.push(format!("relative:{base}/{}", m.replace(".", "/")));
        } else {
            imports.push(format!("relative:{base}"));
        }
        imports
    } else if let Some(module) = module {
        let imprt = module.split('.').next().unwrap_or("").replace("_", "-");
        if imprt == "u" || imprt == "f" {
            vec![format!("relative:{}", module.replace(".", "/"))]
        } else {
            vec![replace_full_import(&module).unwrap_or(replace_import(imprt))]
        }
    } else {
        vec![]
    }
}

pub fn parse_relative_imports(code: &str, path: &str) -> error::Result<Vec<String>> {
    let nimports = parse_code_for_imports(code, path)?;
    return Ok(nimports
        .into_iter()
        .filter_map(|x| {
            if x.starts_with("relative:") {
                Some(x.replace("relative:", ""))
            } else {
                None
            }
        })
        .collect());
}

fn parse_code_for_imports(code: &str, path: &str) -> error::Result<Vec<String>> {
    let mut code = code.split(DEF_MAIN).next().unwrap_or("").to_string();

    // remove main function decorator from end of file if it exists
    if code
        .lines()
        .last()
        .map(|x| x.starts_with("@"))
        .unwrap_or(false)
    {
        code = code
            .lines()
            .take(code.lines().count() - 1)
            .collect::<Vec<&str>>()
            .join("\n")
            + "\n";
    }

    let ast = Suite::parse(&code, "main.py").map_err(|e| {
        error::Error::ExecutionErr(format!("Error parsing code for imports: {}", e.to_string()))
    })?;
    let nimports: Vec<String> = ast
        .into_iter()
        .filter_map(|x| match x {
            Stmt::Import(StmtImport { names, .. }) => Some(
                names
                    .into_iter()
                    .map(|x| {
                        let name = x.name.to_string();
                        process_import(Some(name), path, 0)
                    })
                    .flatten()
                    .collect::<Vec<String>>(),
            ),
            Stmt::ImportFrom(StmtImportFrom { level: Some(i), module, .. }) if i.to_u32() > 0 => {
                Some(process_import(
                    module.map(|x| x.to_string()),
                    path,
                    i.to_usize(),
                ))
            }
            Stmt::ImportFrom(StmtImportFrom { level: _, module, .. }) => {
                Some(process_import(module.map(|x| x.to_string()), path, 0))
            }
            _ => None,
        })
        .flatten()
        .filter(|x| !STDIMPORTS.contains(&x.as_str()))
        .unique()
        .collect();
    return Ok(nimports);
}

pub async fn parse_python_imports(
    code: &str,
    w_id: &str,
    path: &str,
    db: &Pool<Postgres>,
    already_visited: &mut Vec<String>,
    annotated_pyv_numeric: &mut Option<u32>,
) -> error::Result<Vec<String>> {
    parse_python_imports_inner(
        code,
        w_id,
        path,
        db,
        already_visited,
        annotated_pyv_numeric,
        &mut annotated_pyv_numeric.and_then(|_| Some(path.to_owned())),
    )
    .await
}

#[async_recursion]
async fn parse_python_imports_inner(
    code: &str,
    w_id: &str,
    path: &str,
    db: &Pool<Postgres>,
    already_visited: &mut Vec<String>,
    annotated_pyv_numeric: &mut Option<u32>,
    path_where_annotated_pyv: &mut Option<String>,
) -> error::Result<Vec<String>> {
    let PythonAnnotations { py310, py311, py312, py313, .. } = PythonAnnotations::parse(&code);

    // we pass only if there is none or only one annotation

    // Naive:
    // 1. Check if there are multiple annotated version
    // 2. If no, take one and compare with annotated version
    // 3. We continue if same or replace none with new one

    // Optimized:
    // 1. Iterate over all annotations compare each with annotated_pyv and replace on flight
    // 2. If annotated_pyv is different version, throw and error

    // This way we make sure there is no multiple annotations for same script
    // and we get detailed span on conflicting versions

    let mut check = |is_py_xyz, numeric| -> error::Result<()> {
        if is_py_xyz {
            if let Some(v) = annotated_pyv_numeric {
                if *v != numeric {
                    return Err(error::Error::from(anyhow::anyhow!(
                        "Annotated 2 or more different python versions: \n - py{v} at {}\n - py{numeric} at {path}\nIt is possible to use only one.",
                        path_where_annotated_pyv.clone().unwrap_or("Unknown".to_owned())
                    )));
                }
            } else {
                *annotated_pyv_numeric = Some(numeric);
            }

            *path_where_annotated_pyv = Some(path.to_owned());
        }
        Ok(())
    };

    check(py310, 310)?;
    check(py311, 311)?;
    check(py312, 312)?;
    check(py313, 313)?;

    let find_requirements = code
        .lines()
        .find_position(|x| x.starts_with("#requirements:") || x.starts_with("# requirements:"));
    if let Some((pos, _)) = find_requirements {
        let lines = code
            .lines()
            .skip(pos + 1)
            .map_while(|x| {
                RE.captures(x)
                    .map(|x| x.get(1).unwrap().as_str().to_string())
            })
            .collect();
        Ok(lines)
    } else {
        let find_extra_requirements = code.lines().find_position(|x| {
            x.starts_with("#extra_requirements:") || x.starts_with("# extra_requirements:")
        });
        let mut imports: Vec<String> = vec![];
        if let Some((pos, _)) = find_extra_requirements {
            let lines: Vec<String> = code
                .lines()
                .skip(pos + 1)
                .map_while(|x| {
                    RE.captures(x)
                        .map(|x| x.get(1).unwrap().as_str().to_string())
                })
                .collect();
            imports.extend(lines);
        }

        let nimports = parse_code_for_imports(code, path)?;
        for n in nimports.iter() {
            let nested = if n.starts_with("relative:") {
                let rpath = n.replace("relative:", "");
                let code = sqlx::query_scalar!(
                    r#"
                    SELECT content FROM script WHERE path = $1 AND workspace_id = $2
                    AND created_at = (SELECT max(created_at) FROM script WHERE path = $1 AND
                    workspace_id = $2)
                    "#,
                    &rpath,
                    w_id
                )
                .fetch_optional(db)
                .await?
                .unwrap_or_else(|| "".to_string());

                if already_visited.contains(&rpath) {
                    vec![]
                } else {
                    already_visited.push(rpath.clone());
                    parse_python_imports_inner(
                        &code,
                        w_id,
                        &rpath,
                        db,
                        already_visited,
                        annotated_pyv_numeric,
                        path_where_annotated_pyv,
                    )
                    .await?
                }
            } else {
                vec![n.to_string()]
            };
            for imp in nested {
                if !imports.contains(&imp) {
                    imports.push(imp);
                }
            }
        }
        imports.sort();
        Ok(imports)
    }
}

const STDIMPORTS: [&str; 303] = [
    "--future--",
    "-abc",
    "-aix-support",
    "-ast",
    "-asyncio",
    "-bisect",
    "-blake2",
    "-bootsubprocess",
    "-bz2",
    "-codecs",
    "-codecs-cn",
    "-codecs-hk",
    "-codecs-iso2022",
    "-codecs-jp",
    "-codecs-kr",
    "-codecs-tw",
    "-collections",
    "-collections-abc",
    "-compat-pickle",
    "-compression",
    "-contextvars",
    "-crypt",
    "-csv",
    "-ctypes",
    "-curses",
    "-curses-panel",
    "-datetime",
    "-dbm",
    "-decimal",
    "-elementtree",
    "-frozen-importlib",
    "-frozen-importlib-external",
    "-functools",
    "-gdbm",
    "-hashlib",
    "-heapq",
    "-imp",
    "-io",
    "-json",
    "-locale",
    "-lsprof",
    "-lzma",
    "-markupbase",
    "-md5",
    "-msi",
    "-multibytecodec",
    "-multiprocessing",
    "-opcode",
    "-operator",
    "-osx-support",
    "-overlapped",
    "-pickle",
    "-posixshmem",
    "-posixsubprocess",
    "-py-abc",
    "-pydecimal",
    "-pyio",
    "-queue",
    "-random",
    "-sha1",
    "-sha256",
    "-sha3",
    "-sha512",
    "-signal",
    "-sitebuiltins",
    "-socket",
    "-sqlite3",
    "-sre",
    "-ssl",
    "-stat",
    "-statistics",
    "-string",
    "-strptime",
    "-struct",
    "-symtable",
    "-thread",
    "-threading-local",
    "-tkinter",
    "-tracemalloc",
    "-uuid",
    "-warnings",
    "-weakref",
    "-weakrefset",
    "-winapi",
    "-zoneinfo",
    "zoneinfo",
    "abc",
    "aifc",
    "antigravity",
    "argparse",
    "array",
    "ast",
    "asynchat",
    "asyncio",
    "asyncore",
    "atexit",
    "audioop",
    "base64",
    "bdb",
    "binascii",
    "binhex",
    "bisect",
    "builtins",
    "bz2",
    "cProfile",
    "calendar",
    "cgi",
    "cgitb",
    "chunk",
    "cmath",
    "cmd",
    "code",
    "codecs",
    "codeop",
    "collections",
    "colorsys",
    "compileall",
    "concurrent",
    "configparser",
    "contextlib",
    "contextvars",
    "copy",
    "copyreg",
    "crypt",
    "csv",
    "ctypes",
    "curses",
    "dataclasses",
    "datetime",
    "dbm",
    "decimal",
    "difflib",
    "dis",
    "distutils",
    "doctest",
    "email",
    "encodings",
    "ensurepip",
    "enum",
    "errno",
    "faulthandler",
    "fcntl",
    "filecmp",
    "fileinput",
    "fnmatch",
    "fractions",
    "ftplib",
    "functools",
    "gc",
    "genericpath",
    "getopt",
    "getpass",
    "gettext",
    "glob",
    "graphlib",
    "grp",
    "gzip",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "idlelib",
    "imaplib",
    "imghdr",
    "imp",
    "importlib",
    "inspect",
    "io",
    "ipaddress",
    "itertools",
    "json",
    "keyword",
    "lib2to3",
    "linecache",
    "locale",
    "logging",
    "lzma",
    "mailbox",
    "mailcap",
    "marshal",
    "math",
    "mimetypes",
    "mmap",
    "modulefinder",
    "msilib",
    "msvcrt",
    "multiprocessing",
    "netrc",
    "nis",
    "nntplib",
    "nt",
    "ntpath",
    "nturl2path",
    "numbers",
    "opcode",
    "operator",
    "optparse",
    "os",
    "ossaudiodev",
    "pathlib",
    "pdb",
    "pickle",
    "pickletools",
    "pipes",
    "pkgutil",
    "platform",
    "plistlib",
    "poplib",
    "posix",
    "posixpath",
    "pprint",
    "profile",
    "pstats",
    "pty",
    "pwd",
    "py-compile",
    "pyclbr",
    "pydoc",
    "pydoc-data",
    "pyexpat",
    "queue",
    "quopri",
    "random",
    "re",
    "readline",
    "reprlib",
    "resource",
    "rlcompleter",
    "runpy",
    "sched",
    "secrets",
    "select",
    "selectors",
    "shelve",
    "shlex",
    "shutil",
    "signal",
    "site",
    "smtpd",
    "smtplib",
    "sndhdr",
    "socket",
    "socketserver",
    "spwd",
    "sqlite3",
    "sre-compile",
    "sre-constants",
    "sre-parse",
    "ssl",
    "stat",
    "statistics",
    "string",
    "stringprep",
    "struct",
    "subprocess",
    "sunau",
    "symtable",
    "sys",
    "sysconfig",
    "syslog",
    "tabnanny",
    "tarfile",
    "telnetlib",
    "tempfile",
    "termios",
    "textwrap",
    "this",
    "threading",
    "time",
    "timeit",
    "tkinter",
    "token",
    "tokenize",
    "trace",
    "traceback",
    "tracemalloc",
    "tty",
    "turtle",
    "turtledemo",
    "types",
    "typing",
    "unicodedata",
    "unittest",
    "urllib",
    "uu",
    "uuid",
    "venv",
    "warnings",
    "wave",
    "weakref",
    "webbrowser",
    "winreg",
    "winsound",
    "wsgiref",
    "xdrlib",
    "xml",
    "xmlrpc",
    "zipapp",
    "zipfile",
    "zipimport",
    "zlib",
    "",
];
