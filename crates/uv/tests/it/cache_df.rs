use crate::common::{TestContext, uv_snapshot};
use assert_cmd::assert::OutputAssertExt;

/// `cache df` should succeed and print an empty table for a fresh cache.
#[test]
fn cache_df_empty() {
    let context = TestContext::new("3.12").with_collapsed_whitespace();

    // Ensure the cache starts empty (no rows should have non-zero values)
    context.clean().assert().success();

    uv_snapshot!(context.filters(), context.cache_df(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    CACHE UTILIZATION
    ================================================================================
    Cache Type Count Size Description
    --------------------------------------------------------------------------------
    Wheels 0 [SIZE] Downloaded and cached wheels from registries and direct URLs
    Source Distributions 0 [SIZE] Source distributions and built wheels
    Simple Metadata 0 [SIZE] Package metadata from simple repositories
    Git Repositories 0 [SIZE] Cloned git repositories
    Interpreter Info 0 [SIZE] Cached Python interpreter information
    Flat Index 0 [SIZE] Flat index responses
    Archive 0 [SIZE] Shared archive storage for directories
    Build Environments 0 [SIZE] Ephemeral environments for builds
    Environments 0 [SIZE] Reusable tool environments
    Python 0 [SIZE] Cached Python downloads
    Binaries 0 [SIZE] Downloaded tool binaries
    ================================================================================
    TOTAL 0 [SIZE]
    
    Cache directory: [CACHE_DIR]/
    
    ----- stderr -----
    ");
}
