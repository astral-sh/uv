//! Embedded signatures use big-endian integers, independently of Mach-O endianness.
//! Format definitions: <https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/kern/cs_blobs.h>.
//! Ad-hoc signing reference: <https://github.com/Homebrew/ruby-macho/blob/e106f7782df467357d0273c17aacd40df953de66/lib/macho/code_signing.rb>.

use std::collections::BTreeMap;
use std::ffi::CStr;

use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::bytes::{array, slice};
use crate::regions::FileRegions;
use crate::{Error, SigningIdentifier};

const SUPERBLOB: u32 = 0xfade_0cc0;
const CODE_DIRECTORY: u32 = 0xfade_0c02;
const REQUIREMENTS: u32 = 0xfade_0c01;
const ENTITLEMENTS: u32 = 0xfade_7171;
const DER_ENTITLEMENTS: u32 = 0xfade_7172;
const BLOB_WRAPPER: u32 = 0xfade_0b01;
const CS_ADHOC: u32 = 2;
const CS_LINKER_SIGNED: u32 = 0x20000;
const PAGE_SIZE: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Metadata {
    identifier: SigningIdentifier,
    flags: u32,
    runtime: u32,
    exec_flags: u64,
    components: BTreeMap<ComponentSlot, Vec<u8>>,
}

impl Metadata {
    pub(crate) fn read(
        signature: Option<&[u8]>,
        identifier: &SigningIdentifier,
        code_limit: usize,
        info_plist: Option<&[u8]>,
    ) -> Result<Self, Error> {
        let mut metadata = Self {
            identifier: identifier.clone(),
            flags: CS_ADHOC,
            runtime: 0,
            exec_flags: 0,
            components: BTreeMap::new(),
        };

        if let Some(signature) = signature {
            let signature = Blob::<SUPERBLOB>::parse(signature)?.data;
            let count = u32::from_be_bytes(array(signature, 8)?) as usize;
            let index = slice(signature, 12, count.checked_mul(8).ok_or(Error::TooLarge)?)?;
            let index_end = 12 + index.len();

            let mut entries = BTreeMap::new();
            let mut regions = FileRegions::default();

            for entry in index.as_chunks::<8>().0 {
                let slot = u32::from_be_bytes(array(entry, 0)?);
                let offset = u32::from_be_bytes(array(entry, 4)?) as usize;
                let header = array::<8>(signature, offset)?;
                let size = u32::from_be_bytes(array(&header, 4)?) as usize;
                let data = slice(signature, offset, size)?;
                if offset < index_end || data.len() < header.len() {
                    return Err(Error::Malformed("invalid signature blob range"));
                }

                if entries.insert(slot, data).is_some() {
                    return Err(Error::Malformed("duplicate signature slot"));
                }
                regions
                    .insert(offset..offset + data.len())
                    .map_err(|_| Error::Malformed("overlapping signature blobs"))?;
            }

            if !entries.contains_key(&0) {
                return Err(Error::Malformed("missing primary CodeDirectory"));
            }

            let mut directory = None;
            let mut components = BTreeMap::new();

            for (&slot, &data) in &entries {
                match slot {
                    0 | 0x1000..=0x1004 => {
                        let parsed = Self::directory(
                            Blob::<CODE_DIRECTORY>::parse(data)?,
                            code_limit,
                            info_plist,
                            &entries,
                        )?;
                        if let Some(previous) = &directory
                            && previous != &parsed
                        {
                            return Err(Error::Unsupported("conflicting CodeDirectory metadata"));
                        }

                        directory = Some(parsed);
                    }
                    2 => {
                        let blob = Blob::<REQUIREMENTS>::parse(data)?;
                        components.insert(ComponentSlot::Requirements, blob.data.to_vec());
                    }
                    5 => {
                        let blob = Blob::<ENTITLEMENTS>::parse(data)?;
                        components.insert(ComponentSlot::Entitlements, blob.data.to_vec());
                    }
                    7 => {
                        let blob = Blob::<DER_ENTITLEMENTS>::parse(data)?;
                        components.insert(ComponentSlot::DerEntitlements, blob.data.to_vec());
                    }
                    0x10000 => {
                        Blob::<BLOB_WRAPPER>::parse(data)?;
                    }
                    _ => return Err(Error::Unsupported("unknown code-signing slot")),
                }
            }

            metadata = directory.ok_or(Error::Malformed("signature has no CodeDirectory"))?;
            metadata.components = components;
        }

        // An empty requirements set and CMS wrapper match Apple's bare ad-hoc signatures.
        metadata
            .components
            .entry(ComponentSlot::Requirements)
            .or_insert_with(|| {
                let mut bytes = vec![0; 12];
                bytes[0..4].copy_from_slice(&u32::to_be_bytes(REQUIREMENTS));
                bytes[4..8].copy_from_slice(&u32::to_be_bytes(12));

                bytes
            });

        Ok(metadata)
    }

    fn directory(
        data: Blob<'_, CODE_DIRECTORY>,
        code_limit: usize,
        info_plist: Option<&[u8]>,
        entries: &BTreeMap<u32, &[u8]>,
    ) -> Result<Self, Error> {
        let data = data.data;
        let version = u32::from_be_bytes(array(data, 8)?);
        let flags = u32::from_be_bytes(array(data, 12)?);
        if flags & !0x0003_3f02 != 0 {
            return Err(Error::Unsupported("CodeDirectory flags"));
        }

        let fixed_size = match version {
            0x20000..=0x200ff => 44,
            0x20100..=0x201ff => 48,
            0x20200..=0x202ff => 52,
            0x20300..=0x203ff => 64,
            0x20400..=0x204ff => 88,
            0x20500..=0x205ff => 96,
            0x20600 => 108,
            _ => return Err(Error::Unsupported("CodeDirectory version")),
        };
        let header = slice(data, 0, fixed_size)?;
        if (version >= 0x20100 && u32::from_be_bytes(array(header, 44)?) != 0)
            || (version >= 0x20500 && u32::from_be_bytes(array(header, 92)?) != 0)
            || (version >= 0x20600 && header[96..108].iter().any(|byte| *byte != 0))
            || header[38] != 0
        {
            return Err(Error::Unsupported(
                "scatter, pre-encryption, linkage, or platform signature",
            ));
        }

        let identifier_offset = u32::from_be_bytes(array(header, 20)?) as usize;
        if identifier_offset < fixed_size {
            return Err(Error::Malformed("invalid signing identifier offset"));
        }

        let identifier = CStr::from_bytes_until_nul(
            data.get(identifier_offset..)
                .ok_or(Error::Malformed("invalid string offset"))?,
        )
        .map_err(|_| Error::Malformed("unterminated string"))?;
        if identifier.is_empty() {
            return Err(Error::Malformed("empty signing identifier"));
        }

        let mut strings_end = identifier_offset + identifier.to_bytes_with_nul().len();
        if version >= 0x20200 {
            let team_offset = u32::from_be_bytes(array(header, 48)?) as usize;
            if team_offset != 0 {
                if team_offset < strings_end {
                    return Err(Error::Malformed("invalid signing team offset"));
                }
                let team = CStr::from_bytes_until_nul(
                    data.get(team_offset..)
                        .ok_or(Error::Malformed("invalid string offset"))?,
                )
                .map_err(|_| Error::Malformed("unterminated string"))?;
                strings_end = team_offset + team.to_bytes_with_nul().len();
            }
        }

        let limit = if version >= 0x20300 && u64::from_be_bytes(array(header, 56)?) != 0 {
            u64::from_be_bytes(array(header, 56)?)
        } else {
            u64::from(u32::from_be_bytes(array(header, 32)?))
        };

        if limit != code_limit as u64 {
            return Err(Error::Unsupported(
                "CodeDirectory does not cover the complete image",
            ));
        }

        let hash_size = usize::from(header[36]);
        let expected_hash_size = match header[37] {
            1 | 3 => 20,
            2 => 32,
            4 => 48,
            _ => return Err(Error::Unsupported("CodeDirectory hash algorithm")),
        };

        if hash_size != expected_hash_size {
            return Err(Error::Malformed("invalid CodeDirectory hash size"));
        }

        let special_count = u32::from_be_bytes(array(header, 24)?) as usize;
        let code_count = u32::from_be_bytes(array(header, 28)?) as usize;
        let hash_offset = u32::from_be_bytes(array(header, 16)?) as usize;
        if special_count > 7 {
            return Err(Error::Unsupported("CodeDirectory special slots"));
        }

        let special_start = hash_offset
            .checked_sub(special_count * hash_size)
            .ok_or(Error::Malformed("invalid special-slot hash range"))?;
        if special_start < strings_end {
            return Err(Error::Malformed("hashes overlap CodeDirectory metadata"));
        }

        let code_hashes = slice(
            data,
            hash_offset,
            code_count.checked_mul(hash_size).ok_or(Error::TooLarge)?,
        )?;
        let special_hashes = slice(data, special_start, special_count * hash_size)?;

        let page_size = if header[39] == 0 {
            code_limit.max(1)
        } else {
            1usize
                .checked_shl(u32::from(header[39]))
                .ok_or(Error::TooLarge)?
        };

        if code_hashes.chunks_exact(hash_size).len() != code_limit.div_ceil(page_size) {
            return Err(Error::Malformed("invalid CodeDirectory page count"));
        }

        for (index, hash) in special_hashes.rchunks_exact(hash_size).enumerate() {
            let slot = index + 1;
            if hash.iter().all(|byte| *byte == 0) {
                continue;
            }

            match slot {
                1 if info_plist.is_some() => {}
                2 | 5 | 7 if entries.contains_key(&u32::try_from(slot)?) => {}
                _ => {
                    return Err(Error::Unsupported(
                        "unavailable or unsupported special-slot data",
                    ));
                }
            }
        }

        Ok(Self {
            identifier: SigningIdentifier::new(identifier.to_bytes())?,
            flags: (flags & !CS_LINKER_SIGNED) | CS_ADHOC,
            runtime: if version >= 0x20500 {
                u32::from_be_bytes(array(header, 88)?)
            } else {
                0
            },
            // The main-binary flag is not applicable to a dylib.
            exec_flags: if version >= 0x20400 {
                u64::from_be_bytes(array(header, 80)?) & !1
            } else {
                0
            },
            components: BTreeMap::new(),
        })
    }

    /// Allocate the complete signature before the image's final headers are written.
    pub(crate) fn prepare<'a>(
        &'a self,
        code_limit: usize,
        text: (u64, u64),
        info_plist: Option<&'a [u8]>,
        algorithms: SigningAlgorithms,
    ) -> Result<PreparedSignature<'a>, Error> {
        let algorithms: &[Hash] = match algorithms {
            SigningAlgorithms::Sha256 => &[Hash::Sha256],
            SigningAlgorithms::Sha1AndSha256 => &[Hash::Sha1, Hash::Sha256],
        };
        let directories = algorithms
            .iter()
            .map(|&hash| PreparedCodeDirectory::new(self, code_limit, text, hash))
            .collect::<Result<Vec<_>, Error>>()?;

        let count = self.components.len() + directories.len() + 1;
        let mut size = 12usize.checked_add(count * 8).ok_or(Error::TooLarge)?;
        for length in self
            .components
            .values()
            .map(Vec::len)
            .chain(directories.iter().map(|directory| directory.bytes.len()))
            .chain([8])
        {
            size = size.checked_add(length).ok_or(Error::TooLarge)?;
        }

        Ok(PreparedSignature {
            metadata: self,
            directories,
            info_plist,
            code_limit,
            size: u32::try_from(size)?,
        })
    }
}

/// The hash algorithms required by the dylib's deployment target.
#[derive(Clone, Copy)]
pub(crate) enum SigningAlgorithms {
    Sha256,
    Sha1AndSha256,
}

/// Allocated signature records that cannot be serialized until hashing completes.
pub(crate) struct PreparedSignature<'a> {
    metadata: &'a Metadata,
    directories: Vec<PreparedCodeDirectory<'a>>,
    info_plist: Option<&'a [u8]>,
    code_limit: usize,
    size: u32,
}

impl PreparedSignature<'_> {
    pub(crate) fn size(&self) -> usize {
        self.size as usize
    }

    /// Consume the prepared records, hashing the image after its headers are finalized.
    pub(crate) fn sign(self, source: &[u8]) -> Result<Vec<u8>, Error> {
        if source.len() != self.code_limit {
            return Err(Error::Malformed(
                "signature layout does not match the image size",
            ));
        }

        let mut entries = self
            .metadata
            .components
            .iter()
            .map(|(slot, data)| (*slot as u32, data.clone()))
            .collect::<BTreeMap<_, _>>();
        for (index, directory) in self.directories.into_iter().enumerate() {
            let slot = if index == 0 { 0 } else { 0x1000 };
            entries.insert(slot, directory.sign(source, self.info_plist));
        }

        // An empty CMS wrapper matches Apple's bare ad-hoc signatures.
        let wrapper = [BLOB_WRAPPER.to_be_bytes(), 8u32.to_be_bytes()].concat();
        entries.insert(0x10000, wrapper);

        let mut output = Vec::with_capacity(self.size as usize);
        output.extend_from_slice(&SUPERBLOB.to_be_bytes());
        output.extend_from_slice(&self.size.to_be_bytes());
        output.extend_from_slice(&u32::try_from(entries.len())?.to_be_bytes());

        let mut offset = 12 + entries.len() * 8;
        for (slot, data) in &entries {
            output.extend_from_slice(&slot.to_be_bytes());
            output.extend_from_slice(&u32::try_from(offset)?.to_be_bytes());
            offset += data.len();
        }

        for data in entries.into_values() {
            output.extend(data);
        }

        Ok(output)
    }
}

struct PreparedCodeDirectory<'a> {
    metadata: &'a Metadata,
    bytes: Vec<u8>,
    hash_offset: usize,
    hash: Hash,
}

impl<'a> PreparedCodeDirectory<'a> {
    fn new(
        metadata: &'a Metadata,
        code_limit: usize,
        text: (u64, u64),
        hash: Hash,
    ) -> Result<Self, Error> {
        let fixed_size = if metadata.runtime == 0 { 88 } else { 96 };
        let special_count = metadata
            .components
            .keys()
            .map(|slot| *slot as usize)
            .max()
            .unwrap_or(0);
        let code_count = code_limit.div_ceil(PAGE_SIZE);
        let identifier = metadata.identifier.as_c_str().to_bytes_with_nul();
        let hash_offset = identifier
            .len()
            .checked_add(fixed_size + special_count * hash.size())
            .ok_or(Error::TooLarge)?;
        let length = hash_offset
            .checked_add(code_count.checked_mul(hash.size()).ok_or(Error::TooLarge)?)
            .ok_or(Error::TooLarge)?;
        let length_u32 = u32::try_from(length)?;

        let mut output = vec![0; length];
        output[0..4].copy_from_slice(&u32::to_be_bytes(CODE_DIRECTORY));
        output[4..8].copy_from_slice(&u32::to_be_bytes(length_u32));
        output[8..12].copy_from_slice(&u32::to_be_bytes(if metadata.runtime == 0 {
            0x20400
        } else {
            0x20500
        }));
        output[12..16].copy_from_slice(&u32::to_be_bytes(metadata.flags));

        output[16..20].copy_from_slice(&u32::to_be_bytes(u32::try_from(hash_offset)?));
        output[20..24].copy_from_slice(&u32::to_be_bytes(u32::try_from(fixed_size)?));
        output[24..28].copy_from_slice(&u32::to_be_bytes(u32::try_from(special_count)?));
        output[28..32].copy_from_slice(&u32::to_be_bytes(u32::try_from(code_count)?));
        output[32..36].copy_from_slice(&u32::to_be_bytes(u32::try_from(code_limit)?));

        output[36] = match hash {
            Hash::Sha1 => 20,
            Hash::Sha256 => 32,
        };
        output[37] = match hash {
            Hash::Sha1 => 1,
            Hash::Sha256 => 2,
        };
        output[39] = 12;

        output[64..72].copy_from_slice(&u64::to_be_bytes(text.0));
        output[72..80].copy_from_slice(&u64::to_be_bytes(text.1));
        output[80..88].copy_from_slice(&u64::to_be_bytes(metadata.exec_flags));
        if metadata.runtime != 0 {
            output[88..92].copy_from_slice(&u32::to_be_bytes(metadata.runtime));
        }

        output[fixed_size..fixed_size + identifier.len()].copy_from_slice(identifier);

        Ok(Self {
            metadata,
            bytes: output,
            hash_offset,
            hash,
        })
    }

    fn sign(mut self, source: &[u8], info_plist: Option<&[u8]>) -> Vec<u8> {
        for (&slot, data) in &self.metadata.components {
            let offset = self.hash_offset - slot as usize * self.hash.size();
            self.hash
                .write(data, &mut self.bytes[offset..offset + self.hash.size()]);
        }

        if let Some(info_plist) = info_plist {
            self.hash.write(
                info_plist,
                &mut self.bytes[self.hash_offset - self.hash.size()..self.hash_offset],
            );
        }

        for (page, destination) in source
            .chunks(PAGE_SIZE)
            .zip(self.bytes[self.hash_offset..].chunks_mut(self.hash.size()))
        {
            self.hash.write(page, destination);
        }

        self.bytes
    }
}

/// A complete code-signing blob with a validated magic and declared length.
#[derive(Clone, Copy)]
struct Blob<'a, const MAGIC: u32> {
    data: &'a [u8],
}

impl<'a, const MAGIC: u32> Blob<'a, MAGIC> {
    fn parse(data: &'a [u8]) -> Result<Self, Error> {
        if u32::from_be_bytes(array(data, 0)?) != MAGIC {
            return Err(Error::Malformed("unexpected code-signing blob magic"));
        }
        let length = u32::from_be_bytes(array(data, 4)?) as usize;
        if length < 8 {
            return Err(Error::Malformed("invalid code-signing blob length"));
        }

        Ok(Self {
            data: slice(data, 0, length)?,
        })
    }
}

/// The metadata components an ad-hoc signature can retain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ComponentSlot {
    Requirements = 2,
    Entitlements = 5,
    DerEntitlements = 7,
}

#[derive(Clone, Copy)]
enum Hash {
    Sha1,
    Sha256,
}

impl Hash {
    fn size(self) -> usize {
        match self {
            Self::Sha1 => 20,
            Self::Sha256 => 32,
        }
    }

    fn write(self, data: &[u8], destination: &mut [u8]) {
        match self {
            Self::Sha1 => destination.copy_from_slice(&Sha1::digest(data)),
            Self::Sha256 => destination.copy_from_slice(&Sha256::digest(data)),
        }
    }
}
