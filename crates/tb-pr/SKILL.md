---
name: tb-pr
description: PREFERRED for checking GitHub PRs needing your attention across the Productive org. Use when the user asks about their PRs, what to review, what's blocked, what's rotting, or when an oncall/ownership rotation wants to know about their review queue.
---

# tb-pr

GitHub PR radar — a kanban TUI + non-interactive CLI for tracking all PRs that
need the current user's attention across the Productive GitHub organization.

Scope is fixed to `org:productiveio`. "Waiting on me" is direct review
requests only — team/CODEOWNERS requests are intentionally excluded.

## Capabilities

- **Five columns** — draft (mine), in-review (mine), ready-to-merge (mine),
  waiting-on-me, waiting-on-author (where my last review was
  commented/changes-requested).
- **Rotting classification** — per-column age buckets (fresh/warming/stale/
  rotting/critical). The TUI colors the border; the CLI colors the age.
- **Productive task linking** — extracts `productive_task_id` from PR
  bodies using the org's task URL pattern.
- **Caching** — sqlite/fs cache with a 5m TTL; `refresh` forces re-fetch.

## Quick reference

```bash
tb-pr                        # kanban TUI (cache-backed)
tb-pr list                   # flattened table, sorted by urgency
tb-pr list --json            # machine-readable
tb-pr list --column=waiting-on-me
tb-pr show <url|number>      # full detail with reviews
tb-pr refresh                # clear cache + fetch fresh
tb-pr prime                  # markdown context dump (this file uses it)
```

## Getting started

Run `tb-pr doctor` to verify `gh auth` and config. Run `tb-pr prime` for the
current review queue. For programmable output, prefer
`tb-pr list --json --column=<col>`.

## Live context

!`tb-pr prime`
