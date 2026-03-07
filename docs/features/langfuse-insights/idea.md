# Idea: langfuse-insights (tb-lf)

## What are we building?

A Rust CLI tool (`tb-lf`) that provides read access to AI observability data stored in devportal. It's a thin client that queries devportal's `/spa_api/ai/*` endpoints, filters/formats locally, and outputs LLM-friendly results for debugging and monitoring AI agent behavior.

## Why?

Our AI agents (running in production via Langfuse) generate traces, observations, scores, and session data. This data flows into devportal via sync. Today, the only way to explore it is through the devportal web UI. A CLI tool enables:

- **Fast debugging**: find a problematic trace, drill into observations, check scores — all from the terminal
- **AI-assisted investigation**: output is designed for LLM consumption (Claude Code, MCP), enabling AI-powered triage workflows
- **Monitoring**: quick checks on eval runs, triage queue status, daily reports, cost trends
- **Integration**: pipe output into other tools, scripts, or dashboards

## What's in scope?

- Read-only access to devportal AI data (traces, sessions, observations, scores, comments)
- Dashboard and reporting (daily metrics, trends, costs, adoption)
- Triage queue inspection (flagged items, stats, runs)
- Eval run inspection (runs, coverage, flaky tests)
- Daily reports and findings
- Langfuse proxy (fetch raw trace/observation data)
- Feature/category browsing (what features are tracked, queue items per feature)
- JSON/table output formats, filtering, sorting

## What's explicitly out?

- Write operations (no creating/updating triage items, no triggering syncs — use devportal UI or admin tools)
- Admin operations (project management, sync scheduling, triage scheduling)
- Local data storage (no SQLite, no caching)
- Direct Langfuse API access (everything goes through devportal)

## Who benefits?

- **Developers** debugging AI agent issues in the terminal
- **Claude Code / AI agents** using the CLI as an MCP tool for automated investigation
- **Team leads** wanting quick status checks on eval quality, triage progress, costs

## How do we measure success?

- Can find and inspect any trace within seconds from the terminal
- Can answer "what's the state of our AI agents?" with a single command
- Output is useful when piped to Claude for analysis

## Key constraint

Devportal is co-developed — if we need an endpoint change or addition, we can make it. The CLI should drive requirements for the API, not work around its limitations.
