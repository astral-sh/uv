//! Integration tests for uv-delocate functionality.
//!
//! These tests use real Mach-O binaries from the delocate test suite.

use std::collections::HashSet;
use std::path::PathBuf;

use fs_err as fs;

use uv_delocate::{Arch, DelocateError, DelocateOptions};

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

mod macho_parsing {
    use super::*;

    #[test]
    fn test_is_macho_file() {
        let data_dir = test_data_dir();

        // Should recognize dylibs.
        assert!(uv_delocate::macho::is_macho_file(&data_dir.join("liba.dylib")).unwrap());
        assert!(uv_delocate::macho::is_macho_file(&data_dir.join("libb.dylib")).unwrap());
        assert!(uv_delocate::macho::is_macho_file(&data_dir.join("liba_both.dylib")).unwrap());

        // Should recognize .so files.
        assert!(
            uv_delocate::macho::is_macho_file(
                &data_dir.join("np-1.24.1_arm_random__sfc64.cpython-311-darwin.so")
            )
            .unwrap()
        );

        // Non-existent files should return false.
        assert!(!uv_delocate::macho::is_macho_file(&data_dir.join("nonexistent.dylib")).unwrap());
    }

    #[test]
    fn test_parse_single_arch_x86_64() {
        let data_dir = test_data_dir();
        let macho = uv_delocate::macho::parse_macho(&data_dir.join("liba.dylib")).unwrap();

        // Check architecture.
        assert!(macho.archs.contains(&Arch::X86_64));
        assert_eq!(macho.archs.len(), 1);

        // Check install ID.
        assert!(macho.install_id.is_some());
        let install_id = macho.install_id.as_ref().unwrap();
        assert_eq!(install_id.name, "liba.dylib");

        // Check dependencies; should have system libs.
        let dep_names: Vec<&str> = macho.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"/usr/lib/libc++.1.dylib"));
        assert!(dep_names.contains(&"/usr/lib/libSystem.B.dylib"));
    }

    #[test]
    fn test_parse_single_arch_arm64() {
        let data_dir = test_data_dir();
        let macho = uv_delocate::macho::parse_macho(&data_dir.join("libam1.dylib")).unwrap();

        // Check architecture.
        assert!(macho.archs.contains(&Arch::Arm64));
        assert_eq!(macho.archs.len(), 1);
    }

    #[test]
    fn test_parse_universal_binary() {
        let data_dir = test_data_dir();
        let macho = uv_delocate::macho::parse_macho(&data_dir.join("liba_both.dylib")).unwrap();

        // Should have both architectures.
        assert!(macho.archs.contains(&Arch::X86_64));
        assert!(macho.archs.contains(&Arch::Arm64));
        assert_eq!(macho.archs.len(), 2);
    }

    #[test]
    fn test_parse_with_dependencies() {
        let data_dir = test_data_dir();

        // libb depends on liba.
        let macho = uv_delocate::macho::parse_macho(&data_dir.join("libb.dylib")).unwrap();
        let dep_names: Vec<&str> = macho.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"liba.dylib"));

        // libc depends on liba and libb.
        let macho = uv_delocate::macho::parse_macho(&data_dir.join("libc.dylib")).unwrap();
        let dep_names: Vec<&str> = macho.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"liba.dylib"));
        assert!(dep_names.contains(&"libb.dylib"));
    }

    #[test]
    fn test_parse_with_rpath() {
        let data_dir = test_data_dir();
        let macho =
            uv_delocate::macho::parse_macho(&data_dir.join("libextfunc_rpath.dylib")).unwrap();

        // Should have @rpath dependencies.
        let dep_names: Vec<&str> = macho.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"@rpath/libextfunc2_rpath.dylib"));

        // Should have rpaths.
        assert!(!macho.rpaths.is_empty());
        let rpath_set: HashSet<&str> = macho
            .rpaths
            .iter()
            .map(std::string::String::as_str)
            .collect();
        assert!(rpath_set.contains("@loader_path/"));
        assert!(rpath_set.contains("@executable_path/"));
    }

    #[test]
    fn test_parse_numpy_extension() {
        let data_dir = test_data_dir();

        // x86_64 numpy extension.
        let macho = uv_delocate::macho::parse_macho(
            &data_dir.join("np-1.24.1_x86_64_random__sfc64.cpython-311-darwin.so"),
        )
        .unwrap();
        assert!(macho.archs.contains(&Arch::X86_64));

        // arm64 numpy extension.
        let macho = uv_delocate::macho::parse_macho(
            &data_dir.join("np-1.24.1_arm_random__sfc64.cpython-311-darwin.so"),
        )
        .unwrap();
        assert!(macho.archs.contains(&Arch::Arm64));
    }
}

mod arch_checking {
    use super::*;

    #[test]
    fn test_check_archs_single_present() {
        let data_dir = test_data_dir();
        // x86_64 only library.
        assert!(
            uv_delocate::macho::check_archs(&data_dir.join("liba.dylib"), &[Arch::X86_64]).is_ok()
        );
    }

    #[test]
    fn test_check_archs_single_missing() {
        let data_dir = test_data_dir();
        // x86_64 library doesn't have arm64.
        let result = uv_delocate::macho::check_archs(&data_dir.join("liba.dylib"), &[Arch::Arm64]);
        assert!(matches!(
            result,
            Err(DelocateError::MissingArchitecture { .. })
        ));
    }

    #[test]
    fn test_check_archs_universal() {
        let data_dir = test_data_dir();
        // Universal binary should have both.
        assert!(
            uv_delocate::macho::check_archs(&data_dir.join("liba_both.dylib"), &[Arch::X86_64])
                .is_ok()
        );
        assert!(
            uv_delocate::macho::check_archs(&data_dir.join("liba_both.dylib"), &[Arch::Arm64])
                .is_ok()
        );
        assert!(
            uv_delocate::macho::check_archs(
                &data_dir.join("liba_both.dylib"),
                &[Arch::X86_64, Arch::Arm64]
            )
            .is_ok()
        );
    }
}

mod wheel_operations {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_unpack_wheel() {
        let data_dir = test_data_dir();
        let temp_dir = TempDir::new().unwrap();

        uv_delocate::wheel::unpack_wheel(
            &data_dir.join("fakepkg1-1.0-cp36-abi3-macosx_10_9_universal2.whl"),
            temp_dir.path(),
        )
        .unwrap();

        // Check that dist-info exists.
        assert!(temp_dir.path().join("fakepkg1-1.0.dist-info").exists());
        assert!(
            temp_dir
                .path()
                .join("fakepkg1-1.0.dist-info/WHEEL")
                .exists()
        );
        assert!(
            temp_dir
                .path()
                .join("fakepkg1-1.0.dist-info/RECORD")
                .exists()
        );

        // Check that package files exist.
        assert!(temp_dir.path().join("fakepkg1").exists());
    }

    #[test]
    fn test_find_dist_info() {
        let data_dir = test_data_dir();
        let temp_dir = TempDir::new().unwrap();

        uv_delocate::wheel::unpack_wheel(
            &data_dir.join("fakepkg1-1.0-cp36-abi3-macosx_10_9_universal2.whl"),
            temp_dir.path(),
        )
        .unwrap();

        let dist_info = uv_delocate::wheel::find_dist_info(temp_dir.path()).unwrap();
        assert_eq!(dist_info, "fakepkg1-1.0.dist-info");
    }

    #[test]
    fn test_unpack_repack_wheel() {
        let data_dir = test_data_dir();
        let temp_dir = TempDir::new().unwrap();
        let unpack_dir = temp_dir.path().join("unpacked");
        let output_wheel = temp_dir.path().join("output.whl");

        fs::create_dir(&unpack_dir).unwrap();

        // Unpack.
        uv_delocate::wheel::unpack_wheel(
            &data_dir.join("fakepkg2-1.0-py3-none-any.whl"),
            &unpack_dir,
        )
        .unwrap();

        // Repack.
        uv_delocate::wheel::pack_wheel(&unpack_dir, &output_wheel).unwrap();

        // Verify the output is a valid zip.
        assert!(output_wheel.exists());
        let file = fs::File::open(&output_wheel).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        assert!(!archive.is_empty());
    }
}

#[cfg(target_os = "macos")]
mod install_name_modification {
    use super::*;
    use tempfile::TempDir;

    fn copy_dylib(name: &str, temp_dir: &TempDir) -> PathBuf {
        let src = test_data_dir().join(name);
        let dst = temp_dir.path().join(name);
        fs::copy(&src, &dst).unwrap();
        dst
    }

    #[test]
    fn test_change_install_name() {
        let temp_dir = TempDir::new().unwrap();
        let dylib = copy_dylib("libextfunc_rpath.dylib", &temp_dir);

        // This dylib depends on @rpath/libextfunc2_rpath.dylib.
        // Change it to a shorter @loader_path path (which will fit).
        uv_delocate::macho::change_install_name(
            &dylib,
            "@rpath/libextfunc2_rpath.dylib",
            "@loader_path/ext2.dylib",
        )
        .unwrap();

        // Verify the change.
        let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
        let dep_names: Vec<&str> = macho.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"@loader_path/ext2.dylib"));
        assert!(!dep_names.contains(&"@rpath/libextfunc2_rpath.dylib"));
    }

    #[test]
    fn test_change_install_name_longer_via_install_name_tool() {
        let temp_dir = TempDir::new().unwrap();
        let dylib = copy_dylib("libb.dylib", &temp_dir);

        // libb depends on liba.dylib (short name).
        // Change to a longer name - should succeed via install_name_tool fallback.
        uv_delocate::macho::change_install_name(
            &dylib,
            "liba.dylib",
            "@loader_path/long/path/liba.dylib",
        )
        .unwrap();

        // Verify the change.
        let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
        let dep_names: Vec<&str> = macho.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"@loader_path/long/path/liba.dylib"));
    }

    #[test]
    fn test_change_install_name_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let dylib = copy_dylib("liba.dylib", &temp_dir);

        // Get original dependencies.
        let original = uv_delocate::macho::parse_macho(&dylib).unwrap();
        let original_deps: Vec<_> = original
            .dependencies
            .iter()
            .map(|d| d.name.clone())
            .collect();

        // liba doesn't depend on "nonexistent.dylib".
        // install_name_tool silently does nothing if the old name doesn't exist.
        let result = uv_delocate::macho::change_install_name(
            &dylib,
            "nonexistent.dylib",
            "@loader_path/foo.dylib",
        );
        assert!(result.is_ok());

        // Verify dependencies are unchanged.
        let after = uv_delocate::macho::parse_macho(&dylib).unwrap();
        let after_deps: Vec<_> = after.dependencies.iter().map(|d| d.name.clone()).collect();
        assert_eq!(original_deps, after_deps);
    }

    #[test]
    fn test_change_install_id() {
        let temp_dir = TempDir::new().unwrap();
        let dylib = copy_dylib("libextfunc_rpath.dylib", &temp_dir);

        // Change install ID (original is @rpath/libextfunc_rpath.dylib).
        // Use a shorter name that fits.
        uv_delocate::macho::change_install_id(&dylib, "@loader_path/ext.dylib").unwrap();

        // Verify the change.
        let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
        assert!(macho.install_id.is_some());
        assert_eq!(
            macho.install_id.as_ref().unwrap().name,
            "@loader_path/ext.dylib"
        );
    }

    #[test]
    fn test_change_install_id_longer_via_install_name_tool() {
        let temp_dir = TempDir::new().unwrap();
        let dylib = copy_dylib("liba.dylib", &temp_dir);

        // Original ID is "liba.dylib" (short).
        // Change to a longer name - should succeed via install_name_tool fallback.
        uv_delocate::macho::change_install_id(&dylib, "@loader_path/long/path/liba.dylib").unwrap();

        // Verify the change.
        let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
        assert!(macho.install_id.is_some());
        assert_eq!(
            macho.install_id.as_ref().unwrap().name,
            "@loader_path/long/path/liba.dylib"
        );
    }

    #[test]
    fn test_change_install_name_universal_binary() {
        let temp_dir = TempDir::new().unwrap();
        let dylib = copy_dylib("libextfunc_rpath.dylib", &temp_dir);

        // Change install ID in universal binary - should update both slices.
        // Original is @rpath/libextfunc_rpath.dylib.
        uv_delocate::macho::change_install_id(&dylib, "@loader_path/ext.dylib").unwrap();

        // Verify both architectures see the change.
        let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
        assert!(macho.install_id.is_some());
        assert_eq!(
            macho.install_id.as_ref().unwrap().name,
            "@loader_path/ext.dylib"
        );
        // Should still have both architectures.
        assert!(macho.archs.contains(&Arch::X86_64));
        assert!(macho.archs.contains(&Arch::Arm64));
    }
}

mod delocate_integration {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_list_wheel_dependencies_pure_python() {
        let data_dir = test_data_dir();
        let deps =
            uv_delocate::list_wheel_dependencies(&data_dir.join("fakepkg2-1.0-py3-none-any.whl"))
                .unwrap();

        // Pure Python wheel should have no external dependencies.
        assert!(deps.is_empty());
    }

    #[test]
    fn test_delocate_pure_python_wheel() {
        let data_dir = test_data_dir();
        let temp_dir = TempDir::new().unwrap();

        let result = uv_delocate::delocate_wheel(
            &data_dir.join("fakepkg2-1.0-py3-none-any.whl"),
            temp_dir.path(),
            &DelocateOptions::default(),
        )
        .unwrap();

        // Should just copy the wheel unchanged.
        assert!(result.exists());
        assert_eq!(result.file_name().unwrap(), "fakepkg2-1.0-py3-none-any.whl");
    }

    #[test]
    fn test_delocate_wheel_with_require_archs() {
        let data_dir = test_data_dir();
        let temp_dir = TempDir::new().unwrap();

        // This should work because the wheel has universal2 binaries.
        let options = DelocateOptions {
            require_archs: vec![Arch::X86_64, Arch::Arm64],
            ..Default::default()
        };

        let result = uv_delocate::delocate_wheel(
            &data_dir.join("fakepkg1-1.0-cp36-abi3-macosx_10_9_universal2.whl"),
            temp_dir.path(),
            &options,
        );

        // The wheel should be processed (it may or may not have external deps).
        assert!(result.is_ok());
    }
}

mod macos_version {
    use super::*;

    #[test]
    fn test_parse_macos_version_from_binary() {
        let data_dir = test_data_dir();

        // Parse a modern ARM64 binary.
        let macho = uv_delocate::macho::parse_macho(
            &data_dir.join("np-1.24.1_arm_random__sfc64.cpython-311-darwin.so"),
        )
        .unwrap();

        // ARM64 binaries require macOS 11.0 or later.
        if let Some(version) = macho.min_macos_version {
            assert!(version.major >= 11, "ARM64 binary should require macOS 11+");
        }
    }

    #[test]
    fn test_parse_macos_version_from_x86_binary() {
        let data_dir = test_data_dir();

        // Parse an x86_64 binary.
        let macho = uv_delocate::macho::parse_macho(
            &data_dir.join("np-1.24.1_x86_64_random__sfc64.cpython-311-darwin.so"),
        )
        .unwrap();

        // x86_64 binaries can target older macOS versions.
        if let Some(version) = macho.min_macos_version {
            // Just verify we can read it.
            assert!(version.major >= 10);
        }
    }

    #[test]
    fn test_macos_version_ordering() {
        use uv_delocate::MacOSVersion;

        let v10_9 = MacOSVersion::new(10, 9, 0);
        let v10_15 = MacOSVersion::new(10, 15, 0);
        let v11_0 = MacOSVersion::new(11, 0, 0);
        let v14_0 = MacOSVersion::new(14, 0, 0);

        assert!(v10_9 < v10_15);
        assert!(v10_15 < v11_0);
        assert!(v11_0 < v14_0);
        assert!(v10_9 < v14_0);
    }

    #[test]
    fn test_macos_version_display() {
        use uv_delocate::MacOSVersion;

        assert_eq!(MacOSVersion::new(10, 9, 0).to_string(), "10.9.0");
        assert_eq!(MacOSVersion::new(11, 0, 0).to_string(), "11.0.0");
        assert_eq!(MacOSVersion::new(14, 2, 1).to_string(), "14.2.1");
    }
}

#[cfg(target_os = "macos")]
mod code_signing {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn copy_dylib(name: &str, temp_dir: &TempDir) -> PathBuf {
        let src = test_data_dir().join(name);
        let dest = temp_dir.path().join(name);
        fs::copy(&src, &dest).unwrap();
        dest
    }

    #[test]
    fn test_codesign_after_modification() {
        let temp_dir = TempDir::new().unwrap();
        let dylib = copy_dylib("liba.dylib", &temp_dir);

        // Modify the install ID.
        uv_delocate::macho::change_install_id(&dylib, "@loader_path/new_id.dylib").unwrap();

        // Verify the binary is still valid (codesign should have been applied).
        let output = Command::new("codesign")
            .args(["--verify", &dylib.to_string_lossy()])
            .output()
            .unwrap();

        // Ad-hoc signed binaries should verify successfully.
        assert!(
            output.status.success(),
            "Binary should be valid after modification: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn test_codesign_after_rpath_deletion() {
        let temp_dir = TempDir::new().unwrap();
        let dylib = copy_dylib("libextfunc_rpath.dylib", &temp_dir);

        // Parse and check rpaths.
        let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();

        // Delete an rpath if there is one.
        if !macho.rpaths.is_empty() {
            let rpath = &macho.rpaths[0];
            uv_delocate::macho::delete_rpath(&dylib, rpath).unwrap();

            // Verify the binary is still valid.
            let output = Command::new("codesign")
                .args(["--verify", &dylib.to_string_lossy()])
                .output()
                .unwrap();

            assert!(
                output.status.success(),
                "Binary should be valid after rpath deletion"
            );
        }
    }
}

mod wheel_metadata {
    use std::str::FromStr;
    use uv_distribution_filename::WheelFilename;
    use uv_platform_tags::{BinaryFormat, PlatformTag};

    #[test]
    fn test_wheel_filename_with_platform() {
        let filename =
            WheelFilename::from_str("foo-1.0-cp311-cp311-macosx_10_9_x86_64.whl").unwrap();

        // Test generating new filename with updated platform.
        let new_name = uv_delocate::wheel::filename_with_platform(
            &filename,
            &[PlatformTag::Macos {
                major: 11,
                minor: 0,
                binary_format: BinaryFormat::X86_64,
            }],
        );
        assert_eq!(new_name, "foo-1.0-cp311-cp311-macosx_11_0_x86_64.whl");
    }
}

mod rpath_handling {
    use super::*;
    #[cfg(target_os = "macos")]
    use tempfile::TempDir;

    #[cfg(target_os = "macos")]
    fn copy_dylib(name: &str, temp_dir: &TempDir) -> PathBuf {
        let src = test_data_dir().join(name);
        let dest = temp_dir.path().join(name);
        fs::copy(&src, &dest).unwrap();
        dest
    }

    #[test]
    fn test_parse_rpath_from_binary() {
        let data_dir = test_data_dir();

        // This binary has rpaths.
        let macho =
            uv_delocate::macho::parse_macho(&data_dir.join("libextfunc_rpath.dylib")).unwrap();

        // Should have at least one rpath.
        assert!(!macho.rpaths.is_empty(), "Binary should have rpaths");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_delete_rpath() {
        let temp_dir = TempDir::new().unwrap();
        let dylib = copy_dylib("libextfunc_rpath.dylib", &temp_dir);

        // Parse the binary and get rpaths.
        let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
        let original_count = macho.rpaths.len();

        if original_count > 0 {
            let rpath_to_delete = macho.rpaths[0].clone();
            uv_delocate::macho::delete_rpath(&dylib, &rpath_to_delete).unwrap();

            // Verify the rpath was deleted.
            let macho = uv_delocate::macho::parse_macho(&dylib).unwrap();
            assert_eq!(macho.rpaths.len(), original_count - 1);
            assert!(!macho.rpaths.contains(&rpath_to_delete));
        }
    }
}
