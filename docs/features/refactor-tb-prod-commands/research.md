# Research: tb-prod Command Structure

## Current Architecture

### CLI Framework
- **clap derive macros** — `Cli` struct + `Commands` enum in `main.rs`
- All subcommand args are inline fields on enum variants (no separate `*Args` structs)
- Two nested subcommand examples already exist: `Cache { action: CacheAction }` and `Config { action: ConfigAction }`

### Key Files
- `src/main.rs` — CLI definition (lines 11-173), dispatch match (lines 188-350), `read_text_input` helper
- `src/commands/mod.rs` — declares 11 command submodules
- `src/api.rs` — `ProductiveClient` with generic CRUD + resource-specific wrappers
- `src/cache.rs` — local JSON cache with TTL, name resolution helpers
- `src/output.rs` — `render_json()`, `relative_time()`, `truncate()`
- `src/error.rs` — `TbProdError` enum via thiserror
- `src/config.rs` — config loading from secrets.toml + config.toml + env

### Command Modules (src/commands/)
| File | Functions | Maps to |
|---|---|---|
| `tasks.rs` | `run()` | `tasks` — list with filters |
| `task.rs` | `run()` | `task` — single detail |
| `task_create.rs` | `run()` | `create` |
| `task_update.rs` | `run()` | `update` |
| `task_comment.rs` | `run()` | `comment` |
| `todos.rs` | `list()`, `create()`, `update()` | `todos`, `todo-create`, `todo-update` |
| `project.rs` | `run()` | `project` |
| `prime.rs` | `run()` | `prime` |
| `cache_cmd.rs` | `sync()`, `clear()` | `cache sync/clear` |
| `doctor.rs` | `run()` | `doctor` |
| `config_cmd.rs` | `init()`, `show()` | `config init/show` |

### Shared Patterns
- Every command takes `&ProductiveClient` as first arg, `json: bool` for output mode
- Name resolution (project, person, status, task list) happens in `main.rs` before dispatch
- All commands return `crate::error::Result<()>`
- No cross-command imports — commands are independent

### Existing Nesting Pattern
`cache` and `config` already use nested enums:
```rust
Cache { #[command(subcommand)] action: CacheAction }
enum CacheAction { Sync, Clear }
```
This is exactly the pattern we'll replicate for `task`, `todo`, `comment`, `project`.

### Inconsistencies to Address
- Name resolution is duplicated: `task.rs` has private `resolve_name()`/`resolve_person()` that duplicate `Cache` methods
- `create`/`update` status resolution is duplicated inline in `main.rs`
- `todos.rs` groups 3 operations because they share `TodoRow` — this grouping is natural and can stay

### Dependencies
- `tb-prod` depends on `toolbox-core` (just for `version()`)
- Self-contained binary crate, no cross-crate command logic
