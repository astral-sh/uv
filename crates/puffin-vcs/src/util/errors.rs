use std::fmt::{self, Write};

use curl::easy::Easy;

use super::truncate_with_ellipsis;

pub type CargoResult<T> = anyhow::Result<T>;

/// These are headers that are included in error messages to help with
/// diagnosing issues.
pub const DEBUG_HEADERS: &[&str] = &[
    // This is the unique ID that identifies the request in CloudFront which
    // can be used for looking at the AWS logs.
    "x-amz-cf-id",
    // This is the CloudFront POP (Point of Presence) that identifies the
    // region where the request was routed. This can help identify if an issue
    // is region-specific.
    "x-amz-cf-pop",
    // The unique token used for troubleshooting S3 requests via AWS logs or support.
    "x-amz-request-id",
    // Another token used in conjunction with x-amz-request-id.
    "x-amz-id-2",
    // Whether or not there was a cache hit or miss (both CloudFront and Fastly).
    "x-cache",
    // The cache server that processed the request (Fastly).
    "x-served-by",
];

#[derive(Debug)]
pub struct HttpNotSuccessful {
    pub code: u32,
    pub url: String,
    pub ip: Option<String>,
    pub body: Vec<u8>,
    pub headers: Vec<String>,
}

impl HttpNotSuccessful {
    pub fn new_from_handle(
        handle: &mut Easy,
        initial_url: &str,
        body: Vec<u8>,
        headers: Vec<String>,
    ) -> HttpNotSuccessful {
        let ip = handle.primary_ip().ok().flatten().map(|s| s.to_string());
        let url = handle
            .effective_url()
            .ok()
            .flatten()
            .unwrap_or(initial_url)
            .to_string();
        HttpNotSuccessful {
            code: handle.response_code().unwrap_or(0),
            url,
            ip,
            body,
            headers,
        }
    }

    /// Renders the error in a compact form.
    pub fn display_short(&self) -> String {
        self.render(false)
    }

    fn render(&self, show_headers: bool) -> String {
        let mut result = String::new();
        let body = std::str::from_utf8(&self.body)
            .map(|s| truncate_with_ellipsis(s, 512))
            .unwrap_or_else(|_| format!("[{} non-utf8 bytes]", self.body.len()));

        write!(
            result,
            "failed to get successful HTTP response from `{}`",
            self.url
        )
        .unwrap();
        if let Some(ip) = &self.ip {
            write!(result, " ({ip})").unwrap();
        }
        write!(result, ", got {}\n", self.code).unwrap();
        if show_headers {
            let headers: Vec<_> = self
                .headers
                .iter()
                .filter(|header| {
                    let Some((name, _)) = header.split_once(":") else {
                        return false;
                    };
                    DEBUG_HEADERS.contains(&name.to_ascii_lowercase().trim())
                })
                .collect();
            if !headers.is_empty() {
                writeln!(result, "debug headers:").unwrap();
                for header in headers {
                    writeln!(result, "{header}").unwrap();
                }
            }
        }
        write!(result, "body:\n{body}").unwrap();
        result
    }
}

impl fmt::Display for HttpNotSuccessful {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render(true))
    }
}

impl std::error::Error for HttpNotSuccessful {}
