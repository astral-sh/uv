// macro pass-through
pub use anyhow_original;
use std::convert::Infallible;

#[macro_export]
macro_rules! anyhow {
    ($($args:tt)*) => {
        $crate::Error::Anyhow($crate::anyhow_original::anyhow!($($args)*))
    };
}

#[macro_export]
macro_rules! format_err {
    ($($args:tt)*) => {
        $crate::Error::Anyhow($crate::anyhow_original::format_err!($($args)*))
    };
}

#[macro_export]
macro_rules! bail {
    ($msg:literal $(,)?) => {
        return $crate::anyhow_original::__private::Err($crate::anyhow!($msg))
    };
    ($err:expr $(,)?) => {
        return $crate::anyhow_original::__private::Err($crate::anyhow!($err))
    };
    ($fmt:expr, $($arg:tt)*) => {
        return $crate::anyhow_original::__private::Err($crate::anyhow!($fmt, $($arg)*))
    };
}

/// Copied almost verbatim from anyhow
pub trait Context<T, E> {
    /// Wrap the error value with additional context.
    fn context<C>(self, context: C) -> anyhow_original::Result<T, Error>
    where
        C: std::fmt::Display + Send + Sync + 'static;

    /// Wrap the error value with additional context that is evaluated lazily
    /// only once an error does occur.
    fn with_context<C, F>(self, f: F) -> anyhow_original::Result<T, Error>
    where
        C: std::fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T, E> Context<T, E> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context<C>(self, context: C) -> std::result::Result<T, Error>
    where
        C: std::fmt::Display + Send + Sync + 'static,
    {
        Ok(anyhow_original::Context::context(self, context)?)
    }

    fn with_context<C, F>(self, context: F) -> std::result::Result<T, Error>
    where
        C: std::fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        Ok(anyhow_original::Context::with_context(self, context)?)
    }
}

impl<T> Context<T, Infallible> for Option<T> {
    fn context<C>(self, context: C) -> std::result::Result<T, Error>
    where
        C: std::fmt::Display + Send + Sync + 'static,
    {
        Ok(anyhow_original::Context::context(self, context)?)
    }

    fn with_context<C, F>(self, context: F) -> std::result::Result<T, Error>
    where
        C: std::fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        Ok(anyhow_original::Context::with_context(self, context)?)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub trait TraversableError: std::error::Error + Send + Sync + 'static {
    fn name(&self) -> &str;
}

pub enum Error {
    Traversable(Box<dyn TraversableError>),
    Anyhow(anyhow_original::Error),
}

impl Error {
    pub fn new<E>(error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Anyhow(anyhow_original::Error::new(error))
    }

    pub fn chain(&self) -> anyhow_original::Chain {
        match self {
            Error::Traversable(err) => anyhow_original::Chain::new(err.as_ref()),
            Error::Anyhow(err) => err.chain(),
        }
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Traversable(err) => std::fmt::Debug::fmt(err, f),
            Self::Anyhow(err) => std::fmt::Debug::fmt(err, f),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Traversable(err) => std::fmt::Display::fmt(err, f),
            Self::Anyhow(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Traversable(err) => Some(err.as_ref()),
            Self::Anyhow(err) => Some(err.as_ref()),
        }
    }
}

impl<E: TraversableError> From<E> for Error {
    fn from(error: E) -> Self {
        Self::Traversable(Box::new(error))
    }
}

impl<E: TraversableError> TraversableError for Box<E> {
    fn name(&self) -> &str {
        (&**self).name()
    }
}

impl TraversableError for std::fmt::Error {
    fn name(&self) -> &str {
        "std::fmt::Error"
    }
}

impl TraversableError for std::io::Error {
    fn name(&self) -> &str {
        "std::io::Error"
    }
}

// TODO(konsti): Where are the matching std and anyhow methods?
// anyhow supports `GIT.as_ref()?` but our error doesn't.
impl<E: TraversableError> TraversableError for &'static E {
    fn name(&self) -> &str {
        TraversableError::name(*self)
    }
}

impl From<anyhow_original::Error> for Error {
    fn from(error: anyhow_original::Error) -> Self {
        Self::Anyhow(error)
    }
}
