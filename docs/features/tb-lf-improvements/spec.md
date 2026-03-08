# tb-lf improvements

**Status:** Ready
**Last updated:** 2026-03-08

## Summary

Fix 14 broken/degraded tb-lf commands and add client-side date normalization so `--from/--to` works intuitively. Separately, document DevPortal API changes needed for peglanje migration (scores date filtering, traces has_error filter, dashboard total cost/trace count).

## Requirements

### R1: Fix deserialization for paginated endpoints

Commands `eval runs`, `triage-runs`, and `features` crash because they expect `Vec<T>` but the API returns `{data: [...], meta: {...}}`.

- Use existing `PaginatedResponse<T>` wrapper (already used by `traces`, `sessions`, `search`)
- Access `.data` after deserialization

### R2: Fix field name mismatches in types.rs

Six commands display empty/wrong data because struct field names don't match API response keys.

**`EvalRun`:**
| tb-lf field | API field |
|---|---|
| `total_items` | `total_cases` |
| `passed_items` | `passed_cases` |
| `failed_items` | `failed_cases` |
| `avg_score` | `total_score` |
| `duration_seconds` | `duration_ms` |
| `model` | _(remove — doesn't exist on eval runs)_ |

**`TriageRun`:**
| tb-lf field | API field |
|---|---|
| `processed_count` | `total_processed` |
| `cost_usd` | `cost_cents` |
| `duration_seconds` | _(remove — doesn't exist; compute from started_at/completed_at if needed)_ |

Note: `cost_cents` is in cents, not USD. Display logic must divide by 100.

**`Feature`:**
| tb-lf field | API field |
|---|---|
| `category: Option<String>` | `category: Option<{id, name}>` — use nested object |
| `teams: Option<Vec<String>>` | `teams: Option<Vec<{id, name}>>` — use nested objects |
| CLI filter `--category` | Must send as `category_id` query param |

**`EvalSuite`:**
| tb-lf field | API field |
|---|---|
| `suite` | `suite_name` (with `suite_key` also available) |
| `last_run_date` | `last_run_at` |

**`EvalFlaky`:**
| tb-lf field | API field |
|---|---|
| `suite` | `suite_key` |
| `case` | `case_key` |
| `passed` | `passed_count` |
Also available: `suite_name`, `case_name` — use these for display.

**`EvalRevision`:**
| tb-lf field | API field |
|---|---|
| `message` | `revision_message` |
| `date` | `latest_started_at` |
| `runs` | `runs_count` |
| `passed` | `total_passed` |
| `failed` | `total_failed` |

### R3: Format queue-stats and triage-runs-stats output

Both commands dump raw `serde_json::to_string_pretty`. Replace with structured display.

**queue-stats** — show:
- Total count
- By status breakdown (one line per status)
- By category breakdown

**triage-runs-stats** — show:
- `pending_trace_count` and `lookback_days`
- Queue summary (total, by-status counts)
- Last 3-5 runs in a compact table (id, status, processed, flagged, dismissed, cost, date)

### R4: Remove `daily` command

The `/reports/:date` endpoint exists but isn't wired up yet. Remove the command and its associated types. Can be re-added when DevPortal's daily reports feature is ready.

### R5: Fix `--to` date semantics (client-side)

`--from 2026-03-06 --to 2026-03-06` returns empty results for traces/sessions because the server interprets date strings as midnight timestamps, making the range zero-width.

Fix: in `TimeRange::resolve()` (`cli.rs`), when `to` is a bare date (no time component), add +1 day before sending to the API. This makes `--to` inclusive from the user's perspective while matching the server's timestamp semantics.

Affects: all commands using `TimeRange` (traces, sessions, dashboard, scores, etc.) — applied once, fixes everywhere.

### R6: Add `--limit` to `scores` command

The scores command currently has no `--limit` flag. Add it, defaulting to the same pagination default as other commands. Peglanje needs up to 10,000 per request.

## Non-goals

- **No DevPortal API changes in this work.** Scores `--from/--to` server-side filtering, traces `--has-error`, dashboard total cost/trace count — these are documented separately in the DevPortal repo's feature file and will be implemented there.
- **No new commands.** This is purely fixing existing broken functionality.
- **No `daily` command replacement.** Removed until the DevPortal feature is ready.
- **No `--json` output changes.** JSON mode passes through API responses as-is — only human-readable formatting is being fixed.
- **Don't fix `eval runs` pagination display** (page N of M) unless `PaginatedResponse` meta is naturally available after R1.

## Technical approach

### types.rs field renames

Use `#[serde(rename = "...")]` for simple renames. For structural changes (Feature's `category` object → display string), add intermediate structs:

```rust
#[derive(Deserialize)]
struct NamedRef {
    // id: u64,  // not needed for display
    name: String,
}

struct Feature {
    // ...
    category: Option<NamedRef>,
    teams: Option<Vec<NamedRef>>,
}
```

For `TriageRun.cost_cents`: keep as `f64`, rename field, adjust display to divide by 100.

### TimeRange +1 day

In `cli.rs`, `TimeRange::resolve()`: parse the `to` string, check if it's a bare date (`YYYY-MM-DD` without `T`), add one day using `chrono::NaiveDate`, return the adjusted string. This is the minimal change — one place, affects all commands consistently.

### Key decisions

1. **Rename via serde attributes, not field names** — keeps Rust field names idiomatic while matching API wire format. Less churn in display code.
2. **+1 day on client, not server** — server behavior is correct (timestamp range). The "dates are inclusive" expectation is a UX concern best handled at the CLI layer.
3. **Remove `daily` rather than stub it** — dead code is worse than no code. The command definition, types, and handler all go.

## Open questions

None — all resolved during research.
