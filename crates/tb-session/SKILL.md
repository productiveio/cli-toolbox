---
name: tb-session
description: Search and manage Claude Code sessions. Use when the user references past sessions, wants to find prior work, or needs to resume a specific conversation.
---

# tb-session

Claude Code session search CLI. Full-text search across session history with metadata filtering. Built for AI agent consumption but works for humans too.

## Capabilities

- **Search** — full-text search across user and assistant messages with BM25 ranking
- **List** — browse sessions by metadata (branch, date, project)
- **Show** — session detail with conversation preview
- **Resume** — resume a past session (execs into claude --resume)

## When to use

- User references prior work: "remember when we...", "that session where..."
- Before starting work: check if a prior session already started the same task
- User asks to find or resume a past session
- Always use `--json` for programmatic access

## Important

- `resume` is a session-ending action — it execs into a new Claude process. Always confirm with the user before resuming.
- Default scope is the current project directory. Use `--all-projects` to widen.
- Session ID can be a prefix — `show abc123` matches `abc123-def-456-...`

## Getting started

Run `tb-session prime` for available commands and index status.

## Live context

!`tb-session prime`
