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
- **Resume vs. digest** — `resume` loads the entire past conversation (useful when full context matters). `show` + a fresh session is often preferable when the user wants to continue work without the noise of prior turns. Pick based on user intent (see Workflow step 5).

## Workflow

When handling a session-related request:

1. **Always use `--json`** on search and list commands — structured output is easier to parse and present accurately.
2. **Pick the right command**:
   - User describes content → `tb-session search "<query>" --json`
   - User mentions a PR → `tb-session search --pr <N> --json`
   - User asks "what have we been working on" → `tb-session list --json`
   - User wants to continue past work → see step 5 (two valid patterns)
3. **Always present results with structured fields** — for each session shown, include:
   - Session ID (prefix is fine)
   - Branch name
   - Summary or first prompt
   - Last-active timestamp
   - Even when matches are weak, show the top 1–2 results so the user can judge relevance.
4. **After presenting results**, offer a concrete next step:
   - `tb-session show <id>` to see conversation detail
   - `tb-session resume <id>` to continue the session in a new tab
5. **When continuing past work**, pick the pattern that matches the user's intent:
   - **Explicit resume** ("resume session X", "resume where we left off") → call `tb-session resume <id>`. This reopens the full conversation in a new tab.
   - **Digest-and-continue** ("what were we doing on X?", "let's continue on X", "pick up where we left off on X") → call `tb-session show <id>`, summarize the key context for the user, and continue in the *current* session. This avoids loading noise from the old conversation.
   - **If unclear**, briefly offer both options and let the user pick. Do not default silently to either — the choice affects the working context.
6. **Stay efficient** — aim for 1–3 tool calls. One search + one action is usually enough. Only retry with different keywords if the first search returned no relevant results.

## Getting started

Run `tb-session prime` for available commands and index status.

## Live context

!`tb-session prime`
