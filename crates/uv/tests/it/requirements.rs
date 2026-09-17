use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use std::path::Path;
use url::Url;

use uv_client::BaseClientBuilder;
use uv_configuration::RequirementsInput;
use uv_redacted::DisplaySafeUrl;
use uv_requirements::{RequirementsSource, RequirementsSpecification};

#[test]
fn parse_requirements_input() -> Result<()> {
    assert_eq!("-".parse::<RequirementsInput>()?, RequirementsInput::Stdin);
    assert_eq!(
        RequirementsInput::from(Path::new("-")),
        RequirementsInput::Stdin
    );

    let relative_path = "requirements.txt";
    assert_eq!(
        relative_path.parse::<RequirementsInput>()?,
        RequirementsInput::Local(relative_path.into())
    );

    for windows_path in [
        r"C:\Users\ferris\requirements.txt",
        "C:/Users/ferris/requirements.txt",
    ] {
        assert_eq!(
            windows_path.parse::<RequirementsInput>()?,
            RequirementsInput::Local(windows_path.into())
        );
    }

    let absolute_path = std::env::current_dir()?.join("requirements.txt");
    let file_url =
        Url::from_file_path(&absolute_path).expect("an absolute path should convert to a file URL");
    assert_eq!(
        file_url.as_str().parse::<RequirementsInput>()?,
        RequirementsInput::Local(absolute_path.clone())
    );

    for remote in [
        "https://example.com/requirements.txt",
        "ftp://example.com/requirements.txt",
    ] {
        assert_eq!(
            remote.parse::<RequirementsInput>()?,
            RequirementsInput::Remote(DisplaySafeUrl::parse(remote)?)
        );
    }

    let remote = "https://example.com/nested/requirements.txt".parse::<RequirementsInput>()?;
    assert_eq!(
        remote.resolve("child.txt", Path::new("unused"))?,
        RequirementsInput::Remote(DisplaySafeUrl::parse(
            "https://example.com/nested/child.txt"
        )?)
    );
    assert_eq!(
        remote.resolve("/child.txt", Path::new("unused"))?,
        RequirementsInput::Remote(DisplaySafeUrl::parse("https://example.com/child.txt")?)
    );
    assert_eq!(
        RequirementsInput::Local("requirements.txt".into())
            .resolve("child.txt", Path::new("project"))?,
        RequirementsInput::Local(Path::new("project").join("child.txt"))
    );
    assert_eq!(
        RequirementsInput::Local("requirements.txt".into())
            .resolve_local_path(Path::new("child.txt"), Path::new("project")),
        Some(Path::new("project").join("child.txt"))
    );
    assert_eq!(
        RequirementsInput::Stdin.resolve_local_path(Path::new("child.txt"), Path::new("project")),
        Some(Path::new("project").join("child.txt"))
    );
    assert_eq!(
        remote.resolve_local_path(Path::new("child.txt"), Path::new("unused")),
        None
    );
    assert_eq!(
        remote.resolve_local_path(&absolute_path, Path::new("unused")),
        None
    );

    Ok(())
}

#[tokio::test]
async fn constraint_specifications_preserve_hashes() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let constraints_txt = temp_dir.child("constraints.txt");
    constraints_txt.write_str(indoc! {r"
        packaging==23.2 \
            --hash=sha256:1111111111111111111111111111111111111111111111111111111111111111
        hatchling==1.20.0 \
            --hash=sha256:2222222222222222222222222222222222222222222222222222222222222222 \
            --hash=sha256:3333333333333333333333333333333333333333333333333333333333333333
    "})?;

    let specification = RequirementsSpecification::from_sources(
        &[],
        &[RequirementsSource::RequirementsTxt(
            constraints_txt.to_path_buf().into(),
        )],
        &[],
        &[],
        None,
        &BaseClientBuilder::default(),
    )
    .await?;

    insta::assert_debug_snapshot!(
        specification
            .constraints
            .iter()
            .map(|entry| (entry.requirement.to_string(), entry.hashes.as_slice()))
            .collect::<Vec<_>>(),
        @r#"
    [
        (
            "packaging==23.2",
            [
                "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            ],
        ),
        (
            "hatchling==1.20.0",
            [
                "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                "sha256:3333333333333333333333333333333333333333333333333333333333333333",
            ],
        ),
    ]
    "#
    );

    Ok(())
}
