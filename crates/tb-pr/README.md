# tb-pr

GitHub PR radar — a kanban TUI + non-interactive CLI for tracking all PRs
needing your attention across the Productive GitHub organization.

One centralized view: no more tab-hopping across Draft / In review / Ready
to merge / Waiting on me / Waiting on author.

## Features

- **Kanban TUI** (`tb-pr`) with rotting colors per card, per-column scroll,
  size/task/comments badges, auto-refresh every 5 minutes, and vim-style
  navigation.
- **Non-interactive CLI** (`tb-pr list`, `tb-pr show`, `tb-pr prime`) with
  a 5-minute sqlite/fs cache so repeated `list --json` calls from scripts
  don't hit the GitHub API.
- **Direct-only "waiting on me"** — team/CODEOWNERS requests are excluded
  via `user-review-requested:@me`.
- **Productive task linking** — extracts `productive_task_id` from PR
  descriptions; press `t` in the TUI to open the linked task.
- **Claude Code skill** — ships a `SKILL.md` so agents can reason about
  your review queue via `tb-pr prime`.

## Install

```bash
./scripts/install.sh tb-pr --with-skill
```

## Getting started

```bash
tb-pr doctor          # verify gh auth + config + org access
tb-pr config init     # write default config (optional — defaults work)
tb-pr                 # launch the kanban TUI
```

## Commands

```bash
tb-pr                        # default → TUI
tb-pr tui                    # explicit TUI launch

tb-pr list                   # flattened pretty table, sorted by urgency
tb-pr list --column=waiting-on-me
tb-pr list --stale-days=7    # only PRs older than N days
tb-pr list --json            # machine-readable

tb-pr show <number|url>      # detail view of one PR
tb-pr show <url> --json

tb-pr refresh                # force full fetch, update cache
tb-pr open <number|url>      # open PR in browser

tb-pr prime                  # markdown context dump (for the skill)
tb-pr skill install          # install SKILL.md to ~/.claude/skills/
tb-pr config init|show
tb-pr doctor
```

## Columns

| Column | Source | Filter |
|---|---|---|
| Draft (mine) | `author:@me draft:true` | — |
| In review (mine) | `author:@me draft:false` | not fully approved |
| Ready to merge (mine) | `author:@me draft:false` | ≥1 approval, no CHANGES_REQUESTED |
| Waiting on me | `user-review-requested:@me` | direct requests only (no CODEOWNERS team) |
| Waiting on author | `reviewed-by:@me -author:@me` | my last review is COMMENTED or CHANGES_REQUESTED |

All queries are scoped to `org:productiveio`.

## TUI keybinds

| Keys | Action |
|---|---|
| ←/→ or h/l | switch column (wraps) |
| ↑/↓ or j/k | move selection |
| Enter / d | open PR in browser |
| t | open Productive task (if linked) |
| r | refresh now |
| c | copy PR URL to clipboard |
| w | toggle full titles (wrap) — on by default |
| ? | help popup |
| q / Esc / Ctrl-C | quit |

## Configuration

`~/.config/tb-pr/config.toml` (resolved via `toolbox-core::config`).

```toml
[github]
org = "productiveio"
username_override = ""  # default: derived from `gh auth`

[refresh]
interval_minutes = 5

[productive]
org_slug = "109-productive"
```

## Cache

SQLite/fs-backed via `toolbox-core::cache`. Board state cached under a
single key; per-PR `show` responses cached by URL. TTL is 5 minutes.
`tb-pr refresh` clears the cache and re-fetches. Warm `list` hits in
under 10 ms.

## GitHub authentication

Uses `gh auth token` under the hood — no separate token management.
If you're not logged in, `tb-pr doctor` will tell you.
