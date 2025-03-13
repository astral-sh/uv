# globfilter

Portable directory walking with includes and excludes.

Motivating example: You want to allow the user to select paths within a project.

```toml
include = ["src", "License.txt", "resources/icons/*.svg"]
exclude = ["target", "/dist", ".cache", "*.tmp"]
```

When traversing the directory, you can use
`GlobDirFilter::from_globs(...)?.match_directory(&relative)` skip directories that never match in
`WalkDir`s `filter_entry`.

## Syntax

This crate supports the cross-language, restricted glob syntax from
[PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key):

- Alphanumeric characters, underscores (`_`), hyphens (`-`) and dots (`.`) are matched verbatim.
- The special glob characters are:
  - `*`: Matches any number of characters except path separators
  - `?`: Matches a single character except the path separator
  - `**`: Matches any number of characters including path separators
  - `[]`, containing only the verbatim matched characters: Matches a single of the characters
    contained. Within `[...]`, the hyphen indicates a locale-agnostic range (e.g., `a-z`, order
    based on Unicode code points). Hyphens at the start or end are matched literally.
- The path separator is the forward slash character (`/`). Patterns are relative to the given
  directory, a leading slash character for absolute paths is not supported.
- Parent directory indicators (`..`) are not allowed.

These rules mean that matching the backslash (`\`) is forbidden, which avoid collisions with the
windows path separator.
