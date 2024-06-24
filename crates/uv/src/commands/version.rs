use anyhow::Result;

use uv_cli::VersionFormat;

/// Display version information
pub(crate) fn version(output_format: VersionFormat, buffer: &mut dyn std::io::Write) -> Result<()> {
    let version_info = uv_cli::version::version();

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
