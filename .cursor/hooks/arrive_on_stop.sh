#!/usr/bin/env bash
#
# ARRIVE Hook: stop
# Snapshots the session to an Advance file with Changes Made log
#
# Input (stdin): JSON with session context
# Output (stdout): JSON with notifications
#

set -uo pipefail

# Load common utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

# ============================================================================
# Session Snapshot Logic
# ============================================================================

# Create or find advance for current session
ensure_advance() {
    local advance
    advance=$(get_current_advance)
    
    if [[ -n "$advance" ]]; then
        echo "$advance"
        return 0
    fi
    
    # Try to create one
    if has_arrive; then
        arrive_draft >/dev/null 2>&1
        # Check again
        advance=$(get_current_advance)
        if [[ -n "$advance" ]]; then
            echo "$advance"
            return 0
        fi
    fi
    
    return 1
}

# Generate session summary
generate_session_summary() {
    local files_touched="$1"
    local advance_path="$2"
    
    local advance_id
    advance_id=$(get_advance_id "$advance_path")
    
    local file_count
    file_count=$(echo "$files_touched" | wc -l | tr -d ' ')
    
    local score_msg=""
    if has_arrive; then
        local score_json
        score_json=$(arrive_score_json 2>/dev/null) || true
        if [[ -n "$score_json" ]]; then
            local total level
            total=$(get_score_total "$score_json")
            level=$(get_score_level "$score_json")
            score_msg=" | Score: $total [${level^^}]"
        fi
    fi
    
    echo "Session complete: $advance_id | $file_count files touched$score_msg"
}

# ============================================================================
# Main
# ============================================================================

main() {
    # Read input from stdin
    local input=""
    if [[ ! -t 0 ]]; then
        input=$(cat)
    fi
    
    log_debug "stop hook input: $input"

    # If hooks are paused, skip snapshot behavior (fail open) but record activity.
    if hooks_are_paused; then
        append_hook_activity "stop" "paused" '{"reason":"paused"}'
        json_empty
        return 0
    fi
    
    # Check prerequisites
    if ! has_arrive; then
        log_debug "arrive CLI not available, skipping"
        append_hook_activity "stop" "skipped" '{"reason":"arrive_cli_missing"}'
        json_empty
        return 0
    fi
    
    if ! in_git_repo; then
        log_debug "Not in git repo, skipping"
        append_hook_activity "stop" "skipped" '{"reason":"not_in_git_repo"}'
        json_empty
        return 0
    fi
    
    # Get changed files
    local files_touched
    files_touched=$(get_changed_files)
    
    if [[ -z "$files_touched" ]]; then
        log_debug "No files changed, skipping"
        append_hook_activity "stop" "skipped" '{"reason":"no_files_changed"}'
        json_empty
        return 0
    fi
    
    # Ensure we have an advance
    local advance_path
    advance_path=$(ensure_advance) || {
        log_debug "Could not find or create advance"
        append_hook_activity "stop" "error" '{"error":"could not find or create advance"}'
        json_notification "info" "Session ended. No Advance file found - run /arrive-start to track changes."
        return 0
    }
    
    local advance_id
    advance_id=$(get_advance_id "$advance_path")
    
    # Generate summary
    local summary
    summary=$(generate_session_summary "$files_touched" "$advance_path")
    
    log_debug "Session summary: $summary"
    
    # Check final score and add recommendation
    local score_json notification_type="info" recommendation=""
    if has_arrive; then
        score_json=$(arrive_score_json 2>/dev/null) || true
        if [[ -n "$score_json" ]]; then
            local level
            level=$(get_score_level "$score_json")
            
            case "$level" in
                yellow)
                    notification_type="warning"
                    recommendation=" Consider checkpointing before continuing."
                    ;;
                red)
                    notification_type="warning"
                    recommendation=" Recommend splitting changes before committing."
                    ;;
            esac
        fi
    fi
    
    # Platform nudge: complete-but-unpublished advance (ADV-HOOKS-006).
    # Read-only reminder; empty string in every non-nudge case.
    local platform_nudge=""
    if [[ "${ARRIVE_PLATFORM_HOOKS:-1}" != "0" ]]; then
        platform_nudge=$(platform_publish_nudge 2>/dev/null) || platform_nudge=""
    fi

    # Output notification
    append_hook_activity "stop" "ran" "{\"advance_id\":\"$advance_id\"}"
    json_notification "$notification_type" "$summary$recommendation$platform_nudge"
}

# Run main and handle errors gracefully
if ! main "$@" 2>/dev/null; then
    # On any error, return empty response
    json_empty
fi
