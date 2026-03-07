# LFI v2 — Implementation Checklist

Reference: [lfi-v2.md](./lfi-v2.md)

---

## Phase 1: Foundation (Config + Client)

### 1.1 Configuration

- [ ] Define new `Config` struct: `devportal.url`, `devportal.token`, `defaults.project`
- [ ] Read/write `~/.config/langfuse-insights/config.toml` in new format
- [ ] `lfi cfg show` — display config with masked token
- [ ] `lfi cfg set <key> <value>` — update individual config values
- [ ] `lfi cfg login` — interactive setup: prompt URL + token, verify with `GET /spa_api/ai/projects`
- [ ] `lfi cfg completions <shell>` — keep existing shell completion generation

### 1.2 DevPortal HTTP Client

- [ ] `DevPortalClient` struct: base URL + Bearer token from config
- [ ] Request helper: sets `Authorization: Bearer dp_...` header, `Accept: application/json`
- [ ] Error handling: map HTTP status codes (401 → auth error, 403 → forbidden, 404 → not found, 5xx → server error)
- [ ] Auto-pagination: follow `meta.page`/`meta.total` to fetch all pages when needed
- [ ] `--project` flag resolution: use flag → config default → error if ambiguous
- [ ] Timeout configuration (sensible defaults, no config knob needed yet)

### 1.3 CLI Skeleton

- [ ] New `cli.rs` with clap definitions for all v2 commands (see [Command Mapping](./lfi-v2.md#keep-backed-by-devportal-endpoints))
- [ ] Global flags: `--json`, `--csv`, `--project`
- [ ] `main.rs` dispatch: load config → build client → route to command handler
- [ ] Verify: `cargo build` succeeds, `lfi --help` shows v2 command tree

### 1.4 Response Types

- [ ] `Trace` — id, langfuse_id, name, timestamp, user_id, session_id, tags, environment, cost_usd, latency_ms, user_query, agent_response, triage_status, display_name, user_satisfied, user_feedback
- [ ] `Score` — id, langfuse_id, trace_langfuse_id, name, value, string_value, data_type, source, comment, timestamp, environment
- [ ] `Observation` — id, langfuse_id, trace_langfuse_id, observation_type, name, start_time, end_time, latency_ms, model, input_tokens, output_tokens, total_tokens, cost_usd, level
- [ ] `Comment` — id, langfuse_id, object_type, content, author_user_id
- [ ] `Session` — session_id + grouped trace data
- [ ] `DailyMetric` — date, trace_count, eval_count, total_cost_usd, total_tokens, avg_latency_ms, unique_users, eval_avg_score, error_count
- [ ] `Project` — id, name, active
- [ ] `PaginatedResponse<T>` — data: Vec<T>, meta: { page, per_page, total }
- [ ] `Dashboard` — kpi, adoption, activity, feedback, latency, evaluations sections
- [ ] `QueueItem` — trace_langfuse_id, status, ai_category, ai_confidence, ai_reasoning, category, note, reviewed_by
- [ ] `TriageRun` — id, status, total_processed, flagged_count, dismissed_count, trace_time_from, trace_time_to
- [ ] `EvalRun` — id, status, mode, total_items, passed_items, failed_items
- [ ] `DailyReport` — date, status, summary, trace_count, total_cost_usd, findings

---

## Phase 2: Read Commands

### 2.1 List Commands

- [ ] `list traces` → `GET /spa_api/ai/traces`
  - [ ] Filters: `--name`, `--user`, `--session`, `--tag`, `--environment`, `--from`/`--to`/`--since`, `--limit`
  - [ ] Local post-filters where server doesn't support: `--has-error`, `--score`
  - [ ] Output: table (default), `--json`, `--csv`
  - [ ] Navigation hint: "Run `lfi fetch trace <id>` for full details"
- [ ] `list scores` → `GET /spa_api/ai/scores`
  - [ ] Filters: `--name`, `--source`, `--environment`, `--trace`, `--from`/`--to`/`--since`, `--limit`
  - [ ] Local post-filters: `--value`, `--max-value` (pending [devportal gap](./lfi-v2.md#devportal-gaps-need-to-add))
  - [ ] Group-by: `--by={name|source|string_value}`
  - [ ] Output: table, `--json`, `--csv`, `--stats`
- [ ] `list observations` → `GET /spa_api/ai/observations`
  - [ ] Filters: `--trace`, `--type`, `--model`, `--level`, `--environment`, `--from`/`--to`/`--since`, `--limit`
  - [ ] Output: table, `--json`, `--csv`, `--stats`
- [ ] `list comments` → `GET /spa_api/ai/comments`
  - [ ] Filters: `--trace`, `--author`, `--search`, `--from`/`--to`/`--since`, `--limit`
  - [ ] Output: table, `--json`, `--csv`
- [ ] `list sessions` → `GET /spa_api/ai/sessions`
  - [ ] Filters: `--user`, `--name`, `--from`/`--to`/`--since`, `--limit`
  - [ ] Output: table, `--json`, `--csv`

### 2.2 Fetch Commands

- [ ] `fetch trace <id>` → `GET /spa_api/ai/langfuse/traces/:id`
  - [ ] Flags: `--full`, `--observations`
  - [ ] Requires `--project` (proxy needs project credentials)
  - [ ] Output: formatted detail view, `--json`
- [ ] `fetch observation <id>` → `GET /spa_api/ai/langfuse/observations/:id`
  - [ ] Flags: `--full`
  - [ ] Requires `--project`
  - [ ] Output: formatted detail view, `--json`

### 2.3 Dashboard

- [ ] `report dashboard` → `GET /spa_api/ai/dashboard`
  - [ ] Params: `--project`, `--from`/`--to`, `--compare`
  - [ ] Render: KPI summary, adoption trends, activity breakdown, feedback stats, latency percentiles, evaluation pass rates
  - [ ] Output: formatted view (default), `--json`, `--csv`

---

## Phase 3: Reports + Aggregation

### 3.1 Report Commands

- [ ] `report failing` → `GET /spa_api/ai/scores` + local threshold filtering
  - [ ] Flags: `--threshold` (default 0.80), `--by={category|tag}`, `--tag`, `--category`, `--limit`
  - [ ] Logic: fetch scores, group by evaluator name, compute pass rates, filter below threshold
  - [ ] Depends on: [devportal gap — failing evaluations report](./lfi-v2.md#devportal-gaps-need-to-add) (can work client-side until endpoint exists)
- [ ] `report costs` → `GET /spa_api/ai/daily_metrics`
  - [ ] Flags: `--by={day|user|tag|environment}`, `--from`/`--to`/`--since`
  - [ ] Logic: fetch daily metrics, aggregate locally by grouping key
  - [ ] Depends on: [devportal gap — cost breakdown](./lfi-v2.md#devportal-gaps-need-to-add) for user/tag grouping
- [ ] `report trends` → `GET /spa_api/ai/daily_metrics`
  - [ ] Flags: `--metric={pass-rate|cost|trace|score}`, `--evaluator`, `--tag`, `--from`/`--to`/`--since`
  - [ ] Logic: fetch daily metrics, render time-series (sparklines or table)
- [ ] `report latency` → `GET /spa_api/ai/dashboard` (latency section)
  - [ ] Flags: `--by={name|day}`, `--from`/`--to`/`--since`
  - [ ] Render: p50/p95/p99 breakdown
- [ ] `report adoption` → `GET /spa_api/ai/dashboard` (adoption section)
  - [ ] Flags: `--limit`, `--from`/`--to`/`--since`
  - [ ] Render: user growth, sessions per user
- [ ] `report summary` → `GET /spa_api/ai/daily_metrics` + `GET /spa_api/ai/scores`
  - [ ] Compose: daily breakdown + evaluation results into period digest
  - [ ] Flags: `--tag`, `--from`/`--to`/`--since`
- [ ] `report feedback` → `GET /spa_api/ai/queue_items`
  - [ ] Render: flagged items summary, category breakdown
  - [ ] Flags: `--since`, `--limit`

### 3.2 Search + Tags

- [ ] `search` → `GET /spa_api/ai/traces` with filter params
  - [ ] Map query to: `--name`, `--tag`, `--user_search`, `--environment`
  - [ ] Flags: `--type={name|tag|comment|input}`, `--ids-only`, `--limit`, `--from`/`--to`/`--since`
  - [ ] Output: table, `--json`, `--csv`
- [ ] `trace-tags` → `GET /spa_api/ai/traces/names` + tag aggregation
  - [ ] Flags: `--trend`, `--stats`, `--from`/`--to`/`--since`
  - [ ] Depends on: [devportal gap — trace-tags discovery](./lfi-v2.md#devportal-gaps-need-to-add)

---

## Phase 4: Triage + Eval + New Commands

### 4.1 Triage (read-only)

- [ ] `triage list` → `GET /spa_api/ai/queue_items`
  - [ ] Filters: `--status`, `--category`, `--from`/`--to`/`--since`, `--limit`
  - [ ] Output: table with trace ID, category, confidence, status
- [ ] `triage stats` → `GET /spa_api/ai/queue_items/stats`
  - [ ] Render: total, by-status breakdown, by-category breakdown
- [ ] `triage runs` → `GET /spa_api/ai/triage_runs`
  - [ ] Render: run list with status, processed/flagged/dismissed counts
  - [ ] Flags: `--limit`

### 4.2 Eval (read-only)

- [ ] `eval runs` → `GET /spa_api/ai/eval/runs`
  - [ ] Render: run list with status, total/passed/failed counts
  - [ ] Flags: `--limit`, `--status`
- [ ] `eval coverage` → `GET /spa_api/ai/eval/coverage/*`
  - [ ] Subcommands or flags: `--suites`, `--cases`, `--flaky`
  - [ ] Render: coverage tables

### 4.3 New Commands

- [ ] `report daily [date]` → `GET /spa_api/ai/reports/:date`
  - [ ] Default: today (or latest available)
  - [ ] Render: summary, metrics, findings
  - [ ] Output: formatted view, `--json`
- [ ] `sessions <id>` → `GET /spa_api/ai/sessions/:id`
  - [ ] Render: all traces in the session, ordered by timestamp
  - [ ] Output: table, `--json`

---

## Phase 5: Static/Local Commands

- [ ] `prime` — Update for v2 command set and devportal-backed workflow
- [ ] `human` — Update cheat sheet for v2 commands
- [ ] `explain` — Keep as-is (domain concepts unchanged)

---

## Phase 6: Cleanup + Polish

### 6.1 Remove v1 Code

- [ ] Delete `src/db/` (schema, queries, aggregation, persistence, migrations, labels, baselines, sync, query_builder, projects, types)
- [ ] Delete `src/commands/sync/` (engine, types, parsers, status)
- [ ] Delete `src/commands/label.rs`
- [ ] Delete `src/commands/baseline.rs`
- [ ] Delete `src/commands/datasets.rs`, `dataset_items.rs`, `dataset_item.rs`, `dataset_diff.rs`, `dataset_report.rs`, `eval_compare.rs`
- [ ] Delete `src/commands/project.rs`, `prune.rs`, `quickstart.rs`, `onboard.rs`, `skill.rs`
- [ ] Delete `src/client/langfuse_client.rs` (direct Langfuse API client)
- [ ] Delete `src/command_context.rs` (replace with v2 context carrying client + config)
- [ ] Delete `src/paths.rs` if no longer needed (no SQLite path resolution)

### 6.2 Dependencies

- [ ] Remove `rusqlite` from Cargo.toml
- [ ] Remove `indicatif` from Cargo.toml
- [ ] Audit remaining deps — remove anything only used by deleted code
- [ ] Verify `cargo build` clean, no dead code warnings

### 6.3 Tests

- [ ] Remove all SQLite-based unit tests (`src/db/mod.rs` tests)
- [ ] Remove pipeline tests (`tests/pipeline_tests.rs`)
- [ ] Remove VCR tests (`tests/vcr_tests.rs`) or adapt to mock devportal
- [ ] Add integration tests: mock devportal HTTP responses (e.g., `wiremock` or `mockito`)
- [ ] Test config loading/saving
- [ ] Test client auth + pagination
- [ ] Test local post-filtering logic
- [ ] Test output formatting (JSON, CSV, table)

### 6.4 Documentation

- [ ] Update `CLAUDE.md` — new commands, remove sync guard, update key files
- [ ] Update `README.md`
- [ ] Update `prime` output
- [ ] Update `human` cheat sheet
- [ ] Archive or remove v1 docs that no longer apply

---

## DevPortal Work (tracked separately)

Items that need to be added/changed in devportal before certain lfi commands reach full parity. LFI can ship with client-side workarounds initially.

- [ ] Score value range filtering (`min_value`/`max_value` on scores endpoint)
- [ ] Tag-based aggregation (group-by-tag on traces/daily_metrics)
- [ ] Failing evaluations endpoint (scores below threshold, grouped by evaluator)
- [ ] Cost breakdown by user/tag/evaluator (group-by on daily_metrics)
- [ ] Trace-tags discovery endpoint (`GET /spa_api/ai/traces/tags`)
- [ ] Score stats by name (`GET /spa_api/ai/scores/stats`)
- [ ] Fetch score proxy (`GET /spa_api/ai/langfuse/scores/:id`)
- [ ] Environment filter audit across all endpoints
