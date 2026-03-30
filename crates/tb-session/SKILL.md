---
name: tb-session
description: Search and manage Claude Code sessions. Use when the user references past sessions, wants to find prior work, or needs to resume a specific conversation.
---

# tb-session

Claude Code session search CLI. Full-text search across session history with metadata filtering. Built for AI agent consumption but works for humans too.

## Capabilities

- **Search** — full-text search across messages with BM25 ranking, `--pr` filter, metadata filters
- **List** — browse sessions by metadata (branch, date, project), worktree-aware
- **Show** — session detail with conversation preview
- **Resume** — resume a past session in a **new terminal tab** (accepts UUIDs, prefixes, or name search)

## When to use

- User references prior work: "remember when we...", "that session where..."
- User asks about a specific PR: use `--pr` to find sessions mentioning it
- User asks to find or resume a past session
- Before starting work: check if a prior session already started the same task
- Use `--json` for programmatic access when processing results

## Quick reference

```bash
# Search by content
tb-session search "authentication middleware"
tb-session search "budget calculation" --all-projects

# Search by PR (number or URL)
tb-session search --pr 557
tb-session search --pr https://github.com/org/repo/pull/123

# List recent sessions
tb-session list
tb-session list --all-projects --limit 20

# Show session details (prefix match works)
tb-session show bcb7ff

# Resume by UUID prefix or name
tb-session resume bcb7ffed
tb-session resume "auth refactor"
```

## Important

- **Resume opens a new terminal tab** — Claude sessions can't nest inside each other. When called from within Claude Code (non-TTY), `resume` opens a new iTerm/Terminal.app tab, cd's into the original project, and runs `claude --resume`. This is expected — not an error.
- **Worktree-aware by default** — `list` and `search` include sessions from all git worktrees of the same repo. Use `--all-projects` for everything.
- **Resume accepts names** — `resume "auth refactor"` searches summary/first prompt and resumes the most recent match. UUID prefixes of any length also work.
- **URLs work as search queries** — special characters are sanitized automatically.

## Getting started

Run `tb-session prime` for available commands and index status.

## Live context

!`tb-session prime`
