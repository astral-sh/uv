#!/usr/bin/env bash

fail() {
    printf 'codex-cgroup: %s\n' "$*" >&2
    return 1
}

validate_codex_user() {
    [[ ${1:-} =~ ^uvcodex[[:xdigit:]]{16}$ ]]
}

validate_permission_profile() {
    [[ ${1:-} =~ ^[[:alnum:]_:][[:alnum:]_.:-]*$ ]]
}

validate_service_unit() {
    [[ ${1:-} =~ ^uv-codex-[[:digit:]]+-[[:digit:]]+-[[:xdigit:]]{16}$ ]]
}

append_service_environment() {
    local name=${1:-}
    local value=${2:-}

    case "$name" in
        PATH | CODEX_HOME | CODEX_INTERNAL_ORIGINATOR_OVERRIDE | CI | FORCE_COLOR | \
            RUNNER_TEMP | TMPDIR | UV_CACHE_DIR | CARGO_HOME | CARGO_TARGET_DIR | \
            RUSTUP_HOME | UV_PYTHON_INSTALL_DIR | INSTA_UPDATE) ;;
        *) return 1 ;;
    esac

    [[ $value != *$'\n'* && $value != *$'\r'* ]] || return 1
    SERVICE_ENVIRONMENT+=("--setenv=${name}=${value}")
}

build_service_environment() {
    local trusted_path=$1
    local codex_home=$2
    local guest_temp=$3
    local rustup_home=${CODEX_RUSTUP_HOME:-${RUSTUP_HOME:-}}
    local uv_python_install_dir=${CODEX_UV_PYTHON_INSTALL_DIR:-${UV_PYTHON_INSTALL_DIR:-}}

    SERVICE_ENVIRONMENT=()
    append_service_environment PATH "$trusted_path" || return
    append_service_environment CODEX_HOME "$codex_home" || return
    append_service_environment CODEX_INTERNAL_ORIGINATOR_OVERRIDE codex_github_action || return
    append_service_environment RUNNER_TEMP "$guest_temp" || return
    append_service_environment TMPDIR "$guest_temp/tmp" || return
    append_service_environment UV_CACHE_DIR "$guest_temp/uv-cache" || return
    append_service_environment CARGO_HOME "$guest_temp/cargo" || return
    append_service_environment CARGO_TARGET_DIR "$guest_temp/target" || return
    append_service_environment CI true || return
    append_service_environment FORCE_COLOR 1 || return

    if [[ -n ${INSTA_UPDATE:-} ]]; then
        append_service_environment INSTA_UPDATE "$INSTA_UPDATE" || return
    fi
    if [[ -n $rustup_home ]]; then
        append_service_environment RUSTUP_HOME "$rustup_home" || return
    fi
    if [[ -n $uv_python_install_dir ]]; then
        append_service_environment UV_PYTHON_INSTALL_DIR "$uv_python_install_dir" || return
    fi
}

build_service_arguments() {
    local service_unit=$1
    local codex_user=$2

    validate_service_unit "$service_unit" || return
    validate_codex_user "$codex_user" || return

    SERVICE_ARGUMENTS=(
        --quiet
        --pipe
        --wait
        "--unit=${service_unit}"
        "--uid=${codex_user}"
        --property=Type=exec
        --property=ExitType=main
        --property=KillMode=control-group
        --property=TimeoutStopSec=10s
        --property=RuntimeMaxSec=45min
        --property=SendSIGKILL=yes
        --property=NoNewPrivileges=yes
        --property=ProtectControlGroups=yes
        --property=ProtectProc=invisible
    )
    SERVICE_ARGUMENTS+=("${SERVICE_ENVIRONMENT[@]}")
}

require_hosted_linux() {
    [[ ${RUNNER_OS:-} == Linux ]] || fail "a Linux runner is required"
    [[ ${RUNNER_ENVIRONMENT:-} == github-hosted ]] || fail "a GitHub-hosted Linux runner is required"
    [[ $(stat --file-system --format=%T /sys/fs/cgroup) == cgroup2fs ]] ||
        fail "cgroup v2 is unavailable"
    [[ $(ps -p 1 -o comm= | xargs) == systemd ]] || fail "systemd is not PID 1"

    local tool
    for tool in sudo setfacl getfacl openssl realpath jq; do
        command -v "$tool" >/dev/null || fail "$tool is unavailable"
    done

    [[ -x /usr/bin/systemd-run && -x /usr/bin/systemctl ]] ||
        fail "the host systemd tools are unavailable"
    sudo -n true || fail "passwordless sudo is unavailable"
}

grant_traversal() {
    local path=$1
    local codex_user=$2

    while [[ $path != / ]]; do
        sudo -n setfacl --modify "u:${codex_user}:--x" "$path"
        path=$(dirname -- "$path")
    done
}

grant_writable_tree() {
    local path=$1
    local codex_user=$2
    local runner_user=$3

    [[ -d $path ]] || fail "writable directory does not exist: $path"
    sudo -n setfacl --recursive --modify "u:${codex_user}:rwX" "$path"
    sudo -n find "$path" -type d -exec \
        setfacl --modify "d:u:${codex_user}:rwX,d:u:${runner_user}:rwX" '{}' +
}

resolve_workspace_path() {
    local input=$1
    local workspace=$2
    local resolved

    if [[ $input == /* ]]; then
        resolved=$(realpath --canonicalize-missing -- "$input") || return
    else
        resolved=$(realpath --canonicalize-missing -- "$workspace/$input") || return
    fi

    case "$resolved" in
        "$workspace"/*) printf '%s\n' "$resolved" ;;
        *) return 1 ;;
    esac
}

prepare() {
    require_hosted_linux
    validate_permission_profile "${CODEX_PERMISSION_PROFILE:-}" ||
        fail "invalid Codex permission profile"

    local workspace
    local runner_temp
    local codex_home
    local nonce
    local codex_user
    local guest_temp
    local service_unit
    local runner_user
    local runner_group
    local rustup_home
    local uv_python_install_dir

    workspace=$(realpath -- "${GITHUB_WORKSPACE:?GITHUB_WORKSPACE is required}")
    runner_temp=$(realpath -- "${RUNNER_TEMP:?RUNNER_TEMP is required}")
    codex_home=$(resolve_workspace_path "${CODEX_CONFIG_HOME:?codex-home is required}" "$workspace") ||
        fail "codex-home must be inside the workspace"
    [[ -f $codex_home/config.toml ]] || fail "trusted Codex configuration is missing"

    nonce=$(openssl rand -hex 8)
    codex_user="uvcodex${nonce}"
    validate_codex_user "$codex_user" || fail "invalid generated Codex user"
    service_unit="uv-codex-${GITHUB_RUN_ID:?GITHUB_RUN_ID is required}-${GITHUB_RUN_ATTEMPT:?GITHUB_RUN_ATTEMPT is required}-${nonce}"
    validate_service_unit "$service_unit" || fail "invalid generated systemd unit"

    runner_user=$(id -un)
    runner_group=$(id -gn)
    guest_temp="$runner_temp/$service_unit"

    sudo -n useradd --system --create-home --user-group \
        --shell /usr/sbin/nologin -- "$codex_user"

    grant_traversal "$workspace" "$codex_user"
    grant_traversal "$runner_temp" "$codex_user"
    sudo -n setfacl --recursive --modify "u:${codex_user}:rX" "$workspace"

    grant_writable_tree "$workspace/crates/uv/tests/it" "$codex_user" "$runner_user"
    grant_writable_tree "$workspace/crates/uv-client/tests/it" "$codex_user" "$runner_user"

    sudo -n install --directory --owner="$codex_user" --group="$runner_group" \
        --mode=0750 "$guest_temp" "$guest_temp/tmp" "$guest_temp/uv-cache" \
        "$guest_temp/cargo" "$guest_temp/target"

    sudo -n install --directory --owner="$runner_user" --group="$runner_group" \
        --mode=0755 "$codex_home/sessions"
    grant_writable_tree "$codex_home/sessions" "$codex_user" "$runner_user"

    local source
    for source in issue-triage-event.json bug-reproduction-result.json; do
        if [[ -f $runner_temp/$source ]]; then
            sudo -n install --owner="$codex_user" --group="$runner_group" \
                --mode=0640 "$runner_temp/$source" "$guest_temp/$source"
        fi
    done

    rustup_home=${RUSTUP_HOME:-${HOME:?HOME is required}/.rustup}
    if [[ -d $rustup_home ]]; then
        rustup_home=$(realpath -- "$rustup_home")
        grant_traversal "$rustup_home" "$codex_user"
        sudo -n setfacl --recursive --modify "u:${codex_user}:rX" "$rustup_home"
    else
        rustup_home=
    fi
    uv_python_install_dir=${UV_PYTHON_INSTALL_DIR:-${HOME:?HOME is required}/.local/share/uv/python}
    if [[ -d $uv_python_install_dir ]]; then
        uv_python_install_dir=$(realpath -- "$uv_python_install_dir")
        grant_traversal "$uv_python_install_dir" "$codex_user"
        sudo -n setfacl --recursive --modify "u:${codex_user}:rX" "$uv_python_install_dir"
    else
        uv_python_install_dir=
    fi

    {
        printf 'codex-user=%s\n' "$codex_user"
        printf 'guest-temp=%s\n' "$guest_temp"
        printf 'service-unit=%s\n' "$service_unit"
        printf 'rustup-home=%s\n' "$rustup_home"
        printf 'uv-python-install-dir=%s\n' "$uv_python_install_dir"
    } >> "${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"
}

build_trusted_path() {
    local workspace=$1
    local runner_temp=$2
    local command_name
    local command_path
    local directory
    local trusted_path=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

    for command_name in node codex uv cargo rustup; do
        if command_path=$(command -v "$command_name" 2>/dev/null); then
            [[ $command_path == /* ]] || fail "$command_name is not an absolute executable"
            directory=$(realpath -- "$(dirname -- "$command_path")") ||
                fail "$command_name is not in a resolvable executable directory"
            case "$directory" in
                "$workspace" | "$workspace"/* | "$runner_temp" | "$runner_temp"/*)
                    fail "$command_name is inside an untrusted writable directory"
                    ;;
            esac
            case ":$trusted_path:" in
                *":$directory:"*) ;;
                *) trusted_path="$directory:$trusted_path" ;;
            esac
        elif [[ $command_name == node || $command_name == codex || $command_name == uv ]]; then
            fail "$command_name is unavailable"
        fi
    done

    printf '%s\n' "$trusted_path"
}

grant_trusted_path_access() {
    local trusted_path=$1
    local workspace=$2
    local runner_temp=$3
    local codex_user=$4
    local runner_home
    local directory
    local -a directories

    runner_home=$(realpath -- "${HOME:?HOME is required}") ||
        fail "the runner home could not be resolved"
    IFS=: read -r -a directories <<< "$trusted_path"

    for directory in "${directories[@]}"; do
        [[ -d $directory ]] || fail "trusted executable directory does not exist"

        case "$directory" in
            "$workspace" | "$workspace"/* | "$runner_temp" | "$runner_temp"/*)
                fail "an executable directory is inside an untrusted writable directory"
                ;;
        esac

        case "$directory" in
            "$runner_home" | "$runner_home"/*)
                grant_traversal "$directory" "$codex_user"
                sudo -n setfacl --recursive --modify "u:${codex_user}:rX" "$directory"
                ;;
        esac
    done
}

stop_service() {
    local service_unit=$1

    sudo -n /usr/bin/systemctl kill --kill-whom=all --signal=SIGKILL \
        "$service_unit" >/dev/null 2>&1 || true
    sudo -n /usr/bin/systemctl stop --no-block "$service_unit" >/dev/null 2>&1 || true
}

run_codex() {
    require_hosted_linux

    local codex_user=${CODEX_GUEST_USER:?CODEX_GUEST_USER is required}
    local guest_temp=${CODEX_GUEST_TEMP:?CODEX_GUEST_TEMP is required}
    local service_unit=${CODEX_SERVICE_UNIT:?CODEX_SERVICE_UNIT is required}
    local profile=${CODEX_PERMISSION_PROFILE:?CODEX_PERMISSION_PROFILE is required}
    local workspace
    local runner_temp
    local codex_home
    local prompt_file
    local schema_file
    local codex_path
    local trusted_path
    local output_file

    validate_codex_user "$codex_user" || fail "invalid Codex user"
    validate_service_unit "$service_unit" || fail "invalid systemd unit"
    validate_permission_profile "$profile" || fail "invalid Codex permission profile"

    workspace=$(realpath -- "${GITHUB_WORKSPACE:?GITHUB_WORKSPACE is required}")
    runner_temp=$(realpath -- "${RUNNER_TEMP:?RUNNER_TEMP is required}")
    [[ $guest_temp == "$runner_temp/$service_unit" && -d $guest_temp && ! -L $guest_temp ]] ||
        fail "the guest directory must be the dedicated service temporary directory"

    codex_home=$(resolve_workspace_path "${CODEX_CONFIG_HOME:?codex-home is required}" "$workspace") ||
        fail "codex-home must be inside the workspace"
    prompt_file=$(resolve_workspace_path \
        "${CODEX_PROMPT_FILE:?prompt-file is required}" "$workspace") ||
        fail "the Codex prompt must be inside the workspace"
    [[ -f $prompt_file ]] || fail "the Codex prompt does not exist"
    schema_file=$(resolve_workspace_path \
        "${CODEX_OUTPUT_SCHEMA_FILE:?output-schema-file is required}" "$workspace") ||
        fail "the output schema must be inside the workspace"
    [[ -f $schema_file ]] || fail "the Codex output schema does not exist"

    trusted_path=$(build_trusted_path "$workspace" "$runner_temp") ||
        fail "could not construct a safe Codex executable path"
    grant_trusted_path_access "$trusted_path" "$workspace" "$runner_temp" "$codex_user"
    codex_path=$(command -v codex) || fail "Codex was not installed during bootstrap"
    [[ $codex_path == /* ]] || fail "Codex executable must have an absolute path"

    build_service_environment "$trusted_path" "$codex_home" "$guest_temp" ||
        fail "invalid Codex service environment"
    build_service_arguments "$service_unit" "$codex_user" ||
        fail "invalid Codex service arguments"

    output_file="$guest_temp/final-message.json"

    trap 'stop_service "$service_unit"' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM

    {
        sudo -n /usr/bin/systemd-run "${SERVICE_ARGUMENTS[@]}" \
            -- "$codex_path" exec \
            --skip-git-repo-check \
            --cd "$workspace" \
            --output-last-message "$output_file" \
            --output-schema "$schema_file" \
            --config "default_permissions=\"${profile}\""
    } < "$prompt_file"

    [[ -f $output_file && ! -L $output_file ]] ||
        fail "Codex did not produce a regular final-message file"
    [[ $(stat --format=%s -- "$output_file") -le 65536 ]] ||
        fail "the Codex final message exceeds its 64 KiB limit"

    local message
    message=$(jq --slurp --compact-output --exit-status \
        'if length == 1 and (.[0] | type == "object") then .[0] else empty end' \
        "$output_file") ||
        fail "Codex did not produce a valid JSON final message"
    printf 'final-message=%s\n' "$message" >> "${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"

    trap - EXIT INT TERM
}

main() {
    set -euo pipefail

    case ${1:-} in
        prepare) prepare ;;
        run) run_codex ;;
        *) fail "expected prepare or run" ;;
    esac
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    main "$@"
fi
