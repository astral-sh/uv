use std::str::FromStr;

use pep508_rs::Requirement;

#[derive(Debug)]
pub struct VerbatimRequirement<'a> {
    /// The name of the requirement as provided by the user.
    pub given_name: &'a str,
    /// The normalized requirement.
    pub requirement: Requirement,
}

impl<'a> TryFrom<&'a str> for VerbatimRequirement<'a> {
    type Error = pep508_rs::Pep508Error;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        let requirement = Requirement::from_str(s)?;
        Ok(Self {
            given_name: s,
            requirement,
        })
    }
}
