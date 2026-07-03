#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PrereleaseMode {
    /// Disallow all pre-release versions.
    Disallow,

    /// Allow all pre-release versions.
    Allow,

    /// Allow pre-release versions when no stable candidate satisfies the active constraints.
    IfNecessary,

    /// Allow pre-release versions when no stable candidate satisfies the active constraints, or
    /// when an active direct or transitive requirement contains an explicit pre-release marker.
    #[default]
    #[serde(alias = "explicit")]
    #[cfg_attr(feature = "clap", value(alias("explicit")))]
    IfNecessaryOrExplicit,
}

impl std::fmt::Display for PrereleaseMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disallow => write!(formatter, "disallow"),
            Self::Allow => write!(formatter, "allow"),
            Self::IfNecessary => write!(formatter, "if-necessary"),
            Self::IfNecessaryOrExplicit => write!(formatter, "if-necessary-or-explicit"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "clap")]
    use clap::ValueEnum;

    use super::PrereleaseMode;

    #[test]
    fn explicit_is_a_legacy_alias() {
        let mode = serde_json::from_str::<PrereleaseMode>(r#""explicit""#)
            .expect("legacy value should parse");
        assert_eq!(mode, PrereleaseMode::IfNecessaryOrExplicit);

        #[cfg(feature = "clap")]
        assert_eq!(
            PrereleaseMode::from_str("explicit", false).expect("legacy CLI value should parse"),
            PrereleaseMode::IfNecessaryOrExplicit
        );
    }
}
