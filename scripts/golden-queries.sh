#!/usr/bin/env bash
#
# Golden queries against a *deployed* Totem (ADV-CORE-008).
#
# The defect this advance fixes was found by asking a running gateway four
# questions and getting the same seven records back in the same order, every
# time. No in-process test could have found it: the corpus tests exercise
# ranking against the deterministic embedder, and the deployment ranks real
# BGE-small-en-v1.5 vectors over a real estate. This script is the only thing
# that asks the deployed system what it actually returns.
#
# It is a *measurement*, and the measurement is not free:
#
#   `recall` reinforces every record it returns — use_count += 1,
#   last_used_at = now, currency = 1.0. Running this changes the ranking
#   inputs of whatever it surfaced. Run it once before the fix and once
#   after, and do not touch the instance in between.
#
# Usage:
#   scripts/golden-queries.sh before      # → evidence/golden-queries-before.txt
#   scripts/golden-queries.sh after
#
# Requires a bearer token at ~/.totem/bootstrap-token (mode 600) and `jq`.

set -euo pipefail

LABEL="${1:?usage: golden-queries.sh <before|after>}"
HOST="${TOTEM_HOST:-https://totem-dev.fly.dev}"
TOKEN_FILE="${TOTEM_TOKEN_FILE:-$HOME/.totem/bootstrap-token}"
ACTOR="${TOTEM_ACTOR:-shawn}"
PROJECT="${TOTEM_PROJECT:-srswart/totem}"
OUT="evidence/golden-queries-${LABEL}.txt"

[ -r "$TOKEN_FILE" ] || { echo "no readable token at $TOKEN_FILE" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
TOKEN="$(cat "$TOKEN_FILE")"

# The query set. Fixed, so before and after are comparable — changing these
# between runs would make the comparison meaningless.
#
# The first two are the ones that exposed the defect: they quote a stored
# record's own body, so an exact match exists and nothing but a broken ranker
# could miss it. The rest are paraphrases, which is what the real embedder is
# *for* — the deterministic embedder cannot answer them at all, so they can
# only ever be asked here.
QUERIES=(
  "The gateway owns the embedded store exclusively; a second process cannot open it."
  "Cursor's remote-MCP reach is untested."
  "Which process is allowed to open the SurrealDB engine?"
  "What do we still not know about running Totem from Cursor?"
  "How should an agent decide what is worth saving to memory?"
  "What went wrong with the Docker build for the real embedder?"
)

mkdir -p "$(dirname "$OUT")"
{
  echo "# Golden queries — ${LABEL}"
  echo "# host:    ${HOST}"
  echo "# taken:   $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "# commit:  $(git rev-parse --short HEAD)"
  echo "# embedder:"
  curl -sS -H "Authorization: Bearer ${TOKEN}" "${HOST}/admin/embedding" \
    | jq -c '{running, uniform, rows_by_model}' | sed 's/^/#   /'
  echo

  for query in "${QUERIES[@]}"; do
    echo "## ${query}"
    jq -n \
      --arg actor "$ACTOR" --arg project "$PROJECT" --arg query "$query" \
      --arg session "golden-${LABEL}" \
      '{actor: $actor, project: $project, query: $query, limit: 7,
        harness: "claude_code", session: $session}' \
    | curl -sS -X POST "${HOST}/recall" \
        -H "Authorization: Bearer ${TOKEN}" \
        -H 'content-type: application/json' \
        --data @- \
    | jq -r '.records[] | "  [\(.category)] \(.content.body[0:110])"'

    # Why that order (ADV-GATEWAY-016). Non-mutating, and it includes the
    # records the gate excluded — which the results above cannot show,
    # because a gated record simply is not there.
    if [ "${EXPLAIN:-1}" = "1" ]; then
      jq -n \
        --arg actor "$ACTOR" --arg project "$PROJECT" --arg query "$query" \
        --arg session "golden-${LABEL}" \
        '{actor: $actor, project: $project, query: $query, limit: 7,
          harness: "claude_code", session: $session}' \
      | curl -sS -X POST "${HOST}/recall/explain" \
          -H "Authorization: Bearer ${TOKEN}" \
          -H 'content-type: application/json' \
          --data @- \
      | jq -r '.candidates[] |
          "    \(if .included then "IN " elif .gated_out then "GATE" else "cut " end) " +
          "combined=\(.combined | .*1000 | round / 1000) " +
          "dist=\(if .distance == null then "-" else (.distance | .*1000 | round / 1000) end) " +
          "rel=\(.relevance | .*1000 | round / 1000) " +
          "val=\(.value | .*1000 | round / 1000) " +
          "cur=\(.currency | .*1000 | round / 1000) " +
          "cat=\(.category_weight | .*1000 | round / 1000)  " +
          "\(.record.content.body[0:60])"'
    fi
    echo
  done
} | tee "$OUT"

echo "written: $OUT" >&2
