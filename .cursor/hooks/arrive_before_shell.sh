#!/usr/bin/env bash
#
# ARRIVE Hook: beforeShellExecution
# Validates shell commands and checks governance requirements
#
# Input (stdin): JSON with { "command": "git commit -m ..." }
# Output (stdout): JSON with { "decision": "allow|warn|block", "message": "..." }
#

set -uo pipefail

# Load common utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

# ============================================================================
# Command Patterns
# ============================================================================

# Check if command is a git commit
is_git_commit() {
    local cmd="$1"
    [[ "$cmd" =~ ^git[[:space:]]+commit ]]
}

# Check if command is a test command
is_test_command() {
    local cmd="$1"
    [[ "$cmd" =~ ^(cargo[[:space:]]+test|npm[[:space:]]+test|yarn[[:space:]]+test|pytest|go[[:space:]]+test) ]]
}

# Check if command is potentially destructive
is_destructive_command() {
    local cmd="$1"
    [[ "$cmd" =~ ^(rm[[:space:]]+-rf|git[[:space:]]+push[[:space:]]+-f|git[[:space:]]+reset[[:space:]]+--hard|drop[[:space:]]+database) ]]
}

# ============================================================================
# Validation Logic
# ============================================================================

# Check if an Advance file exists for current work
check_advance_exists() {
    local advance
    advance=$(get_current_advance)
    [[ -n "$advance" ]]
}

# Validate git commit
validate_git_commit() {
    # Check if there's a planned advance
    if ! check_advance_exists; then
        json_decision "warn" "No Advance file found. Consider running /arrive-start to create one."
        return
    fi
    
    # Check reviewability score
    if has_arrive; then
        local score_json
        score_json=$(arrive_score_json 2>/dev/null) || {
            json_decision "allow" ""
            return
        }
        
        local level
        level=$(get_score_level "$score_json")
        
        case "$level" in
            red)
                json_decision "warn" "Reviewability score is RED. Consider splitting changes before committing."
                ;;
            yellow)
                json_decision "allow" ""  # Allow but could warn
                ;;
            *)
                json_decision "allow" ""
                ;;
        esac
    else
        json_decision "allow" ""
    fi
}

# Validate test command (for evidence tracking)
validate_test_command() {
    # Allow tests, could log for evidence
    log_debug "Test command detected, could capture for evidence"
    json_decision "allow" ""
}

# Validate potentially destructive command
validate_destructive_command() {
    json_decision "warn" "This appears to be a destructive command. Please confirm before proceeding."
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
    
    log_debug "beforeShellExecution input: $input"
    
    # Parse command from input (Cursor or Claude Code PreToolUse Bash)
    local command
    command=$(hook_shell_command "$input")
    
    if [[ -z "$command" ]]; then
        log_debug "No command in input"
        json_decision "allow" ""
        return 0
    fi
    
    log_debug "Command: $command"

    # If hooks are paused, always allow, but record that we skipped governance checks.
    if hooks_are_paused; then
        append_hook_activity "beforeShellExecution" "paused" '{"reason":"paused"}'
        json_decision "allow" ""
        return 0
    fi
    
    # Check if we're in a git repo (some checks need this)
    if ! in_git_repo; then
        log_debug "Not in git repo"
        json_decision "allow" ""
        return 0
    fi
    
    # Route to appropriate validator
    if is_git_commit "$command"; then
        append_hook_activity "beforeShellExecution" "ran" '{"kind":"git_commit"}'
        validate_git_commit
    elif is_test_command "$command"; then
        append_hook_activity "beforeShellExecution" "ran" '{"kind":"test"}'
        validate_test_command
    elif is_destructive_command "$command"; then
        append_hook_activity "beforeShellExecution" "ran" '{"kind":"destructive"}'
        validate_destructive_command
    else
        # Default: allow
        append_hook_activity "beforeShellExecution" "skipped" '{"kind":"other"}'
        json_decision "allow" ""
    fi
}

# Run main and handle errors gracefully
if ! main "$@" 2>/dev/null; then
    # On any error, allow (fail open)
    json_decision "allow" ""
fi
