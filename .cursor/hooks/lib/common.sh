#!/usr/bin/env bash
#
# ARRIVE Hooks - Common Utilities
# Shared functions for all hook scripts
#

# Strict mode (but don't exit on error - hooks must be resilient)
set -uo pipefail

# ============================================================================
# Configuration
# ============================================================================

ARRIVE_CLI="${ARRIVE_CLI:-arrive}"
ARRIVE_HOOKS_DEBUG="${ARRIVE_HOOKS_DEBUG:-0}"

# Repo-local hooks state file (pause toggle)
ARRIVE_HOOKS_STATE_FILE="${ARRIVE_HOOKS_STATE_FILE:-.arrive/hooks.json}"
# Repo-local hook activity log (append-only JSONL)
ARRIVE_HOOKS_ACTIVITY_LOG="${ARRIVE_HOOKS_ACTIVITY_LOG:-.arrive/hook-activity.jsonl}"
ARRIVE_WORK_CONTEXT_FILE="${ARRIVE_WORK_CONTEXT_FILE:-.arrive/work-context.json}"
ARRIVE_AUTH_CONFIG_FILE="${ARRIVE_AUTH_CONFIG_FILE:-.arrive/auth.json}"

# ============================================================================
# Logging (to stderr, so it doesn't interfere with JSON output)
# ============================================================================

log_debug() {
    if [[ "$ARRIVE_HOOKS_DEBUG" == "1" ]]; then
        echo "[ARRIVE-HOOK DEBUG] $*" >&2
    fi
}

log_error() {
    echo "[ARRIVE-HOOK ERROR] $*" >&2
}

# ============================================================================
# Repo-local hooks state (pause)
# ============================================================================

find_repo_root() {
    git rev-parse --show-toplevel 2>/dev/null || return 1
}

hooks_state_path() {
    local repo_root
    repo_root=$(find_repo_root) || return 1
    echo "$repo_root/$ARRIVE_HOOKS_STATE_FILE"
}

hooks_activity_log_path() {
    local repo_root
    repo_root=$(find_repo_root) || return 1
    echo "$repo_root/$ARRIVE_HOOKS_ACTIVITY_LOG"
}

hooks_are_paused() {
    local path
    path=$(hooks_state_path) || return 1
    [[ -f "$path" ]] || return 1

    if has_jq; then
        local paused
        paused=$(jq -r '.paused // false' "$path" 2>/dev/null || echo "false")
        [[ "$paused" == "true" ]]
    else
        # Best-effort fallback (limited): look for 'paused: true' or '"paused": true'
        grep -Eq 'paused[[:space:]]*[:=][[:space:]]*true|\"paused\"[[:space:]]*:[[:space:]]*true' "$path" 2>/dev/null
    fi
}

# Best-effort: extract pause reason/updated_at for logging.
hooks_pause_reason() {
    local path
    path=$(hooks_state_path) || return 1
    [[ -f "$path" ]] || return 1

    if has_jq; then
        jq -r '.reason // empty' "$path" 2>/dev/null || true
    elif command -v python3 &>/dev/null; then
        python3 - "$path" <<'PY' 2>/dev/null || true
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        v = json.load(f)
    s = v.get("reason") or ""
    if s:
        print(s)
except Exception:
    pass
PY
    else
        echo ""
    fi
}

hooks_pause_updated_at() {
    local path
    path=$(hooks_state_path) || return 1
    [[ -f "$path" ]] || return 1

    if has_jq; then
        jq -r '.updated_at // empty' "$path" 2>/dev/null || true
    elif command -v python3 &>/dev/null; then
        python3 - "$path" <<'PY' 2>/dev/null || true
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        v = json.load(f)
    s = v.get("updated_at") or ""
    if s:
        print(s)
except Exception:
    pass
PY
    else
        echo ""
    fi
}

# ============================================================================
# Hook activity logging (append-only JSONL)
# ============================================================================

append_hook_activity() {
    local hook="$1"      # afterFileEdit | beforeShellExecution | stop
    local outcome="$2"   # ran | skipped | paused | error
    local details_json="${3:-{}}"

    local repo_root
    repo_root=$(find_repo_root) || return 1

    local log_path
    log_path=$(hooks_activity_log_path) || return 1

    mkdir -p "$(dirname "$log_path")" 2>/dev/null || true

    local occurred_at
    occurred_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    local advance_id actor_email paused_reason paused_updated_at
    advance_id=$(get_active_work_advance_id "$repo_root" 2>/dev/null || true)
    actor_email=$(get_actor_email "$repo_root" 2>/dev/null || true)
    paused_reason=""
    paused_updated_at=""
    if [[ "$outcome" == "paused" ]]; then
        paused_reason=$(hooks_pause_reason 2>/dev/null || true)
        paused_updated_at=$(hooks_pause_updated_at 2>/dev/null || true)
    fi

    log_debug "hook_activity hook=$hook outcome=$outcome advance_id=$advance_id actor_email=$actor_email paused_reason=$paused_reason paused_updated_at=$paused_updated_at details_json=$details_json"

    if has_jq; then
        # Ensure details is valid JSON; fall back to {} if not.
        if ! echo "$details_json" | jq -e . >/dev/null 2>&1; then
            details_json="{}"
        fi

        local line=""
        line=$(jq -cn \
            --arg v "1" \
            --arg occurred_at "$occurred_at" \
            --arg hook "$hook" \
            --arg outcome "$outcome" \
            --arg repo_root "$repo_root" \
            --arg advance_id "$advance_id" \
            --arg actor_email "$actor_email" \
            --arg paused_reason "$paused_reason" \
            --arg paused_updated_at "$paused_updated_at" \
            --argjson details "$details_json" \
            '{
              version: ($v|tonumber),
              occurred_at: $occurred_at,
              hook: $hook,
              outcome: $outcome,
              repo_root: $repo_root
            }
            + ( $advance_id | length > 0 ? { advance_id: $advance_id } : {} )
            + {
              details: (
                $details
                + ( $actor_email | length > 0 ? { actor_email: $actor_email } : {} )
                + ( $paused_reason | length > 0 ? { paused_reason: $paused_reason } : {} )
                + ( $paused_updated_at | length > 0 ? { paused_updated_at: $paused_updated_at } : {} )
              )
            }' 2>/dev/null) || line=""

        if [[ -n "$line" ]]; then
            printf '%s\n' "$line" >>"$log_path" 2>/dev/null || true
            return 0
        fi
        # If jq failed, fall through to minimal logging.
    fi

    # Fallback without jq: prefer python3 to safely construct JSON (preserves details).
    if command -v python3 &>/dev/null; then
        python3 - "$occurred_at" "$hook" "$outcome" "$repo_root" "$advance_id" "$actor_email" "$paused_reason" "$paused_updated_at" "$details_json" >>"$log_path" 2>/dev/null <<'PY' || true
import json
import sys

occurred_at, hook, outcome, repo_root, advance_id, actor_email, paused_reason, paused_updated_at, details_json = sys.argv[1:]
try:
    details = json.loads(details_json) if details_json.strip() else {}
    if not isinstance(details, dict):
        details = {"value": details}
except Exception:
    # Some callers accidentally double-close JSON (e.g. `{"k":"v"}}`).
    # Try a simple repair before giving up.
    repaired = details_json[:-1] if details_json.endswith("}}") else None
    if repaired is not None:
        try:
            details = json.loads(repaired) if repaired.strip() else {}
            if not isinstance(details, dict):
                details = {"value": details}
        except Exception:
            details = {"raw_details": details_json}
    else:
        # Preserve something useful for debugging, rather than dropping details entirely.
        details = {"raw_details": details_json}

if actor_email:
    details.setdefault("actor_email", actor_email)
if paused_reason:
    details.setdefault("paused_reason", paused_reason)
if paused_updated_at:
    details.setdefault("paused_updated_at", paused_updated_at)

ev = {
    "version": 1,
    "occurred_at": occurred_at,
    "hook": hook,
    "outcome": outcome,
    "repo_root": repo_root,
    "details": details,
}
if advance_id:
    ev["advance_id"] = advance_id

sys.stdout.write(json.dumps(ev, separators=(",", ":")) + "\n")
PY
        return 0
    fi

    # Minimal fallback without jq/python: write a small JSON object without details.
    printf '{"version":1,"occurred_at":"%s","hook":"%s","outcome":"%s","repo_root":"%s"%s}\n' \
        "$occurred_at" "$hook" "$outcome" "$repo_root" \
        "${advance_id:+,\"advance_id\":\"$advance_id\"}" >>"$log_path" 2>/dev/null || true
}
# ============================================================================
# JSON Helpers
# ============================================================================

# Check if jq is available
has_jq() {
    command -v jq &>/dev/null
}

# Shell command from hook stdin (Cursor: top-level "command"; Claude Code: tool_input.command)
hook_shell_command() {
    local json="$1"
    local cmd
    cmd=$(json_get "$json" "command" "")
    if [[ -n "$cmd" ]]; then
        echo "$cmd"
        return 0
    fi
    if has_jq; then
        echo "$json" | jq -r '.tool_input.command // empty' 2>/dev/null
    fi
}

# Parse JSON field (with fallback if jq not available)
json_get() {
    local json="$1"
    local field="$2"
    local default="${3:-}"
    
    if has_jq; then
        local value
        value=$(echo "$json" | jq -r ".$field // empty" 2>/dev/null)
        if [[ -n "$value" && "$value" != "null" ]]; then
            echo "$value"
        else
            echo "$default"
        fi
    else
        # Fallback: simple grep-based extraction (limited)
        echo "$default"
    fi
}

# Create notification JSON
json_notification() {
    local type="$1"  # info, warning, error
    local message="$2"
    
    if has_jq; then
        jq -n \
            --arg type "$type" \
            --arg msg "$message" \
            '{notifications: [{type: $type, message: $msg}]}'
    else
        # Fallback: manual JSON construction
        printf '{"notifications":[{"type":"%s","message":"%s"}]}' "$type" "$message"
    fi
}

# Create decision JSON
json_decision() {
    local decision="$1"  # allow, warn, block
    local message="$2"
    
    if has_jq; then
        jq -n \
            --arg dec "$decision" \
            --arg msg "$message" \
            '{decision: $dec, message: $msg}'
    else
        printf '{"decision":"%s","message":"%s"}' "$decision" "$message"
    fi
}

# Create empty/neutral response
json_empty() {
    echo '{}'
}

# ============================================================================
# ARRIVE CLI Wrappers
# ============================================================================

# Check if arrive CLI is available
has_arrive() {
    command -v "$ARRIVE_CLI" &>/dev/null
}

# Run arrive status and capture output
arrive_status() {
    if has_arrive; then
        "$ARRIVE_CLI" status 2>/dev/null
    else
        log_debug "arrive CLI not found"
        return 1
    fi
}

# Run arrive score and get JSON output
arrive_score_json() {
    if has_arrive; then
        "$ARRIVE_CLI" score --json 2>/dev/null
    else
        log_debug "arrive CLI not found"
        return 1
    fi
}

# Run arrive score and get plain output
arrive_score() {
    if has_arrive; then
        "$ARRIVE_CLI" score 2>/dev/null
    else
        log_debug "arrive CLI not found"
        return 1
    fi
}

# Run arrive check
arrive_check() {
    local strict="${1:-}"
    if has_arrive; then
        if [[ "$strict" == "--strict" ]]; then
            "$ARRIVE_CLI" check --strict 2>/dev/null
        else
            "$ARRIVE_CLI" check 2>/dev/null
        fi
    else
        log_debug "arrive CLI not found"
        return 1
    fi
}

# Run arrive draft
arrive_draft() {
    if has_arrive; then
        "$ARRIVE_CLI" draft 2>/dev/null
    else
        log_debug "arrive CLI not found"
        return 1
    fi
}

# Run arrive log
arrive_log() {
    local advance="$1"
    local commit_type="$2"
    local summary="$3"
    
    if has_arrive; then
        "$ARRIVE_CLI" log \
            --advance "$advance" \
            --commit-type "$commit_type" \
            --summary "$summary" 2>/dev/null
    else
        log_debug "arrive CLI not found"
        return 1
    fi
}

# ============================================================================
# Score Level Helpers
# ============================================================================

# Get score level from JSON score output
get_score_level() {
    local score_json="$1"
    json_get "$score_json" "level" "unknown"
}

# Get total score from JSON score output
get_score_total() {
    local score_json="$1"
    json_get "$score_json" "total" "0"
}

# Format score message based on level
format_score_message() {
    local total="$1"
    local level="$2"
    
    case "$level" in
        green)
            echo "Score: $total [GREEN] - looking good"
            ;;
        yellow)
            echo "Score: $total [YELLOW] - consider checkpoint"
            ;;
        red)
            echo "Score: $total [RED] - split recommended"
            ;;
        *)
            echo "Score: $total"
            ;;
    esac
}

# ============================================================================
# Advance Helpers
# ============================================================================

# Find existing advances in planned status
find_planned_advances() {
    local repo_root
    repo_root=$(git rev-parse --show-toplevel 2>/dev/null) || return 1
    
    local arrive_dir="$repo_root/arrive/systems"
    if [[ ! -d "$arrive_dir" ]]; then
        return 1
    fi
    
    # Find all advance files with status: planned
    find "$arrive_dir" -name "ADV-*.md" -type f 2>/dev/null | while read -r file; do
        if grep -q "status: planned" "$file" 2>/dev/null; then
            echo "$file"
        fi
    done
}

# Get first planned advance (if any)
get_current_advance() {
    find_planned_advances | head -1
}

# Extract advance ID from file path
get_advance_id() {
    local file="$1"
    basename "$file" .md
}

# ============================================================================
# Actor + Work Context Helpers
# ============================================================================

get_actor_email() {
    local repo_root="$1"

    # Prefer repo-local auth config
    local auth_path="$repo_root/$ARRIVE_AUTH_CONFIG_FILE"
    if [[ -f "$auth_path" ]]; then
        if has_jq; then
            local email
            email=$(jq -r '.email // empty' "$auth_path" 2>/dev/null || true)
            [[ -n "$email" ]] && { echo "$email"; return 0; }
        elif command -v python3 &>/dev/null; then
            local email
            email=$(python3 - "$auth_path" <<'PY' 2>/dev/null || true
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        cfg = json.load(f)
    email = cfg.get("email") or ""
    if email:
        print(email)
except Exception:
    pass
PY
)
            [[ -n "$email" ]] && { echo "$email"; return 0; }
        fi
    fi

    # Fall back to git config
    git config user.email 2>/dev/null || true
}

get_active_work_advance_id() {
    local repo_root="$1"

    local ctx_path="$repo_root/$ARRIVE_WORK_CONTEXT_FILE"
    if [[ -f "$ctx_path" ]]; then
        if has_jq; then
            local aid
            aid=$(jq -r '.slots.default.advance_id // empty' "$ctx_path" 2>/dev/null || true)
            [[ -n "$aid" ]] && { echo "$aid"; return 0; }
        elif command -v python3 &>/dev/null; then
            local aid
            aid=$(python3 - "$ctx_path" <<'PY' 2>/dev/null || true
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        ctx = json.load(f)
    slots = ctx.get("slots") or {}
    default = slots.get("default") or {}
    aid = default.get("advance_id") or ""
    if aid:
        print(aid)
except Exception:
    pass
PY
)
            [[ -n "$aid" ]] && { echo "$aid"; return 0; }
        fi
    fi

    # Best-effort fallback via CLI (if available)
    if has_arrive; then
        local work_json
        work_json=$("$ARRIVE_CLI" work show --json 2>/dev/null || true)
        if [[ -n "$work_json" ]]; then
            if has_jq; then
                echo "$work_json" | jq -r '.slots.default.advance_id // empty' 2>/dev/null || true
            elif command -v python3 &>/dev/null; then
                python3 - <<'PY' 2>/dev/null || true
import json, sys
try:
    ctx = json.loads(sys.stdin.read())
    slots = ctx.get("slots") or {}
    default = slots.get("default") or {}
    aid = default.get("advance_id") or ""
    if aid:
        print(aid)
except Exception:
    pass
PY
            fi
        fi
    fi
}

# ============================================================================
# Git Helpers
# ============================================================================

# Check if we're in a git repo
in_git_repo() {
    git rev-parse --git-dir &>/dev/null
}

# Get list of changed files
get_changed_files() {
    if in_git_repo; then
        git diff --name-only 2>/dev/null
        git diff --name-only --cached 2>/dev/null
    fi | sort -u
}

# ============================================================================
# Platform Helpers (TD-PLAT-012, ADV-HOOKS-006)
# ============================================================================

# Emit a reminder (to stdout) when the active advance is complete but has no
# dispatch in the platform repo. Prints nothing (exit 0) in every other case:
# no platform block / not a member / no active advance / advance not complete /
# already published. Never writes anything — the nudge proposes
# /arrive-platform-publish; publication stays propose-and-confirm.
platform_publish_nudge() {
    local repo_root
    repo_root=$(find_repo_root) || return 0
    local registry="$repo_root/arrive/registry.yaml"
    [[ -f "$registry" ]] || return 0
    grep -q '^platform:' "$registry" 2>/dev/null || return 0
    sed -n '/^platform:/,/^[^ ]/p' "$registry" | grep -q 'role: member' || return 0

    # Dispatches are opt-in (TD-PLAT-013). Default off when adoption is absent.
    if sed -n '/^platform:/,/^[^ ]/p' "$registry" | grep -q '^  adoption:'; then
        sed -n '/^platform:/,/^[^ ]/p' "$registry" | grep -q 'dispatches: true' || return 0
    else
        return 0
    fi

    local advance_id
    advance_id=$(get_active_work_advance_id "$repo_root" 2>/dev/null || true)
    [[ -n "$advance_id" ]] || return 0

    local advance_file
    advance_file=$(find "$repo_root/arrive/systems" -path "*/advances/$advance_id.md" 2>/dev/null | head -1)
    [[ -n "$advance_file" ]] || return 0
    grep -q 'status: complete' "$advance_file" || return 0

    local plat_path repo_id
    plat_path=$(sed -n '/^platform:/,/^[^ ]/p' "$registry" | sed -n 's/^  path: *//p' | head -1)
    repo_id=$(sed -n 's/^  repo_id: *//p' "$registry" | head -1)
    [[ -n "$plat_path" && -n "$repo_id" ]] || return 0

    if [[ ! -d "$repo_root/$plat_path" ]]; then
        echo " Advance $advance_id is complete but unpublished — platform checkout not found at $plat_path (see 'arrive platform check'). Silence: ARRIVE_PLATFORM_HOOKS=0."
        return 0
    fi

    local dispatch_dir="$repo_root/$plat_path/arrive/platform/dispatches/$repo_id"
    if compgen -G "$dispatch_dir/*--$advance_id.yaml" >/dev/null 2>&1; then
        return 0
    fi

    echo " Advance $advance_id is complete but has no platform dispatch — consider /arrive-platform-publish."
}
