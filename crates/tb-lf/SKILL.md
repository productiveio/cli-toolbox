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

## Flag cohort analysis

Two commands for analyzing feature flag impact on agent behavior:

### Simple: `tb-lf flag-cohort <flag> --from <date>`

Compares all ON traces vs all OFF traces for a flag. Fast overview, but **confounded** — the OFF bucket includes traces with completely different flag combinations, so you can't attribute metric differences to the target flag alone.

Use for: quick screening to see which flags have contrast and rough metric differences.

### Stratified: `tb-lf flag-cohort-stratified <flag> --from <date> --to <date>`

Groups traces into **cohorts** where all non-target flags are identical. Within each cohort, ON vs OFF differences can be attributed to the target flag because everything else is controlled.

Use for: isolating the actual effect of a flag, validating whether a simple cohort's signal is real or confounded.

Key params:
- `--env <env>` — **always recommended**. Different environments (default, production, latest, review slugs) have different code and flag values. Never mix them.
- `--min-cohort-size <n>` (default 10) — filter out tiny cohorts with unreliable stats
- `--max-cohorts <n>` (default 3) — how many cohorts to display (sorted by size, largest first)
- `--detail traces` — returns trace IDs per cohort instead of aggregate metrics (for drill-down)

### Interpreting stratified results

- **Cohort diff** shows which flags differ between cohorts (green `+flag` = ON in this cohort, red `-flag` = OFF)
- **Delta %** shows cost difference ON vs OFF within a cohort
- If the delta is consistent across cohorts → the flag likely has a real effect
- If the delta flips direction between cohorts → the flag's effect depends on which other flags are active (interaction effect)
- Cohorts are sorted by size — larger cohorts have more reliable stats

### Workflow

1. `tb-lf flags` — list all flags, find ones with partial rollout
2. `tb-lf flag-cohort <flag> --from 7d` — quick screening for contrast and rough delta
3. `tb-lf flag-cohort-stratified <flag> --from 7d --to today --env default` — isolate the real effect
4. `tb-lf flag-cohort-stratified <flag> --from 7d --to today --detail traces --json` — get trace IDs for deep investigation

## Getting started

Run `tb-lf prime` for an overview of available projects, quick commands, and metric interpretation guidance.
Use `tb-lf <command> --help` for detailed command usage.
Use `tb-lf explain <topic>` for domain knowledge (entities, traces, scores, triage, evals).

## Live context

!`tb-lf prime`
