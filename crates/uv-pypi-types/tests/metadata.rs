use std::assert_matches;

use astral_mail_headers::DecodeError;

use uv_normalize::ExtraName;
use uv_pypi_types::{Metadata10, Metadata23, MetadataError, ResolutionMetadata};

const HEADERS: &str = "Metadata-Version: 2.3\nName: demo\nVersion: 1.0\n";

#[test]
fn first_values_and_unknown_filtering() -> anyhow::Result<()> {
    let source = format!(
        "{HEADERS}\
         nAmE: =?utf-8?b?A?=\n\
         Version: 2.0\n\
         Requires-Python: UNKNOWN\n\
         Requires-Python: >=3.10\n\
         Summary: UNKNOWN\n\
         Summary: =?utf-8?b?A?=\n\
         Requires-Dist: UNKNOWN\n\
         Requires-Dist: idna\n\
         Requires-Dist: requests\n\
         Provides-Extra: UNKNOWN\n\
         Provides-Extra: test\n\
         Provides-Extra: test\n"
    );
    let identity = Metadata10::parse_pkg_info(source.as_bytes())?;
    assert_eq!(identity.name.as_str(), "demo");
    assert_eq!(identity.version, "1.0");
    for metadata in [
        ResolutionMetadata::parse_metadata(source.as_bytes())?,
        ResolutionMetadata::parse_pkg_info(source.as_bytes())?,
    ] {
        assert_eq!(metadata.name.as_str(), "demo");
        assert_eq!(metadata.version.to_string(), "1.0");
        assert!(metadata.requires_python.is_none());
        assert_eq!(
            metadata
                .requires_dist
                .iter()
                .map(|requirement| requirement.name.as_str())
                .collect::<Vec<_>>(),
            ["idna", "requests"]
        );
        assert_eq!(
            metadata
                .provides_extra
                .iter()
                .map(ExtraName::as_str)
                .collect::<Vec<_>>(),
            ["test", "test"]
        );
    }
    let metadata = Metadata23::parse(source.as_bytes())?;
    assert_eq!(metadata.name, "demo");
    assert_eq!(metadata.version, "1.0");
    assert!(metadata.requires_python.is_none());
    assert!(metadata.summary.is_none());
    assert_eq!(metadata.requires_dist, ["idna", "requests"]);
    assert_eq!(metadata.provides_extra, ["test", "test"]);

    let source = b"Metadata-Version: 2.3\nName: UNKNOWN\nName: demo\nVersion: 1.0\n";
    for result in [
        Metadata10::parse_pkg_info(source).map(|_| ()),
        ResolutionMetadata::parse_metadata(source).map(|_| ()),
        ResolutionMetadata::parse_pkg_info(source).map(|_| ()),
        Metadata23::parse(source).map(|_| ()),
    ] {
        assert_matches!(result, Err(MetadataError::FieldNotFound("Name")));
    }
    Ok(())
}

#[test]
fn consumed_values_propagate_decoding_errors() {
    for (value, expected) in [
        (
            "=?x-unknown?q?demo?=",
            DecodeError::UnsupportedCharset("x-unknown".to_owned()),
        ),
        ("=?utf-8?b?A?=", DecodeError::InvalidBase64),
    ] {
        let source = format!("Metadata-Version: 2.3\nName: {value}\nVersion: 1.0\n");
        for result in [
            Metadata10::parse_pkg_info(source.as_bytes()).map(|_| ()),
            ResolutionMetadata::parse_metadata(source.as_bytes()).map(|_| ()),
            ResolutionMetadata::parse_pkg_info(source.as_bytes()).map(|_| ()),
            Metadata23::parse(source.as_bytes()).map(|_| ()),
        ] {
            assert_matches!(result, Err(MetadataError::Decode(error)) if error == expected);
        }
    }
    for field in ["Requires-Dist", "Provides-Extra", "Dynamic"] {
        let source = format!("{HEADERS}{field}: idna\n{field}: =?utf-8?b?A?=\n");
        for result in [
            ResolutionMetadata::parse_metadata(source.as_bytes()).map(|_| ()),
            ResolutionMetadata::parse_pkg_info(source.as_bytes()).map(|_| ()),
            Metadata23::parse(source.as_bytes()).map(|_| ()),
        ] {
            assert_matches!(
                result,
                Err(MetadataError::Decode(DecodeError::InvalidBase64))
            );
        }
    }
}

#[test]
fn unconsumed_values_are_not_decoded() -> anyhow::Result<()> {
    let source = format!("{HEADERS}X-Unused: =?x-unknown?q?text?=\n");
    assert_eq!(Metadata23::parse(source.as_bytes())?.name, "demo");

    let source = format!("{source}Summary: =?utf-8?b?A?=\n");
    assert_eq!(
        Metadata10::parse_pkg_info(source.as_bytes())?.name.as_str(),
        "demo"
    );
    for metadata in [
        ResolutionMetadata::parse_metadata(source.as_bytes())?,
        ResolutionMetadata::parse_pkg_info(source.as_bytes())?,
    ] {
        assert_eq!(metadata.name.as_str(), "demo");
    }
    assert_matches!(
        Metadata23::parse(source.as_bytes()),
        Err(MetadataError::Decode(DecodeError::InvalidBase64))
    );
    Ok(())
}

#[test]
fn description_fallback_only_decodes_a_used_header() -> anyhow::Result<()> {
    let source = format!("{HEADERS}Description: =?utf-8?b?A?=\n\n body\r\n ");
    assert_eq!(
        Metadata23::parse(source.as_bytes())?.description.as_deref(),
        Some(" body\r\n ")
    );

    let source = format!("{HEADERS}Description: fallback\r\n text\r\n\r\n \t\r\n");
    assert_eq!(
        Metadata23::parse(source.as_bytes())?.description.as_deref(),
        Some("fallback text")
    );

    let source = format!("{HEADERS}Description: =?utf-8?b?A?=\n\n \t\n");
    assert_matches!(
        Metadata23::parse(source.as_bytes()),
        Err(MetadataError::Decode(DecodeError::InvalidBase64))
    );
    Ok(())
}

#[test]
fn only_publishing_validates_description_utf8() {
    let mut source = format!("{HEADERS}Description: fallback\n\n").into_bytes();
    source.push(0xff);
    assert!(Metadata10::parse_pkg_info(&source).is_ok());
    assert!(ResolutionMetadata::parse_metadata(&source).is_ok());
    assert!(ResolutionMetadata::parse_pkg_info(&source).is_ok());
    assert_matches!(
        Metadata23::parse(&source),
        Err(MetadataError::DescriptionEncoding(_))
    );
}

#[test]
fn malformed_lines_end_headers_and_become_the_description() -> anyhow::Result<()> {
    for line in ["colonless", "Bad Name: value"] {
        let body = format!("{line}\nRequires-Dist: idna\n");
        let source = format!("{HEADERS}Description: fallback\n{body}");
        for metadata in [
            ResolutionMetadata::parse_metadata(source.as_bytes())?,
            ResolutionMetadata::parse_pkg_info(source.as_bytes())?,
        ] {
            assert!(metadata.requires_dist.is_empty());
        }
        let metadata = Metadata23::parse(source.as_bytes())?;
        assert!(metadata.requires_dist.is_empty());
        assert_eq!(metadata.description.as_deref(), Some(body.as_str()));
    }
    Ok(())
}

#[test]
fn leading_continuations_and_lone_cr_are_recovered() -> anyhow::Result<()> {
    let source = b" orphan\rMetadata-Version: 2.3\rName: demo\rVersion: 1.0\r";
    assert_eq!(Metadata10::parse_pkg_info(source)?.name.as_str(), "demo");
    for metadata in [
        ResolutionMetadata::parse_metadata(source)?,
        ResolutionMetadata::parse_pkg_info(source)?,
    ] {
        assert_eq!(metadata.name.as_str(), "demo");
    }
    assert_eq!(Metadata23::parse(source)?.name, "demo");
    Ok(())
}
