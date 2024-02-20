use owo_colors::OwoColorize;
use uv_cache::Cache;
use uv_fs::Normalized;

/// Show the cache directory.
pub(crate) fn cache_dir(cache: &Cache) {
    anstream::println!("{}", cache.root().normalized_display().cyan());
}
