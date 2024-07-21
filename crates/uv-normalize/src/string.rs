use rkyv::{
    ser::{ScratchSpace, Serializer},
    string::{ArchivedString, StringResolver},
    Archive, Deserialize, Fallible, Serialize,
};
use std::borrow::Cow;

use ustr::Ustr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
pub struct InternedString(Ustr);

impl InternedString {
    pub fn as_str(&self) -> &'static str {
        self.0.as_str()
    }
}

impl From<&str> for InternedString {
    fn from(s: &str) -> Self {
        InternedString(Ustr::from(s))
    }
}

impl From<String> for InternedString {
    fn from(s: String) -> Self {
        InternedString(Ustr::from(s.as_str()))
    }
}

impl From<Cow<'_, str>> for InternedString {
    fn from(s: Cow<'_, str>) -> Self {
        InternedString(Ustr::from(s.as_ref()))
    }
}

impl AsRef<str> for InternedString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for InternedString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl std::fmt::Display for InternedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl Archive for InternedString {
    type Archived = ArchivedString;
    type Resolver = StringResolver;

    #[inline]
    #[allow(unsafe_code)]
    unsafe fn resolve(&self, pos: usize, resolver: Self::Resolver, out: *mut Self::Archived) {
        ArchivedString::resolve_from_str(self.0.as_str(), pos, resolver, out);
    }
}

impl<S: ScratchSpace + Serializer + ?Sized> Serialize<S> for InternedString {
    #[inline]
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        ArchivedString::serialize_from_str(self.0.as_str(), serializer)
    }
}

impl<D: Fallible + ?Sized> Deserialize<InternedString, D> for ArchivedString {
    #[inline]
    fn deserialize(&self, _deserializer: &mut D) -> Result<InternedString, D::Error> {
        Ok(InternedString::from(self.as_str()))
    }
}

impl PartialEq<InternedString> for ArchivedString {
    fn eq(&self, other: &InternedString) -> bool {
        other.as_str() == self.as_str()
    }
}
