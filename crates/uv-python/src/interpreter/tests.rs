use std::str::FromStr;

use fs_err as fs;
use indoc::{formatdoc, indoc};
use tempfile::tempdir;

use uv_cache::Cache;
use uv_pep440::Version;

use crate::Interpreter;

#[test]
fn test_cache_invalidation() {
    let mock_dir = tempdir().unwrap();
    let mocked_interpreter = mock_dir.path().join("python");
    let json = indoc! {r##"
        {
            "result": "success",
            "platform": {
                "os": {
                    "name": "manylinux",
                    "major": 2,
                    "minor": 38
                },
                "arch": "x86_64"
            },
            "manylinux_compatible": false,
            "markers": {
                "implementation_name": "cpython",
                "implementation_version": "3.12.0",
                "os_name": "posix",
                "platform_machine": "x86_64",
                "platform_python_implementation": "CPython",
                "platform_release": "6.5.0-13-generic",
                "platform_system": "Linux",
                "platform_version": "#13-Ubuntu SMP PREEMPT_DYNAMIC Fri Nov  3 12:16:05 UTC 2023",
                "python_full_version": "3.12.0",
                "python_version": "3.12",
                "sys_platform": "linux"
            },
            "sys_base_exec_prefix": "/home/ferris/.pyenv/versions/3.12.0",
            "sys_base_prefix": "/home/ferris/.pyenv/versions/3.12.0",
            "sys_prefix": "/home/ferris/projects/uv/.venv",
            "sys_executable": "/home/ferris/projects/uv/.venv/bin/python",
            "sys_path": [
                "/home/ferris/.pyenv/versions/3.12.0/lib/python3.12/lib/python3.12",
                "/home/ferris/.pyenv/versions/3.12.0/lib/python3.12/site-packages"
            ],
            "stdlib": "/home/ferris/.pyenv/versions/3.12.0/lib/python3.12",
            "scheme": {
                "data": "/home/ferris/.pyenv/versions/3.12.0",
                "include": "/home/ferris/.pyenv/versions/3.12.0/include",
                "platlib": "/home/ferris/.pyenv/versions/3.12.0/lib/python3.12/site-packages",
                "purelib": "/home/ferris/.pyenv/versions/3.12.0/lib/python3.12/site-packages",
                "scripts": "/home/ferris/.pyenv/versions/3.12.0/bin"
            },
            "virtualenv": {
                "data": "",
                "include": "include",
                "platlib": "lib/python3.12/site-packages",
                "purelib": "lib/python3.12/site-packages",
                "scripts": "bin"
            },
            "pointer_size": "64",
            "gil_disabled": true
        }
    "##};

    let cache = Cache::temp().unwrap().init().unwrap();

    fs::write(
        &mocked_interpreter,
        formatdoc! {r##"
        #!/bin/bash
        echo '{json}'
        "##},
    )
    .unwrap();

    fs::set_permissions(
        &mocked_interpreter,
        std::os::unix::fs::PermissionsExt::from_mode(0o770),
    )
    .unwrap();
    let interpreter = Interpreter::query(&mocked_interpreter, &cache).unwrap();
    assert_eq!(
        interpreter.markers.python_version().version,
        Version::from_str("3.12").unwrap()
    );
    fs::write(
        &mocked_interpreter,
        formatdoc! {r##"
        #!/bin/bash
        echo '{}'
        "##, json.replace("3.12", "3.13")},
    )
    .unwrap();
    let interpreter = Interpreter::query(&mocked_interpreter, &cache).unwrap();
    assert_eq!(
        interpreter.markers.python_version().version,
        Version::from_str("3.13").unwrap()
    );
}
