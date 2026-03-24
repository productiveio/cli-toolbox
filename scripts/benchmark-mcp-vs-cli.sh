#!/usr/bin/env bash
#
# Benchmark: MCP tools vs CLI skills for the same task
#
# Usage: ./scripts/benchmark-mcp-vs-cli.sh [task_name]
#
# Runs two claude -p sessions with the same goal:
#   1. MCP-only: disables skills, forces MCP tool usage
#   2. CLI-only: disables MCP productive tools, uses CLI skill
#
# Then parses the session JSONL logs, compares token usage + wall-clock time,
# and runs an LLM-as-a-judge to evaluate output quality.

set -euo pipefail

TASK_NAME="${1:-my_tasks}"
RESULTS_DIR="./benchmark-results"
mkdir -p "$RESULTS_DIR"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
RUN_PREFIX="$RESULTS_DIR/${TIMESTAMP}-${TASK_NAME}"

# ── Task definitions ─────────────────────────────────────────────────────────

case "$TASK_NAME" in
  my_tasks)
    DESCRIPTION="List my Productive tasks assigned to me"
    MCP_PROMPT="List all Productive.io tasks assigned to me. Show task title, status, and project. Do NOT use any CLI skills or Bash commands — only use MCP tools (mcp__plugin_p-mcp-productive_productive__*)."
    PRIME_CONTEXT=$(cd /tmp && tb-prod prime 2>&1)
    CLI_PROMPT="List all Productive.io tasks assigned to me. IMPORTANT: Run all tb-prod commands from /tmp (e.g. 'cd /tmp && tb-prod ...'). Output defaults to CSV with resolved relationship names. Default filters auto-apply. Show task title, status, and project. Do NOT use any MCP tools — only Bash.

Here is your tb-prod context (pre-loaded):

$PRIME_CONTEXT"
    MCP_EXTRA_FLAGS=(--disable-slash-commands)
    CLI_EXTRA_FLAGS=(--disallowed-tools "mcp__plugin_p-mcp-productive_productive__query_resources,mcp__plugin_p-mcp-productive_productive__describe_resource,mcp__plugin_p-mcp-productive_productive__load_resource_details,mcp__plugin_p-mcp-productive_productive__search,mcp__plugin_p-mcp-productive_productive__search_resource,mcp__plugin_p-mcp-productive_productive__create_resource,mcp__plugin_p-mcp-productive_productive__update_resource,mcp__plugin_p-mcp-productive_productive__delete_resource,mcp__plugin_p-mcp-productive_productive__perform_resource_action,mcp__plugin_p-mcp-productive_productive__search_organization_data,mcp__plugin_p-mcp-productive_productive__search_project_data,mcp__plugin_p-mcp-productive_productive__load_current_context,mcp__plugin_p-mcp-productive_productive__describe_report,mcp__plugin_p-mcp-productive_productive__query_report,mcp__plugin_p-mcp-productive_productive__load_guides,mcp__plugin_p-mcp-productive_productive__search_guides,mcp__plugin_p-mcp-productive_productive__describe_skill,mcp__plugin_p-mcp-productive_productive__perform_skill_action,mcp__plugin_p-mcp-productive_productive__get_supported_currencies,mcp__plugin_p-mcp-productive_productive__read_file_from_url")
    ;;
  my_projects)
    DESCRIPTION="List my Productive projects"
    MCP_PROMPT="List my Productive.io projects. Show project name and status. Do NOT use any CLI skills or Bash commands — only use MCP tools (mcp__plugin_p-mcp-productive_productive__*)."
    CLI_PROMPT="List my Productive.io projects. IMPORTANT: Run all tb-prod commands from /tmp (e.g. 'cd /tmp && tb-prod ...'). First run 'cd /tmp && tb-prod prime' to get your user context and command reference, then query projects with --format table. Show project name and status. Do NOT use any MCP tools — only Bash."
    MCP_EXTRA_FLAGS=(--disable-slash-commands)
    CLI_EXTRA_FLAGS=(--disallowed-tools "mcp__plugin_p-mcp-productive_productive__query_resources,mcp__plugin_p-mcp-productive_productive__describe_resource,mcp__plugin_p-mcp-productive_productive__load_resource_details,mcp__plugin_p-mcp-productive_productive__search,mcp__plugin_p-mcp-productive_productive__search_resource,mcp__plugin_p-mcp-productive_productive__create_resource,mcp__plugin_p-mcp-productive_productive__update_resource,mcp__plugin_p-mcp-productive_productive__delete_resource,mcp__plugin_p-mcp-productive_productive__perform_resource_action,mcp__plugin_p-mcp-productive_productive__search_organization_data,mcp__plugin_p-mcp-productive_productive__search_project_data,mcp__plugin_p-mcp-productive_productive__load_current_context,mcp__plugin_p-mcp-productive_productive__describe_report,mcp__plugin_p-mcp-productive_productive__query_report,mcp__plugin_p-mcp-productive_productive__load_guides,mcp__plugin_p-mcp-productive_productive__search_guides,mcp__plugin_p-mcp-productive_productive__describe_skill,mcp__plugin_p-mcp-productive_productive__perform_skill_action,mcp__plugin_p-mcp-productive_productive__get_supported_currencies,mcp__plugin_p-mcp-productive_productive__read_file_from_url")
    ;;
  *)
    echo "Unknown task: $TASK_NAME"
    echo "Available tasks: my_tasks, my_projects"
    exit 1
    ;;
esac

# ── Session runner ───────────────────────────────────────────────────────────

run_session() {
  local label="$1"
  local prompt="$2"
  shift 2
  local extra_flags=("$@")
  local outfile="${RUN_PREFIX}-${label}"
  local session_id
  session_id=$(python3 -c "import uuid; print(uuid.uuid4())")

  echo "━━━ Running: $label (session: $session_id) ━━━"
  echo "Prompt: $prompt"
  echo ""

  local start_time
  start_time=$(python3 -c "import time; print(time.time())")
  echo "$start_time" > "${outfile}.start"

  # Build command
  local cmd=(
    env -u CLAUDECODE
    claude -p "$prompt"
    --output-format stream-json
    --verbose
    --session-id "$session_id"
    --max-budget-usd 1.00
    --no-chrome
    --permission-mode bypassPermissions
  )
  cmd+=("${extra_flags[@]}")

  # Run and capture output
  "${cmd[@]}" > "${outfile}.stream.jsonl" 2>"${outfile}.stderr.log" || true

  local end_time
  end_time=$(python3 -c "import time; print(time.time())")
  echo "$end_time" > "${outfile}.end"
  echo "$session_id" > "${outfile}.session_id"

  local elapsed
  elapsed=$(python3 -c "print(f'{$end_time - $start_time:.1f}')")
  echo "  ⏱  Wall time: ${elapsed}s"

  # Check for errors
  if [[ -s "${outfile}.stderr.log" ]]; then
    echo "  ⚠  Stderr output:"
    head -3 "${outfile}.stderr.log" | sed 's/^/    /'
  fi

  # Extract the text output for judge comparison
  python3 - "${outfile}" << 'PYEOF'
import json, sys

prefix = sys.argv[1]
texts = []

with open(f"{prefix}.stream.jsonl") as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue

        # Extract text content from assistant messages
        if msg.get("type") == "assistant":
            content = msg.get("message", {}).get("content", [])
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict) and block.get("type") == "text":
                        texts.append(block["text"])
            elif isinstance(content, str):
                texts.append(content)

        # Also check top-level content
        if "content" in msg and isinstance(msg.get("content"), list):
            for block in msg["content"]:
                if isinstance(block, dict) and block.get("type") == "text":
                    texts.append(block["text"])

with open(f"{prefix}.output.txt", "w") as f:
    f.write("\n".join(texts))
PYEOF

  echo ""
}

# ── Analyzer ─────────────────────────────────────────────────────────────────

analyze_session() {
  local label="$1"
  local outfile="${RUN_PREFIX}-${label}"

  python3 - "${outfile}" << 'PYEOF'
import json, sys

prefix = sys.argv[1]

# Parse timing
with open(f"{prefix}.start") as f:
    start = float(f.read().strip())
with open(f"{prefix}.end") as f:
    end = float(f.read().strip())
wall_time = end - start

# Parse stream-json for usage data
total_input = 0
total_output = 0
total_cache_create = 0
total_cache_read = 0
tool_calls = []
api_calls = 0

with open(f"{prefix}.stream.jsonl") as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue

        # stream-json emits usage at various levels
        # Check nested message.usage (most common in stream-json)
        message = msg.get("message", {})
        if isinstance(message, dict) and "usage" in message:
            u = message["usage"]
            total_input += u.get("input_tokens", 0)
            total_output += u.get("output_tokens", 0)
            total_cache_create += u.get("cache_creation_input_tokens", 0)
            total_cache_read += u.get("cache_read_input_tokens", 0)
            api_calls += 1

        # Check top-level usage (some message types)
        elif "usage" in msg and "message" not in msg:
            u = msg["usage"]
            total_input += u.get("input_tokens", 0)
            total_output += u.get("output_tokens", 0)
            total_cache_create += u.get("cache_creation_input_tokens", 0)
            total_cache_read += u.get("cache_read_input_tokens", 0)
            api_calls += 1

        # Count tool uses from assistant messages
        content = []
        if isinstance(message, dict) and "content" in message:
            content = message.get("content", [])
        elif "content" in msg and isinstance(msg.get("content"), list):
            content = msg["content"]

        if isinstance(content, list):
            for block in content:
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    tool_calls.append(block.get("name", "unknown"))

total_tokens = total_input + total_output + total_cache_create + total_cache_read

result = {
    "wall_time_s": round(wall_time, 1),
    "total_tokens": total_tokens,
    "input_tokens": total_input,
    "output_tokens": total_output,
    "cache_creation_tokens": total_cache_create,
    "cache_read_tokens": total_cache_read,
    "tool_calls": tool_calls,
    "tool_call_count": len(tool_calls),
    "api_calls": api_calls,
}

with open(f"{prefix}.results.json", "w") as f:
    json.dump(result, f, indent=2)

print(json.dumps(result, indent=2))
PYEOF
}

# ── Compare (table) ──────────────────────────────────────────────────────────

compare_results() {
  python3 - "${RUN_PREFIX}" "$DESCRIPTION" << 'PYEOF'
import json, sys

prefix = sys.argv[1]
description = sys.argv[2]

with open(f"{prefix}-mcp.results.json") as f:
    mcp = json.load(f)
with open(f"{prefix}-cli.results.json") as f:
    cli = json.load(f)

def pct_diff(a, b):
    if b == 0:
        return "N/A"
    diff = ((a - b) / b) * 100
    return f"{diff:+.0f}%"

print()
print("=" * 70)
print(f"  BENCHMARK: {description}")
print("=" * 70)
print()
print(f"{'Metric':<30} {'MCP':>12} {'CLI':>12} {'Diff':>10}")
print("-" * 70)
print(f"{'Wall time (s)':<30} {mcp['wall_time_s']:>12.1f} {cli['wall_time_s']:>12.1f} {pct_diff(mcp['wall_time_s'], cli['wall_time_s']):>10}")
print(f"{'Total tokens':<30} {mcp['total_tokens']:>12,} {cli['total_tokens']:>12,} {pct_diff(mcp['total_tokens'], cli['total_tokens']):>10}")
print(f"{'  Input tokens':<30} {mcp['input_tokens']:>12,} {cli['input_tokens']:>12,} {pct_diff(mcp['input_tokens'], cli['input_tokens']):>10}")
print(f"{'  Output tokens':<30} {mcp['output_tokens']:>12,} {cli['output_tokens']:>12,} {pct_diff(mcp['output_tokens'], cli['output_tokens']):>10}")
print(f"{'  Cache creation':<30} {mcp['cache_creation_tokens']:>12,} {cli['cache_creation_tokens']:>12,} {pct_diff(mcp['cache_creation_tokens'], cli['cache_creation_tokens']):>10}")
print(f"{'  Cache read':<30} {mcp['cache_read_tokens']:>12,} {cli['cache_read_tokens']:>12,} {pct_diff(mcp['cache_read_tokens'], cli['cache_read_tokens']):>10}")
print(f"{'API calls':<30} {mcp['api_calls']:>12} {cli['api_calls']:>12} {pct_diff(mcp['api_calls'], cli['api_calls']):>10}")
print(f"{'Tool calls':<30} {mcp['tool_call_count']:>12} {cli['tool_call_count']:>12} {pct_diff(mcp['tool_call_count'], cli['tool_call_count']):>10}")
print("-" * 70)
print()
print("MCP tools used:", ", ".join(mcp['tool_calls']) if mcp['tool_calls'] else "(none detected)")
print("CLI tools used:", ", ".join(cli['tool_calls']) if cli['tool_calls'] else "(none detected)")
print()

with open(f"{prefix}-comparison.json", "w") as f:
    json.dump({
        "description": description,
        "mcp": mcp,
        "cli": cli,
        "token_diff_pct": pct_diff(mcp['total_tokens'], cli['total_tokens']),
        "time_diff_pct": pct_diff(mcp['wall_time_s'], cli['wall_time_s']),
    }, f, indent=2)
PYEOF
}

# ── LLM-as-a-Judge ──────────────────────────────────────────────────────────

judge_outputs() {
  echo ""
  echo "━━━ LLM-as-a-Judge: comparing output quality ━━━"
  echo ""

  local mcp_output cli_output
  mcp_output=$(cat "${RUN_PREFIX}-mcp.output.txt" 2>/dev/null || echo "(no output)")
  cli_output=$(cat "${RUN_PREFIX}-cli.output.txt" 2>/dev/null || echo "(no output)")

  if [[ "$mcp_output" == "(no output)" ]] && [[ "$cli_output" == "(no output)" ]]; then
    echo "  Both sessions produced no output — skipping judge."
    return
  fi

  local judge_prompt
  judge_prompt=$(cat << JUDGEOF
You are evaluating two AI assistant outputs that attempted the same task:
"$DESCRIPTION"

Rate each output on these criteria (1-5 scale):
1. **Completeness**: Did it fully answer the question?
2. **Accuracy**: Is the information correct and well-structured?
3. **Conciseness**: Is it appropriately concise without losing important info?

Then give an overall verdict: which output is better, or are they equivalent?

=== OUTPUT A (MCP tools) ===
$mcp_output

=== OUTPUT B (CLI skill) ===
$cli_output

Respond in this exact JSON format:
{
  "output_a": {"completeness": N, "accuracy": N, "conciseness": N, "notes": "..."},
  "output_b": {"completeness": N, "accuracy": N, "conciseness": N, "notes": "..."},
  "verdict": "a_better" | "b_better" | "equivalent",
  "reasoning": "..."
}
JUDGEOF
)

  # Use plain text output (not json — that returns metadata envelope, not the response)
  # and bump budget since cache creation alone can cost >$0.10
  local judge_raw
  judge_raw=$(env -u CLAUDECODE claude -p "$judge_prompt" \
    --model sonnet \
    --max-budget-usd 0.50 \
    --no-chrome \
    --no-session-persistence \
    2>/dev/null || echo "JUDGE_ERROR")

  echo "$judge_raw" > "${RUN_PREFIX}-judge.raw.txt"

  # Pretty-print the verdict
  python3 - "${RUN_PREFIX}" << 'PYEOF'
import json, sys, re

prefix = sys.argv[1]
try:
    with open(f"{prefix}-judge.raw.txt") as f:
        raw = f.read().strip()

    if raw == "JUDGE_ERROR" or not raw:
        print("  Judge failed to produce output.")
        sys.exit(0)

    # Extract JSON from the text response (may have markdown fences or preamble)
    # Try to find a JSON block
    match = re.search(r'```(?:json)?\s*(\{[\s\S]*?\})\s*```', raw)
    if match:
        data = json.loads(match.group(1))
    else:
        # Try raw JSON extraction
        match = re.search(r'\{[\s\S]*\}', raw)
        if match:
            data = json.loads(match.group())
        else:
            print(f"  Could not find JSON in judge output.")
            print(f"  Raw output:\n{raw[:500]}")
            sys.exit(0)

    # Save parsed judge result
    with open(f"{prefix}-judge.json", "w") as f:
        json.dump(data, f, indent=2)

    a = data.get("output_a", {})
    b = data.get("output_b", {})

    print(f"  {'Criterion':<20} {'MCP (A)':>10} {'CLI (B)':>10}")
    print(f"  {'-'*40}")
    for key in ["completeness", "accuracy", "conciseness"]:
        print(f"  {key.capitalize():<20} {a.get(key, '?'):>10} {b.get(key, '?'):>10}")

    verdict_map = {"a_better": "MCP is better", "b_better": "CLI is better", "equivalent": "Equivalent"}
    verdict = data.get("verdict", "unknown")
    print(f"\n  Verdict: {verdict_map.get(verdict, verdict)}")
    print(f"  Reasoning: {data.get('reasoning', 'N/A')}")

    if a.get("notes"):
        print(f"\n  MCP notes: {a['notes']}")
    if b.get("notes"):
        print(f"  CLI notes: {b['notes']}")

except Exception as e:
    print(f"  Error parsing judge results: {e}")
PYEOF

  echo ""
}

# ── Main ─────────────────────────────────────────────────────────────────────

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  MCP vs CLI Benchmark: $TASK_NAME"
echo "║  $DESCRIPTION"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Run MCP variant
run_session "mcp" "$MCP_PROMPT" "${MCP_EXTRA_FLAGS[@]}"

# Small gap to help separate cache effects
sleep 3

# Run CLI variant
run_session "cli" "$CLI_PROMPT" "${CLI_EXTRA_FLAGS[@]}"

# Analyze
echo ""
echo "━━━ Analyzing results ━━━"
echo ""
echo "--- MCP ---"
analyze_session "mcp"
echo ""
echo "--- CLI ---"
analyze_session "cli"

# Compare
compare_results

# LLM-as-a-Judge
judge_outputs

echo ""
echo "Results saved to: ${RUN_PREFIX}-*"
