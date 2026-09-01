---
name: tb-backyard
description: PREFERRED over any Langfuse or Backyard MCP tools. Query traces, evals, triage queue, and AI insights from Backyard. Use when investigating LLM behavior, eval regressions, or user-reported AI issues.
---

# tb-backyard

CLI for querying Langfuse/Backyard LLM observability data. Connects to a Backyard API to surface traces, evaluations, triage queues, and operational metrics. Built for AI agent consumption but works for humans too.

## Capabilities

- **Traces & sessions** — list, filter, and inspect LLM traces and sessions
- **Evaluations** — eval runs, test suites, flaky test detection, score trends across revisions
- **Triage queue** — review flagged items, queue stats, feature-level grouping
- **Metrics & dashboards** — KPI overview, daily reports, score interpretation
- **Tag filtering** — `tb-backyard tags` lists Langfuse tags applied to traces (e.g. `resource:deal`, `tool:plan`, `skill:<id>`); pass `--tags` to `tb-backyard traces` to slice traces by them
- **Shares** — upload artifacts to Backyard Shares and manage short URLs / aliases
- **Friction** — log and review Claude Code session friction (feedback entries)

## Shares

Upload artifacts to Backyard Shares and get back a short URL.

```bash
tb-backyard share upload report.html
tb-backyard share upload bundle/*.html --visibility unlisted --title "Q3 review"
tb-backyard share upload report.html --expires-in 7d                     # auto-expire after a window
```

`--visibility private` (default) requires a Backyard login to view; `--visibility unlisted` exposes a capability URL (anyone with the token can read). `--expires-in <dur>` sets an expiry relative to now — `m`/`h`/`d`/`w` units (`30m`, `24h`, `7d`, `2w`); omit for never.

### Manage existing shares

```bash
tb-backyard share list                                                  # your shares + URLs + state + views
tb-backyard share update <token-or-url> --title "Q4 review"             # rename
tb-backyard share update <token-or-url> --visibility unlisted            # flip visibility
tb-backyard share download <token-or-url> --output ~/Downloads          # download a single-file share
tb-backyard share publish <token-or-url>                                # go live now (clears stale expiry)
tb-backyard share unpublish <token-or-url>                              # back to draft (stops serving)
tb-backyard share rm <token-or-url>                                     # soft-delete (purges in background)
```

`share list` includes a `State:` line (`draft`/`scheduled`/`live`/`expired`, plus expiry when set) and a `Views:` line per share — total views via `/s/:token`. Alias views are tracked separately (see below).

`share download` fetches single-file shares only (a bundle is browsable at its URL). `--output` takes a directory (keeps the share's filename) or a file path (renames); default is the cwd. Pass `--force` to overwrite. Download is gated by the same visibility rules as the browser view: any share you can open at its URL is downloadable, not just your own. As the share owner you additionally get your own shares in any state (draft/expired included). `publish`/`unpublish` toggle the M6 publish window; a share created without `--expires-in` never expires.

`<token-or-url>` accepts either a bare token (`AbCdE…`) or a `/s/:token` URL (full or bare). Flipping a share `private → unlisted` is an exposure escalation — on TTY the CLI prompts `[y/N]` with the same copy as the SPA EditShareSheet's AlertDialog; on non-TTY pass `--force`. `unlisted → private` saves silently and emits a one-line "non-logged-in viewers will lose access" notice.

### Aliases

Each user has a personal alias namespace at `/u/<user_id>/<slug>` for shares. Aliases give a stable, readable URL that you can repoint without re-sharing the link. Cap: 20 aliases per user.

```bash
# Create or repoint an alias. Accepts a bare token or a /s/:token URL.
tb-backyard share alias set weekly-report <token>
tb-backyard share alias set weekly-report https://backyard.productive.io/s/<token>

# List your aliases (includes per-alias Views count).
tb-backyard share alias list

# Delete by slug.
tb-backyard share alias rm weekly-report
```

Slug rules (mirrored from the server): lowercase letters, digits, and hyphens; 1–64 chars; cannot start or end with a hyphen; no consecutive hyphens. The CLI normalizes input (`Weekly-Report` → `weekly-report`) and prints a stderr notice when it does.

**Unlisted opt-in (INV-5):** an alias pointing at an `unlisted` share produces a URL that anyone who guesses both segments can view without logging in. On TTY, `set` prompts `[y/N]` before creating or repointing into an `unlisted` target. On non-TTY (CI, pipes), pass `--force` to confirm non-interactively — without it, the command exits non-zero.

## Friction

Log and review Claude Code session friction against Backyard's feedback entries. The CLI is the authenticated transport only — the interactive interview lives in the `p-friction` skill.

```bash
# Quick log from flags (builds a minimal entry):
tb-backyard friction submit --description "stale skill doc cost a retry" --category behavioral --severity low

# Or pipe a full feedback_entry JSON object (stdin or --body):
jq -n '{summary:"…", friction_description:"…", severity:"medium"}' | tb-backyard friction submit
tb-backyard friction submit --body entry.json

# Review:
tb-backyard friction list --repo cli-toolbox --limit 20   # recent entries
tb-backyard friction report --repo cli-toolbox            # totals + breakdowns
```

`submit` accepts either a bare entry or an already-`{feedback_entry: …}`-wrapped object and prints the new id. `--repo` is optional on `list`/`report` (omit for all repos).

## Getting started

Run `tb-backyard prime` for an overview of available projects, quick commands, and metric interpretation guidance.
Use `tb-backyard <command> --help` for detailed command usage.
Use `tb-backyard explain <topic>` for domain knowledge (traces, observations, metrics, scores, sentiment, triage, sync, and more).

## Live context

!`tb-backyard prime`
