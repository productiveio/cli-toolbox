# Plan: Refactor tb-prod Commands

## Tasks

1. [ ] Restructure `Commands` enum and add subcommand enums in `main.rs`
   - Replace flat variants with nested `Task { action: TaskAction }`, `Todo { action: TodoAction }`, `Comment { action: CommentAction }`, `Project { action: ProjectAction }`
   - Define `TaskAction`, `TodoAction`, `CommentAction`, `ProjectAction` enums
   - Keep `CacheAction` and `ConfigAction` as-is

2. [ ] Update dispatch match in `main()` to use nested matches
   - `Commands::Task { action } => match action { TaskAction::List { .. } => ..., ... }`
   - Same for Todo, Comment, Project
   - All resolution logic (cache, name resolution) stays in main.rs, just moves into nested arms
   - depends on: 1

3. [ ] Update `prime.rs` command reference text to reflect new command structure
   - depends on: 1

4. [ ] Verify: `cargo build`, `cargo clippy`, run `tb-prod help` and `tb-prod task help` to confirm output
   - depends on: 1, 2, 3
