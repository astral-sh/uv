use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use rkyv::rancor::{Fallible, OptionExt, Source};
use rkyv::ser::Writer;
use rkyv::string::{ArchivedString, StringResolver};
use rkyv::with::{ArchiveWith, DeserializeWith, SerializeWith};

#[derive(Debug)]
struct InvalidUtf8Path;

impl fmt::Display for InvalidUtf8Path {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("path is not valid UTF-8")
    }
}

impl Error for InvalidUtf8Path {}

pub(crate) struct BoxPathAsString;

impl ArchiveWith<Box<Path>> for BoxPathAsString {
    type Archived = ArchivedString;
    type Resolver = StringResolver;

    fn resolve_with(field: &Box<Path>, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        ArchivedString::resolve_from_str(&field.to_string_lossy(), resolver, out);
    }
}

impl<S> SerializeWith<Box<Path>, S> for BoxPathAsString
where
    S: Fallible + Writer + ?Sized,
    S::Error: Source,
{
    fn serialize_with(field: &Box<Path>, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        ArchivedString::serialize_from_str(field.to_str().into_trace(InvalidUtf8Path)?, serializer)
    }
}

impl<D> DeserializeWith<ArchivedString, Box<Path>, D> for BoxPathAsString
where
    D: Fallible + ?Sized,
{
    fn deserialize_with(field: &ArchivedString, _: &mut D) -> Result<Box<Path>, D::Error> {
        Ok(PathBuf::from(field.as_str()).into_boxed_path())
    }
}
