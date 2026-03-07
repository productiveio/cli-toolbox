# Refactor tb-prod Commands

## Problem

tb-prod has a flat command structure that doesn't scale. Tasks have 5 top-level commands (`tasks`, `task`, `create`, `update`, `comment`), todos have 3 (`todos`, `todo-create`, `todo-update`), and `create`/`update` aren't scoped to any model. Adding new models (deals, projects, comments on non-task entities) will make the CLI increasingly confusing.

## Solution

Group commands under their model name as subcommands:

- `tb-prod task list|show|create|update` (was: `tasks`, `task`, `create`, `update`)
- `tb-prod todo list|create|update` (was: `todos`, `todo-create`, `todo-update`)
- `tb-prod comment add` (was: `comment` — now model-scoped, can attach to tasks, deals, etc.)
- `tb-prod project show|list|search` (was: `project` — room to grow)
- Utility commands stay flat: `prime`, `cache`, `doctor`, `config`

## Scope

- **In:** Restructure CLI commands into `<model> <action>` pattern. Clean break, no backwards compatibility aliases.
- **Out:** No new functionality — this is purely a structural refactor.

## Success

`tb-prod help` shows a clean list of model groups + utilities. Adding a new model means adding one subcommand group without polluting the top level.
