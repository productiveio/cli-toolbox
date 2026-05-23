# CLI Toolbox

## Crates and Skills

- Each crate ships a paired `crates/<tool>/SKILL.md` that becomes a Claude Code skill — that's how LLM agents downstream learn to use the binary. The SKILL.md is the primary user-facing documentation; treat it as a first-class artifact, not a side note.
- SKILL.md is **embedded in the binary** via `include_str!("../SKILL.md")` and written to `~/.claude/skills/<tool>/SKILL.md` by `<tool> skill install` (which `scripts/install.sh --with-skill` calls). **Editing SKILL.md is therefore a user-visible change** — it only reaches users via a new tagged release.

## Git Workflow

- Create a feature branch and open a draft PR for review. Do not commit directly to `main`.
- **Before publishing a tool**, always check for uncommitted changes in its crate directory and commit them first. The bump script only commits the version change — any pending code changes will be left out of the tagged release.
- **Version bumps go in PRs, tags go on main.** `scripts/bump.sh` updates the version and commits — safe to use on any branch. Tags (`<tool>-v<version>`) are created only on `main` after the PR merges, because tags trigger CI release builds. Never push tags from feature branches.

## When to Bump

- Bump when the change is user-visible: new command, new flag, changed output, **changed SKILL.md text**, bug fix a user would notice.
- Don't bump for pure refactors, internal helper dedup, dependency-only changes, or test cleanups. The convention is "bumps go in PRs", not "every PR bumps".
- When in doubt: if a user installing the new release would notice anything different (including agents reading the updated SKILL.md), bump.

## Release Flow

- For the end-to-end flow (bump → push commits → tag-one-at-a-time → monitor CI → install locally), prefer the bundled `/cli-toolbox_publish` skill at `.claude/skills/cli-toolbox_publish/SKILL.md` over running the steps by hand. The "tags must be pushed one at a time" gotcha (GitHub Actions doesn't trigger per-tag workflows on batched pushes) bites manual flows.
- After a release lands, run `scripts/install.sh --reinstall --with-skill <tool>` to pick up the new binary **and** the freshly embedded SKILL.md locally. Skipping `--with-skill` leaves stale skill text in `~/.claude/skills/<tool>/`.

## Bugs and Issues

- "Pre-existing" issues are still issues. Don't dismiss or deprioritize a bug just because it wasn't caused by the current change.
- When encountering a broken behavior, investigate the root cause and offer to fix it. If it's quick, just fix it. If it's complex, propose a plan.
- Don't waste time proving something is pre-existing — spend that time understanding and fixing it instead.

## Feature Artifacts

- When a feature is complete and there's nothing left to do, offer to delete the feature's `docs/features/<name>/` directory if the markdown files have no ongoing reference value. The spec/plan served their purpose during development — don't keep them around as clutter.
