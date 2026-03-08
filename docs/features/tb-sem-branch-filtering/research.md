# Research: tb-sem Branch Filtering

## Current State

### Branch in config: never existed
`ProjectConfig` only has `id: String`. The idea.md was slightly off — there was never a `branch` field in tb-sem config. Branch is purely a runtime/CLI concern today.

### Existing `--branch` flag
Only `runs` has it: `tb-sem runs <project> --branch <name>`. It maps directly to the Semaphore API's `&branch_name=` query parameter. Fully functional.

### Commands that call `list_workflows` without branch filtering
| Command | Would benefit from `--branch`? | Notes |
|---|---|---|
| `flaky` | Yes | Flaky test analysis is often branch-specific |
| `history` | Yes | Test history varies by branch |
| `triage` | Yes | Triage latest failure on a specific branch |
| `deploys` | Marginal | Deploys are usually tied to specific branches already |
| `prime` | Low | Status overview across all projects |

### Commands where branch is irrelevant
`pipeline`, `failures`, `logs`, `tests`, `promotions`, `compare` — these take a pipeline ID directly, so the branch is already implied.

## API Layer

**Workflow listing** (`GET /plumber-workflows`):
- Accepts `branch_name` as a query parameter — server-side filter
- Also accepts `created_after` and `created_before` for time ranges
- Supports pagination via `Link` headers

**Pipeline listing** (`GET /pipelines?wf_id=...`):
- Scoped to a workflow — no branch filter needed
- Both `Workflow` and `Pipeline` structs carry `branch_name` in their response, so client-side filtering is also possible

## Pattern for Adding `--branch`

Already established in `runs`:
1. Add `#[arg(long)] branch: Option<String>` to the CLI variant in `main.rs`
2. Thread it through the command's `run()` signature
3. Pass it to `list_workflows(project_id, branch.as_deref(), ...)`

Straightforward mechanical change for each command.

## Key Insight

The original idea framed this as "rethink the filtering strategy." But the research shows the architecture is actually fine — branch is a runtime filter, not a config value. The real gap is just that `--branch` only exists on `runs` and not on the other workflow-based commands. This is a small, mechanical fix, not an architectural rethink.

## Recommendation

Add `--branch` flag to `flaky`, `history`, `triage`, and optionally `deploys` and `prime`. No config changes needed. Follow the existing pattern from `runs`.
