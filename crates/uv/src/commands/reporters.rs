use std::env;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use url::Url;

use distribution_types::{
    BuildableSource, CachedDist, DistributionMetadata, Name, SourceDist, VersionOrUrlRef,
};
use uv_normalize::PackageName;
use uv_python::PythonInstallationKey;

use crate::printer::Printer;

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

impl ProgressReporter {
    fn new(root: ProgressBar, multi_progress: MultiProgress, printer: Printer) -> ProgressReporter {
        let mode = if env::var("JPY_SESSION_NAME").is_ok() {
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
        progress.set_message(format!(
            "{} {}",
            "Building".bold().cyan(),
            source.to_color_string()
        ));

        state.headers += 1;
        state.bars.insert(id, progress);
        id
    }

    fn on_build_complete(&self, source: &BuildableSource, id: usize) {
        let ProgressMode::Multi { state, .. } = &self.mode else {
            return;
        };

        let progress = {
            let mut state = state.lock().unwrap();
            state.headers -= 1;
            state.bars.remove(&id).unwrap()
        };

        progress.finish_with_message(format!(
            "   {} {}",
            "Built".bold().green(),
            source.to_color_string()
        ));
    }

    fn on_download_start(&self, name: String, size: Option<u64>) -> usize {
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

        if size.is_some() {
            progress.set_style(
                ProgressStyle::with_template(
                    "{msg:10.dim} {bar:30.green/dim} {decimal_bytes:>7}/{decimal_total_bytes:7}",
                )
                .unwrap()
                .progress_chars("--"),
            );
            progress.set_message(name);
        } else {
            progress.set_style(ProgressStyle::with_template("{wide_msg:.dim} ....").unwrap());
            progress.set_message(name);
            progress.finish();
        }

        let id = state.id();
        state.bars.insert(id, progress);
        id
    }

    fn on_download_progress(&self, id: usize, bytes: u64) {
        let ProgressMode::Multi { state, .. } = &self.mode else {
            return;
        };

        state.lock().unwrap().bars[&id].inc(bytes);
    }

    fn on_download_complete(&self, id: usize) {
        let ProgressMode::Multi { state, .. } = &self.mode else {
            return;
        };

        let progress = state.lock().unwrap().bars.remove(&id).unwrap();
        progress.finish_and_clear();
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
        progress.set_message(format!(
            "{} {} ({})",
            "Updating".bold().cyan(),
            url,
            rev.dimmed()
        ));
        progress.finish();

        state.headers += 1;
        state.bars.insert(id, progress);
        id
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, id: usize) {
        let ProgressMode::Multi { state, .. } = &self.mode else {
            return;
        };

        let progress = {
            let mut state = state.lock().unwrap();
            state.headers -= 1;
            state.bars.remove(&id).unwrap()
        };

        progress.finish_with_message(format!(
            " {} {} ({})",
            "Updated".bold().green(),
            url,
            rev.dimmed()
        ));
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
