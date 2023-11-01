use std::time::Duration;

use curl::easy::{Easy, SslVersion};

use crate::util::CargoResult;

/// Creates a new HTTP handle with appropriate global configuration for cargo.
pub fn http_handle() -> CargoResult<Easy> {
    let (mut handle, timeout) = http_handle_and_timeout()?;
    timeout.configure(&mut handle)?;
    Ok(handle)
}

pub fn http_handle_and_timeout() -> CargoResult<(Easy, HttpTimeout)> {
    // The timeout option for libcurl by default times out the entire transfer,
    // but we probably don't want this. Instead we only set timeouts for the
    // connect phase as well as a "low speed" timeout so if we don't receive
    // many bytes in a large-ish period of time then we time out.
    let mut handle = Easy::new();
    let timeout = configure_http_handle(&mut handle)?;
    Ok((handle, timeout))
}

/// Configure a libcurl http handle with the defaults options for Cargo
pub fn configure_http_handle(handle: &mut Easy) -> CargoResult<HttpTimeout> {
    // Empty string accept encoding expands to the encodings supported by the current libcurl.
    handle.accept_encoding("")?;
    if cfg!(windows) {
        // This is a temporary workaround for some bugs with libcurl and
        // schannel and TLS 1.3.
        //
        // Our libcurl on Windows is usually built with schannel.
        // On Windows 11 (or Windows Server 2022), libcurl recently (late
        // 2022) gained support for TLS 1.3 with schannel, and it now defaults
        // to 1.3. Unfortunately there have been some bugs with this.
        // https://github.com/curl/curl/issues/9431 is the most recent. Once
        // that has been fixed, and some time has passed where we can be more
        // confident that the 1.3 support won't cause issues, this can be
        // removed.
        //
        // Windows 10 is unaffected. libcurl does not support TLS 1.3 on
        // Windows 10. (Windows 10 sorta had support, but it required enabling
        // an advanced option in the registry which was buggy, and libcurl
        // does runtime checks to prevent it.)
        handle.ssl_min_max_version(SslVersion::Default, SslVersion::Tlsv12)?;
    }

    Ok(HttpTimeout::default())
}

#[must_use]
pub struct HttpTimeout {
    pub dur: Duration,
    pub low_speed_limit: u32,
}

impl Default for HttpTimeout {
    fn default() -> Self {
        Self {
            dur: Duration::new(30, 0),
            low_speed_limit: 10,
        }
    }
}

impl HttpTimeout {
    pub fn configure(&self, handle: &mut Easy) -> CargoResult<()> {
        // The timeout option for libcurl by default times out the entire
        // transfer, but we probably don't want this. Instead we only set
        // timeouts for the connect phase as well as a "low speed" timeout so
        // if we don't receive many bytes in a large-ish period of time then we
        // time out.
        handle.connect_timeout(self.dur)?;
        handle.low_speed_time(self.dur)?;
        handle.low_speed_limit(self.low_speed_limit)?;
        Ok(())
    }
}
