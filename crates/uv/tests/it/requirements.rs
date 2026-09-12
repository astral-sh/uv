use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;

use uv_client::BaseClientBuilder;
use uv_requirements::{RequirementsSource, RequirementsSpecification};

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
            constraints_txt.to_path_buf(),
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
