#!/usr/bin/env bash
# Install the pinned Maturin implementation for release-symbol validation only.
# Source this script inside maturin-action containers so the fork takes precedence on PATH.
set -euo pipefail

install_maturin_fork() {
    local revision=65b6ccbf57add2b62001b74960eb6fed7bdb62fd
    local install_root="$PWD/target/maturin-fork-tool"
    local host
    host=$(rustc -vV | sed -n 's/^host: //p')
    test -n "$host"

    (
        # The build tool runs on the host and must not inherit uv's cross/PGO configuration.
        unset CARGO RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_RUSTFLAGS
        unset CARGO_TARGET_DIR CARGO_BUILD_TARGET RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
        unset CC CXX AR CFLAGS CXXFLAGS LDFLAGS
        local variable
        while IFS= read -r variable; do unset "$variable"; done < <(compgen -v CARGO_TARGET_ || true)
        export CARGO_PROFILE_DEV_DEBUG=0
        cargo install maturin --debug --locked --jobs 4 --bin maturin \
            --git https://github.com/ewdurbin/maturin --rev "$revision" \
            --target "$host" --root "$install_root"
    )

    local executable=maturin
    if [[ "${RUNNER_OS:-}" == Windows ]]; then executable=maturin.exe; fi
    echo "Maturin fork: ewdurbin/maturin@$revision (host $host)"
    if [[ "${RUNNER_OS:-}" == macOS ]]; then
        shasum -a 256 "$install_root/bin/$executable"
    else
        sha256sum "$install_root/bin/$executable"
    fi

    if [[ "${RUNNER_OS:-}" == macOS || "${RUNNER_OS:-}" == Windows ]]; then
        local tool_cache="$RUNNER_TOOL_CACHE"
        if [[ "$RUNNER_OS" == Windows ]]; then tool_cache=$(cygpath -u "$tool_cache"); fi
        # maturin-action uses the literal x64 cache key on both native platforms.
        local cached="$tool_cache/maturin/1.15.0-fork.65b6ccbf/x64"
        mkdir -p "$cached"
        cp "$install_root/bin/$executable" "$cached/$executable"
        touch "$cached.complete"
    fi
    export PATH="$install_root/bin:$PATH"
}

install_maturin_fork
