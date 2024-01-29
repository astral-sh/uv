/*!
Defines some helpers for use with `rkyv`.

Principally, we define our own implementation of the `Serializer` trait.
This involves a fair bit of boiler plate, but it was largely copied from
`CompositeSerializer`. (Indeed, our serializer wraps a `CompositeSerializer`.)

The motivation for doing this is to support the archiving of `PathBuf` types.
Namely, for reasons AG doesn't completely understand at the time of writing,
the serializers that rkyv bundled cannot handle the error returned by `PathBuf`
potentially failing to serialize. Namely, since `PathBuf` has a platform
dependent representation when its contents are not valid UTF-8, serialization
in `rkyv` requires that it be valid UTF-8. If it isn't, serialization will
fail.
*/

use std::convert::Infallible;

use rkyv::{
    ser::serializers::{
        AlignedSerializer, AllocScratch, AllocScratchError, AllocSerializer, CompositeSerializer,
        CompositeSerializerError, FallbackScratch, HeapScratch, SharedSerializeMap,
        SharedSerializeMapError,
    },
    util::AlignedVec,
    Archive, ArchiveUnsized, Fallible,
};

pub struct Serializer<const N: usize> {
    composite: CompositeSerializer<
        AlignedSerializer<AlignedVec>,
        FallbackScratch<HeapScratch<N>, AllocScratch>,
        SharedSerializeMap,
    >,
}

impl<const N: usize> Serializer<N> {
    pub fn new() -> Serializer<N> {
        let composite = AllocSerializer::<N>::default();
        Serializer { composite }
    }

    pub fn into_serializer(self) -> AlignedSerializer<AlignedVec> {
        self.composite.into_serializer()
    }
}

impl<const N: usize> Fallible for Serializer<N> {
    type Error = SerializerError;
}

impl<const N: usize> rkyv::ser::Serializer for Serializer<N> {
    #[inline]
    fn pos(&self) -> usize {
        self.composite.pos()
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.composite
            .write(bytes)
            .map_err(SerializerError::Composite)
    }

    #[inline]
    fn pad(&mut self, padding: usize) -> Result<(), Self::Error> {
        self.composite
            .pad(padding)
            .map_err(SerializerError::Composite)
    }

    #[inline]
    fn align(&mut self, align: usize) -> Result<usize, Self::Error> {
        self.composite
            .align(align)
            .map_err(SerializerError::Composite)
    }

    #[inline]
    fn align_for<T>(&mut self) -> Result<usize, Self::Error> {
        self.composite
            .align_for::<T>()
            .map_err(SerializerError::Composite)
    }

    #[inline]
    unsafe fn resolve_aligned<T: Archive + ?Sized>(
        &mut self,
        value: &T,
        resolver: T::Resolver,
    ) -> Result<usize, Self::Error> {
        self.composite
            .resolve_aligned::<T>(value, resolver)
            .map_err(SerializerError::Composite)
    }

    #[inline]
    unsafe fn resolve_unsized_aligned<T: ArchiveUnsized + ?Sized>(
        &mut self,
        value: &T,
        to: usize,
        metadata_resolver: T::MetadataResolver,
    ) -> Result<usize, Self::Error> {
        self.composite
            .resolve_unsized_aligned(value, to, metadata_resolver)
            .map_err(SerializerError::Composite)
    }
}

impl<const N: usize> rkyv::ser::ScratchSpace for Serializer<N> {
    #[inline]
    unsafe fn push_scratch(
        &mut self,
        layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, Self::Error> {
        self.composite
            .push_scratch(layout)
            .map_err(SerializerError::Composite)
    }

    #[inline]
    unsafe fn pop_scratch(
        &mut self,
        ptr: std::ptr::NonNull<u8>,
        layout: std::alloc::Layout,
    ) -> Result<(), Self::Error> {
        self.composite
            .pop_scratch(ptr, layout)
            .map_err(SerializerError::Composite)
    }
}

impl<const N: usize> rkyv::ser::SharedSerializeRegistry for Serializer<N> {
    #[inline]
    fn get_shared_ptr(&self, value: *const u8) -> Option<usize> {
        self.composite.get_shared_ptr(value)
    }

    #[inline]
    fn add_shared_ptr(&mut self, value: *const u8, pos: usize) -> Result<(), Self::Error> {
        self.composite
            .add_shared_ptr(value, pos)
            .map_err(SerializerError::Composite)
    }
}

#[derive(Debug)]
pub enum SerializerError {
    Composite(CompositeSerializerError<Infallible, AllocScratchError, SharedSerializeMapError>),
    AsString(rkyv::with::AsStringError),
}

impl std::fmt::Display for SerializerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            SerializerError::Composite(ref e) => e.fmt(f),
            SerializerError::AsString(ref e) => e.fmt(f),
        }
    }
}

impl std::error::Error for SerializerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            SerializerError::Composite(ref e) => Some(e),
            SerializerError::AsString(ref e) => Some(e),
        }
    }
}

/// Provides a way to build a serializer error if converting an
/// `OsString`/`PathBuf` to a `String` fails. i.e., It's invalid UTF-8.
///
/// This impl is the entire point of this module. For whatever reason, none of
/// the serializers in rkyv handle this particular error case. Apparently, the
/// only way to use `rkyv::with::AsString` with `PathBuf` is to create one's
/// own serializer and provide a `From` impl for the `AsStringError` type.
/// Specifically, from the [AsString] docs:
///
/// > Regular serializers don’t support the custom error handling needed for
/// > this type by default. To use this wrapper, a custom serializer with an
/// > error type satisfying <S as Fallible>::Error: From<AsStringError> must be
/// > provided.
///
/// If we didn't need to use `rkyv::with::AsString` (which we do for
/// serializing `PathBuf` at time of writing), then we could just
/// use an `AllocSerializer` directly (which is a type alias for
/// `CompositeSerializer<...>`.
///
/// [AsString]: https://docs.rs/rkyv/0.7.43/rkyv/with/struct.AsString.html
impl From<rkyv::with::AsStringError> for SerializerError {
    fn from(e: rkyv::with::AsStringError) -> SerializerError {
        SerializerError::AsString(e)
    }
}
