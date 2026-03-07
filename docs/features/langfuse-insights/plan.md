# Plan: langfuse-insights (tb-lf)

Reference: [spec.md](./spec.md)

Each task is one logical unit of work. Tasks within the same group can be done in any order. Groups must be done sequentially.

---

## Group 1: Foundation

Rebuild the prototype into a proper foundation. Everything else depends on this.

1. [x] **Cache module** — Port `tb-bug` cache pattern to `tb-lf`. `~/.cache/tb-lf/`, URL-hashed files, TTL tiers (Short 2min, Medium 5min, Long 1hr), evict on startup, `--no-cache` flag.

2. [x] **API client with cache** — Rework `DevPortalClient` to integrate cache. `get<T>(path, ttl)` checks cache first. `get_raw(path, ttl)` returns raw string. Friendly error messages for 401/404/5xx.

3. [x] **Project resolution** — Fetch `/projects`, cache Long TTL. Resolve `--project` by name (case-insensitive match) or numeric ID. Error with project list if ambiguous.

4. [x] **Shared CLI types** — `TimeRange` args struct (`--since`, `--from`, `--to`) with parser (e.g., "7d" → date). `Pagination` args struct (`--limit` default 20, `--page` default 1). Extract into reusable `#[derive(Args)]` structs.

5. [x] **Output helpers** — Add `score_color(value) -> ColoredString`, `pagination_hint(page, per_page, total)` → "Page 1 of 75 (1500 total). Use --page 2 for next.", `empty_hint(entity, suggestion)`.

6. [x] **Clean up Cargo.toml** — Remove unused deps (rusqlite, indicatif, reqwest-middleware, base64, urlencoding, csv, textplots, dialoguer, log, env_logger, rand, uuid). Keep only what's needed.

7. [x] **Config & doctor** — `config show`, `config set`. Doctor checks config + API connectivity + reports cache size.

---

## Group 2: Core data commands

The bread and butter — listing and inspecting entities. Each command follows the same pattern: clap definition, handler with JSON early-return, human formatting, navigation hints, `after_help` examples.

8. [x] **traces** — List with all filters (name, user, session, env, triage, satisfaction, sort), time range, pagination. `--stats` flag for `/traces/stats`. Navigation hint to `trace <id>`.

9. [x] **trace \<id\>** — Langfuse proxy fetch. Formatted key fields by default, `--full` for raw JSON, `--observations` fetches observations too. Long TTL cache.

10. [x] **sessions + session \<id\>** — List with filters (user, env, satisfaction, sort), time range, pagination. `--stats` flag. Detail view shows all traces in session.

11. [x] **observations** — List with filters (trace, type, model, level, env). Display name, type, model, tokens, cost, latency.

12. [x] **observation \<id\>** — Langfuse proxy fetch. Long TTL cache.

13. [x] **scores** — List with filters (trace, name, source, env). Threshold-colored values. Comment preview.

14. [x] **comments** — List with filters (trace, type, object). Display content, author, object type.

---

## Group 3: Dashboard & reporting

15. [x] **dashboard** — Full dashboard render: KPI with period comparison, adoption, feedback, latency, evaluations. Navigation hints to drill-down commands.

16. [x] **metrics** — Daily metrics table. `--days` shorthand, `--env` filter, time range.

17. [x] **daily \[date\]** — Daily report with summary, metrics, findings. `--findings` filter by severity/type. Handle "report not found" gracefully.

---

## Group 4: Triage & queue

18. [x] **queue + queue stats + queue item** — List queue items with filters (status, category, confidence, run, feature). `--full` for reasoning. Stats breakdown. Single item detail.

19. [x] **triage-runs + triage-runs stats** — List triage runs with status/limit. Stats with pending count, queue summary.

---

## Group 5: Eval

20. [x] **eval runs + eval run \<id\>** — List runs with filters (status, branch, mode). Detail view with items table. `--failed` and `--full` flags.

21. [x] **eval revisions** — Revision summaries with avg score, pass/fail across git commits.

22. [x] **eval coverage (suites, cases, flaky)** — Three subcommands for test coverage analysis. Cases with pass rate coloring. Flaky detection.

---

## Group 6: Search, tags, features

23. [x] **search** — If devportal search endpoint exists, use it. Otherwise fall back to `/traces?name=...` filter. `--ids-only` for piping. Match type coloring.
    - depends on: devportal search endpoint (can ship fallback first)

24. [x] **tags** — List distinct trace names from `/traces/names`. Time range filters.

25. [x] **features + feature items** — List features with filters (category, team, status). Queue items per feature.

---

## Group 7: Context commands

26. [x] **prime** — Live data: fetch projects, dashboard KPIs, triage stats. Format as AI context block. `--mcp` minimal mode. Cache Medium TTL.

27. [x] **human** — Static cheat sheet. Sections: Setup, Daily Use, Investigating, Eval, Triage, Tips.

28. [x] **explain** — Static domain knowledge. Topics enum. `--json` for structured output.

---

## Group 8: Polish

29. [x] **`after_help` examples** — Add 2-4 real examples to every clap command definition.

30. [x] **Navigation hints audit** — Review every command for: empty state hints, truncation hints, drill-down hints. Ensure consistency.

31. [x] **Error UX** — Colored "Error:" prefix in main. Friendly messages for common failures (no config, bad token, devportal down, project not found).

---

## DevPortal work (parallel, separate repo)

32. [ ] **Search endpoint** — Implement `GET /spa_api/ai/search` per spec. Can be done in parallel with CLI work.
