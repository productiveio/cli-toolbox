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

## Getting started

Run `tb-sem prime` for configured projects, recent pipeline status, and quick commands.
Use `tb-sem <command> --help` for detailed command usage.

## Live context

!`tb-sem prime`
