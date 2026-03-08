# Feature: tb-sem-branch-filtering
**Current phase:** QA
**Started:** 2026-03-08

## Progress
- [x] Idea — Single branch per project is wrong; need to rethink filtering strategy
- [x] Research — Simpler than expected: just add --branch flag to more commands
- [ ] Prototype — skipped
- [x] Spec — --branch on 4 commands, new `branches` command, prime perf fix
- [x] Plan — 4 tasks: --branch flag, branches command, prime concurrency, prime over-fetch fix
- [x] Execute — All 4 tasks implemented
- [x] QA — Clean: no issues found, implementation matches spec
