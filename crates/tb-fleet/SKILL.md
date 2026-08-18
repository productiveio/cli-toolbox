---
name: tb-fleet
description: Manage the Claude Code sessions running on this machine — see, peek at, steer, spawn, and hand work off to sessions across iTerm tabs and tmux panes. Use when the user asks "what's my fleet doing", "what sessions are running", "peek at session X", "tell session X to…", "spawn a session to…", "is anyone stuck", or wants to hand the current work off to another terminal ("take this to another terminal", "baci to u drugi terminal", "let's solve that over there") or watch/supervise their running sessions.
---

# tb-fleet

A firstmate-style orchestrator for the many Claude Code sessions the user runs in parallel. The user talks to *this* session; it inspects and steers the others through `tb-fleet`. Backend (iTerm tab vs tmux pane) is auto-detected per session — you never pick it.

## Commands

| Intent | Command |
| --- | --- |
| "what's running / my fleet" | `tb-fleet list` (add `--json` to parse) |
| "open the fleet dashboard" | `tb-fleet` — bare, no subcommand; tell the user to run it, don't run it inline |
| "peek at / what is X doing" | `tb-fleet peek <target> [--lines N]` |
| "tell X to … / steer X" | `tb-fleet send <target> "<text>"` |
| "rename X / call it …" | `tb-fleet rename <target> "<name>"` |
| "spawn / start a session to …" | `tb-fleet spawn "<prompt>" --dir <path> [--name <name>] [--backend iterm\|tmux] [--window]` |
| "take this to another terminal" | `tb-fleet handoff --file <brief.md> --dir <path> [--name <name>] [--tab] [--no-wait]` — see below |
| "watch / notify me / anyone stuck" | `tb-fleet watch [--interval 5] [--stuck 300] [--quiet]` |

`<target>` = a session's derived name (e.g. `work-f9`), sessionId prefix, or pid — as shown by `list`.

## How to behave

1. **Read freely, act with confirmation.** `list` and `peek` are read-only — run them whenever relevant. `send` and `spawn` change a running agent's state, so **draft the exact command and confirm with the user before running it**, unless their instruction already fully specified it ("tell work-48 to run the tests" is explicit → just do it; "spawn something to look into the flaky test" needs you to confirm dir/prompt first). `handoff` is the exception — the user asking for it *is* the confirmation.
2. **Resolve loose targets from `list` first** ("the ai-agent one", "the stuck one") — run `list`, match by cwd/title/status, then act on the resolved name.
3. **`spawn` defaults:** always pass an explicit `--dir` for the repo the user means. Backend defaults to iTerm (or tmux inside tmux); only pass `--backend` when asked.
4. **Reporting:** after `list`/`peek`, summarize in plain language — who's working, who's idle, who needs the user — rather than dumping raw output. Lead with anything that needs a decision.
5. **`watch`** (also what bare `tb-fleet` runs at a terminal) is a long-running loop (live TUI + macOS notifications on finished/stuck). Don't run it inline; tell the user to run it in a spare terminal tab. The TUI is interactive: ↑/↓ (or j/k) select a session, Enter jumps focus to that session's tab/pane, `n` renames the selected one. `--quiet` = notifications only, backgroundable.

## Handing work off ("take that to another terminal")

When the user wants a piece of *this* conversation continued elsewhere — "throw that into another terminal", "baci to u drugi terminal", "let's do that one over there", "hand this off" — that's `handoff`, not `spawn`. `spawn` starts a stranger; `handoff` transplants context. The test: **would the new session need anything we worked out here?** If yes it's a handoff, however the user phrased it; a genuinely standalone errand ("start a session to watch the deploy") is a `spawn`.

The phrase is the instruction: **do it, don't ask for confirmation.** Only ask when the *subject* is genuinely ambiguous ("that" could be two different threads) or when you can't tell which directory it belongs in.

1. **Write the brief first.** Everything the new session needs and cannot see, because it starts with an empty context window. Write it to a scratch file and pass `--file`:
   - the goal, in one or two sentences — what "done" looks like;
   - where things stand: branch/worktree, what's already changed, what's been tried and ruled out;
   - the concrete files, commands, URLs, task/PR links you'd otherwise have to re-discover;
   - decisions and constraints already settled here, so they don't get re-litigated;
   - **the scope**: the brief's preamble tells the new session to work autonomously within it, so spell out any limit — read-only, investigate-don't-fix, don't push, ask before touching X — or it will assume a free hand;
   - the first move, if there's an obvious one.

   Write it *to* the other agent ("Investigate X. The repro is …"), not as a summary of your chat. Err on the side of too much: the brief is the only context it gets.
2. **Pick the directory deliberately** — the worktree or repo the work belongs to, `--dir <path>`. It defaults to the current directory, which is usually wrong for a handoff.
3. **Report back the name** printed by the command, so the user (and you) can `peek`/`send` that session later. `handoff` waits for it to register; `--no-wait` skips that.
4. Handoff opens a **new window** by default (that's what "another terminal" means); pass `--tab` when the user asks for a tab, and it follows the same iTerm/tmux auto-detection as `spawn`.
5. The brief is saved under `~/.claude/fleet-handoffs/` — the record of what was sent, and re-readable by the new session at any time.
6. The receiving session is told how to `tb-fleet send` an update **back** to this one when it finishes. If such a message arrives, treat it as a report from the session you dispatched.

Keep working on whatever the user kept here — the point of a handoff is that both threads run in parallel.

## Session names

Names are Claude's own, from its session registry — tb-fleet reads them, it doesn't invent them. By default a session is named after its cwd plus a short hash (`work-f9`), which is why five sessions in the same repo look alike. Four ways to fix that:

- **At launch:** `spawn`/`handoff --name "<name>"` (passes `claude -n`), so it lands in the fleet already named. Name every session you spawn — one glance at `list` should say *which* piece of work it is, not which folder.
- **From outside:** `tb-fleet rename <target> "<name>"` — sends `/rename` to that session and confirms the registry picked it up.
- **In the `watch` TUI:** select a row, press `n`, type, ⏎ (esc cancels).
- **Inside a session:** `/rename <name>` (or `/name`); with no argument Claude names the conversation from its own context.

Renaming is cosmetic and instant — it changes the display name, never the sessionId, so `peek`/`send` targets keep working. Old names are kept by Claude under `formerNames`. Prefer short, task-shaped names (`cdc-spike`, `flag-cleanup`); long ones get truncated in the fleet views.

## Notes

- Discovery is authoritative: Claude's own `~/.claude/sessions/<pid>.json` registry gives precise session↔status and real busy/idle.
- `send` types text and submits it (Enter) into the target — treat it like speaking for the user into another agent; that's why it's confirm-first.
- "stuck" = a session idle past `--stuck` seconds while blocked on a permission/confirmation prompt.
- Bare `tb-fleet` opens the dashboard on a terminal but falls back to a one-shot `list` when stdout is piped — so keep using `tb-fleet list` explicitly when you parse the output; never call it bare expecting to read stdout.
- macOS + iTerm/tmux only.
