---
name: e2e-triage
description: Investigate e2e test failures — find what failed on Semaphore, understand the tests, and diagnose root cause (code regression, infrastructure issue, or flag release).
argument-hint: "<optional: 'last run', 'why is tasks failing', workflow URL, or specific test name>"
allowed-tools: Bash(tb-sem *), Bash(tb-bug *), mcp__semaphoreci-dev__*, mcp__claude_ai_Slack__slack_search_public, mcp__claude_ai_Slack__slack_read_channel, Read, Grep, Glob, Bash(git *), Bash(source .envrc && node scripts/backoffice-client.mjs *), Agent, AskUserQuestion
---

# E2E Test Failure Triage

Investigate failing e2e tests on Semaphore CI. Finds failures, reads test code, and diagnoses root cause across the e2e-tests, frontend, and api repos.

## Usage

```
/e2e-triage                                    # Investigate latest failures
/e2e-triage last 3 runs                        # Check last 3 workflow runs
/e2e-triage why is budgeting failing            # Focus on a specific product area
/e2e-triage <workflow-url-or-id>               # Investigate a specific workflow
```

## Project Context

```
Semaphore projects:
  e2e-tests  (branch: master)
  frontend   (branch: develop)
  api        (branch: develop → latest)
```

## Repos

- **e2e-tests** (`repos/e2e-tests`) — Cucumber.js + Puppeteer test suite. Features in `features/`, step definitions in `step-definitions/`, test org config in `data/organizations.yml`
- **frontend** (`repos/frontend`) — Ember.js frontend monorepo
- **api** (`repos/api`) — Rails backend API

Ensure repos are up to date (`git fetch origin`) before investigating.

## How Endtoend Deploys Work

The e2e tests run against the **endtoend environment**, deployed independently from the e2e-tests repo. Deploy timing is critical for diagnosis.

### Frontend deploys
- **Branch:** `develop`
- **Mechanism:** Auto-promotion after successful CI on develop
- **Frequency:** Every successful develop merge (multiple times per day)
- **Duration:** ~4 minutes

### API deploys
- **Branch:** `latest` (NOT develop directly)
- **Mechanism:** A scheduled job rebases `latest` from `develop` ~hourly. CI runs on `latest`, then auto-promotes "deploy to endtoend".
- **Deploy steps:** schema migrations → data migrations → ECS deploy
- **Frequency:** ~hourly (scheduled)
- **Duration:** ~10-12 minutes

### Flag releases
- Search Slack `#release` channel for "SYSTEM RELEASE" announcements on the day of the failure
- Or query backoffice for recently system-released flags on the endtoend environment

## Workflow

### Phase 1: Find Failures

1. Start with automated triage of the e2e-tests project on master. If that gives enough context, skip to Phase 3.
2. If you need more detail or a specific run, list recent failed runs and drill into the target pipeline.
3. Get the failure summary and structured test results for the failed pipeline.
4. For deeper inspection, look at individual job logs (errors only, or full).

**Timezone note:** Slack shows CET/CEST. Semaphore returns UTC by default. Convert carefully when cross-referencing.

### Phase 2: Classify Failures

Extract failed scenario names, feature file paths, error messages, and retry counts.

| Pattern | Likely cause |
|---------|-------------|
| `TimeoutError`, `waiting for selector`, `element not found` | UI change, env slowness, deploy overlap, **or hidden API error** (see caveat below) |
| `net::ERR_CONNECTION_REFUSED`, `502`, `503`, `ECONNRESET` | Infrastructure / env issue / deploy overlap |
| `AssertionError`, expected vs actual mismatch | Code regression |
| `403 Forbidden`, `401 Unauthorized` | Permission/auth change |
| Most/all tests fail simultaneously | Environment is down or mid-deploy |
| Only specific feature area fails | Targeted regression or flag change |
| Test fails N/N retries then passes in next run | Deploy overlap (see Phase 4d) |
| Many scenarios need retries + `USE_CHECK=true` → exit 1 | Flaky; all passed on retry but USE_CHECK mode fails the job |

**Caveat: e2e logs hide API errors.** The test runner only reports the Puppeteer-level symptom (e.g. `TimeoutError: waiting for selector`), not the underlying cause. A 500 error from the API will surface as a generic timeout — the logs won't show the 500 or its stack trace. When you see timeouts that suggest an API issue (hanging requests, page not settling), **always check Bugsnag for API errors in the failure time window** before investigating code. This is often the fastest path to root cause.

### Phase 3: Understand the Tests

For each failing test:

1. **Read the feature file** from `repos/e2e-tests/features/` matching the path from logs
2. **Identify what it tests** — which screen, which user flow, which API interactions
3. **Note the test organization** — `Given current organization is "<Name>"` tells you which org config to check in `data/organizations.yml`
4. **Read step definitions** if the failure is in a specific step — check `step-definitions/` for the matching pattern

This tells you WHERE in frontend/api to look for the regression.

### Phase 4: Find Root Cause

Investigate possible causes in parallel using the Agent tool. **Start with 4d (deploy overlap) and 4e (Bugsnag API errors)** — these are the fastest to check and most commonly the answer.

#### 4e. API Errors in Bugsnag (CHECK ALONGSIDE 4d)

When logs show timeouts or hanging API requests, check Bugsnag for API errors on the endtoend environment during the failure window. Use `tb-bug` to search for recent errors. A 500 from the API often manifests as a generic `TimeoutError` in the e2e logs — Bugsnag will show you the actual exception and stack trace, which is the fastest path to root cause.

#### 4a. Code Regression

In the relevant repos (`repos/frontend`, `repos/api`), check recent commits on develop touching the product area that matches the failing tests. For example, if `features/tasks/` tests fail, look at:
- Frontend: `app/components/tasks/`, `app/routes/tasks/`, related services
- API: `app/controllers/*task*`, `app/models/task*`, `app/serializers/*task*`

#### 4b. Infrastructure / Environment Issue

Signals: connection errors, 502/503 responses, DNS failures, timeouts on basic navigation. If most or all tests fail (not just one feature area), it's likely env. Recommend **rerunning** once stable.

#### 4c. Feature Flag System Release

A system-released flag removes the feature gate, making the feature available to all orgs — including e2e test orgs.

1. List recently system-released flags on the endtoend environment (backoffice or Slack `#release`)
2. Focus on flags where `system_released_at` is within the last few days
3. Cross-reference with the failing test area — search for the flag name (both camelCase and snake_case) in `repos/e2e-tests/`, `repos/frontend/`, and `repos/api/`
4. If a match: determine what behavior changed and whether the e2e tests assumed the old (flagged) behavior

#### 4d. Deploy Overlap (CHECK THIS FIRST)

A deploy during a test run restarts the API/frontend mid-test. This is the **most common cause** of failures that self-heal in the next run.

**Signature:** Test fails consistently (e.g. 6/6 retries) in one run, then passes in the next run with no code changes.

Check if any frontend or API deploy to endtoend overlapped the failed e2e pipeline's run window. The e2e runs and API deploys are both ~hourly, so overlaps are frequent.

If confirmed:
- No code fix needed — infrastructure timing issue
- Tests that retry during the deploy window fail all retries (deploy takes 10+ min, longer than retry cycles)
- Tests that retry after the deploy completes pass

### Phase 5: Present Diagnosis

```markdown
## E2E Test Failure Report

**Workflow:** <link/id> | **Branch:** master | **Run:** <timestamp>
**Overall:** X passed, Y failed, Z skipped

### Failed Tests

| Scenario | Feature | Error Type | Likely Cause |
|----------|---------|------------|--------------|
| ... | ... | ... | Regression / Infra / Flag / Flaky |

### Root Cause Analysis

#### [Most likely cause with evidence]
- What: ...
- Evidence: [commit links, log excerpts, flag release info]
- Affected area: ...

### Recommended Next Steps
- [ ] [Action items based on diagnosis]
```

| Cause | Recommended action |
|-------|-------------------|
| Code regression | Link to the offending commit, suggest fix or revert |
| Deploy overlap | No action needed — confirm the next run passed |
| Infrastructure | Recommend rerunning once env is stable |
| Flag release | Tests need updating — describe what changed |
| Flaky test | Note the pattern, suggest adding `@broken` tag or fixing the test |

### Phase 6: Offer Follow-ups

- "Want me to look deeper into any specific failure?"
- "Should I check if this is a known flaky test?" (check test history)
- "Should I look at more historical runs?" (check flaky tests across runs)
- "Want to compare this run with a previous one?"

## Tips

1. **Start with automated triage** — only go manual if you need more detail
2. **Check deploy overlap first** — most common cause, quickest to rule out
3. **Start broad, then narrow** — overview of what failed, then deep-dive
4. **Use parallel agents** — investigate frontend and api repos simultaneously
5. **Check the obvious first** — if all tests fail, it's almost certainly env; don't dig into code
6. **Feature file paths = product area** — `features/tasks/` → task code, `features/budgeting/` → budgeting code
7. **Retries matter** — fails N/N then passes next run = deploy overlap. Fails 1/N = flaky. Fails N/N across runs = real regression.
8. **e2e-tests uses `master`** (frontend and api use `develop`)
9. **Test orgs in `data/organizations.yml`** — check org config for relevant flags/settings
10. **USE_CHECK mode** — pipeline fails if ANY scenario needed a retry, even if it eventually passed
11. **Flag releases** — check Slack `#release` for SYSTEM RELEASE on the day of failure

## Safety

- All operations are **read-only** — this skill never modifies code or triggers CI runs
- If the user wants to rerun a workflow, confirm via `AskUserQuestion` first
- If checking flag status on production, warn the user
