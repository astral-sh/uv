use crate::Error;
use std::str::FromStr;

/// Architecture variants, e.g., with support for different instruction sets
#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash, Ord, PartialOrd)]
pub enum ArchVariant {
    /// Targets 64-bit Intel/AMD CPUs newer than Nehalem (2008).
    /// Includes SSE3, SSE4 and other post-2003 CPU instructions.
    V2,
    /// Targets 64-bit Intel/AMD CPUs newer than Haswell (2013) and Excavator (2015).
    /// Includes AVX, AVX2, MOVBE and other newer CPU instructions.
    V3,
    /// Targets 64-bit Intel/AMD CPUs with AVX-512 instructions (post-2017 Intel CPUs).
    /// Many post-2017 Intel CPUs do not support AVX-512.
    V4,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
pub struct Arch {
    pub(crate) family: target_lexicon::Architecture,
    pub(crate) variant: Option<ArchVariant>,
}

impl Ord for Arch {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.family == other.family {
            return self.variant.cmp(&other.variant);
        }

        // For the time being, manually make aarch64 windows disfavored
        // on its own host platform, because most packages don't have wheels for
        // aarch64 windows, making emulation more useful than native execution!
        //
        // The reason we do this in "sorting" and not "supports" is so that we don't
        // *refuse* to use an aarch64 windows pythons if they happen to be installed
        // and nothing else is available.
        //
        // Similarly if someone manually requests an aarch64 windows install, we
        // should respect that request (this is the way users should "override"
        // this behaviour).
        let preferred = if cfg!(all(windows, target_arch = "aarch64")) {
            Self {
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            }
        } else {
            // Prefer native architectures
            Self::from_env()
        };

        match (
            self.family == preferred.family,
            other.family == preferred.family,
        ) {
            (true, true) => unreachable!(),
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => {
                // Both non-preferred, fallback to lexicographic order
                self.family.to_string().cmp(&other.family.to_string())
            }
        }
    }
}

impl PartialOrd for Arch {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Arch {
    pub fn new(family: target_lexicon::Architecture, variant: Option<ArchVariant>) -> Self {
        Self { family, variant }
    }

    pub fn from_env() -> Self {
        #[cfg(test)]
        {
            if let Some(arch) = test_support::get_mock_arch() {
                return arch;
            }
        }

        Self {
            family: target_lexicon::HOST.architecture,
            variant: None,
        }
    }

    pub fn family(&self) -> target_lexicon::Architecture {
        self.family
    }

    pub fn is_arm(&self) -> bool {
        matches!(self.family, target_lexicon::Architecture::Arm(_))
    }

    pub fn is_wasm(&self) -> bool {
        matches!(self.family, target_lexicon::Architecture::Wasm32)
    }
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.family {
            target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686) => {
                write!(f, "x86")?;
            }
            inner => write!(f, "{inner}")?,
        }
        if let Some(variant) = self.variant {
            write!(f, "_{variant}")?;
        }
        Ok(())
    }
}

impl FromStr for Arch {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn parse_family(s: &str) -> Result<target_lexicon::Architecture, Error> {
            let inner = match s {
                // Allow users to specify "x86" as a shorthand for the "i686" variant, they should not need
                // to specify the exact architecture and this variant is what we have downloads for.
                "x86" => {
                    target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)
                }
                _ => target_lexicon::Architecture::from_str(s)
                    .map_err(|()| Error::UnknownArch(s.to_string()))?,
            };
            if matches!(inner, target_lexicon::Architecture::Unknown) {
                return Err(Error::UnknownArch(s.to_string()));
            }
            Ok(inner)
        }

        // First check for a variant
        if let Some((Ok(family), Ok(variant))) = s
            .rsplit_once('_')
            .map(|(family, variant)| (parse_family(family), ArchVariant::from_str(variant)))
        {
            // We only support variants for `x86_64` right now
            if !matches!(family, target_lexicon::Architecture::X86_64) {
                return Err(Error::UnsupportedVariant(
                    variant.to_string(),
                    family.to_string(),
                ));
            }
            return Ok(Self {
                family,
                variant: Some(variant),
            });
        }

        let family = parse_family(s)?;

        Ok(Self {
            family,
            variant: None,
        })
    }
}

impl FromStr for ArchVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "v2" => Ok(Self::V2),
            "v3" => Ok(Self::V3),
            "v4" => Ok(Self::V4),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for ArchVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V2 => write!(f, "v2"),
            Self::V3 => write!(f, "v3"),
            Self::V4 => write!(f, "v4"),
        }
    }
}

impl From<&uv_platform_tags::Arch> for Arch {
    fn from(value: &uv_platform_tags::Arch) -> Self {
        match value {
            uv_platform_tags::Arch::Aarch64 => Self::new(
                target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                None,
            ),
            uv_platform_tags::Arch::Armv5TEL => Self::new(
                target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv5te),
                None,
            ),
            uv_platform_tags::Arch::Armv6L => Self::new(
                target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv6),
                None,
            ),
            uv_platform_tags::Arch::Armv7L => Self::new(
                target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                None,
            ),
            uv_platform_tags::Arch::S390X => Self::new(target_lexicon::Architecture::S390x, None),
            uv_platform_tags::Arch::Powerpc => {
                Self::new(target_lexicon::Architecture::Powerpc, None)
            }
            uv_platform_tags::Arch::Powerpc64 => {
                Self::new(target_lexicon::Architecture::Powerpc64, None)
            }
            uv_platform_tags::Arch::Powerpc64Le => {
                Self::new(target_lexicon::Architecture::Powerpc64le, None)
            }
            uv_platform_tags::Arch::X86 => Self::new(
                target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                None,
            ),
            uv_platform_tags::Arch::X86_64 => Self::new(target_lexicon::Architecture::X86_64, None),
            uv_platform_tags::Arch::LoongArch64 => {
                Self::new(target_lexicon::Architecture::LoongArch64, None)
            }
            uv_platform_tags::Arch::Riscv64 => Self::new(
                target_lexicon::Architecture::Riscv64(target_lexicon::Riscv64Architecture::Riscv64),
                None,
            ),
            uv_platform_tags::Arch::Wasm32 => Self::new(target_lexicon::Architecture::Wasm32, None),
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        static MOCK_ARCH: RefCell<Option<Arch>> = const { RefCell::new(None) };
    }

    pub(crate) fn get_mock_arch() -> Option<Arch> {
        MOCK_ARCH.with(|arch| *arch.borrow())
    }

    fn set_mock_arch(arch: Option<Arch>) {
        MOCK_ARCH.with(|mock| *mock.borrow_mut() = arch);
    }

    pub(crate) struct MockArchGuard {
        previous: Option<Arch>,
    }

    impl MockArchGuard {
        pub(crate) fn new(arch: Arch) -> Self {
            let previous = get_mock_arch();
            set_mock_arch(Some(arch));
            Self { previous }
        }
    }

    impl Drop for MockArchGuard {
        fn drop(&mut self) {
            set_mock_arch(self.previous);
        }
    }

    /// Run a function with a mocked architecture.
    /// The mock is automatically cleaned up after the function returns.
    pub(crate) fn run_with_arch<F, R>(arch: Arch, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = MockArchGuard::new(arch);
        f()
    }

    pub(crate) fn x86_64() -> Arch {
        Arch::new(target_lexicon::Architecture::X86_64, None)
    }

    pub(crate) fn aarch64() -> Arch {
        Arch::new(
            target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
            None,
        )
    }
}
