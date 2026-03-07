# Feature: langfuse-insights
**Current phase:** Execute (complete — ready for QA)
**Started:** 2026-03-07

## Progress
- [x] Idea — CLI for reading AI observability data from devportal
- [x] Research — Endpoint inventory, old lfi patterns, workspace conventions, command structure
- [x] Prototype — Core data path validated: config, auth, 8 commands working against local devportal
- [x] Spec — 14 requirement areas, command structure, cache strategy, search endpoint spec
- [x] Plan — 32 tasks across 8 groups
- [x] Execute — All 31 CLI tasks complete (31/31). Task 32 is devportal-side work.
- [ ] QA

## Execution progress
- [x] Group 1: Foundation (7/7)
- [x] Group 2: Core data commands (7/7)
- [x] Group 3: Dashboard & reporting (3/3)
- [x] Group 4: Triage & queue (2/2)
- [x] Group 5: Eval (3/3)
- [x] Group 6: Search, tags, features (3/3)
- [x] Group 7: Context commands (3/3)
- [x] Group 8: Polish (3/3)
- [ ] Task 32: DevPortal search endpoint (separate repo)

## Commands implemented (25 total)
traces, trace, sessions, session, observations, observation, scores, comments,
dashboard, metrics, daily, queue, queue-stats, queue-item, triage-runs,
triage-runs-stats, eval (runs/run/revisions/suites/cases/flaky), search, tags,
features, feature-items, prime, human, explain, config (show/set), doctor
