/// Available project build backends for use in `pyproject.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ProjectBuildBackend {
    #[default]
    /// Use [hatchling](https://pypi.org/project/hatchling) as the project build backend.
    Hatch,
    /// Use [flit-core](https://pypi.org/project/flit-core) as the project build backend.
    Flit,
    /// Use [pdm-backend](https://pypi.org/project/pdm-backend) as the project build backend.
    PDM,
    /// Use [setuptools](https://pypi.org/project/setuptools) as the project build backend.
    Setuptools,
    /// Use [maturin](https://pypi.org/project/maturin) as the project build backend.
    Maturin,
    /// Use [scikit-build-core](https://pypi.org/project/scikit-build-core) as the project build backend.
    Scikit,
}
