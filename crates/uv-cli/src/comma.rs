use std::str::FromStr;

/// A comma-separated string of requirements, e.g., `"flask,anyio"`, that takes extras into account
/// (i.e., treats `"psycopg[binary,pool]"` as a single requirement).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommaSeparatedRequirements(Vec<String>);

impl IntoIterator for CommaSeparatedRequirements {
    type Item = String;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromStr for CommaSeparatedRequirements {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        // Split on commas _outside_ of brackets.
        let mut requirements = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;
        for (i, c) in input.char_indices() {
            match c {
                '[' => {
                    depth = depth.saturating_add(1);
                }
                ']' => {
                    depth = depth.saturating_sub(1);
                }
                ',' if depth == 0 => {
                    // If the next character is a version identifier, skip the comma, as in:
                    // `requests>=2.1,<3`.
                    if let Some(c) = input
                        .get(i + ','.len_utf8()..)
                        .and_then(|s| s.chars().find(|c| !c.is_whitespace()))
                    {
                        if matches!(c, '!' | '=' | '<' | '>' | '~') {
                            continue;
                        }
                    }

                    let requirement = input[start..i].trim().to_string();
                    if !requirement.is_empty() {
                        requirements.push(requirement);
                    }
                    start = i + ','.len_utf8();
                }
                _ => {}
            }
        }
        let requirement = input[start..].trim().to_string();
        if !requirement.is_empty() {
            requirements.push(requirement);
        }
        Ok(Self(requirements))
    }
}

#[cfg(test)]
mod tests {
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
            CommaSeparatedRequirements(vec!["requests>=2.1,<3".to_string()])
        );
    }

    #[test]
    fn double_specifiers() {
        assert_eq!(
            CommaSeparatedRequirements::from_str("requests>=2.1,<3, flask").unwrap(),
            CommaSeparatedRequirements(vec!["requests>=2.1,<3".to_string(), "flask".to_string()])
        );
    }
}
