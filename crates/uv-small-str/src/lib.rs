use std::cmp::PartialEq;
use std::ops::Deref;

/// An optimized type for immutable identifiers. Represented as an [`arcstr::ArcStr`] internally.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SmallString(arcstr::ArcStr);

impl From<arcstr::ArcStr> for SmallString {
    #[inline]
    fn from(s: arcstr::ArcStr) -> Self {
        Self(s)
    }
}

impl From<&str> for SmallString {
    #[inline]
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<String> for SmallString {
    #[inline]
    fn from(s: String) -> Self {
        Self(s.into())
    }
}

impl AsRef<str> for SmallString {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl core::borrow::Borrow<str> for SmallString {
    #[inline]
    fn borrow(&self) -> &str {
        self
    }
}

impl Deref for SmallString {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::fmt::Debug for SmallString {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.0, f)
    }
}

impl core::fmt::Display for SmallString {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.0, f)
    }
}

/// A [`serde::Serialize`] implementation for [`SmallString`].
impl serde::Serialize for SmallString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

/// An [`rkyv`] implementation for [`SmallString`].
impl rkyv::Archive for SmallString {
    type Archived = rkyv::string::ArchivedString;
    type Resolver = rkyv::string::StringResolver;

    #[inline]
    fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        rkyv::string::ArchivedString::resolve_from_str(&self.0, resolver, out);
    }
}

impl<S> rkyv::Serialize<S> for SmallString
where
    S: rkyv::rancor::Fallible + rkyv::ser::Allocator + rkyv::ser::Writer + ?Sized,
    S::Error: rkyv::rancor::Source,
{
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        rkyv::string::ArchivedString::serialize_from_str(&self.0, serializer)
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized> rkyv::Deserialize<SmallString, D>
    for rkyv::string::ArchivedString
{
    fn deserialize(&self, _deserializer: &mut D) -> Result<SmallString, D::Error> {
        Ok(SmallString::from(self.as_str()))
    }
}

impl PartialEq<SmallString> for rkyv::string::ArchivedString {
    fn eq(&self, other: &SmallString) -> bool {
        **other == **self
    }
}

impl PartialOrd<SmallString> for rkyv::string::ArchivedString {
    fn partial_cmp(&self, other: &SmallString) -> Option<::core::cmp::Ordering> {
        Some(self.as_str().cmp(other))
    }
}

/// An [`schemars::JsonSchema`] implementation for [`SmallString`].
#[cfg(feature = "schemars")]
impl schemars::JsonSchema for SmallString {
    fn is_referenceable() -> bool {
        String::is_referenceable()
    }

    fn schema_name() -> String {
        String::schema_name()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        String::json_schema(_gen)
    }
}
