use std::cell::RefCell;

use curl::easy::Easy;
use lazycell::LazyCell;

use crate::util::network::http::{configure_http_handle, http_handle};
use crate::util::CargoResult;

/// Configuration information for cargo. This is not specific to a build, it is information
/// relating to cargo itself.
#[derive(Debug, Default)]
pub struct Config {
    git_fetch_with_cli: bool,
    easy: LazyCell<RefCell<Easy>>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            git_fetch_with_cli: false,
            easy: LazyCell::new(),
        }
    }

    pub fn git_fetch_with_cli(&self) -> bool {
        self.git_fetch_with_cli
    }

    pub fn http(&self) -> CargoResult<&RefCell<Easy>> {
        let http = self
            .easy
            .try_borrow_with(|| http_handle().map(RefCell::new))?;
        {
            let mut http = http.borrow_mut();
            http.reset();
            let timeout = configure_http_handle(&mut http)?;
            timeout.configure(&mut http)?;
        }
        Ok(http)
    }
}
