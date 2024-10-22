use super::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use std::iter;
use tempfile::TempDir;

fn extend_project(payload: &str) -> String {
    formatdoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"
            {payload}

            [build-system]
            requires = ["uv>=0.4.15,<5"]
            build-backend = "uv"
        "#
    }
}

fn format_err(err: impl std::error::Error) -> String {
    let mut formatted = err.to_string();
    for source in iter::successors(err.source(), |&err| err.source()) {
        formatted += &format!("\n  Caused by: {source}");
    }
    formatted
}

#[test]
fn valid() {
    let temp_dir = TempDir::new().unwrap();

    fs_err::write(
        temp_dir.path().join("Readme.md"),
        indoc! {r"
            # Foo

            This is the foo library.
        "},
    )
    .unwrap();

    fs_err::write(
        temp_dir.path().join("License.txt"),
        indoc! {r#"
                THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
                INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
                PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
                HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF
                CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE
                OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
        "#},
    )
    .unwrap();

    let contents = indoc! {r#"
            # See https://github.com/pypa/sampleproject/blob/main/pyproject.toml for another example

            [project]
            name = "hello-world"
            version = "0.1.0"
            description = "A Python package"
            readme = "Readme.md"
            requires_python = ">=3.12"
            license = { file = "License.txt" }
            authors = [{ name = "Ferris the crab", email = "ferris@rustacean.net" }]
            maintainers = [{ name = "Konsti", email = "konstin@mailbox.org" }]
            keywords = ["demo", "example", "package"]
            classifiers = [
                "Development Status :: 6 - Mature",
                "License :: OSI Approved :: MIT License",
                # https://github.com/pypa/trove-classifiers/issues/17
                "License :: OSI Approved :: Apache Software License",
                "Programming Language :: Python",
            ]
            dependencies = ["flask>=3,<4", "sqlalchemy[asyncio]>=2.0.35,<3"]
            # We don't support dynamic fields, the default empty array is the only allowed value.
            dynamic = []

            [project.optional-dependencies]
            postgres = ["psycopg>=3.2.2,<4"]
            mysql = ["pymysql>=1.1.1,<2"]

            [project.urls]
            "Homepage" = "https://github.com/astral-sh/uv"
            "Repository" = "https://astral.sh"

            [project.scripts]
            foo = "foo.cli:__main__"

            [project.gui-scripts]
            foo-gui = "foo.gui"

            [project.entry-points.bar_group]
            foo-bar = "foo:bar"

            [build-system]
            requires = ["uv>=0.4.15,<5"]
            build-backend = "uv"
        "#
    };

    let pyproject_toml = PyProjectToml::parse(contents).unwrap();
    let metadata = pyproject_toml.to_metadata(temp_dir.path()).unwrap();

    assert_snapshot!(metadata.core_metadata_format(), @r###"
        Metadata-Version: 2.3
        Name: hello-world
        Version: 0.1.0
        Summary: A Python package
        Keywords: demo,example,package
        Author: Ferris the crab
        License: THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
                 INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
                 PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
                 HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF
                 CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE
                 OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
        Classifier: Development Status :: 6 - Mature
        Classifier: License :: OSI Approved :: MIT License
        Classifier: License :: OSI Approved :: Apache Software License
        Classifier: Programming Language :: Python
        Requires-Dist: flask>=3,<4
        Requires-Dist: sqlalchemy[asyncio]>=2.0.35,<3
        Maintainer: Konsti
        Project-URL: Homepage, https://github.com/astral-sh/uv
        Project-URL: Repository, https://astral.sh
        Provides-Extra: mysql
        Provides-Extra: postgres
        Description-Content-Type: text/markdown

        # Foo

        This is the foo library.
        "###);

    assert_snapshot!(pyproject_toml.to_entry_points().unwrap().unwrap(), @r###"
        [console_scripts]
        foo = foo.cli:__main__

        [gui_scripts]
        foo-gui = foo.gui

        [bar_group]
        foo-bar = foo:bar

        "###);
}

#[test]
fn build_system_valid() {
    let contents = extend_project("");
    let pyproject_toml = PyProjectToml::parse(&contents).unwrap();
    assert!(pyproject_toml.check_build_system("1.0.0+test"));
}

#[test]
fn build_system_no_bound() {
    let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["uv"]
            build-backend = "uv"
        "#};
    let pyproject_toml = PyProjectToml::parse(contents).unwrap();
    assert!(!pyproject_toml.check_build_system("1.0.0+test"));
}

#[test]
fn build_system_multiple_packages() {
    let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["uv>=0.4.15,<5", "wheel"]
            build-backend = "uv"
        "#};
    let pyproject_toml = PyProjectToml::parse(contents).unwrap();
    assert!(!pyproject_toml.check_build_system("1.0.0+test"));
}

#[test]
fn build_system_no_requires_uv() {
    let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["setuptools"]
            build-backend = "uv"
        "#};
    let pyproject_toml = PyProjectToml::parse(contents).unwrap();
    assert!(!pyproject_toml.check_build_system("1.0.0+test"));
}

#[test]
fn build_system_not_uv() {
    let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["uv>=0.4.15,<5"]
            build-backend = "setuptools"
        "#};
    let pyproject_toml = PyProjectToml::parse(contents).unwrap();
    assert!(!pyproject_toml.check_build_system("1.0.0+test"));
}

#[test]
fn minimal() {
    let contents = extend_project("");

    let metadata = PyProjectToml::parse(&contents)
        .unwrap()
        .to_metadata(Path::new("/do/not/read"))
        .unwrap();

    assert_snapshot!(metadata.core_metadata_format(), @r###"
        Metadata-Version: 2.3
        Name: hello-world
        Version: 0.1.0
        "###);
}

#[test]
fn invalid_readme_spec() {
    let contents = extend_project(indoc! {r#"
            readme = { path = "Readme.md" }
        "#
    });

    let err = PyProjectToml::parse(&contents).unwrap_err();
    assert_snapshot!(format_err(err), @r###"
        Invalid pyproject.toml
          Caused by: TOML parse error at line 4, column 10
          |
        4 | readme = { path = "Readme.md" }
          |          ^^^^^^^^^^^^^^^^^^^^^^
        data did not match any variant of untagged enum Readme
        "###);
}

#[test]
fn missing_readme() {
    let contents = extend_project(indoc! {r#"
            readme = "Readme.md"
        "#
    });

    let err = PyProjectToml::parse(&contents)
        .unwrap()
        .to_metadata(Path::new("/do/not/read"))
        .unwrap_err();
    // Simplified for windows compatibility.
    assert_snapshot!(err.to_string().replace('\\', "/"), @"failed to open file `/do/not/read/Readme.md`");
}

#[test]
fn multiline_description() {
    let contents = extend_project(indoc! {r#"
            description = "Hi :)\nThis is my project"
        "#
    });

    let err = PyProjectToml::parse(&contents)
        .unwrap()
        .to_metadata(Path::new("/do/not/read"))
        .unwrap_err();
    assert_snapshot!(format_err(err), @r###"
        Invalid pyproject.toml
          Caused by: `project.description` must be a single line
        "###);
}

#[test]
fn mixed_licenses() {
    let contents = extend_project(indoc! {r#"
            license-files = ["licenses/*"]
            license =  { text = "MIT" }
        "#
    });

    let err = PyProjectToml::parse(&contents)
        .unwrap()
        .to_metadata(Path::new("/do/not/read"))
        .unwrap_err();
    assert_snapshot!(format_err(err), @r###"
        Invalid pyproject.toml
          Caused by: When `project.license-files` is defined, `project.license` must be an SPDX expression string
        "###);
}

#[test]
fn valid_license() {
    let contents = extend_project(indoc! {r#"
            license = "MIT OR Apache-2.0"
        "#
    });
    let metadata = PyProjectToml::parse(&contents)
        .unwrap()
        .to_metadata(Path::new("/do/not/read"))
        .unwrap();
    assert_snapshot!(metadata.core_metadata_format(), @r###"
        Metadata-Version: 2.4
        Name: hello-world
        Version: 0.1.0
        License-Expression: MIT OR Apache-2.0
        "###);
}

#[test]
fn invalid_license() {
    let contents = extend_project(indoc! {r#"
            license = "MIT XOR Apache-2"
        "#
    });
    let err = PyProjectToml::parse(&contents)
        .unwrap()
        .to_metadata(Path::new("/do/not/read"))
        .unwrap_err();
    // TODO(konsti): We mess up the indentation in the error.
    assert_snapshot!(format_err(err), @r###"
        Invalid pyproject.toml
          Caused by: `project.license` is not a valid SPDX expression: `MIT XOR Apache-2`
          Caused by: MIT XOR Apache-2
            ^^^ unknown term
        "###);
}

#[test]
fn dynamic() {
    let contents = extend_project(indoc! {r#"
            dynamic = ["dependencies"]
        "#
    });

    let err = PyProjectToml::parse(&contents)
        .unwrap()
        .to_metadata(Path::new("/do/not/read"))
        .unwrap_err();
    assert_snapshot!(format_err(err), @r###"
        Invalid pyproject.toml
          Caused by: Dynamic metadata is not supported
        "###);
}

fn script_error(contents: &str) -> String {
    let err = PyProjectToml::parse(contents)
        .unwrap()
        .to_entry_points()
        .unwrap_err();
    format_err(err)
}

#[test]
fn invalid_entry_point_group() {
    let contents = extend_project(indoc! {r#"
            [project.entry-points."a@b"]
            foo = "bar"
        "#
    });
    assert_snapshot!(script_error(&contents), @"Entrypoint groups must consist of letters and numbers separated by dots, invalid group: `a@b`");
}

#[test]
fn invalid_entry_point_name() {
    let contents = extend_project(indoc! {r#"
            [project.scripts]
            "a@b" = "bar"
        "#
    });
    assert_snapshot!(script_error(&contents), @"Entrypoint names must consist of letters, numbers, dots and dashes; invalid name: `a@b`");
}

#[test]
fn invalid_entry_point_conflict_scripts() {
    let contents = extend_project(indoc! {r#"
            [project.entry-points.console_scripts]
            foo = "bar"
        "#
    });
    assert_snapshot!(script_error(&contents), @"Use `project.scripts` instead of `project.entry-points.console_scripts`");
}

#[test]
fn invalid_entry_point_conflict_gui_scripts() {
    let contents = extend_project(indoc! {r#"
            [project.entry-points.gui_scripts]
            foo = "bar"
        "#
    });
    assert_snapshot!(script_error(&contents), @"Use `project.gui-scripts` instead of `project.entry-points.gui_scripts`");
}
