# tb-sem Branch Filtering Rethink

## Problem

tb-sem currently stores a single `branch` per project in the config (e.g. `branch: "main"`). All commands that list workflows/pipelines filter to this one branch. This is architecturally wrong because different branches serve fundamentally different purposes:

- **app project:** `develop` → deploys to endtoend/latest environments, `release/*` → deploys to edge, `master` → deploys to production
- **api project:** different branching model entirely

Storing one "default branch to watch" means you're always missing data from other important branches. There's no single correct answer for which branch to show.

## Discovered during

Discovered while working on [config-init-frictions](../tb-bug-config-init-frictions/). The `branch` field is being removed from project config as part of that feature. This feature picks up the question: what should replace it?

## Possible directions

1. **No default branch** — commands show all branches by default, `--branch` flag filters when needed
2. **Branch watch list** — store multiple branches per project (e.g. `["main", "develop", "release/*"]`)
3. **Branch patterns** — glob-style patterns per project (e.g. `release/*`)
4. **Context-dependent defaults** — different commands care about different branches (deploys → production branch, CI status → develop)

## Out of scope

- Changing how Semaphore's API works
- Multi-org support

## Open questions

- What's the most common use case? "Show me everything" vs "show me what matters"?
- Should `prime` (the overview command) aggregate across branches or show per-branch status?
- How do other CI tools handle this? (e.g. GitHub Actions shows all branches by default)
