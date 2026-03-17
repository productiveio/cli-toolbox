# CLI Toolbox

## Git Workflow

- Commit directly to `main` — no branches or PRs needed for now.
- **Before publishing a tool**, always check for uncommitted changes in its crate directory and commit them first. The bump script only commits the version change — any pending code changes will be left out of the tagged release.

## Bugs and Issues

- "Pre-existing" issues are still issues. Don't dismiss or deprioritize a bug just because it wasn't caused by the current change.
- When encountering a broken behavior, investigate the root cause and offer to fix it. If it's quick, just fix it. If it's complex, propose a plan.
- Don't waste time proving something is pre-existing — spend that time understanding and fixing it instead.

## Feature Artifacts

- When a feature is complete and there's nothing left to do, offer to delete the feature's `docs/features/<name>/` directory if the markdown files have no ongoing reference value. The spec/plan served their purpose during development — don't keep them around as clutter.
