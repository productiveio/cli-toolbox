# tb-sem Branch Filtering

**Status:** Ready
**Last updated:** 2026-03-08

## Summary

Add `--branch` filtering to all workflow-based commands that currently lack it, add a `branches` command to discover available branches per project, and fix the `prime` command's performance problem (serial API calls, over-fetching).

## Requirements

### 1. `--branch` flag on workflow-based commands

Add `--branch <name>` (exact match) to these commands:
- `flaky`
- `history`
- `triage`
- `deploys`

When omitted, behavior stays the same (show all branches). The flag maps to the existing `&branch_name=` API parameter, same as `runs` already does.

### 2. `branches` command

New command: `tb-sem branches <project>`

Lists distinct branch names from recent workflows for a given project. Implementation: fetch workflows (1-2 pages), collect unique `branch_name` values, sort, and print. This gives the user branch names to pass to `--branch`.

### 3. `prime` performance fix

Two problems:
1. **Serial execution** — projects are fetched one at a time in a `for` loop. With N projects × ~1-2s per API call pair, latency stacks to 6-18s.
2. **Over-fetching** — `list_workflows` is called with `max_pages=10`, but `prime` only uses `.first()`.

Fix:
- Fetch all projects concurrently using `futures::future::join_all` (or `JoinSet`).
- Add a `max_pages` parameter (or a dedicated method) so `prime` can request just 1 page instead of 10.

Expected result: `prime` drops from 6-18s to ~1-3s (single round-trip latency).

**No `--branch` for `prime`** — it's a quick status overview, branch filtering doesn't fit. Output stays the same (latest workflow from any branch); this feature only fixes its performance.

## Non-goals

- No branch config — branch stays a runtime CLI flag, never stored in config.
- No glob/pattern matching for branch names — exact match only.
- No changes to commands that take a pipeline ID directly (`pipeline`, `failures`, `logs`, `tests`, `promotions`, `compare`).
- No changes to the Semaphore API interaction model beyond what's described.

## Technical approach

### `--branch` flag

Follow the existing pattern from `runs`:
1. Add `#[arg(long)] branch: Option<String>` to the CLI enum variant in `main.rs`
2. Pass `branch.as_deref()` through the command's `run()` function
3. Forward to `client.list_workflows(project_id, branch, ...)`

Four commands, same mechanical change each.

### `branches` command

```
tb-sem branches <project>
```

- Call `list_workflows(project_id, None, None, None)` with 2 pages max
- Collect unique `branch_name` values into a `BTreeSet`
- Print sorted, one per line

### `prime` concurrency

Replace the serial `for` loop with concurrent futures:

```rust
let futures: Vec<_> = config.projects.iter().map(|(name, proj)| {
    async move { /* fetch workflow + pipeline for this project */ }
}).collect();
let results = futures::future::join_all(futures).await;
```

Add a `list_workflows_limited` method (or a `max_pages` param) so `prime` only fetches 1 page of workflows.

### Key decisions

| Decision | Rationale |
|---|---|
| No branch in config | Branch is contextual, not a persistent setting |
| Exact match only | Semaphore API supports exact match; client-side glob adds complexity for little value |
| `branches` as a new command | Users need discoverability before they can filter |
| Fix `prime` perf in this feature | It's related (we're touching the same commands/API layer) and it's a real user pain point |
