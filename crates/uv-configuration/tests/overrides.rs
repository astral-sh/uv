use std::str::FromStr;

use anyhow::Result;

use uv_configuration::{DependencyModifierScope, DependencyModifiers, Excludes, Overrides};
use uv_distribution_types::Requirement;
use uv_normalize::ExtraName;
use uv_pep508::Requirement as Pep508Requirement;
use uv_pypi_types::VerbatimParsedUrl;

/// An override replaces a version requirement without activating unrequested optional extras.
#[test]
fn overrides_preserve_disjunctive_optional_extras() -> Result<()> {
    let dependency = Requirement::from(Pep508Requirement::<VerbatimParsedUrl>::from_str(
        "leaf==1; extra == 'a' or extra == 'b'",
    )?);
    let replacement =
        Requirement::from(Pep508Requirement::<VerbatimParsedUrl>::from_str("leaf==2")?);
    let modifiers = DependencyModifiers::new(
        Overrides::from_requirements(vec![replacement]),
        Excludes::default(),
    );
    let effective = modifiers
        .apply(DependencyModifierScope::Global, [&dependency])
        .collect::<Vec<_>>();
    assert_eq!(effective.len(), 1);
    let mut activation = Vec::new();
    for extra in [None, Some("a"), Some("b"), Some("unrelated")] {
        let extras: Vec<ExtraName> = extra.map(str::parse).transpose()?.into_iter().collect();
        activation.push(effective[0].evaluate_markers(None, &extras));
    }
    assert_eq!(activation, [false, true, true, false]);
    insta::assert_snapshot!(effective[0].to_string(), @"leaf==2 ; extra == 'a' or extra == 'b'");
    Ok(())
}
