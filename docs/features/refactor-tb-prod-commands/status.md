# Feature: refactor-tb-prod-commands
**Current phase:** QA
**Started:** 2026-03-07

## Progress
- [x] Idea — Group flat commands under model subcommands (`task`, `todo`, `comment`, `project`)
- [x] Research — Mapped CLI structure: clap derive, flat Commands enum in main.rs, nested pattern already exists for cache/config
- [ ] Prototype — skipped
- [x] Spec — Full command mapping, enum definitions, file change scope defined
- [x] Plan — 4 tasks: restructure enum, update dispatch, update prime text, verify
- [x] Execute — All 4 tasks done, build + clippy clean
- [x] QA — Code review clean, no behavior changes, no issues introduced
