//! Detect architecture versions from Linux CPU IDs and userspace features.

use procfs::{CpuInfo, Current};
use rustc_hash::FxHashSet;

use super::{BASELINE, M1, M2, M3, profiles};

pub(super) fn values() -> impl Iterator<Item = String> {
    host_versions()
        .unwrap_or_else(|| vec![BASELINE])
        .into_iter()
        .map(str::to_owned)
}

fn host_versions() -> Option<Vec<&'static str>> {
    versions(&CpuInfo::current().ok()?)
}

fn versions(cpu: &CpuInfo) -> Option<Vec<&'static str>> {
    // A VM can mask features while retaining the CPU ID. The provider compiles for
    // `-march=armv<version>-a`; check the userspace features that target enables and Linux
    // exposes. For example, without pointer authentication, offer at most Armv8.2-A.
    // https://github.com/llvm/llvm-project/blob/87f0227cb60147a26a1eeb4fb06e3b505e9c7261/llvm/lib/Target/AArch64/AArch64Features.td#L837-L880
    // https://docs.kernel.org/arch/arm64/elf_hwcaps.html
    let features: FxHashSet<_> = cpu
        .fields
        .get("Features")?
        .split_ascii_whitespace()
        .collect();
    let has = |flags: &str| {
        flags
            .split_ascii_whitespace()
            .all(|flag| features.contains(flag))
    };
    let mut supported = vec![BASELINE];
    for (version, required) in [
        ("8.1a", "crc32 atomics asimdrdm"),
        ("8.2a", "dcpop"),
        ("8.3a", "lrcpc paca pacg jscvt fcma"),
        ("8.4a", "asimddp asimdfhm dit flagm ilrcpc uscat"),
        ("8.5a", "flagm2 frint sb ssbs dcpodp bti"),
        ("8.6a", "bf16 i8mm"),
    ] {
        if !has(required) {
            break;
        }
        supported.push(version);
    }
    if supported.contains(&"8.5a") && has("fphp asimdhp sve sve2") {
        supported.push("9.0a");
    }

    // Linux reports "CPU architecture: 8" even on Armv9. Use the implementer and part to
    // identify each core, and keep only versions supported wherever the process can run.
    for core in 0..cpu.num_cores() {
        let part = cpu.get_field(core, "CPU part")?;
        let versions = match cpu.get_field(core, "CPU implementer")? {
            "0x61" => core_versions(part),
            implementer => profiles::linux_versions(implementer, part),
        }?;
        supported.retain(|version| versions.contains(version));
    }
    supported.reverse();
    Some(supported)
}

/// Linux reports each core's [Apple CPU part number] in `/proc/cpuinfo`.
///
/// [Apple CPU part number]:
///   https://github.com/apple-oss-distributions/xnu/blob/xnu-12377.121.6/osfmk/arm/cpuid.h#L161-L225
fn core_versions(part: &str) -> Option<&'static [&'static str]> {
    match part {
        // Icestorm and Firestorm.
        "0x020" | "0x021" | "0x022" | "0x023" | "0x024" | "0x025" | "0x028" | "0x029" => Some(M1),
        // Blizzard and Avalanche.
        "0x030" | "0x031" | "0x032" | "0x033" | "0x034" | "0x035" | "0x038" | "0x039" => Some(M2),
        // Palma efficiency and performance cores.
        "0x048" | "0x049" => Some(M3),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use procfs::{CpuInfo, FromRead};

    use super::versions;

    #[test]
    fn mixed_cores_and_masked_features() -> Result<(), Box<dyn std::error::Error>> {
        let features = concat!(
            "crc32 atomics asimdrdm dcpop lrcpc paca pacg jscvt fcma ",
            "asimddp asimdfhm dit flagm ilrcpc uscat flagm2 frint sb ssbs dcpodp bti"
        );
        let cpuinfo = format!(
            indoc! {"
                processor: 0
                Features: {features}
                CPU implementer: 0x61
                CPU part: 0x022

                processor: 1
                Features: {features}
                CPU implementer: 0x61
                CPU part: 0x030
            "},
            features = features,
        );
        let cpu = CpuInfo::from_read(cpuinfo.as_bytes())?;
        assert_eq!(
            versions(&cpu),
            Some(vec!["8.4a", "8.3a", "8.2a", "8.1a", "8a"])
        );

        let masked = cpuinfo.replace("paca ", "");
        let cpu = CpuInfo::from_read(masked.as_bytes())?;
        assert_eq!(versions(&cpu), Some(vec!["8.2a", "8.1a", "8a"]));

        let unknown = cpuinfo.replace("0x030", "0xfff");
        let cpu = CpuInfo::from_read(unknown.as_bytes())?;
        assert_eq!(versions(&cpu), None);
        Ok(())
    }
}
