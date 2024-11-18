use super::CommaSeparatedRequirements;
use std::str::FromStr;

#[test]
fn single() {
    assert_eq!(
        CommaSeparatedRequirements::from_str("flask").unwrap(),
        CommaSeparatedRequirements(vec!["flask".to_string()])
    );
}

#[test]
fn double() {
    assert_eq!(
        CommaSeparatedRequirements::from_str("flask,anyio").unwrap(),
        CommaSeparatedRequirements(vec!["flask".to_string(), "anyio".to_string()])
    );
}

#[test]
fn empty() {
    assert_eq!(
        CommaSeparatedRequirements::from_str("flask,,anyio").unwrap(),
        CommaSeparatedRequirements(vec!["flask".to_string(), "anyio".to_string()])
    );
}

#[test]
fn single_extras() {
    assert_eq!(
        CommaSeparatedRequirements::from_str("psycopg[binary,pool]").unwrap(),
        CommaSeparatedRequirements(vec!["psycopg[binary,pool]".to_string()])
    );
}

#[test]
fn double_extras() {
    assert_eq!(
        CommaSeparatedRequirements::from_str("psycopg[binary,pool], flask").unwrap(),
        CommaSeparatedRequirements(vec![
            "psycopg[binary,pool]".to_string(),
            "flask".to_string()
        ])
    );
}

#[test]
fn single_specifiers() {
    assert_eq!(
        CommaSeparatedRequirements::from_str("requests>=2.1,<3").unwrap(),
        CommaSeparatedRequirements(vec!["requests>=2.1,<3".to_string(),])
    );
}

#[test]
fn double_specifiers() {
    assert_eq!(
        CommaSeparatedRequirements::from_str("requests>=2.1,<3, flask").unwrap(),
        CommaSeparatedRequirements(vec!["requests>=2.1,<3".to_string(), "flask".to_string()])
    );
}
