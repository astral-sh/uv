#[cfg(feature = "clap")]
use clap::ValueEnum;

/// Available project build backends for use in `pyproject.toml`.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ProjectBuildBackend {
    #[cfg_attr(feature = "clap", value(alias = "uv-build", alias = "uv_build"))]
    /// Use uv as the project build backend.
    Uv,
    #[serde(alias = "hatchling")]
    #[cfg_attr(feature = "clap", value(alias = "hatchling"))]
    /// Use [hatchling](https://pypi.org/project/hatchling) as the project build backend.
    Hatch,
    /// Use [flit-core](https://pypi.org/project/flit-core) as the project build backend.
    #[serde(alias = "flit-core")]
    #[cfg_attr(feature = "clap", value(alias = "flit-core"))]
    Flit,
    /// Use [pdm-backend](https://pypi.org/project/pdm-backend) as the project build backend.
    #[serde(alias = "pdm-backend")]
    #[cfg_attr(feature = "clap", value(alias = "pdm-backend"))]
    PDM,
    /// Use [poetry-core](https://pypi.org/project/poetry-core) as the project build backend.
    #[serde(alias = "poetry-core")]
    #[cfg_attr(feature = "clap", value(alias = "poetry-core", alias = "poetry_core"))]
    Poetry,
    /// Use [setuptools](https://pypi.org/project/setuptools) as the project build backend.
    Setuptools,
    /// Use [maturin](https://pypi.org/project/maturin) as the project build backend.
    Maturin,
    /// Use [scikit-build-core](https://pypi.org/project/scikit-build-core) as the project build backend.
    #[serde(alias = "scikit-build-core")]
    #[cfg_attr(feature = "clap", value(alias = "scikit-build-core"))]
    Scikit,
}

#[cfg(feature = "clap")]
impl std::str::FromStr for ProjectBuildBackend {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        <Self as ValueEnum>::from_str(value, false).map_err(|_| {
            let choices = Self::value_variants()
                .iter()
                .filter_map(ValueEnum::to_possible_value)
                .map(|value| value.get_name().to_owned())
                .collect::<Vec<_>>();
            format!("expected one of: {}", choices.join(", "))
        })
    }
}
