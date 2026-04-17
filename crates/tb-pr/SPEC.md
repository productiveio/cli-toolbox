# tb-pr — GitHub PR Radar

A kanban-style TUI + CLI for tracking all GitHub PRs that require your attention across the Productive organization.

## Identity

- **Binary:** `tb-pr`
- **Crate:** `crates/tb-pr/`
- **Purpose:** One centralized view of PRs needing attention across the Productive GitHub organization.
- **Two modes in one binary:**
  - **Interactive TUI** (default, `tb-pr` or `tb-pr tui`) — kanban dashboard for humans.
  - **Non-interactive CLI** (`tb-pr list`, `tb-pr show`, `tb-pr prime`) — structured output for the Claude Code skill and scripts.

## Architecture

Same data layer, two presentations.

```
┌─────────────────────────────────────────────┐
│  CLI (clap subcommands)                      │
│  ├─ tui      → renders ratatui app           │
│  ├─ list     → JSON / pretty table           │
│  ├─ show     → one PR in detail              │
│  ├─ refresh  → force fetch, update cache     │
│  ├─ prime    → context dump for the skill    │
│  ├─ config   → init / show                   │
│  └─ doctor   → verifies gh auth, config      │
└───────────────┬─────────────────────────────┘
                │
┌───────────────▼─────────────────────────────┐
│  core/ — business logic                      │
│  ├─ github.rs      fetch 4 column queries    │
│  ├─ reviews.rs     per-PR reviews API        │
│  ├─ classifier.rs  size, rotting, bucketing  │
│  ├─ productive.rs  task URL extraction       │
│  ├─ cache.rs       sqlite, TTL invalidation  │
│  └─ model.rs       Pr, Column, BoardState    │
└───────────────┬─────────────────────────────┘
                │
┌───────────────▼─────────────────────────────┐
│  tui/ — ratatui app                          │
│  ├─ app.rs        state, event loop          │
│  ├─ columns.rs    5 columns side-by-side     │
│  ├─ card.rs       PR card                    │
│  ├─ footer.rs     keys, last-refresh status  │
│  └─ actions.rs    open url, keybinds         │
└─────────────────────────────────────────────┘
```

The TUI command is thin — it calls core, receives a `BoardState`, renders. The non-interactive `list --json` calls the identical core and outputs JSON. Single source of truth.

## Commands

```bash
tb-pr                        # default → TUI
tb-pr tui                    # explicit TUI launch

tb-pr list                   # pretty table (all columns flattened)
tb-pr list --column=waiting-on-me
tb-pr list --json
tb-pr list --stale-days=7    # only PRs older than N days

tb-pr show <number|url>      # detail view of one PR
tb-pr show <url> --json

tb-pr refresh                # force full fetch, update cache
tb-pr open <number|url>      # open PR in browser

tb-pr prime                  # context dump for the Claude skill
tb-pr skill install          # install SKILL.md
tb-pr config init|show
tb-pr doctor                 # check gh auth, config, cache health
```

## Columns

Five columns. Queries are scoped to `org:productiveio` — PRs outside the org are ignored.

| Column | GitHub search query | Extra processing |
|---|---|---|
| **Draft (mine)** | `is:pr is:open draft:true author:@me org:productiveio` | — |
| **In review (mine)** | `is:pr is:open draft:false author:@me org:productiveio` | Exclude PRs that are fully approved |
| **Ready to merge (mine)** | `is:pr is:open draft:false author:@me org:productiveio` | Keep only PRs with at least one approving review and no pending CHANGES_REQUESTED |
| **Waiting on me (review-requested)** | `is:pr is:open review-requested:@me org:productiveio` | — |
| **Waiting on author** | `is:pr is:open reviewed-by:@me -author:@me org:productiveio` | Keep only PRs where the last review by me is `COMMENTED` or `CHANGES_REQUESTED` AND the last commit happened before the review. Mark with 🆕 if the author has pushed new commits since. |

`@me` is native to GitHub search API — the tool works without hardcoding a username.

## Per-PR data on each card

- `title`, `url`, `repo`, `number`
- `state`: `draft` | `ready` | `approved`
- `created_at` → compute `age_days`, rotting bucket
- `additions + deletions` → size bucket
- `productive_task`: regex extracts task ID from body (`https://app.productive.io/109-productive/tasks/(\w+)`)
- `comments_count`, `review_comments_count` (shown as `💬 N`)
- `base_branch` (lightly indicated if non-default)
- `has_new_commits_since_my_review`: shown as 🆕 in the "Waiting on author" column

## Classifiers

### Rotting (border color), thresholds per column

| Column | fresh | warming | stale | rotting | critical |
|---|---|---|---|---|---|
| Draft (mine) | <3d grey | <7d — | <14d yellow | <30d orange | ≥30d red |
| In review (mine) | <1d grey | <3d green | <7d yellow | <14d orange | ≥14d red |
| Ready to merge (mine) | <1d grey | <3d green | <7d yellow | <14d orange | ≥14d red |
| Waiting on me | <4h grey | <1d green | <2d yellow | <4d orange | ≥4d red bold |
| Waiting on author | <2d grey | <5d — | <10d yellow | <14d orange | ≥14d red |

"Waiting on me" has the most aggressive rot because other humans are blocked on me.

### Size badge (additions + deletions)

- `XS` <20
- `S` <100
- `M` <300
- `L` <800
- `XL` ≥800

## TUI layout

```
┌ tb-pr ─── ilucin@productiveio ─── last refresh 1m ago ──────────────────────┐
│                                                                              │
│ Draft (mine) │ In review   │ Ready merge │ Waiting on me │ Waiting on author │
│              │             │             │ ⚠ rotting    │                   │
├──────────────┼─────────────┼─────────────┼──────────────┼───────────────────┤
│ ┌─ ai-agent ─┤ ┌ frontend ─┤ ┌ api ─────┤ ┌ api ──────┤ ┌─ mobile ─────────┤
│ │ ✍ Spike... │ │ 👀 Fix... │ │ ✅ Add... │ │ 👀 Add... │ │ 👀 Refactor ...   │
│ │ [P-1234] S │ │ [P-9999] M│ │    M     │ │    L 💬3  │ │ [P-7777] XL 💬12 │
│ │ 3d         │ │ 1d        │ │ 2d       │ │ 5d ⚠⚠    │ │ 2d 🆕             │
│ └────────────┤ └───────────┤ └──────────┤ └───────────┤ └───────────────────┤
│ ...          │ ...         │ ...         │ ...          │ ...               │
│                                                                              │
├──────────────────────────────────────────────────────────────────────────────┤
│ ←/→ h/l column   ↑/↓ j/k nav   enter=open  t=task  r=refresh  ?=help  q=quit │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Keybinds (arrows and vim both work)

- `←/→` or `h/l` — switch column (wraps)
- `↑/↓` or `j/k` — move selection within column
- `Enter` → open PR URL in browser
- `t` → open Productive task URL if present (footer note if absent)
- `r` → force refresh now
- `?` → keybind help popup
- `d` → alias for `Enter`
- `c` → copy PR URL to clipboard
- `q` / `Ctrl-C` / `Esc` → quit

### Auto-refresh

- Background `tokio::time::interval(5min)` emits `RefreshTick` events.
- UI shows a spinner in the header during fetch; never blocks input.
- On error, keep the previous state and show a ⚠ indicator with the error in the header.

## Non-interactive output

### `tb-pr list --json`

```json
{
  "user": "ilucin",
  "fetched_at": "2026-04-16T10:22:00Z",
  "columns": {
    "draft_mine": [ { "number": 370, "repo": "ai-agent", "title": "...", "url": "...", "age_days": 3, "size": "S", "productive_task_id": "1234" } ],
    "review_mine": [],
    "ready_to_merge_mine": [],
    "waiting_on_me": [],
    "waiting_on_author": []
  }
}
```

### `tb-pr list` (pretty)

Flattened table sorted by urgency (critical → fresh).

### `tb-pr prime` (for the skill)

Short markdown context dump:

```
# tb-pr live state
- 2 PRs waiting on me (oldest: 5 days)
- 4 of my PRs in review (1 ready to merge)
- 1 draft experiment (15 days old)

## Waiting on me (urgent)
- frontend#1234: "Fix billing form" (5d, L, [P-9999])
- api#567: "Add webhook endpoint" (2d, M)
```

Claude can then call `tb-pr list --json --column=waiting-on-me` for programmable access.

## Cache

- SQLite (already a toolbox dependency) under `toolbox-core`'s cache location.
- Each column has its own TTL (5 min default, configurable).
- Each PR records `refreshed_at` — displayed on the TUI as "last seen Nm ago".
- `refresh` clears TTL and forces a re-fetch.
- Cache guarantees that `list --json` from a script does not hit GitHub on every invocation.

## GitHub client

Hybrid approach: use `gh auth token` (one shell call) to obtain the token, then talk to the API directly via `reqwest`. This avoids token management and keeps the dependency tree small while still being fast.

### Parallel fetching

All four search queries and the per-PR reviews API fetches run concurrently via `tokio::join!`. Target: total under 2s for ~50 PRs.

## Config

`~/.config/tb-pr/config.toml` via `toolbox-core::config`.

```toml
[github]
org = "productiveio"
username_override = ""  # default: derived from gh auth

[refresh]
interval_minutes = 5
stale_check_on_focus = true

[columns.waiting_on_me]
rotting_thresholds_days = [0.17, 1, 2, 4]  # 4h, 1d, 2d, 4d

[productive]
org_slug = "109-productive"
task_url_pattern = 'https://app\.productive\.io/{org_slug}/tasks/(\w+)'
```

## Skill integration

`crates/tb-pr/SKILL.md` with frontmatter following the existing pattern:

```yaml
---
name: tb-pr
description: PREFERRED for checking GitHub PRs needing your attention across the Productive org. Use when the user asks about their PRs, what to review, what's blocked, or what's rotting.
---
```

Add `tb-pr` to `install.sh`. The `update-skills-cheatsheet` workflow in productive-work will pick it up automatically.

## Incremental milestones

Each milestone is one or more commits on the same draft PR, tested before moving on.

- **M1: Skeleton** — new crate, clap CLI with `doctor`, `config init`, stub `prime`. `gh auth` verified. Goal: compiles and runs.
- **M2: Data layer** — `list --json` fetches the four queries in parallel, applies classifiers and task extraction. Goal: correct data in JSON.
- **M3: Reviews API** — "Waiting on author" and "Ready to merge" filters computed correctly (last review state + commit timestamps).
- **M4: Pretty CLI** — `list` table, `show <url>`, `open`. Useful before the TUI lands.
- **M5: Cache** — sqlite, TTL, `refresh`.
- **M6: TUI basics** — ratatui boilerplate, five columns, cards, arrow nav, `q` to quit.
- **M7: TUI full** — colors (rotting), size badge, Enter → open, `t` → task, `r` → manual refresh, auto-refresh tick.
- **M8: Skill + install.sh** — `prime`, `SKILL.md`, wired into `install.sh --all`.
- **M9: Polish** — doctor checks, error states, help popup, README, docs.

## Decisions already made

1. **"Waiting on author" edge case** — PR stays in the column until someone re-requests review. If the author pushes new commits since my review, show a 🆕 indicator.
2. **Scope** — everything outside `org:productiveio` is ignored.
3. **Config** — uses `toolbox-core::config` for consistency with other tb tools.
4. **"Ready to merge (mine)"** — separate fifth column for approved, non-merged PRs authored by me.
5. **Keybinds** — arrows and vim-style (`hjkl`) both work.
