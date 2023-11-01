/// Configuration information for cargo. This is not specific to a build, it is information
/// relating to cargo itself.
#[derive(Debug)]
pub struct Config {
    // /// The location of the user's Cargo home directory. OS-dependent.
    // home_path: Filesystem,
    // /// Information about how to write messages to the shell
    // shell: RefCell<Shell>,
    // /// A collection of configuration options
    // values: LazyCell<HashMap<String, ConfigValue>>,
    // /// A collection of configuration options from the credentials file
    // credential_values: LazyCell<HashMap<String, ConfigValue>>,
    // /// CLI config values, passed in via `configure`.
    // cli_config: Option<Vec<String>>,
    // /// The current working directory of cargo
    // cwd: PathBuf,
    // /// Directory where config file searching should stop (inclusive).
    // search_stop_path: Option<PathBuf>,
    // /// The location of the cargo executable (path to current process)
    // cargo_exe: LazyCell<PathBuf>,
    // /// The location of the rustdoc executable
    // rustdoc: LazyCell<PathBuf>,
    // /// Whether we are printing extra verbose messages
    // extra_verbose: bool,
    // /// `frozen` is the same as `locked`, but additionally will not access the
    // /// network to determine if the lock file is out-of-date.
    // frozen: bool,
    // /// `locked` is set if we should not update lock files. If the lock file
    // /// is missing, or needs to be updated, an error is produced.
    // locked: bool,
    // /// `offline` is set if we should never access the network, but otherwise
    // /// continue operating if possible.
    // offline: bool,
    // /// A global static IPC control mechanism (used for managing parallel builds)
    // jobserver: Option<jobserver::Client>,
    // /// Cli flags of the form "-Z something" merged with config file values
    // unstable_flags: CliUnstable,
    // /// Cli flags of the form "-Z something"
    // unstable_flags_cli: Option<Vec<String>>,
    // /// A handle on curl easy mode for http calls
    // easy: LazyCell<RefCell<Easy>>,
    // /// Cache of the `SourceId` for crates.io
    // crates_io_source_id: LazyCell<SourceId>,
    // /// If false, don't cache `rustc --version --verbose` invocations
    // cache_rustc_info: bool,
    // /// Creation time of this config, used to output the total build time
    // creation_time: Instant,
    // /// Target Directory via resolved Cli parameter
    // target_dir: Option<Filesystem>,
    // /// Environment variable snapshot.
    // env: Env,
    // /// Tracks which sources have been updated to avoid multiple updates.
    // updated_sources: LazyCell<RefCell<HashSet<SourceId>>>,
    // /// Cache of credentials from configuration or credential providers.
    // /// Maps from url to credential value.
    // credential_cache: LazyCell<RefCell<HashMap<CanonicalUrl, CredentialCacheValue>>>,
    // /// Cache of registry config from from the `[registries]` table.
    // registry_config: LazyCell<RefCell<HashMap<SourceId, Option<RegistryConfig>>>>,
    // /// Locks on the package and index caches.
    // package_cache_lock: CacheLocker,
    // /// Cached configuration parsed by Cargo
    // http_config: LazyCell<CargoHttpConfig>,
    // future_incompat_config: LazyCell<CargoFutureIncompatConfig>,
    // net_config: LazyCell<CargoNetConfig>,
    // build_config: LazyCell<CargoBuildConfig>,
    // target_cfgs: LazyCell<Vec<(String, TargetCfgConfig)>>,
    // doc_extern_map: LazyCell<RustdocExternMap>,
    // progress_config: ProgressConfig,
    // env_config: LazyCell<EnvConfig>,
    // /// This should be false if:
    // /// - this is an artifact of the rustc distribution process for "stable" or for "beta"
    // /// - this is an `#[test]` that does not opt in with `enable_nightly_features`
    // /// - this is an integration test that uses `ProcessBuilder`
    // ///      that does not opt in with `masquerade_as_nightly_cargo`
    // /// This should be true if:
    // /// - this is an artifact of the rustc distribution process for "nightly"
    // /// - this is being used in the rustc distribution process internally
    // /// - this is a cargo executable that was built from source
    // /// - this is an `#[test]` that called `enable_nightly_features`
    // /// - this is an integration test that uses `ProcessBuilder`
    // ///       that called `masquerade_as_nightly_cargo`
    // /// It's public to allow tests use nightly features.
    // /// NOTE: this should be set before `configure()`. If calling this from an integration test,
    // /// consider using `ConfigBuilder::enable_nightly_features` instead.
    // pub nightly_features_allowed: bool,
    // /// WorkspaceRootConfigs that have been found
    // pub ws_roots: RefCell<HashMap<PathBuf, WorkspaceRootConfig>>,
}

impl Config {
    /// Creates a new config instance.
    ///
    /// This is typically used for tests or other special cases. `default` is
    /// preferred otherwise.
    ///
    /// This does only minimal initialization. In particular, it does not load
    /// any config files from disk. Those will be loaded lazily as-needed.
    pub fn new(shell: Shell, cwd: PathBuf, homedir: PathBuf) -> Config {
        static mut GLOBAL_JOBSERVER: *mut jobserver::Client = 0 as *mut _;
        static INIT: Once = Once::new();

        // This should be called early on in the process, so in theory the
        // unsafety is ok here. (taken ownership of random fds)
        INIT.call_once(|| unsafe {
            if let Some(client) = jobserver::Client::from_env() {
                GLOBAL_JOBSERVER = Box::into_raw(Box::new(client));
            }
        });

        let env = Env::new();

        let cache_key = "CARGO_CACHE_RUSTC_INFO";
        let cache_rustc_info = match env.get_env_os(cache_key) {
            Some(cache) => cache != "0",
            _ => true,
        };

        Config {
            home_path: Filesystem::new(homedir),
            shell: RefCell::new(shell),
            cwd,
            search_stop_path: None,
            values: LazyCell::new(),
            credential_values: LazyCell::new(),
            cli_config: None,
            cargo_exe: LazyCell::new(),
            rustdoc: LazyCell::new(),
            extra_verbose: false,
            frozen: false,
            locked: false,
            offline: false,
            jobserver: unsafe {
                if GLOBAL_JOBSERVER.is_null() {
                    None
                } else {
                    Some((*GLOBAL_JOBSERVER).clone())
                }
            },
            unstable_flags: CliUnstable::default(),
            unstable_flags_cli: None,
            easy: LazyCell::new(),
            crates_io_source_id: LazyCell::new(),
            cache_rustc_info,
            creation_time: Instant::now(),
            target_dir: None,
            env,
            updated_sources: LazyCell::new(),
            credential_cache: LazyCell::new(),
            registry_config: LazyCell::new(),
            package_cache_lock: CacheLocker::new(),
            http_config: LazyCell::new(),
            future_incompat_config: LazyCell::new(),
            net_config: LazyCell::new(),
            build_config: LazyCell::new(),
            target_cfgs: LazyCell::new(),
            doc_extern_map: LazyCell::new(),
            progress_config: ProgressConfig::default(),
            env_config: LazyCell::new(),
            nightly_features_allowed: matches!(&*features::channel(), "nightly" | "dev"),
            ws_roots: RefCell::new(HashMap::new()),
        }
    }
}