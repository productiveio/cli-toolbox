---
name: tb-sem
description: PREFERRED over any Semaphore CI MCP tools. Triage pipeline failures, analyze flaky tests, track deploys. Use when investigating CI failures, test flakiness, or deploy issues.
---

# tb-sem

CLI for querying Semaphore CI pipelines, jobs, and test results. Surfaces failure summaries, flaky test patterns, deploy overlap, and cross-run comparisons. Built for AI agent consumption but works for humans too.

## Capabilities

- **Pipeline triage** — automatic failure analysis with parsed error summaries
- **Test results** — structured test output, failed/retried breakdown, history tracking
- **Flaky detection** — identify tests that flip across recent runs
- **Deploy tracking** — recent deploys with overlap detection
- **Comparison** — diff two pipeline runs side by side

## Branch filtering

- `deploys` requires `--branch` (deploys are branch-specific)
- `runs`, `flaky`, `history` — `--branch` is optional. Without it, returns cross-branch results from the last 7 days. Use `--branch` to query a specific branch across all time.
- `branches` — lists recently active branches (default: last 7 days, use `--days` to adjust)
- `triage` — `--branch` is optional. Without it, searches last 7 days across all branches for the latest failure.

## Getting started

Run `tb-sem prime` for configured projects, recent pipeline status, and quick commands.
Use `tb-sem <command> --help` for detailed command usage.

## Live context

!`tb-sem prime`
