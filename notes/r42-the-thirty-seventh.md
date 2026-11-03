# r42 — lupin 0.1.59, prepared for wolf 0.2.37

Prepared 2026-11-03. This is the interpreter release-metadata phase and
stops before the lupin tag. Footprint: cross-cutting

## 1. Forbidden

No tag, release, merge, trunk push, package publication, label or issue
change. No attribution trailers. No runtime or compiler semantic repair
belongs in this pass: a behavioral regression is a named STOP, not a
ledger adjustment. All source edits are in an isolated clone. No Mac
builds; Linux work uses committed snapshots on hasu, never the live tree.
Protected data and running services are untouched.

The Linux rig is Ubuntu 24.04.5 (`localhost/r41-toolchain:1`), Rust
1.97.1. Each debug/release submission selects the block with fewer outer
lock waiters, then takes the block lock and its older inner name once,
both with `-w 21600`. A `--memory 12g` container pins its command inside
the container to four CPUs in that block. A root bash remains its reaping
pid 1; a capability-free uid 1000 runs the tests below a separate session
wrapper. No host pid namespace, catatonit signal mask or process-group
kill. A container-only native taskset shim remaps constrained probes
without changing dispositions or reopening closed standard descriptors.
The 7,000-second foreground timeout starts immediately after outer-lock
acquisition, before the inner wait, and has a 10-second hard escalation
(7,010 seconds including grace, below two hours). Both flock descriptors
are closed to workload descendants. The inner phase consumes the same
remaining budget: bounded CLI teardown, podman's native container lifetime
cap, and a fresh absolute-deadline calculation inside root PID 1, before
compilation or tests. Every timeout targets its direct child, not a group;
PID-1 exit ends the throwaway namespace. These setup guards have focused
offline tests; the corrected Linux execution is still pending verification.
The launcher retains pid, log and exit marker and prints a heartbeat each
minute. Debug and release are separate takes. The lane budget is 10 GB.

## 2. Inputs, verified

- wolf-interp trunk `ed711840ceb9522015438e9807cf6c214ed37076`, exactly
  38 commits after `v0.1.58` at
  `6240efe5570dba1a33ba6269e3ba44bae33dd972`.
- The existing spec/corpus PIN and gitlink both name
  `b16353e8ae616ff3a35bd27adc56b727c50df345`. Neither is changed.
- wolf-lang trunk `50fa61c9a48f828d26c9cfb71e2942d7ec397365`,
  `v0.2.36` at `5adf3d91f75b3545ff8d38e5b6693b0b9f367a05`;
  Cargo posture 0.2.36, off-tag suffix retained. Its complete Unreleased
  section and `docs/releases/0.2.36.md` were read before the first edit.
- wolf-std trunk `d407430ca238cd5756407ff3e1a7847dfc3a2b6e`;
  recommended fixed pin `5b3a59942b2ee69b99682544341ce8a12c984e9e`,
  from `78b9d3cbc41d51db44d18c40b6694403644d1c83`. Both new
  CHANGELOG entries and all 38 commits since the old pin were derived.
- homebrew-wolf trunk `84c4788cf10c5825d2972f9ea0c1ccc471368120`.
- Published wolf/lupin release ids 427033151 and 425116893. All eleven
  asset names and metadata digests agree with the preserved inputs in
  `ci/r42-prediction.txt`. Archive downloads and their two independent
  hash reads are prerequisites of the later differ, not work claimed in
  this interpreter-metadata phase.

The 38 interpreter commits reach users: argument readers/byte argv,
container copies, parser diagnostics, imported error aliases and module
aliases, uname/core calls and `fs_access`. The CHANGELOG derives those
changes and their remaining boundaries rather than adding behavior.
The future wolf pairing is 0.1.59 / b16353e8, pending validation in the
compiler phase. The five s272 builtin names and RAW_LF/ONLCR are explicitly
unsupported cells here, not a claim that wolf-interp#315 is fixed.

## 3. Prediction

`ci/r42-prediction.txt` was committed first at `99df90b`, before the
regression test or any version/documentation edit. It preserves every
full input revision, the eleven control digests, fourteen predicted
compiler differ movers by name and owner, 561 std tests (552 previously),
the lupin move, eleven readback digests and Ubuntu's predicted 12-syscall
start-up row. Compiler/std/start-up measurements belong after STOP 1.

Recommendations, not decisions:

- **R-r42-1:** (A) re-pin std to `5b3a599`, (B) keep `78b9d3c`.
  Recommend A for bounded incremental SHA-2 and the derived std fixes;
  implementation is reserved for the wolf phase after STOP 1.
- **R-r42-2:** (A) prepare lupin 0.1.59, (B) retain 0.1.58.
  Recommend A for the user-visible 38-commit delta; implemented by this
  metadata pass, without authority to tag or publish it.

## 4. Evidence index

### Red before the metadata fix

`d24ecb20b7048ff480dfcdc257fdacbb56fc22be` committed the regression
before the fix. In its immutable Linux snapshot,
`python3 -I ci/r42/release-posture.py --version 0.1.59 --pin
b16353e8ae616ff3a35bd27adc56b727c50df345` failed four named rows:

- `test_manifest_and_lock_name_the_candidate` (0.1.58 != 0.1.59),
- `test_changelog_names_pairing_and_unsupported_surface`,
- `test_readme_version_transcript_names_the_candidate`,
- `test_manual_protocol_transcript_names_the_candidate`.

The existing spec/anchor preservation row passed. Log:
`/home/mfwolffe/lanes/r42/logs/metadata-red.log`,
`END metadata-red exit=1`.

At the corrected metadata commit `f510822`, all five rows passed on
hasu block A. Log:
`/home/mfwolffe/lanes/r42/logs/metadata-green.log`,
`END metadata-green exit=0`. These source-metadata tests are not a claim
that the final-head compiled gauntlet or release tier has passed.

### Reproducible exact-head gauntlet and off-line plant

`ci/r42/hasu-run.sh` runs one profile, from an already staged immutable
worktree; `ci/r42/gauntlet.sh` stops at the first nonzero step and prints
an `END <step> exit=N` line for every executed step. It checks the exact
head and clean tracked tree at both ends. Each run records its full head.

- Debug: metadata, focused guard tests, fmt, planner/dist/anchor checks,
  clippy on all targets, locked host-library check, stale-lock plant proof,
  D57/build-stamp, mirror, argv/CLI, variadic, anchor, exec/SIGPIPE and all
  eight differ CLI rows.
- Release: metadata/guard-tests/fmt/planner/dist/anchors, locked release build,
  CLI/protocol identity, binary digest twice, the whole release test
  suite with `--nocapture`, corpus and place-trace identity. Only the two
  differ CLI rows that require an UNOPTIMIZED harness are filtered here;
  both run unfiltered in debug. Every loud SKIP stays in the log.
- Plant: `r42-plant-lock` contains the candidate plus a deliberate root
  Cargo.lock rollback to 0.1.58. `ci/r42/plant-lock.py` requires
  `cargo check --locked --offline --lib` to fail specifically because that
  lock needs updating. Its separate log names
  `locked_manifest_refuses_stale_root_version` and retains the actual
  red exit. This break never belongs in the release branch. The plant
  is branch-only, with no PR or dispatch.

The first compiled-gate launch at `928efcbf` stopped before its first
step: the worktree's absolute gitdir was not visible through the `/lane`
volume alias. This was a rig prerequisite failure, not a test verdict.
The launcher now also mounts this lane at its original absolute path,
so both sides of git's worktree metadata resolve without editing it.
A fresh committed snapshot is used for the corrected launch. At `bf835f78`,
clippy passed, but offline unfiltered `cargo metadata` then requested an
uncached Windows-only package (`anstyle-wincon`). The locked guard and
plant now use the host's `cargo check --locked --offline --lib` after
clippy has populated its host dependencies, preserving the same stale-lock
rejection without requiring unrelated target downloads.

### Guard correction after the ended `610c4fb7` run

The next debug run ended **exit 1** at 2026-11-03T04:34:49Z, not pending.
Clippy and `locked-check` passed. Cargo rejected the stale lock as intended:
`END locked_manifest_refuses_stale_root_version exit=101`. However, the
proof wrapper accepted only Cargo's older diagnostic, so
`END plant-lock-proof exit=1` correctly stopped the gauntlet. The actual
message was "cannot update the lock file ... because --locked was passed
to prevent this". That failed run is not waived or called green.

`ci/r42/guard-correction-prediction.txt` was committed at `714e11e`
before correcting the guards. `004bcea` committed nine focused regression
rows, all red before the fix. The corrected predicate accepts both Cargo
lock-update/`--locked` causal diagnostics with a nonzero exit, and refuses
success, permission/compile/cache errors and help-only text. The lifetime
correction moves the outer-held timeout before the inner wait, preserves
both single `-w 21600` requests, adds hard escalation and a native
container lifetime cap, and recomputes the work budget after startup.

`python3 -I ci/r42/guard-tests.py` now passes **11 tests**, including
budget boundary arithmetic, static wrapper/PID-1 checks and offline CLI
integration with recording fake flock/timeout/podman/cargo executables.
Those tests execute no real lock, container, signal or remote job and
perform no Rust build. They retain a child's nonzero exit and reject an
unrelated Cargo failure through the real plant entrypoint. Red/green logs
are under `/Users/mfwolffe/lanes/r42/logs/guard-tests-red.log` and
`/Users/mfwolffe/lanes/r42/logs/guard-tests-green.log`.

The corrected candidate has **not** been submitted remotely. A new exact-
head debug/release gauntlet and current plant proof remain required after
verification. The runner writes `records/<profile>-<full-head>.pid`, a
`.locker.pid`, a `.exit` marker and a matching directory of records;
its caller redirects the log to `logs/<profile>-<full-head>.log`.
No CI run has been requested by this pass.

### README

README read against commands, counts, scope, archive shapes, and
install/build instructions. Only the executable version transcript
changes to 0.1.59, in its own README commit. The manual's JSON version
moves separately. Corpus counts, command list and the install/build
paragraphs remain current. Existing `doc_truth` validates both version
transcripts during the compiled suite.

## 5. Done-when

- [x] Derive every input and commit the prediction before implementation.
- [x] Commit the release-posture test before its named red, then fix the
  version/lock, derived CHANGELOG, README and manual; metadata green.
- [x] Recommend R-r42-1/A and R-r42-2/A; implement the lupin recommendation.
- [x] Preserve the spec PIN, gitlink, dependency graph and six-asset shape.
- [ ] Exact-head debug and release gauntlets green; every END read, with
  the off-line plant red by name and its branch pushed without a PR.
- [ ] Verification, final branch push and five-section PR with commit
  bullets, test checklist and `Footprint: cross-cutting`.
- [ ] Exactly one final-head lane audit and its exact last line recorded.
- [ ] Release-matrix label and CI watch by the orchestrator; release tier
  green before STOP 1 can be cleared.
- [ ] STOP 1: lupin tag is the authorized publisher's step, never this lane's.

No new issue is filed or closed by this release-metadata pass. Existing
mirror remainders (#314, #315, the remaining #277/#278/#293 surfaces and
the documented diagnostic/errno gaps) remain open; their fixes are not
smuggled into a version bump. The compiler, std pin, tap, downstream
updates and all later STOPs remain untouched at this handoff.
