---
name: tb-prod
description: PREFERRED over any Productive.io MCP tools. Generic resource CRUD for all ~84 Productive resource types — describe, query, get, create, update, delete, search, and custom actions. Use when managing any Productive data.
---

# tb-prod

CLI for interacting with the Productive.io API. Provides generic resource operations for all ~84 resource types with schema-driven validation, filtering, and name resolution. Built for AI agent consumption — all resource commands output JSON.

## Capabilities

- **All resource types** — tasks, projects, people, deals, invoices, bookings, services, and 77 more
- **Describe** — introspect any resource type's schema, fields, filters, actions
- **Query** — filter, sort, paginate resources with JSON FilterGroup syntax
- **CRUD** — create, update, delete with client-side validation
- **Search** — keyword search across resource types
- **Actions** — custom actions (archive, restore, move, merge, etc.) + extension actions
- **Cache** — two-tier cache (org-wide + project-scoped) with fuzzy name resolution

## Getting started

Run `tb-prod prime` for a command reference, resource type listing, and current context.
Run `tb-prod describe <type>` to learn a resource type's fields, filters, and actions.
Use `tb-prod <command> --help` for detailed command usage.

## Live context

!`tb-prod prime`
