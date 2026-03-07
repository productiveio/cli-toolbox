# Productive CLI Needs Analysis

Analysis of Productive MCP tool friction when used by Claude Code. `papi` already handles read queries well — this focuses on **gaps** that still force peglanje and other workflows to fall back to MCP tools.

## Current MCP Tools Still Used (Despite papi)

| Tool | Where | What For | Why Not papi |
|------|-------|----------|-------------|
| `mcp__productive__query_tasks` | peglanje Phase 2 | Query open tasks in a specific task list | papi has no task list filter |
| `mcp__productive__search` | peglanje Phase 2 | Search ALL statuses (open + closed) | papi only queries open/started tasks |
| `mcp__productive__filter_tasks_by_prompt` | peglanje Phase 2 | Semantic search fallback | N/A — LLM-based, not replaceable by CLI |
| `mcp__productive__create_tasks` | peglanje Phase 3 | Create tasks for findings | papi is read-only |
| `mcp__productive__create_todos` | remove-styles-helper | Create PR link todos | papi is read-only |

## Pain Points

### 1. Duplicate detection requires 2+ MCP calls per finding

Peglanje's report mode checks each finding against existing tasks to avoid duplicates. For each finding, it needs:
- `query_tasks` with `task_list_id` → open tasks only
- `search` with `status: "all"` → catches closed/discarded tasks (regression detection)

With 7 findings, that's **14+ MCP calls** just for duplicate detection. Each call returns JSONAPI responses with nested relationships — verbose, ~1,000-3,000 tokens per response.

**CLI need:**
```
papi tasks --task-list 2418986 --category all --json
```
Single call returning all tasks (open + closed) in a task list. ~200 tokens of compact output.

### 2. Search across all task statuses is unreliable

`mcp__productive__search` is a generic search endpoint — it searches by keyword across all resource types. Results are often noisy (matches in descriptions, comments, unrelated projects). The agent must filter results manually.

`mcp__productive__filter_tasks_by_prompt` uses an LLM to interpret the query — unpredictable, sometimes misses exact matches, and adds latency.

**CLI need:**
```
papi tasks --task-list 2418986 --category all --search "context-window-exceeded"
papi tasks --project 746128 --category closed --search "[MCP] JWT"
```
Deterministic text search within task titles/descriptions, scoped to a project or task list, across all statuses.

### 3. papi is read-only — no task creation

Peglanje creates 3-7 tasks per report run. Each `create_tasks` MCP call requires a full JSONAPI payload (title, project_id, task_list_id, workflow_status_id, HTML description). The response is a large JSONAPI document (~2,000 tokens) when all we need is the task ID and number.

**CLI need:**
```
papi task create \
  --title "[ReportBuilder] Report uses wrong time breakdown columns" \
  --project 746128 \
  --task-list 2418986 \
  --status 48472 \
  --description-file /tmp/finding-desc.html
```
Returns: task ID, task number, URL. ~30 tokens instead of 2,000.

The `--description-file` flag avoids passing large HTML blobs as CLI arguments. Alternative: `--description-stdin` to pipe HTML.

### 4. No task list filtering in papi

papi's `tasks` command filters by person, project, status category, and workflow status name. But peglanje needs to query tasks **by task list** — a specific board column used to track findings.

**CLI need:**
```
papi tasks --task-list 2418986
papi tasks --task-list "Peglanje Triage"  # by name
```

### 5. JSONAPI verbosity wastes context

MCP responses use JSONAPI format with `data`, `attributes`, `relationships`, and `included` arrays. A task with 5 relationships might be 2,000 tokens. The agent extracts ~5 fields: id, title, status, number, permalink.

papi already solves this for reads. But MCP responses for writes (create/update) have the same problem.

**CLI need:** Already handled by papi's compact output format. Extend this to write operations — return only essential fields after creation.

### 6. No task update capability

Peglanje doesn't currently update tasks, but the natural next step is closing findings that are resolved, or adding comments with new evidence. Currently requires MCP's `update_tasks` or `create_comment`.

**CLI need:**
```
papi task update 16618729 --status "In Progress"
papi task comment 16618729 "New evidence from 2026-03-06 report: ..."
```

### 7. Batch operations are sequential

Creating 5 tasks requires 5 sequential MCP calls (each waits for the task ID to log it). Each round trip adds ~3-5 seconds of latency plus token overhead.

**CLI need:**
```
papi task create --batch /tmp/tasks.json
```
Where `tasks.json` contains an array of task payloads. Returns all created task IDs/numbers/URLs in one response.

## What papi Already Covers Well

| Capability | papi Command | Notes |
|-----------|-------------|-------|
| Task details | `papi task <id>` | Subtasks, todos, comments, deps — compact |
| My tasks | `papi tasks` | Filters by person, project, status |
| Team tasks | `papi tasks "john"` | Person-scoped query |
| Project tasks | `papi tasks --project "AI Agent"` | Project-scoped |
| People lookup | `papi people "john"` | Fast, cached |
| Availability | `papi availability` | Team/org-wide |
| Cache | `papi cache sync` | Keeps lookups fast |

## Gaps to Fill (Priority Order)

### P0 — Would eliminate all remaining MCP usage in peglanje

1. **Task list filter**: `papi tasks --task-list <id-or-name>` — query tasks scoped to a task list
2. **All-status query**: `papi tasks --category all` (currently only: not-started, started, closed, open) — include closed/discarded tasks
3. **Task creation**: `papi task create --title ... --project ... --task-list ... --status ... --description-file ...`

### P1 — Would improve quality and reduce tokens further

4. **Text search in tasks**: `papi tasks --search "keyword"` — substring match in title/description
5. **Task update**: `papi task update <id> --status "Done"` — change workflow status
6. **Task comment**: `papi task comment <id> "message"` — add a comment

### P2 — Nice to have

7. **Batch create**: `papi task create --batch <file>` — create multiple tasks in one call
8. **Task URL output**: After create, print `https://app.productive.io/109-productive/tasks/<id>` (agent currently constructs this manually)
9. **Dry-run mode**: `papi task create --dry-run` — validate payload without creating

## Token Impact Estimate

### Peglanje Report Mode (duplicate detection + task creation for 7 findings)

| Approach | Calls | Context Tokens |
|----------|-------|---------------|
| MCP (current) | ~17 calls (2 per finding for dedup + 3 creates) | 25,000-40,000 |
| papi (with P0 gaps filled) | 1 list + 3 creates = 4 calls | 1,500-2,500 |

Estimated savings: **~90% context reduction** for the Productive integration phase.

### Full Peglanje Report Mode (including Bugsnag — see bugsnag-cli-needs.md)

| Phase | MCP Tokens | CLI Tokens | Savings |
|-------|-----------|-----------|---------|
| Bugsnag analysis | 10,000-15,000 | 500-1,000 | ~93% |
| Productive dedup + create | 25,000-40,000 | 1,500-2,500 | ~93% |
| **Total** | **35,000-55,000** | **2,000-3,500** | **~93%** |

This is why the session ran out of context — MCP overhead dominated the token budget.
