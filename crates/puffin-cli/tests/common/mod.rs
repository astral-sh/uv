pub(crate) const BIN_NAME: &str = "puffin";
pub(crate) const INSTA_FILTERS: &[(&str, &str)] = &[
    (r"(\d+\.)?\d+(ms|s)", "[TIME]"),
    (r"#    .* pip-compile", "#    [BIN_PATH] pip-compile"),
    (r"--cache-dir .*", "--cache-dir [CACHE_DIR]"),
];
