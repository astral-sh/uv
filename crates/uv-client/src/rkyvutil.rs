/*!
Defines some helpers for use with `rkyv`.

# Owned archived type

Typical usage patterns with rkyv involve using an `&Archived<T>`, where values
of that type are cast from a `&[u8]`. The owned archive type in this module
effectively provides a way to use `Archive<T>` without needing to worry about
the lifetime of the buffer it's attached to. This works by making the owned
archive type own the buffer itself.

# Custom serializer

This module provides our own implementation of the `Serializer` trait.
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
    de::deserializers::SharedDeserializeMap,
    ser::serializers::{
        AlignedSerializer, AllocScratch, AllocScratchError, CompositeSerializer,
        CompositeSerializerError, FallbackScratch, HeapScratch, SharedSerializeMap,
        SharedSerializeMapError,
    },
    util::AlignedVec,
    validation::validators::DefaultValidator,
    Archive, ArchiveUnsized, CheckBytes, Deserialize, Fallible, Serialize,
};

use crate::{Error, ErrorKind};

/// An owned archived type.
///
/// This type is effectively an owned version of `Archived<A>`. Normally, when
/// one gets an archived type from a buffer, the archive type is bound to the
/// lifetime of the buffer. This effectively provides a home for that buffer so
/// that one can pass around an archived type as if it were owned.
///
/// Constructing the type requires validating the bytes are a valid
/// representation of an `Archived<A>`, but subsequent accesses (via deref) are
/// free.
///
/// Note that this type makes a number of assumptions about the specific
/// serializer, deserializer and validator used. This type could be made
/// more generic, but it's not clear we need that in uv. By making our
/// choices concrete here, we make use of this type much simpler to understand.
/// Unfortunately, AG couldn't find a way of making the trait bounds simpler,
/// so if `OwnedVec` is being used in trait implementations, the traits bounds
/// will likely need to be copied from here.
#[derive(Debug)]
pub struct OwnedArchive<A> {
    raw: rkyv::util::AlignedVec,
    archive: std::marker::PhantomData<A>,
}

impl<A> OwnedArchive<A>
where
    A: Archive + Serialize<Serializer<4096>>,
    A::Archived: (for<'a> CheckBytes<DefaultValidator<'a>>) + Deserialize<A, SharedDeserializeMap>,
{
    /// Create a new owned archived value from the raw aligned bytes of the
    /// serialized representation of an `A`.
    ///
    /// # Errors
    ///
    /// If the bytes fail validation (e.g., contains unaligned pointers or
    /// strings aren't valid UTF-8), then this returns an error.
    pub fn new(raw: rkyv::util::AlignedVec) -> Result<Self, Error> {
        // We convert the error to a simple string because... the error type
        // does not implement Send. And I don't think we really need to keep
        // the error type around anyway.
        let _ = rkyv::validation::validators::check_archived_root::<A>(&raw)
            .map_err(|e| ErrorKind::ArchiveRead(e.to_string()))?;
        Ok(Self {
            raw,
            archive: std::marker::PhantomData,
        })
    }

    /// Like `OwnedArchive::new`, but reads the value from the given reader.
    ///
    /// Note that this consumes the entirety of the given reader.
    ///
    /// # Errors
    ///
    /// If the bytes fail validation (e.g., contains unaligned pointers or
    /// strings aren't valid UTF-8), then this returns an error.
    pub fn from_reader<R: std::io::Read>(mut rdr: R) -> Result<Self, Error> {
        let mut buf = rkyv::util::AlignedVec::with_capacity(1024);
        buf.extend_from_reader(&mut rdr).map_err(ErrorKind::Io)?;
        Self::new(buf)
    }

    /// Creates an owned archive value from the unarchived value.
    ///
    /// # Errors
    ///
    /// This can fail if creating an archive for the given type fails.
    /// Currently, this, at minimum, includes cases where an `A` contains a
    /// `PathBuf` that is not valid UTF-8.
    pub fn from_unarchived(unarchived: &A) -> Result<Self, Error> {
        use rkyv::ser::Serializer;

        let mut serializer = crate::rkyvutil::Serializer::<4096>::default();
        serializer
            .serialize_value(unarchived)
            .map_err(ErrorKind::ArchiveWrite)?;
        let raw = serializer.into_serializer().into_inner();
        Ok(Self {
            raw,
            archive: std::marker::PhantomData,
        })
    }

    /// Write the underlying bytes of this archived value to the given writer.
    ///
    /// Note that because this type has a `Deref` impl, this method requires
    /// fully-qualified syntax. So, if `o` is an `OwnedValue`, then use
    /// `OwnedValue::write(&o, wtr)`.
    ///
    /// # Errors
    ///
    /// Any failures from writing are returned to the caller.
    pub fn write<W: std::io::Write>(this: &Self, mut wtr: W) -> Result<(), Error> {
        Ok(wtr.write_all(&this.raw).map_err(ErrorKind::Io)?)
    }

    /// Returns the raw underlying bytes of this owned archive value.
    ///
    /// They are guaranteed to be a valid serialization of `Archived<A>`.
    ///
    /// Note that because this type has a `Deref` impl, this method requires
    /// fully-qualified syntax. So, if `o` is an `OwnedValue`, then use
    /// `OwnedValue::as_bytes(&o)`.
    pub fn as_bytes(this: &Self) -> &[u8] {
        &this.raw
    }

    /// Deserialize this owned archived value into the original
    /// `SimpleMetadata`.
    ///
    /// Note that because this type has a `Deref` impl, this method requires
    /// fully-qualified syntax. So, if `o` is an `OwnedValue`, then use
    /// `OwnedValue::deserialize(&o)`.
    pub fn deserialize(this: &Self) -> A {
        (**this)
            .deserialize(&mut SharedDeserializeMap::new())
            .expect("valid archive must deserialize correctly")
    }
}

impl<A> std::ops::Deref for OwnedArchive<A>
where
    A: Archive + Serialize<Serializer<4096>>,
    A::Archived: (for<'a> CheckBytes<DefaultValidator<'a>>) + Deserialize<A, SharedDeserializeMap>,
{
    type Target = A::Archived;

    fn deref(&self) -> &A::Archived {
        // SAFETY: We've validated that our underlying buffer is a valid
        // archive for SimpleMetadata in the constructor, so we can skip
        // validation here. Since we don't mutate the buffer, this conversion
        // is guaranteed to be correct.
        #[allow(unsafe_code)]
        unsafe {
            rkyv::archived_root::<A>(&self.raw)
        }
    }
}

#[derive(Default)]
pub struct Serializer<const N: usize> {
    composite: CompositeSerializer<
        AlignedSerializer<AlignedVec>,
        FallbackScratch<HeapScratch<N>, AllocScratch>,
        SharedSerializeMap,
    >,
}

impl<const N: usize> Serializer<N> {
    fn into_serializer(self) -> AlignedSerializer<AlignedVec> {
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
    #[allow(unsafe_code)]
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
    #[allow(unsafe_code)]
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
    #[allow(unsafe_code)]
    unsafe fn push_scratch(
        &mut self,
        layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, Self::Error> {
        self.composite
            .push_scratch(layout)
            .map_err(SerializerError::Composite)
    }

    #[inline]
    #[allow(unsafe_code)]
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
            Self::Composite(ref e) => e.fmt(f),
            Self::AsString(ref e) => e.fmt(f),
        }
    }
}

impl std::error::Error for SerializerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Self::Composite(ref e) => Some(e),
            Self::AsString(ref e) => Some(e),
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
/// > error type satisfying <S as Fallible>`::Error`: From<AsStringError> must be
/// > provided.
///
/// If we didn't need to use `rkyv::with::AsString` (which we do for
/// serializing `PathBuf` at time of writing), then we could just
/// use an `AllocSerializer` directly (which is a type alias for
/// `CompositeSerializer<...>`.
///
/// [AsString]: https://docs.rs/rkyv/0.7.43/rkyv/with/struct.AsString.html
impl From<rkyv::with::AsStringError> for SerializerError {
    fn from(e: rkyv::with::AsStringError) -> Self {
        Self::AsString(e)
    }
}
