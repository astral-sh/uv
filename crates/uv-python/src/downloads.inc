// DO NOT EDIT
//
// Generated with `crates/uv-python/template-download-metadata.py`
// From template at `crates/uv-python/src/downloads.inc.mustache`

use crate::platform::ArchVariant;
use crate::PythonVariant;
use uv_pep440::{Prerelease, PrereleaseKind};

pub(crate) const PYTHON_DOWNLOADS: &[ManagedPythonDownload] = &[
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("25ca42cfac68cfa6bd6aa6c950cb76c950dca56e01ff00458120d3f62967a498")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("cf3cfc41dc3017732d448f7273ba9a77a5629e3605459feff756922721c410fd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("c801a80c40ad36d7e3d38d2d550bb64b39af0efef7081a4b1de3e4a43ece5c0d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("8c32b8a7a210a108ba3bf5391ecba48d30b5794c2d27783fdcec50b95797157e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("e854912d185dfd6f1e8606cb4a74a95a7535a7c95c58b5be78f4014296b488be")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("dc519bd25ee659db1b5f1f05e342fdc9c74c348b49f793ca3536f33e189002b0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("f15df22d793733a457115292289091fbf51d64af042800b1218daa2a0498105f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("05b3bbc97d113b64f4a0dab680e0dbe73e9f59ce8304312ef64d4b6c2822c520")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("5bfb969703d2f26c26e3a31b07145d40c7a3910052fdc5f95a199868908c78e3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("07af19e001b8bce915296e2f0aaa32314515864d735175d52d604f8f8fef55ad")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("27be1c4a64b3b1ace70f3e72662405c393b5e50690516b755f56c65582235e24")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("d56109dd712199b25477154a93eb68b867ad46d5049c07869f8161c5c9271fab")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("4949fd6a337418c9eb7e55871619502b96e8be712711f61414f62a161456b83c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("410460370f55da3b4778f89acafc3ec02d2aa390fb9b48f6b5dbdccfd60d39ac")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("aa0316647307516ee1028055f0df98f05656033f1968d6a935dd1a34dde955b7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("082907fea39786100190137cdcf2d2e3672a33ef90e88ad0b669ba0c5205ec7d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("af37b475224eb9d37aa90e68094bbc5f9186dd421eb0fdff59f24c34a1eed4f0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-aarch64-apple-darwin-freethreaded%2Bpgo%2Blto-full.tar.zst",
        sha256: Some("b7044c2d584eb2261edb18db58a51b8b164e3cf2101a650f7be6d850a2859474")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64-apple-darwin-freethreaded%2Bpgo%2Blto-full.tar.zst",
        sha256: Some("ccff425786a59f387d449eeff7d8afeab2d11cf2aa3631b5777e7d697f60bacd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-aarch64-unknown-linux-gnu-freethreaded%2Blto-full.tar.zst",
        sha256: Some("5d0b6ab67f02f343348560e32664d5cf4343aa016f34334fc178fe976e5ea74b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-armv7-unknown-linux-gnueabi-freethreaded%2Blto-full.tar.zst",
        sha256: Some("4373948ca535d2c011ddc42e9d7d6b415ac1c2495d2c8ec414cc22042ae0811c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-armv7-unknown-linux-gnueabihf-freethreaded%2Blto-full.tar.zst",
        sha256: Some("bbfb13cdf815bceffdf3b22d2992ddd24f09deb3cd3721285438151733c027db")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-ppc64le-unknown-linux-gnu-freethreaded%2Blto-full.tar.zst",
        sha256: Some("9f82f0e05d37362be651f51a5028979b0409c0b4b3ceecd207b8b4539b8c72ef")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-s390x-unknown-linux-gnu-freethreaded%2Blto-full.tar.zst",
        sha256: Some("5b6bacbeb90bac09799f08f9937f811ab87c18e2654f9827f4ef2cb7832d8920")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64-unknown-linux-gnu-freethreaded%2Bpgo%2Blto-full.tar.zst",
        sha256: Some("065368d84d0d271cf081cf8a3599a0bfdcab00d45ddf3cde487dc5f901992ab5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64_v2-unknown-linux-gnu-freethreaded%2Bpgo%2Blto-full.tar.zst",
        sha256: Some("196aa926332003687eb8167e55d5761439890a763dc3f0a0758355af10979f9f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64_v3-unknown-linux-gnu-freethreaded%2Bpgo%2Blto-full.tar.zst",
        sha256: Some("65176446a0aa53f8cdbcb967d3e70d09e6bb21b058657461041611dc0f6ac111")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64_v4-unknown-linux-gnu-freethreaded%2Blto-full.tar.zst",
        sha256: Some("a3f43b7b6073ef7a932638c3fde128c9db306c26655ec0cdaf13af9e15f1f0cf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-i686-pc-windows-msvc-freethreaded%2Bpgo-full.tar.zst",
        sha256: Some("957da31c99b168d15116af5e1f3c285a5e0da9ed2c6440aa7080f49118c0931a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.13.1%2B20241219-x86_64-pc-windows-msvc-freethreaded%2Bpgo-full.tar.zst",
        sha256: Some("e34828f6ae08e252b07a277f0021658963cddca8a858ae858397c1e5898d7e2a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("e94fafbac07da52c965cb6a7ffc51ce779bd253cd98af801347aac791b96499f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("406664681bd44af35756ad08f5304f1ec57070bb76fae8ff357ff177f229b224")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("06e633164cb0133685a2ce14af88df0dbcaea4b0b2c5d3348d6b81393307481a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("1b18f0eac4c3578ecca52ff388276546c701cea22410235716195c52ad7d0344")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("be2bbcb985ecf12eb7a16c18043a2b0b8551d8e8799c49a0d766b541dd465f47")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("afe014200fea7505a67658fd82e70ccb49982deee752809849e781b941b941ec")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("b5782c027a8802b19656e961f73193cf060b124fd052dff19bb6d21b9e51ed14")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("b5e74d1e16402b633c6f04519618231fc0dbae7d2f9e4b1ac17c294cc3d3d076")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("10978500ab6589760716c644aeadffa0f2c0bf31ea10f0c6160fee933933a567")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("5c10c0b05c66bc6fc9a87f456ac1606057fe1865cc525eb7ecd9a5640f15426a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("6797067b7da58c29384cd32cb77f62dc18e813e6c72f6b0baf39672d84431bf2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("a9e705f714ccbe721ba0e29b80e6f2a5f0960c39245959de58c8076fd31515e0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("0572fbf46e49adaa5a418eeb92daeb624a080466a46d87d48e4a80f1ba9e408a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("36bd61970dc1d3a7034f2645aa14b47b6aa1669819fb7803650b5a7919afddbf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("a846556bdd4d61b3f96aa8b096c86db031d7f5b5ef50f9e614c9259527afc9de")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("d5538ed2a247220516d4c14e8452f2c49318b29f8b524c908a1ed42e405bd8cc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("c8134287496727922a5c47896b4f2b1623e3aab91cbb7c1ca64542db7593f3f1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-aarch64-apple-darwin-freethreaded%2Bpgo%2Blto-full.tar.zst",
        sha256: Some("efc2e71c0e05bc5bedb7a846e05f28dd26491b1744ded35ed82f8b49ccfa684b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64-apple-darwin-freethreaded%2Bpgo%2Blto-full.tar.zst",
        sha256: Some("2e07dfea62fe2215738551a179c87dbed1cc79d1b3654f4d7559889a6d5ce4eb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-aarch64-unknown-linux-gnu-freethreaded%2Blto-full.tar.zst",
        sha256: Some("59b50df9826475d24bb7eff781fa3949112b5e9c92adb29e96a09cdf1216d5bd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-armv7-unknown-linux-gnueabi-freethreaded%2Blto-full.tar.zst",
        sha256: Some("cafc0f10503e6ec0a62da9273aabb7b1d5c3f3619e80a08f9076665eb7e24b00")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-armv7-unknown-linux-gnueabihf-freethreaded%2Blto-full.tar.zst",
        sha256: Some("636fe5015ffefaa5588dbcb62c026bfd71e14e3fbfac92af0b969d9f88efc4a5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-ppc64le-unknown-linux-gnu-freethreaded%2Blto-full.tar.zst",
        sha256: Some("1217efa5f4ce67fcc9f7eb64165b1bd0912b2a21bc25c1a7e2cb174a21a5df7e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-s390x-unknown-linux-gnu-freethreaded%2Blto-full.tar.zst",
        sha256: Some("6c3e1e4f19d2b018b65a7e3ef4cd4225c5b9adfbc490218628466e636d5c4b8c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64-unknown-linux-gnu-freethreaded%2Bpgo%2Blto-full.tar.zst",
        sha256: Some("a73adeda301ad843cce05f31a2d3e76222b656984535a7b87696a24a098b216c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64_v2-unknown-linux-gnu-freethreaded%2Bpgo%2Blto-full.tar.zst",
        sha256: Some("6dd725b5866e193d201b4d6361bf03b2a954eeb8b67788d875ca4991776f852f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64_v3-unknown-linux-gnu-freethreaded%2Bpgo%2Blto-full.tar.zst",
        sha256: Some("fd16a95790203891163db772b6818522ee887857edae1a1cd6860e58eaf8f305")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64_v4-unknown-linux-gnu-freethreaded%2Blto-full.tar.zst",
        sha256: Some("8ad7fd8dd74cad2c9d5a269ad585885ef2b41468a691804f1dd66e1cec8286cb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-i686-pc-windows-msvc-freethreaded%2Bpgo-full.tar.zst",
        sha256: Some("7794b0209af46b6347aab945f1ccc3b24add0a17b3f6fb7741447bc44d10bf4a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Freethreaded
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0%2B20241016-x86_64-pc-windows-msvc-freethreaded%2Bpgo-full.tar.zst",
        sha256: Some("bfd89f9acf866463bc4baf01733da5e767d13f5d0112175a4f57ba91f1541310")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("685ef71882f16eabab0bc838094727978370f0ad95c29f7f5c244ffa31316aeb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("0f5f9fcf82093c428b80c552165544439f4adcdbe5129ecf721d619e532e9b5e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("1414c6b37f37e8fd9d14e48d81e313eb9c965cb0330747d5d2d689dd7e0c7043")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("11befeaf4768c2ebbb258f5b07f94b7700f16424f858d6d2c250b434e99ce07c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("b7180d5ea5fda2f397d04e2e6e11a2a7e0d732542bf54c484afb81d087a7b927")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("59a2a81991d78bd658742d69b577a2b4c0734628ed42bff68615686eaf96f2ab")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("2769182e58b0dddec15222bfeecbd4b12fde61c38f23a90aa942514f3545fb9b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("445156c61e1cc167f7b8777ad08cc36e5598e12cd27e07453f6e6dc0f62e421e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("4df6b7665c735a728d72e6f49034f1a6b7d9a54b0fbc472dc2ca525eb3dd513f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("ae477db35ccec397a19c9d61271455adf4917ab35993dbcacae8d126890f6b12")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("8c73e28a683b7e826ed5bda4cf119ec8270238fdf936e3a2b3ca0938cfcde8c9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("2a505cda4d7d62dec1829a06fa502eb514a3182c193b2f2aaf9c08bccb143dc0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("27a93fc7678782b392e5ee8e654635bc29409939318bbc341df3d29386f2166f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("8c1424f2501419b88560951497df095c82e856af9b8f817f96beedeb4d8bc32d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("8c318b2a79d75ca7ce3b76f8799a1db0337ba1278601bf0ec4433d64313d1aca")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("873905b3e5e8cba700126e8d6ed28ad3aef0dd102f730f8ca196018477dd2da6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 3 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.13.0rc3%2B20241002-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("b59317828ef88f138ee122d420b60f2705bc72ae846ff69562e79e6c5cbc3177")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("9e17f9fcc314a5dd489089a7502a525c4dd08af862f9cf33b52161a752f2a5b7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("971668ac7f3168efc4d2b589e9d36247ab8ca9f9525c56c8aa7bfd374060105b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("d99a663d3b9f8792a659e366372e685550045cad12aef11645c06a9b6edcd071")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("4ca7f2aeaabf8dbb2193f0fa86f869525a5c209eb403a39a73f4cf7040cf3613")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("0db2d263bdbb3af1e8dc0677fa44a5cda992ba989551346ccbbfd50a86135c3d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("70073333f7d3f0b900c7299659fec069bbefd5e04808b3729d2434b2232ac729")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("50a2080e30d1504e76e5471e46830f0b4974c66b538ed8ec7df416975133ff89")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("1893a218709d3664b7a2b80f5598b5f25c0c3fe2bcc8d0a1c75eec6bbb93d602")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("6f09aa5ba6aab8bf21955dbc3d6bab19125130ef0ebe29242b0e5ac1eebb3161")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("e60fc176fad636bbd287e92f9e9954b8b10d164984e98f51e4dc3d47314fa8a5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("16f23e539336fb45895b3c4fac981365b53fbbd86115feb9516b50a0716394b1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("bf34803e8c05cdb4eee5df5b051cdded1732975c991116a1af823a2b15e49be3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("df60f51c87da60af67453e5d7c5673f1534c2772d92a4885ae51add3cd32848e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("fb8d87e21bd0cb83bfbb5325f2db170365726a9cac58dd5b552fbd084671b785")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("eafb931d1a0e6f7237486d2967d0ccacd49791bef27f233895b5d18581ae939a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("759f600b27a6a0ef2638cb02e8bbcc6de726dd1c896759f78da3e412f6c992e9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 13,
            patch: 0,
            prerelease: Some(Prerelease { kind: PrereleaseKind::Rc, number: 2 }),
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.13.0rc2%2B20240909-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("c883205751c714bd0519592673a88f160a55d34344cc1368353ad34a679eb94a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("abe1de2494bb8b243fd507944f4d50292848fa00685d5288c858a72623a16635")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("867c1af10f204224b571f8f2593fc9eb580fe0c2376224d1096ebe855ad8c722")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("fb983ec85952513f5f013674fcbf4306b1a142c50fcfd914c2c3f00c61a874b0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("0567907c0147753b54683ffdd3559d53aa4f10b203f067628586bdc545dddffb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("d970da6a727a069ae765ae067ca93e369cf9881e5140e28f5030f6314595dd9b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("462b028ce314eb57ced802ce7f9459520365ce5d635d7ae24d2d532b78c9ee12")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("6791e6572b6f412fab38e6032dac87c37d6870b12c598f17c76c230678a7a86d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("698e53b264a9bcd35cfa15cd680c4d78b0878fa529838844b5ffd0cd661d6bc2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("451c02f24d1883c2f8ed80914ca4d8567dbe429888332e7cc6cf597e8a7c2555")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("9cf1273ae2fdbbbf308bd81ad2d0b384bec963097a83f8a6455bb187a7714d82")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("630e0131c947a82b342cc8115e85a98a70bb1ebb1580c88b01e5c1dc61d25814")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("07319b611e2563ef5281ab4233c78bab1eca02be498fec636a2233c46dd81334")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("81769694bda1d525a9d9b7f493e71ae816ce939e208561c5b8b1362d6a484ed8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("4e83c898973be409cf242c6dfd33a2576433f36f58f4000d6f120cc436edb5cb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("d0c84e94534042361f715a8e1cf9139745ab6804ab0b78289b4dd13c533c9930")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("cbdf8fca5d3a43e8fe06d2d56f969dbc46051439828211f73047b4800c9c5ed6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.12.8%2B20241219-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("1a702b3463cf87ec0d2e33902a47e95456053b0178fe96bd673c1dbb554f5d15")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("95dd397e3aef4cc1846867cf20be704bdd74edd16ea8032caf01e48f0c53d65d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("848405b92bda20fad1f9bba99234c7d3f11e0b31e46f89835d1cb3d735e932aa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("c8f5ed70ee3c19da72d117f7b306adc6ca1eaf26afcbe1cc1be57d1e18df184c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("d73cb8428a105d01141dee0ceec445328ab70e039e31cd8c5c1d7d226fb67afc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("04b3087272d2bb8df98eec5fe81b666052907f292381cbecce17bec40fdd30c5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("922aa21fb9eacdd1c0a26ced4dca2725595453ae5b922d56b39ebdd2388175fd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("8e92d65b245b572fa6f520d428a9807a9da36428c7379a11d41ae428e69ed921")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("3a4d53a7ba3916c0c1f35cbbe57068e2571b138389f29cf5c35367fec8f4c617")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("9314cb4d5aa525f2dc9f8d6ac204bebcfdfa8eb0dd4d3788af68769184355484")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("205395841e6e7cbd33d504b78ac81792364831911866416da7a34d9b4a06d7d1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("bfdd008fb669ddd023a8c36c16987c98b0d2bdfb99c9d45e2c9a792d87710b72")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("ad1d2bfccc7006612af93e1dbf6760ede5b07148141d0ca05a7d605ea666a55f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("2c11efdb7df78ed787ac67c094c58a75f6549a78660888d99a861fc08e88ebe2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("172c0ab8ac018a0b44a47f03a7a78cd583bdc1e60cd5cfbf7d05b269c5d73f5c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("4576e092f2eaa6f5685be048b50671f9df0fff2b261ee001f604fb983bc9bb71")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("d7d7c897f11f12808d3fd9a0ce48e4de19369df4a9ee9390a4adae302902e333")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7%2B20241016-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("fa8ac308a7cd1774d599ad9a29f1e374fbdc11453b12a8c50cc4afdb5c4bfd1a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("0419bafa4444a5aa0c554197bce0679e7cc0f28edc7ee8cfbe0ccea860bdb904")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("b10d19eb5548a3b3b0a5e6f9109834d7ecfc139bc15754f81a94d39eaa5bdd26")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("22d119ac7df7f0bddfd4dfd075bcc4eb2532ed3df0bdba0579106835d49ef9cd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("190c23eb3b9c6b9638f69dc7fb829df8967ad64c82e82c93898a4d878d18ed2a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("31a043c40e1dbb528404ff6e1fcad25638d54dfab2d379c3989d47ec24e6938b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("fb49374b512b0e9f2cd2a720b3836f8a04228d73eb0786e64221eb55979edc6e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("89be19666ecb7cdbbfd596e462d690a78a380f1fe5c2967b25a1779b0cec9339")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("b080463e4f0c452e592cdac1ca97936a6a19bb3d9a64da669a50ca843fce0108")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("661e2a4b03d6eccbb5b15f5bd2869fbdd39132513394d758287e46115e48d4ef")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("3ce9eb5d974dc2109c3e2c3986f4883ec5403b3479259ea781475a319f9a6787")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("6dab95cdd6d3f165aaaa7dcd7cb263add390d97c7b57eb62d2d1ca9a304353da")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("8dcc21cd45c09b273cca17c4b144c26c1a9e373eec871d540cc0762ab9c82c12")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("f2a295a6e400c16d6b3fe51c63c5b017e6c20a0a9e21a664170ecb6b9a104d8e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("debba8985df38a492e8fc4b7b8d2261567f56eae716aa7630b836523625c6a96")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("03c0ac05d6027b603ea0f5f9f2c463bd58f942706ae78dca27922b25dca8f16a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("d87275e613632ab738528fe20a94a7193e824e91ba7f1e7845e7fcfc1f114900")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240909/cpython-3.12.6%2B20240909-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("fe9898060f52c2171c2aa074f470f91339bdcf9896dae6709021c914f58aa863")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("90715cdab075e5a2680acf2695572d165b6269bdb5d1942ab577491478aea55f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("49a9f7ad41d62e0ece9e664ca5ae95f022e7b68eef48e8a6f11620ec9247c686")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("06e512178cb513658a01c054b3eafc649ca362ccbeb02a6ae8a55b02c1ba75ca")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("7a584de9c2824f43d7a7b1c26eb61a18af770ebd603a74b45d57601ba62ba508")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("a9992b30d7b3ecb558cd12fde919e3e2836f161f8f777afea31140d5fff6362e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("3bea081f4e6fa67e600a6a791bcfebb2891531ede2c21e23e1b7321b3369c737")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("2b6ea3a5242de99574191ee42df864756eca6d7cb1dbd4cd7ab2850ba8b828f8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("10680b593b5e31833218fd83104dee74af970a3463403a22bae613b952a34e8d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("e61b1274e1195f227cb30ba5d89ea32d743796d992adcaffad4819e4b0405d24")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("c0064dcd97e5a09b5ff9529fc65cf6fd7a8a20f71a7050f584ecaa0cc8c33f2d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("35ffd76881bef5ce09a123175f4f3419eb250802e60a6bb78649585325b8409f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("6414f1c4b0efb5c1f2eb30926c70519d01f46b1f0d97d407fe53e360865a4a23")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("302a26f5d35e1c9e9d82765aa7dbf5ceb609c402431ec97d7fa0e85d8247c023")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("d7e82fcf72ec5faedfbabf1084ca55f850971a559ba54350de312663570f4782")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("46771e859224aa128150160d568d5d19a69e0f9a8f27d110ea26c031b588c734")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("b1009d46b87330c099d02411ca5e9e333f13305c5abdbe20810a7c467cedb051")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.12.5%2B20240814-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("6eb0398795e8875575934cf21cdc9c7c7acddb46f9a52f91fdad509723f2f0e9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("ef6948e836f531bd7a58ffbe602803ff1c83c65f99d1da19be369ea61f136c93")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("9d68cbdd12d1d6f98d35cc76add232c12db75c6b7f49733bffc88e7b1c025a79")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("6c9cf13644edc7250525ab1b2529ba1c0fff56c0c5a5c2242d84b6d4889d2bea")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("5a23ed8eaf948fe48d7c05dbfb58ea8638dcd2c4880d8519e069281ab427cbcb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("4281764e69339a138e30211b9923d74036d07c7a56c6aacc6dbdb2802a575f51")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("35a8359f1dc17a7a70007dae102a5e1562c0715a721377ede92137b2a0292406")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("b2fd015ab3689e024de6fbb34a4942acdb54c2184d1963e22829aafa1d81ba2c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("ca076aee4329f53f988346eb0521ad2a2cf7f723b6296088d03b98d8f22f5420")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("de4983ffa610ff2c3b9bcb62882366f017d94bf11b194c1fce17ad9e502acce6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("798e562eb68b8d825c25c747b9995046b700c5bafbe8f7e558f41a3d8a57ceb7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("762dc23608111f49729f02c19fa8459219ac886e8694b79f541ef01141bf58c5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("681d65c59f48c4b1bfe29f08ca87a50620c9aef308e27ea11fbe2ef8ce1a3764")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("304bdcdf495529a6edcac34646e4f065f0963e4c3061cef9093b76bb0e8cca90")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("507fce8676d23af12f97b1f66efa876360d19fbe89675163a0c7c5d631763c84")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("8989f75d70f6c874f3031ba1841efd96089f589f04d445fbacd478a1f866c118")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("ff0fab24f38c22130e45b90b7ec10dc4ce9677b545d9fb9109a72d2ffbab7b02")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240726/cpython-3.12.4%2B20240726-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("6dd7b4607f8a25f0f5f68e745f4c572b1a20c3bbfa86accfa45b52ab93b18ece")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("ccc40e5af329ef2af81350db2a88bbd6c17b56676e82d62048c15d548401519e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("c37a22fca8f57d4471e3708de6d13097668c5f160067f264bb2b18f524c890c8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ec8126de97945e629cca9aedc80a29c4ae2992c9d69f2655e27ae73906ba187d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-armv7-unknown-linux-gnueabi-install_only.tar.gz",
        sha256: Some("f693dd22b69361c17076157889eb8f1ce1a5ea670c031fae46782481ad892a64")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-armv7-unknown-linux-gnueabihf-install_only.tar.gz",
        sha256: Some("635080827bed4616dc271545677837203098e5b55e7195d803e1dca7da24fc0c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c5dcf08b8077e617d949bda23027c49712f583120b3ed744f9b143da1d580572")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("872fc321363b8cdd826fd2cb1adfd1ceb813bc1281f9d410c1c2c4e177e8df86")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("a73ba777b5d55ca89edef709e6b8521e3f3d4289581f174c8699adfb608d09d6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("eb70814dc254f02714c77305de01b8ed2250c146320e22d0ed14b39021f89a8a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("b9b91f486e2a52b6cc392101245705d6ab5dd6ad4a4e2b3492baec8e4b96508b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("e47a8d25c2348bf9b0e818642e4dae90700fc3fbd96ffd7f4ad9518a7206d369")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("edb786bf15a92a7c40fc5ace2376736d73ff356458b7c24da0cd408f36b945bf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("9685fc300be464a9338e94e48fc7315ee0563999833f8c830f2a2dee2d750b7e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("790e70e565b3efc5f1d14294f7cc083d1fb2aa4c15074d547e8e6bb9d2adb70a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("c4d399923625ff5b5c20eceba0d8580c858768eff73dd9380aa8265030788850")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-i686-pc-windows-msvc-install_only.tar.gz",
        sha256: Some("bd723ad1aa05551627715a428660250f0e74db0f1421b03f399235772057ef55")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64-pc-windows-msvc-install_only.tar.gz",
        sha256: Some("f7cfa4ad072feb4578c8afca5ba9a54ad591d665a441dd0d63aa366edbe19279")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("01c064c00013b0175c7858b159989819ead53f4746d40580b5b0b35b6e80fba6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("a53a6670a202c96fec0b8c55ccc780ea3af5307eb89268d5b41a9775b109c094")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e52550379e7c4ac27a87de832d172658bc04150e4e27d4e858e6d8cbb96fd709")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("74bc02c4bbbd26245c37b29b9e12d0a9c1b7ab93477fed8b651c988b6a9a6251")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ecd6b0285e5eef94deb784b588b4b425a15a43ae671bf206556659dc141a9825")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("57a37b57f8243caa4cdac016176189573ad7620f0b6da5941c5e40660f9468ab")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("b428b4151c70b85339ac2659e5f69f7e47142d34a506e05ecd095efe2e3dec81")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("adfe5c1a6039b8806b3dee0aed5fe860540d55231be87df48891a7844279d76a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("e5c0adb646defd55791bbdd217f64d08a51855e818ee67e3ea6207521f92abee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("0ab408e31ecc30893020b617dd049af05b76cbe8bb8585b289f75557a24bcfd4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("bcf21a4105fe82cbefbd8bdf76dfc7eb98297b51ad02f0fc0a92929bcc95d6e9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("32ec4268b4d16a428deb642ddd875ae6e738b85558bfbdc4628018ff2e5b9e95")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("3b0e8c27c3a1dd339d8e6b53b3fc7faee379a70755093917d6ddca00253621c9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("1e919365f3e04eb111283f7a45d32eac2f327287ab7bf46720d5629e144cbff9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("1e5655a6ccb1a64a78460e4e3ee21036c70246800f176a6c91043a3fe3654a3b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("f93f8375ca6ac0a35d58ff007043cbd3a88d9609113f1cb59cf7c8d215f064af")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("eca96158c1568dedd9a0b3425375637a83764d1fa74446438293089a8bfac1f8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("236533ef20e665007a111c2f36efb59c87ae195ad7dca223b6dc03fb07064f0b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("78051f0d1411ee62bc2af5edfccf6e8400ac4ef82887a2affc19a7ace6a05267")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("60631211c701f8d2c56e5dd7b154e68868128a019b9db1d53a264f56c0d4aee2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("74e330b8212ca22fd4d9a2003b9eec14892155566738febc8e5e572f267b9472")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("876389f071d62ee9a4bdd7ce31e69c3cdd256fe498e4dd6bb2b80e674e7351fe")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("9dd6fc5a1326985896493d475e7eae0d07f6de0d932faef3c4b04bdd81b88c0c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("db45fc29b4c0389b23af6ccd2d1427c3200ed7db4a81e5fd3bfbd9a4fd011c41")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("a7bcc6c9f66dbd47ea99615f30f101c5d2dd0084ca333d2f7336e64050951338")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("2585d13ac6b3e18a24d31f4baea23a810f4c7eb37306c348eeee4dfb87f84866")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("510f3f3e1841bb0e236dfac8dbd6680a4990d60a060b6972978cbc89524d4736")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("6f34607bafc6104d294ae16be240856af6217555015665c34ff7e94ad33dfe29")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("13c8a6f337a4e1ef043ffb8ea3c218ab2073afe0d3be36fcdf8ceb6f757210e8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("fd5a9e0f41959d0341246d3643f2b8794f638adc0cec8dd5e1b6465198eae08a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("4734a2be2becb813830112c780c9879ac3aff111a0b0cd590e65ec7465774d02")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("5a9e88c8aa52b609d556777b52ebde464ae4b4f77e4aac4eb693af57395c9abf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("bccfe67cf5465a3dfb0336f053966e2613a9bc85a6588c2fcf1366ef930c4f88")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("b5dae075467ace32c594c7877fe6ebe0837681f814601d5d90ba4c0dfd87a1f2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("5681621349dd85d9726d1b67c84a9686ce78f72e73a6f9e4cc4119911655759e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e51a5293f214053ddb4645b2c9f84542e2ef86870b8655704367bd4b29d39fe9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("922f9404f39dc4edb8558a93cef5c3330895a4c87acb1de2a2cf662ab942dbe5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("6d7710c9a74f624d1fe60a5a01ed6db874659d906220b1d98a0a79a36bbcb2e6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("927de631f2f37b950c2ea8ca53dc36ce8389026f34c31add31c5f730e7227bae")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("dccac6b50581aba8f4ddceb4276589bcc602a672f2461e170076890f0c114444")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("28aa3b88cacb908ab57c4202010eebe4a18fad65b5c289d061aec841a0fbbdfb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("13f4c20d3277d0bff7b14125d4904bbf5c498fe14d550d31fd584b5beabe6e0f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("8c20230e3679d5b2d765c6cae2afead81207762c3a9c725249e7ba644f61d4f5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("6e4f30a998245cfaef00d1b87f8fd5f6c250bd222f933f8f38f124d4f03227f9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("facfaa1fbc8653f95057f3c4a0f8aa833dab0e0b316e24ee8686bc761d4b4f8d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("af82c85992dace78cd2682deb8e9ef41e448f66cb602f31e5b7c7af75a74bbf1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("d33b1ee09b2a9fe692cbc6e3ed0fbadb23c430233ce92e2954522c5594c6b274")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("23fa4e8eeb3a9c7fff540b0fdd4047ae5d2e6f0caf5095e7d767ed96ccc7efe7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("d798c008dfde19f0ffdf3684f49adaf922873bee2ad1a2e5dd8d0dd2ad3a4699")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("e838902cb7d47fbf8636d30b59d27182bf818dc633eef4b47c8d28163689b5ff")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("477dbc811b0f834bc3386126925d490ce734c2191a6e5022e1e0ebb346e5ddcf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("4e6a9f5111d08e0421446de539224addca624865d1d1e0c5c6ce7b2382d6cf11")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("a5448390ef50caaeac60c42806060d0d00488df0b61e7ab4df3b020cceb2bb73")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("916b2d4336d9f672b5b1f0f15b0ee43504ee178244ec6656ce49de57ddefaa76")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("267dbf5f9272bb2f8336d13b9f2468ff2b5659a5bf820afc564af49531dc676e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("f9678be5d8366faefcabe11ce9db63a3172176e1f3d6f910ec007453fb2e114d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("3746c4c74ec610e4425b876a74161ebcd01aaf556d99017254d782e975d35f46")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("6662013e83d5f97c673f6b346f062d29c5a6dda36a088a0deabfd5dcf6fb9b09")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("a93ebc464644990638ae3029c9bbea6249eb90047152dfb98cbaac66da1c199e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("5d01a991ad42ad49569145d86752aa79d881d49dd9503c8304ac9a7cea0b83c8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("063b3582b31d4b5c918439701ad4037cec9b90c8e2e4c55c502a9001c638ebc6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.11.11%2B20241219-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("f5bceb5efff7802e38638caf9dd748a88d7b67a4620d36f7511c2848b1eb83e3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("a5a224138a526acecfd17210953d76a28487968a767204902e2bde809bb0e759")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("575b49a7aa64e97b06de605b7e947033bf2310b5bc5f9aedb9859d4745033d91")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("9d124604ffdea4fbaabb10b343c5a36b636a3e7b94dfc1cccd4531f33fceae5e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("deb089a5ac0fbd9ad2e3dc843d90019ead75b1ec895fd57a5abca190ba86cb77")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("3655da6f1ccde823fc03f790bebfff106825e2b5ec4b733be225150275cd6321")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("cc16cf0b1a1aa61f4e90d38ccaad0b65085cea69d2dcc2c6281ef9d4e6cccdd8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("e8017e3b916f8c7b8fbdf2bd5fc18c6eb7ce2397df240fbeea84b05d4c7a37a4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("03f15e19e2452641b6375b59ba094ff6cf2fc118315d24a6ca63ce60e4d4a6e0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("5b33f0ff29552f15daacf81c426ed585fae24987b47d614142a7906eae6f2b04")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("64aefc042352e6bd10c4f600f1962af7dfec4586385f723c218b6369d3f211a2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("3714fec489d93c1ea458788a348d3380f98de22679a40a063e761753e7a48e71")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("ce94270c008e9780a3be5231223a0342e676bae04cb30b7554b0496a8fa7b799")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("a2778c2e5c8c48b555f935b4a7ff64f77f6ba0d1bdfb81b12d32905ddf7179cb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("56ed6aeed4795235b4f4349c4c0bf4ee81fdd00ad854eaedcccd5c43388d7545")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("c8e8f7a3fa9d797ec0957562f82eaba4dcadf04ef304ac616bdb6332c725a609")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("0a5b423517722e9868ac4a63893f24f24db9bd67e8679e6e448343c5829d2e77")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10%2B20241016-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("ea770ebabc620ff46f1d0f905c774a9b8aa5834620e89617ad5e01f90d36b3ee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("c4e2f7774421bcb381245945e132419b529399dfa4a56059acda1493751fa377")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("c8680f90137e36b54b3631271ccdfe5de363e7d563d8df87c53e11b956a00e04")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("364cf099524fff92c31b8ff5ae3f7b32b0fa6cf1d380c6e37cf56140d08dfc87")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("e64d3cf033c804e9c14aaf4ae746632c01894706098b20acbf00df4bd28d0b0e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("7630838c7602e6a6a56c41263d6a808a2a2004a7ea38770ffc4c7aaf34e169ae")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("2387479d17127e5b087f582bac948f859c25c4b38c64f558e0a399af7a8a0225")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("30c71053e9360471b7f350f1562ff4e42eb91ad2ca61b391295b5dea8b2b9efd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("daa487c7e73005c4426ac393273117cf0e2dc4ab9b2eeda366e04cd00eea00c9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("b3e94cbf19bd08bf02f6e6945f6c2211453f601c7c6f79721da63a06bf99b1f9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("6f579d9b2ec635b7cc4eb983719ae8b4ee34248f2054939cc3b1b23b44c65c60")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("d776c6bfd09719f0a18fe23e25b314fed8e32c0c0474c21524020d5481bdcda1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("d6eeb714389614fa954f49e5ec4323f18eccbc700c516521c1297860364226cf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("e8ed1a00acd996590056c8e00351038fd1ecf9910428f592989410f06a765eef")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("92247f338ba132e9ff6e4b0dffc2eacfab6958552b0354e623f5016c5e83bafd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("adb10e3c2eb598560c14a375b34776d80bc83656e571f8ddc2b882630db8cd4d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("091c99a210f4f401a305231f3f218ee3d5714658b8d3aac344d34efc716dff85")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.11.9%2B20240814-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("8ac54a8d711ef0d49b62a2c3521c2d0403f1b221dc9d84c5f85fe48903e82523")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("389a51139f5abe071a0d70091ca5df3e7a3dfcfcbe3e0ba6ad85fb4c5638421e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("097f467b0c36706bfec13f199a2eaf924e668f70c6e2bd1f1366806962f7e86e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("389b9005fb78dd5a6f68df5ea45ab7b30d9a4b3222af96999e94fd20d4ad0c6a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("eb2b31f8e50309aae493c6a359c32b723a676f07c641f5e8fe4b6aa4dbb50946")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("844f64f4c16e24965778281da61d1e0e6cd1358a581df1662da814b1eed096b9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("94e13d0e5ad417035b80580f3e893a72e094b0900d5d64e7e34ab08e95439987")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("08e1ebf51b5965e23f8e68664d17274c1cdabb5b2d7509a2003920e5d58172c7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e4f70dcb40acc2342d63103808a19f728a1c1dc9e0fad5344061daaab03a4ce5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("a3f12605552d5450cfc8fe6c8fc468f99a42b2ae33f1ef42d5f49356d66ad305")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("52b3e24b08e53e5098561a13a61e28d241231331fd903dcb2a1e4161f3753dc1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("417890e151d07f9242d0b7ed2e16b7fee59b6606cd133105bba234f14823f8ce")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e363cc041a4464bd729771d6d223f3ec13c1e76dacdedc207ad1f6fb777bcb71")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("9d430c32970e6b0f7cf030ed69dc63ee7b198543ef1e818c5a074c34052fa9e4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("75039951f8f94d7304bc17b674af1668b9e1ea6d6c9ba1da28e90c0ad8030e3c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("b618f1f047349770ee1ef11d1b05899840abd53884b820fd25c7dfe2ec1664d4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("b042c966920cf8465385ca3522986b12d745151a72c060991088977ca36d3883")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("a0e615eef1fafdc742da0008425a9030b7ea68a4ae4e73ac557ef27b112836d4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("b102eaf865eb715aa98a8a2ef19037b6cc3ae7dfd4a632802650f29de635aa13")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("b44e1b74afe75c7b19143413632c4386708ae229117f8f950c2094e9681d34c7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("49520e3ff494708020f306e30b0964f079170be83e956be4504f850557378a22")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("4a51ce60007a6facf64e5495f4cf322e311ba9f39a8cd3f3e4c026eae488e140")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("1a919a35172eb9419eba841eeb0ec9879dbc2b006b284ee5c454c08197b50f74")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("2cdd399100e647aa9d381e197e6a8c98e822ecb8fc1b6bda4b1eb554dbfb8177")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("e9387d628e6f49587206655e61295cdbfea43944c4259c24566fa19e0596ba4e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("08dd57796b8fcc2a7307ed4dbe7a69cf35856cb29a5c79f827fe08a0663c227f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("eec99f0614e8250027e8f72b535776b44e0ad74e24cba9716942cd4c6e08844c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("28590cf568f192dbc5c91814321bca8bfe749cdf5e60a2aad968a0ae74d6bb4a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("3e3b2d4831c6353ae68808a517d59425addd857dbf925958db11b3280f46ed1e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("f5a6ca1280749d8ceaf8851585ef6b0cd2f1f76e801a77c1d744019554eef2f0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("67077e6fa918e4f4fd60ba169820b00be7c390c497bf9bc9cab2c255ea8e6f3e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("916c35125b5d8323a21526d7a9154ca626453f63d0878e95b9f613a95006c990")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("178cb1716c2abc25cb56ae915096c1a083e60abeba57af001996e8bc6ce1a371")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("3e26a672df17708c4dc928475a5974c3fb3a34a9b45c65fb4bd1e50504cc84ec")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("7937035f690a624dba4d014ffd20c342e843dd46f89b0b0a1e5726b85deb8eaf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("f9f19823dba3209cedc4647b00f46ed0177242917db20fb7fb539970e384531c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ee37a7eae6e80148c7e3abc56e48a397c1664f044920463ad0df0fc706eacea8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("c929e5fe676ad20afcf6807a797d21261ae0827e84ec18742031a9582aed0d46")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("d93961f7d6df53f5e888ce070b92d19a7fce588bb03abfac2b6f3c5bf2923c80")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("3bb96293951e613fc3f36e7132dcfc34190e5ed634448b12dd1e496723bedbd0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("9a2f2bb9fca7f31502e102306fbeed8ceb7f634f4376a08f9f630c1982f62fcb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("99afdac2a24966c828faf6f13c8c100c6c432d1c888450cfadb39dbc7a7562f1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ea850efb2de01c9389580ea0a6f55c7271dd3028b31fe0cca3ff9716fb580879")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("ebd862470d4f66da160f9a65fc058d4feff3822a97318ccc5c3765a99e0ba439")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("dd48b2cfaae841b4cd9beed23e2ae68b13527a065ef3d271d228735769c4e64d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("3933545e6d41462dd6a47e44133ea40995bc6efeed8c2e4cbdf1a699303e95ea")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("dab64b3580118ad2073babd7c29fd2053b616479df5c107d31fe2af1f45e948b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("4a4efa7378c72f1dd8ebcce1afb99b24c01b07023aa6b8fea50eaedb50bf2bfc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("bb5c5d1ea0f199fe2d3f0996fff4b48ca6ddc415a3dbd98f50bff7fce48aac80")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("82de7e2551c015145c017742a5c0411d67a7544595df43c02b5efa4762d5123e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("14121b53e9c8c6d0741f911ae00102a35adbcf5c3cdf732687ef7617b7d7304d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("fe459da39874443579d6fe88c68777c6d3e331038e1fb92a0451879fb6beb16d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("fbed6f7694b2faae5d7c401a856219c945397f772eea5ca50c6eb825cbc9d1e1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("fe09ecd87f69a724acf26ca508d7ead91a951abb2da18dfb98fe22c284454121")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("6f25769f73827cfc9f80114dbbc04fa959e96f82bf1b1297bc56ae08afaa6a90")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("f442249bea0a61dfec2911d86aa49f4c047eabfa2322914d040f2377c1228b48")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("5b975dd6a4648d16f278e9d82027f29feb01eda6009b239d7a7c69f421ebd519")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("41e2068884c86fc6a51670b12de140e12d6eb849bb450a970c77798f568aef72")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("9759ce08bb96716f26aedd4e4d5879f810d9ca1e6f185d9881910837ae66cd29")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("568e7a24d390fcd91b7524869c76c4e03dac9940692b381c29ae005651c5f1d7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("936b624c2512a3a3370aae8adf603d6ae71ba8ebd39cc4714a13306891ea36f0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("00f002263efc8aea896bcfaaf906b1f4dab3e5cd3db53e2b69ab9a10ba220b97")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("cb6d2948384a857321f2aa40fa67744cd9676a330f08b6dad7070bda0b6120a4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("47e1557d93a42585972772e82661047ca5f608293158acb2778dccf120eabb00")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("2e84fc53f4e90e11963281c5c871f593abcb24fc796a50337fa516be99af02fb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("abdccc6ec7093f49da99680f5899a96bff0b96fde8f5d73f7aac121e0d05fdd8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("df7b92ed9cec96b3bb658fb586be947722ecd8e420fb23cee13d2e90abcfcf25")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e477f0749161f9aa7887964f089d9460a539f6b4a8fdab5166f898210e1a87a4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e26247302bc8e9083a43ce9e8dd94905b40d464745b1603041f7bc9a93c65d05")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("1218ca44595aeaf34271508db64a2abc581c3ee1eb307c1b0537ea746922b806")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("dd1530d8a2e002f68e3d7ed1aa568a0e9278a5c87ba1f2ec4b9bae75777a6bc2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("9bea574e0f01eda7154ab1972a3d413c462a731568b5923ed2ae30174167a408")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("f6882a821a02c8f727fce12044f8ed03c0814c68c483e9c074b7d62e8aaf3adf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("d8a4a5d572e163eca53f83c91fca78955a92ec5cd2c9713e0e51ad7901add798")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("3802bf8c6f305b7b841dbbf1b091d9820d9bd65e9f5b246d2d071c42baa80fec")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("c50912dce8107bd06c93cd73f96c4bfec0714fc63ba9246c8726b253443e21a7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("e2f4b41c3d89c5ec735e2563d752856cb3c19a0aa712ec7ef341712bafa7e905")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("878614c03ea38538ae2f758e36c85d2c0eb1eaaca86cd400ff8c76693ee0b3e1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("09e412506a8d63edbb6901742b54da9aa7faf120b8dbdce56c57b303fc892c86")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("f710b8d60621308149c100d5175fec39274ed0b9c99645484fd93d1716ef4310")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("8190accbbbbcf7620f1ff6d668e4dd090c639665d11188ce864b62554d40e5ab")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("36ff6c5ebca8bf07181b774874233eb37835a62b39493f975869acc5010d839d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("767d24f3570b35fedb945f5ac66224c8983f2d556ab83c5cfaa5f3666e9c212c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("da50b87d1ec42b3cb577dfd22a3655e43a53150f4f98a4bfb40757c9d7839ab5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("82eed5ae1ca9e60ed9b9cac97e910927ffe2e80e91161c74b2d70e44d5227de0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("d0191c35051ade259d3324f437c6f2422743ccb79197af6dc64c161b06eddca9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("9715fc26b9a6de09ebc29c978ce1bea81c0a64f730125f1230743c18f536de27")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("6452fe315b5240040acffc5688e97fc264d9eb8fbfdd90c6ede0bc46b20640e0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("9ea67df4e6cafabb6cee6adcc5f863708b1e717caad47be105eb4eb7170eca11")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("5bc5eb3892957c0a9eae87fc3ce97f2b0b31406fd6ef1d20bfa8074a44121cb4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("7b7645a773cb0660b0f0c807af580b9b4a49d49de39b89d10ccfb01372e5a573")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("a6751e6fa5c7c4d4748ed534a7f00ad7f858f62ce73d63d44dd907036ba53985")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("24741066da6f35a7ff67bee65ce82eae870d84e1181843e64a7076d1571e95af")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("4918cdf1cab742a90f85318f88b8122aeaa2d04705803c7b6e78e81a3dd40f80")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("20a4203d069dc9b710f70b09e7da2ce6f473d6b1110f9535fb6f4c469ed54733")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("debf15783bdcb5530504f533d33fda75a7b905cec5361ae8f33da5ba6599f8b4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("8392230cf76c282cfeaf67dcbd2e0fac6da8cd3b3aead1250505c6ddd606caae")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("02a551fefab3750effd0e156c25446547c238688a32fabde2995c941c03a6423")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("7f0425d3e9b2283aba205493e9fe431bc2c2d67cc369bc922825b827a1b06b82")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("cc322d17b9ead6aeee4ac116fa2f802d525b4d97343fac8b8ada458810b47b40")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("0e7dae505cdb31bee4095f7c004a9f89d78263d6f24fe32eb225bfbc34d7f6ab")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("a7b538f35630a17ea8b5e1703b38906da189b2d6054297b571c7f5e81fc953a0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("ae7add6ad69f9a1852fed17ca04ebe3412bf5f190e008d8cfbe94bfe21445c48")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("dcee403d2f3416c0a7beae2fe58d9ca5646bb73ae47c0431d43911a0a8581a62")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("257d4ca1a7167f0b1d65900ca27d0d0f747e1e10bd27fbadeaa4a8bf0bf5b5e3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("50b250dd261c3cca9ae8d96cb921e4ffbc64f778a198b6f8b8b0a338f77ae486")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("edc08979cb0666a597466176511529c049a6f0bba8adf70df441708f766de5bf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("a5a1eb1f13a688621af748d552888d7b22daaddfe6b9c7069f9c955c0fcf2da3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("27bc1b56a5268f79a9eb9bf79f8281b336f8bfffc361032f41811bc3c58cdb1d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("85d1ba2ada13fdbeedbaad24e1871d49766be1f4ba6cdee3c16ca7c4197cca92")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("61f76e2d924b4c6cdd706d322bdda9634bec4d41e50071a03c7e1f7a5178aef2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("68ff1232aeba789195f6061c6c0e2e6868c4a0c8547f2aa7a873c4226bf04a3a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("57282eba8bccf723af7edf0e018407ea5a035deed9daa5fe684c980d4dd2aebc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("d2048f722562ff30f268e89310f9e567508d3dd6e071ced4c04b4a4aceb6236c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("ec5ff780e53c9af0399086f17299a88e1dd3de707940eab76fa6ef15642edcbd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("9a6f91e3166e36aceb1dfc1f2246f4253b62de7ca0ee0534282e0ded229ab1f2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("99e51551b920a73066ca0e6b4139c1e47cecb0bd97711d8efaba819c9c3cf5ed")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("bcd8a5c316e1e6727bb891d1b0ac58f3cf0b492a1ca55db4d7b54a8655a3c52c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("bb7cc15b0870979a101d887ec6ba5ed7ac38063a1a02201b709218a32f6151fa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("37b96735ec9776fb40a17d22761d9ed5b4d42ff891cfb3d8ef9ac08411e9051a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("344b34b0c8abd2e536c4b16ab38773e4f4b6c490000a4df162d9181fb396de7c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("be3471934543269246ce9b5e33552d9480fffa038296a5c7875d6ca194ddda5d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("56462270fd467a3cb2a353a6b6b7c8cd7b2734513dfa87638313db6e769407af")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.10.16%2B20241219-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("6121810d6899d1bfd89a1c3b7fba3ece4402a7f49772a3ebef6b4c707bcf22ec")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("fa79bd909bfeb627ffe66a8b023153495ece659e5e3b2ff56268535024db851c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("0d952fa2342794523ea7beee6a58e79e62045d0f018314ce282e9f2f1427ee2c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("6008b42df79a0c8a4efe3aa88c2aea1471116aa66881a8ed15f04d66438cb7f5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("38daa81e0cbdc199d69241c35855dd05709f8246484cfe66b84666e123abb7df")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("af28aab17dd897d14ae04955b19be3080fbaa6778a251943d268bc597ac39427")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("4b86196b928b51ef3a0d51aa1690236e3da4561e34254e2929c0fcd37b37a002")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("fbac57f67ca8a684f0442ff73c511efc177850c48f508f23521a816eae34d75f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("25fb8e23cd3b82b748075a04fd18f3183cc7316c11d6f59eb4b0326843892600")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("a169bdcd98f62421062fb9066763495913f4a86ee88c7d36e51df86d5d3cbe62")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("a85f3481c8117a11b5aa4fda0a6eb174b54e97b122cc8f83cf773a876751785b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("3f78660fd377d6073fa0ab642c20729c16fbc74fd67f8f35688e648fee014f4a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("f36b7ad24ead564455937ff8841a3ec16a194d9eb8411ed0470a0fbd627c683e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("75df2a6b3845ce3e4aa556276d65288e509b17bc35ebb462a60230a8eff5039c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("7d616298bffd2e4ffd0e72ab3786f2e4b9759dcd59a8cf7ac00f74a8642d5537")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("b343dff67fc9a6a03943b2bb2d6eb7d15f693533c706cc31f887d7db2625a70d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("976d1560a02f2b921668fafc76196c1ff1bb24ccaa76ed5567539fb6dab0aa5a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15%2B20241016-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("45a95225c659f9b988f444d985df347140ecc71c0297c6857febf5ef440d689a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("f7ca9bffbce433c8d445edd33a5424c405553d735efee65a2fc5d8bbb1c8e137")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("4404f44ec69c0708d4d88e98f39c2c1fe3bd462dc6a958b60aaf63028550c485")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("0ffe64c77cacda7e3afcb0d8ba271c59ca0a30dfda218da39a573b412bb4afd7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("451449f18a49e6ceecf9c1f70f4aee0d1552eff103c3db291319125238182c9d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("7f215b85df78c568847329faeb2c5007c301741d9c4ccebbd935a3a2963197b5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("8b83fdd95cb864f8ebfa1a1dd7e700bb046b8283bfd0a3aa04f1ff259eaff99e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("ff1c4f010b1c6f563c71fa30f68293168536e0ed65f7d470a7e8c73252d08653")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("159c456bb4a3802bafbce065ff54b99ddb16422500d75c1315573ee3b673af17")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("8803a748f2197ec2360af6feebe9c936f4f6beabcae1db5557fdd98fc922982c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("327049baeb8319f0a31a14d28fce5608af546c27fb09994f56e3c0e17efb48c3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("cc2ed06f898466616c6534de1fd3b00d02fae415274ecb7210f5199b2550c38c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("94d7bf472551494c75a122595b30f798b3785b3fcce319e1a66f7b7358399c15")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("c3177e9c8a9b339f33fa18ca896d59dbe22853cb88be3d844ac7859c543b5913")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("5f0b093b83d5dad323463269d2cd53516e51ef0f5e965001ac8947450c3fc917")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("845008e1afab23633b1f8f49323d8de15f24cdda523974df2bab2d080af5bfbf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("a84742f13584fd39f4f4b0d9a5865621a3c88cad91b31f17f414186719063364")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.10.14%2B20240814-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("61ad1abcaca639eecb5bd0b129ac0315d79f7b90cf0aca8e9fb85c9e7269c26b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("5fdc0f6a5b5a90fd3c528e8b1da8e3aac931ea8690126c2fdb4254c84a3ff04a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("6378dfd22f58bb553ddb02be28304d739cd730c1f95c15c74955c923a1bc3d6a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("a898a88705611b372297bb8fe4d23cc16b8603ce5f24494c3a8cfa65d83787f9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.10.13%2B20230826-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("424d239b6df60e40849ad18505de394001233ab3d7470b5280fec6e643208bb9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c23706e138a0351fc1e9def2974af7b8206bac7ecbbb98a78f5aa9e7535fee42")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("09be8fb2cdfbb4a93d555f268f244dbe4d8ff1854b2658e8043aa4ec08aede3e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("d995d032ca702afd2fc3a689c1f84a6c64972ecd82bba76a61d525f08eb0e195")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("48365ea10aa1b0768a153bfff50d1515a757d42409b02a4af4db354803f2d180")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ee43e708b6c4c1ffb5b17b41c51305672e3af2fd686960e4e024a86f969c1a1e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("779be1530f099b4ce0b87fe03f4621e6edca926393c7c857d581f50ffc4df66b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("325b708dccba08446658137517630c1a9fe38519780d516a760597e79a9d8d69")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("13d11d53372a058a9df956057d75e14ac229cce8daae6c42d97e742fed63f978")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c1cdc034c483794a4792571b44318f7608769b5b2d116635723e4bd702b5ea69")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("c1e61caff30433105aad15ea47ddc20b5ced4ccafd8e3666fa85667777354584")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("5365b90f9cba7186d12dd86516ece8b696db7311128e0b49c92234e01a74599f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("086f7fe9156b897bb401273db8359017104168ac36f60f3af4e31ac7acd6634e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("bc66c706ea8c5fc891635fda8f9da971a1a901d41342f6798c20ad0b2a25d1d6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("8a6e3ed973a671de468d9c691ed9cb2c3a4858c5defffcf0b08969fba9c1dd04")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("fee80e221663eca5174bd794cb5047e40d3910dbeadcdf1f09d405a4c1c15fe4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c7a5321a696ef6467791312368a04d36828907a8f5c557b96067fa534c716c18")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("bb5e8cb0d2e44241725fa9b342238245503e7849917660006b0246a9c97b1d6c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("8d33d435ae6fb93ded7fc26798cc0a1a4f546a4e527012a1e2909cc314b332df")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("a476dbca9184df9fc69fe6309cda5ebaf031d27ca9e529852437c94ec1bc43d3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("9080014bee2d4bd1f96bcbebf447d40c35ae9354382246add1160bd0d433ebf7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("843504ad0f655f19614f2edb689808ed5fd2712bbf5a0eb0331ff1ebf871d457")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("d5d33effa6d64c83392f12b63fdf122843d943c7cfaa05ab734677699241cf93")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("602e8eb7cac2921a9aa12938e9d69ef139797f6952f285ac8522a0502616a137")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("bc17cb3b279a031cbcb2427238254758e89ac44541a95a5ddec968bc13ff0181")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("7cc21bad92259bf08d60cbe0650133d55f461f102a8ac9908dbd23400d9b35fa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("1eef14ac488ab43ccf54144697d6892645414fc4926cd1456136cd427a8ae454")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("a5a5f9c9082b6503462a6b134111d3c303052cbc49ff31fff2ade38b39978e5d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("c1a31c353ca44de7d1b1a3b6c55a823e9c1eed0423d4f9f66e617bdb1b608685")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("8348bc3c2311f94ec63751fb71bd0108174be1c4def002773cf519ee1506f96f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("bd3fc6e4da6f4033ebf19d66704e73b0804c22641ddae10bbe347c48f82374ad")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c7573fdb00239f86b22ea0e8e926ca881d24fde5e5890851339911d76110bc35")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c70518620e32b074b1b40579012f0c67191a967e43e84b8f46052b6b893f7eeb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("73a9d4c89ed51be39dd2de4e235078281087283e9fdedef65bec02f503e906ee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c5bcaac91bc80bfc29cf510669ecad12d506035ecb3ad85ef213416d54aecd79")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("c5dde3276541a8ad000ba631ec70012aa2261926c13f54d2b1de83dad61d59c1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("7e6281f9ff93ac54c38abfffe874907591ae6db431df7633b73fa5e62066582c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("bc168048359070be77bf7cf03af1e4cf8c9144d348e59bd5849ebf046694462c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("d94e9958580586c9ff95a35b2e1c5afde7087b9bb0f32d3d141a5a7ec2b8a50b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("862119d9b37def3cba4ceaf10b7415594bc171b4aa6c057e70433349ecb33474")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("488df9d6af3baca3fd2b26005e5b1a0e29b53c602bc6ecfa4f21d20715107fba")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("35db9299916982e0b9fbf483ae15b7a4b90a418c857ded61be241d81acbc9779")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("e4ed3414cd0e687017f0a56fed88ff39b3f5dfb24a0d62e9c7ca55854178bcde")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("9c2d3604a06fcd422289df73015cd00e7271d90de28d2c910f0e2309a7f73a68")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("018d05a779b2de7a476f3b3ff2d10f503d69d14efcedd0774e6dab8c22ef84ff")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("0e685f98dce0e5bc8da93c7081f4e6c10219792e223e4b5886730fd73a7ba4c6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("2003750f40cd09d4bf7a850342613992f8d9454f03b3c067989911fb37e7a4d1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("44566c08eb8054aa0784f76b85d2c6c70a62f4988d5e9abcce819b517b329fdd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("d196347aeb701a53fe2bb2b095abec38d27d0fa0443f8a1c2023a1bed6e18cdf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("cf17e6d042777170e423c6b80e096ad8273d9848708875db0d23dd45bdb3d516")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e69b2e48696f1e19231bed30616aa4ac6ef0fb4b3760e9e525e2c9c4c30ddb61")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("89d13b06e73bd347e46b1424f8c56823e13920cc3cdb32a75c247852cbc84f76")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("062b42b9939908f3587f112769373be3d773f0d965ea1a08c65572e2551a2af4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("e7c53f1fe15ecba006276b41c3e1947c19ae3af4d53b6fae35f86b210db4e4df")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("2c40f64d283d05755b531247eaeb6172cde6d31ba6c46cfa106b97b4669cea88")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("8dae0e8c598003e84351a2559d87b44335d02317eb82c75ea13c51aac5891a3f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("c5c51d9a3e8d8cdac67d8f3ad7c4008de169ff1480e17021f154d5c99fcee9e3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("59c6970cecb357dc1d8554bd0540eb81ee7f6d16a07acf3d14ed294ece02c035")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("d52b03817bd245d28e0a8b2f715716cd0fcd112820ccff745636932c76afa20a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("525b79c7ce5de90ab66bd07b0ac1008bafa147ddc8a41bef15ffb7c9c1e9e7c5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("33170bef18c811906b738be530f934640491b065bf16c4d276c6515321918132")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("2deee7cbbd5dad339d713a75ec92239725d2035e833af5b9981b026dee0b9213")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("6c8db44ae0e18e320320bbaaafd2d69cde8bfea171ae2d651b7993d1396260b7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("9f035bbe53f55fb406f95cb68459ba245b386084eeb5760f1660f416b730328d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("81eaeef0c63349f296d6c9fed6bb6ff3f2f29db649f0725351e6af147413cc90")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("efe550859e00a15325b6c4fff0d5df23ae08017ba8f74616bc0e9e73f899871d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("06a2fc7fa664301bf2657144650d2a655f1f2557861bc5684315df0d2b54d671")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("51ebd0a996a750d9101791d5b7789237f7262e4381c21ee5b87e3a2dd88b7628")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("b2617ece01070eeaa37fd5feaf1b5835435bd5151386218e6383cf9ee57bb4d2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("c641d68243082094f4afbda99eee7da5226906aa7bfca5859044c217e7133db6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("94e76273166f72624128e52b5402db244cea041dab4a6bcdc70b304b66e27e95")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("f2b6d2f77118f06dd2ca04dae1175e44aaa5077a5ed8ddc63333c15347182bfe")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("70f6ca1da8e6fce832ad0b7f9fdaba0b84ba0ac0a4c626127acb6d49df4b8f91")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("6101f580434544d28d5590543029a7c6bdf07efa4bcdb5e4cbedb3cd83241922")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("dfeec186a62a6068259d90e8d77e7d30eaf9c2b4ae7b205ff8caab7cb21f277c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("4a611ce990dc1f32bc4b35d276f04521464127f77e1133ac5bb9c6ba23e94a82")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c12c9ad2b2c75464541d897c0528013adecd8be5b30acf4411f7759729841711")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("3e0cab6e49ad5ef95851049463797ec713eee6e1f2fa1d99e30516d37797c3f0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ae5ad8fe84d8b1fd2e6fe2f47a632c9a1244686c12ce98cdd4a553be1fda2f9e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("4af5e7eb30ae33db0729c019cbca4d76c973dac27167ad4131902078531eb05b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("a85919da343455cd22f8c5cf2e748498222ae92f4dcd8f37a366fec668622cbe")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("1cd4072435039557c0b6f528f18865b4d4d36c807d51abd69a6a43c73c26e9af")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("f7c803a2ecd63dfc89210e1b6dffb0c509f2680ec9484e824c2d3a14f91a7803")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("2bef222e2469cbb4e8cac98a7b31f92b7dcd9672ec4db714fe6fe0090806ad53")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("384e711dd657c3439be4e50b2485478a7ed7a259a741d4480fc96d82cc09d318")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("b464352f8cbf06ab4c041b7559c9bda7e9f6001a94f67ab0a342cba078f3805f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("efaf66acdb9a4eb33d57702607d2e667b1a319d58c167a43c96896b97419b8b7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("7718411adf3ea1480f3f018a643eb0550282aefe39e5ecb3f363a4a566a9398c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("81625f5c97f61e2e3d7e9f62c484b1aa5311f21bd6545451714b949a29da5435")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("b152801a2609e6a38f3cc9e7e21d8b6cf5b6f31dacfcaca01e162c514e851ed6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("55aa2190d28dcfdf414d96dc5dcea9fe048fadcd583dc3981fec020869826111")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("8cafe6409e9d192b288b84a21bc0c309f1d3f6b809a471b2858c7bf1bb09f3a7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e5478adc739ecd6f7b770e613fa0e5757f13fe9cbeb34f7990e5522b0593d6db")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("6097200a90aa57ad5fe8d4446f4f09d6653b3db6dd1fec80f96cbeafcf76de12")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("2601630fef417aaeb1d8d07949e81f6390e439cdd2a347ec90218731bbd24b25")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("9d36dcbd9b47b3205dc5b1371a3839a5ad5317e7ba5fa353bac326635717a3a0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("327533dae8332ee61ed87dd5b7cfee3188a25a9fde65c3e27865185cb3e79b2b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("c09bd9d306cd1eae99ed1de66313b0b2617b63a9cafec5e9fa3c5dd82e2d43ca")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("27f22babf29ceebae18b2c2e38e2c48d22de686688c8a31c5f8d7d51541583c1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("91889a7dbdceea585ff4d3b7856a6bb8f8a4eca83a0ff52a73542c2e67220eaa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("19d1aa4a6d9ddb0094fc36961b129de9abe1673bce66c86cd97b582795c496a8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("eca0584397d9a3ef6f7bb32b0476318b01c89b7b0a031ef97a0dcaa5ba5127a8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("012fa37c12d2647d76d004dc003302563864d2f1cd0731b71eeafad63d28b3f0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("5abf5baf40f8573ce7d7e4ad323457f511833e1663e61ac5a11d5563a735159f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("460f87a389be28c953c24c6f942f172f9ce7f331367b4daf89cb450baedd51d7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("6aad42c7b03989173dd0e4d066e8c1e9f176f4b31d5bde26dbb5297f38f656d0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("81e0c857175472f62641c49186f637cd1a391e4741b43afff5f55688512de1f1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("febb7ebb1df876145be4f25a1caabbb8ec05758950e7efdebc33587f7ad3ca3c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("462f88de88b450bf80e8e660610adfd7d0ee2e409270ec406e3584093d2262ce")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("f28cfb4da443626267792c81b1ac4919765782d38757b0e90daf2db525502615")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("cab2739ef27fcb7920dfbc99e57f2aec1256146a4646cf01488c60a267ed9ea3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("5864ccaefdb3f6372f0b59f9c823e5925526c4064a94afad2bcdc9cf919e9365")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("2846e9c7e8484034989ab218022009fdd9dcb12a7bfb4b0329a404552d37e9aa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("c830ab2a3a488f9cf95e4e81c581d9ef73e483c2e6546136379443e9bb725119")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("6d2e4e6b1c403bce84cfb846400754017f525fe8017f186e8e7072fcaaf3aa71")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("c4a57a13b084d49ce8c2eb5b2662ee45b0c55b08ddd696f473233b0787f03988")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("7a8989392dc9b41d85959a752448c60852cf0061de565e98445c27f6bbdf63be")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("f3bc0828a0e0a8974e3fe90b4e99549296a7578de2321d791be1bad28191921d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("1f8423808ad84c0e56c8e14c32685cbfbc1159e0d9f943ac946f29e84cf1b5ee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("74c8da0aa24233c76bdd984d3c9e44442eca316be8a2cb4972d9264fedb0d5e8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("a94db687c197ac8fd9ade2695fe93ce9da392818ed52980459c25c7d39bcd73f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("c2bfbee97c9a4c4f5f5722ea68ea1a4d9f0e2510d02919af0f989d3061c66b3f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("96fc13a16df730afead8c17812e65e2aad53d64d57e3180ef5169cda035f33e4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("a750ac0ba4c23de6ab3ce494fd23e9b0e47dbf961e1fd29cb543ca60e200ba44")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("12b5c2f325e99499fa6d79fb9cef3562f8a7635917353f2a79d4c30ac529e7a0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("70bcf588b9f9b7a6aca236ff8214f17b123caa5da039ecc00ddee4828e9cc1bb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("e1dfa5dde910f908cad8bd688b29d28df832f7b150555679c204580d1af0c4a6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("7231ba2af9525cae620a5f4ae3bf89a939fdc053ba0cc64ee3dead8f13188005")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("db46dadfccc407aa1f66ed607eefbf12f781e343adcb1edee0a3883d081292ce")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("ec2e90b6a589db7ef9f74358b1436558167629f9e4d725c8150496f9cb08a9d4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("f52ee68c13c4f9356eb78a5305d3178af2cb90c38a8ce8ce9990a7cf6ff06144")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("2f125a927c3af52ef89af11857df988a042e26ce095129701b915e75b2ec6bff")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("b9989411bed71ba4867538c991f20b55f549dd9131905733f0df9f3fde81ad1d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("a60b589176879bdd465659660b87e954f969bed072c03c578ec828d6134f4ae1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("56b36066cda53fb371d92e769b7dc4b18df2d857930871946052d6be724febce")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("f2cd3e6824d768603d6adc911aeb31de87f12bff9d3dc93624e944c6312acbdf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c14205c1de8febc05190c879dd4e3eac67ce9afdae962ec00bd768279b262b18")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("e8ff9e1db06889cc65324a878b3d1e49de1213766a0401345b486761721ea125")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("04df195d33f88ce2e1160416951eb772e9c61528aad1d15c508d9fd11041fc59")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("7ad8fdc6f0d26156bc350533a9af7bf582230be8ad6935934911fd8afd184085")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("bb7f2a5143010fa482c5b442cced85516696cfc416ca92c903ef374532401a33")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("ba593370742ed8a7bc70ce563dd6a53e30ece1f6881e3888d334c1b485b0d9d0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("1409acd9a506e2d1d3b65c1488db4e40d8f19d09a7df099667c87a506f71c0ef")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("8146ad4390710ec69b316a5649912df0247d35f4a42e2aa9615bffd87b3e235a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("8f351a8cc348bb45c0f95b8634c8345ec6e749e483384188ad865b7428342703")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("4fa49dab83bf82409816db431806525ce894280a509ca96c91e3efc9beed1fea")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("9b64eca2a94f7aff9409ad70bdaa7fbbf8148692662e764401883957943620dd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("c4f398f6f7f9bbf0df98407ad66bc5760f3afc2cd8ba33a99cf4dcc8c90fd9ae")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("1cbd17424d858fbb37e6184bf355f72830a47d2e33a4b8de872fa87aaa49a15e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("1067c4c6fc40bbd2d8b747177296bbeb4abfcb54e700226f2f211c3a2b90be46")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("a110dcb9cdca8ce655b215427b0c5fa2b1a374badfe38fbb57cfb0ce28db434e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("dddc62de5f796dbb33cbc81ebdb496758ea653c918a840d12574373aa4835165")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("0493e89aae5b95b17f3092402cd7b6516494cad817aff68cc2988a4c065ae1ab")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("7dd9634e06daf56e58dfab4f71bea2d4d9b5c808fc30a40eb6208e76dfe24b14")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("5321f8c2c71239b1e2002d284be8ec825d4a6f95cd921e58db71f259834b7aa1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("a1d9a594cd3103baa24937ad9150c1a389544b4350e859200b3e5c036ac352bd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-x86_64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-unknown-linux-gnu-lto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-i686-unknown-linux-gnu-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-x86_64-unknown-linux-gnu-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-x86_64-unknown-linux-musl-lto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-i686-pc-windows-msvc-shared-pgo-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.10.0-x86_64-pc-windows-msvc-shared-pgo-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("f0628c6dee878610c1e3da924c8d930c6948e85c56e75b95b3d437e902acf782")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("ed5c87541ea92d5dd66a6187dbec4d43d395d6908f7f42e650e12e62bf6fe145")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("72c1ce8791b78719080fc00575bc96f21de1024955a56d00b151eeb774804bc9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("8dcfda0951f25bc5be61ef244f8892a2015ba4512bcabcf37005480683f6e72b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("f6bac804b844fc2ad75e98bfa4ba5fb2df17dd484a6f8b7b9fda7e55e4de283b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("eff34eb5c7314c98382a8028647c3e0c788c48aab12879282feae2e7e176af6c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("a1c252593e364e7fdb99f48b233311c54001a6e993057948a232ead94b10329e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("e0ce3038ef0e779791f62ac64ed08bfaf4e09bc48ff15a82038cef2009124cbf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("57ba961d7cd48a4473000a0fb10581094cd5fd4cd33558b3ddc8ae6856097e80")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("a96090f2eb47952b3abb6815864d7c4477bbf23d5d2559c4514140c674c8835b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("fb308474d3d148fbe4223704d47f4397a85317d1000dd9978f2cd97c7992a931")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("b9d39c72d6f503e7fb759c32310fbf1cb402189c2bb26f1d25608b8c1d8a41b6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("6dd47b0f86888312cee676a4ac63ddd1e217009317e8f0c6dcb82d681a56277b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("4f6ca60512658efeb4b73afd8b86883948370ed02081af2f722b2c63e08fc1ec")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("3d39de9f4c52465a546e2bed637e20b1811ba8d85b1400dad28dbb2db4af3c2f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("b0c1a25cd1c07bdf7d3d861d39527eaff66363a28eb3306463d8ffc134d55743")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 21,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241219/cpython-3.9.21%2B20241219-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("acd70e2f5596d8e24a06d47394b15d326fc12c26662a8cc018e3635fb3025897")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("41e9bb2d45e1a0467e534dafc6691b3d3c2b79fd9a564562f4c0c41eb343d30a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("440f4ebc651e707ed24d5dc68d3b0b2197e7fb369bb77685b1b539dbf30ab1e5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("3742c9d6563527a003b12ac689c07e6965911ff89fd9cbbd3c17ac7bfb037d4a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("33f89a84b170bbed966f3028b84f5c39b3c6e30d615585107d69c9ed9fe49564")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("73ecdd7318b44c88cc5877d039111067723510e922852fd207ac05b03a11863c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("556dd3e80ba644dfb8ca5a8a68681f243717d8ef4a517e486a49e1f6da46278c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("9eb2296c68484602bf9a27659ca91f6c073c8b1c97c2791bb1b0191aa8b9e45a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("44d9d016f9820f39e5bb542782557d46876b69d23d0a204eb2f367739da623e0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("332ce515daa15173f73d0ecebc988fadfac5583af8355d8895b3eb8086dac813")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("1461b5364d07a56f1879390eaec72542fe0dd48fa369ce4507c555677bfe07ea")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("237ec3d144e488130476c1d2c70d8ca7f2b36f423eb0ead348a78c6fc1fb7abb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("0e2c93f61971e0bda6e679a211c1e1ae074f43d2113328b8304b2e38c77c5a32")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("0f99bd0d5b0d1afb207f3e6d4fb36be303bf277ac7c327dd07b7d1e6ba90f9ea")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("693a3f129a86c48c176b35db81eae0bf54faec893fe8386ec5116070638c4712")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("0d8f9e08f4eb14575574728b174591736ceaada37a03c1bdec17184475ffabe8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("4d331f59031e02c857f4afbcfc933de3c68c8fb47ce919103147d760a0d7165f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.9.20%2B20241016-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("ce3779065ab824333e8d6d0a3d055d4073cdcc9a6e60abe24929023369f91512")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("451582f8a6a8c15ef35a327afcdbf8d03b1ebba7192e90d850d092dac91f91c6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("ebaf4336d0cbff4466c994d5bcaa92a38c91d06694d0cd675ec663259d5f37c7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("23d08f7f0bf151c2ea54b2c9c143bc710faf166ff74225b0f967fab1e2d7a151")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-armv7-unknown-linux-gnueabi-install_only_stripped.tar.gz",
        sha256: Some("cebb879d47874f3f943a4334a8fcd8baa3cd7ef4be8cae6b4c8ae980d981a28d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-armv7-unknown-linux-gnueabihf-install_only_stripped.tar.gz",
        sha256: Some("c32d3227c44919349172c27b35275bad379f1679f729fbd4f336625903171a1a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-ppc64le-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("3cc60442d5694db1abe2a0c6e73459ebb6e7ba7fc7b0a986f3699d463a8f9557")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-s390x-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("b41f834311532ee9dcf76cad3cdeda285d0e283de2182ce9870c37c40970cdd3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("3b7d574b6bbf8303789a1d26b96a81dcca907381441ce15818c784e18d1db299")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("5c6605b1cfa6a952420f2267d10bed9ae20a02858a769b7275d8805f6b9fe40b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-x86_64_v2-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("1d8179bde4db3e6cffac45f5c6a105e342a50a854b316b26760c17ba32bb164a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-x86_64_v2-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("2f9eac6ff7bc42a06b0f04cc28f5f24e851dcbb5d4a025a170e036ac1511a226")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-x86_64_v3-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("66c1df96394e43826ff5ed7f980ad93241af1e3ec212ce92b48efea183e8b9db")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-x86_64_v3-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("3c79c2c628d9412658e2fe21864439f53d22fa2dd8bc4f355564e73115715e37")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-x86_64_v4-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("7098fa8e0dac6a689d5289e761b85f713b607c476bfb1c1da13b3f86edce8461")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-x86_64_v4-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("1c6c6d2113fd3965d29b2fc70621ff81a1f7165a9dd660de980b805ba61a5c39")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("1c78c6dd763e6d583c3c3f917544bc4446d0e9fbe2b6e206042fa801ff9fb9ab")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.9.19%2B20240814-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("426da4d31e665b77dacf15cd89494a995ed634a9b97324bbef9cf36fcda4c8a9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("2548f911a6e316575c303ba42bb51540dc9b47a9f76a06a2a37460d93b177aa2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("171d8b472fce0295be0e28bb702c43d5a2a39feccb3e72efe620ac3843c3e402")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e5bc5196baa603d635ee6b0cd141e359752ad3e8ea76127eb9141a3155c51200")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.9.18%2B20230826-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("10c422080317886057e968010495037ba65731ab7653bcaeabadf67a6fa5e99e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("d6b18df7a25fe034fd5ce4e64216df2cc78b2d4d908d2a1c94058ae700d73d22")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("15d059507c7e900e9665f31e8d903e5a24a68ceed24f9a1c5ac06ab42a354f3f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("0e5663025121186bd17d331538a44f48b41baff247891d014f3f962cbe2716b4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("cb47455810ae63d98501b3bb4fcdfdb9924633fb2e86e62d77e523a3bdee44ba")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ff8e33905d114a590cd13f5e17590efd6ad56c4eb841c9820863da2f55b13860")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("2ab7781e0d14d92272d7bcfb1383328ded38213fa9ad9ca00d1d08dae1ada26e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e85a752197d7c7529a19b83c17efa194c8743cbc4608d5cbca77da506ab30956")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("20c45210b2ef110ce6f01038e2861435cc6b9ccf94e7f015dcb111c3f5d0b629")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("31b1548bd8e259b6b4a84d69d6ddcd27397af5971e3ffadd162f3a38eb623f6a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("61db6789f2974b6cf85fefc0e06950e43898e4a468d3c643cc35427d8ebf4fc0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("904ff5d2f6402640e2b7e2b12075af0bd75b3e8685cc5248fd2a3cda3105d2a8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("a9bdbd728ed4c353a4157ecf74386117fb2a2769a9353f491c528371cfe7f6cd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("73dbe2d702210b566221da9265acc274ba15275c5d0d1fa327f44ad86cde9aa1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("dfe1bea92c94b9cb779288b0b06e39157c5ff7e465cdd24032ac147c2af485c0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("b77012ddaf7e0673e4aa4b1c5085275a06eee2d66f33442b5c54a12b62b96cbe")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("aed29a64c835444c2f1aff83c55b14123114d74c54d96493a0eabfdd8c6d012c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c591a28d943dce5cf9833e916125fdfbeb3120270c4866ee214493ccb5b83c3c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-s390x-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("01454d7cc7c9c2fccde42ba868c4f372eaaafa48049d49dd94c9cf2875f497e6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("26c4a712b4b8e11ed5c027db5654eb12927c02da4857b777afb98f7a930ce637")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("194316e9cc7add1dd12be3e3eea2908fd4d623799edd7df69e360c6a446b750d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("3d9275e70d39030030f7e9db5f05fc07e2be6fb4964af0053f4a461bac9ba1bd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("cdfe58799d017947c414bc8513ddb51bc7f4c25b2e603faa8dfcfd931a47392f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("be15573d9213c20ead1de62f500817f5e069510d41a4f48a78c3086953c1693e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("4a541fb47184505c165adcb88490fa9df11e7d34fd9bd0117f70b42c50dcc7e1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("114573e1c4919116e5ecf4c6425f44e09b03527fcddffe3d53f73fe1ce9df7d8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("53a1b8cd550e803e258ba04192a18ccfe2ce4cbca2dc776cec47b2eba2507305")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("09f9d4bc66be5e0df2dfd1dc4742923e46c271f8f085178696c77073477aa0c1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("9b9a1e21eff29dcf043cea38180cf8ca3604b90117d00062a7b31605d4157714")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("c1de1d854717a6245f45262ef1bb17b09e2c587590e7e3f406593c143ff875bd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("3abc4d5fbbc80f5f848f280927ac5d13de8dc03aabb6ae65d8247cbb68e6f6bf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("f629b75ebfcafe9ceee2e796b7e4df5cf8dbd14f3c021afca078d159ab797acf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ab0a14b3ae72bf48b94820e096e86b3cf3e05729862f768e109aa8318016c4f2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Powerpc64le,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-ppc64le-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ff3ac35c58f67839aff9b5185a976abd3d1abbe61af02089f7105e876c1fe284")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("2b6e146234a4ef2a8946081fc3fbfffe0765b80b690425a49ebe40b47c33445b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("5d9b13e8d5ee7a26fd0cf6e6d7e5a1ea90ddddd1f30ed2400bda60506f7dcea3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("08acda6031601b35291fa54f98888dc17d36c9ff35bdb2adea4631df81e24271")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("b78dbd216e23a8f4adda881fbe11f6749371c09b4235dd519a4b4be761dd1504")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("337fb379e9c46b6145e4c52f827513989c2feb9c84beac17c38decb50af5b53a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("675b7d77d653f47561536eae8fcd1ffaedd467ffcf43afdd9c96d2d32e8e84c4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("fc289e674fb61ca4e39a4676ee11971e3f87bab607951898336b5df53b6ca328")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("9bdb07f9e9dc429553d12b434293f270c9e281ea7c7ffe5be4f6e8b2cc151e03")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("219532ffa49af88e3b90e9135cf3b6e1fa11cf165b03098fb9776a07af8ca6d0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("cdabb47204e96ce7ea31fbd0b5ed586114dd7d8f8eddf60a509a7f70b48a1c5e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("64dc7e1013481c9864152c3dd806c41144c79d5e9cd3140e185c6a5060bdc9ab")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("f2bcade6fc976c472f18f2b3204d67202d43ae55cf6f9e670f95e488f780da08")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("52a8c0a67fb919f80962d992da1bddb511cdf92faf382701ce7673e10a8ff98f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("bf32a86c220e4d1690bb92b67653f20b8325808accd81bff03b5c30ae74e6444")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("cdc3a4cfddcd63b6cebdd75b14970e02d8ef0ac5be4d350e57ab5df56c19e85e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("81b1c76ac789521fcececdcdc643f6de6fc282083b1a36a9973d835fc8a39391")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("436c35bd809abdd028f386cc623ae020c77e6b544eaaca405098387c4daa444c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("52e774f1ccbbc61a30da2a95286348700ee7d7707412ef10035420f43469c39f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("6a3d25cb22b65355dd793bfb4838a16a5cd85edc07e3dac8cc22adad860f2c89")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("a7c4dcff34014b01de397ffe6497787935575f604f489d01cdd843b41dd320e6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("65350987640f33876d072a44d03e6e397dcf5d2512819bd8765c892ec5c0f153")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("33b227fdde7a236e69635dafc0cfbfcee76594b9892092357ffd35a0dcb303e7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("0b81089247f258f244e9792daaa03675da6f58597daa6913e82f2679862238dd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("022daacab215679b87f0d200d08b9068a721605fa4721ebeda38220fc641ccf6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("e38df7f230979ce6c53a5bafb3a81287838e5f3892c40cd1b98a0c961c444713")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("b7d3a1f4b57e9350571ccee49c82f503133de0d113a2dbaebc8ccf108fb3fe1b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("fe538201559ca37f44cd5f66c42a65fe7272cb4f1f63edd698b6f306771db1e9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("3af1c255110c2f42ed0b7957502c92edf8b5c5e6fc5f699a2475bf8a560325c0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e63d0c00a499e0202ba7a0f53ce69fca6d30237af39af9bc3c76bce6c7bf14d7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("ceb26ef5f5a9b7b47fa95225fffce9c8ef0c9c1fbeca69fbda236a0c10de7ad8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("3b858cd4a187db8cd1eaedddd5ff80a077783092ca4d84970097127220f55cd4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("d912966b120f5406046bf5a5252a6585cfb7620fe5d570a265cd221dfd8fbeb3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("798cd9d019885fb2f0010a234873ebe7de35e4eb8fc4e2ba1d40ea31b6abb0bf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("46cd2e2b988bb5f6e12b184f2130d678ee781f84520dee1d8c487f2e0d1e3371")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("f14391bc3b71dadc8855577325e3a49b17f351d428cc4caad8b1216b90f3ad80")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("355b1b5c3098b45f9b3dd1602321787c6731a70e574846efe29d96aef31d1e5b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("f3526e8416be86ff9091750ebc7388d6726acf32cc5ab0e6a60c67c6aacb2569")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("f111c3c129f4a5a171d25350ce58dad4c7e58fbe664e9b4f7c275345c9fe18a6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("d9603edc296a2dcbc59d7ada780fd12527f05c3e0b99f7545112daf11636d6e5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("9540a7efb7c8a54a48aff1cb9480e49588d9c0a3f934ad53f5b167338174afa3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("80415aac1b96255b9211f6a4c300f31e9940c7e07a23d0dec12b53aa52c0d25e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("efcc8fef0d498afe576ab209fee001fda3b552de1a85f621f2602787aa6cf3d4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ce1cfca2715e7e646dd618a8cb9baff93000e345ccc979b801fc6ccde7ce97df")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("766ed7e805e8c7abc4e50f1c94814575e7978ed7bd1f9e9ccec82d66b47b567f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("1e1b522a91ed805287793427013762bc8f6c02440147e9fa5306316369307e58")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("e3142427ed091bfe0986cf49954fa3fcbe7754f42d66e4fd0ce68f41a513c3c1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("315a9fbc75e2a0545254edf8974d88577ec64537ee04322ad1a7ce9752353f33")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("40be6728331395abce574e080e5411e781c0a8a3a5d5175ae10534cee8d83ae2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("b4c5a9908a77e4cae37901f8df2655380186d0e10723902672ef5e5296093071")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("3b21780f0eba88519f0c00032cfb8ddb959b31a76cf21611a26da5d8296a9200")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("90e3879382f06fea3ba6d477f0c2a434a1e14cd83d174e1c7b87e2f22bc2e748")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("b538127025a467c64b3351babca2e4d2ea7bdfb7867d5febb3529c34456cdcd4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("8dee06c07cc6429df34b6abe091a4684a86f7cec76f5d1ccc1c3ce2bd11168df")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("2453ba7f76b3df3310353b48c881d6cff622ba06e30d2b6ae91588b2bc9e481a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("2ee1426c181e65133e57dc55c6a685cb1fb5e63ef02d684b8a667d5c031c4203")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("233e1a9626d9fe13baac8de3689df48401d0ad5da1c2f134ad57d8e3e878a1a5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("ccca12f698b3b810d79c52f007078f520d588232a36bc12ede944ec3ea417816")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("dd0eaf7ef64008d4a51a73243f368e0311b7936b0ac18f8d1305fffb0dfb76e6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("72085205843949f783c9245ad42902a63a00dd35367f1e5ea6f4aa709b2029ce")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("140ea77e2924fc312ecd266337f1add09e560084422843673b08ff847259a93f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("bb702440330bf8cd852cf36735a47ee10aa3941271b5d2f96cd6bfa6ac7a46b4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("afceda37726445a1472b023ca7e6fb1b169557c7fc26a2ffa53e2b59247e1c6c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("7a5fcad095ad183049b2f975cd55efd8a2ce3e4d894ee67cb5fa01186f50203e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("81138b9b65bc01ed441bf0ab0fa2225c3d131a3b13838cc698c9e4aaef4cab11")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("8b7e440137bfa349a008641a75a2b1fd8ae22d290731778a144878a59a721c51")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("3024147fd987d9e1b064a3d94932178ff8e0fe98cfea955704213c0762fee8df")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("cf92a28f98c8d884df0937bf19d5f1a40caa25a6a211a237b7e9b592b2b71c2b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("43889d1a424c84fb155e1619f062adb6984fbde80b6043611790f22bcbeec300")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("0e50f099409c5e651b5fddd16124af1d830d11653e786a93c28e5b8f8aa470c4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("75ac727631eab002bd120246197a8235145cb90687be181f7a52de6f41d44d34")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("0429d5ceb095d5e24c292bf1a39208b88ae236a680ef8fa3e1830e3a1a7e8882")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("ae8f55d90ae173f96e81f376daa5a9969a77531a6f7b8eacbe8ad90b41bbca1d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("bd9d808283eef69882e8e0c2529fe400c8605b7c08ef64e679d1766c1a87ef95")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("69439fb7c2e8f8bda28f2d5f4c71634116800d71a06f7cbeeb8f21ebca829fd2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("9cb5ab650efad6e475026f7782f852094a9403966bcc1f795065b1a4a33e362b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("190cb2cc081d0d6ffca9232e877a959915452f6fd3808cfe128f6ebc392fcfb2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("8d7eee9c085e08f3cc6cdf56be7cc0dc8a3e25e053856c1a933efb3529c087de")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("0455c95330532edf5c30d4a30873140e7e6c715fbca921ac127769865382050b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("ceac8729b285a8c8e861176dd2dadd7f8e7e26d8f64cac6c6226a14d2252cd4c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("0c529a511f7a03908fc126c4a8467b47e24a4d98812147e8e786cf59e86febf0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("ad66c2a3e7263147e046a32694de7b897a46fb0124409d29d3a93ede631c8aee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("fdaf594142446029e314a9beb91f1ac75af866320b50b8b968181e592550cd68")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("12dd1f125762f47975990ec744532a1cf3db74ad60f4dfb476ca42deb7f78ca4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("37ba43845c3df9ba012d69121ad29ea7f21ea2f5994a155007cf1560d74ce503")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("455089cc576bd9a58db45e919d1fc867ecdbb0208067dffc845cc9bbf0701b70")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("30add63ec16e07ad13e19f6d7061f7e4c7b971962354f48ab3e85656ce3b393d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64_v2-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("f9c6b0412c0773e604a1f95d5db97f79f98d960bcf0e85215e533ea573721216")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V2),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64_v2-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("5e4bde1751608e7f3ae2d9029c8866d0325db24c5a3b8441b62bef14fb8484be")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64_v3-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("5c23a5f13356adfcf26d27df7e48885e634489e3a2383e67c013c3f1ec2e082e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V3),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64_v3-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("649371a944665efa5b286d530c76808fb4f6ae4694f59c2ced8596fbe65d5164")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64_v4-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e97477ab9d30b52faf7de9e364ff50acf4490a28f7bf6fa11887f5a26e56ec93")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: Some(ArchVariant::V4),
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64_v4-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("6bcba95a5d6f1378821ba16c5d5b8a2e02bb365c1314d0492f66545973459ae4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("56c0342a9af0412676e89cdf7b52ac76037031786b3f5c40942b8b82d366c96f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("c145d9d8143ce163670af124b623d7a2405143a3708b033b4d33eed355e61b24")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.9.7-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.9.7-x86_64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.9.7-aarch64-unknown-linux-gnu-lto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.9.7-i686-unknown-linux-gnu-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.9.7-x86_64-unknown-linux-gnu-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.9.7-x86_64-unknown-linux-musl-lto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.9.7-i686-pc-windows-msvc-shared-pgo-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20211017/cpython-3.9.7-x86_64-pc-windows-msvc-shared-pgo-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.9.6-aarch64-apple-darwin-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.9.6-x86_64-apple-darwin-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.9.6-aarch64-unknown-linux-gnu-lto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.9.6-i686-unknown-linux-gnu-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.9.6-x86_64-unknown-linux-gnu-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.9.6-x86_64-unknown-linux-musl-lto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.9.6-i686-pc-windows-msvc-shared-pgo-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.9.6-x86_64-pc-windows-msvc-shared-pgo-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.9.5-aarch64-apple-darwin-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-apple-darwin-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.9.5-i686-unknown-linux-gnu-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-unknown-linux-gnu-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-unknown-linux-musl-lto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.9.5-i686-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.9.4-aarch64-apple-darwin-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-apple-darwin-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.9.4-i686-unknown-linux-gnu-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-unknown-linux-gnu-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-unknown-linux-musl-lto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.9.4-i686-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210414/cpython-3.9.3-aarch64-apple-darwin-pgo%2Blto-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-apple-darwin-pgo%2Blto-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-unknown-linux-gnu-pgo%2Blto-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-unknown-linux-musl-lto-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210414/cpython-3.9.3-i686-pc-windows-msvc-shared-pgo-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-pc-windows-msvc-shared-pgo-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.9.2-aarch64-apple-darwin-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-apple-darwin-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.9.2-i686-unknown-linux-gnu-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-unknown-linux-gnu-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-unknown-linux-musl-lto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.9.2-i686-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-apple-darwin-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-unknown-linux-gnu-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-unknown-linux-musl-noopt-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.9.1-i686-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 1,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-apple-darwin-pgo-20201020T0626.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-unknown-linux-gnu-pgo-20201020T0627.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-unknown-linux-musl-noopt-20201020T0627.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20201020/cpython-3.9.0-i686-pc-windows-msvc-shared-pgo-20201021T0245.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 0,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-pc-windows-msvc-shared-pgo-20201021T0245.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.8.20%2B20241002-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("30ba44af64e599bde7307908393374bdcd99e185bf9b3c9de3f697f3fbe6bf8f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.8.20%2B20241002-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("375b6eead6c852cabbf3ccfd43dc4f6dd4c36381bf74c9a7910acb839fd5c57f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.8.20%2B20241002-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("75a187ebfab81096e3f3d91d70c1349e64defbdfb0e8a067cb5233d017655e31")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.8.20%2B20241002-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("a3a75094545912d4e9413673441b3f0d2e58ce9b264477f910800148801ccf11")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.8.20%2B20241002-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("fcddfd3f1090833e1f3106be021809630008b53026bc96dcaab2986625db27fa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.8.20%2B20241002-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("d829105aaf53a1cadf8738e040c6211bc9bef2c6e4757b972954f0f322d57e7d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 20,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20241002/cpython-3.8.20%2B20241002-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("ec2f723dcfbf09581578a716c05cc67823a43d77111e6dd9e0d1557ccc6dcbf3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.8.19%2B20240814-aarch64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("6a15ee2b507aed4d5b15fd1b66fc570aa49183f15aa6c412eccd065446f17d8e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.8.19%2B20240814-x86_64-apple-darwin-install_only_stripped.tar.gz",
        sha256: Some("1a24263b039c1172bd42d74a5694492f3e3dbe4d3e52a1e7cc2856fee7dbee4a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.8.19%2B20240814-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("202211923850303f521146ee1831642aaf357ffeeadbe13a0a91884317227528")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.8.19%2B20240814-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
        sha256: Some("0f1579dbb01c98af7a12fef4c9aa8a99d45b91393f64431f5de712f892bc5c0b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.8.19%2B20240814-x86_64-unknown-linux-musl-install_only_stripped.tar.gz",
        sha256: Some("6ee6c7469c9d2c7078beb95a9a3a261c42502e0b1603722a0689bdb2e789060c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.8.19%2B20240814-i686-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("73bf0135330b96c48ca79ccd6d2f3287a7466573a5fc1b62d982bcdb1d5f0ab3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240814/cpython-3.8.19%2B20240814-x86_64-pc-windows-msvc-install_only_stripped.tar.gz",
        sha256: Some("89d238b125cd7546b7d0cbd7f484a438d2c2f239c15c9b38ec3c62b1f343a6ca")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("4d493a1792bf211f37f98404cc1468f09bd781adc2602dea0df82ad264c11abc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("7d2cd8d289d5e3cdd0a8c06c028c7c621d3d00ce44b7e2f08c1724ae0471c626")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("6588c9eed93833d9483d01fe40ac8935f691a1af8e583d404ec7666631b52487")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("5ae36825492372554c02708bdd26b8dcd57e3dbf34b3d6d599ad91d93540b2b7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("e591d3925f88f78a5dffb765fd10b9dab6e497d35cf58169da83eab521c86a37")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("c24f9c9e8638cff0ce6aa808a57cc5f22009bc33e3bcf410a726b79d7c5545fe")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("dba923ee5df8f99db04f599e826be92880746c02247c8d8e4d955d4bc711af11")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("c6f7a130d0044a78e39648f4dae56dcff5a41eba91888a99f6e560507162e6a1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("155b06821607bae1a58ecc60a7d036b358c766f19e493b8876190765c883a5c2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("9f6d585091fe26906ff1dbb80437a3fe37a1e3db34d6ecc0098f3d6a78356682")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e580fdd923bbae612334559dc58bd5fd13cce53b769294d63bc88e7c6662f7d9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("8d3e1826c0bb7821ec63288038644808a2d45553245af106c685ef5892fabcd8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("322b7837cfd8282c62ae3d2f0e98f0843cbe287e4b8c4852b786123f2e13b307")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("cb6af626ba811044e9c5ee09140a6920565d2b1b237a11886b96354a9fcc242e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("6428e1b4e0b4482d390828de7d4c82815257443416cb786abe10cb2466ca68cd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("7e484eb6de40d6f6bdfd5099eaa9647f65e45fb6d846ccfc56b1cb1e38b5ab02")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("28506e509646c11cb2f57a7203bd1b08b6e8e5b159ae308bd5bb93b0d334bdaf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("9c6615931fd1045bf9f2148aa7dd9ce1ece8575ed68a5483a0b615322a43d54c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("1260fd6af34104bbd57489175e6f7bfea76d4bd06a242a0f8e20e390e870b227")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("b1f1502c3a13b899724dbd32bd77a973fa9733b932c5700d747fe33d5de9ac4f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("840aefa3b03b66b6561360735dc0ac4e0a36a3ebb4d1f85d92f5b5f6638953cc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("77466f93ef5b030cf13d0446067089b0ce0d415cc6d1702655bdbb12a8c18c97")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("120b3312fa79bac2ace45641171c2bc590c4e4462d7ad124d64597e124a36ae7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("1e0a92d1a4f5e6d4a99f86b1cbf9773d703fe7fd032590f3e9c285c7a5eeb00a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("70b57f28c2b5e1e3dd89f0d30edd5bc414e8b20195766cf328e1b26bed7890e1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("886ab33ced13c84bf59ce8ff79eba6448365bfcafea1bf415bd1d75e21b690aa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("3bc1f49147913d93cea9cbb753fbaae90b86f1ee979f975c4712a35f02cbd86b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("e47edfb2ceaf43fc699e20c179ec428b6f3e497cf8e2dcd8e9c936d4b96b1e56")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("f767d0438eca5b18c1267c5121055a5808a1412ea7668ef17da3dc9bdd24a55f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("318c059324b84b5d7685bcd0874698799d9e3689b51dbcf596e7a47a39a3d49a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("2fdc3fa1c95f982179bbbaedae2b328197658638799b6dcb63f9f494b0de59e2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("6c17f6dcda59de5d8eee922ef7eede403a540dae05423ef2c2a042d8d4f22467")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("3ed4db8d0308c584196d97c629058ea69bbd8b7f9a034cf8c2c701ebb286c091")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("c45e42deee43e3ebc4ca5b019c37d8ae25fb5b5f1ba5f602098a81b99d2bc804")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("d01d813939ad549ca253c52e5b8361b4490cc5c8cbda00ab6e0c524565153e2b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("4eb53bce831bf52682067579c09ccaccb6524dd44bd4b8047454c69b4817f4f0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("72c08b1c1d8cc14cb8d22eab18b463bb514ea160472fdc7400bd69ae375cf9c4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("a0730f3a9e60581f02bdb852953fbb52cf98e8431259fa39cb668a060bd002a0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("1af39953b4c8324ed0608e316bc763006f27e76643155d92eae18e4db6fc162f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("ae4131253d890b013171cb5f7b03cadc585ae263719506f7b7e063a7cf6fde76")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("cd6e7c0a27daf7df00f6882eaba01490dd963f698e99aeee9706877333e0df69")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-aarch64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("8dc7814bf3425bbf78c6e6e5a6529ded6ae463fa6a4b79c025b343bae4fd955a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("9485599ad9053dfba08c91854717272e95b7c81e0d099d9c51a46fc5a095ccb4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("fb566629ccb5f76ef56d275a3f8017d683f1c20c5beb5d5f38b155ed11e16187")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("2c90a0d048caf146d4c33560d6eead1428a225219018d364b1af77f23c492984")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("a50668d4c5fbcb374d3ca93ee18db910bc3b462693db073669f31e6da993abf9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("f20643f1b3e263a56287319aea5c3888530c09ad9de3a5629b1a5d207807e6b9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-aarch64-apple-darwin-install_only.tar.gz",
        sha256: Some("f9a3cbb81e0463d6615125964762d133387d561b226a30199f5b039b20f1d944")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-apple-darwin-install_only.tar.gz",
        sha256: Some("f323fbc558035c13a85ce2267d0fad9e89282268ecb810e364fff1d0a079d525")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-i686-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("fcb2033f01a2b10a51be68c9a1b4c7d7759b582f58a503371fe67ab59987b418")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-unknown-linux-gnu-install_only.tar.gz",
        sha256: Some("5be9c6d61e238b90dfd94755051c0d3a2d8023ebffdb4b0fa4e8fedd09a6cab6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-unknown-linux-musl-install_only.tar.gz",
        sha256: Some("27faf8aa62de2cd4e59b75a6edce4cab549eba81f0f9cc21df0e370a8a2f3a25")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-i686-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("aaa75b9115af73dc3daf7db050ed4f60fd67d2a23ebab30670f18fb8cfa71f33")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-pc-windows-msvc-shared-install_only.tar.gz",
        sha256: Some("4658e08a00d60b1e01559b74d58ff4dd04da6df935d55f6268a15d6d0a679d74")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.8.11-x86_64-apple-darwin-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.8.11-i686-unknown-linux-gnu-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.8.11-x86_64-unknown-linux-gnu-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.8.11-x86_64-unknown-linux-musl-lto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.8.11-i686-pc-windows-msvc-shared-pgo-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210724/cpython-3.8.11-x86_64-pc-windows-msvc-shared-pgo-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-apple-darwin-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.8.10-i686-unknown-linux-gnu-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-unknown-linux-gnu-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-unknown-linux-musl-lto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.8.10-i686-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-apple-darwin-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.8.9-i686-unknown-linux-gnu-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-unknown-linux-gnu-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-unknown-linux-musl-lto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.8.9-i686-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-apple-darwin-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.8.8-i686-unknown-linux-gnu-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-unknown-linux-gnu-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-unknown-linux-musl-lto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.8.8-i686-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-apple-darwin-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-unknown-linux-gnu-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-unknown-linux-musl-noopt-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.8.7-i686-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-apple-darwin-pgo-20201020T0626.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-unknown-linux-gnu-pgo-20201020T0627.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-unknown-linux-musl-noopt-20201020T0627.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20201020/cpython-3.8.6-i686-pc-windows-msvc-shared-pgo-20201021T0233.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-pc-windows-msvc-shared-pgo-20201021T0232.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200823/cpython-3.8.5-x86_64-apple-darwin-pgo-20200823T2228.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200822/cpython-3.8.5-x86_64-unknown-linux-gnu-pgo-20200823T0036.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200822/cpython-3.8.5-x86_64-unknown-linux-musl-noopt-20200823T0036.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200830/cpython-3.8.5-i686-pc-windows-msvc-shared-pgo-20200830T2311.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 5,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200830/cpython-3.8.5-x86_64-pc-windows-msvc-shared-pgo-20200830T2254.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200530/cpython-3.8.3-x86_64-apple-darwin-pgo-20200530T1845.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200517/cpython-3.8.3-x86_64-unknown-linux-gnu-pgo-20200518T0040.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200517/cpython-3.8.3-x86_64-unknown-linux-musl-noopt-20200518T0040.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200517/cpython-3.8.3-i686-pc-windows-msvc-shared-pgo-20200518T0154.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 3,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200517/cpython-3.8.3-x86_64-pc-windows-msvc-shared-pgo-20200517T2207.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-apple-darwin-pgo-20200418T2238.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-unknown-linux-gnu-pgo-20200418T2243.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-unknown-linux-musl-noopt-20200418T2309.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200418/cpython-3.8.2-i686-pc-windows-msvc-shared-pgo-20200418T2315.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 2,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-pc-windows-msvc-shared-pgo-20200418T2315.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200823/cpython-3.7.9-x86_64-apple-darwin-pgo-20200823T2228.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200822/cpython-3.7.9-x86_64-unknown-linux-gnu-pgo-20200823T0036.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200822/cpython-3.7.9-x86_64-unknown-linux-musl-noopt-20200823T0036.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200822/cpython-3.7.9-i686-pc-windows-msvc-shared-pgo-20200823T0159.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200822/cpython-3.7.9-x86_64-pc-windows-msvc-shared-pgo-20200823T0118.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200530/cpython-3.7.7-x86_64-apple-darwin-pgo-20200530T1845.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200517/cpython-3.7.7-x86_64-unknown-linux-gnu-pgo-20200518T0040.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200517/cpython-3.7.7-x86_64-unknown-linux-musl-noopt-20200518T0040.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200517/cpython-3.7.7-i686-pc-windows-msvc-shared-pgo-20200517T2153.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 7,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200517/cpython-3.7.7-x86_64-pc-windows-msvc-shared-pgo-20200517T2128.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200216/cpython-3.7.6-windows-x86-shared-pgo-20200217T0110.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 6,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://github.com/astral-sh/python-build-standalone/releases/download/20200216/cpython-3.7.6-windows-amd64-shared-pgo-20200217T0022.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.17-macos_arm64.tar.bz2",
        sha256: Some("a050e25e8d686853dd5afc363e55625165825dacfb55f8753d8225ebe417cfd2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.17-macos_x86_64.tar.bz2",
        sha256: Some("6c2c5f2300d7564e711421b4968abd63243cb96f76e363975dd648ebf4a362ee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.17-aarch64.tar.bz2",
        sha256: Some("53b6e5907df869c49e4eae7aca09fba16d150741097efb245892c1477d2395f2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.17-linux32.tar.bz2",
        sha256: Some("e534110e1047da37c1d586c392f74de3424f871d906a2083de6d41f2a8cc9164")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.16-s390x.tar.bz2",
        sha256: Some("af97efe498a209ba18c7bc7d084164a9907fb3736588b6864955177e19d5216a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.17-linux64.tar.bz2",
        sha256: Some("fdcdb9b24f1a7726003586503fdeb264fd68fc37fbfcea022dcfe825a7fee18b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.17-win64.zip",
        sha256: Some("cab794a03ddda26238c72942ea6f225612e0dc17c76cac6652da83a95024e6e8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.15-macos_arm64.tar.bz2",
        sha256: Some("d927c5105ea7880f7596fe459183e35cc17c853ef5105678b2ad62a8d000a548")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.15-macos_x86_64.tar.bz2",
        sha256: Some("559b61ba7e7c5a5c23cef5370f1fab47ccdb939ac5d2b42b4bef091abe3f6964")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.15-aarch64.tar.bz2",
        sha256: Some("52146fccaf64e87e71d178dda8de63c01577ec3923073dc69e1519622bcacb74")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.15-linux32.tar.bz2",
        sha256: Some("75dd58c9abd8b9d78220373148355bc3119febcf27a2c781d64ad85e7232c4aa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.15-s390x.tar.bz2",
        sha256: Some("209e57596381e13c9914d1332f359dc4b78de06576739747eb797bdbf85062b8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.15-linux64.tar.bz2",
        sha256: Some("33c584e9a70a71afd0cb7dd8ba9996720b911b3b8ed0156aea298d4487ad22c3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.15-win64.zip",
        sha256: Some("b378b3ab1c3719aee0c3e5519e7bff93ff67b2d8aa987fe4f088b54382db676c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.12-macos_arm64.tar.bz2",
        sha256: Some("45671b1e9437f95ccd790af10dbeb57733cca1ed9661463b727d3c4f5caa7ba0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.12-macos_x86_64.tar.bz2",
        sha256: Some("dbc15d8570560d5f79366883c24bc42231a92855ac19a0f28cb0adeb11242666")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.12-aarch64.tar.bz2",
        sha256: Some("26208b5a134d9860a08f74cce60960005758e82dc5f0e3566a48ed863a1f16a1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.12-linux32.tar.bz2",
        sha256: Some("811667825ae58ada4b7c3d8bc1b5055b9f9d6a377e51aedfbe0727966603f60e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.12-s390x.tar.bz2",
        sha256: Some("043c13a585479428b463ab69575a088db74aadc16798d6e677d97f563585fee3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.12-linux64.tar.bz2",
        sha256: Some("6c577993160b6f5ee8cab73cd1a807affcefafe2f7441c87bd926c10505e8731")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.10-v7.3.12-win64.zip",
        sha256: Some("8c3b1d34fb99100e230e94560410a38d450dc844effbee9ea183518e4aff595c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.16-macos_arm64.tar.bz2",
        sha256: Some("88f824e7a2d676440d09bc90fc959ae0fd3557d7e2f14bfbbe53d41d159a47fe")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.16-macos_x86_64.tar.bz2",
        sha256: Some("fda015431621e7e5aa16359d114f2c45a77ed936992c1efff86302e768a6b21c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.16-aarch64.tar.bz2",
        sha256: Some("de3f2ed3581b30555ac0dd3e4df78a262ec736a36fb2e8f28259f8539b278ef4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.16-linux32.tar.bz2",
        sha256: Some("583b6d6dd4e8c07cbc04da04a7ec2bdfa6674825289c2378c5e018d5abe779ea")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.16-s390x.tar.bz2",
        sha256: Some("7a56ebb27dba3110dc1ff52d8e0449cdb37fe5c2275f7faf11432e4e164833ba")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.16-linux64.tar.bz2",
        sha256: Some("16f9c5b808c848516e742986e826b833cdbeda09ad8764e8704595adbe791b23")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.16-win64.zip",
        sha256: Some("06ec12a5e964dc0ad33e6f380185a4d295178dce6d6df512f508e7aee00a1323")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.15-macos_arm64.tar.bz2",
        sha256: Some("300541c32125767a91b182b03d9cc4257f04971af32d747ecd4d62549d72acfd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.15-macos_x86_64.tar.bz2",
        sha256: Some("18ad7c9cb91c5e8ef9d40442b2fd1f6392ae113794c5b6b7d3a45e04f19edec6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.15-aarch64.tar.bz2",
        sha256: Some("03e35fcba290454bb0ccf7ee57fb42d1e63108d10d593776a382c0a2fe355de0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.15-linux32.tar.bz2",
        sha256: Some("c6209380977066c9e8b96e8258821c70f996004ce1bc8659ae83d4fd5a89ff5c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.15-s390x.tar.bz2",
        sha256: Some("deeb5e54c36a0fd9cfefd16e63a0d5bed4f4a43e6bbc01c23f0ed8f7f1c0aaf3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.15-linux64.tar.bz2",
        sha256: Some("f062be307200bde434817e1620cebc13f563d6ab25309442c5f4d0f0d68f0912")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.15-win64.zip",
        sha256: Some("a156dad8b58570597eaaabe05663f00f80c60bc11df4a9c46d0953b6c5eb9209")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.12-macos_arm64.tar.bz2",
        sha256: Some("0e8a1a3468b9790c734ac698f5b00cc03fc16899ccc6ce876465fac0b83980e3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.12-macos_x86_64.tar.bz2",
        sha256: Some("64f008ffa070c407e5ef46c8256b2e014de7196ea5d858385861254e7959f4eb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.12-aarch64.tar.bz2",
        sha256: Some("e9327fb9edaf2ad91935d5b8563ec5ff24193bddb175c1acaaf772c025af1824")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.12-linux32.tar.bz2",
        sha256: Some("aa04370d38f451683ccc817d76c2b3e0f471dbb879e0bd618d9affbdc9cd37a4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.12-s390x.tar.bz2",
        sha256: Some("20d84658a6899bdd2ca35b00ead33a2f56cff2c40dce1af630466d27952f6d4f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.12-linux64.tar.bz2",
        sha256: Some("84c89b966fab2b58f451a482ee30ca7fec3350435bd0b9614615c61dc6da2390")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.12-win64.zip",
        sha256: Some("0996054207b401aeacace1aa11bad82cfcb463838a1603c5f263626c47bbe0e6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.11-macos_arm64.tar.bz2",
        sha256: Some("91ad7500f1a39531dbefa0b345a3dcff927ff9971654e8d2e9ef7c5ae311f57e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.11-macos_x86_64.tar.bz2",
        sha256: Some("d33f40b207099872585afd71873575ca6ea638a27d823bc621238c5ae82542ed")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.11-aarch64.tar.bz2",
        sha256: Some("09175dc652ed895d98e9ad63d216812bf3ee7e398d900a9bf9eb2906ba8302b9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.11-linux32.tar.bz2",
        sha256: Some("0099d72c2897b229057bff7e2c343624aeabdc60d6fb43ca882bff082f1ffa48")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.11-s390x.tar.bz2",
        sha256: Some("e1f30f2ddbe3f446ddacd79677b958d56c07463b20171fb2abf8f9a3178b79fc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.11-linux64.tar.bz2",
        sha256: Some("d506172ca11071274175d74e9c581c3166432d0179b036470e3b9e8d20eae581")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.11-win64.zip",
        sha256: Some("57faad132d42d3e7a6406fcffafffe0b4f390cf0e2966abb8090d073c6edf405")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.10-macos_arm64.tar.bz2",
        sha256: Some("e2a6bec7408e6497c7de8165aa4a1b15e2416aec4a72f2578f793fb06859ccba")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.10-macos_x86_64.tar.bz2",
        sha256: Some("f90c8619b41e68ec9ffd7d5e913fe02e60843da43d3735b1c1bc75bcfe638d97")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.10-aarch64.tar.bz2",
        sha256: Some("657a04fd9a5a992a2f116a9e7e9132ea0c578721f59139c9fb2083775f71e514")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.10-linux32.tar.bz2",
        sha256: Some("b6db59613b9a1c0c1ab87bc103f52ee95193423882dc8a848b68850b8ba59cc5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.10-s390x.tar.bz2",
        sha256: Some("ca6525a540cf0c682d1592ae35d3fbc97559a97260e4b789255cc76dde7a14f0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.10-linux64.tar.bz2",
        sha256: Some("95cf99406179460d63ddbfe1ec870f889d05f7767ce81cef14b88a3a9e127266")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.10-win64.zip",
        sha256: Some("07e18b7b24c74af9730dfaab16e24b22ef94ea9a4b64cbb2c0d80610a381192a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.9-osx64.tar.bz2",
        sha256: Some("59c8852168b2b1ba1f0211ff043c678760380d2f9faf2f95042a8878554dbc25")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.9-aarch64.tar.bz2",
        sha256: Some("2e1ae193d98bc51439642a7618d521ea019f45b8fb226940f7e334c548d2b4b9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.9-linux32.tar.bz2",
        sha256: Some("0de4b9501cf28524cdedcff5052deee9ea4630176a512bdc408edfa30914bae7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.9-s390x.tar.bz2",
        sha256: Some("774dca83bcb4403fb99b3d155e7bd572ef8c52b9fe87a657109f64e75ad71732")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.9-linux64.tar.bz2",
        sha256: Some("46818cb3d74b96b34787548343d266e2562b531ddbaf330383ba930ff1930ed5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.9-win64.zip",
        sha256: Some("be48ab42f95c402543a7042c999c9433b17e55477c847612c8733a583ca6dff5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.8-osx64.tar.bz2",
        sha256: Some("95bd88ac8d6372cd5b7b5393de7b7d5c615a0c6e42fdb1eb67f2d2d510965aee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.8-aarch64-portable.tar.bz2",
        sha256: Some("b7282bc4484bceae5bc4cc04e05ee4faf51cb624c8fc7a69d92e5fdf0d0c96aa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.8-linux32.tar.bz2",
        sha256: Some("a0d18e4e73cc655eb02354759178b8fb161d3e53b64297d05e2fff91f7cf862d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.8-s390x.tar.bz2",
        sha256: Some("37b596bfe76707ead38ffb565629697e9b6fa24e722acc3c632b41ec624f5d95")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.8-linux64.tar.bz2",
        sha256: Some("129a055032bba700cd1d0acacab3659cf6b7180e25b1b2f730e792f06d5b3010")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.9-v7.3.8-win64.zip",
        sha256: Some("c1b2e4cde2dcd1208d41ef7b7df8e5c90564a521e7a5db431673da335a1ba697")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.11-macos_arm64.tar.bz2",
        sha256: Some("78cdc79ff964c4bfd13eb45a7d43a011cbe8d8b513323d204891f703fdc4fa1a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.11-macos_x86_64.tar.bz2",
        sha256: Some("194ca0b4d91ae409a9cb1a59eb7572d7affa8a451ea3daf26539aa515443433a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.11-aarch64.tar.bz2",
        sha256: Some("9a2fa0b8d92b7830aa31774a9a76129b0ff81afbd22cd5c41fbdd9119e859f55")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.11-linux32.tar.bz2",
        sha256: Some("a79b31fce8f5bc1f9940b6777134189a1d3d18bda4b1c830384cda90077c9176")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.11-s390x.tar.bz2",
        sha256: Some("eab7734d86d96549866f1cba67f4f9c73c989f6a802248beebc504080d4c3fcd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.11-linux64.tar.bz2",
        sha256: Some("470330e58ac105c094041aa07bb05676b06292bc61409e26f5c5593ebb2292d9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.11-win64.zip",
        sha256: Some("0f46fb6df32941ea016f77cfd7e9b426d5ac25a2af2453414df66103941c8435")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.10-macos_arm64.tar.bz2",
        sha256: Some("6cb1429371e4854b718148a509d80143f801e3abfc72fef58d88aeeee1e98f9e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.10-macos_x86_64.tar.bz2",
        sha256: Some("399eb1ce4c65f62f6a096b7c273536601b7695e3c0dc0457393a659b95b7615b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.10-aarch64.tar.bz2",
        sha256: Some("e4caa1a545f22cfee87d5b9aa6f8852347f223643ad7d2562e0b2a2f4663ad98")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.10-linux32.tar.bz2",
        sha256: Some("b70ed7fdc73a74ebdc04f07439f7bad1a849aaca95e26b4a74049d0e483f071c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.10-s390x.tar.bz2",
        sha256: Some("c294f8e815158388628fe77ac5b8ad6cd93c8db1359091fa02d41cf6da4d61a1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.10-linux64.tar.bz2",
        sha256: Some("ceef6496fd4ab1c99e3ec22ce657b8f10f8bb77a32427fadfb5e1dd943806011")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.10-win64.zip",
        sha256: Some("362dd624d95bd64743190ea2539b97452ecb3d53ea92ceb2fbe9f48dc60e6b8f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.9-osx64.tar.bz2",
        sha256: Some("91a5c2c1facd5a4931a8682b7d792f7cf4f2ba25cd2e7e44e982139a6d5e4840")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.9-aarch64.tar.bz2",
        sha256: Some("5e124455e207425e80731dff317f0432fa0aba1f025845ffca813770e2447e32")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.9-linux32.tar.bz2",
        sha256: Some("4b261516c6c59078ab0c8bd7207327a1b97057b4ec1714ed5e79a026f9efd492")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.9-s390x.tar.bz2",
        sha256: Some("c6177a0016c9145c7b99fddb5d74cc2e518ccdb216a6deb51ef6a377510cc930")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.9-linux64.tar.bz2",
        sha256: Some("08be25ec82fc5d23b78563eda144923517daba481a90af0ace7a047c9c9a3c34")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.9-win64.zip",
        sha256: Some("05022baaa55db2b60880f2422312d9e4025e1267303ac57f33e8253559d0be88")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.8-osx64.tar.bz2",
        sha256: Some("de1b283ff112d76395c0162a1cf11528e192bdc230ee3f1b237f7694c7518dee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.8-aarch64-portable.tar.bz2",
        sha256: Some("0210536e9f1841ba283c13b04783394050837bb3e6f4091c9f1bd9c7f2b94b55")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.8-linux32.tar.bz2",
        sha256: Some("bea4b275decd492af6462157d293dd6fcf08a949859f8aec0959537b40afd032")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.8-s390x.tar.bz2",
        sha256: Some("ad53d373d6e275a41ca64da7d88afb6a17e48e7bfb2a6fff92daafdc06da6b90")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.8-linux64.tar.bz2",
        sha256: Some("089f8e3e357d6130815964ddd3507c13bd53e4976ccf0a89b5c36a9a6775a188")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.8-v7.3.8-win64.zip",
        sha256: Some("0894c468e7de758c509a602a28ef0ba4fbf197ccdf946c7853a7283d9bb2a345")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.9-osx64.tar.bz2",
        sha256: Some("12d92f578a200d50959e55074b20f29f93c538943e9a6e6522df1a1cc9cef542")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.9-aarch64.tar.bz2",
        sha256: Some("dfc62f2c453fb851d10a1879c6e75c31ffebbf2a44d181bb06fcac4750d023fc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.9-linux32.tar.bz2",
        sha256: Some("3398cece0167b81baa219c9cd54a549443d8c0a6b553ec8ec13236281e0d86cd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.9-s390x.tar.bz2",
        sha256: Some("fcab3b9e110379948217cf592229542f53c33bfe881006f95ce30ac815a6df48")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.9-linux64.tar.bz2",
        sha256: Some("c58195124d807ecc527499ee19bc511ed753f4f2e418203ca51bc7e3b124d5d1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 13,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.9-win64.zip",
        sha256: Some("8acb184b48fb3c854de0662e4d23a66b90e73b1ab73a86695022c12c745d8b00")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.8-osx64.tar.bz2",
        sha256: Some("76b8eef5b059a7e478f525615482d2a6e9feb83375e3f63c16381d80521a693f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.8-aarch64-portable.tar.bz2",
        sha256: Some("639c76f128a856747aee23a34276fa101a7a157ea81e76394fbaf80b97dcf2f2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.8-linux32.tar.bz2",
        sha256: Some("38429ec6ea1aca391821ee4fbda7358ae86de4600146643f2af2fe2c085af839")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.8-s390x.tar.bz2",
        sha256: Some("5c2cd3f7cf04cb96f6bcc6b02e271f5d7275867763978e66651b8d1605ef3141")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.8-linux64.tar.bz2",
        sha256: Some("409085db79a6d90bfcf4f576dca1538498e65937acfbe03bd4909bdc262ff378")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 12,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.8-win64.zip",
        sha256: Some("96df67492bc8d62b2e71dddf5f6c58965a26cac9799c5f4081401af0494b3bcc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.5-osx64.tar.bz2",
        sha256: Some("b3a7d3099ad83de7c267bb79ae609d5ce73b01800578ffd91ba7e221b13f80db")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.5-aarch64.tar.bz2",
        sha256: Some("85d83093b3ef5b863f641bc4073d057cc98bb821e16aa9361a5ff4898e70e8ee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.5-linux32.tar.bz2",
        sha256: Some("3dd8b565203d372829e53945c599296fa961895130342ea13791b17c84ed06c4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.5-s390x.tar.bz2",
        sha256: Some("dffdf5d73613be2c6809dc1a3cf3ee6ac2f3af015180910247ff24270b532ed5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.5-linux64.tar.bz2",
        sha256: Some("9000db3e87b54638e55177e68cbeb30a30fe5d17b6be48a9eb43d65b3ebcfc26")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 10,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.5-win64.zip",
        sha256: Some("072bd22427178dc4e65d961f50281bd2f56e11c4e4d9f16311c703f69f46ae24")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Darwin(None)),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.3-osx64.tar.bz2",
        sha256: Some("d72b27d5bb60813273f14f07378a08822186a66e216c5d1a768ad295b582438d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.3-aarch64.tar.bz2",
        sha256: Some("ee4aa041558b58de6063dd6df93b3def221c4ca4c900d6a9db5b1b52135703a8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.3-linux32.tar.bz2",
        sha256: Some("7d81b8e9fcd07c067cfe2f519ab770ec62928ee8787f952cadf2d2786246efc8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::S390x,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.3-s390x.tar.bz2",
        sha256: Some("92000d90b9a37f2e9cb7885f2a872adfa9e48e74bf7f84a8b8185c8181f0502d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_64,
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.3-linux64.tar.bz2",
        sha256: Some("37e2804c4661c86c857d709d28c7de716b000d31e89766599fdf5a98928b7096")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            prerelease: None,
            implementation: LenientImplementationName::Known(ImplementationName::PyPy),
            arch: Arch{
                family: target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
                variant: None,
            },
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
            variant: PythonVariant::Default
        },
        url: "https://downloads.python.org/pypy/pypy3.7-v7.3.3-win32.zip",
        sha256: Some("a282ce40aa4f853e877a5dbb38f0a586a29e563ae9ba82fd50c7e5dc465fb649")
    },
];
