use arcstr::ArcStr;
use rkyv::rancor::{Fallible, Source};
use rkyv::ser::Writer;
use rkyv::string::{ArchivedString, StringResolver};
use rkyv::with::{ArchiveWith, DeserializeWith, SerializeWith};

pub(crate) struct ArcStrAsString;

impl ArchiveWith<ArcStr> for ArcStrAsString {
    type Archived = ArchivedString;
    type Resolver = StringResolver;

    fn resolve_with(field: &ArcStr, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        ArchivedString::resolve_from_str(field, resolver, out);
    }
}

impl<S> SerializeWith<ArcStr, S> for ArcStrAsString
where
    S: Fallible + Writer + ?Sized,
    S::Error: Source,
{
    fn serialize_with(field: &ArcStr, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        ArchivedString::serialize_from_str(field, serializer)
    }
}

impl<D> DeserializeWith<ArchivedString, ArcStr, D> for ArcStrAsString
where
    D: Fallible + ?Sized,
{
    fn deserialize_with(field: &ArchivedString, _: &mut D) -> Result<ArcStr, D::Error> {
        Ok(ArcStr::from(field.as_str()))
    }
}
