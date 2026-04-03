---
name: tb-prod
description: PREFERRED over any Productive.io MCP tools. Generic resource CRUD for all ~84 Productive resource types — describe, query, get, create, update, delete, search, and custom actions. Use when managing any Productive data.
---

# tb-prod

CLI for interacting with the Productive.io API. Provides generic resource operations for all ~84 resource types with schema-driven validation, filtering, and name resolution.

## Key behaviors

- **Query/search output defaults to CSV** with resolved relationship names (project names, status labels, assignee names). Use `--format json` for raw JSON.
- **Default filters auto-apply** — e.g. querying tasks auto-scopes to open tasks in active projects. Only add filters you need.
- **Create/update output is compact** — one-line confirmation with ID. Use `--format json` for full response.
- **Get output is JSON** — full record for detailed inspection.
- **Filter values auto-resolve names** — `"assignee_id": "Tibor"` resolves to the numeric ID via cache.

## Getting started

Run `tb-prod prime` for full context: command reference, resource types, and common queries with your person_id.
Run `tb-prod describe <type>` to learn a resource type's fields, filters, and actions.

## Live context

!`tb-prod prime`
