# Research: langfuse-insights (tb-lf)

## Sources

- Devportal codebase (`~/code/productive/devportal`) — 40+ AI endpoints inventoried
- Old lfi codebase (`../langfuse-insights`) — full command tree analyzed
- Workspace crates (`tb-sem`, `tb-bug`, `tb-prod`) — established patterns documented

---

## 1. DevPortal Endpoints Available

### Core Data (read-only, all under `/spa_api/ai/`)

| Endpoint | Method | Key Params | Notes |
|---|---|---|---|
| `/traces` | GET | project_id, from, to, environment, user_id, user_search, session_id, name, satisfaction, triage_status, sort_by, sort_dir | Paginated, full filter set |
| `/traces/stats` | GET | Same filters (minus sort/page) | Returns total_traces, total_cost, avg/max duration |
| `/traces/names` | GET | Same filters | Distinct trace name strings |
| `/sessions` | GET | project_id, from, to, environment, user_id, user_search, satisfaction, sort_by, sort_dir | Paginated |
| `/sessions/:id` | GET | project_id | All traces in a session |
| `/sessions/stats` | GET | Same filters | total_sessions, total_cost, avg_traces, max_duration |
| `/observations` | GET | project_id, trace_id, type, model, level, environment | Array (not paginated wrapper) |
| `/scores` | GET | project_id, trace_id, name, source, environment | Array |
| `/comments` | GET | project_id, object_type, object_id | Array |
| `/daily_metrics` | GET | project_id, from, to, environment | trace_count, unique_users, costs, latency, eval stats per day |
| `/dashboard` | GET | project_id, from, to, compare | Rich aggregate: KPI, adoption, activity, feedback, latency, evaluations |
| `/reports` | GET | project_id, from, to, status | Paginated list of daily reports |
| `/reports/:date` | GET | project_id | Full report with metrics[] and findings[] |
| `/reports/:date/findings` | GET | project_id, finding_type, severity | Filtered findings |

### Triage & Queue

| Endpoint | Method | Notes |
|---|---|---|
| `/queue_items` | GET | Filter by status, ai_category, category, ai_confidence, triage_run_id, feature_id |
| `/queue_items/:id` | GET | Single item detail |
| `/queue_items/stats` | GET | Counts by status, category, confidence, feature |
| `/triage_runs` | GET | Filter by status; paginated |
| `/triage_runs/:id` | GET | Single run detail |
| `/triage_runs/stats` | GET | pending_trace_count, lookback_days, queue_summary |

### Eval

| Endpoint | Method | Notes |
|---|---|---|
| `/eval/runs` | GET | Filter by status, branch, revision, mode; paginated |
| `/eval/runs/:id` | GET | Full run with items[] and scores[] |
| `/eval/runs/revisions` | GET | Revision summaries with avg_score, pass/fail counts |
| `/eval/coverage/suites` | GET | Suite-level coverage |
| `/eval/coverage/cases` | GET | Case-level with pass_rate; paginated |
| `/eval/coverage/flaky` | GET | Flaky cases across last N runs |

### Features & Categories

| Endpoint | Method | Notes |
|---|---|---|
| `/features` | GET | Filter by category_id, team_id, status; paginated |
| `/features/:id/queue_items` | GET | Queue items linked to a feature |
| `/categories` | GET | Full ordered list with feature_count |

### Langfuse Proxy

| Endpoint | Method | Notes |
|---|---|---|
| `/langfuse/traces/:id` | GET | Raw Langfuse trace (needs project_id) |
| `/langfuse/observations/:id` | GET | Raw Langfuse observation (needs project_id) |

### Meta / Auth

| Endpoint | Method | Notes |
|---|---|---|
| `/projects` | GET | List projects (admin only — need non-admin variant or use dashboard) |
| `/tokens` | GET/POST/DELETE | API token management |
| `/sync` | GET | Sync state and run history |
| `/sync/heatmap` | GET | Data coverage heatmap by date |

---

## 2. Patterns from Old lfi Worth Carrying Forward

### Must-have UX patterns

1. **`prime` / `human` / `explain` trio** — Three audiences (AI agent, human user, domain learner). `prime --mcp` produces minimal output for hooks.
2. **`--json` is first-class** — Every command checks `json` first. JSON output is the serialized struct, never reformatted.
3. **Navigation hints everywhere** — Empty states suggest next commands. Non-`--full` views hint at `--full`. Every output ends with a contextual next step.
4. **`after_help` examples on every command** — Real worked examples in `--help`, not just flag descriptions.
5. **Progressive disclosure** — `--full` adds details, `--stats` adds aggregates, `--ids-only` enables piping.
6. **Threshold coloring** — Scores: green (≥0.80), yellow (≥0.50), red (<0.50). Consistent everywhere.
7. **Time range args** — Shared `--since` (human: "7d", "30 days"), `--from`/`--to` (ISO dates). Flattened into most commands.

### Command patterns to adapt

| Old lfi | New tb-lf | Data source |
|---|---|---|
| `report dashboard` | `dashboard` | `/dashboard` endpoint (pre-aggregated!) |
| `report failing` | `failing` | `/scores` + local threshold filter |
| `report costs` | `costs` | `/daily_metrics` |
| `report trends` | `trends` | `/daily_metrics` |
| `report latency` | Part of `dashboard` | `/dashboard` has latency section |
| `report adoption` | Part of `dashboard` | `/dashboard` has adoption section |
| `report summary` | `daily [date]` | `/reports/:date` (server generates these now) |
| `report feedback` | `feedback` | `/queue_items` with status filters |
| `list traces` | `traces` | `/traces` |
| `list scores` | `scores` | `/scores` |
| `list observations` | `observations` | `/observations` |
| `list comments` | `comments` | `/comments` |
| `list sessions` | `sessions` | `/sessions` |
| `fetch trace` | `trace <id>` | `/langfuse/traces/:id` |
| `fetch observation` | `observation <id>` | `/langfuse/observations/:id` |
| `search` | `search` | `/traces` with name/user/tag filters |
| `trace-tags` | `tags` | `/traces/names` |
| `triage` (interactive) | `queue` | `/queue_items` (read-only in CLI) |
| `triage --status` | `queue stats` | `/queue_items/stats` |
| `baseline compare` | Not in v1 (revisit later) | — |
| `datasets` | Not in v1 (revisit later) | — |

### Patterns to DROP

- **Local SQLite** — No sync, no local storage, no offline mode
- **Interactive triage labeling** — Triage is now AI-driven in devportal; CLI is read-only
- **`--push` to Langfuse** — No write operations from CLI
- **Dataset management** — Eval tracking moved to devportal
- **`baseline save/compare`** — May revisit, but not for v1
- **Multi-project aggregation** — Simplify: one project at a time via `--project`

---

## 3. Workspace Conventions to Follow

### Config (`secrets.toml` pattern)

```toml
[langfuse]  # or [devportal] — TBD section name
url = "https://devportal.example.com"
token = "dp_abc123..."
project = "endtoend"  # default project name
```

Load priority: `secrets.toml [section]` → `~/.config/tb-lf/config.toml` → env vars (`DEVPORTAL_TOKEN`, `DEVPORTAL_URL`)

### Error handling

`thiserror` enum: `Api { status, message }`, `Config(String)`, `Http`, `Io`, `Json`, `TomlDeserialize`, `TomlSerialize`. `Result<T>` alias.

### API client

`DevPortalClient` struct with `reqwest::Client`, base_url, token. `get<T>()` and `get_paginated<T>()` methods. Bearer token auth. Optional filesystem cache (URL-hashed, TTL tiers).

### Output

Free functions: `relative_time()`, `truncate()`, `render_json()`, `fmt_count()`. No external table library — `println!` with format alignment + `colored` crate.

### Binary naming

`tb-lf` (matches `tb-sem`, `tb-bug`, `tb-prod` pattern).

---

## 4. New Capabilities (devportal has, old lfi didn't)

1. **Server-side dashboard aggregation** — `/dashboard` returns KPI, adoption, activity, feedback, latency, evaluations in one call. No need to compute locally.
2. **Daily reports with findings** — `/reports/:date` has AI-generated summaries, metrics, and findings with severity levels. This replaces the old `report summary`.
3. **Eval runs with item-level detail** — `/eval/runs/:id` returns every test case with scores, conversation logs, error messages.
4. **Eval revisions** — `/eval/runs/revisions` tracks score trends across git revisions.
5. **Features & categories** — Structured taxonomy of what the AI agent does. Queue items are linked to features.
6. **Triage runs with stats** — `/triage_runs/stats` gives pending trace count and queue summary.
7. **Session stats** — `/sessions/stats` aggregate.
8. **Trace stats** — `/traces/stats` aggregate.
9. **Sync heatmap** — Data coverage visualization.
10. **Trace name maps** — Display name overrides for raw trace names.

---

## 5. Proposed Command Structure

Flatten the old `report`/`list`/`fetch` hierarchy. Most users don't think in terms of "is this a report or a list?" — they think in terms of the entity or question.

```
tb-lf
├── dashboard                    # Health overview (KPI, adoption, latency, evals)
├── traces [filters]             # List/search traces
├── trace <id>                   # Fetch single trace detail (Langfuse proxy)
├── sessions [filters]           # List sessions
├── session <id>                 # All traces in a session
├── observations [filters]       # List observations
├── observation <id>             # Fetch single observation (Langfuse proxy)
├── scores [filters]             # List scores
├── comments [filters]           # List comments
├── daily [date]                 # Daily report with findings
├── metrics [filters]            # Daily metrics time series
├── failing [filters]            # Evaluators below threshold
├── costs [filters]              # Cost breakdown
├── trends [filters]             # ASCII chart of metric over time
├── tags                         # Discover trace names with counts
├── queue [filters]              # Triage queue items
│   └── stats                    # Queue statistics
├── triage-runs [filters]        # Triage run history
│   └── stats                    # Triage run statistics
├── eval                         # Eval subcommands
│   ├── runs [filters]           # List eval runs
│   ├── run <id>                 # Single eval run with items
│   ├── revisions [filters]      # Score trends across revisions
│   ├── suites                   # Test suite coverage
│   ├── cases [filters]          # Test case coverage
│   └── flaky                    # Flaky test detection
├── features [filters]           # List features
│   └── items <id>               # Queue items for a feature
├── search <query>               # Search traces by name/user/tag
├── prime                        # AI context block (--mcp for minimal)
├── human                        # Human cheat sheet
├── explain [topic]              # Domain knowledge
├── config                       # Configuration
│   ├── show                     # Show current config
│   ├── set <key> <value>        # Set config value
│   ├── login                    # Interactive setup
│   └── completions <shell>      # Shell completions
└── doctor                       # Health check
```

### Global flags

- `--json` — JSON output
- `--csv` — CSV output (where applicable)
- `--project <name>` — Override default project
- `--no-cache` — Skip cache

### Shared time range flags (on most commands)

- `--since <duration>` — Human-friendly: "7d", "30 days", "2w"
- `--from <date>` — Start date (YYYY-MM-DD)
- `--to <date>` — End date (YYYY-MM-DD)

---

## 6. Gaps & Open Questions

### DevPortal gaps for CLI

| Gap | Impact | Workaround |
|---|---|---|
| `/projects` is admin-only | Can't list projects as normal user | Use dashboard endpoint to infer project, or add non-admin project list |
| No score value range filter | `failing` must fetch all scores and filter locally | Add `min_value`/`max_value` to scores endpoint |
| No score group-by-name stats | Would need to fetch all scores for stats view | Add `/scores/stats` grouped endpoint |
| No trace tag discovery | `/traces/names` gives names, not tags | Add `/traces/tags` endpoint |
| Observations not paginated | Could be large for trace-heavy projects | Add pagination to observations endpoint |

### Design decisions needed

1. **Section name in secrets.toml** — `[langfuse]` (familiar) or `[devportal]` (accurate)? Suggest `[devportal]` since auth is against devportal, not Langfuse.
2. **Cache strategy** — Dashboard and daily reports change slowly (cache 5min). Traces/scores change on sync (cache 2min). Detail fetches are stable (cache 1hr). Or start with no cache and add later.
3. **Project resolution** — DevPortal project IDs vs names. Need a way to resolve names → IDs. Dashboard endpoint might return project info. Or cache from first successful API call.
4. **Multi-project** — Support it from day 1 or defer? Old lfi had it. Suggest: defer, support `--project` for single project.

---

## 7. Implementation Priority

### Phase 1: Foundation
Config, client, error types, output helpers, `doctor`, `config` subcommands.

### Phase 2: Core data access
`traces`, `trace`, `sessions`, `session`, `observations`, `observation`, `scores`, `comments`, `search`, `tags`

### Phase 3: Reports & analytics
`dashboard`, `daily`, `metrics`, `failing`, `costs`, `trends`

### Phase 4: Triage & eval
`queue`, `triage-runs`, `eval` subcommands

### Phase 5: Context & polish
`prime`, `human`, `explain`, `features`, cache, navigation hints, `after_help` examples
