# Gourgeist

Gourgeist is a rust library to create python virtual environments. It also has a CLI.

It currently supports only unix (linux/mac), windows support is missing.

## Rust

```rust
use camino::Utf8PathBuf;
use gourgeist::{create_venv, get_interpreter_info, parse_python_cli};

let location = cli.path.unwrap_or(Utf8PathBuf::from(".venv"));
let python = parse_python_cli(cli.python)?;
let data = get_interpreter_info(&python)?;
create_venv(&location, &python, &data, cli.bare)?;
```

## CLI

Use `python` as base for a virtualenv `.venv`:
```bash
gourgeist
```

Or use custom defaults:
```bash
gourgeist -p 3.11 my_env
```

## Jessie's gourgeist

![Jessie's gourgeist, a pokemon with a jack o'lantern as body](static/gourgeist.png)