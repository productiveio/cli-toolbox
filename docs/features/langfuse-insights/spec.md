# tb-lf — DevPortal AI Insights CLI

**Status:** Ready
**Last updated:** 2026-03-07

## Summary

`tb-lf` is a read-only Rust CLI that queries devportal's `/spa_api/ai/*` endpoints to explore, debug, and monitor AI agent behavior. It provides fast terminal access to traces, sessions, observations, scores, eval runs, triage queues, and dashboards — with output designed for both humans and LLM consumption.

## Requirements

### R1: Configuration

- Load config from `secrets.toml` `[devportal]` section (CWD), falling back to `~/.config/tb-lf/config.toml`
- Config fields: `url` (devportal base URL), `token` (Bearer token), `project` (optional default project name)
- Environment variable overrides: `DEVPORTAL_URL`, `DEVPORTAL_TOKEN`
- `config show` displays config with masked token
- `config set <key> <value>` updates individual values in the standalone config file
- `doctor` verifies config + API connectivity

### R2: Project resolution

- `--project` flag accepts project name (e.g., `production`) or numeric ID (e.g., `2`)
- Project name → ID resolution via `/projects` endpoint, cached for 1 hour
- `prime` command outputs available projects with their IDs
- If no `--project` flag and no default configured, error with list of available projects

### R3: Core data access

Each entity command follows the pattern: list view (table), single-item detail, `--json` output.

#### traces

- List traces with filters: `--name`, `--user`, `--session`, `--env`, `--triage` (flagged/dismissed/untouched), `--satisfaction` (down), `--sort` (timestamp/cost_usd/latency_ms), `--limit` (default 20), `--page`
- Time range: `--since`, `--from`, `--to`
- Display: langfuse_id, display_name (or name), cost, latency, relative time, triage status, user query preview
- `--stats` flag: call `/traces/stats` instead, show total_traces, total_cost, avg/max duration
- Navigation hint: "Run `tb-lf trace <id>` for full details"
- Endpoint: `GET /traces`

#### trace \<id\>

- Fetch full trace via Langfuse proxy: `GET /langfuse/traces/:id`
- Requires `--project` (proxy needs project credentials)
- Default: formatted key fields (input, output, metadata, tags, scores, latency, cost)
- `--full` flag: complete JSON dump
- `--observations` flag: also fetch and display observations for this trace
- Cache: Long TTL (1 hour) — trace data is immutable once synced

#### sessions

- List sessions with filters: `--user`, `--env`, `--satisfaction`, `--sort` (last_trace_at/trace_count/total_cost_usd/duration), `--limit`
- Time range: `--since`, `--from`, `--to`
- Display: session_id, trace_count, cost, last activity, user IDs
- `--stats` flag: call `/sessions/stats`
- Endpoint: `GET /sessions`

#### session \<id\>

- Show all traces in a session: `GET /sessions/:id`
- Display: ordered trace list with name, cost, latency
- Navigation hint: "Run `tb-lf trace <id>` to inspect a trace"

#### observations

- List observations with filters: `--trace` (required or filter), `--type`, `--model`, `--level`, `--env`
- Display: name, type, model, tokens, cost, latency
- Endpoint: `GET /observations`

#### observation \<id\>

- Fetch full observation via Langfuse proxy: `GET /langfuse/observations/:id`
- Requires `--project`
- Cache: Long TTL (1 hour)

#### scores

- List scores with filters: `--trace`, `--name`, `--source` (EVAL/API/ANNOTATION), `--env`
- Display: name, value (threshold-colored: green ≥0.80, yellow ≥0.50, red <0.50), source, trace_id, comment preview
- Endpoint: `GET /scores`

#### comments

- List comments with filters: `--trace`, `--type` (object_type), `--object` (object_id)
- Display: content, author, object type
- Endpoint: `GET /comments`

### R4: Search

- `search <query>` searches traces by matching against name, user_id, tags, and user_query content
- Backed by a **new devportal endpoint**: `GET /spa_api/ai/search`
- Request: `?q=<query>&project_id=...&from=...&to=...&per_page=...&page=...`
- Server-side: searches across trace `name`, `user_id`, `tags` (array contains), `user_query` (ILIKE), `agent_response` (ILIKE)
- Response: same as traces index, but with `match_type` (name/user/tag/input/output) and `match_context` (the matched snippet) added per result
- Flags: `--ids-only` (output just langfuse IDs, one per line, for piping), `--limit`, time range
- If devportal search endpoint not yet implemented, fall back to `GET /traces` with `--name` filter and note the limitation

### R5: Tags discovery

- `tags` command lists distinct trace names with occurrence counts
- Endpoint: `GET /traces/names` (returns name strings)
- For counts: fetch `/traces/stats` per name, or accept name-only list for v1
- Time range filters

### R6: Dashboard & reports

#### dashboard

- Single overview command: `GET /dashboard`
- Display sections: KPI (sessions, users, avg cost, satisfaction, latency p50), adoption (retention, power users), feedback (positive/negative), latency (p50/p95/p99), evaluations (recent runs with scores)
- Period comparison: show current vs previous with % change (green/red colored)
- `--from`, `--to` override default 30-day window
- Navigation hints to relevant drill-down commands

#### daily \[date\]

- View AI-generated daily report: `GET /reports/:date`
- Default: latest available date
- Display: summary, metrics, findings (with severity coloring)
- `--findings` flag: show only findings, filterable by `--severity` and `--type`
- If no report exists for the date, say so and suggest available date range

#### metrics

- Daily metrics time series: `GET /daily_metrics`
- Filters: `--env`, time range
- Display: table with date, traces, users, cost, latency, errors
- `--days <n>` shorthand for `--since <n>d`

### R7: Triage queue

#### queue

- List triage queue items: `GET /queue_items`
- Filters: `--status` (pending_review/confirmed/dismissed), `--category` (bug/feature_request/unknown), `--confidence` (high/medium/low), `--run` (triage_run_id), `--feature` (feature_id)
- Display: trace_id, status, ai_category, ai_confidence, ai_reasoning preview, category, reviewed_by
- `--limit`, `--full` (show full reasoning)

#### queue stats

- Queue statistics: `GET /queue_items/stats`
- Display: total, breakdown by status, by category, by confidence

#### queue item \<id\>

- Single queue item detail: `GET /queue_items/:id`

#### triage-runs

- List triage runs: `GET /triage_runs`
- Filters: `--status`, `--limit`
- Display: id, status, processed/flagged/dismissed counts, duration, model, cost

#### triage-runs stats

- Triage overview: `GET /triage_runs/stats`
- Display: pending trace count, lookback days, recent runs, queue summary

### R8: Eval

#### eval runs

- List eval runs: `GET /eval/runs`
- Filters: `--status`, `--branch`, `--mode`, `--limit`
- Display: name, branch, status, total/passed/failed, score, duration, model

#### eval run \<id\>

- Single eval run with all items: `GET /eval/runs/:id`
- Display: run summary, then items table (suite, case, status, score, duration)
- `--failed` flag: show only failed items
- `--full` flag: include conversation logs and error messages
- Navigation hint: "Run `tb-lf trace <trace_id>` to see the full trace"

#### eval revisions

- Score trends across git revisions: `GET /eval/runs/revisions`
- Filters: `--branch`, `--mode`, `--limit`
- Display: revision (short sha), message, date, runs, avg score, passed/failed

#### eval suites

- Test suite coverage: `GET /eval/coverage/suites`
- Filters: `--mode`, `--branch`
- Display: suite name, run count, last run date

#### eval cases

- Test case coverage: `GET /eval/coverage/cases`
- Filters: `--suite`, `--mode`, `--branch`, `--limit`
- Display: suite, case, runs, pass rate (threshold-colored), last run

#### eval flaky

- Flaky test detection: `GET /eval/coverage/flaky`
- Filters: `--last-n` (sample size, default 20), `--mode`, `--branch`
- Display: suite, case, sample size, passed, pass rate (all colored as warning since they're flaky)

### R9: Features

#### features

- List tracked features: `GET /features`
- Filters: `--category`, `--team`, `--status`
- Display: name, category, teams, status, queue item count

#### features items \<id\>

- Queue items for a feature: `GET /features/:id/queue_items`
- Display: trace_id, status, category, confidence

### R10: Context commands (static + light data)

#### prime

- AI-optimized context block for LLM consumption
- Sections:
  - Available projects (from cached `/projects` — name + ID)
  - Quick commands with examples
  - Current state: call `/dashboard` for headline KPIs, `/triage_runs/stats` for queue status
  - Interpreting metrics (thresholds, what's normal)
  - Common questions → command mappings
- `--mcp` flag: minimal output (~50 tokens) for hook injection
- `--project` scopes the live data sections

#### human

- Static cheat sheet for human users
- Sections: Setup, Daily Use, Investigating Traces, Eval Runs, Triage, Tips
- No API calls

#### explain \[topic\]

- Static domain knowledge
- Topics: entities, relationships, traces, scores, observations, sessions, evaluations, triage, features
- `--json` flag: structured output for programmatic consumption
- No API calls

### R11: Output

- **Human (default)**: Colored terminal output using `colored` crate. Box-drawing for headers. Threshold coloring for scores. Relative timestamps. Truncated previews. Right-aligned numbers.
- **JSON (`--json`)**: `serde_json::to_string_pretty` of the data struct. Every command supports it. For paginated responses, output the `data` array only (not the meta wrapper) unless `--full` is used.
- **No CSV** in v1 — `--json | jq` covers piping needs.

### R12: Navigation hints

Every command output ends with contextual next-step suggestions:
- List views: "Run `tb-lf <entity> <id>` for details"
- Empty results: "No traces found. Try widening filters or check `tb-lf doctor`"
- Truncated views: "Showing N of M. Use --limit to see more"
- `after_help` on every clap command with 2-4 real examples

### R13: Caching

URL-keyed filesystem cache at `~/.cache/tb-lf/`, following `tb-bug` pattern.

| TTL tier | Duration | Used for |
|---|---|---|
| Long | 1 hour | Projects list, Langfuse proxy (trace/observation detail), explain topics |
| Medium | 5 min | Dashboard, daily reports, eval run details, triage stats |
| Short | 2 min | Trace/session/score lists, queue items, daily metrics |

- `--no-cache` global flag bypasses cache reads (still writes)
- `doctor` reports cache size
- On startup, evict entries older than Long TTL

### R14: API client

- `DevPortalClient` struct: `reqwest::Client`, base_url, token, cache
- `get<T>(path, ttl)` — check cache, fetch if miss, store result, deserialize
- `get_raw(path, ttl)` — same but return raw string (for JSON passthrough)
- `build_path(base, params)` — construct query string from optional params
- Bearer token auth: `Authorization: Bearer <token>`
- Error mapping: 401 → "Invalid token. Run `tb-lf config show` to check", 404 → "Not found", 5xx → "DevPortal error"
- Numeric fields come back as strings — use `string_or_f64` / `string_or_u64` serde helpers

## Non-goals

- **No write operations** — tb-lf is strictly read-only. No creating queue items, triggering triage runs, triggering syncs, or posting scores. Use devportal UI for mutations.
- **No admin operations** — No project CRUD, sync scheduling, triage scheduling. These are devportal admin UI features.
- **No local database** — No SQLite, no data persistence beyond cache. DevPortal is the single source of truth.
- **No direct Langfuse API access** — Everything goes through devportal. The proxy endpoints exist for trace/observation detail.
- **No multi-project aggregation** — Each command operates on one project at a time via `--project`. No cross-project summaries.
- **No interactive TUI** — No dialoguer prompts, no interactive triage labeling. The old lfi had this; tb-lf is non-interactive.
- **No CSV output** — JSON + human-readable covers all needs for v1.
- **No offline mode** — No local data, no sync. If devportal is down, the CLI doesn't work.
- **No trends/charts** — Deferred to post-v1. `metrics` table covers the data; ASCII charts are a visualization nicety.
- **No auto-pagination** — Never fetch all pages automatically. Single page + `--page N` navigation. Protect AI context windows from data floods.
- **No baseline/snapshot feature** — May revisit later, not in v1.
- **No dataset management** — Eval tracking is the replacement; datasets are a devportal concern.

## Technical approach

### Code structure

```
crates/tb-lf/src/
├── main.rs           # CLI definitions (clap) + command dispatch
├── lib.rs            # Module declarations
├── config.rs         # Config loading (secrets.toml / standalone / env vars)
├── api.rs            # DevPortalClient (HTTP, cache, auth, pagination)
├── cache.rs          # Filesystem cache (URL-hashed, TTL tiers)
├── error.rs          # TbLfError enum + Result<T> alias
├── output.rs         # Formatting helpers (relative_time, truncate, fmt_cost, render_json, score_color)
├── types.rs          # Response structs (Trace, Session, Score, Observation, etc.)
└── commands/
    ├── mod.rs
    ├── traces.rs     # traces, trace <id>
    ├── sessions.rs   # sessions, session <id>
    ├── observations.rs
    ├── scores.rs
    ├── comments.rs
    ├── search.rs
    ├── tags.rs
    ├── dashboard.rs
    ├── daily.rs      # daily reports
    ├── metrics.rs
    ├── queue.rs      # queue items + stats
    ├── triage.rs     # triage runs + stats
    ├── eval.rs       # eval runs, revisions, coverage, flaky
    ├── features.rs
    ├── prime.rs
    ├── human.rs
    ├── explain.rs
    ├── config_cmd.rs
    └── doctor.rs
```

### Shared patterns

**Time range args** — Reusable `#[derive(clap::Args)]` struct flattened into commands:
```rust
#[derive(clap::Args)]
struct TimeRange {
    #[arg(long)] since: Option<String>,  // "7d", "30 days", "2w"
    #[arg(long)] from: Option<String>,   // YYYY-MM-DD
    #[arg(long)] to: Option<String>,     // YYYY-MM-DD
}
```
Parsed into `from`/`to` query params. `--since` is converted relative to now.

**Output dispatch** — Every command handler checks `json` flag first, returns early with JSON if set. Human output follows.

**Error display** — Main catches all errors and prints them with colored "Error:" prefix. No stack traces in normal mode.

### Dependencies

Keep: `clap`, `tokio`, `reqwest`, `serde`, `serde_json`, `toml`, `chrono`, `colored`, `thiserror`, `dirs`

Remove from current Cargo.toml (not needed): `rusqlite`, `indicatif`, `reqwest-middleware`, `clap_complete`, `base64`, `urlencoding`, `csv`, `textplots`, `dialoguer`, `log`, `env_logger`, `rand`, `uuid`

May add later: `textplots` (for `trends` ASCII charts, post-v1)

### Key decisions

1. **`[devportal]` config section** — Auth is against devportal, not Langfuse. The section name reflects this.
2. **Flat command hierarchy** — No `report`/`list`/`fetch` nesting. Users think in entities, not command categories.
3. **Project by name or ID** — Resolve via cached `/projects` list. Names are more memorable; IDs are unambiguous.
4. **Cache from day 1** — Follows tb-bug pattern. URL-hashed filesystem cache with TTL tiers. Prevents hammering devportal during exploratory sessions.
5. **`string_or_f64` deserializer** — DevPortal (Rails) returns decimals as strings. Custom serde helper handles both string and numeric JSON values.
6. **Dashboard as a single endpoint** — DevPortal pre-aggregates everything. No need for local computation.
7. **Search as a devportal endpoint** — Push search logic server-side rather than fetching + filtering client-side. Spec the endpoint, implement in devportal, consume in CLI.

## DevPortal endpoint to add: search

### `GET /spa_api/ai/search`

**Purpose:** Full-text search across traces for the CLI and future UI use.

**Query params:**
- `q` (required) — Search query string
- `project_id` — Filter by project
- `from`, `to` — Time range
- `page`, `per_page` — Pagination

**Search behavior:**
- Match `q` against: trace `name` (exact or ILIKE), `user_id` (exact or ILIKE), `tags` (array contains), `user_query` (ILIKE `%q%`), `agent_response` (ILIKE `%q%`)
- Each result includes `match_type` (which field matched) and `match_context` (snippet around the match)
- Order by: relevance (exact name match first, then tag, then content), then timestamp desc

**Response:** Same as `/traces` index but each trace object has additional fields:
```json
{
  "data": [
    {
      ...trace fields...,
      "match_type": "input",
      "match_context": "...find john smith contact..."
    }
  ],
  "meta": { "page": 1, "per_page": 50, "total": 12 }
}
```

## Resolved decisions

1. **`prime` live data** — Yes, `prime` makes API calls (dashboard KPIs, queue stats, projects). Cached with Medium TTL (5min). Worth the latency for useful context.

2. **`trends` command** — Deferred to post-v1. `metrics` table view covers the data. ASCII charts are a nice-to-have visualization.

3. **Pagination strategy** — Single page by default (`--limit 20`). Expose `--page <n>` for navigation. Show "Page 1 of 75 (1500 total). Use --page 2 for next." Never auto-fetch all pages — protect AI context windows from data floods. The AI can explicitly request `--page N` if it needs more.
