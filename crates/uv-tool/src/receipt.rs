use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Tool;

/// A `uv-receipt.toml` file tracking the installation of a tool.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

// Ignore raw document in comparison.
impl PartialEq for ToolReceipt {
    fn eq(&self, other: &Self) -> bool {
        self.tool.eq(&other.tool)
    }
}

impl Eq for ToolReceipt {}

impl From<Tool> for ToolReceipt {
    fn from(tool: Tool) -> Self {
        ToolReceipt {
            tool,
            raw: String::new(),
        }
    }
}
