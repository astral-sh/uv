# glob AppleDouble reproduction

This standalone reproduction has two parts:

1. `setup.sh` creates an APFS source file under `./generated`, uses macOS's
   built-in `xattr` command to add a quarantine attribute, and copies the file
   into a fresh directory on an exFAT volume.
2. The standalone Rust program only prints the destination entries returned by
   the
   [`glob`](https://crates.io/crates/glob) crate for the pattern `**/*`.

On macOS, pass an exFAT volume to the setup script, then pass the generated
directory to the Rust program:

```console
$ cd debug
$ destination="$(./setup.sh /Volumes/EXFATTEST)"
$ cargo run --locked -- "$destination"
```

The copy onto exFAT creates a `._plain.txt` AppleDouble sidecar. The `glob`
crate returns both the intended `plain.txt` file and the generated sidecar:

```text
[file     ] [appledouble=true ] ._plain.txt
[file     ] [appledouble=false] plain.txt
```

The setup script creates fresh `glob-appledouble-source-<run-id>` and
`glob-appledouble-destination-<run-id>` directories for each run, so it does
not delete or overwrite existing files.
