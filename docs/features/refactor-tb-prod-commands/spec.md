# Spec: Refactor tb-prod Commands

## Goal

Replace the flat command structure with model-scoped subcommand groups: `tb-prod <model> <action>`. Clean break, no backwards compatibility.

## Command Structure

### Before → After

```
tb-prod tasks ...             →  tb-prod task list ...
tb-prod task <id>             →  tb-prod task show <id>
tb-prod create ...            →  tb-prod task create ...
tb-prod update <id> ...       →  tb-prod task update <id> ...
tb-prod comment <id> ...      →  tb-prod comment add <id> ...
tb-prod todos <task_id>       →  tb-prod todo list <task_id>
tb-prod todo-create ...       →  tb-prod todo create ...
tb-prod todo-update ...       →  tb-prod todo update ...
tb-prod project <project>     →  tb-prod project show <project>
tb-prod prime                 →  tb-prod prime          (unchanged)
tb-prod cache sync|clear      →  tb-prod cache sync|clear (unchanged)
tb-prod doctor                →  tb-prod doctor         (unchanged)
tb-prod config init|show      →  tb-prod config init|show (unchanged)
```

### New Commands Enum

```rust
enum Commands {
    /// Manage tasks
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// Manage todos
    Todo {
        #[command(subcommand)]
        action: TodoAction,
    },
    /// Manage comments
    Comment {
        #[command(subcommand)]
        action: CommentAction,
    },
    /// Project information
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// AI context dump
    Prime,
    /// Manage cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Health check
    Doctor,
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}
```

### Subcommand Enums

```rust
enum TaskAction {
    /// List tasks with filters
    List {
        #[arg(long)] task_list: Option<String>,
        #[arg(long)] project: Option<String>,
        #[arg(long)] assignee: Option<String>,
        #[arg(long)] category: Option<String>,
        #[arg(long)] search: Option<String>,
    },
    /// Show single task detail
    Show { id: String },
    /// Create a new task
    Create {
        #[arg(long)] title: String,
        #[arg(long)] project: String,
        #[arg(long)] task_list: String,
        #[arg(long)] status: Option<String>,
        #[arg(long)] assignee: Option<String>,
        #[arg(long)] description: Option<String>,
        #[arg(long)] description_file: Option<String>,
        #[arg(long)] description_stdin: bool,
        #[arg(long)] due_date: Option<String>,
    },
    /// Update a task
    Update {
        id: String,
        #[arg(long)] status: Option<String>,
        #[arg(long)] title: Option<String>,
        #[arg(long)] assignee: Option<String>,
    },
}

enum TodoAction {
    /// List todos for a task
    List { task_id: String },
    /// Create a todo on a task
    Create {
        task_id: String,
        #[arg(long)] title: String,
        #[arg(long)] assignee: Option<String>,
    },
    /// Update a todo
    Update {
        todo_id: String,
        #[arg(long)] done: Option<bool>,
        #[arg(long)] title: Option<String>,
    },
}

enum CommentAction {
    /// Add a comment to a task
    Add {
        /// Task ID (future: any commentable entity ID)
        id: String,
        body: Option<String>,
        #[arg(long)] body_file: Option<String>,
        #[arg(long)] body_stdin: bool,
    },
}

enum ProjectAction {
    /// Show project context — statuses, task lists
    Show { project: String },
}
```

## File Changes

### main.rs
- Replace `Commands` enum with new nested structure
- Replace flat `match cli.command` arms with nested matches:
  ```rust
  Commands::Task { action } => match action {
      TaskAction::List { ... } => { ... }
      TaskAction::Show { id } => { ... }
      TaskAction::Create { ... } => { ... }
      TaskAction::Update { ... } => { ... }
  },
  ```
- Name resolution logic stays in main.rs (already the established pattern)
- `read_text_input` stays as-is

### commands/mod.rs
- No changes needed — module names stay the same (`tasks`, `task`, `task_create`, `task_update`, `task_comment`, `todos`, `project`)

### Command handler files
- No changes — function signatures and logic remain identical
- The refactor is purely in the CLI layer (main.rs)

## Help Output

Target `tb-prod help`:
```
Productive.io CLI — compact, AI-optimized

Usage: tb-prod [OPTIONS] <COMMAND>

Commands:
  task      Manage tasks
  todo      Manage todos
  comment   Manage comments
  project   Project information
  prime     AI context dump — quick command reference
  cache     Manage cache
  doctor    Health check
  config    Manage configuration
  help      Print this message or the help of the given subcommand(s)
```

Target `tb-prod task help`:
```
Manage tasks

Usage: tb-prod task <COMMAND>

Commands:
  list     List tasks with filters
  show     Show single task detail
  create   Create a new task
  update   Update a task
```

## Open Questions

None — scope is clear, pattern exists in codebase, clean break confirmed.

## Update prime command

The `prime` command outputs a static command reference for AI context. After the refactor, update the static text in `prime.rs` to reflect the new command structure.
