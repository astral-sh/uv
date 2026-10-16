//! Embedded signatures use big-endian integers, independently of Mach-O endianness.
//! Format definitions: <https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/kern/cs_blobs.h>.
//! Ad-hoc signing reference: <https://github.com/Homebrew/ruby-macho/blob/e106f7782df467357d0273c17aacd40df953de66/lib/macho/code_signing.rb>.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::bytes::{array, c_string, slice};
use crate::regions::FileRegions;
use crate::{Error, SigningIdentifier};

const SUPERBLOB: u32 = 0xfade_0cc0;
const CODE_DIRECTORY: u32 = 0xfade_0c02;
const REQUIREMENTS: u32 = 0xfade_0c01;
const ENTITLEMENTS: u32 = 0xfade_7171;
const DER_ENTITLEMENTS: u32 = 0xfade_7172;
const BLOB_WRAPPER: u32 = 0xfade_0b01;
const SLOT_REQUIREMENTS: u32 = 2;
const SLOT_ENTITLEMENTS: u32 = 5;
const SLOT_DER_ENTITLEMENTS: u32 = 7;
const CS_ADHOC: u32 = 2;
const CS_LINKER_SIGNED: u32 = 0x20000;
const PAGE_SIZE: usize = 4096;
const SHA256_SIZE: usize = 32;
// An empty requirements set and CMS wrapper match Apple's bare ad-hoc signatures.
const EMPTY_REQUIREMENTS: &[u8] = [
    REQUIREMENTS.to_be_bytes(),
    12u32.to_be_bytes(),
    0u32.to_be_bytes(),
]
.as_flattened();
const EMPTY_CMS_WRAPPER: &[u8] = [BLOB_WRAPPER.to_be_bytes(), 8u32.to_be_bytes()].as_flattened();

pub(crate) struct Metadata<'image> {
    directory: CodeDirectoryMetadata<'image>,
    requirements: &'image [u8],
    entitlements: Option<&'image [u8]>,
    der_entitlements: Option<&'image [u8]>,
}

impl<'image> Metadata<'image> {
    pub(crate) fn read(
        signature: Option<&'image [u8]>,
        identifier: SigningIdentifier<'image>,
        code_limit: usize,
        info_plist: Option<&[u8]>,
    ) -> Result<Self, Error> {
        let mut metadata = Self {
            directory: CodeDirectoryMetadata {
                identifier,
                flags: CS_ADHOC,
                runtime: 0,
                exec_flags: 0,
            },
            requirements: EMPTY_REQUIREMENTS,
            entitlements: None,
            der_entitlements: None,
        };

        let Some(signature) = signature else {
            return Ok(metadata);
        };
        let signature = parse_blob::<SUPERBLOB>(signature)?;
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

        let primary = entries
            .get(&0)
            .ok_or(Error::Malformed("missing primary CodeDirectory"))?;
        metadata.directory =
            CodeDirectoryMetadata::parse(primary, code_limit, info_plist, &entries)?;

        for (&slot, &data) in entries.range(1..) {
            match slot {
                SLOT_REQUIREMENTS => metadata.requirements = parse_blob::<REQUIREMENTS>(data)?,
                SLOT_ENTITLEMENTS => {
                    metadata.entitlements = Some(parse_blob::<ENTITLEMENTS>(data)?);
                }
                SLOT_DER_ENTITLEMENTS => {
                    metadata.der_entitlements = Some(parse_blob::<DER_ENTITLEMENTS>(data)?);
                }
                0x1000..=0x1004 => {
                    if metadata.directory
                        != CodeDirectoryMetadata::parse(data, code_limit, info_plist, &entries)?
                    {
                        return Err(Error::Unsupported("conflicting CodeDirectory metadata"));
                    }
                }
                0x10000 => {
                    parse_blob::<BLOB_WRAPPER>(data)?;
                }
                _ => {
                    return Err(Error::UnsupportedValue {
                        field: "code-signing slot",
                        value: u64::from(slot),
                    });
                }
            }
        }

        Ok(metadata)
    }

    /// Iterate retained components in ascending code-signing slot order.
    fn components(&self) -> impl Iterator<Item = (u32, &'image [u8])> + Clone {
        [
            (SLOT_REQUIREMENTS, Some(self.requirements)),
            (SLOT_ENTITLEMENTS, self.entitlements),
            (SLOT_DER_ENTITLEMENTS, self.der_entitlements),
        ]
        .into_iter()
        .filter_map(|(slot, data)| data.map(|data| (slot, data)))
    }

    /// Plan the signature and allocate its code-directory hashes before finalizing the image.
    pub(crate) fn prepare<'a>(
        &'a self,
        code_limit: usize,
        text: (u64, u64),
        info_plist: Option<&'a [u8]>,
    ) -> Result<PreparedSignature<'a>, Error> {
        // All supported macOS hosts accept SHA-256, regardless of the dylib's deployment target.
        let directory = PreparedCodeDirectory::new(self, code_limit, text)?;

        let count = self.components().count() + 2;
        let mut size = 12usize.checked_add(count * 8).ok_or(Error::TooLarge)?;
        for length in self
            .components()
            .map(|(_, data)| data.len())
            .chain([directory.bytes.len(), EMPTY_CMS_WRAPPER.len()])
        {
            size = size.checked_add(length).ok_or(Error::TooLarge)?;
        }

        Ok(PreparedSignature {
            metadata: self,
            directory,
            info_plist,
            code_limit,
            size: u32::try_from(size)?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct CodeDirectoryMetadata<'image> {
    identifier: SigningIdentifier<'image>,
    flags: u32,
    runtime: u32,
    exec_flags: u64,
}

impl<'image> CodeDirectoryMetadata<'image> {
    fn parse(
        data: &'image [u8],
        code_limit: usize,
        info_plist: Option<&[u8]>,
        entries: &BTreeMap<u32, &[u8]>,
    ) -> Result<Self, Error> {
        let data = parse_blob::<CODE_DIRECTORY>(data)?;
        let version = u32::from_be_bytes(array(data, 8)?);
        let flags = u32::from_be_bytes(array(data, 12)?);
        if flags & !0x0003_3f02 != 0 {
            return Err(Error::UnsupportedValue {
                field: "CodeDirectory flags",
                value: u64::from(flags),
            });
        }

        let fixed_size = match version {
            0x20000..=0x200ff => 44,
            0x20100..=0x201ff => 48,
            0x20200..=0x202ff => 52,
            0x20300..=0x203ff => 64,
            0x20400..=0x204ff => 88,
            0x20500..=0x205ff => 96,
            0x20600 => 108,
            _ => {
                return Err(Error::UnsupportedValue {
                    field: "CodeDirectory version",
                    value: u64::from(version),
                });
            }
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

        let identifier = c_string(data, identifier_offset)?;
        let identifier = SigningIdentifier::new(identifier)
            .map_err(|_| Error::Malformed("empty signing identifier"))?;

        let mut strings_end = identifier_offset + identifier.as_c_str().to_bytes_with_nul().len();
        if version >= 0x20200 {
            let team_offset = u32::from_be_bytes(array(header, 48)?) as usize;
            if team_offset != 0 {
                if team_offset < strings_end {
                    return Err(Error::Malformed("invalid signing team offset"));
                }
                let team = c_string(data, team_offset)?;
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
            algorithm => {
                return Err(Error::UnsupportedValue {
                    field: "CodeDirectory hash algorithm",
                    value: u64::from(algorithm),
                });
            }
        };

        if hash_size != expected_hash_size {
            return Err(Error::Malformed("invalid CodeDirectory hash size"));
        }

        let special_count = u32::from_be_bytes(array(header, 24)?) as usize;
        let code_count = u32::from_be_bytes(array(header, 28)?) as usize;
        let hash_offset = u32::from_be_bytes(array(header, 16)?) as usize;
        if special_count > SLOT_DER_ENTITLEMENTS as usize {
            return Err(Error::UnsupportedValue {
                field: "CodeDirectory special-slot count",
                value: special_count as u64,
            });
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
            if hash.iter().all(|byte| *byte == 0) {
                continue;
            }

            let slot = u32::try_from(index + 1)?;
            match slot {
                1 if info_plist.is_some() => {}
                SLOT_REQUIREMENTS | SLOT_ENTITLEMENTS | SLOT_DER_ENTITLEMENTS
                    if entries.contains_key(&slot) => {}
                _ => {
                    return Err(Error::UnsupportedValue {
                        field: "unavailable or unsupported CodeDirectory special slot",
                        value: u64::from(slot),
                    });
                }
            }
        }

        Ok(Self {
            identifier,
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
        })
    }
}

/// A signature plan that cannot be serialized until the image headers are finalized.
pub(crate) struct PreparedSignature<'a> {
    metadata: &'a Metadata<'a>,
    directory: PreparedCodeDirectory<'a>,
    info_plist: Option<&'a [u8]>,
    code_limit: usize,
    size: u32,
}

impl PreparedSignature<'_> {
    pub(crate) fn size(&self) -> usize {
        self.size as usize
    }

    /// Hash the finalized image and append its signature to the same buffer.
    pub(crate) fn sign(self, image: &mut Vec<u8>) -> Result<(), Error> {
        if image.len() != self.code_limit {
            return Err(Error::Malformed(
                "signature layout does not match the image size",
            ));
        }

        let directory = self.directory.sign(image, self.info_plist);
        let entries = [(0, directory.as_slice())]
            .into_iter()
            .chain(self.metadata.components())
            .chain([(0x10000, EMPTY_CMS_WRAPPER)]);
        let count = entries.clone().count();

        image.extend_from_slice(&SUPERBLOB.to_be_bytes());
        image.extend_from_slice(&self.size.to_be_bytes());
        image.extend_from_slice(&u32::try_from(count)?.to_be_bytes());

        let mut offset = 12 + count * 8;
        for (slot, data) in entries.clone() {
            image.extend_from_slice(&slot.to_be_bytes());
            image.extend_from_slice(&u32::try_from(offset)?.to_be_bytes());
            offset += data.len();
        }

        for (_, data) in entries {
            image.extend_from_slice(data);
        }

        Ok(())
    }
}

struct PreparedCodeDirectory<'a> {
    metadata: &'a Metadata<'a>,
    bytes: Vec<u8>,
    hash_offset: usize,
}

impl<'a> PreparedCodeDirectory<'a> {
    fn new(metadata: &'a Metadata<'a>, code_limit: usize, text: (u64, u64)) -> Result<Self, Error> {
        let directory = &metadata.directory;
        let fixed_size = if directory.runtime == 0 { 88 } else { 96 };
        let special_count = metadata
            .components()
            .map(|(slot, _)| slot as usize)
            .max()
            .unwrap_or(0);
        let code_count = code_limit.div_ceil(PAGE_SIZE);
        let identifier = directory.identifier.as_c_str().to_bytes_with_nul();
        let hash_offset = identifier
            .len()
            .checked_add(fixed_size + special_count * SHA256_SIZE)
            .ok_or(Error::TooLarge)?;
        let length = hash_offset
            .checked_add(code_count.checked_mul(SHA256_SIZE).ok_or(Error::TooLarge)?)
            .ok_or(Error::TooLarge)?;
        let length_u32 = u32::try_from(length)?;

        let mut output = vec![0; length];
        output[0..4].copy_from_slice(&u32::to_be_bytes(CODE_DIRECTORY));
        output[4..8].copy_from_slice(&u32::to_be_bytes(length_u32));
        output[8..12].copy_from_slice(&u32::to_be_bytes(if directory.runtime == 0 {
            0x20400
        } else {
            0x20500
        }));
        output[12..16].copy_from_slice(&u32::to_be_bytes(directory.flags));

        output[16..20].copy_from_slice(&u32::to_be_bytes(u32::try_from(hash_offset)?));
        output[20..24].copy_from_slice(&u32::to_be_bytes(u32::try_from(fixed_size)?));
        output[24..28].copy_from_slice(&u32::to_be_bytes(u32::try_from(special_count)?));
        output[28..32].copy_from_slice(&u32::to_be_bytes(u32::try_from(code_count)?));
        output[32..36].copy_from_slice(&u32::to_be_bytes(u32::try_from(code_limit)?));

        output[36] = 32;
        output[37] = 2;
        output[39] = 12;

        output[64..72].copy_from_slice(&u64::to_be_bytes(text.0));
        output[72..80].copy_from_slice(&u64::to_be_bytes(text.1));
        output[80..88].copy_from_slice(&u64::to_be_bytes(directory.exec_flags));
        if directory.runtime != 0 {
            output[88..92].copy_from_slice(&u32::to_be_bytes(directory.runtime));
        }

        output[fixed_size..fixed_size + identifier.len()].copy_from_slice(identifier);

        Ok(Self {
            metadata,
            bytes: output,
            hash_offset,
        })
    }

    fn sign(mut self, source: &[u8], info_plist: Option<&[u8]>) -> Vec<u8> {
        for (slot, data) in self.metadata.components() {
            let offset = self.hash_offset - slot as usize * SHA256_SIZE;
            self.bytes[offset..offset + SHA256_SIZE].copy_from_slice(&Sha256::digest(data));
        }

        if let Some(info_plist) = info_plist {
            self.bytes[self.hash_offset - SHA256_SIZE..self.hash_offset]
                .copy_from_slice(&Sha256::digest(info_plist));
        }

        let (hashes, _) = self.bytes[self.hash_offset..].as_chunks_mut::<SHA256_SIZE>();
        for (page, destination) in source.chunks(PAGE_SIZE).zip(hashes) {
            destination.copy_from_slice(&Sha256::digest(page));
        }

        self.bytes
    }
}

/// Validate a code-signing blob's magic and return its declared bytes.
fn parse_blob<const MAGIC: u32>(data: &[u8]) -> Result<&[u8], Error> {
    if u32::from_be_bytes(array(data, 0)?) != MAGIC {
        return Err(Error::Malformed("unexpected code-signing blob magic"));
    }
    let length = u32::from_be_bytes(array(data, 4)?) as usize;
    if length < 8 {
        return Err(Error::Malformed("invalid code-signing blob length"));
    }
    slice(data, 0, length)
}
