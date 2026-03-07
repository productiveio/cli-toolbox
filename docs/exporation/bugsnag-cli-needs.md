# Bugsnag CLI Needs Analysis

Analysis of Bugsnag MCP tool friction when used by Claude Code, primarily in the peglanje (production health) workflow. The `bsi` CLI design at `~/code/rust/bugsnag-insights/docs/cli-design.md` already addresses many of these — this document captures the full picture from actual usage.

## Current MCP Tools Used

| Tool | Where | What For |
|------|-------|----------|
| `mcp__smartbear__bugsnag_list_project_errors` | peglanje Phase 3 | List recent unresolved errors |
| `mcp__smartbear__bugsnag_get_error` | peglanje Phase 3 | Get error details by ID |
| `mcp__smartbear__bugsnag_get_event` | peglanje Phase 3 | Get latest event with stack trace |
| `mcp__smartbear__bugsnag_list_projects` | ad-hoc | Find project IDs |

Fallback: `mcp__bugsnag__search_issues` when `list_errors` returns 400.

## Pain Points

### 1. N+1 round trips burn tokens and context

A typical peglanje Bugsnag analysis requires:
- 1 call to list errors (get ~20-50 error summaries)
- 3-5 calls to `get_error` for top errors by frequency
- 3-5 calls to `get_event` for latest event detail (stack traces, breadcrumbs)

That's **7-11 MCP tool calls** just for Bugsnag. Each call adds ~500-2000 tokens of overhead (tool invocation + JSON response + LLM reasoning about results). Total Bugsnag cost: **5,000-15,000 tokens** of context.

**CLI equivalent:** `bsi errors --status open --since 2026-03-05 --sort events` → one call, ~200 tokens of compact output.

### 2. No date filtering — must fetch all and filter manually

MCP `list_errors` returns all recent errors with no date filter. The agent must scan all results and mentally filter to "errors that occurred on 2026-03-05". This wastes tokens on irrelevant errors and requires the LLM to do date comparison.

**CLI need:** `--since <date>` and `--until <date>` flags on `errors` command. Already in bsi design.

### 3. No environment filtering at query time

Peglanje needs to distinguish production errors from latest/endtoend test noise. MCP returns everything — the agent must filter by `release_stages` field in the response.

**CLI need:** `--stage production` filter. Not explicitly in bsi design — **consider adding `--stage <s>` to `bsi errors`**.

### 4. Massive JSON responses consume context

A single `get_event` response includes full stack traces, breadcrumbs (50+), device info, metadata objects, request headers, and feature flags. A typical event is 3,000-8,000 tokens in the MCP response. Most of this is noise for the peglanje use case (we need: error class, message, frequency, affected users, release stage).

**CLI need:** Compact default output with `--long` for detail. `bsi errors` table format gives ~20 tokens per error vs 3,000+ per MCP event fetch. The `bsi fetch event <id>` command should have `--short` or default to a summary.

### 5. Two MCP namespaces cause confusion

There are both `mcp__bugsnag__*` and `mcp__smartbear__bugsnag_*` tools available. They overlap in functionality but have different parameter names and response formats. The peglanje skill has to document fallback strategies (`list_errors` → `search_issues`).

**CLI need:** Single `bsi` binary, one interface. Already solved by design.

### 6. No aggregation or frequency data

MCP gives individual errors but not "how many events did this error produce on date X?" or "what's the event trend for this error?". The agent must infer frequency from the `events` count field (which is cumulative, not per-day).

**CLI need:** `bsi trends --error <id> --resolution 1d` and per-error event counts with date breakdowns. Already in bsi design.

### 7. No "dashboard" view

Peglanje needs a quick health snapshot: how many open errors, top errors by impact, any new errors since last check. This requires composing 3+ MCP calls.

**CLI need:** `bsi report dashboard`. Already in bsi design.

### 8. Error recurrence detection is manual

Peglanje tracks `bugsnag_ids` across daily reports to detect recurring vs new errors. With MCP, there's no way to check "has this error appeared in previous days?" without loading old reports.

**CLI need:** `bsi errors --first-seen-after <date>` to identify genuinely new errors vs recurring ones. Consider `--new` flag that filters to errors first seen within the query period.

## What bsi Design Already Covers

Most of the above is addressed in the existing `bsi` CLI design:

| Pain Point | bsi Command |
|-----------|-------------|
| N+1 round trips | `bsi errors` (single call, compact output) |
| No date filtering | `--since` flag |
| Massive JSON | Human-readable tables by default |
| Two namespaces | Single `bsi` binary |
| No aggregation | `bsi trends` |
| No dashboard | `bsi report dashboard` |

## Gaps in Current bsi Design

These are NOT yet in the bsi design and would specifically help peglanje:

### 1. Environment/stage filtering on errors

```
bsi errors --stage production --since 2026-03-05
```

The `--stage` flag is on `releases` and `stability` but not on `errors`. Peglanje always needs to filter out test/endtoend noise.

### 2. "New errors since date" filter

```
bsi errors --first-seen-after 2026-03-04
```

Distinct from `--since` (last seen after). This identifies genuinely new error classes vs recurring ones — critical for daily health reports.

### 3. Event count per time period

```
bsi errors --since 2026-03-05 --until 2026-03-06 --sort events-in-period
```

Currently `events` is cumulative all-time. For daily reports, we need "how many events did this error produce *today*?"

### 4. Compact event summary

```
bsi fetch event <id> --short
```

Returns: error class, message, first 3 stack frames, release stage, user email. No breadcrumbs, no full metadata. ~50 tokens instead of 3,000.

### 5. Prime command for peglanje context

```
bsi prime --since 2026-03-05
```

AI-optimized dump: open error count, top 5 errors by today's events, any new errors, stability trend. Designed for ~200 tokens. This replaces 7-11 MCP calls with 1 CLI call.

## Token Impact Estimate

| Approach | Calls | Context Tokens |
|----------|-------|---------------|
| MCP (current) | 7-11 tool calls | 10,000-15,000 |
| bsi (with gaps filled) | 1-2 CLI calls | 500-1,000 |

Estimated savings: **~90% context reduction** for the Bugsnag analysis phase.
