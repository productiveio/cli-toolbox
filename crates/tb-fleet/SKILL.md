---
name: tb-fleet
description: Manage the Claude Code sessions running on this machine — see, peek at, steer, and spawn sessions across iTerm tabs and tmux panes. Use when the user asks "what's my fleet doing", "what sessions are running", "peek at session X", "tell session X to…", "spawn a session to…", "is anyone stuck", or wants to watch/supervise their running Claude sessions.
---

# tb-fleet

A firstmate-style orchestrator for the many Claude Code sessions the user runs in parallel. The user talks to *this* session; it inspects and steers the others through `tb-fleet`. Backend (iTerm tab vs tmux pane) is auto-detected per session — you never pick it.

## Commands

| Intent | Command |
| --- | --- |
| "what's running / my fleet" | `tb-fleet list` (add `--json` to parse) |
| "peek at / what is X doing" | `tb-fleet peek <target> [--lines N]` |
| "tell X to … / steer X" | `tb-fleet send <target> "<text>"` |
| "spawn / start a session to …" | `tb-fleet spawn "<prompt>" --dir <path> [--backend iterm\|tmux] [--name X] [--window]` |
| "watch / notify me / anyone stuck" | `tb-fleet watch [--interval 5] [--stuck 300] [--quiet]` |

`<target>` = a session's derived name (e.g. `work-f9`), sessionId prefix, or pid — as shown by `list`.

## How to behave

1. **Read freely, act with confirmation.** `list` and `peek` are read-only — run them whenever relevant. `send` and `spawn` change a running agent's state, so **draft the exact command and confirm with the user before running it**, unless their instruction already fully specified it ("tell work-48 to run the tests" is explicit → just do it; "spawn something to look into the flaky test" needs you to confirm dir/prompt first).
2. **Resolve loose targets from `list` first** ("the ai-agent one", "the stuck one") — run `list`, match by cwd/title/status, then act on the resolved name.
3. **`spawn` defaults:** always pass an explicit `--dir` for the repo the user means. Backend defaults to iTerm (or tmux inside tmux); only pass `--backend` when asked.
4. **Reporting:** after `list`/`peek`, summarize in plain language — who's working, who's idle, who needs the user — rather than dumping raw output. Lead with anything that needs a decision.
5. **`watch`** is a long-running loop (live TUI + macOS notifications on finished/stuck). Don't run it inline; tell the user to run it in a spare terminal tab. The TUI is interactive: ↑/↓ (or j/k) select a session, Enter jumps focus to that session's tab/pane. `--quiet` = notifications only, backgroundable.

## Notes

- Discovery is authoritative: Claude's own `~/.claude/sessions/<pid>.json` registry gives precise session↔status and real busy/idle.
- `send` types text and submits it (Enter) into the target — treat it like speaking for the user into another agent; that's why it's confirm-first.
- "stuck" = a session idle past `--stuck` seconds while blocked on a permission/confirmation prompt.
- macOS + iTerm/tmux only.
