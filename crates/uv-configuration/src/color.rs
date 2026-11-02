/// Whether command output should use colors.
#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum ColorChoice {
    /// Enables colored output only when the output is going to a terminal or TTY with support.
    Auto,

    /// Enables colored output regardless of the detected environment.
    Always,

    /// Disables colored output.
    Never,
}

impl ColorChoice {
    /// Return the command-line representation of this color choice.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }

    /// Combine self (higher priority) with an [`anstream::ColorChoice`] (lower priority).
    ///
    /// This method allows prioritizing the user choice, while using the inferred choice for a
    /// stream as default.
    #[must_use]
    pub fn and_colorchoice(self, next: anstream::ColorChoice) -> Self {
        match self {
            Self::Auto => match next {
                anstream::ColorChoice::Auto => Self::Auto,
                anstream::ColorChoice::Always | anstream::ColorChoice::AlwaysAnsi => Self::Always,
                anstream::ColorChoice::Never => Self::Never,
            },
            Self::Always | Self::Never => self,
        }
    }
}

impl From<ColorChoice> for anstream::ColorChoice {
    fn from(value: ColorChoice) -> Self {
        match value {
            ColorChoice::Auto => Self::Auto,
            ColorChoice::Always => Self::Always,
            ColorChoice::Never => Self::Never,
        }
    }
}
