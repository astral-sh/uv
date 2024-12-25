/*!
Defines some helpers for use with `rkyv`.

# Owned archived type

Typical usage patterns with rkyv involve using an `&Archived<T>`, where values
of that type are cast from a `&[u8]`. The owned archive type in this module
effectively provides a way to use `Archive<T>` without needing to worry about
the lifetime of the buffer it's attached to. This works by making the owned
archive type own the buffer itself. It then provides convenient routines for
serializing and deserializing.
*/

use rkyv::{
    api::high::{HighDeserializer, HighSerializer, HighValidator},
    bytecheck::CheckBytes,
    rancor,
    ser::allocator::ArenaHandle,
    util::AlignedVec,
    Archive, Deserialize, Portable, Serialize,
};

use crate::{Error, ErrorKind};

/// A convenient alias for the rkyv serializer used by `uv-client`.
///
/// This utilizes rkyv's `HighSerializer` but fixes its type parameters where
/// possible since we don't need the full flexibility of a generic serializer.
pub type Serializer<'a> = HighSerializer<AlignedVec, ArenaHandle<'a>, rancor::Error>;

/// A convenient alias for the rkyv deserializer used by `uv-client`.
///
/// This utilizes rkyv's `HighDeserializer` but fixes its type parameters
/// where possible since we don't need the full flexibility of a generic
/// deserializer.
pub type Deserializer = HighDeserializer<rancor::Error>;

/// A convenient alias for the rkyv validator used by `uv-client`.
///
/// This utilizes rkyv's `HighValidator` but fixes its type parameters where
/// possible since we don't need the full flexibility of a generic validator.
pub type Validator<'a> = HighValidator<'a, rancor::Error>;

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
    raw: AlignedVec,
    archive: std::marker::PhantomData<A>,
}

impl<A> OwnedArchive<A>
where
    A: Archive + for<'a> Serialize<Serializer<'a>>,
    A::Archived: Portable + Deserialize<A, Deserializer> + for<'a> CheckBytes<Validator<'a>>,
{
    /// Create a new owned archived value from the raw aligned bytes of the
    /// serialized representation of an `A`.
    ///
    /// # Errors
    ///
    /// If the bytes fail validation (e.g., contains unaligned pointers or
    /// strings aren't valid UTF-8), then this returns an error.
    pub fn new(raw: AlignedVec) -> Result<Self, Error> {
        // We convert the error to a simple string because... the error type
        // does not implement Send. And I don't think we really need to keep
        // the error type around anyway.
        let _ = rkyv::access::<A::Archived, rancor::Error>(&raw)
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
        let mut buf = AlignedVec::with_capacity(1024);
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
        let raw = rkyv::to_bytes::<rancor::Error>(unarchived)
            .map_err(|e| ErrorKind::ArchiveWrite(e.to_string()))?;
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
        rkyv::deserialize(&**this).expect("valid archive must deserialize correctly")
    }
}

impl<A> std::ops::Deref for OwnedArchive<A>
where
    A: Archive + for<'a> Serialize<Serializer<'a>>,
    A::Archived: Portable + Deserialize<A, Deserializer> + for<'a> CheckBytes<Validator<'a>>,
{
    type Target = A::Archived;

    fn deref(&self) -> &A::Archived {
        // SAFETY: We've validated that our underlying buffer is a valid
        // archive for SimpleMetadata in the constructor, so we can skip
        // validation here. Since we don't mutate the buffer, this conversion
        // is guaranteed to be correct.
        #[allow(unsafe_code)]
        unsafe {
            rkyv::access_unchecked::<A::Archived>(&self.raw)
        }
    }
}
