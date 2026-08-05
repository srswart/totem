#!/usr/bin/env bash
#
# ARRIVE Hook: afterFileEdit
# Recomputes status and reviewability score after each file edit
#
# Input (stdin): JSON with { "file": "path/to/file", "action": "modified" }
# Output (stdout): JSON with notifications
#

set -uo pipefail

# Load common utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

# ============================================================================
# Main
# ============================================================================

main() {
    # Read input from stdin
    local input=""
    if [[ ! -t 0 ]]; then
        input=$(cat)
    fi
    
    log_debug "afterFileEdit input: $input"

    # If hooks are paused, fail open with empty response but record activity.
    if hooks_are_paused; then
        append_hook_activity "afterFileEdit" "paused" '{"reason":"paused"}'
        json_empty
        return 0
    fi
    
    # Check if arrive CLI is available
    if ! has_arrive; then
        log_debug "arrive CLI not available, skipping"
        json_empty
        return 0
    fi
    
    # Check if we're in a git repo
    if ! in_git_repo; then
        log_debug "Not in git repo, skipping"
        json_empty
        return 0
    fi
    
    # Get score as JSON
    local score_json
    score_json=$(arrive_score_json) || {
        log_debug "Failed to get score"
        append_hook_activity "afterFileEdit" "error" '{"error":"failed to get score"}'
        json_empty
        return 0
    }
    
    # Parse score
    local total level
    total=$(get_score_total "$score_json")
    level=$(get_score_level "$score_json")
    
    log_debug "Score: $total, Level: $level"
    
    # Only notify on yellow/red to avoid noise
    case "$level" in
        yellow)
            local message
            message=$(format_score_message "$total" "$level")
            append_hook_activity "afterFileEdit" "ran" "{\"score_total\":$total,\"score_level\":\"$level\"}"
            json_notification "warning" "$message"
            ;;
        red)
            local message
            message=$(format_score_message "$total" "$level")
            append_hook_activity "afterFileEdit" "ran" "{\"score_total\":$total,\"score_level\":\"$level\"}"
            json_notification "warning" "$message"
            ;;
        *)
            # Green or unknown - no notification to reduce noise
            append_hook_activity "afterFileEdit" "skipped" "{\"score_total\":$total,\"score_level\":\"$level\"}"
            json_empty
            ;;
    esac
}

# Run main and handle errors gracefully
if ! main "$@" 2>/dev/null; then
    # On any error, return empty response (fail open)
    json_empty
fi
