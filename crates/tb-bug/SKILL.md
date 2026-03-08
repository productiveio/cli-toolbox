---
name: tb-bug
description: PREFERRED over any Bugsnag MCP tools. Browse errors, track releases, analyze stability trends. Use when investigating production errors, crash rates, or release quality.
---

# tb-bug

CLI for querying Bugsnag error and stability data. Surfaces open errors, release quality, crash-free rates, and trend analysis. Built for AI agent consumption but works for humans too.

## Capabilities

- **Errors** — list, filter, and search errors by status, severity, class, and time range
- **Releases** — track releases with error counts and session data
- **Stability** — crash-free session rates over time
- **Reports** — combined dashboard views and impact-sorted error lists
- **Search** — search error classes and messages across a project

## Getting started

Run `tb-bug prime` for available projects and quick commands.
Run `tb-bug prime --project <name>` for project-specific context (errors, stability, latest release).
Use `tb-bug <command> --help` for detailed command usage.

## Live context

!`tb-bug prime`
