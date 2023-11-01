//! This module contains all code sporting `gitoxide` for operations on `git` repositories and it mirrors
//! `utils` closely for now. One day it can be renamed into `utils` once `git2` isn't required anymore.

use crate::util::network::http::HttpTimeout;
use crate::util::{human_readable_bytes, network, MetricsCounter, Progress};
use crate::{CargoResult, Config};
use cargo_util::paths;
use gix::bstr::{BString, ByteSlice};
use std::cell::RefCell;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tracing::debug;

/// For the time being, `repo_path` makes it easy to instantiate a gitoxide repo just for fetching.
/// In future this may change to be the gitoxide repository itself.
pub fn with_retry_and_progress(
    repo_path: &std::path::Path,
    config: &Config,
    cb: &(dyn Fn(
        &std::path::Path,
        &AtomicBool,
        &mut gix::progress::tree::Item,
        &mut dyn FnMut(&gix::bstr::BStr),
    ) -> Result<(), crate::sources::git::fetch::Error>
          + Send
          + Sync),
) -> CargoResult<()> {
    std::thread::scope(|s| {
        let mut progress_bar = Progress::new("Fetch", config);
        let is_shallow = config
            .cli_unstable()
            .gitoxide
            .map_or(false, |gix| gix.shallow_deps || gix.shallow_index);
        network::retry::with_retry(config, || {
            let progress_root: Arc<gix::progress::tree::Root> =
                gix::progress::tree::root::Options {
                    initial_capacity: 10,
                    message_buffer_capacity: 10,
                }
                .into();
            let root = Arc::downgrade(&progress_root);
            let thread = s.spawn(move || {
                let mut progress = progress_root.add_child("operation");
                let mut urls = RefCell::new(Default::default());
                let res = cb(
                    &repo_path,
                    &AtomicBool::default(),
                    &mut progress,
                    &mut |url| {
                        *urls.borrow_mut() = Some(url.to_owned());
                    },
                );
                amend_authentication_hints(res, urls.get_mut().take())
            });
            translate_progress_to_bar(&mut progress_bar, root, is_shallow)?;
            thread.join().expect("no panic in scoped thread")
        })
    })
}

fn translate_progress_to_bar(
    progress_bar: &mut Progress<'_>,
    root: Weak<gix::progress::tree::Root>,
    is_shallow: bool,
) -> CargoResult<()> {
    let remote_progress: gix::progress::Id = gix::remote::fetch::ProgressId::RemoteProgress.into();
    let read_pack_bytes: gix::progress::Id =
        gix::odb::pack::bundle::write::ProgressId::ReadPackBytes.into();
    let delta_index_objects: gix::progress::Id =
        gix::odb::pack::index::write::ProgressId::IndexObjects.into();
    let resolve_objects: gix::progress::Id =
        gix::odb::pack::index::write::ProgressId::ResolveObjects.into();

    // We choose `N=10` here to make a `300ms * 10slots ~= 3000ms`
    // sliding window for tracking the data transfer rate (in bytes/s).
    let mut last_percentage_update = Instant::now();
    let mut last_fast_update = Instant::now();
    let mut counter = MetricsCounter::<10>::new(0, last_percentage_update);

    let mut tasks = Vec::with_capacity(10);
    let slow_check_interval = std::time::Duration::from_millis(300);
    let fast_check_interval = Duration::from_millis(50);
    let sleep_interval = Duration::from_millis(10);
    debug_assert_eq!(
        slow_check_interval.as_millis() % fast_check_interval.as_millis(),
        0,
        "progress should be smoother by keeping these as multiples of each other"
    );
    debug_assert_eq!(
        fast_check_interval.as_millis() % sleep_interval.as_millis(),
        0,
        "progress should be smoother by keeping these as multiples of each other"
    );

    let num_phases = if is_shallow { 3 } else { 2 }; // indexing + delta-resolution, both with same amount of objects to handle
    while let Some(root) = root.upgrade() {
        std::thread::sleep(sleep_interval);
        let needs_update = last_fast_update.elapsed() >= fast_check_interval;
        if !needs_update {
            continue;
        }
        let now = Instant::now();
        last_fast_update = now;

        root.sorted_snapshot(&mut tasks);

        fn progress_by_id(
            id: gix::progress::Id,
            task: &gix::progress::Task,
        ) -> Option<(&str, &gix::progress::Value)> {
            (task.id == id)
                .then(|| task.progress.as_ref())
                .flatten()
                .map(|value| (task.name.as_str(), value))
        }
        fn find_in<K>(
            tasks: &[(K, gix::progress::Task)],
            cb: impl Fn(&gix::progress::Task) -> Option<(&str, &gix::progress::Value)>,
        ) -> Option<(&str, &gix::progress::Value)> {
            tasks.iter().find_map(|(_, t)| cb(t))
        }

        if let Some((_, objs)) = find_in(&tasks, |t| progress_by_id(resolve_objects, t)) {
            // Phase 3: Resolving deltas.
            let objects = objs.step.load(Ordering::Relaxed);
            let total_objects = objs.done_at.expect("known amount of objects");
            let msg = format!(", ({objects}/{total_objects}) resolving deltas");

            progress_bar.tick(
                (total_objects * (num_phases - 1)) + objects,
                total_objects * num_phases,
                &msg,
            )?;
        } else if let Some((objs, read_pack)) =
            find_in(&tasks, |t| progress_by_id(read_pack_bytes, t)).and_then(|read| {
                find_in(&tasks, |t| progress_by_id(delta_index_objects, t))
                    .map(|delta| (delta.1, read.1))
            })
        {
            // Phase 2: Receiving objects.
            let objects = objs.step.load(Ordering::Relaxed);
            let total_objects = objs.done_at.expect("known amount of objects");
            let received_bytes = read_pack.step.load(Ordering::Relaxed);

            let needs_percentage_update = last_percentage_update.elapsed() >= slow_check_interval;
            if needs_percentage_update {
                counter.add(received_bytes, now);
                last_percentage_update = now;
            }
            let (rate, unit) = human_readable_bytes(counter.rate() as u64);
            let msg = format!(", {rate:.2}{unit}/s");

            progress_bar.tick(
                (total_objects * (num_phases - 2)) + objects,
                total_objects * num_phases,
                &msg,
            )?;
        } else if let Some((action, remote)) =
            find_in(&tasks, |t| progress_by_id(remote_progress, t))
        {
            if !is_shallow {
                continue;
            }
            // phase 1: work on the remote side

            // Resolving deltas.
            let objects = remote.step.load(Ordering::Relaxed);
            if let Some(total_objects) = remote.done_at {
                let msg = format!(", ({objects}/{total_objects}) {action}");
                progress_bar.tick(objects, total_objects * num_phases, &msg)?;
            }
        }
    }
    Ok(())
}

fn amend_authentication_hints(
    res: Result<(), crate::sources::git::fetch::Error>,
    last_url_for_authentication: Option<gix::bstr::BString>,
) -> CargoResult<()> {
    let Err(err) = res else { return Ok(()) };
    let e = match &err {
        crate::sources::git::fetch::Error::PrepareFetch(
            gix::remote::fetch::prepare::Error::RefMap(gix::remote::ref_map::Error::Handshake(err)),
        ) => Some(err),
        _ => None,
    };
    if let Some(e) = e {
        use anyhow::Context;
        let auth_message = match e {
            gix::protocol::handshake::Error::Credentials(_) => {
                "\n* attempted to find username/password via \
                     git's `credential.helper` support, but failed"
                    .into()
            }
            gix::protocol::handshake::Error::InvalidCredentials { .. } => {
                "\n* attempted to find username/password via \
                     `credential.helper`, but maybe the found \
                     credentials were incorrect"
                    .into()
            }
            gix::protocol::handshake::Error::Transport(_) => {
                let msg = concat!(
                    "network failure seems to have happened\n",
                    "if a proxy or similar is necessary `net.git-fetch-with-cli` may help here\n",
                    "https://doc.rust-lang.org/cargo/reference/config.html#netgit-fetch-with-cli"
                );
                return Err(anyhow::Error::from(err)).context(msg);
            }
            _ => None,
        };
        if let Some(auth_message) = auth_message {
            let mut msg = "failed to authenticate when downloading \
                       repository"
                .to_string();
            if let Some(url) = last_url_for_authentication {
                msg.push_str(": ");
                msg.push_str(url.to_str_lossy().as_ref());
            }
            msg.push('\n');
            msg.push_str(auth_message);
            msg.push_str("\n\n");
            msg.push_str("if the git CLI succeeds then `net.git-fetch-with-cli` may help here\n");
            msg.push_str(
                "https://doc.rust-lang.org/cargo/reference/config.html#netgit-fetch-with-cli",
            );
            return Err(anyhow::Error::from(err)).context(msg);
        }
    }
    Err(err.into())
}

/// The reason we are opening a git repository.
///
/// This can affect the way we open it and the cost associated with it.
pub enum OpenMode {
    /// We need `git_binary` configuration as well for being able to see credential helpers
    /// that are configured with the `git` installation itself.
    /// However, this is slow on windows (~150ms) and most people won't need it as they use the
    /// standard index which won't ever need authentication, so we only enable this when needed.
    ForFetch,
}

impl OpenMode {
    /// Sometimes we don't need to pay for figuring out the system's git installation, and this tells
    /// us if that is the case.
    pub fn needs_git_binary_config(&self) -> bool {
        match self {
            OpenMode::ForFetch => true,
        }
    }
}

/// Produce a repository with everything pre-configured according to `config`. Most notably this includes
/// transport configuration. Knowing its `purpose` helps to optimize the way we open the repository.
/// Use `config_overrides` to configure the new repository.
pub fn open_repo(
    repo_path: &std::path::Path,
    config_overrides: Vec<BString>,
    purpose: OpenMode,
) -> Result<gix::Repository, gix::open::Error> {
    gix::open_opts(repo_path, {
        let mut opts = gix::open::Options::default();
        opts.permissions.config = gix::open::permissions::Config::all();
        opts.permissions.config.git_binary = purpose.needs_git_binary_config();
        opts.with(gix::sec::Trust::Full)
            .config_overrides(config_overrides)
    })
}

/// Convert `git` related cargo configuration into the respective `git` configuration which can be
/// used when opening new repositories.
pub fn cargo_config_to_gitoxide_overrides(config: &Config) -> CargoResult<Vec<BString>> {
    use gix::config::tree::{gitoxide, Core, Http, Key};
    let timeout = HttpTimeout::new(config)?;
    let http = config.http_config()?;

    let mut values = vec![
        gitoxide::Http::CONNECT_TIMEOUT.validated_assignment_fmt(&timeout.dur.as_millis())?,
        Http::LOW_SPEED_LIMIT.validated_assignment_fmt(&timeout.low_speed_limit)?,
        Http::LOW_SPEED_TIME.validated_assignment_fmt(&timeout.dur.as_secs())?,
        // Assure we are not depending on committer information when updating refs after cloning.
        Core::LOG_ALL_REF_UPDATES.validated_assignment_fmt(&false)?,
    ];
    if let Some(proxy) = &http.proxy {
        values.push(Http::PROXY.validated_assignment_fmt(proxy)?);
    }
    if let Some(check_revoke) = http.check_revoke {
        values.push(Http::SCHANNEL_CHECK_REVOKE.validated_assignment_fmt(&check_revoke)?);
    }
    if let Some(cainfo) = &http.cainfo {
        values.push(
            Http::SSL_CA_INFO.validated_assignment_fmt(&cainfo.resolve_path(config).display())?,
        );
    }

    values.push(if let Some(user_agent) = &http.user_agent {
        Http::USER_AGENT.validated_assignment_fmt(user_agent)
    } else {
        Http::USER_AGENT.validated_assignment_fmt(&format!("cargo {}", crate::version()))
    }?);
    if let Some(ssl_version) = &http.ssl_version {
        use crate::util::config::SslVersionConfig;
        match ssl_version {
            SslVersionConfig::Single(version) => {
                values.push(Http::SSL_VERSION.validated_assignment_fmt(&version)?);
            }
            SslVersionConfig::Range(range) => {
                values.push(
                    gitoxide::Http::SSL_VERSION_MIN
                        .validated_assignment_fmt(&range.min.as_deref().unwrap_or("default"))?,
                );
                values.push(
                    gitoxide::Http::SSL_VERSION_MAX
                        .validated_assignment_fmt(&range.max.as_deref().unwrap_or("default"))?,
                );
            }
        }
    } else if cfg!(windows) {
        // This text is copied from https://github.com/rust-lang/cargo/blob/39c13e67a5962466cc7253d41bc1099bbcb224c3/src/cargo/ops/registry.rs#L658-L674 .
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
        values.push(gitoxide::Http::SSL_VERSION_MIN.validated_assignment_fmt(&"default")?);
        values.push(gitoxide::Http::SSL_VERSION_MAX.validated_assignment_fmt(&"tlsv1.2")?);
    }
    if let Some(debug) = http.debug {
        values.push(gitoxide::Http::VERBOSE.validated_assignment_fmt(&debug)?);
    }
    if let Some(multiplexing) = http.multiplexing {
        let http_version = multiplexing.then(|| "HTTP/2").unwrap_or("HTTP/1.1");
        // Note that failing to set the HTTP version in `gix-transport` isn't fatal,
        // which is why we don't have to try to figure out if HTTP V2 is supported in the
        // currently linked version (see `try_old_curl!()`)
        values.push(Http::VERSION.validated_assignment_fmt(&http_version)?);
    }

    Ok(values)
}

/// Reinitializes a given Git repository. This is useful when a Git repository
/// seems corrupted and we want to start over.
pub fn reinitialize(git_dir: &Path) -> CargoResult<()> {
    fn init(path: &Path, bare: bool) -> CargoResult<()> {
        let mut opts = git2::RepositoryInitOptions::new();
        // Skip anything related to templates, they just call all sorts of issues as
        // we really don't want to use them yet they insist on being used. See #6240
        // for an example issue that comes up.
        opts.external_template(false);
        opts.bare(bare);
        git2::Repository::init_opts(&path, &opts)?;
        Ok(())
    }
    // Here we want to drop the current repository object pointed to by `repo`,
    // so we initialize temporary repository in a sub-folder, blow away the
    // existing git folder, and then recreate the git repo. Finally we blow away
    // the `tmp` folder we allocated.
    debug!("reinitializing git repo at {:?}", git_dir);
    let tmp = git_dir.join("tmp");
    let bare = !git_dir.ends_with(".git");
    init(&tmp, false)?;
    for entry in git_dir.read_dir()? {
        let entry = entry?;
        if entry.file_name().to_str() == Some("tmp") {
            continue;
        }
        let path = entry.path();
        drop(paths::remove_file(&path).or_else(|_| paths::remove_dir_all(&path)));
    }
    init(git_dir, bare)?;
    paths::remove_dir_all(&tmp)?;
    Ok(())
}
