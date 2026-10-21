# tl30b — ruling #101 C: a code pull request runs the merge gate itself

Crawl 54; contract `sprints/tooling/tl30-footprints-and-the-macos-cap.md`
(deliverable 2), ruling #101 C as the orchestrator relayed it (planning
`0d67596`, "Rulings #101 and #104"), under `sprints/RULES.md`. Branch
`tl30b` from trunk `b28a510`. Touches `ci/` and `.github/workflows/ci.yml`
only. §1 to §4 are committed before the first change.

## 1. Forbidden

- No test, no assertion, no job, no entry and no step changes; nothing
  leaves `tier release` or the nightly. What changes is the NAME of a code
  pull request's run and what the `full-matrix` label asks for: since tl29
  the pr tier and the full tier were the same 13 jobs and 6 entries, so a
  lane's labelled run repeated its pull request's run job for job (R-tl29-2,
  about 2 h and 400 job-minutes a merge for no coverage).
- The planner (`ci/tiers.py`) and its own proof (`ci/tiers-test.py`) change
  together, as is75, tl21, is78 and tl29 did (R-tl29-1).
- Plants: a `tier partial` dispatch naming `plan-test` (`-f jobs=plan-test`),
  never a full matrix (RULES.md §7).
- No build on nomad-1 and none needed (`python3 ci/tiers-test.py`); no pool
  host. No merge, tag, issue filed or closed; no trailer; nothing under
  `~/.claude`; `gh run watch` only with `--interval 60`.

## 2. Inputs, verified (against origin)

| input | re-derived | drift |
|---|---|---|
| trunk | `b28a510` | none |
| the tiers | `ci/tiers.py`: `release` every job and entry (nightly, `-f tier=release`, `release-matrix`); `full` every job but `bundle-identical`, ubuntu's four entries and two smokes (push, dispatch, the `full-matrix` label, an unread or unknown path); `pr` the same jobs and entries (`PR_SKIPS == FULL_SKIPS`, one `matrix_of` branch for both); `noop`; `partial` | none: `pr` and `full` differ in name alone |
| ruling #101 C | a code PR runs the full gate on every push; the `full-matrix` label is no longer required; wolf-interp joins lane-audit's verdict-only list (muzzle, tree-sitter-wolf); `release-matrix` and `tier release` unchanged | — |
| lane-audit | planning `0d67596`: a verdict-only repo needs a green `tier full` (or `tier release`) pull_request or push run at the head and no label | so the code PR's run must be NAMED `tier full` for the audit to read it as the merge run |

## 3. The shape

| tier | when | jobs, entries |
|---|---|---|
| `release` | the nightly, `-f tier=release`, the `release-matrix` label | unchanged: every job, is75's twelve and the windows smoke |
| `full` | **every code pull request's push** (a path a rule calls code, or one no rule names, or an unread change), with or without the `full-matrix` label, which no longer means anything; a trunk push; a dispatch | unchanged: every job but `bundle-identical`, ubuntu's four, the macOS and windows smokes |
| `noop` | a prose-only PR (the notes diet stays), labelled or not; any label but `release-matrix` added, **`full-matrix` included** (the PR's own run is already the gate) | nothing |
| `partial` | a dispatch naming jobs | unchanged |

`pr` is retired as a name: its jobs and entries were full's since tl29.
The concurrency group of a pull request's runs is `release` with the
`release-matrix` label and `gate` otherwise, so a push cancels its
superseded gate run whether or not the old `full-matrix` label sits on the
PR, and a label's noop cancels nothing.

## 4. Prediction (committed before the first change)

- **A lane's pull request**: each push runs `tier full`, the same 13 jobs,
  about 380 to 460 job-minutes and 110 to 120 minutes of wall as tl29's `tier
  pr` runs (39514832485: 377.6 job-minutes, 109.8 wall); the label is not
  needed and adding it starts a noop (under a minute). **Per merge, about
  −400 job-minutes and −2 h of the orchestrator's watch** (the labelled
  repeat no longer runs); the trunk push's `tier full` and the nightly's
  `tier release` unchanged.
- **This pull request** touches `ci/` and `.github/`: its own run is the
  first `tier full` of a pull request (no label), and is the measurement.
- **The plant** (`tl30b-plant`: the planner names a code PR's run `pr`
  again, wolf-interp before #101) reds `ci/tiers-test.py` in the
  `plan-test` job of a `tier partial` dispatch; nothing else runs.
- lane-audit's half (wolf-interp in `VERDICT_ONLY`) is a patch to the
  planning repo, handed to the orchestrator (`lane-audit-104.diff`).

## 5. Evidence

**The planner, seen red locally, then in CI.** Branch `tl30b-plant`
(`4123795`, never merged) plants one break over `e14779b`: `ci/tiers.py`
put back as trunk's (`b28a510`), the planner before ruling #101 (a code
PR's run is `pr`, the `full-matrix` label runs `full`). Locally,
`ci/tiers-test.py` failed on exactly 41 checks; unplanted it passes 168.
The `tier partial` dispatch `gh workflow run CI --ref tl30b-plant -f
jobs=plan-test` is **run 39686189002**: `the planner's own test` (job
119520788888) failed on the same 41 (`tiers-test: FAIL (41)`: T1 six, T2
thirty-two, T4 one, T6 one, T7 one), every other job skipped, and `tier
partial` (job 119520886401) failed with it.

The pull request's own run is the first `tier full` of a pull request with
no label, and is the measurement; it is named in the pull request's body.
