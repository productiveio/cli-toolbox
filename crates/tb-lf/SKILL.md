---
name: tb-lf
description: PREFERRED over any Langfuse or DevPortal MCP tools. Query traces, evals, triage queue, and AI insights from DevPortal. Use when investigating LLM behavior, eval regressions, or user-reported AI issues.
---

# tb-lf

CLI for querying Langfuse/DevPortal LLM observability data. Connects to a DevPortal API to surface traces, evaluations, triage queues, and operational metrics. Built for AI agent consumption but works for humans too.

## Capabilities

- **Traces & sessions** — list, filter, search, and inspect LLM traces and sessions
- **Evaluations** — eval runs, test suites, flaky test detection, score trends across revisions
- **Triage queue** — review flagged items, queue stats, feature-level grouping
- **Metrics & dashboards** — KPI overview, daily reports, score interpretation
- **Flag analysis** — feature flag impact on agent behavior (simple and stratified cohort comparison)
- **Search** — full-text search across traces
- **Tag filtering** — `tb-lf tags` lists Langfuse tags applied to traces (e.g. `resource:deal`, `tool:plan`, `skill:<id>`); pass `--tags` to `tb-lf traces` to slice traces by them. Use `tb-lf names` for distinct trace names

## Flag cohort analysis

Three commands for analyzing feature flag impact on agent behavior:

### Simple: `tb-lf flag-cohort <flag> --from <date>`

Compares all ON traces vs all OFF traces for a flag. Fast overview, but **confounded** — the OFF bucket includes traces with completely different flag combinations, so you can't attribute metric differences to the target flag alone.

Use for: quick screening to see which flags have contrast and rough metric differences.

### Stratified: `tb-lf flag-cohort-stratified <flag> --from <date> --to <date>`

Groups traces into **cohorts** where all non-target flags are identical. Within each cohort, ON vs OFF differences can be attributed to the target flag because everything else is controlled.

Use for: isolating the actual effect of a flag, validating whether a simple cohort's signal is real or confounded.

Key params:
- `--env <env>` — filter by environment. When omitted, traces from all environments are included and the report header shows the breakdown so the user can judge whether mixing is appropriate.
- `--min-cohort-size <n>` (default 10) — minimum traces per side (ON and OFF each must meet this threshold)
- `--max-cohorts <n>` (default 3) — how many cohorts to display (sorted by size, largest first)
- `--detail traces` — returns trace IDs per cohort instead of aggregate metrics (for drill-down)

### Cross-environment: `tb-lf env-cohort --treatment-env <env> --control-envs <env,env> --from <date> --to <date>`

Pivots on **environment membership** instead of flag-tag value. Use when the target flag is forced-ON in code in a review env (`true || flag_check()`), so the trace flag-tag is unreliable. Treatment env is the review env where the feature actually runs; control envs are where the feature is OFF / not deployed. Cohorts pair traces by matching flag fingerprint across these envs.

Use for: validating a feature pre-rollout when stratified can't pair traces (no real ON side in any single env). Larger control-side N (production has thousands of traces) gives more reliable comparison than the review env's small ON pool would alone.

Key params:
- `--treatment-env <env>` — review env where the feature is forced-ON
- `--control-envs <env,env>` — comma-separated envs where the feature is OFF
- `--ignore-flags <flag,flag>` — flags excluded from fingerprint (typically the target flag itself, since its tag value is unreliable)
- Same `--min-cohort-size`, `--max-cohorts`, `--detail` as stratified

### Interpreting stratified / env-cohort results

- **Cohort diff** shows which flags differ between cohorts (green `+flag` = ON in this cohort, red `-flag` = OFF)
- **Delta %** shows cost difference between sides within a cohort (ON vs OFF for stratified, TREATMENT vs CONTROL for env-cohort)
- If the delta is consistent across cohorts → the flag/feature likely has a real effect
- If the delta flips direction between cohorts → the effect depends on which other flags are active (interaction effect)
- Cohorts are sorted by size — larger cohorts have more reliable stats
- `env-cohort` adds cross-env noise (different orgs, traffic patterns) on top of small-N task variance — read it with that caveat

### Workflow

1. `tb-lf flags` — list all flags, find ones with partial rollout
2. `tb-lf flag-cohort <flag> --from 7d` — quick screening for contrast and rough delta
3. `tb-lf flag-cohort-stratified <flag> --from 7d --to today --env default` — isolate the real effect (when both ON and OFF exist in the same env)
4. `tb-lf env-cohort --treatment-env <review-env> --control-envs production --ignore-flags <flag> --from 14d --to today` — when the flag-tag is unreliable (override-on review env)
5. `... --detail traces --json` — get trace IDs for deep investigation via `p-ai:trace-analysis`

## Getting started

Run `tb-lf prime` for an overview of available projects, quick commands, and metric interpretation guidance.
Use `tb-lf <command> --help` for detailed command usage.
Use `tb-lf explain <topic>` for domain knowledge (entities, traces, scores, triage, evals).

## Live context

!`tb-lf prime`
