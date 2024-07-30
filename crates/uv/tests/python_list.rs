#![cfg(all(feature = "python", feature = "pypi"))]

use common::{uv_snapshot, TestContext};

mod common;

#[cfg(unix)]
#[test]
fn python_list_unsupported_python() {
    let context: TestContext = TestContext::new_with_versions(&[]);

    // 3.6.15 is not supported, `uv python list` should ignore it.
    let mocked_interpreter = context.temp_dir.join("python");
    let json = indoc::indoc! {r#"
        {
            "result": "success",
            "markers": {
                "implementation_name": "cpython",
                "implementation_version": "3.6.15",
                "os_name": "posix",
                "platform_machine": "arm64",
                "platform_python_implementation": "CPython",
                "platform_release": "23.5.0",
                "platform_system": "Darwin",
                "platform_version": "Darwin Kernel Version 23.5.0: Wed May  1 20:13:18 PDT 2024; root:xnu-10063.121.3~5/RELEASE_ARM64_T6030",
                "python_full_version": "3.6.15",
                "python_version": "3.6",
                "sys_platform": "darwin"
            },
            "sys_base_prefix": "/home/ferris/.pyenv/versions/3.6.15",
            "sys_base_exec_prefix": "/home/ferris/.pyenv/versions/3.6.15",
            "sys_prefix": "/home/ferris/.pyenv/versions/3.6.15",
            "sys_base_executable": null,
            "sys_executable": "/home/ferris/.pyenv/versions/3.6.15/bin/python",
            "sys_path": [
                "/private/tmp",
                "/home/ferris/.pyenv/versions/3.6.15/lib/python36.zip",
                "/home/ferris/.pyenv/versions/3.6.15/lib/python3.6",
                "/home/ferris/.pyenv/versions/3.6.15/lib/python3.6/lib-dynload",
                "/home/ferris/.pyenv/versions/3.6.15/lib/python3.6/site-packages"
            ],
            "stdlib": "/home/ferris/.pyenv/versions/3.6.15/lib/python3.6",
            "scheme": {
                "platlib": "/home/ferris/.pyenv/versions/3.6.15/lib/python3.6/site-packages",
                "purelib": "/home/ferris/.pyenv/versions/3.6.15/lib/python3.6/site-packages",
                "include": "/home/ferris/.pyenv/versions/3.6.15/include/python3.6m",
                "scripts": "/home/ferris/.pyenv/versions/3.6.15/bin",
                "data": "/home/ferris/.pyenv/versions/3.6.15"
            },
            "virtualenv": {
                "purelib": "lib/python3.6/site-packages",
                "platlib": "lib/python3.6/site-packages",
                "include": "include/site/python3.6",
                "scripts": "bin",
                "data": ""
            },
            "platform": {
                "os": {
                    "name": "macos",
                    "major": 14,
                    "minor": 5
                },
                "arch": "arm64"
            },
            "gil_disabled": false,
            "pointer_size": "64"
        }
        "#};

    fs_err::write(
        &mocked_interpreter,
        indoc::formatdoc! {r##"
            #!/bin/bash
            echo '{json}'
            "##},
    )
    .unwrap();
    fs_err::set_permissions(
        &mocked_interpreter,
        std::os::unix::fs::PermissionsExt::from_mode(0o770),
    )
    .unwrap();

    uv_snapshot!(context.filters(), context.python_list().env("UV_TEST_PYTHON_PATH", context.temp_dir.as_os_str()), @r###"
        success: true
        exit_code: 0
        ----- stdout -----
        cpython-3.6.15-macos-aarch64-none    /home/ferris/.pyenv/versions/3.6.15/bin/python

        ----- stderr -----
        "###);
}
