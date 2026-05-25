#!/usr/bin/env bash
# Generate a GitHub Actions job summary and artifact bundle for PR review.
set -euo pipefail

summary="${GITHUB_STEP_SUMMARY:-/dev/stdout}"
out_dir="review-artifacts"
mkdir -p "$out_dir"

base="${BASE_SHA:-}"
head="${GITHUB_SHA:-HEAD}"

if [ -z "$base" ] && [ "${GITHUB_EVENT_NAME:-}" = "pull_request" ]; then
    base="$(jq -r '.pull_request.base.sha' "$GITHUB_EVENT_PATH")"
fi

if [ -z "$base" ] && [ -n "${GITHUB_EVENT_PATH:-}" ] && [ -f "$GITHUB_EVENT_PATH" ]; then
    base="$(jq -r '.before // empty' "$GITHUB_EVENT_PATH")"
fi

if [ -z "$base" ] || ! git cat-file -e "$base^{commit}" 2>/dev/null; then
    base="HEAD~1"
fi

git diff --name-only "$base" "$head" > "$out_dir/changed-files.txt"
git diff --stat "$base" "$head" > "$out_dir/diff-stat.txt"
git diff --numstat "$base" "$head" > "$out_dir/numstat.txt"

changed_count="$(wc -l < "$out_dir/changed-files.txt" | tr -d ' ')"
insertions="$(awk '{s += $1} END {print s + 0}' "$out_dir/numstat.txt")"
deletions="$(awk '{s += $2} END {print s + 0}' "$out_dir/numstat.txt")"

count_prefix() {
    local pattern="$1"
    grep -Ec "$pattern" "$out_dir/changed-files.txt" || true
}

rust_count="$(count_prefix '^(src-tauri|crates)/|Cargo\.(toml|lock)$')"
ui_count="$(count_prefix '^ui/')"
workflow_count="$(count_prefix '^\.github/workflows/')"
agent_count="$(count_prefix '^(AGENTS\.md|CLAUDE\.md|BEHAVIOR\.md|CONTEXT\.md|docs/agents/|\.github/copilot-instructions\.md)')"
script_count="$(count_prefix '^scripts/')"

high_attention_re='^(AGENTS\.md|CLAUDE\.md|BEHAVIOR\.md|CONTEXT\.md|Cargo\.toml|src-tauri/src/db/migrations\.rs)$'
migration_re='(^CONTEXT\.md$|migrations|src-tauri/src/db/migrations\.rs)'

high_attention_files="$(grep -E "$high_attention_re" "$out_dir/changed-files.txt" || true)"
migration_files="$(grep -E "$migration_re" "$out_dir/changed-files.txt" || true)"
lock_files="$(grep -E '(^Cargo\.lock$|^ui/package-lock\.json$)' "$out_dir/changed-files.txt" || true)"

while IFS= read -r file; do
    [ -n "$file" ] || continue
    echo "::warning file=$file,title=High-attention file changed::Use focused review and mention the reason in the PR body."
done <<< "$high_attention_files"

while IFS= read -r file; do
    [ -n "$file" ] || continue
    echo "::notice file=$file,title=Migration-sensitive path changed::Check CONTEXT.md migration registry and V-number uniqueness if this PR adds a migration."
done <<< "$migration_files"

while IFS= read -r file; do
    [ -n "$file" ] || continue
    echo "::notice file=$file,title=Dependency lockfile changed::Review dependency and license/security checks before merge."
done <<< "$lock_files"

{
    echo "# Review Digest"
    echo
    echo "- Base: \`$base\`"
    echo "- Head: \`$head\`"
    echo "- Changed files: $changed_count"
    echo "- Insertions: $insertions"
    echo "- Deletions: $deletions"
    echo
    echo "## Area Counts"
    echo
    echo "| Area | Files |"
    echo "| --- | ---: |"
    echo "| Rust / Tauri | $rust_count |"
    echo "| UI | $ui_count |"
    echo "| GitHub workflows | $workflow_count |"
    echo "| Agent / policy docs | $agent_count |"
    echo "| Scripts | $script_count |"
    echo
    echo "## Attention Flags"
    echo
    if [ -n "$high_attention_files" ]; then
        echo "High-attention files changed:"
        echo "$high_attention_files" | sed 's/^/- /'
    else
        echo "- No high-attention policy files changed."
    fi
    if [ -n "$migration_files" ]; then
        echo
        echo "Migration-sensitive files changed:"
        echo "$migration_files" | sed 's/^/- /'
    fi
    if [ -n "$lock_files" ]; then
        echo
        echo "Dependency lockfiles changed:"
        echo "$lock_files" | sed 's/^/- /'
    fi
    echo
    echo "## Diff Stat"
    echo
    echo '```text'
    cat "$out_dir/diff-stat.txt"
    echo '```'
} | tee "$out_dir/review-digest.md" >> "$summary"
