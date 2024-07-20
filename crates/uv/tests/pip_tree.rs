use std::process::Command;

use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;

use common::uv_snapshot;

use crate::common::{get_bin, TestContext};

mod common;

#[test]
fn no_package() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----


    ----- stderr -----
    "###
    );
}

#[test]
fn prune_last_in_the_subgroup() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), context.pip_tree().arg("--prune").arg("certifi"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    └── urllib3 v2.2.1

    ----- stderr -----
    "###
    );
}

#[test]
fn single_package() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    ├── urllib3 v2.2.1
    └── certifi v2024.2.2

    ----- stderr -----
    "###
    );
}
// `pandas` requires `numpy` with markers on Python version.
#[test]
#[cfg(not(windows))]
fn python_version_marker() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pandas==2.2.1").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + numpy==1.26.4
     + pandas==2.2.1
     + python-dateutil==2.9.0.post0
     + pytz==2024.1
     + six==1.16.0
     + tzdata==2024.1

    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pandas v2.2.1
    ├── numpy v1.26.4
    ├── python-dateutil v2.9.0.post0
    │   └── six v1.16.0
    ├── pytz v2024.1
    └── tzdata v2024.1

    ----- stderr -----
    "###
    );
}

#[test]
fn nested_dependencies() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("scikit-learn==1.4.1.post1")
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + joblib==1.3.2
     + numpy==1.26.4
     + scikit-learn==1.4.1.post1
     + scipy==1.12.0
     + threadpoolctl==3.4.0
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scikit-learn v1.4.1.post1
    ├── numpy v1.26.4
    ├── scipy v1.12.0
    │   └── numpy v1.26.4
    ├── joblib v1.3.2
    └── threadpoolctl v3.4.0

    ----- stderr -----
    "###
    );
}

// Identical test as `invert` since `--reverse` is simply an alias for `--invert`.
#[test]
fn reverse() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("scikit-learn==1.4.1.post1")
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + joblib==1.3.2
     + numpy==1.26.4
     + scikit-learn==1.4.1.post1
     + scipy==1.12.0
     + threadpoolctl==3.4.0
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--reverse"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    joblib v1.3.2
    └── scikit-learn v1.4.1.post1
    numpy v1.26.4
    ├── scikit-learn v1.4.1.post1
    └── scipy v1.12.0
        └── scikit-learn v1.4.1.post1
    threadpoolctl v3.4.0
    └── scikit-learn v1.4.1.post1

    ----- stderr -----
    "###
    );
}

#[test]
fn invert() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("scikit-learn==1.4.1.post1")
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + joblib==1.3.2
     + numpy==1.26.4
     + scikit-learn==1.4.1.post1
     + scipy==1.12.0
     + threadpoolctl==3.4.0
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    joblib v1.3.2
    └── scikit-learn v1.4.1.post1
    numpy v1.26.4
    ├── scikit-learn v1.4.1.post1
    └── scipy v1.12.0
        └── scikit-learn v1.4.1.post1
    threadpoolctl v3.4.0
    └── scikit-learn v1.4.1.post1

    ----- stderr -----
    "###
    );
}

#[test]
fn depth() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("scikit-learn==1.4.1.post1")
        .unwrap();

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + joblib==1.3.2
     + numpy==1.26.4
     + scikit-learn==1.4.1.post1
     + scipy==1.12.0
     + threadpoolctl==3.4.0
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--depth")
        .arg("0")
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scikit-learn v1.4.1.post1

    ----- stderr -----

    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--depth")
        .arg("1")
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scikit-learn v1.4.1.post1
    ├── numpy v1.26.4
    ├── scipy v1.12.0
    ├── joblib v1.3.2
    └── threadpoolctl v3.4.0

    ----- stderr -----

    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--depth")
        .arg("2")
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scikit-learn v1.4.1.post1
    ├── numpy v1.26.4
    ├── scipy v1.12.0
    │   └── numpy v1.26.4
    ├── joblib v1.3.2
    └── threadpoolctl v3.4.0

    ----- stderr -----
    "###
    );
}

#[test]
fn prune() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("scikit-learn==1.4.1.post1")
        .unwrap();

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + joblib==1.3.2
     + numpy==1.26.4
     + scikit-learn==1.4.1.post1
     + scipy==1.12.0
     + threadpoolctl==3.4.0
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--prune")
        .arg("numpy")
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scikit-learn v1.4.1.post1
    ├── scipy v1.12.0
    ├── joblib v1.3.2
    └── threadpoolctl v3.4.0

    ----- stderr -----
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--prune")
        .arg("numpy")
        .arg("--prune")
        .arg("joblib")
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scikit-learn v1.4.1.post1
    ├── scipy v1.12.0
    └── threadpoolctl v3.4.0

    ----- stderr -----
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--prune")
        .arg("scipy")
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scikit-learn v1.4.1.post1
    ├── numpy v1.26.4
    ├── joblib v1.3.2
    └── threadpoolctl v3.4.0

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(target_os = "macos")]
fn complex_nested_dependencies_inverted() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("packse").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 32 packages in [TIME]
    Prepared 32 packages in [TIME]
    Installed 32 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + chevron-blue==0.2.1
     + docutils==0.20.1
     + hatchling==1.22.4
     + idna==3.6
     + importlib-metadata==7.1.0
     + jaraco-classes==3.3.1
     + jaraco-context==4.3.0
     + jaraco-functools==4.0.0
     + keyring==25.0.0
     + markdown-it-py==3.0.0
     + mdurl==0.1.2
     + more-itertools==10.2.0
     + msgspec==0.18.6
     + nh3==0.2.15
     + packaging==24.0
     + packse==0.3.12
     + pathspec==0.12.1
     + pkginfo==1.10.0
     + pluggy==1.4.0
     + pygments==2.17.2
     + readme-renderer==43.0
     + requests==2.31.0
     + requests-toolbelt==1.0.0
     + rfc3986==2.0.0
     + rich==13.7.1
     + setuptools==69.2.0
     + trove-classifiers==2024.3.3
     + twine==4.0.2
     + urllib3==2.2.1
     + zipp==3.18.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    certifi v2024.2.2
    └── requests v2.31.0
        ├── requests-toolbelt v1.0.0
        │   └── twine v4.0.2
        │       └── packse v0.3.12
        └── twine v4.0.2 (*)
    charset-normalizer v3.3.2
    └── requests v2.31.0 (*)
    chevron-blue v0.2.1
    └── packse v0.3.12
    docutils v0.20.1
    └── readme-renderer v43.0
        └── twine v4.0.2 (*)
    idna v3.6
    └── requests v2.31.0 (*)
    jaraco-context v4.3.0
    └── keyring v25.0.0
        └── twine v4.0.2 (*)
    mdurl v0.1.2
    └── markdown-it-py v3.0.0
        └── rich v13.7.1
            └── twine v4.0.2 (*)
    more-itertools v10.2.0
    ├── jaraco-classes v3.3.1
    │   └── keyring v25.0.0 (*)
    └── jaraco-functools v4.0.0
        └── keyring v25.0.0 (*)
    msgspec v0.18.6
    └── packse v0.3.12
    nh3 v0.2.15
    └── readme-renderer v43.0 (*)
    packaging v24.0
    └── hatchling v1.22.4
        └── packse v0.3.12
    pathspec v0.12.1
    └── hatchling v1.22.4 (*)
    pkginfo v1.10.0
    └── twine v4.0.2 (*)
    pluggy v1.4.0
    └── hatchling v1.22.4 (*)
    pygments v2.17.2
    ├── readme-renderer v43.0 (*)
    └── rich v13.7.1 (*)
    rfc3986 v2.0.0
    └── twine v4.0.2 (*)
    setuptools v69.2.0
    └── packse v0.3.12
    trove-classifiers v2024.3.3
    └── hatchling v1.22.4 (*)
    urllib3 v2.2.1
    ├── requests v2.31.0 (*)
    └── twine v4.0.2 (*)
    zipp v3.18.1
    └── importlib-metadata v7.1.0
        └── twine v4.0.2 (*)
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(target_os = "macos")]
fn complex_nested_dependencies() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("packse").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 32 packages in [TIME]
    Prepared 32 packages in [TIME]
    Installed 32 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + chevron-blue==0.2.1
     + docutils==0.20.1
     + hatchling==1.22.4
     + idna==3.6
     + importlib-metadata==7.1.0
     + jaraco-classes==3.3.1
     + jaraco-context==4.3.0
     + jaraco-functools==4.0.0
     + keyring==25.0.0
     + markdown-it-py==3.0.0
     + mdurl==0.1.2
     + more-itertools==10.2.0
     + msgspec==0.18.6
     + nh3==0.2.15
     + packaging==24.0
     + packse==0.3.12
     + pathspec==0.12.1
     + pkginfo==1.10.0
     + pluggy==1.4.0
     + pygments==2.17.2
     + readme-renderer==43.0
     + requests==2.31.0
     + requests-toolbelt==1.0.0
     + rfc3986==2.0.0
     + rich==13.7.1
     + setuptools==69.2.0
     + trove-classifiers==2024.3.3
     + twine==4.0.2
     + urllib3==2.2.1
     + zipp==3.18.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    packse v0.3.12
    ├── chevron-blue v0.2.1
    ├── hatchling v1.22.4
    │   ├── packaging v24.0
    │   ├── pathspec v0.12.1
    │   ├── pluggy v1.4.0
    │   └── trove-classifiers v2024.3.3
    ├── msgspec v0.18.6
    ├── setuptools v69.2.0
    └── twine v4.0.2
        ├── pkginfo v1.10.0
        ├── readme-renderer v43.0
        │   ├── nh3 v0.2.15
        │   ├── docutils v0.20.1
        │   └── pygments v2.17.2
        ├── requests v2.31.0
        │   ├── charset-normalizer v3.3.2
        │   ├── idna v3.6
        │   ├── urllib3 v2.2.1
        │   └── certifi v2024.2.2
        ├── requests-toolbelt v1.0.0
        │   └── requests v2.31.0 (*)
        ├── urllib3 v2.2.1
        ├── importlib-metadata v7.1.0
        │   └── zipp v3.18.1
        ├── keyring v25.0.0
        │   ├── jaraco-classes v3.3.1
        │   │   └── more-itertools v10.2.0
        │   ├── jaraco-functools v4.0.0
        │   │   └── more-itertools v10.2.0
        │   └── jaraco-context v4.3.0
        ├── rfc3986 v2.0.0
        └── rich v13.7.1
            ├── markdown-it-py v3.0.0
            │   └── mdurl v0.1.2
            └── pygments v2.17.2
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(target_os = "macos")]
fn prune_large_tree() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("packse").unwrap();

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 32 packages in [TIME]
    Prepared 32 packages in [TIME]
    Installed 32 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + chevron-blue==0.2.1
     + docutils==0.20.1
     + hatchling==1.22.4
     + idna==3.6
     + importlib-metadata==7.1.0
     + jaraco-classes==3.3.1
     + jaraco-context==4.3.0
     + jaraco-functools==4.0.0
     + keyring==25.0.0
     + markdown-it-py==3.0.0
     + mdurl==0.1.2
     + more-itertools==10.2.0
     + msgspec==0.18.6
     + nh3==0.2.15
     + packaging==24.0
     + packse==0.3.12
     + pathspec==0.12.1
     + pkginfo==1.10.0
     + pluggy==1.4.0
     + pygments==2.17.2
     + readme-renderer==43.0
     + requests==2.31.0
     + requests-toolbelt==1.0.0
     + rfc3986==2.0.0
     + rich==13.7.1
     + setuptools==69.2.0
     + trove-classifiers==2024.3.3
     + twine==4.0.2
     + urllib3==2.2.1
     + zipp==3.18.1
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--prune")
        .arg("hatchling")
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    packse v0.3.12
    ├── chevron-blue v0.2.1
    ├── msgspec v0.18.6
    ├── setuptools v69.2.0
    └── twine v4.0.2
        ├── pkginfo v1.10.0
        ├── readme-renderer v43.0
        │   ├── nh3 v0.2.15
        │   ├── docutils v0.20.1
        │   └── pygments v2.17.2
        ├── requests v2.31.0
        │   ├── charset-normalizer v3.3.2
        │   ├── idna v3.6
        │   ├── urllib3 v2.2.1
        │   └── certifi v2024.2.2
        ├── requests-toolbelt v1.0.0
        │   └── requests v2.31.0 (*)
        ├── urllib3 v2.2.1
        ├── importlib-metadata v7.1.0
        │   └── zipp v3.18.1
        ├── keyring v25.0.0
        │   ├── jaraco-classes v3.3.1
        │   │   └── more-itertools v10.2.0
        │   ├── jaraco-functools v4.0.0
        │   │   └── more-itertools v10.2.0
        │   └── jaraco-context v4.3.0
        ├── rfc3986 v2.0.0
        └── rich v13.7.1
            ├── markdown-it-py v3.0.0
            │   └── mdurl v0.1.2
            └── pygments v2.17.2
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

// Ensure `pip tree` behaves correctly with a package that has a cyclic dependency.
// package `uv-cyclic-dependencies-a` and `uv-cyclic-dependencies-b` depend on each other,
// which creates a dependency cycle.
// Additionally, package `uv-cyclic-dependencies-c` is included (depends on `uv-cyclic-dependencies-a`)
// to make this test case more realistic and meaningful.
#[test]
fn cyclic_dependency() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("uv-cyclic-dependencies-c")
        .unwrap();

    let mut command = context.pip_install();
    command.env_remove("UV_EXCLUDE_NEWER");
    command
        .arg("-r")
        .arg("requirements.txt")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/");

    uv_snapshot!(context.filters(), command, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + uv-cyclic-dependencies-a==0.1.0
     + uv-cyclic-dependencies-b==0.1.0
     + uv-cyclic-dependencies-c==0.1.0
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    uv-cyclic-dependencies-c v0.1.0
    └── uv-cyclic-dependencies-a v0.1.0
        └── uv-cyclic-dependencies-b v0.1.0
            └── uv-cyclic-dependencies-a v0.1.0 (*)
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

// Ensure `pip tree` behaves correctly after a package has been removed.
#[test]
fn removed_dependency() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    uv_snapshot!(context.filters(), context
        .pip_uninstall()
        .arg("requests"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - requests==2.31.0
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    certifi v2024.2.2
    charset-normalizer v3.3.2
    idna v3.6
    urllib3 v2.2.1

    ----- stderr -----
    "###
    );
}

#[test]
fn multiple_packages() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        requests==2.31.0
        click==8.1.7
    ",
        )
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + click==8.1.7
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    let mut filters = context.filters();
    if cfg!(windows) {
        filters.push(("└── colorama v0.4.6\n", ""));
    }
    context.assert_command("import requests").success();
    uv_snapshot!(filters, context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    click v8.1.7
    requests v2.31.0
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    ├── urllib3 v2.2.1
    └── certifi v2024.2.2

    ----- stderr -----
    "###
    );
}

// Both `pendulum` and `boto3` depend on `python-dateutil`.
#[test]
#[cfg(not(windows))]
fn multiple_packages_shared_descendant() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        pendulum==3.0.0
        boto3==1.34.69
    ",
        )
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 10 packages in [TIME]
    Installed 10 packages in [TIME]
     + boto3==1.34.69
     + botocore==1.34.69
     + jmespath==1.0.1
     + pendulum==3.0.0
     + python-dateutil==2.9.0.post0
     + s3transfer==0.10.1
     + six==1.16.0
     + time-machine==2.14.1
     + tzdata==2024.1
     + urllib3==2.2.1

    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    boto3 v1.34.69
    ├── botocore v1.34.69
    │   ├── jmespath v1.0.1
    │   ├── python-dateutil v2.9.0.post0
    │   │   └── six v1.16.0
    │   └── urllib3 v2.2.1
    ├── jmespath v1.0.1
    └── s3transfer v0.10.1
        └── botocore v1.34.69 (*)
    pendulum v3.0.0
    ├── python-dateutil v2.9.0.post0 (*)
    ├── tzdata v2024.1
    └── time-machine v2.14.1
        └── python-dateutil v2.9.0.post0 (*)
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

// Test the interaction between `--no-dedupe` and `--invert`.
#[test]
#[cfg(not(windows))]
fn no_dedupe_and_invert() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        pendulum==3.0.0
        boto3==1.34.69
    ",
        )
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 10 packages in [TIME]
    Installed 10 packages in [TIME]
     + boto3==1.34.69
     + botocore==1.34.69
     + jmespath==1.0.1
     + pendulum==3.0.0
     + python-dateutil==2.9.0.post0
     + s3transfer==0.10.1
     + six==1.16.0
     + time-machine==2.14.1
     + tzdata==2024.1
     + urllib3==2.2.1

    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--no-dedupe").arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    jmespath v1.0.1
    ├── boto3 v1.34.69
    └── botocore v1.34.69
        ├── boto3 v1.34.69
        └── s3transfer v0.10.1
            └── boto3 v1.34.69
    six v1.16.0
    └── python-dateutil v2.9.0.post0
        ├── botocore v1.34.69
        │   ├── boto3 v1.34.69
        │   └── s3transfer v0.10.1
        │       └── boto3 v1.34.69
        ├── pendulum v3.0.0
        └── time-machine v2.14.1
            └── pendulum v3.0.0
    tzdata v2024.1
    └── pendulum v3.0.0
    urllib3 v2.2.1
    └── botocore v1.34.69
        ├── boto3 v1.34.69
        └── s3transfer v0.10.1
            └── boto3 v1.34.69

    ----- stderr -----
    "###
    );
}

// Ensure that --no-dedupe behaves as expected
// in the presence of dependency cycles.
#[test]
#[cfg(not(windows))]
fn no_dedupe_and_cycle() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        pendulum==3.0.0
        boto3==1.34.69
    ",
        )
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 10 packages in [TIME]
    Installed 10 packages in [TIME]
     + boto3==1.34.69
     + botocore==1.34.69
     + jmespath==1.0.1
     + pendulum==3.0.0
     + python-dateutil==2.9.0.post0
     + s3transfer==0.10.1
     + six==1.16.0
     + time-machine==2.14.1
     + tzdata==2024.1
     + urllib3==2.2.1

    "###
    );

    let mut command = context.pip_install();
    command.env_remove("UV_EXCLUDE_NEWER");
    command
        .arg("uv-cyclic-dependencies-c==0.1.0")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/");

    uv_snapshot!(context.filters(), command, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + uv-cyclic-dependencies-a==0.1.0
     + uv-cyclic-dependencies-b==0.1.0
     + uv-cyclic-dependencies-c==0.1.0
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree()
        .arg("--no-dedupe"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    boto3 v1.34.69
    ├── botocore v1.34.69
    │   ├── jmespath v1.0.1
    │   ├── python-dateutil v2.9.0.post0
    │   │   └── six v1.16.0
    │   └── urllib3 v2.2.1
    ├── jmespath v1.0.1
    └── s3transfer v0.10.1
        └── botocore v1.34.69
            ├── jmespath v1.0.1
            ├── python-dateutil v2.9.0.post0
            │   └── six v1.16.0
            └── urllib3 v2.2.1
    pendulum v3.0.0
    ├── python-dateutil v2.9.0.post0
    │   └── six v1.16.0
    ├── tzdata v2024.1
    └── time-machine v2.14.1
        └── python-dateutil v2.9.0.post0
            └── six v1.16.0
    uv-cyclic-dependencies-c v0.1.0
    └── uv-cyclic-dependencies-a v0.1.0
        └── uv-cyclic-dependencies-b v0.1.0
            └── uv-cyclic-dependencies-a v0.1.0 (*)
    (*) Package tree is a cycle and cannot be shown

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(not(windows))]
fn no_dedupe() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        pendulum==3.0.0
        boto3==1.34.69
    ",
        )
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 10 packages in [TIME]
    Installed 10 packages in [TIME]
     + boto3==1.34.69
     + botocore==1.34.69
     + jmespath==1.0.1
     + pendulum==3.0.0
     + python-dateutil==2.9.0.post0
     + s3transfer==0.10.1
     + six==1.16.0
     + time-machine==2.14.1
     + tzdata==2024.1
     + urllib3==2.2.1

    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree()
        .arg("--no-dedupe"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    boto3 v1.34.69
    ├── botocore v1.34.69
    │   ├── jmespath v1.0.1
    │   ├── python-dateutil v2.9.0.post0
    │   │   └── six v1.16.0
    │   └── urllib3 v2.2.1
    ├── jmespath v1.0.1
    └── s3transfer v0.10.1
        └── botocore v1.34.69
            ├── jmespath v1.0.1
            ├── python-dateutil v2.9.0.post0
            │   └── six v1.16.0
            └── urllib3 v2.2.1
    pendulum v3.0.0
    ├── python-dateutil v2.9.0.post0
    │   └── six v1.16.0
    ├── tzdata v2024.1
    └── time-machine v2.14.1
        └── python-dateutil v2.9.0.post0
            └── six v1.16.0

    ----- stderr -----
    "###
    );
}

#[test]
fn with_editable() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/hatchling_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + hatchling-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/hatchling_editable)
     + iniconfig==2.0.1.dev6+g9cae431 (from git+https://github.com/pytest-dev/iniconfig@9cae43103df70bac6fde7b9f35ad11a9f1be0cb4)
    "###
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hatchling-editable v0.1.0
    └── iniconfig v2.0.1.dev6+g9cae431

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(target_os = "macos")]
fn package_flag_complex() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("packse").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 32 packages in [TIME]
    Prepared 32 packages in [TIME]
    Installed 32 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + chevron-blue==0.2.1
     + docutils==0.20.1
     + hatchling==1.22.4
     + idna==3.6
     + importlib-metadata==7.1.0
     + jaraco-classes==3.3.1
     + jaraco-context==4.3.0
     + jaraco-functools==4.0.0
     + keyring==25.0.0
     + markdown-it-py==3.0.0
     + mdurl==0.1.2
     + more-itertools==10.2.0
     + msgspec==0.18.6
     + nh3==0.2.15
     + packaging==24.0
     + packse==0.3.12
     + pathspec==0.12.1
     + pkginfo==1.10.0
     + pluggy==1.4.0
     + pygments==2.17.2
     + readme-renderer==43.0
     + requests==2.31.0
     + requests-toolbelt==1.0.0
     + rfc3986==2.0.0
     + rich==13.7.1
     + setuptools==69.2.0
     + trove-classifiers==2024.3.3
     + twine==4.0.2
     + urllib3==2.2.1
     + zipp==3.18.1
    "###
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--package")
        .arg("hatchling")
        .arg("--package")
        .arg("keyring"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hatchling v1.22.4
    ├── packaging v24.0
    ├── pathspec v0.12.1
    ├── pluggy v1.4.0
    └── trove-classifiers v2024.3.3

    keyring v25.0.0
    ├── jaraco-classes v3.3.1
    │   └── more-itertools v10.2.0
    ├── jaraco-functools v4.0.0
    │   └── more-itertools v10.2.0
    └── jaraco-context v4.3.0

    ----- stderr -----
    "###
    );
}

#[test]
fn package_flag() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("scikit-learn==1.4.1.post1")
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + joblib==1.3.2
     + numpy==1.26.4
     + scikit-learn==1.4.1.post1
     + scipy==1.12.0
     + threadpoolctl==3.4.0
    "###
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--package")
        .arg("numpy"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    numpy v1.26.4

    ----- stderr -----
    "###
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--package")
        .arg("scipy")
        .arg("--package")
        .arg("joblib"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scipy v1.12.0
    └── numpy v1.26.4

    joblib v1.3.2

    ----- stderr -----
    "###
    );
}

#[test]
fn show_version_specifiers_simple() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--show-version-specifiers"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    ├── charset-normalizer v3.3.2 [required: <4, >=2]
    ├── idna v3.6 [required: <4, >=2.5]
    ├── urllib3 v2.2.1 [required: <3, >=1.21.1]
    └── certifi v2024.2.2 [required: >=2017.4.17]

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(target_os = "macos")]
fn show_version_specifiers_complex() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("packse").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 32 packages in [TIME]
    Prepared 32 packages in [TIME]
    Installed 32 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + chevron-blue==0.2.1
     + docutils==0.20.1
     + hatchling==1.22.4
     + idna==3.6
     + importlib-metadata==7.1.0
     + jaraco-classes==3.3.1
     + jaraco-context==4.3.0
     + jaraco-functools==4.0.0
     + keyring==25.0.0
     + markdown-it-py==3.0.0
     + mdurl==0.1.2
     + more-itertools==10.2.0
     + msgspec==0.18.6
     + nh3==0.2.15
     + packaging==24.0
     + packse==0.3.12
     + pathspec==0.12.1
     + pkginfo==1.10.0
     + pluggy==1.4.0
     + pygments==2.17.2
     + readme-renderer==43.0
     + requests==2.31.0
     + requests-toolbelt==1.0.0
     + rfc3986==2.0.0
     + rich==13.7.1
     + setuptools==69.2.0
     + trove-classifiers==2024.3.3
     + twine==4.0.2
     + urllib3==2.2.1
     + zipp==3.18.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--show-version-specifiers"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    packse v0.3.12
    ├── chevron-blue v0.2.1 [required: >=0.2.1, <0.3.0]
    ├── hatchling v1.22.4 [required: >=1.20.0, <2.0.0]
    │   ├── packaging v24.0 [required: >=21.3]
    │   ├── pathspec v0.12.1 [required: >=0.10.1]
    │   ├── pluggy v1.4.0 [required: >=1.0.0]
    │   └── trove-classifiers v2024.3.3 [required: *]
    ├── msgspec v0.18.6 [required: >=0.18.4, <0.19.0]
    ├── setuptools v69.2.0 [required: >=69.1.1, <70.0.0]
    └── twine v4.0.2 [required: >=4.0.2, <5.0.0]
        ├── pkginfo v1.10.0 [required: >=1.8.1]
        ├── readme-renderer v43.0 [required: >=35.0]
        │   ├── nh3 v0.2.15 [required: >=0.2.14]
        │   ├── docutils v0.20.1 [required: >=0.13.1]
        │   └── pygments v2.17.2 [required: >=2.5.1]
        ├── requests v2.31.0 [required: >=2.20]
        │   ├── charset-normalizer v3.3.2 [required: <4, >=2]
        │   ├── idna v3.6 [required: <4, >=2.5]
        │   ├── urllib3 v2.2.1 [required: <3, >=1.21.1]
        │   └── certifi v2024.2.2 [required: >=2017.4.17]
        ├── requests-toolbelt v1.0.0 [required: !=0.9.0, >=0.8.0]
        │   └── requests v2.31.0 [required: <3.0.0, >=2.0.1] (*)
        ├── urllib3 v2.2.1 [required: >=1.26.0]
        ├── importlib-metadata v7.1.0 [required: >=3.6]
        │   └── zipp v3.18.1 [required: >=0.5]
        ├── keyring v25.0.0 [required: >=15.1]
        │   ├── jaraco-classes v3.3.1 [required: *]
        │   │   └── more-itertools v10.2.0 [required: *]
        │   ├── jaraco-functools v4.0.0 [required: *]
        │   │   └── more-itertools v10.2.0 [required: *]
        │   └── jaraco-context v4.3.0 [required: *]
        ├── rfc3986 v2.0.0 [required: >=1.4.0]
        └── rich v13.7.1 [required: >=12.0.0]
            ├── markdown-it-py v3.0.0 [required: >=2.2.0]
            │   └── mdurl v0.1.2 [required: ~=0.1]
            └── pygments v2.17.2 [required: >=2.13.0, <3.0.0]
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

#[test]
fn show_version_specifiers_with_invert() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("scikit-learn==1.4.1.post1")
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + joblib==1.3.2
     + numpy==1.26.4
     + scikit-learn==1.4.1.post1
     + scipy==1.12.0
     + threadpoolctl==3.4.0
    "###
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--show-version-specifiers")
        .arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    joblib v1.3.2
    └── scikit-learn v1.4.1.post1 [requires: joblib >=1.2.0]
    numpy v1.26.4
    ├── scikit-learn v1.4.1.post1 [requires: numpy <2.0, >=1.19.5]
    └── scipy v1.12.0 [requires: numpy <1.29.0, >=1.22.4]
        └── scikit-learn v1.4.1.post1 [requires: scipy >=1.6.0]
    threadpoolctl v3.4.0
    └── scikit-learn v1.4.1.post1 [requires: threadpoolctl >=2.0.0]

    ----- stderr -----
    "###
    );
}

#[test]
fn show_version_specifiers_with_package() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("scikit-learn==1.4.1.post1")
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + joblib==1.3.2
     + numpy==1.26.4
     + scikit-learn==1.4.1.post1
     + scipy==1.12.0
     + threadpoolctl==3.4.0
    "###
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--show-version-specifiers")
        .arg("--package")
        .arg("scipy"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scipy v1.12.0
    └── numpy v1.26.4 [required: <1.29.0, >=1.22.4]

    ----- stderr -----
    "###
    );
}
