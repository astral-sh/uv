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
    Archive, Deserialize, Portable, Serialize,
    api::high::{HighDeserializer, HighSerializer, HighValidator},
    bytecheck::CheckBytes,
    rancor,
    ser::allocator::ArenaHandle,
    util::AlignedVec,
};

use crate::{Error, ErrorKind};

/// A convenient alias for the rkyv serializer used by `uv-client`.
///
/// This utilizes rkyv's `HighSerializer` but fixes its type parameters where
/// possible since we don't need the full flexibility of a generic serializer.
pub(crate) type Serializer<'a> = HighSerializer<AlignedVec, ArenaHandle<'a>, rancor::Error>;

/// A convenient alias for the rkyv deserializer used by `uv-client`.
///
/// This utilizes rkyv's `HighDeserializer` but fixes its type parameters
/// where possible since we don't need the full flexibility of a generic
/// deserializer.
pub(crate) type Deserializer = HighDeserializer<rancor::Error>;

/// A convenient alias for the rkyv validator used by `uv-client`.
///
/// This utilizes rkyv's `HighValidator` but fixes its type parameters where
/// possible since we don't need the full flexibility of a generic validator.
pub(crate) type Validator<'a> = HighValidator<'a, rancor::Error>;

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

/// A catalog can outlive the few queries that materialize a registry distribution from it.
/// Keep large archives compressed. If an archive is queried repeatedly, retain its unpacked
/// form so that backtracking can query every remaining version without decoding it again.
#[derive(Debug)]
pub struct RetainedArchive<A>(std::sync::Mutex<RetainedBacking<A>>);

#[derive(Debug)]
enum RetainedBacking<A> {
    Unpacked(OwnedArchive<A>),
    Packed {
        bytes: Box<[u8]>,
        size: usize,
        read_once: bool,
    },
}

impl<A> RetainedArchive<A> {
    /// Pack large archives when the compressed bytes are smaller.
    pub fn new(archive: OwnedArchive<A>, minimum_size: usize) -> Self {
        let size = archive.raw.len();
        if size >= minimum_size
            && let Ok(bytes) = zstd::bulk::compress(&archive.raw, -5)
            && bytes.len() < size
        {
            return Self(std::sync::Mutex::new(RetainedBacking::Packed {
                bytes: bytes.into_boxed_slice(),
                size,
                read_once: false,
            }));
        }
        Self(std::sync::Mutex::new(RetainedBacking::Unpacked(archive)))
    }

    /// Read an archive, retaining the decoded buffer from its second access onwards.
    pub fn with<R>(&self, f: impl FnOnce(&OwnedArchive<A>) -> R) -> R {
        let mut backing = self.0.lock().expect("catalog lock is not poisoned");
        match &mut *backing {
            RetainedBacking::Unpacked(archive) => f(archive),
            RetainedBacking::Packed {
                bytes,
                size,
                read_once,
            } => {
                let mut raw = AlignedVec::with_capacity(*size);
                raw.resize(*size, 0);
                let written = zstd::bulk::decompress_to_buffer(bytes, &mut raw)
                    .expect("compressed bytes produced from a valid archive must decode");
                assert_eq!(written, *size);
                // The input comes exclusively from lossless compression of an OwnedArchive.
                // This restores the exact validated bytes to a buffer with the same alignment.
                let archive = OwnedArchive {
                    raw,
                    archive: std::marker::PhantomData,
                };
                let value = f(&archive);
                if *read_once {
                    *backing = RetainedBacking::Unpacked(archive);
                } else {
                    *read_once = true;
                }
                value
            }
        }
    }
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
    pub(crate) fn new(raw: AlignedVec) -> Result<Self, Error> {
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

    /// Creates an owned archive value from the unarchived value.
    ///
    /// # Errors
    ///
    /// This can fail if creating an archive for the given type fails.
    /// Currently, this, at minimum, includes cases where an `A` contains a
    /// `PathBuf` that is not valid UTF-8.
    pub(crate) fn from_unarchived(unarchived: &A) -> Result<Self, Error> {
        let raw = rkyv::to_bytes::<rancor::Error>(unarchived)
            .map_err(|e| ErrorKind::ArchiveWrite(e.to_string()))?;
        Ok(Self {
            raw,
            archive: std::marker::PhantomData,
        })
    }

    /// Returns the raw underlying bytes of this owned archive value.
    ///
    /// They are guaranteed to be a valid serialization of `Archived<A>`.
    ///
    /// Note that because this type has a `Deref` impl, this method requires
    /// fully-qualified syntax. So, if `o` is an `OwnedValue`, then use
    /// `OwnedValue::as_bytes(&o)`.
    pub(crate) fn as_bytes(this: &Self) -> &[u8] {
        &this.raw
    }

    /// Deserialize this owned archived value into the original
    /// `SimpleDetailMetadata`.
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
        // archive for SimpleDetailMetadata in the constructor, so we can skip
        // validation here. Since we don't mutate the buffer, this conversion
        // is guaranteed to be correct.
        #[allow(unsafe_code)]
        unsafe {
            rkyv::access_unchecked::<A::Archived>(&self.raw)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OwnedArchive, RetainedArchive, RetainedBacking};

    #[test]
    fn retained_archive_preserves_bytes_across_promotion() {
        let data: Vec<u8> = (0..=u8::MAX).cycle().take(100_000).collect();
        let archive = OwnedArchive::from_unarchived(&data).unwrap();
        let archive = RetainedArchive::new(archive, 0);
        assert!(matches!(
            *archive.0.lock().unwrap(),
            RetainedBacking::Packed { .. }
        ));
        for _ in 0..3 {
            archive.with(|restored| assert_eq!(restored.as_slice(), data.as_slice()));
        }
        assert!(matches!(
            *archive.0.lock().unwrap(),
            RetainedBacking::Unpacked(_)
        ));
    }
}
