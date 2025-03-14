use std::env;
use std::fmt::Write;
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use url::Url;

use crate::commands::human_readable_bytes;
use crate::printer::Printer;
use uv_cache::Removal;
use uv_distribution_types::{
    BuildableSource, CachedDist, DistributionMetadata, Name, SourceDist, VersionOrUrlRef,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_python::PythonInstallationKey;
use uv_static::EnvVars;

/// Since downloads, fetches and builds run in parallel, their message output order is
/// non-deterministic, so can't capture them in test output.
static HAS_UV_TEST_NO_CLI_PROGRESS: LazyLock<bool> =
    LazyLock::new(|| env::var(EnvVars::UV_TEST_NO_CLI_PROGRESS).is_ok());

#[derive(Debug)]
struct ProgressReporter {
    printer: Printer,
    root: ProgressBar,
    mode: ProgressMode,
}

#[derive(Debug)]
enum ProgressMode {
    /// Reports top-level progress.
    Single,
    /// Reports progress of all concurrent download, build, and checkout processes.
    Multi {
        multi_progress: MultiProgress,
        state: Arc<Mutex<BarState>>,
    },
}

#[derive(Default, Debug)]
struct BarState {
    /// The number of bars that precede any download bars (i.e. build/checkout status).
    headers: usize,
    /// A list of download bar sizes, in descending order.
    sizes: Vec<u64>,
    /// A map of progress bars, by ID.
    bars: FxHashMap<usize, ProgressBar>,
    /// The download size, if known, by ID.
    size: FxHashMap<usize, Option<u64>>,
    /// A monotonic counter for bar IDs.
    id: usize,
}

impl BarState {
    /// Returns a unique ID for a new progress bar.
    fn id(&mut self) -> usize {
        self.id += 1;
        self.id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Upload,
    Download,
}

impl ProgressReporter {
    fn new(root: ProgressBar, multi_progress: MultiProgress, printer: Printer) -> ProgressReporter {
        let mode = if env::var(EnvVars::JPY_SESSION_NAME).is_ok() {
            // Disable concurrent progress bars when running inside a Jupyter notebook
            // because the Jupyter terminal does not support clearing previous lines.
            // See: https://github.com/astral-sh/uv/issues/3887.
            ProgressMode::Single
        } else {
            ProgressMode::Multi {
                state: Arc::default(),
                multi_progress,
            }
        };

        ProgressReporter {
            printer,
            root,
            mode,
        }
    }

    fn on_build_start(&self, source: &BuildableSource) -> usize {
        let ProgressMode::Multi {
            multi_progress,
            state,
        } = &self.mode
        else {
            return 0;
        };

        let mut state = state.lock().unwrap();
        let id = state.id();

        let progress = multi_progress.insert_before(
            &self.root,
            ProgressBar::with_draw_target(None, self.printer.target()),
        );

        progress.set_style(ProgressStyle::with_template("{wide_msg}").unwrap());
        let message = format!(
            "   {} {}",
            "Building".bold().cyan(),
            source.to_color_string()
        );
        if multi_progress.is_hidden() && !*HAS_UV_TEST_NO_CLI_PROGRESS {
            let _ = writeln!(self.printer.stderr(), "{message}");
        }
        progress.set_message(message);

        state.headers += 1;
        state.bars.insert(id, progress);
        id
    }

    fn on_build_complete(&self, source: &BuildableSource, id: usize) {
        let ProgressMode::Multi {
            state,
            multi_progress,
        } = &self.mode
        else {
            return;
        };

        let progress = {
            let mut state = state.lock().unwrap();
            state.headers -= 1;
            state.bars.remove(&id).unwrap()
        };

        let message = format!(
            "      {} {}",
            "Built".bold().green(),
            source.to_color_string()
        );
        if multi_progress.is_hidden() && !*HAS_UV_TEST_NO_CLI_PROGRESS {
            let _ = writeln!(self.printer.stderr(), "{message}");
        }
        progress.finish_with_message(message);
    }

    fn on_request_start(&self, direction: Direction, name: String, size: Option<u64>) -> usize {
        let ProgressMode::Multi {
            multi_progress,
            state,
        } = &self.mode
        else {
            return 0;
        };

        let mut state = state.lock().unwrap();

        // Preserve ascending order.
        let position = size.map_or(0, |size| state.sizes.partition_point(|&len| len < size));
        state.sizes.insert(position, size.unwrap_or(0));

        let progress = multi_progress.insert(
            // Make sure not to reorder the initial "Preparing..." bar, or any previous bars.
            position + 1 + state.headers,
            ProgressBar::with_draw_target(size, self.printer.target()),
        );

        if let Some(size) = size {
            // We're using binary bytes to match `human_readable_bytes`.
            progress.set_style(
                ProgressStyle::with_template(
                    "{msg:10.dim} {bar:30.green/dim} {binary_bytes:>7}/{binary_total_bytes:7}",
                )
                .unwrap()
                .progress_chars("--"),
            );
            // If the file is larger than 1MB, show a message to indicate that this may take
            // a while keeping the log concise.
            if multi_progress.is_hidden() && !*HAS_UV_TEST_NO_CLI_PROGRESS && size > 1024 * 1024 {
                let (bytes, unit) = human_readable_bytes(size);
                let _ = writeln!(
                    self.printer.stderr(),
                    "{} {} {}",
                    match direction {
                        Direction::Download => "Downloading",
                        Direction::Upload => "Uploading",
                    }
                    .bold()
                    .cyan(),
                    name,
                    format!("({bytes:.1}{unit})").dimmed()
                );
            }
            progress.set_message(name);
        } else {
            progress.set_style(ProgressStyle::with_template("{wide_msg:.dim} ....").unwrap());
            if multi_progress.is_hidden() && !*HAS_UV_TEST_NO_CLI_PROGRESS {
                let _ = writeln!(
                    self.printer.stderr(),
                    "{} {}",
                    match direction {
                        Direction::Download => "Downloading",
                        Direction::Upload => "Uploading",
                    }
                    .bold()
                    .cyan(),
                    name
                );
            }
            progress.set_message(name);
            progress.finish();
        }

        let id = state.id();
        state.bars.insert(id, progress);
        state.size.insert(id, size);
        id
    }

    fn on_request_progress(&self, id: usize, bytes: u64) {
        let ProgressMode::Multi { state, .. } = &self.mode else {
            return;
        };

        state.lock().unwrap().bars[&id].inc(bytes);
    }

    fn on_request_complete(&self, direction: Direction, id: usize) {
        let ProgressMode::Multi {
            state,
            multi_progress,
        } = &self.mode
        else {
            return;
        };

        let mut state = state.lock().unwrap();
        let progress = state.bars.remove(&id).unwrap();
        let size = state.size[&id];
        if multi_progress.is_hidden()
            && !*HAS_UV_TEST_NO_CLI_PROGRESS
            && size.is_none_or(|size| size > 1024 * 1024)
        {
            let _ = writeln!(
                self.printer.stderr(),
                " {} {}",
                match direction {
                    Direction::Download => "Downloaded",
                    Direction::Upload => "Uploaded",
                }
                .bold()
                .green(),
                progress.message()
            );
        }

        progress.finish_and_clear();
    }

    fn on_download_progress(&self, id: usize, bytes: u64) {
        self.on_request_progress(id, bytes);
    }

    fn on_download_complete(&self, id: usize) {
        self.on_request_complete(Direction::Download, id);
    }

    fn on_download_start(&self, name: String, size: Option<u64>) -> usize {
        self.on_request_start(Direction::Download, name, size)
    }

    fn on_upload_progress(&self, id: usize, bytes: u64) {
        self.on_request_progress(id, bytes);
    }

    fn on_upload_complete(&self, id: usize) {
        self.on_request_complete(Direction::Upload, id);
    }

    fn on_upload_start(&self, name: String, size: Option<u64>) -> usize {
        self.on_request_start(Direction::Upload, name, size)
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        let ProgressMode::Multi {
            multi_progress,
            state,
        } = &self.mode
        else {
            return 0;
        };

        let mut state = state.lock().unwrap();
        let id = state.id();

        let progress = multi_progress.insert_before(
            &self.root,
            ProgressBar::with_draw_target(None, self.printer.target()),
        );

        progress.set_style(ProgressStyle::with_template("{wide_msg}").unwrap());
        let message = format!("   {} {} ({})", "Updating".bold().cyan(), url, rev.dimmed());
        if multi_progress.is_hidden() && !*HAS_UV_TEST_NO_CLI_PROGRESS {
            let _ = writeln!(self.printer.stderr(), "{message}");
        }
        progress.set_message(message);
        progress.finish();

        state.headers += 1;
        state.bars.insert(id, progress);
        id
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, id: usize) {
        let ProgressMode::Multi {
            state,
            multi_progress,
        } = &self.mode
        else {
            return;
        };

        let progress = {
            let mut state = state.lock().unwrap();
            state.headers -= 1;
            state.bars.remove(&id).unwrap()
        };

        let message = format!(
            "    {} {} ({})",
            "Updated".bold().green(),
            url,
            rev.dimmed()
        );
        if multi_progress.is_hidden() && !*HAS_UV_TEST_NO_CLI_PROGRESS {
            let _ = writeln!(self.printer.stderr(), "{message}");
        }
        progress.finish_with_message(message);
    }
}

#[derive(Debug)]
pub(crate) struct PrepareReporter {
    reporter: ProgressReporter,
}

impl From<Printer> for PrepareReporter {
    fn from(printer: Printer) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());
        let root = multi_progress.add(ProgressBar::with_draw_target(None, printer.target()));
        root.enable_steady_tick(Duration::from_millis(200));
        root.set_style(
            ProgressStyle::with_template("{spinner:.white} {msg:.dim} ({pos}/{len})")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        root.set_message("Preparing packages...");

        let reporter = ProgressReporter::new(root, multi_progress, printer);
        Self { reporter }
    }
}

impl PrepareReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.reporter.root.set_length(length);
        self
    }
}

impl uv_installer::PrepareReporter for PrepareReporter {
    fn on_progress(&self, _dist: &CachedDist) {
        self.reporter.root.inc(1);
    }

    fn on_complete(&self) {
        // Need an extra call to `set_message` here to fully clear avoid leaving ghost output
        // in Jupyter notebooks.
        self.reporter.root.set_message("");
        self.reporter.root.finish_and_clear();
    }

    fn on_build_start(&self, source: &BuildableSource) -> usize {
        self.reporter.on_build_start(source)
    }

    fn on_build_complete(&self, source: &BuildableSource, id: usize) {
        self.reporter.on_build_complete(source, id);
    }

    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize {
        self.reporter.on_download_start(name.to_string(), size)
    }

    fn on_download_progress(&self, id: usize, bytes: u64) {
        self.reporter.on_download_progress(id, bytes);
    }

    fn on_download_complete(&self, _name: &PackageName, id: usize) {
        self.reporter.on_download_complete(id);
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, id: usize) {
        self.reporter.on_checkout_complete(url, rev, id);
    }
}

#[derive(Debug)]
pub(crate) struct ResolverReporter {
    reporter: ProgressReporter,
}

impl ResolverReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.reporter.root.set_length(length);
        self
    }
}

impl From<Printer> for ResolverReporter {
    fn from(printer: Printer) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());
        let root = multi_progress.add(ProgressBar::with_draw_target(None, printer.target()));
        root.enable_steady_tick(Duration::from_millis(200));
        root.set_style(
            ProgressStyle::with_template("{spinner:.white} {wide_msg:.dim}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        root.set_message("Resolving dependencies...");

        let reporter = ProgressReporter::new(root, multi_progress, printer);
        Self { reporter }
    }
}

impl uv_resolver::ResolverReporter for ResolverReporter {
    fn on_progress(&self, name: &PackageName, version_or_url: &VersionOrUrlRef) {
        match version_or_url {
            VersionOrUrlRef::Version(version) => {
                self.reporter.root.set_message(format!("{name}=={version}"));
            }
            VersionOrUrlRef::Url(url) => {
                self.reporter.root.set_message(format!("{name} @ {url}"));
            }
        }
    }

    fn on_complete(&self) {
        self.reporter.root.set_message("");
        self.reporter.root.finish_and_clear();
    }

    fn on_build_start(&self, source: &BuildableSource) -> usize {
        self.reporter.on_build_start(source)
    }

    fn on_build_complete(&self, source: &BuildableSource, id: usize) {
        self.reporter.on_build_complete(source, id);
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, id: usize) {
        self.reporter.on_checkout_complete(url, rev, id);
    }

    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize {
        self.reporter.on_download_start(name.to_string(), size)
    }

    fn on_download_progress(&self, id: usize, bytes: u64) {
        self.reporter.on_download_progress(id, bytes);
    }

    fn on_download_complete(&self, _name: &PackageName, id: usize) {
        self.reporter.on_download_complete(id);
    }
}

impl uv_distribution::Reporter for ResolverReporter {
    fn on_build_start(&self, source: &BuildableSource) -> usize {
        self.reporter.on_build_start(source)
    }

    fn on_build_complete(&self, source: &BuildableSource, id: usize) {
        self.reporter.on_build_complete(source, id);
    }

    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize {
        self.reporter.on_download_start(name.to_string(), size)
    }

    fn on_download_progress(&self, id: usize, bytes: u64) {
        self.reporter.on_download_progress(id, bytes);
    }

    fn on_download_complete(&self, _name: &PackageName, id: usize) {
        self.reporter.on_download_complete(id);
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, id: usize) {
        self.reporter.on_checkout_complete(url, rev, id);
    }
}

#[derive(Debug)]
pub(crate) struct InstallReporter {
    progress: ProgressBar,
}

impl From<Printer> for InstallReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.set_style(
            ProgressStyle::with_template("{bar:20} [{pos}/{len}] {wide_msg:.dim}").unwrap(),
        );
        progress.set_message("Installing wheels...");
        Self { progress }
    }
}

impl InstallReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.progress.set_length(length);
        self
    }
}

impl uv_installer::InstallReporter for InstallReporter {
    fn on_install_progress(&self, wheel: &CachedDist) {
        self.progress.set_message(format!("{wheel}"));
        self.progress.inc(1);
    }

    fn on_install_complete(&self) {
        self.progress.set_message("");
        self.progress.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct PythonDownloadReporter {
    reporter: ProgressReporter,
}

impl PythonDownloadReporter {
    /// Initialize a [`PythonDownloadReporter`] for a single Python download.
    pub(crate) fn single(printer: Printer) -> Self {
        Self::new(printer, 1)
    }

    /// Initialize a [`PythonDownloadReporter`] for multiple Python downloads.
    pub(crate) fn new(printer: Printer, length: u64) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());
        let root = multi_progress.add(ProgressBar::with_draw_target(
            Some(length),
            printer.target(),
        ));
        let reporter = ProgressReporter::new(root, multi_progress, printer);
        Self { reporter }
    }
}

impl uv_python::downloads::Reporter for PythonDownloadReporter {
    fn on_progress(&self, _name: &PythonInstallationKey, id: usize) {
        self.reporter.on_download_complete(id);
    }

    fn on_download_start(&self, name: &PythonInstallationKey, size: Option<u64>) -> usize {
        self.reporter.on_download_start(name.to_string(), size)
    }

    fn on_download_progress(&self, id: usize, inc: u64) {
        self.reporter.on_download_progress(id, inc);
    }

    fn on_download_complete(&self) {
        self.reporter.root.set_message("");
        self.reporter.root.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct PublishReporter {
    reporter: ProgressReporter,
}

impl PublishReporter {
    /// Initialize a [`PublishReporter`] for a single upload.
    pub(crate) fn single(printer: Printer) -> Self {
        Self::new(printer, 1)
    }

    /// Initialize a [`PublishReporter`] for multiple uploads.
    pub(crate) fn new(printer: Printer, length: u64) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());
        let root = multi_progress.add(ProgressBar::with_draw_target(
            Some(length),
            printer.target(),
        ));
        let reporter = ProgressReporter::new(root, multi_progress, printer);
        Self { reporter }
    }
}

impl uv_publish::Reporter for PublishReporter {
    fn on_progress(&self, _name: &str, id: usize) {
        self.reporter.on_download_complete(id);
    }

    fn on_upload_start(&self, name: &str, size: Option<u64>) -> usize {
        self.reporter.on_upload_start(name.to_string(), size)
    }

    fn on_upload_progress(&self, id: usize, inc: u64) {
        self.reporter.on_upload_progress(id, inc);
    }

    fn on_upload_complete(&self, id: usize) {
        self.reporter.on_upload_complete(id);
    }
}

#[derive(Debug)]
pub(crate) struct LatestVersionReporter {
    progress: ProgressBar,
}

impl From<Printer> for LatestVersionReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.set_style(
            ProgressStyle::with_template("{bar:20} [{pos}/{len}] {wide_msg:.dim}").unwrap(),
        );
        progress.set_message("Fetching latest versions...");
        Self { progress }
    }
}

impl LatestVersionReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.progress.set_length(length);
        self
    }

    pub(crate) fn on_fetch_progress(&self) {
        self.progress.inc(1);
    }

    pub(crate) fn on_fetch_version(&self, name: &PackageName, version: &Version) {
        self.progress.set_message(format!("{name} v{version}"));
        self.progress.inc(1);
    }

    pub(crate) fn on_fetch_complete(&self) {
        self.progress.set_message("");
        self.progress.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct CleaningDirectoryReporter {
    bar: ProgressBar,
}

impl CleaningDirectoryReporter {
    /// Initialize a [`CleaningDirectoryReporter`] for cleaning the cache directory.
    pub(crate) fn new(printer: Printer, max: usize) -> Self {
        let bar = ProgressBar::with_draw_target(Some(max as u64), printer.target());
        bar.set_style(
            ProgressStyle::with_template("{prefix} [{bar:20}] {percent}%")
                .unwrap()
                .progress_chars("=> "),
        );
        bar.set_prefix(format!("{}", "Cleaning".bold().cyan()));
        Self { bar }
    }
}

impl uv_cache::CleanReporter for CleaningDirectoryReporter {
    fn on_clean(&self) {
        self.bar.inc(1);
    }

    fn on_complete(&self) {
        self.bar.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct CleaningPackageReporter {
    bar: ProgressBar,
}

impl CleaningPackageReporter {
    /// Initialize a [`CleaningPackageReporter`] for cleaning packages from the cache.
    pub(crate) fn new(printer: Printer, max: usize) -> Self {
        let bar = ProgressBar::with_draw_target(Some(max as u64), printer.target());
        bar.set_style(
            ProgressStyle::with_template("{prefix} [{bar:20}] {pos}/{len}{msg}")
                .unwrap()
                .progress_chars("=> "),
        );
        bar.set_prefix(format!("{}", "Cleaning".bold().cyan()));
        Self { bar }
    }

    pub(crate) fn on_clean(&self, package: &str, removal: &Removal) {
        self.bar.inc(1);
        self.bar.set_message(format!(
            ": {}, {} files {} folders removed",
            package, removal.num_files, removal.num_dirs,
        ));
    }

    pub(crate) fn on_complete(&self) {
        self.bar.finish_and_clear();
    }
}

/// Like [`std::fmt::Display`], but with colors.
trait ColorDisplay {
    fn to_color_string(&self) -> String;
}

impl ColorDisplay for SourceDist {
    fn to_color_string(&self) -> String {
        let name = self.name();
        let version_or_url = self.version_or_url();
        format!("{}{}", name, version_or_url.to_string().dimmed())
    }
}

impl ColorDisplay for BuildableSource<'_> {
    fn to_color_string(&self) -> String {
        match self {
            BuildableSource::Dist(dist) => dist.to_color_string(),
            BuildableSource::Url(url) => url.to_string(),
        }
    }
}
