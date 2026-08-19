---
name: tb-fleet
description: Manage the Claude Code sessions running on this machine — see, peek at, steer, spawn, and hand work off to sessions across iTerm tabs and tmux panes. Use when the user asks "what's my fleet doing", "what sessions are running", "peek at session X", "tell session X to…", "spawn a session to…", "is anyone stuck", "name my sessions properly", or wants to hand the current work off to another terminal ("take this to another terminal", "baci to u drugi terminal", "let's solve that over there") or watch/supervise their running sessions.
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
| "rename X / call it …" | `tb-fleet rename <target> "<name>" [--no-tmux-sync] [--force]` |
| "name X / what should this be called" | `tb-fleet name <target>` — suggests; add `--apply` to send it |
| "name all the unnamed ones" | `tb-fleet name --all [--apply] [--no-tmux-sync]` |
| "that name is wrong / regenerate it" | `tb-fleet name <target> --refresh` — ignores the cached name and replaces it |
| "spawn / start a session to …" | `tb-fleet spawn "<prompt>" --dir <path> [--name <name>] [--backend iterm\|tmux] [--window]` |
| "take this to another terminal" | `tb-fleet handoff --file <brief.md> --dir <path> [--name <name>] [--tab] [--no-wait]` — see below |
| "watch / notify me / anyone stuck" | `tb-fleet watch [--interval 5] [--stuck 300] [--quiet] [--rows 1\|2\|auto] [--no-mouse]` |

`<target>` = a session's derived name (e.g. `work-f9`), sessionId prefix, or pid — as shown by `list`.

## How to behave

1. **Read freely, act with confirmation.** `list` and `peek` are read-only — run them whenever relevant. `send` and `spawn` change a running agent's state, so **draft the exact command and confirm with the user before running it**, unless their instruction already fully specified it ("tell work-48 to run the tests" is explicit → just do it; "spawn something to look into the flaky test" needs you to confirm dir/prompt first). `handoff` is the exception — the user asking for it *is* the confirmation.
2. **Resolve loose targets from `list` first** ("the ai-agent one", "the stuck one") — run `list`, match by cwd/title/status, then act on the resolved name.
3. **`spawn` defaults:** always pass an explicit `--dir` for the repo the user means. Backend defaults to iTerm (or tmux inside tmux); only pass `--backend` when asked.
4. **Reporting:** after `list`/`peek`, summarize in plain language — who's working, who's idle, who needs the user — rather than dumping raw output. Lead with anything that needs a decision.
5. **`watch`** (also what bare `tb-fleet` runs at a terminal) is a long-running loop (live TUI + macOS notifications on finished/stuck). Don't run it inline; tell the user to run it in a spare terminal tab. `--quiet` = notifications only, backgroundable. The TUI keys:

   | Key | Does |
   | --- | --- |
   | `1`-`9`, `0` | jump straight to that row **and** focus it — one keypress, no Enter |
   | ⏎, space, `l`, `o` | focus the selected session's tab/pane |
   | ↑/↓, `k`/`j` | move the selection |
   | tap / click | select + focus (mouse capture is on by default; `--no-mouse` or `[ui] mouse = false` turns it off) |
   | scroll wheel | move the selection |
   | `n` | rename the selected session |
   | `N` | suggest a name for the selected session (opens the rename buffer prefilled) |
   | Ctrl-N | suggest names for every still-unnamed session; results land in the event log |
   | `r` | refresh now |
   | `z` | toggle 1-row ⇄ 2-row items (remembered in `[ui] rows`) |
   | `e` | show/hide the events pane |
   | `?` | keybinding overlay |
   | `q`, Esc, Ctrl-C | quit |

   The layout is responsive: below ~70 columns each session takes two lines (name + status on top, terminal/dir/prompt underneath), below ~40 the borders go, and the events pane shrinks or disappears with the terminal height. The numbered gutter and the digit keys exist because **Enter doesn't reach some mobile SSH clients** — over mosh/Termius a bare Return arrives as Ctrl-J, which is bound to focus as well (and to apply, inside the rename buffer; Ctrl-H deletes there, Ctrl-C still quits).

   Sessions the registry reports as `waiting` are hoisted to the top and shown as a bold **⏸ needs you** — that's the row that needs a human.

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

The name is Claude's own, from its session registry — tb-fleet reads it, and can *propose* a new one, but only Claude's `/rename` ever sets it. By default a session is named after its cwd plus a short hash (`work-f9`), which is why five sessions in the same repo look alike. Five ways to fix that:

- **At launch:** `spawn`/`handoff --name "<name>"` (passes `claude -n`), so it lands in the fleet already named. Name every session you spawn — one glance at `list` should say *which* piece of work it is, not which folder.
- **From outside:** `tb-fleet rename <target> "<name>"` — sends `/rename` to that session and confirms the registry picked it up.
- **In the `watch` TUI:** select a row, press `n`, type, ⏎ (esc cancels). The buffer is pinned to the session you started it on, so a poll re-sorting the list under you can't redirect the rename.
- **Inside a session:** `/rename <name>` (or `/name`); with no argument Claude names the conversation from its own context.
- **Let it name itself:** `tb-fleet name <target>` (or `N` in the TUI) — see below.

Renaming is cosmetic and instant — it changes the display name, never the sessionId, so `peek`/`send` targets keep working. Old names are kept by Claude under `formerNames`. Prefer short, task-shaped names (`cdc-spike`, `flag-cleanup`); long ones get truncated in the fleet views.

### Generated names (`name`, `N`, Ctrl-N)

`tb-fleet name` asks a model what a session is *actually working on* and proposes a short kebab-case slug (`cdc-backfill-retry`). It reads the session's directory, git branch and first prompt, and shells out to `claude -p --model haiku` — a couple of seconds and a few hundred tokens per session.

**It is suggest-and-confirm, never auto-apply.** Generating a name changes nothing:

- `tb-fleet name <target>` / `--all` prints `old → new (llm)` and stops. `--dry-run` is the explicit spelling of the same thing.
- `--apply` is what actually sends `/rename`. Confirm with the user before passing it, exactly like `send`.
- `--refresh` (alias `--no-cache`) throws away the cached name for that session, generates a new one and stores *that*. It's the escape hatch when a bad name got cached — otherwise the cache would keep serving it for free, forever.
- In the TUI, `N` on a row generates and then **prefills the rename buffer** — the user still presses ⏎ (or edits, or Esc). Ctrl-N generates for every unnamed session at once and reports each one in the event log; applying is still one `N` + ⏎ per session.

Other things worth knowing:

- **Never renames a session mid-turn.** `/rename` is typed into a live Claude TUI, so a busy session, or one sitting on a permission prompt, would read it as its answer. Those are *held* with a note instead — rename them when they go idle. The hold covers **every** path: `rename`, `name --apply` and the TUI's rename buffer. `tb-fleet rename <target> <name> --force` overrides it for a session that is essentially always busy — it really does type into the live turn, so confirm with the user first.
- **A model answer has to look like a name.** The reply is only accepted when it is a single kebab-case token: anything carrying a space or an apostrophe is a refusal or a CLI error ("I cannot provide a name.", "Credit balance is too low", "Invalid API key · Please run /login"), and those are rejected, retried, and never cached. So `(heuristic)` after a name means "the model said something unusable", not necessarily "the model was down".
- **`--all` only touches Claude-derived names** (`name_source == "derived"`). A name a human or an earlier pass chose is left alone.
- **Names are cached** in `~/.claude/fleet-names.json`, keyed by session and by a hash of what was fed to the model, so re-running `name` on an unchanged session is free. Only usable model answers are cached — never a heuristic guess, never a rejected reply. `--refresh` is how you replace an entry. The cache is written atomically — two `watch` instances (say a Mac and a phone) can share it.
- **Fallback:** if `claude` is missing, logged out or rate-limited, it falls back to the session's git branch (ignoring `main`/`master`/`develop`) or a slug of its first prompt, marked `(heuristic)` and said once, not once per session.
- **Inert in fixture mode.** `TB_FLEET_FIXTURE` never reaches the model and never touches tmux.

### tmux session sync

One tmux session per job is the convention this serves, so a **confirmed** rename also renames the session's tmux session — `⧉ work-3` becomes `⧉ flag-cleanup`. It applies to `rename`, `name --apply` and the TUI's rename buffer; `--no-tmux-sync` or `[naming] sync_tmux = false` opts out. Handles are pane ids, so a rename can never break `peek`/`send`/focus.

It refuses, with a logged reason rather than an error, when:

- the tmux session is named `fleet` (the `switch-client` binding depends on that name);
- it is the session the dashboard itself is running in;
- it has more than one window or pane — one-session-one-job visibly doesn't hold there, so the name isn't ours to rewrite;
- the session is an iTerm tab, or its backend is unknown.

`:`, `.` and whitespace become `-` before tmux sees them (tmux would silently rewrite them to `_`), and a name already taken gets a `-2`/`-3` suffix.

### `[naming]` config

In `~/Library/Application Support/tb-fleet/config.toml`:

```toml
[naming]
enabled = true      # false = heuristic only; no `claude` child is spawned by
                    # `name`, `N` or Ctrl-N, and names come from the branch/title
model = "haiku"     # passed to `claude -p --model`
sync_tmux = true    # rename the tmux session along with the Claude session
```

With `enabled = false`, `tb-fleet name` says so up front and prints the branch/title guess — there is nothing to pay for and nothing to confirm, but `--apply` still works if you want that guess sent.

## Notes

- Discovery is authoritative: Claude's own `~/.claude/sessions/<pid>.json` registry gives precise session↔status and real busy/idle.
- `send` types text and submits it (Enter) into the target — treat it like speaking for the user into another agent; that's why it's confirm-first.
- "stuck" = a session idle past `--stuck` seconds while blocked on a permission/confirmation prompt.
- Bare `tb-fleet` opens the dashboard on a terminal but falls back to a one-shot `list` when stdout is piped — so keep using `tb-fleet list` explicitly when you parse the output; never call it bare expecting to read stdout.
- The terminal a session lives in is shown as `⧉ <tmux session>` (or `▣ <iTerm tab>`), and `list --json` carries it as `tmux_session`. The tmux *session* name is the useful one — one session per job is the convention; the window name is usually just `zsh`.
- `list --json` also carries `name_source` (`"derived"` = Claude's cwd+hash fallback, i.e. a name that says nothing) and `waiting_for`.
- `TB_FLEET_FIXTURE=<sessions.json>` makes every view read a canned fleet from that file instead of the live registry — a demo/screenshot mode. It's inert by design: no backend is driven (`peek`/`send`/`rename`/focus all refuse), no notifications fire, and no watch state is written. So it's safe to leave running against fabricated handles, but it can't be used to drive real sessions.
- `TB_FLEET_FIXTURE` is inert for naming too: no `claude` child is ever spawned and no tmux session is renamed.
- macOS + iTerm/tmux only.
