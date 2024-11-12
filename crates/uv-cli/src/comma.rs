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
mod tests;
