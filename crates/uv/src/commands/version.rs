use anyhow::Result;

use crate::commands::VersionFormat;

/// Display version information
pub(crate) fn version(output_format: VersionFormat, buffer: &mut dyn std::io::Write) -> Result<()> {
    let version_info = crate::version::version();

    match output_format {
        VersionFormat::Text => {
            writeln!(buffer, "uv {}", &version_info)?;
        }
        VersionFormat::Json => {
            serde_json::to_writer_pretty(&mut *buffer, &version_info)?;
            // Add a trailing newline
            writeln!(buffer)?;
        }
    };
    Ok(())
}
