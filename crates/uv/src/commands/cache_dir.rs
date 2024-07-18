use anstream::println;
use owo_colors::OwoColorize;

use uv_cache::Cache;
use uv_fs::Simplified;

/// Show the cache directory.
pub(crate) fn cache_dir(cache: &Cache) {
    println!("{}", cache.root().simplified_display().cyan());
}
