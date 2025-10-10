/// An encoded JWT access token.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct AccessToken(String);

impl AccessToken {
    /// Return the [`AccessToken`] as a vector of bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_bytes()
    }

    /// Return the [`AccessToken`] as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for AccessToken {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl AsRef<[u8]> for AccessToken {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl std::fmt::Display for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "****")
    }
}
