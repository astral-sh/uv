# Style guide

_The following is a work-in-progress style guide for our user-facing messaging in the CLI output and
documentation_.

## General

1. Use of "e.g." and "i.e." should always be wrapped in commas, e.g., as shown here.
1. Em-dashes are okay, but not recommended when using monospace fonts. Use "—", not "--" or "-".
1. Always wrap em-dashes in spaces, e.g., "hello — world" not "hello—world".
1. Hyphenate compound words, e.g., use "platform-specific" not "platform specific".
1. Use backticks to escape: commands, code expressions, package names, and file paths.
1. Use less than and greater than symbols to wrap bare URLs, e.g., `<https://astral.sh>` (unless it
   is an example; then, use backticks).
1. Avoid bare URLs outside of reference documentation, prefer labels, e.g., `[name](url)`.
1. If a message ends with a single relevant value, precede it with a colon, e.g.,
   `This is the value: value`. If the value is a literal, wrap it in backticks.
1. Markdown files should be wrapped at 100 characters.
1. Use a space, not an equals sign, for command line arguments with a value, e.g.
   `--resolution lowest`, not `--resolution=lowest`.

## Styling uv

Just uv, please.

1. Do not escape with backticks, e.g., `uv`, unless referring specifically to the `uv` executable.
1. Do not capitalize, e.g., "Uv", even at the beginning of a sentence.
1. Do not uppercase, e.g., "UV", unless referring to an environment variable, e.g., `UV_PYTHON`.

## Terminology

1. Use "lockfile" not "lock file".
2. Use "pre-release", not "prerelease" (except in code, in which case: use `Prerelease`, not
   `PreRelease`; and `prerelease`, not `pre_release`).

## Documentation

1. Use periods at the end of all sentences, including lists unless they enumerate single items.
1. Avoid language that patronizes the reader, e.g., "simply do this".
1. Only refer to "the user" in internal or contributor documentation.
1. Avoid "we" in favor of "uv" or imperative language.

### Sections

The documentation is divided into:

1. Guides
2. Concepts
3. Reference documentation

#### Guides

1. Should assume no previous knowledge about uv.
1. May assume basic knowledge of the domain.
1. Should refer to relevant concept documentation.
1. Should have a clear flow.
1. Should be followed by a clear call to action.
1. Should cover the basic behavior needed to get started.
1. Should not cover behavior in detail.
1. Should not enumerate all possibilities.
1. Should avoid linking to reference documentation unless not covered in a concept document.
1. May generally ignore platform-specific behavior.
1. Should be written from second-person point of view.
1. Should use the imperative voice.

#### Concepts

1. Should cover behavior in detail.
1. Should not enumerate all possibilities.
1. Should cover most common configuration.
1. Should refer to the relevant reference documentation.
1. Should discuss platform-specific behavior.
1. Should be written from the third-person point of view, not second-person (i.e., avoid "you").
1. Should not use the imperative voice.

#### Reference documentation

1. Should enumerate all options.
1. Should generally be generated from documentation in the code.
1. Should be written from the third-person point of view, not second-person (i.e., avoid "you").
1. Should not use the imperative voice.

### Code blocks

1. All code blocks should have a language marker.
1. When using `console` syntax, use `$` to indicate commands — everything else is output.
1. Never use the `bash` syntax when displaying command output.
1. Prefer `console` with `$` prefixed commands over `bash`.
1. Command output should rarely be included — it's hard to keep up to date.
1. Use `title` for example files, e.g., `pyproject.toml`, `Dockerfile`, or `example.py`.

## CLI

1. Do not use periods at the end of sentences :), unless the message spans more than a single
   sentence.
1. May use the second-person point of view, e.g., "Did you mean...?".

### Colors and style

1. All CLI output must be interpretable and understandable _without_ the use of color and other
   styling. (For example: even if a command is rendered in green, wrap it in backticks.)
1. `NO_COLOR` must be respected when using any colors or styling.
1. `UV_NO_PROGRESS` must be respected when using progress-styling like bars or spinners.
1. In general, use:
   - Green for success.
   - Red for error.
   - Yellow for warning.
   - Cyan for hints.
   - Cyan for file paths.
   - Cyan for important user-facing literals (e.g., a package name in a message).
   - Green for commands.

### Logging

1. `warn`, `info`, `debug`, and `trace` logs are all shown with the `--verbose` flag.
   - Note that the displayed level is controlled with `RUST_LOG`.
1. All logging should be to stderr.

### Output

1. Text can be written to stdout if it is "data" that could be piped to another program.

### Warnings

1. `warn_user` and `warn_user_once` are shown without the `--verbose `flag.
   - These methods should be preferred over tracing warnings when the warning is actionable.
   - Deprecation warnings should use these methods.
1. Deprecation warnings must be actionable.

### Hints

1. Errors may be followed by hints suggesting a solution.
1. Hints should be separated from errors by a blank newline.
1. Hints should be stylized as `hint: <content>`.
