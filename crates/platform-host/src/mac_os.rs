use crate::PlatformError;
use serde::Deserialize;

/// Get the macOS version from the SystemVersion.plist file.
pub(crate) fn get_mac_os_version() -> Result<(u16, u16), PlatformError> {
    // This is actually what python does
    // https://github.com/python/cpython/blob/cb2b3c8d3566ae46b3b8d0718019e1c98484589e/Lib/platform.py#L409-L428
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct SystemVersion {
        product_version: String,
    }
    let system_version: SystemVersion =
        plist::from_file("/System/Library/CoreServices/SystemVersion.plist")
            .map_err(|err| PlatformError::OsVersionDetectionError(err.to_string()))?;

    let invalid_mac_os_version = || {
        PlatformError::OsVersionDetectionError(format!(
            "Invalid macOS version {}",
            system_version.product_version
        ))
    };
    match system_version
        .product_version
        .split('.')
        .collect::<Vec<&str>>()
        .as_slice()
    {
        [major, minor] | [major, minor, _] => {
            let major = major.parse::<u16>().map_err(|_| invalid_mac_os_version())?;
            let minor = minor.parse::<u16>().map_err(|_| invalid_mac_os_version())?;
            Ok((major, minor))
        }
        _ => Err(invalid_mac_os_version()),
    }
}
