# tb-lf-improvements

## Problem

tb-lf has accumulated issues across two areas:
1. **Broken/degraded commands** — API evolved (paginated wrappers), some commands dump raw JSON, one endpoint is gone
2. **Missing features for peglanje migration** — `peglanje` needs to replace `lfi` CLI with `tb-lf`, but several filters and a date semantics bug block this

## Issues found via manual testing

### Deserialization errors (API returns `{data: [...]}`, code expects `[...]`)
1. `eval runs` — crashes with "expected a sequence, got map"
2. `triage-runs` — same
3. `features` — same

### Raw JSON dumps (no formatting)
4. `queue-stats` — dumps raw JSON
5. `triage-runs-stats` — dumps raw JSON (massive payload with full run history)

### Broken API endpoint
6. `daily` — 404 regardless of project arg

### Display issues (data renders uselessly)
7. `eval suites` — all "(unnamed)", no identifying info
8. `eval flaky` — no test names, pass rates look inverted ("0/4 passed (75%)")
9. `eval revisions` — "0 runs" for all but still has scores

## Issues from peglanje migration gaps

### P0 — Bug: `--to` date inconsistency
10. `dashboard` treats `--to` as **inclusive** (Mar 6 to Mar 6 = Mar 6 data)
11. `traces` and `sessions` treat `--to` as **exclusive** (Mar 6 to Mar 6 = empty). Must use `--to Mar 7` for Mar 6 data.

Pick one convention (inclusive is more intuitive) and apply consistently.

### P1 — Missing filters
12. `scores` missing `--from`/`--to` date filtering — returns all scores unfiltered
13. `scores` missing `--limit` flag — peglanje needs up to 10,000 scores per day
14. `traces` missing `--has-error` filter — no way to find error traces for a date range

### P2 — Dashboard output gaps (for peglanje compatibility)
15. Dashboard doesn't expose total trace count directly (available via `traces --stats`)
16. Dashboard exposes avg cost per trace, not total cost (peglanje needs total)
17. Session count scoping unclear — lfi reported 184 sessions vs tb-lf 521 for same period

## Approach

- **Issues 1-3**: fix deserialization to handle paginated response wrapper
- **Issues 4-5**: format specific fields instead of dumping JSON
- **Issue 6**: investigate if endpoint was renamed/removed; fix or drop command
- **Issues 7-9**: investigate API response, fix field mapping and display
- **Issues 10-11**: normalize date semantics to inclusive `--to`
- **Issues 12-14**: add missing filter flags
- **Issues 15-17**: investigate and fix dashboard output; may need API-side changes

## Scope

All fixes in `crates/tb-lf/`. No new commands — just making existing ones work correctly and adding missing filters. P2 dashboard items may require DevPortal API changes and can be deferred.
