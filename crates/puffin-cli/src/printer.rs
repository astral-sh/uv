use anstream::eprint;
use indicatif::ProgressDrawTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Printer {
    /// A printer that prints to standard streams (e.g., stdout).
    Default,
    /// A printer that suppresses all output.
    Quiet,
    /// A printer that prints all output, including debug messages.
    Verbose,
}

impl Printer {
    /// Return the [`ProgressDrawTarget`] for this printer.
    pub(crate) fn target(self) -> ProgressDrawTarget {
        match self {
            Self::Default => ProgressDrawTarget::stderr(),
            Self::Quiet => ProgressDrawTarget::hidden(),
            // Confusingly, hide the progress bar when in verbose mode.
            // Otherwise, it gets interleaved with debug messages.
            Self::Verbose => ProgressDrawTarget::hidden(),
        }
    }
}

impl std::fmt::Write for Printer {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        match self {
            Self::Default | Self::Verbose => {
                #[allow(clippy::print_stderr, clippy::ignored_unit_patterns)]
                {
                    eprint!("{s}");
                }
            }
            Self::Quiet => {}
        }

        Ok(())
    }
}
