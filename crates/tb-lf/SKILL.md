---
name: tb-lf
description: Langfuse LLM observability — query traces, evals, triage queue, and AI insights from DevPortal. Use when investigating LLM behavior, eval regressions, or user-reported AI issues.
---

# tb-lf

CLI for querying Langfuse/DevPortal LLM observability data. Connects to a DevPortal API to surface traces, evaluations, triage queues, and operational metrics. Built for AI agent consumption but works for humans too.

## Capabilities

- **Traces & sessions** — list, filter, search, and inspect LLM traces and sessions
- **Evaluations** — eval runs, test suites, flaky test detection, score trends across revisions
- **Triage queue** — review flagged items, queue stats, feature-level grouping
- **Metrics & dashboards** — KPI overview, daily reports, score interpretation
- **Search** — full-text search across traces

## Getting started

Run `tb-lf prime` for an overview of available projects, quick commands, and metric interpretation guidance.
Use `tb-lf <command> --help` for detailed command usage.
Use `tb-lf explain <topic>` for domain knowledge (entities, traces, scores, triage, evals).

## Live context

!`tb-lf prime`
