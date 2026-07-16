#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf '%s\n' "Usage: $0 <run-id-or-url> [--repo OWNER/REPO] [--artifact PATTERN] [--cwd PATH]"
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

run=""
repository="${GITHUB_REPOSITORY:-}"
artifact_pattern="codex-thread-*"
working_directory=""

while (($#)); do
    case "$1" in
        --repo)
            (($# >= 2)) || fail "--repo requires OWNER/REPO"
            repository="$2"
            shift 2
            ;;
        --artifact)
            (($# >= 2)) || fail "--artifact requires a pattern"
            artifact_pattern="$2"
            shift 2
            ;;
        --cwd)
            (($# >= 2)) || fail "--cwd requires a path"
            working_directory="$2"
            shift 2
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        -*)
            fail "unknown option: $1"
            ;;
        *)
            [[ -z "$run" ]] || fail "only one run ID or URL can be provided"
            run="$1"
            shift
            ;;
    esac
done

[[ -n "$run" ]] || {
    usage >&2
    exit 2
}

for command_name in gh jq codex; do
    command -v "$command_name" >/dev/null 2>&1 || fail "$command_name is required"
done

if [[ "$run" =~ ^https://github\.com/([^/]+/[^/]+)/actions/runs/([0-9]+)([/\?#].*)?$ ]]; then
    repository="${BASH_REMATCH[1]}"
    run_id="${BASH_REMATCH[2]}"
elif [[ "$run" =~ ^[0-9]+$ ]]; then
    run_id="$run"
else
    fail "expected a GitHub Actions run ID or URL, got: $run"
fi

if [[ -z "$repository" ]]; then
    repository="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')" || fail "could not determine the GitHub repository; pass --repo OWNER/REPO"
fi

[[ "$repository" =~ ^[^/]+/[^/]+$ ]] || fail "expected OWNER/REPO, got: $repository"

if [[ -z "$working_directory" ]]; then
    working_directory="$(git rev-parse --show-toplevel 2>/dev/null)" || fail "could not determine the repository root; pass --cwd PATH"
fi
working_directory="$(cd "$working_directory" 2>/dev/null && pwd -P)" || fail "working directory does not exist: $working_directory"

scratch_root="${CODEX_THREAD_SCRATCH_DIR:-${HOME:?}/code/tmp}"
mkdir -p "$scratch_root"
scratch_root="$(cd "$scratch_root" && pwd -P)"
download_directory="$(mktemp -d "$scratch_root/load-github-action-thread.XXXXXXXX")"
server_pid=""

cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi

    if [[ "$download_directory" == "$scratch_root"/load-github-action-thread.* ]]; then
        rm -rf -- "$download_directory"
    fi
}
trap cleanup EXIT INT TERM

printf 'Downloading %s from %s run %s...\n' "$artifact_pattern" "$repository" "$run_id"
if ! gh run download "$run_id" --repo "$repository" --pattern "$artifact_pattern" --dir "$download_directory"; then
    fail "could not download matching artifacts from $repository run $run_id"
fi

rollouts=()
while IFS= read -r -d '' rollout; do
    rollouts+=("$rollout")
done < <(find "$download_directory" -type f \( -name 'rollout-*.jsonl' -o -name 'rollout-*.jsonl.zst' \) -print0)

((${#rollouts[@]})) || fail "no Codex rollout files were found in the downloaded artifacts"

server_log="$download_directory/app-server.stderr"
coproc app_server { codex app-server --stdio 2>"$server_log"; }
server_pid="$!"
exec {server_input}>&"${app_server[1]}"
exec {server_output}<&"${app_server[0]}"

read_response() {
    local expected_id="$1"
    local response response_id

    while IFS= read -r -t 120 response <&"$server_output"; do
        response_id="$(jq -r '.id // empty' <<<"$response" 2>/dev/null || true)"
        if [[ "$response_id" == "$expected_id" ]]; then
            printf '%s\n' "$response"
            return 0
        fi
    done

    printf 'error: Codex app-server did not return response %s\n' "$expected_id" >&2
    if [[ -s "$server_log" ]]; then
        sed -n '1,80p' "$server_log" >&2
    fi
    return 1
}

check_response() {
    local response="$1"
    local operation="$2"

    if jq -e '.error != null' >/dev/null <<<"$response"; then
        printf 'error: Codex app-server could not %s: %s\n' "$operation" "$(jq -r '.error.message // (.error | tostring)' <<<"$response")" >&2
        return 1
    fi
}

initialize="$(jq -cn '{id: 1, method: "initialize", params: {clientInfo: {name: "load-github-action-thread", version: "1"}, capabilities: {experimentalApi: true}}}')"
printf '%s\n' "$initialize" >&"$server_input"
initialize_response="$(read_response 1)" || exit 1
check_response "$initialize_response" "initialize" || exit 1
printf '%s\n' '{"method":"initialized","params":{}}' >&"$server_input"

request_id=2
for rollout in "${rollouts[@]}"; do
    fork_request="$(jq -cn --argjson id "$request_id" --arg path "$rollout" --arg cwd "$working_directory" '{id: $id, method: "thread/fork", params: {threadId: "ignored", path: $path, cwd: $cwd, excludeTurns: true}}')"
    printf '%s\n' "$fork_request" >&"$server_input"
    fork_response="$(read_response "$request_id")" || exit 1
    check_response "$fork_response" "fork $rollout" || exit 1

    thread_id="$(jq -r '.result.thread.id // empty' <<<"$fork_response")"
    source_id="$(jq -r '.result.thread.forkedFromId // empty' <<<"$fork_response")"
    [[ -n "$thread_id" ]] || fail "Codex app-server returned no thread ID for $rollout"

    printf 'Loaded Codex thread %s' "$thread_id"
    if [[ -n "$source_id" ]]; then
        printf ' (forked from %s)' "$source_id"
    fi
    printf '\n'

    request_id=$((request_id + 1))
done

printf 'Loaded %s Codex thread(s) for %s.\n' "${#rollouts[@]}" "$working_directory"
