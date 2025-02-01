use core::{error::Error, str::FromStr};

use rkyv::{
    rancor::{Fallible, ResultExt, Source},
    ser::Writer,
    string::{ArchivedString, StringResolver},
    with::{ArchiveWith, DeserializeWith, SerializeWith},
    Archive,
};

/// A "with type" to serialize and deserialize any type that implements `AsRef<str>` and `FromStr`,
/// for use with `rkyv`.
pub struct AsStr;

impl<T> ArchiveWith<T> for AsStr
where
    T: AsRef<str>,
{
    type Archived = ArchivedString;
    type Resolver = StringResolver;

    fn resolve_with(field: &T, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        ArchivedString::resolve_from_str(field.as_ref(), resolver, out);
    }
}

impl<T, S> SerializeWith<T, S> for AsStr
where
    T: AsRef<str>,
    S: Fallible + Writer + ?Sized,
    S::Error: Source,
{
    fn serialize_with(
        field: &T,
        serializer: &mut S,
    ) -> Result<Self::Resolver, <S as Fallible>::Error> {
        ArchivedString::serialize_from_str(field.as_ref(), serializer)
    }
}

impl<T, D> DeserializeWith<ArchivedString, T, D> for AsStr
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
    D: Fallible + ?Sized,
    D::Error: Source,
{
    fn deserialize_with(field: &ArchivedString, _: &mut D) -> Result<T, <D as Fallible>::Error> {
        Ok(T::from_str(field.as_str()).into_error()?)
    }
}
