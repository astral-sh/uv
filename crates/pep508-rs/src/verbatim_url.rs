use std::ops::Deref;
use url::Url;

/// A wrapper around [`Url`] that preserves the original string.
#[derive(Debug, Clone, Eq, derivative::Derivative)]
#[derivative(PartialEq, Hash)]
pub struct VerbatimUrl {
    /// The parsed URL.
    url: Url,
    /// The URL as it was provided by the user.
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    given: Option<String>,
}

impl VerbatimUrl {
    /// Parse a URL from a string.
    pub fn parse(given: String) -> Result<Self, Error> {
        let url = Url::parse(&given)?;
        Ok(Self {
            given: Some(given),
            url,
        })
    }

    /// Return the underlying [`Url`].
    pub fn raw(&self) -> &Url {
        &self.url
    }

    /// Convert a [`VerbatimUrl`] into a [`Url`].
    pub fn to_url(&self) -> Url {
        self.url.clone()
    }

    /// Create a [`VerbatimUrl`] from a [`Url`].
    ///
    /// This method should be used sparingly (ideally, not at all), as it represents a loss of the
    /// verbatim representation.
    pub fn unknown(url: Url) -> Self {
        Self { given: None, url }
    }
}

impl std::str::FromStr for VerbatimUrl {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s.to_owned())
    }
}

impl std::fmt::Display for VerbatimUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(given) = &self.given {
            given.fmt(f)
        } else {
            self.url.fmt(f)
        }
    }
}

impl Deref for VerbatimUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.url
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    #[error(transparent)]
    Url(#[from] url::ParseError),
}
