use std::path::Path;

use serde::Deserialize;

use crate::Tool;

/// A `uv-receipt.toml` file tracking the installation of a tool.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ToolReceipt {
    pub(crate) tool: Tool,

    /// The raw unserialized document.
    #[serde(skip)]
    pub(crate) raw: String,
}

impl ToolReceipt {
    /// Parse a [`ToolReceipt`] from a raw TOML string.
    pub(crate) fn from_string(raw: String) -> Result<Self, toml::de::Error> {
        let tool = toml::from_str(&raw)?;
        Ok(ToolReceipt { raw, ..tool })
    }

    ///  Read a [`ToolReceipt`] from the given path.
    pub(crate) fn from_path(path: &Path) -> Result<ToolReceipt, crate::Error> {
        match fs_err::read_to_string(path) {
            Ok(contents) => Ok(ToolReceipt::from_string(contents)
                .map_err(|err| crate::Error::ReceiptRead(path.to_owned(), Box::new(err)))?),
            Err(err) => Err(err.into()),
        }
    }

    /// Returns the TOML representation of this receipt.
    pub(crate) fn to_toml(&self) -> Result<String, toml_edit::ser::Error> {
        // We construct a TOML document manually instead of going through Serde to enable
        // the use of inline tables.
        let mut doc = toml_edit::DocumentMut::new();
        doc.insert("tool", toml_edit::Item::Table(self.tool.to_toml()?));

        Ok(doc.to_string())
    }
}

impl From<Tool> for ToolReceipt {
    fn from(tool: Tool) -> Self {
        ToolReceipt {
            tool,
            raw: String::new(),
        }
    }
}
