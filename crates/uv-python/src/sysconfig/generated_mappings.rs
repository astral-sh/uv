//! DO NOT EDIT
//!
//! Generated with `cargo run dev generate-sysconfig-metadata`
//! Targets from <https://github.com/astral-sh/python-build-standalone/blob/20260211/cpython-unix/targets.yml>
//!
#![allow(clippy::all)]
#![cfg_attr(any(), rustfmt::skip)]

use std::collections::BTreeMap;
use std::sync::LazyLock;

use crate::sysconfig::replacements::{ReplacementEntry, ReplacementMode};

/// Mapping for sysconfig keys to lookup and replace with the appropriate entry.
pub(crate) static DEFAULT_VARIABLE_UPDATES: LazyLock<BTreeMap<String, Vec<ReplacementEntry>>> = LazyLock::new(|| {
    BTreeMap::from_iter([
        ("BLDSHARED".to_string(), vec![
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabi-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabihf-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/loongarch64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mips-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mipsel-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/powerpc64le-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/riscv64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/s390x-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/x86_64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "clang".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "musl-clang".to_string() }, to: "cc".to_string() },
        ]),
        ("CC".to_string(), vec![
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabi-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabihf-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/loongarch64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mips-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mipsel-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/powerpc64le-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/riscv64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/s390x-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/x86_64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "clang".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "musl-clang".to_string() }, to: "cc".to_string() },
        ]),
        ("CXX".to_string(), vec![
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabi-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabihf-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/loongarch64-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mips-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mipsel-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/powerpc64le-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/riscv64-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/s390x-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/x86_64-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "clang++".to_string() }, to: "c++".to_string() },
        ]),
        ("LDCXXSHARED".to_string(), vec![
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabi-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabihf-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/loongarch64-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mips-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mipsel-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/powerpc64le-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/riscv64-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/s390x-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/x86_64-linux-gnu-g++".to_string() }, to: "c++".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "clang++".to_string() }, to: "c++".to_string() },
        ]),
        ("LDSHARED".to_string(), vec![
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabi-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabihf-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/loongarch64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mips-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mipsel-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/powerpc64le-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/riscv64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/s390x-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/x86_64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "clang".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "musl-clang".to_string() }, to: "cc".to_string() },
        ]),
        ("LINKCC".to_string(), vec![
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabi-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/arm-linux-gnueabihf-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/loongarch64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mips-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/mipsel-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/powerpc64le-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/riscv64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/s390x-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "/usr/bin/x86_64-linux-gnu-gcc".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "clang".to_string() }, to: "cc".to_string() },
            ReplacementEntry { mode: ReplacementMode::Partial { from: "musl-clang".to_string() }, to: "cc".to_string() },
        ]),
        ("AR".to_string(), vec![
            ReplacementEntry {
                mode: ReplacementMode::Full,
                to: "ar".to_string(),
            },
        ]),
    ])
});
