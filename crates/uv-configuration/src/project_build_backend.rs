/// Available project build backends for use in `pyproject.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ProjectBuildBackend {
    #[cfg_attr(feature = "clap", value(hide = true))]
    #[cfg_attr(feature = "schemars", value(hide = true))]
    /// Use uv as the project build backend.
    Uv,
    #[default]
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
    /// Use [setuptools](https://pypi.org/project/setuptools) as the project build backend.
    Setuptools,
    /// Use [maturin](https://pypi.org/project/maturin) as the project build backend.
    Maturin,
    /// Use [scikit-build-core](https://pypi.org/project/scikit-build-core) as the project build backend.
    #[serde(alias = "scikit-build-core")]
    #[cfg_attr(feature = "clap", value(alias = "scikit-build-core"))]
    Scikit,
}
