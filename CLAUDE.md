# CLI Toolbox

## Git Workflow

- Create a feature branch and open a draft PR for review. Do not commit directly to `main`.
- **Before publishing a tool**, always check for uncommitted changes in its crate directory and commit them first. The bump script only commits the version change — any pending code changes will be left out of the tagged release.
- **Version bumps go in PRs, tags go on main.** `scripts/bump.sh` updates the version and commits — safe to use on any branch. Tags (`<tool>-v<version>`) are created only on `main` after the PR merges, because tags trigger CI release builds. Never push tags from feature branches.

## Bugs and Issues

- "Pre-existing" issues are still issues. Don't dismiss or deprioritize a bug just because it wasn't caused by the current change.
- When encountering a broken behavior, investigate the root cause and offer to fix it. If it's quick, just fix it. If it's complex, propose a plan.
- Don't waste time proving something is pre-existing — spend that time understanding and fixing it instead.

## Feature Artifacts

- When a feature is complete and there's nothing left to do, offer to delete the feature's `docs/features/<name>/` directory if the markdown files have no ongoing reference value. The spec/plan served their purpose during development — don't keep them around as clutter.
