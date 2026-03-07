# LFI v2 — DevPortal-Backed CLI

## Vision

LFI v2 drops local sync and SQLite in favor of devportal as the single source of truth. LFI becomes a thin, focused CLI that fetches data from devportal endpoints, filters/sorts locally, and provides LLM-friendly output with navigation context.

## Architecture Change

```
v1: Langfuse API → lfi sync → SQLite → lfi query → output
v2: Langfuse API → devportal sync → devportal DB → devportal API → lfi query → output
```

LFI no longer owns data. It is a **read-only client** of devportal's SPA API (`/spa_api/ai/*`).

---

## Configuration

### What changes

|                               v1                               |                          v2                          |
|----------------------------------------------------------------|------------------------------------------------------|
| `~/.config/langfuse-insights/config.toml` + SQLite credentials | `~/.config/langfuse-insights/config.toml` only       |
| Project name, public_key, secret_key, host                     | Personal access token + devportal URL                |
| Multiple project configs locally                               | Projects live in devportal, selected via `--project` |

### New config.toml

```toml
[devportal]
url = "https://devportal.example.com"  # or http://localhost:3000 for dev
token = "dp_abc123..."                 # personal access token from devportal

[defaults]
project = "endtoend"                   # optional default project
```

### Commands

- `lfi cfg show` — Show current config (masked token)
- `lfi cfg set <key> <value>` — Set config value
- `lfi cfg login` — Interactive token setup (prompt for URL + token, verify with `GET /spa_api/ai/projects`)

### Removed

- `cfg project add/list/info/delete` — Projects managed in devportal UI
- `cfg prune` — Pruning is a devportal admin operation
- `cfg completions` — Keep (shell completions still useful)

---

## Command Mapping

### Keep (backed by devportal endpoints)

|         Command          |                     DevPortal Endpoint                     |                 Notes                 |
|--------------------------|------------------------------------------------------------|---------------------------------------|
| `report dashboard`       | `GET /spa_api/ai/dashboard`                                | Already aggregated server-side        |
| `report failing`         | `GET /spa_api/ai/scores` + local filtering                 | Filter scores where value < threshold |
| `report costs`           | `GET /spa_api/ai/daily_metrics`                            | Aggregate locally by day/user/tag     |
| `report trends`          | `GET /spa_api/ai/daily_metrics`                            | Time-series from daily metrics        |
| `report latency`         | `GET /spa_api/ai/dashboard` (latency section)              | Percentiles from dashboard endpoint   |
| `report adoption`        | `GET /spa_api/ai/dashboard` (adoption section)             | From dashboard endpoint               |
| `report summary`         | `GET /spa_api/ai/daily_metrics` + `GET /spa_api/ai/scores` | Compose locally                       |
| `report feedback`        | `GET /spa_api/ai/queue_items`                              | Triage queue replaces old feedback    |
| `list traces`            | `GET /spa_api/ai/traces`                                   | Server-side filtering available       |
| `list scores`            | `GET /spa_api/ai/scores`                                   | Server-side filtering available       |
| `list observations`      | `GET /spa_api/ai/observations`                             | Server-side filtering available       |
| `list comments`          | `GET /spa_api/ai/comments`                                 | Server-side filtering available       |
| `list sessions`          | `GET /spa_api/ai/sessions`                                 | Server-side grouping available        |
| `fetch trace <id>`       | `GET /spa_api/ai/langfuse/traces/:id`                      | Proxied to Langfuse API               |
| `fetch observation <id>` | `GET /spa_api/ai/langfuse/observations/:id`                | Proxied to Langfuse API               |
| `eval runs`              | `GET /spa_api/ai/eval/runs`                                | List evaluation runs                  |
| `eval coverage`          | `GET /spa_api/ai/eval/coverage/*`                          | Test suite/case coverage              |
| `search`                 | `GET /spa_api/ai/traces` with filters                      | Use trace name/tag/user filters       |
| `trace-tags`             | `GET /spa_api/ai/traces/names` + `GET /spa_api/ai/traces`  | Aggregate tags locally                |
| `triage list`            | `GET /spa_api/ai/queue_items`                              | Read-only: view flagged items         |
| `triage stats`           | `GET /spa_api/ai/queue_items/stats`                        | Read-only: triage progress            |
| `triage runs`            | `GET /spa_api/ai/triage_runs`                              | Read-only: list triage runs           |
| `prime`                  | Local (no API needed)                                      | Static context output                 |
| `human`                  | Local (no API needed)                                      | Static cheat sheet                    |
| `explain`                | Local (no API needed)                                      | Static domain knowledge               |

### Remove

|            Command             |                 Reason                 |
|--------------------------------|----------------------------------------|
| `sync`                         | Devportal owns sync                    |
| `cfg project *`                | Projects managed in devportal          |
| `cfg prune`                    | Devportal admin operation              |
| `label`                        | Replaced by triage queue items         |
| `datasets *`                   | Replaced by devportal eval tracking    |
| `baseline save/compare/delete` | TBD: may reimplement against devportal |
| `quickstart`                   | Replace with simpler `cfg login`       |
| `onboard`                      | Regenerate for v2                      |
| `skill`                        | Regenerate for v2                      |
| `fetch score <id>`             | No proxy endpoint in devportal yet     |

### New (leveraging devportal features)

|        Command        |        DevPortal Endpoint        |         Purpose          |
|-----------------------|----------------------------------|--------------------------|
| `report daily [date]` | `GET /spa_api/ai/reports/:date`  | View daily reports       |
| `sessions <id>`       | `GET /spa_api/ai/sessions/:id`   | All traces in a session  |

---

## DevPortal Gaps (Need to Add)

Features lfi v1 has that devportal doesn't yet expose:

|               Feature                |                          What's Missing                           |     Priority      |
|--------------------------------------|-------------------------------------------------------------------|-------------------|
| Score filtering by value range       | `min_value`/`max_value` params on scores endpoint                 | High              |
| Tag-based aggregation                | Group-by-tag in traces/daily_metrics                              | High              |
| Failing evaluations report           | Endpoint that returns evals below threshold, grouped by evaluator | Medium            |
| Cost breakdown by user/tag/evaluator | Group-by params on daily_metrics or dedicated cost endpoint       | Medium            |
| Trace-tags discovery                 | `GET /spa_api/ai/traces/tags` distinct tags endpoint              | Medium            |
| Score stats by name                  | `GET /spa_api/ai/scores/stats` grouped statistics                 | Medium            |
| Fetch score proxy                    | `GET /spa_api/ai/langfuse/scores/:id` proxy endpoint              | Low               |
| Environment filter on more endpoints | Some endpoints may lack `environment` param                       | Low               |
| CSV export                           | Server-side CSV or rely on lfi local formatting                   | Low (lfi handles) |
| Baseline save/compare                | New concept for devportal or keep local                           | Low               |

---

## LFI's Role in v2

### What LFI does

1. **Filter/sort** — Apply fine-grained local filters on devportal responses (value ranges, tag combinations, pattern matching) that the API doesn't support
2. **Format** — Render tables, CSV, JSON; colorize pass/fail; truncate for readability
3. **Navigate** — Provide "next steps" hints in output (e.g., "Run `lfi fetch trace <id>` for full details")
4. **Compose** — Combine multiple endpoint responses into unified views (e.g., dashboard = daily_metrics + scores + traces)
5. **Paginate transparently** — Auto-fetch all pages when needed, present unified results

### What LFI does NOT do

- Own or store data (no SQLite, no sync)
- Manage projects or credentials for Langfuse (devportal does this)
- Run long-running operations (sync, triage runs are devportal jobs)
- Mutate triage/queue data (AI classifier handles triage; lfi is read-only)
- Manage datasets (eval tracking is moving to devportal's own solution)

---

## Code Structure (v2)

```
src/
├── main.rs                  # Entry point + command dispatch
├── cli.rs                   # Clap CLI definitions (simplified)
├── config.rs                # TOML config (url, token, defaults)
├── output.rs                # JSON/CSV/table formatting
├── error.rs                 # Error types
├── client/
│   ├── mod.rs               # DevPortal HTTP client (reqwest + Bearer token)
│   ├── types.rs             # Response types (traces, scores, etc.)
│   └── pagination.rs        # Auto-pagination helper
├── commands/
│   ├── cfg.rs               # Config management
│   ├── dashboard.rs         # report dashboard
│   ├── failing.rs           # report failing
│   ├── costs.rs             # report costs
│   ├── trends.rs            # report trends
│   ├── latency.rs           # report latency
│   ├── adoption.rs          # report adoption
│   ├── summary.rs           # report summary
│   ├── feedback.rs          # report feedback
│   ├── traces.rs            # list traces
│   ├── scores.rs            # list scores
│   ├── observations.rs      # list observations
│   ├── comments.rs          # list comments
│   ├── sessions.rs          # list sessions
│   ├── fetch.rs             # fetch trace/observation (Langfuse proxy)
│   ├── eval.rs              # eval runs/coverage
│   ├── search.rs            # search traces
│   ├── triage.rs            # triage queue (read-only)
│   ├── tags.rs              # trace-tags
│   ├── prime.rs             # AI context
│   ├── human.rs             # Cheat sheet
│   └── explain.rs           # Domain knowledge
└── filters.rs               # Local filter/sort helpers
```

### Removed modules

- `db/` — Entire database layer (schema, queries, aggregation, persistence, migrations)
- `commands/sync/` — Sync engine
- `commands/label.rs` — Local labeling (replaced by triage)
- `commands/baseline.rs` — Local baselines
- `client/langfuse_client.rs` — Direct Langfuse API client

### Dependencies removed

- `rusqlite` — No local database
- `indicatif` — No sync progress bars

### Dependencies added/kept

- `reqwest` — HTTP client (already used)
- `serde`/`serde_json` — JSON handling (already used)
- `clap` — CLI parsing (already used)
- `toml` — Config file (already used)
- `colored` — Terminal colors (already used)

---

## Migration Path

### Phase 1: Config + Client

1. New `config.toml` format with devportal URL + token
2. DevPortal HTTP client with Bearer auth + pagination
3. `cfg login` / `cfg show` / `cfg set` commands
4. Verify connectivity: `GET /spa_api/ai/projects`

### Phase 2: Read Commands

Migrate commands one-by-one, keeping v1 code alongside:

1. `list traces` → `GET /spa_api/ai/traces`
2. `list scores` → `GET /spa_api/ai/scores`
3. `list observations` → `GET /spa_api/ai/observations`
4. `fetch trace` → `GET /spa_api/ai/langfuse/traces/:id`
5. `report dashboard` → `GET /spa_api/ai/dashboard`

### Phase 3: Reports + Aggregation

Commands that need local composition:

1. `report failing` — Fetch scores, filter locally by threshold
2. `report costs` — Fetch daily_metrics, group locally
3. `report trends` — Fetch daily_metrics, format as time series
4. `search` — Map to trace filters

### Phase 4: Triage + Eval (New)

1. `triage list/stats/runs` (read-only)
2. `eval runs/coverage`
3. `report daily`

### Phase 5: Cleanup

1. Remove v1 code (db/, sync/, langfuse_client)
2. Remove SQLite dependency
3. Update tests (mock devportal responses instead of seed_db)
4. Update CLAUDE.md, prime, human, explain for v2

---

## Open Questions

1. **Baseline feature** — Keep as local-only (store snapshots in config dir) or move to devportal?
2. **Offline mode** — Should lfi cache any responses locally for offline use?
3. **Rate limiting** — Does devportal need rate limiting for CLI access?
4. **Project selection** — Use project name or ID? Devportal returns IDs, lfi v1 uses names.
5. **Backward compatibility** — Clean break or support v1 config migration?
