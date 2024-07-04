// DO NOT EDIT
//
// Generated with `crates/uv-python/template-download-metadata.py`
// From template at `crates/uv-python/src/downloads.inc.mustache`

pub(crate) const PYTHON_DOWNLOADS: &[ManagedPythonDownload] = &[
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("fa2b8c377f17dfb097a93c0fba217d93075a7ceba0cc877066e95be969e6b73d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("a4f17d1e3b4ea0e4c2a3664f232c0857979522936af582f7de92b57050220f74")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-armv7-unknown-linux-gnueabi-lto-full.tar.zst",
        sha256: Some("ab9c7ff6e686b2542470a5ecdb1bbfcb7fada7f8006b9b69a18cbdb52df1d7f4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-armv7-unknown-linux-gnueabihf-lto-full.tar.zst",
        sha256: Some("a35b08d09a6c1a483f83673c936ed24536a4d2a8930995267425a910d624f860")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("31bb3f579f3dcbbf3bf1dc71a188112e821cdfc77d21c9dbfe82ea78538110e1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("0a55708cdd94f415726bb8ea9a6c70453b9b5ea9f4864534bf14604831e95c71")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("98d6373e6394c1f25c5fb6f8a7dc7a5c6cad3e827bcdb6aca9a48435c043f1ed")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("e49da3f702da08a3e38d01c776cc2356e427217681964ff64a7880507e224a3c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("e51f6676a24c3551657347ef97963164eac801df0a62afcba8e0e28ebb62acee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("bf4ada23b9c52fba6e186b3d3c2ab64990c9e7a701a1f2451c8b61f897c3475b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.12.3%2B20240415-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("776568c92c5f3b47dbf5f17c1c58578f70d75a32654419a158aa8bdc6f95b09a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("2afcc8b25c55793f6ceb0bef2e547e101f53c9e25a0fe0332320e5381a1f0fdb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("2e87c0215aea1614e52ff8588b0ba41eb5ecf555e500094a179c0bbf1b25cbc7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("ee985ae6a6a98f4d5bd19fd8c59f45235911d19b64e1dbd026261b8103f15db5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("add34876a96cea321077e6f5826d99d4c7f1f4e1fdd27d709e4e385031af7b29")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("067d9b68a5ff3eafa27c25ca771e28bee66eb8f36bfe71e0330928b55c88cd91")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("b4b4d19c36e86803aa0b4410395f5568bef28d82666efba926e44dbe06345a12")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("67065f1215e4274edbc44fa368d7d64525a2601636842cff880c2ea538279e0c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("7ec1dc7ad8223ec5839a57d232fd3cf730987f7b0f88b2c4f15ee935bcabbaa9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.12.2%2B20240224-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("a1daf5e8ceb23d34ea29b16b5123b06694810fe7acc5c8384426435c63bf731e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("61e51e3490537b800fcefad718157cf775de41044e95aa538b63ab599f66f3a9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("3621be2cd8b5686e10a022f04869911cad9197a3ef77b30879fe25e792d7c249")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("22866d35fdf58e90e75d6ba9aa78c288b452ea7041fa9bc5549eca9daa431883")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("a94a35ecf61e9e4e9b9b64dcf1540371f35dbb3ca004018119091ca460cfffba")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("48aee32710363fffcad3a6fbe6461692a8b9ea23472e18fe030cca92506fff21")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("bf2b176b0426d7b4d4909c1b19bbb25b4893f9ebdc61e32df144df2b10dcc800")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("f267489a041daf4e523c03d32639de04ee59ca925dff49a8c3ce2f28a9f70a3b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("c4b07a02d8f0986b56e010a67132e5eeba1def4991c6c06ed184f831a484a06f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("d9bc1b566250bf51818976bf98bf50e1f4c59b2503b50d29250cac5ab5ef6b38")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("25fc8cd41e975d18d13bcc8f8beffa096ff8a0b86c4a737e1c6617900092c966")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("86e16b6defbbd7db0b7f98879b2b381e0e5b0ec07126cb9f5fc0cafe9869dc36")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("465e91b6e6d0d1c40c8a4bce3642c4adcb9b75cf03fbd5fd5a33a36358249289")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("3178a7dd91ade5db6aa9023b476235f6b8224520ce0bda322131f26fd3fdb858")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("aa02889eb40c9207cef648b41b6b80eb81a741a5a5f35dfa9fc2be0132b68e67")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("3b4781e7fd4efabe574ba0954e54c35c7d5ac4dc5b2990b40796c1c6aec67d79")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("5ce861907a2751a3a7395b1aaada830c2b072acc03f3dd0bcbaaa2b7a9166fc0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("91b42595cb4b69ff396e746dc492caf67b952a3ed1a367a4ace1acc965ed9cdb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 12,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.12.0%2B20231002-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("5bdff7ed56550d96f9b26a27a8c25f0cc58a03bff19e5f52bba84366183cab8b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("9a59eb9e8e509e742a25cada7b2c1123a56022081d91a8fbe48015cf495b0d0f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("c9f5e493c686ed8a5c38d1748c45fed18dc9b6faa70794d9cc9bb32489cc0b77")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-armv7-unknown-linux-gnueabi-lto-full.tar.zst",
        sha256: Some("756b3c75e3d76fe70683ec58ef203caa21110e55a2ed6dd22aedb3c645fe67c7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-armv7-unknown-linux-gnueabihf-lto-full.tar.zst",
        sha256: Some("c86e84722071dc9dba7cc999cc40d4d1667a1e93c7ff2fda64cbdc5b75565c95")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("f0405bb7e19e4dbf7db290c224fc4aa333d3e16e0ed571f0794becac620fa26a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("404002f47aa92ae43100ffe4cb98d65f6daaa6d30d1fe7474b6f700df997bed9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("b7e098dc0352220aea8ed640cafb9d8be6eeb3e46c708c66974a4ab53e404331")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("b1b156ceed6bc53c3c8816b3b5c3983d2c7070a8a42558b9c6dd730faec164e2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("1c5038da28a4379c065db85116594524010f30e653307c53bb9694e4e710d2c7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("1be233f7a60358681e84a62883485ac0672d55e0fb9191dd2d638a24d47be604")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.11.9%2B20240415-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("25344b08303f72ba2a37c33aa240fbd2c8d5a41bcce79cff63923b3d778c645c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("c0650884b929253b8688797d1955850f6e339bf0428b3d935f62ab3159f66362")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("1d84ed69e5acce555513e9261ce4b78bed19969b06a51a26b2781a375d70083d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("c3e90962996177a027bd73dd9fd8c42a2d6ef832cda26db4ab4efc6105160537")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("db0ab185489fa204d67d0ade3c2293edca3ceaa71867186218abd1ffcfb6db34")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("131407f74fb956bfdcbe5a88da3e569f32bb0533b0b2529bf63dd5983a17a47a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("54f8c8ad7313b3505e495bb093825d85eab244306ca4278836a2c7b5b74fb053")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("ae1bf11b438304622d9334092491266f908f26d76da03f1125514a192cf093f8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("a03a9d8c1f770ce418716a2e8185df7b3a9e0012cdc220f9f2d24480a432650b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.11.8%2B20240224-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("6da82390f7ac49f6c4b19a5b8019c4ddc1eef2c5ad6a2f2d32773a27663a4e14")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("c1f3dd13825906a5eae23ed8de9b653edb620568b2e0226eef3784eb1cce7eed")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("e066d3fb69162e401d2bb1f3c20798fde7c2fffcba0912d792e46d569b591ab3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("6613f1f9238d19969d8a2827deec84611cb772503207056cc9f0deb89bea48cd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("fc2e32d265a11da50d6154a65ece5161055cfd0450cdc94d69e21f021ecff38c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("5fee1c4d7bf370f9b74896e7575b5a94b36aba3d2d9d6746db72c8d3a6e2a4c1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("3f8caf73f2bfe22efa9666974c119727e163716e88af8ed3caa1e0ae5493de61")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("b7e19b262c19dfb82107e092ba3959b2da9b8bc53aafeb86727996afdb577221")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("f387d373d64447bbba8a5657712f93b1dbdfd7246cdfe5a0493f39b83d46ec7c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.11.7%2B20240107-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("89d1d8f080e5494ea57918fc5ecf3d483ffef943cd5a336e64da150cd44b4aa0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("6e9007bcbbf51203e89c34a87ed42561630a35bc4eb04a565c92ba7159fe5826")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("7c621a748a4fd6ae99d8ba7ec2da59173d31475838382a13df6d2b1bf95a7059")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("2670731428191d4476bf260c8144ccf06f9e5f8ac6f2de1dc444ca96ab627082")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("2b3fb2ea8ee2ca290c09dfbca43428b1e8f6853290a7bc99ec394091e2f4b228")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("b7a95b4861caa2cd66c1e272796048711cf063fb84f1e5b4ba447dc7593718a8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("3685156e4139e89484c071ba1a1b85be0b4e302a786de5a170d3b0713863c2e8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("6da291720c9fe2f63c5c55f7acc8b6094a05488453a84cfcc012e92305099ee7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("1b6e32ec93c5a18a03a9da9e2a3a3738d67b733df0795edcff9fd749c33ab931")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20231002/cpython-3.11.6%2B20231002-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("38d2c2fa2f9effbf486207bef7141d1b5c385ad30729ab0c976e6a852a2a9401")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("7bee180b764722a73c2599fbe2c3a6121cf6bbcb08cb3082851e93c43fe130e7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("cf131546383f0d9b81eca17c3fcb80508e01b11d9ca956d790c41baefb859d7d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("e156b972b72ae2703c13da3335b16ce5db9f1f33bac27cb0c444a59d04d918fc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("c9ffe9c2c88685ce3064f734cbdfede0a07de7d826fada58f8045f3bd8f81a9d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("0d46418101588602d7d817c24fbab7d6ce2675f0fb13d758b6a09b68c343b29d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("a4453d38dc9293326741c8062f53527bd9781eace71c89e5453de308522aa23a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("e43d70a49919641ca2939a5a9107b13d5fef8c13af0f511a33a94bb6af2044f0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("556d7d46c2af6f9744da03cac5304975f60de1cd5846a109814dd5c396fe9042")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("9dcf19ee54fb936cb9fd0f02fd655e790663534bc12e142e460c1b30a0b54dbd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.11.5%2B20230826-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("6e4d20e6d498f9edeb3c28cb9541ad20f675f16da350b078e40a9dcfd93cdc3d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("988d476c806f71a3233ff4266eda166a5d28cf83ba306ac88b4220554fc83e8c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("46982228f02dc6d8a1227289de479f938567ec8acaa361909a998a0196823809")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("1bf5ba6806abbe70770e8e00b2902cbbb75dd4ff0c6e992de85e6752a9998e1a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("0d22f43c5bb3f27ff2f9e8c60b0d7abd391bb2cac1790b0960970ff5580f6e9a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("17ad84cbfbfee0ebfe309ca389fe176e75fd864c82671186e1fd9926a0c5cae9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("e7cdb6602dbb5d58d04fca13f55b70c86759bf86b606cc544974876a0fcf662b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("6d9765785316c7f1c07def71b413c92c84302f798b30ee09e2e0b5da28353a51")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("b48061173c763971a28669585b47fa26cde98497eee6ebd8057849547b7282ee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("fc2ea02ced875c90b8d025b409d58c4f045df8ba951bfa2b8b0a3cfe11c3b41c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.11.4%2B20230726-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("1692d795d6199b2261161ae54250009ffad0317929302903f6f2c773befd4d76")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("cd296d628ceebf55a78c7f6a7aed379eba9dbd72045d002e1c2c85af0d6f5049")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("8b8e4c58070f8ff372cf89080f24ecb9154ccfcc7674a8a46d67bdb766a1ee95")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("58734b66ee8d2762911f32c6bf59f36928990dc637e494f9ac8ebdd589d64547")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("877c90ef778a526aa25ab417034f5e70728ac14e5eb1fa5cfd741f531203a3fc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("63a2f046e990bd2a790cc94a1cf04cb0dce9015a44bb633751bfb5b616fe01f2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("2fbb31a8bc6663e2d31d3054319b51a29b1915c03222a94b9d563233e11d1bef")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("b9e2e889a5797b181f086c175a03a0e011277a708199b2b20270bacfca72fb91")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("8c5adef5bc627f39e93b920af86ef740e917aa698530ff727978d446a07bbd8b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.11.3%2B20230507-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("9d27e607fb1cb2d766e17f27853013d8c0f0b09ac53127aaff03ec89ab13370d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("da187194cc351d827232b1d2d85b2855d7e25a4ada3e47bc34b4f87b1d989be5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("cd3b910dce032f0ec9b414156b391878010940368b5ea27c33b998016e9c1cb8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("cce57c5fbd3ff10b91d86978b7ad15b9e02f57447d4f429c0bd4e00aa676d389")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("b062ac2c72a85510fb9300675bd5c716baede21e9482ef6335247b4aa006584c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("0eb61be53ee13cf75a30b8a164ef513a2c7995b25b118a3a503245d46231b13a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("02332441cb610b1e1aa2d2972e261e2910cc6a950b7973cac22c0759a93c5fcd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("ec5da5b428f6d91d96cde2621c0380f67bb96e4257d2628bc70b50e75ec5f629")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 11,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.11.1%2B20230116-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("f5c46fffda7d7894b975af728f739b02d1cec50fd4a3ea49f69de9ceaae74b17")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("fa95c3a18e29234cf10c0befa2f08246307cab7f473ccc1804845be3caab076d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("630bbbba148557bf670fbd65eb7fcd3c212cac45387d135c02799a13967d0f3d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-armv7-unknown-linux-gnueabi-lto-full.tar.zst",
        sha256: Some("7dbd45c6b132907d5c04e3067e03e67da4a8876b2eaf4c3e10fbf214b3b4a7ca")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-armv7-unknown-linux-gnueabihf-lto-full.tar.zst",
        sha256: Some("0d08574d76a30c9ad730fc0cf3c625cd2a8cfa8d5fdfb821231c0e68ef05d713")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("4fafd723944ae98611005caf0ad7dbb262e02c61ddfa7c45f0d097f0965839a8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("8d39b9d99ae3c4672887796135f05336c2be83978b4d08b10ddb4d04ca25860c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("21708a30151b205d287ab6cbc1b665a91038efdc5bddbc2fc297f903aebf66ce")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("cc3fa88159a50d639dff84af9ffe2a50d6eda41b51037c755b5a13b88ce50153")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("add8cc6cbb4f2a3f8af2272e62b7604f7529a8c357c0af0f8a9f7d3dd444ef1e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("614d4e192276e3e3f030bd8eb8155e05e291bb5a8154af8a862e9f56afa74628")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.10.14%2B20240415-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("abc3041f0de7e700229c0628dfcba7ba1d15c8b2924621add7baf1554a088768")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("57b83a4aa32bdbe7611f1290313ef24f2574dff5fa59181c0ccb26c14c688b73")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("7f23a4afa4032a7c5a4e0ec926da37eea242472142613c2baa029ef61c3c493c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.10.13%2B20230826-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("cc5625a16fbec682d4ce40c0d185318164bd181efaa7eaf945ca63015db9fea3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("c8b99dcf267c574fdfbdf4e9d63ec7a4aa4608565fee3fba0b2f73843b9713b2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("26ac03f780005ad3d28e97ea72ed4deac2e570f4fc1b8b35cd1ec48b69191399")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("8bf42de2245406e3a201fd32454665d26a78afe183c62f81735c46c174b5c2b3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("a41c1e28e2a646bac69e023873d40a43c5958d251c6adfa83d5811a7cb034c7a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("d42f0dfa0245eb5d7cf26e86ce21ce6a92efb85bb2fb26c79a4657f18bae5fa1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("fd18e6039be25bf23d13caf5140569df71d61312b823b715b3c788747fec48e9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.10.13%2B20240224-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("6a2c8f37509556e5d463b1f437cdf7772ebd84cdf183c258d783e64bb3109505")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("a7d0cadbe867cc53dd47d7327244154157a7cca02edb88cf3bb760a4f91d4e44")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("bb5fa1d4ad202afc8ee4330f313c093760c9fb1af5be204dc0c6ba50c7610fea")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("159124ac71c86d8617eae17db6ed9b98f01078cc9bd76073261901826f2d940d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("0743b9976f20b06d9cf12de9d1b2dfe06b13f76978275e9dac73a275624bde2c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("bc23f5953c329b0155309e1ac40621da53cb3d1829912ff0616250410d876785")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("068a3621662e806f142945b622ff2f588f7da65db4cd6282640e6f891bfe9b9e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("f1fa448384dd48033825e56ee6b5afc76c5dd67dcf2b73b61d2b252ae2e87bca")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("79fe684338fa26e1af64de583cca77a3fd501d899420de398177952d5182d202")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("b343cbe7c41b7698b568ea5252328cdccb213100efa71da8d3db6e21afd9f6cf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.10.12%2B20230726-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("cb6e7c84d9e369a0ee76c9ea73d415a113ba9982db58f44e6bab5414838d35f3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("da9c8a3cd04485fd397387ea2fa56f3cac71827aafb51d8438b2868f86eb345b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("2e304c39d8af27f9abf1cf44653f5e34e7d05b665cb68e5a5474559c145e7b33")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("f55942f89c54c90af53dba603a86f90956eec87c7fb91f5dc2ae543373224ccd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("60e76e136ab23b891ed1212e58bd11a73a19cd9fd884ec1c5653ca1c159d674e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("d800e3152cd53ca6cfa376e22d66b784cd07ca6ec3fec053a15c6c2b011b9a17")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("e84c12aa0285235eed365971ceedf040f4d8014f5342d371e138a4da9e4e9b7c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("38931a156ed020f5c579af37b771871b99f31e74c34fa7e093e97eb1b2d4f978")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("7918188e01a266915dd0945711e274d45c8d7fb540d48240e13c4fd96f43afbb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.10.11%2B20230507-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("9b4dc4a335b6122ce783bc80f5015b683e3ab1a56054751c5df494db0521da67")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("2508b8d4b725bb45c3e03d2ddd2b8441f1a74677cb6bd6076e692c0923135ded")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("3d20f40654e4356bd42c4e70ec28f4b8d8dd559884467a4e1745c08729fb740a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("ae0745620168e65df44ae60b21622d488c9dd6ca83566083c565765256315283")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("3d79cfd229ec12b678bbfd79c30fb4cbad9950d6bfb29741d2315b11839998b4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("1153b4d3b03cf1e1d8ec93c098160586f665fcc2d162c0812140a716a688df58")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("c5f7ad956c8870573763ed58b59d7f145830a93378234b815c068c893c0d5c1e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("1310f187a73b00164ec4ca34e643841c5c34cbb93fe0b3a3f9504e5ea5001ec7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230116/cpython-3.10.9%2B20230116-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("4cfa6299a78a3959102c461d126e4869616f0a49c60b44220c000fc9aecddd78")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("f8ba5f87153a17717e900ff7bba20e2eefe8a53a5bd3c78f9f6922d6d910912d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("5710521ca6958dd2e50f30f2b1591eb7f6a4c55a64c9b66d3196f8257f40bc96")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("0ab3156bbdc87db8a9b938662a76bb405522b408b1f94d8eb20759f277f96cd8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("7547ea172f7fa3d7619855f28780da9feb615b6cb52c5c64d34f65b542799fee")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("a18f81ecc7da0779be960ad35c561a834866c0e6d1310a4f742fddfd6163753f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("59630be21c77f87b4378f0cf887cbeb6bec64c988c93f3dc795afee782a3322e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("bb87e933afcfd2e8de045e5a691feff1fb8fb06a09315b37d187762fddfc4546")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.10.8%2B20221106-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("ab40f9584be896c697c5fca351ab82d7b55f01b8eb0494f0a15a67562e49161a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("9f44cf63441a90f4ec99a032a2bda43971ae7964822daa0ee730a9cba15d50da")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("f92fb53661f2ceddeb7b15ae1f165671acf4e4d4f9519a87e033981b93ee33b8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("c379f2ef58c8d83f1607357ad75e860770d748232a4eec4263564cbfa6a3efbb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("323532701cb468199d6f14031b991f945d4bbf986ca818185e17e132d3763bdf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("e03e28dc9fe55ea5ca06fece8f2f2a16646b217d28c0cd09ebcd512f444fdc90")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("22e59fa43657dc3487392a44a33a815d507cdd244b6609b6ad08f2661c34169c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("7f2c933d23c0f38cf145c2d6c65b5cf53bb589690d394fd4c01b2230c23c2bff")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.10.7%2B20221002-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("5363974e6ee6c91dbd6bc3533e38b02a26abc2ff1c9a095912f237b916be22d3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("159230851a69cf5cab80318bce48674244d7c6304de81f44c22ff0abdf895cfa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("6606be4283ebcfe2d83b49b05f6d06b958fe120a4d96c1eeeb072369db06b827")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("213374fd9845df5c1d3f1d2f5ac2610fe70ddba094aee0cbc2e91fd2dc808de2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("8d9a259e15d5a1be48ef13cd5627d7f6c15eadf41a3539e99ed1deee668c075e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("9405499573a7aa8b67d070d096ded4f3e571f18c2b34762606ecc8025290b122")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("8072f01279e05bad7c8d1076715db243489d1c2598f7b7d0457d0cac44fcb8b2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("f859a72da0bb2f1261f8cebdac931b05b59474c7cb65cee8e85c34fc014dd452")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.10.6%2B20220802-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("01dc349721594b1bb5b582651f81479a24352f718fdf6279101caa0f377b160a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("f68d25dbe9daa96187fa9e05dd8969f46685547fecf1861a99af898f96a5379e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("6e5e1050549c1aa629924b1b6a3080655d9e110f88dfa734d9b1c98af924cc7d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("dea116554852261e4a9e79c8926a0e4ac483f9e624084ded73b30705e221b62d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("e201192f0aa73904bc5a5f43d1ce4c9fb243dfe02138e690676713fe02c7d662")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("5e372e6738a733532aa985730d9a47ee4c77b7c706e91ef61d37aacbb2e54845")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("2a71e32ef8e1bbffbbfcd1825620d6a8944f97e76851bf1a14dc4fa48b626db8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("3682e0add14a3bac654afe467a84981628b0c7ebdccd4ebf26dfaa916238e2fe")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220630/cpython-3.10.5%2B20220630-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("cff35feefe423d4282e9a3e1bb756d0acbb2f776b1ada82c44c71ac3e1491448")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("c404f226195d79933b1e0a3ec88f0b79d35c873de592e223e11008f3a37f83d6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("5d2ccef5a45d2287d73a6ff63a466b21a197beb373792e644b8881bce3b6aa55")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("b28224a798dea965cb090f831d31aa531c6b9a14028344be6df53ab426497bb4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("c37a47e46de93473916f700a790cb43515f00745fba6790004e2731ec934f4d3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("e447f00fe53168d18cbfe110645dbf33982a17580b9e4424a411f9245d99cd21")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("15f961b087c6145f326fee30041db4af3ce0a8d24bbdefbd8d24973825728a0e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("8b8b97f7746a3deca91ada408025457ced34f582dad2114b33ce6fec9cf35b28")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220528/cpython-3.10.4%2B20220528-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("d636dc1bcca74dd9c6e3b26f7c081b3e229336e8378fe554bf8ba65fe780a2ac")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("b1abefd0fc66922cf9749e4d5ceb97df4d3cfad0cd9cdc4bd04262a68d565698")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("88d2bfc8b714b9e36e95e68129799527077827dd752357934f9d3d0ce756871e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("ea82b0b12e03fdc461c2337e59cb901ecc763194588db5a97372d26f242f4951")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("fbc0924a138937fe435fcdb20b0c6241290558e07f158e5578bd91cc8acef469")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("bc5d6f284b506104ff6b4e36cec84cbdb4602dfed4c6fe19971a808eb8c439ec")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("ee2251d5e59045c6fa1d4431c8a5cd0ed18923a785e7e0f47aa9d32ae0ca344e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("7c034d8a5787744939335ce43d64f2ddcc830a74e63773408d0c8f3c3a4e7916")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.10.3%2B20220318-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("72b91d26f54321ba90a86a3bbc711fa1ac31e0704fec352b36e70b0251ffb13c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("1ef939fd471a9d346a7bc43d2c16fb483ddc4f98af6dad7f08a009e299977a1a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("fb714771145a49482a113f532e4cbc21d601cf0dee4186a57fbc66ddd8d85aef")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("817cc2720c9c67cf87e5c0e41e44111098ceb6372d8140c8adbdd2f0397f1e02")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("698b09b1b8321a4dc43d62f6230b62adcd0df018b2bcf5f1b4a7ce53dcf23bcc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("bacf720c13ab67685a384f1417e9c2420972d88f29c8b7c26e72874177f2d120")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("65d2a31c3181ab15342e60a2ef92d6a0df6945200191115d0303d6e77428521c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("df246cf27db346081935d33ce0344a185d1f08b04a4500eb1e21d4d922ee7eb4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.10.2%2B20220227-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("7397e78a4fbe429144adc1f33af942bdd5175184e082ac88f3023b3a740dd1a0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.10.0-aarch64-unknown-linux-gnu-lto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.10.0-i686-unknown-linux-gnu-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.10.0-i686-pc-windows-msvc-shared-pgo-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.10.0-x86_64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.10.0-x86_64-unknown-linux-gnu-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.10.0-x86_64-unknown-linux-musl-lto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 10,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.10.0-x86_64-pc-windows-msvc-shared-pgo-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("04fd532cfba9b3184a94feaf689bd6147759f1d34ddd674e8b2c146b37a994b1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("c462b6f2ab7d87b1000972ff6c1e797c86a1306cceee02cdcc81cd2240f44d34")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabi),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-armv7-unknown-linux-gnueabi-lto-full.tar.zst",
        sha256: Some("9102cce19d31f58c205c834e070a61fc1c80f03e87241c1daca7829d2a9c1f3d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Arm(target_lexicon::ArmArchitecture::Armv7)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnueabihf),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-armv7-unknown-linux-gnueabihf-lto-full.tar.zst",
        sha256: Some("148e939ceeb5d5755570cb5db5e51b0c3b6d2c86dba818a50439766ba6f98655")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("7e6edb16a3973fbb894f3cf4f60a34e22645e84621ec61c622cf4c5a2f4bf2a2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("06181e0a2f67818d0619556ebd8a06d837acd00b899bf7077d8146b3f4dcae8b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("dcb1fe5c643f79d46f436dfcfc8a9a72ecd82d41b5daf00ffe4c3d17e472092e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("2bb4ed2fc03bb05ac6680b8c11d3c64f7a7dd24b80089c5ad85a91ea4a1795aa")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("e1a07336705b58215f8ea138f4abee4640b1baa018e84a9ed44d9a47c7bfa0c8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("6b8d9f5e0c292cc564edcf12368be114a68bf941269df2473754d1cfb59e48fc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.9.19%2B20240415-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("e3611b5699c97bf5ac289e3636e8f932fb177997ee69a81b0c2b15c766ca6f13")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("579f9b68bbb3a915cbab9682e4d3c253bc96b0556b8a860982c49c25c61f974a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("93d7b15bf02a3191cfdee9d9d68bf2da782fc04cb142bcca6a4299fe524d9b37")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.9.18%2B20230826-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("9e40a541b4eb6eb0a5e2f35724a18332aea91c61e18dec77ca40da5cf2496839")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("212d413ab6f854f588cf368fdd2aa140bb7c7ee930e3f7ac1002cba1e50e9685")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("edac161e775d979f4e5e27bea242014038fcca79e2a3eff126b5991090631295")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("45f61aaa845588e2e43cdc28a523222e39270902ac475eed4ab6a21cc4987ead")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("146537b9b4a1baa672eed94373e149ca1ee339c4df121e8916d8436265e5245e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("7de4b74bd7f5bbe897339cb692652471de28a97910abe4f8382f744baec551cf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("8bf88ae2100e609902d98ec775468e3a41a834f6528e632d6d971f5f75340336")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.9.18%2B20240224-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("924ed4f375ef73c73a725ef18ec6a72726456673d5a116f132f60860a25dd674")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("2902e2a0add6d584999fa27896b721a359f7308404e936e80b01b07aa06e8f5e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("de2eab48ca487550258db38b38cb9372143283f757b3cf9ec522eb657e41a035")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("9984f59284048608f6734b032ff76e6bc3cb208e2235fdb511b0e478158fdb2b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("ffac27bfb8bdf615d0fc6cbbe0becaa65b6ae73feec417919601497fce2be0ab")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("c3233f54dd4719c98559a92725b89bd42d68bebd4e4065a4d567a30c7a98ca28")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::S390x),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-s390x-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("4e06fb7047b92a694638ee622e8d1da9d1df2f604a2726fe8a087f748032ee7c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("ba04f9813b78b61d60a27857949403a1b1dd8ac053e1f1aff72fe2689c238d3c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("cec2385699c047e77d32b93442417ab7d49c3e78c946cf586380dfe0b12a36dd")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("8496473a97e1dd43bf96fc1cf19f02f305608ef6a783e0112274e0ae01df4f2a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.9.17%2B20230726-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("209983b8227e4755197dfed4f6887e45b6a133f61e7eb913c0a934b0d0c3e00f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("c86ed2bf3ff290af10f96183c53e2b29e954abb520806fbe01d3ef2f9d809a75")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("6c516ed541e7f84ba8b322aa15006082701456bba7c57e68e7263d702927a76d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("4df4cae277ba3ff8de7a16ef3b38f7214c2b0e4cc992f09505b859b0c94f2fd8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("d7994b5febb375bb131d028f98f4902ba308913c77095457ccd159b521e20c52")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Powerpc64le),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-ppc64le-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("3fcbf3e1090461a16cb33c66e973ea2e918fe1373e51a84892268df6b7155648")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("5809626ca7907c8ea397341f3d5eafb280ed5b19cc5622e57b14d9b4362eba50")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("9fc89e1f3e1c03b4f5cd3c289f52e53a7c5fc8779113c2af5a10b19b2e8a2c2f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("c397f292021b33531248ad8fede24ef6249cc6172347b2017f92b4a71845b8ed")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230507/cpython-3.9.16%2B20230507-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("199c821505e287c004c3796ba9ac4bd129d7793e1d833e9a7672ed03bdb397d4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("1799b97619572ad595cd6d309bbcc57606138a57f4e90af04e04ee31d187e22f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("4012279410b28c2688b4acfbc9189cdc8c81ef4c4f83c5e4532c39cb8685530e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("7c5d8e6a4255115e96c4b987b76c203ae9c7e6655b2d52c880680f13d2f1af36")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("a5ad2a6ace97d458ad7b2857fba519c5c332362442d88e2b23ed818f243b8a78")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("50fd795eac55c4485e2fefbb8e7b365461817733c45becb50a7480a243e6000e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("b6860b9872f361af78021dd2e1fe7edfe821963deab91b9a813d12d706288d3d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("4597f0009cfb52e748a57badab28edf84a263390b777c182b18c36d666a01440")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.9.15%2B20221106-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("d0f3ce1748a51779eedf155aea617c39426e3f7bfd93b4876cb172576b6e8bda")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("6b9d2ff724aff88a4d0790c86f2e5d17037736f35a796e71732624191ddd6e38")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("b099375504383b3a30af02dcf3a9ce01b0e6fecba5b3a8729b4a0a374fee7984")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("612031ffd5b6dee7f4fe205afeee62a996bbd8df338ae7d0f3731a825aee04fb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("fae990eb312314102408cb0c0453dae670f0eb468f4cbf3e72327ceaa1276b46")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("186155e19b63da3248347415f888fbcf982c7587f6f927922ca243ae3f23ed2f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("7f88ff09b2b57c19f4262026b0919aca59558971838093c63b68dfce7834e84d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("5638c12d47eb81adf96615cea8a5a61e8414c3ac03a8b570d30ae9998cb6d030")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.9.14%2B20221002-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("49f27a3a18b4c2d765b0656c6529378a20b3e37fdb0aca9490576ff7a67243a9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("8612e9328663c0747d1eae36b218d11c2fbc53c39ec7512c7ad6b1b57374a5dc")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("e27d88c3c3424a3694f9f111dc4e881c3925aa5d9ec60ec8395a82da2d7c2f31")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("066d4722bcc75fb16000afd745b11fb5c02847471695c67db633918969e3936b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("3860abee418825c6a33f76fe88773fb05eb4bc724d246f1af063106d9ea3f999")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("16d21a6e62c19c574a4a225961e80966449095a8eb2c4150905e30d4e807cf86")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("e586b6fef3943adff4e74fbc3fe276dfbca12e9d883e273ed0c8d781b24d7d6e")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("c7e48545a8291fe1be909c4454b5c48df0ee4e69e2b5e13b6144b4199c31f895")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.9.13%2B20220802-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("6ef2b164cae483c61da30fb6d245762b8d6d91346d66cb421989d6d1462e5a48")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("b3d09b3c12295e893ee8f2cb60e8af94d8a21fc5c65016282925220f5270b85b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("0749e4f8169b45051c440c81c17449549710d0e5821d4fdb5170b704ddd165c4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("4a32d5f827e9c1fbed68e51974d78f090ccdd8c83f777a2c9f80644a96d53c3f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("361b8fa66d6b5d5623fd5e64af29cf220a693ba86d031bf7ce2b61e1ea50f568")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("825970ae30ae7a30a5b039aa25f1b965e2d1fe046e196e61fa2a3af8fef8c5d9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("9af4ad8e87d1d24352163d519df44f652efefe018b8c7b48ca57604054950abe")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("eb122ab2bf0b2d71926984bc7cf5fef65b415abfe01a0974ed6c1a2502fac764")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220502/cpython-3.9.12%2B20220502-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("c49f8b07e9c4dcfd7a5b55c131e882a4ebdf9f37fef1c7820c3ce9eb23bab8ab")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("6d9f20607a20e2cc5ad1428f7366832dc68403fc15f2e4f195817187e7b6dbbf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("e540f92f78cc84a52a77ce621c3da5a427367205884ab4210e763bc7fdaf889c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("aeb50fcc54214780244dd64c0d66bf5dec30db075c999cf2c5a58134f8d21c33")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("f06338422e7e3ad25d0cd61864bdb36d565d46440dd363cbb98821d388ed377a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("35e649618e7e602778e72b91c9c50c97d01a0c3509d16225a1f41dd0fd6575f0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("49dfa5cb99d4f71657dc651ad68d0fce7cc011beb59499141138ef062bd62b49")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("d83eb5c897120e32287cb6fe5c24dd2dcae00878b3f9d7002590d468bd5de0f1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220318/cpython-3.9.11%2B20220318-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("1fe3c519d43737dc7743aec43f72735e1429c79e06e3901b21bad67b642f1a10")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("ba1b63600ed8d9f3b8d739657bd8e7f5ca167de29a1a58d04b2cd9940b289464")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("a40dc3f12bbcaeb487d2ece8c5415f94f3856b400f78202b6055cd514c5e9a24")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("218a79ef09d599d95a04819311ee27ab0fd34dd80d3722347003fec0139dca7b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("7f3ca15f89775f76a32e6ea9b2c9778ebf0cde753c5973d4493959e75dd92488")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("ef2f090ff920708b4b9aa5d6adf0dc930c09a4bf638d71e6883091f9e629193d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("de0a1b11f56cd6acdbc4b369a023377fd830946726f3abbbce8fc11dcb56cac0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("2744b817f249c0563b844cddd5aba4cc2fd449489b8bd59980d7a31de3a4ece1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.9.10%2B20220227-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("56b2738599131d03b39b914ea0597862fd9096e5e64816bf19466bf026e74f0c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.9.7-aarch64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.9.7-aarch64-unknown-linux-gnu-lto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.9.7-i686-unknown-linux-gnu-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.9.7-i686-pc-windows-msvc-shared-pgo-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.9.7-x86_64-apple-darwin-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.9.7-x86_64-unknown-linux-gnu-pgo%2Blto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.9.7-x86_64-unknown-linux-musl-lto-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20211017/cpython-3.9.7-x86_64-pc-windows-msvc-shared-pgo-20211017T1616.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.9.6-aarch64-apple-darwin-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.9.6-aarch64-unknown-linux-gnu-lto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.9.6-i686-unknown-linux-gnu-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.9.6-i686-pc-windows-msvc-shared-pgo-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.9.6-x86_64-apple-darwin-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.9.6-x86_64-unknown-linux-gnu-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.9.6-x86_64-unknown-linux-musl-lto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.9.6-x86_64-pc-windows-msvc-shared-pgo-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-aarch64-apple-darwin-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-i686-unknown-linux-gnu-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-i686-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-apple-darwin-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-unknown-linux-gnu-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-unknown-linux-musl-lto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.9.5-x86_64-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-aarch64-apple-darwin-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-i686-unknown-linux-gnu-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-i686-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-apple-darwin-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-unknown-linux-gnu-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-unknown-linux-musl-lto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.9.4-x86_64-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-aarch64-apple-darwin-pgo%2Blto-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-i686-pc-windows-msvc-shared-pgo-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-apple-darwin-pgo%2Blto-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-unknown-linux-gnu-pgo%2Blto-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-unknown-linux-musl-lto-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210414/cpython-3.9.3-x86_64-pc-windows-msvc-shared-pgo-20210413T2055.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-aarch64-apple-darwin-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-i686-unknown-linux-gnu-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-i686-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-apple-darwin-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-unknown-linux-gnu-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-unknown-linux-musl-lto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.9.2-x86_64-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-i686-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-apple-darwin-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-unknown-linux-gnu-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-unknown-linux-musl-debug-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.9.1-x86_64-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-i686-pc-windows-msvc-shared-pgo-20201021T0245.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-apple-darwin-pgo-20201020T0626.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-unknown-linux-gnu-pgo-20201020T0627.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-unknown-linux-musl-debug-20201020T0627.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 9,
            patch: 0,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.9.0-x86_64-pc-windows-msvc-shared-pgo-20201021T0245.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.8.19%2B20240415-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("08cf698453d0a3080426a70dbb43220e915eb4401a9ea0fc798f9f27a3bf7f88")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.8.19%2B20240415-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("31415fdb0d8ea48f09d4a7e0e007f0dd77809be67f5e73ec803f6856d491c542")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.8.19%2B20240415-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("d1776a6eecc3bcf549fdbd7adcc9a1ee6e0f0dfaa8ad77f055f5972882d0d227")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.8.19%2B20240415-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("e011239aec57e2074093a31f6fb3fee036671ab777fb9764e32bfdb869a80652")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.8.19%2B20240415-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("778a2f806278f033c683b224aa595775c369717d477e0152b1293c9677ba9377")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.8.19%2B20240415-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("9c73ff563dd42389645923dd2502b95ae07f413576c5220045eb4a4afd07f6c1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 19,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240415/cpython-3.8.19%2B20240415-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("ff0812147ab19101c219d0648cf0dbe22b3612decd6034c286451dafe5fe5134")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("c732c068cddcd6a008c1d6d8e35802f5bdc7323bd2eb64e77210d3d5fe4740c2")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("df66801678a5f4accee67784aff058f283fd52e42898527b7ff0e1cbc3e50e8c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("9f94c7b54b97116cd308e73cda0b7a7b7fff4515932c5cbba18eeae9ec798351")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("4d4b65dd821ce13dcf6dfea3ad5c2d4c3d3a8c2b7dd49fc35c1d79f66238e89b")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("7ede28a7119056c24ea51766ac3cd9d3c5d579d3db133e02051b4bcb300507e9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("fa1bf64cf52d830e7b4bba486c447ee955af644d167df7c42afd169c5dc71d6a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 18,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.8.18%2B20240224-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("c63abd9365a13196eb9f65db864f95b85c1f90b770d218c1acd104e6b48a99d3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("d08a542bed35fc74ac6e8f6884c8aa29a77ff2f4ed04a06dcf91578dea622f9a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("efdf69695af469da13f86d5be23556fee6c03f417f8810fca55307a63aabf08d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("aaf4b15bdc35674dbe25d4538c9e75e243796a0cc8841fd31d7bbbee6703342a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("0931d8ca0e060c6ac1dfcf6bb9b6dea0ac3a9d95daf7906a88128045f4464bf8")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("2c4925f5cf37d498e0d8cfe7b10591cc5f0cd80d2582f566b12006e6f96958b1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("4bfe1055dee03d4357b3dca5b334df3076b8aab066cdd84596199b9712ee3632")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("a316ba0b1f425b04c8dfd7a8a18a05d72ae5852732d401b16d7439bdf25caec3")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 17,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230826/cpython-3.8.17%2B20230826-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("68c7d03de5283c4812f2706c797b2139999a28cec647bc662d1459a922059318")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("bfc91d0a1d6d6dfaa5a31c925aa6adae82bd1ae5eb17813a9f0a50bf9d3e6305")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("62c3e7b417a9c11fb7d251ee6f763c7dd2ae681017a82686122a8167f1b8c081")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("e8d832f16548e199e7c622eec9e06f746ba9dbbdf562dac8810c4e64e1f5115a")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("5de953621402c11cc7db65ba15d45779e838d7ce78e7aa8d43c7d78fff177f13")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("21c0f4a0fa6ee518b9f2f1901c9667e3baf45d9f84235408b7ca50499d19f56d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("446a1f600698167a3e70448787f61dd8b1e6fb8f50f50558c901a0f4d3c7a6d6")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("f7d46196b44d12a26209ac74061200aac478b96c253eea93a0b9734efa642779")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 16,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20230726/cpython-3.8.16%2B20230726-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("6316713c2dcb30127b38ced249fa9608830a33459580b71275a935aaa8cd5d5f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("fc0f944e6f01ed649f79c873af1c317db61d2136b82081b4d7cbb7755f878035")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("3a4975f1b0c196c98b4867ad41d2f1ba211b52dc6a2965c56acbb00eb7f69aa7")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("f76c0d13f600e819696035851ec47cf5a233cf053d2de85fbd8e5e12a8146f5f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("98bb2315c3567316c30b060d613c8d6067b368b64f08ef8fe6196341637c1d78")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("e4fd2fa2255295fbdcfadb8b48014fa80810305eccb246d355880aabb45cbe93")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("1fd71062d9b7d632af202972c4488fa9c2255d2ef072b80766ab059b37473ea5")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("231b35d3c2cff0372d17cea7ff5168c0684a920b94a912ffc965c2518cacb694")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 15,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221106/cpython-3.8.15%2B20221106-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("59beac5610e6da0848ebaccd72f91f6aaaeed65ef59606d006af909e9e79beba")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("d17a3fcc161345efa2ec0b4ab9c9ed6c139d29128f2e34bb636338a484aa7b72")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("650821c45386e7727b6e682620007d2532d9ee599b2caf4b4356575bee3c77a0")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("adb5a08f8dd700bc2d8260226354137349939e9bc5ccfdb8c16493e97b593a19")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("e43f7a5044eac91e95df59fd08bf96f13245898876fc2afd90a081cfcd847e35")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("62edfea77b42e87ca2d85c482319211cd2dd68d55ba85c99f1834f7b64a60133")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("5ca1c591ffb019fad3978018f68d69d4b6c73ce629fb7e42bc2c594cd8344d4f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("c6da442aaea160179a9379b297ccb3ba09b825fc27d84577fc28e62911451e7d")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 14,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20221002/cpython-3.8.14%2B20221002-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("6986b3e6edf7b37f96ea940b7ccba7b767ed3ea9b3faec2a2a60e5b2c4443314")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("a204e5f9e1566bdc170b163300a29fc9580d5c65cd6e896caf6500cd64471373")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-aarch64-unknown-linux-gnu-lto-full.tar.zst",
        sha256: Some("ad2b859fb502491f72f8d74ed3410bfb78a8886f8a1baa6908faea6128d91265")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("9191ac9858eddfc727fa5ebadc654a57a719ac96b9dee4e1e48e6498a27499f4")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("5630739d1c6fcfbf90311d236c5e46314fc4b439364429bee12d0ffc95e134fb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("f706a62de8582bf84b8b693c993314cd786f3e78639892cfd9a7283a526696f9")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("31c98d8329746c19739558f164e6374a2cd9c5c93c9e213d2548c993566a593c")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("410f3223021d1b439cf8e4da699f868adada2066e354d88a00b5f365dc66c4bf")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 13,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220802/cpython-3.8.13%2B20220802-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("c36b703b8b806a047ba71e5e85734ac78d204d3a2b7ebc2efcdc7d4af6f6c263")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::Aarch64(target_lexicon::Aarch64Architecture::Aarch64)),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-aarch64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("386f667f8d49b6c34aee1910cdc0b5b41883f9406f98e7d59a3753990b1cdbac")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-i686-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("61024acdfe5aef07ba4246ea07dba9962770ec1f3d137c54835c0e5b6e040149")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-i686-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("3e2e6c7de78b1924aad37904fed7bfbac6efa2bef05348e9be92180b2f2b1ae1")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-apple-darwin-pgo%2Blto-full.tar.zst",
        sha256: Some("cf614d96e2001d526061b3ce0569c79057fd0074ace472ff4f5f601262e08cdb")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-unknown-linux-gnu-pgo%2Blto-full.tar.zst",
        sha256: Some("a014cf132a642a5d585f37da0c56f7e6672699811726af18e8905d652b261a3f")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-unknown-linux-musl-lto-full.tar.zst",
        sha256: Some("3d958e3f984637d8ca4a90a2e068737b268f87fc615121a6f1808cd46ccacc48")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 12,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20220227/cpython-3.8.12%2B20220227-x86_64-pc-windows-msvc-shared-pgo-full.tar.zst",
        sha256: Some("33f278416ba8074f2ca6d7f8c17b311b60537c9e6431fd47948784c2a78ea227")
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.8.11-i686-unknown-linux-gnu-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.8.11-i686-pc-windows-msvc-shared-pgo-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.8.11-x86_64-apple-darwin-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.8.11-x86_64-unknown-linux-gnu-pgo%2Blto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.8.11-x86_64-unknown-linux-musl-lto-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 11,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210724/cpython-3.8.11-x86_64-pc-windows-msvc-shared-pgo-20210724T1424.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-i686-unknown-linux-gnu-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-i686-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-apple-darwin-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-unknown-linux-gnu-pgo%2Blto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-unknown-linux-musl-lto-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 10,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210506/cpython-3.8.10-x86_64-pc-windows-msvc-shared-pgo-20210506T0943.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-i686-unknown-linux-gnu-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-i686-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-apple-darwin-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-unknown-linux-gnu-pgo%2Blto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-unknown-linux-musl-lto-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210415/cpython-3.8.9-x86_64-pc-windows-msvc-shared-pgo-20210414T1515.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-i686-unknown-linux-gnu-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-i686-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-apple-darwin-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-unknown-linux-gnu-pgo%2Blto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-unknown-linux-musl-lto-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 8,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210327/cpython-3.8.8-x86_64-pc-windows-msvc-shared-pgo-20210327T1202.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-i686-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-apple-darwin-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-unknown-linux-gnu-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-unknown-linux-musl-debug-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20210103/cpython-3.8.7-x86_64-pc-windows-msvc-shared-pgo-20210103T1125.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-i686-pc-windows-msvc-shared-pgo-20201021T0233.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-apple-darwin-pgo-20201020T0626.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-unknown-linux-gnu-pgo-20201020T0627.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-unknown-linux-musl-debug-20201020T0627.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20201020/cpython-3.8.6-x86_64-pc-windows-msvc-shared-pgo-20201021T0232.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200830/cpython-3.8.5-i686-pc-windows-msvc-shared-pgo-20200830T2311.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200823/cpython-3.8.5-x86_64-apple-darwin-pgo-20200823T2228.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.8.5-x86_64-unknown-linux-gnu-pgo-20200823T0036.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.8.5-x86_64-unknown-linux-musl-debug-20200823T0036.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200830/cpython-3.8.5-x86_64-pc-windows-msvc-shared-pgo-20200830T2254.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.8.3-i686-pc-windows-msvc-shared-pgo-20200518T0154.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200530/cpython-3.8.3-x86_64-apple-darwin-pgo-20200530T1845.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.8.3-x86_64-unknown-linux-gnu-pgo-20200518T0040.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.8.3-x86_64-unknown-linux-musl-debug-20200518T0040.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.8.3-x86_64-pc-windows-msvc-shared-pgo-20200517T2207.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.8.2-i686-pc-windows-msvc-shared-pgo-20200418T2315.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-apple-darwin-pgo-20200418T2238.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-unknown-linux-gnu-pgo-20200418T2243.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 8,
            patch: 2,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200418/cpython-3.8.2-x86_64-pc-windows-msvc-shared-pgo-20200418T2315.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.7.9-i686-pc-windows-msvc-shared-pgo-20200823T0159.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200823/cpython-3.7.9-x86_64-apple-darwin-pgo-20200823T2228.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.7.9-x86_64-unknown-linux-gnu-pgo-20200823T0036.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.7.9-x86_64-unknown-linux-musl-debug-20200823T0036.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 9,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200822/cpython-3.7.9-x86_64-pc-windows-msvc-shared-pgo-20200823T0118.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.7.7-i686-pc-windows-msvc-shared-pgo-20200517T2153.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200530/cpython-3.7.7-x86_64-apple-darwin-pgo-20200530T1845.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.7.7-x86_64-unknown-linux-gnu-pgo-20200518T0040.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.7.7-x86_64-unknown-linux-musl-debug-20200518T0040.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 7,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200517/cpython-3.7.7-x86_64-pc-windows-msvc-shared-pgo-20200517T2128.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200216/cpython-3.7.6-windows-x86-shared-pgo-20200217T0110.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200216/cpython-3.7.6-macos-20200216T2344.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200216/cpython-3.7.6-linux64-20200216T2303.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200217/cpython-3.7.6-linux64-musl-20200218T0557.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 6,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20200216/cpython-3.7.6-windows-amd64-shared-pgo-20200217T0022.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20191025/cpython-3.7.5-windows-x86-20191025T0549.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20191025/cpython-3.7.5-macos-20191026T0535.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20191025/cpython-3.7.5-linux64-20191025T0506.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20191025/cpython-3.7.5-linux64-musl-20191026T0603.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 5,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20191025/cpython-3.7.5-windows-amd64-20191025T0540.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20190816/cpython-3.7.4-windows-x86-20190817T0235.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20190816/cpython-3.7.4-macos-20190817T0220.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20190816/cpython-3.7.4-linux64-20190817T0224.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20190816/cpython-3.7.4-linux64-musl-20190817T0227.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 4,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20190816/cpython-3.7.4-windows-amd64-20190817T0227.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686)),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20190617/cpython-3.7.3-windows-x86-20190709T0348.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Darwin),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20190617/cpython-3.7.3-macos-20190618T0523.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20190617/cpython-3.7.3-linux64-20190618T0324.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Musl),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20190617/cpython-3.7.3-linux64-musl-20190618T0400.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 3,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Windows),
            libc: Libc::None,
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20190617/cpython-3.7.3-windows-amd64-20190618T0516.tar.zst",
        sha256: None
    },
    ManagedPythonDownload {
        key: PythonInstallationKey {
            major: 3,
            minor: 7,
            patch: 1,
            implementation: LenientImplementationName::Known(ImplementationName::CPython),
            arch: Arch(target_lexicon::Architecture::X86_64),
            os: Os(target_lexicon::OperatingSystem::Linux),
            libc: Libc::Some(target_lexicon::Environment::Gnu),
        },
        url: "https://github.com/indygreg/python-build-standalone/releases/download/20181218/cpython-3.7.1-linux64-20181218T1905.tar.zst",
        sha256: None
    },
];
