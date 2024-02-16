use etcetera::base_strategy::{BaseStrategy, Xdg};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use toml_edit::Document;

pub(crate) struct Settings {
    pub(crate) venv_name: PathBuf,
}

impl Settings {
    pub(crate) fn read() -> anyhow::Result<Self> {
        let strategy = Xdg::new()?;
        let config_dir = strategy.config_dir().join("uv");

        let config_file = config_dir.with_extension("toml");

        let content = read_file(&config_file);
        let document = content.parse::<Document>().unwrap_or(Document::new());

        let venv_name = PathBuf::from(
            document
                .get("venv-name")
                .and_then(|v| v.as_str())
                .unwrap_or(".venv")
                .to_string(),
        );

        Ok(Self { venv_name })
    }

    pub(crate) fn set_venv_name(&mut self, maybe_name: Option<PathBuf>) {
        if let Some(name) = maybe_name {
            self.venv_name = name;
        }
    }
}

fn read_file(path: &Path) -> String {
    if let Ok(file) = File::open(path) {
        let mut reader = BufReader::new(file);

        let mut contents = String::new();
        reader.read_to_string(&mut contents).unwrap_or(0);
        contents
    } else {
        String::new()
    }
}
