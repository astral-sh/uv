#![cfg(feature = "tokio")]

use std::io;

use uv_fs::read_to_string_transcode;

#[tokio::test]
async fn transcodes_bom_marked_files() -> io::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    // Include a surrogate pair and an interior BOM that must remain part of the text.
    let contents = "# café 🐍\u{feff} end\r\npackage==1.0\n";
    let utf8 = contents.as_bytes().to_vec();
    let utf8_bom = [b"\xef\xbb\xbf".as_slice(), contents.as_bytes()].concat();
    let utf16_le = [0xff, 0xfe]
        .into_iter()
        .chain(contents.encode_utf16().flat_map(u16::to_le_bytes))
        .collect::<Vec<_>>();
    let utf16_be = [0xfe, 0xff]
        .into_iter()
        .chain(contents.encode_utf16().flat_map(u16::to_be_bytes))
        .collect::<Vec<_>>();

    for (filename, encoded, expected) in [
        ("utf8.txt", utf8, contents),
        ("utf8-bom.txt", utf8_bom, contents),
        ("utf16-le.txt", utf16_le, contents),
        ("utf16-be.txt", utf16_be, contents),
        ("empty.txt", vec![], ""),
        ("utf8-bom-only.txt", b"\xef\xbb\xbf".to_vec(), ""),
        ("utf16-le-bom-only.txt", b"\xff\xfe".to_vec(), ""),
        ("utf16-be-bom-only.txt", b"\xfe\xff".to_vec(), ""),
    ] {
        let path = temp_dir.path().join(filename);
        fs_err::write(&path, encoded)?;
        assert_eq!(
            read_to_string_transcode(path).await?,
            expected,
            "{filename}"
        );
    }
    Ok(())
}
