use super::*;

#[test]
fn parse_sha256() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "download.pytorch.org",
                    ),
                ),
                port: None,
                path: "/whl/jinja2/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.2-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: Some(
                        "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    ),
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                yanked: None,
            },
        ],
    }
    "###);
}

#[test]
fn parse_md5() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl#md5=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "download.pytorch.org",
                    ),
                ),
                port: None,
                path: "/whl/jinja2/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.2-py3-none-any.whl",
                hashes: Hashes {
                    md5: Some(
                        "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    ),
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.2-py3-none-any.whl#md5=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                yanked: None,
            },
        ],
    }
    "###);
}

#[test]
fn parse_base() {
    let text = r#"
<!DOCTYPE html>
<html>
<head>
<base href="https://index.python.org/">
</head>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "index.python.org",
                    ),
                ),
                port: None,
                path: "/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.2-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: Some(
                        "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    ),
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                yanked: None,
            },
        ],
    }
    "###);
}

#[test]
fn parse_escaped_fragment() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2&#43;233fca715f49-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2+233fca715f49-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "download.pytorch.org",
                    ),
                ),
                port: None,
                path: "/whl/jinja2/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.2+233fca715f49-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: Some(
                        "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    ),
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.2+233fca715f49-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                yanked: None,
            },
        ],
    }
    "###);
}

#[test]
fn parse_encoded_fragment() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha256%3D4095ada29e51070f7d199a0a5bdf5c8d8e238e03f0bf4dcc02571e78c9ae800d">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "download.pytorch.org",
                    ),
                ),
                port: None,
                path: "/whl/jinja2/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.2-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: Some(
                        "4095ada29e51070f7d199a0a5bdf5c8d8e238e03f0bf4dcc02571e78c9ae800d",
                    ),
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.2-py3-none-any.whl#sha256%3D4095ada29e51070f7d199a0a5bdf5c8d8e238e03f0bf4dcc02571e78c9ae800d",
                yanked: None,
            },
        ],
    }
    "###);
}

#[test]
fn parse_quoted_filepath() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="cpu/torchtext-0.17.0%2Bcpu-cp39-cp39-win_amd64.whl">cpu/torchtext-0.17.0%2Bcpu-cp39-cp39-win_amd64.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "download.pytorch.org",
                    ),
                ),
                port: None,
                path: "/whl/jinja2/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "torchtext-0.17.0+cpu-cp39-cp39-win_amd64.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "cpu/torchtext-0.17.0%2Bcpu-cp39-cp39-win_amd64.whl",
                yanked: None,
            },
        ],
    }
    "###);
}

#[test]
fn parse_missing_hash() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "download.pytorch.org",
                    ),
                ),
                port: None,
                path: "/whl/jinja2/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.2-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.2-py3-none-any.whl",
                yanked: None,
            },
        ],
    }
    "###);
}

#[test]
fn parse_missing_href() {
    let text = r"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a>Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    ";
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap_err();
    insta::assert_snapshot!(result, @"Missing href attribute on anchor link");
}

#[test]
fn parse_empty_href() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap_err();
    insta::assert_snapshot!(result, @"Missing href attribute on anchor link");
}

#[test]
fn parse_empty_fragment() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl#">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "download.pytorch.org",
                    ),
                ),
                port: None,
                path: "/whl/jinja2/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.2-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.2-py3-none-any.whl#",
                yanked: None,
            },
        ],
    }
    "###);
}

#[test]
fn parse_query_string() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl?project=legacy">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "download.pytorch.org",
                    ),
                ),
                port: None,
                path: "/whl/jinja2/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.2-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.2-py3-none-any.whl?project=legacy",
                yanked: None,
            },
        ],
    }
    "###);
}

#[test]
fn parse_missing_hash_value() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha256">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap_err();
    insta::assert_snapshot!(result, @"Unexpected fragment (expected `#sha256=...` or similar) on URL: sha256");
}

#[test]
fn parse_unknown_hash() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl#blake2=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
<!--TIMESTAMP 1703347410-->
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap_err();
    insta::assert_snapshot!(result, @"Unsupported hash algorithm (expected one of: `md5`, `sha256`, `sha384`, or `sha512`) on: `blake2=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61`");
}

#[test]
fn parse_flat_index_html() {
    let text = r#"
        <!DOCTYPE html>
        <html>
        <head><meta http-equiv="Content-Type" content="text/html; charset=utf-8"></head>
        <body>
            <a href="https://storage.googleapis.com/jax-releases/cuda100/jaxlib-0.1.52+cuda100-cp36-none-manylinux2010_x86_64.whl">cuda100/jaxlib-0.1.52+cuda100-cp36-none-manylinux2010_x86_64.whl</a><br>
            <a href="https://storage.googleapis.com/jax-releases/cuda100/jaxlib-0.1.52+cuda100-cp37-none-manylinux2010_x86_64.whl">cuda100/jaxlib-0.1.52+cuda100-cp37-none-manylinux2010_x86_64.whl</a><br>
        </body>
        </html>
    "#;
    let base =
        Url::parse("https://storage.googleapis.com/jax-releases/jax_cuda_releases.html").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "storage.googleapis.com",
                    ),
                ),
                port: None,
                path: "/jax-releases/jax_cuda_releases.html",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "jaxlib-0.1.52+cuda100-cp36-none-manylinux2010_x86_64.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "https://storage.googleapis.com/jax-releases/cuda100/jaxlib-0.1.52+cuda100-cp36-none-manylinux2010_x86_64.whl",
                yanked: None,
            },
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "jaxlib-0.1.52+cuda100-cp37-none-manylinux2010_x86_64.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "https://storage.googleapis.com/jax-releases/cuda100/jaxlib-0.1.52+cuda100-cp37-none-manylinux2010_x86_64.whl",
                yanked: None,
            },
        ],
    }
    "###);
}

/// Test for AWS Code Artifact
///
/// See: <https://github.com/astral-sh/uv/issues/1388#issuecomment-1947659088>
#[test]
fn parse_code_artifact_index_html() {
    let text = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Links for flask</title>
        </head>
        <body>
            <h1>Links for flask</h1>
            <a href="0.1/Flask-0.1.tar.gz#sha256=9da884457e910bf0847d396cb4b778ad9f3c3d17db1c5997cb861937bd284237" data-gpg-sig="false" >Flask-0.1.tar.gz</a>
            <br/>
            <a href="0.10.1/Flask-0.10.1.tar.gz#sha256=4c83829ff83d408b5e1d4995472265411d2c414112298f2eb4b359d9e4563373" data-gpg-sig="false" >Flask-0.10.1.tar.gz</a>
            <br/>
            <a href="3.0.1/flask-3.0.1.tar.gz#sha256=6489f51bb3666def6f314e15f19d50a1869a19ae0e8c9a3641ffe66c77d42403" data-requires-python="&gt;=3.8" data-gpg-sig="false" >flask-3.0.1.tar.gz</a>
            <br/>
        </body>
        </html>
    "#;
    let base = Url::parse("https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/flask/")
        .unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "account.d.codeartifact.us-west-2.amazonaws.com",
                    ),
                ),
                port: None,
                path: "/pypi/shared-packages-pypi/simple/flask/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Flask-0.1.tar.gz",
                hashes: Hashes {
                    md5: None,
                    sha256: Some(
                        "9da884457e910bf0847d396cb4b778ad9f3c3d17db1c5997cb861937bd284237",
                    ),
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "0.1/Flask-0.1.tar.gz#sha256=9da884457e910bf0847d396cb4b778ad9f3c3d17db1c5997cb861937bd284237",
                yanked: None,
            },
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Flask-0.10.1.tar.gz",
                hashes: Hashes {
                    md5: None,
                    sha256: Some(
                        "4c83829ff83d408b5e1d4995472265411d2c414112298f2eb4b359d9e4563373",
                    ),
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "0.10.1/Flask-0.10.1.tar.gz#sha256=4c83829ff83d408b5e1d4995472265411d2c414112298f2eb4b359d9e4563373",
                yanked: None,
            },
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "flask-3.0.1.tar.gz",
                hashes: Hashes {
                    md5: None,
                    sha256: Some(
                        "6489f51bb3666def6f314e15f19d50a1869a19ae0e8c9a3641ffe66c77d42403",
                    ),
                    sha384: None,
                    sha512: None,
                },
                requires_python: Some(
                    Ok(
                        VersionSpecifiers(
                            [
                                VersionSpecifier {
                                    operator: GreaterThanEqual,
                                    version: "3.8",
                                },
                            ],
                        ),
                    ),
                ),
                size: None,
                upload_time: None,
                url: "3.0.1/flask-3.0.1.tar.gz#sha256=6489f51bb3666def6f314e15f19d50a1869a19ae0e8c9a3641ffe66c77d42403",
                yanked: None,
            },
        ],
    }
    "###);
}

#[test]
fn parse_file_requires_python_trailing_comma() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" data-requires-python="&gt;=3.8,">Jinja2-3.1.2-py3-none-any.whl</a><br/>
</body>
</html>
    "#;
    let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "download.pytorch.org",
                    ),
                ),
                port: None,
                path: "/whl/jinja2/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: None,
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.2-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: Some(
                        "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    ),
                    sha384: None,
                    sha512: None,
                },
                requires_python: Some(
                    Ok(
                        VersionSpecifiers(
                            [
                                VersionSpecifier {
                                    operator: GreaterThanEqual,
                                    version: "3.8",
                                },
                            ],
                        ),
                    ),
                ),
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                yanked: None,
            },
        ],
    }
    "###);
}

/// Respect PEP 714 (see: <https://peps.python.org/pep-0714/>).
#[test]
fn parse_core_metadata() {
    let text = r#"
<!DOCTYPE html>
<html>
<body>
<h1>Links for jinja2</h1>
<a href="/whl/Jinja2-3.1.2-py3-none-any.whl" data-dist-info-metadata="true">Jinja2-3.1.2-py3-none-any.whl</a><br/>
<a href="/whl/Jinja2-3.1.3-py3-none-any.whl" data-core-metadata="true">Jinja2-3.1.3-py3-none-any.whl</a><br/>
<a href="/whl/Jinja2-3.1.4-py3-none-any.whl" data-dist-info-metadata="false">Jinja2-3.1.4-py3-none-any.whl</a><br/>
<a href="/whl/Jinja2-3.1.5-py3-none-any.whl" data-core-metadata="false">Jinja2-3.1.5-py3-none-any.whl</a><br/>
<a href="/whl/Jinja2-3.1.6-py3-none-any.whl" data-core-metadata="true" data-dist-info-metadata="false">Jinja2-3.1.6-py3-none-any.whl</a><br/>
</body>
</html>
    "#;
    let base = Url::parse("https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/flask/")
        .unwrap();
    let result = SimpleHtml::parse(text, &base).unwrap();
    insta::assert_debug_snapshot!(result, @r###"
    SimpleHtml {
        base: BaseUrl(
            Url {
                scheme: "https",
                cannot_be_a_base: false,
                username: "",
                password: None,
                host: Some(
                    Domain(
                        "account.d.codeartifact.us-west-2.amazonaws.com",
                    ),
                ),
                port: None,
                path: "/pypi/shared-packages-pypi/simple/flask/",
                query: None,
                fragment: None,
            },
        ),
        files: [
            File {
                core_metadata: Some(
                    Bool(
                        true,
                    ),
                ),
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.2-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.2-py3-none-any.whl",
                yanked: None,
            },
            File {
                core_metadata: Some(
                    Bool(
                        true,
                    ),
                ),
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.3-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.3-py3-none-any.whl",
                yanked: None,
            },
            File {
                core_metadata: Some(
                    Bool(
                        false,
                    ),
                ),
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.4-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.4-py3-none-any.whl",
                yanked: None,
            },
            File {
                core_metadata: Some(
                    Bool(
                        false,
                    ),
                ),
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.5-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.5-py3-none-any.whl",
                yanked: None,
            },
            File {
                core_metadata: Some(
                    Bool(
                        true,
                    ),
                ),
                dist_info_metadata: None,
                data_dist_info_metadata: None,
                filename: "Jinja2-3.1.6-py3-none-any.whl",
                hashes: Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: None,
                },
                requires_python: None,
                size: None,
                upload_time: None,
                url: "/whl/Jinja2-3.1.6-py3-none-any.whl",
                yanked: None,
            },
        ],
    }
    "###);
}
