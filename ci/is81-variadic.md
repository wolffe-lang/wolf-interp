# is81 — the variadic mirror

Crawl 54. Contract: `wolf/sprints/interpreter/is81-the-variadic-mirror.md`.
Base `a1a394d`; final semantic head `02ba839`. The completed outer-write
pass already used is81: its `ci/is81/`, `docs/is81-evidence/` and
`tests/mirror_is81.rs` are preserved. This pass addresses #300.

Footprint: cross-cutting

## 1. Forbidden

No compiler implementation was read or reused. Inputs were the spec/corpus,
diagnostic expectation snapshots/catalog, black-box protocol records and the
brief's explicitly named `lower_corpus.rs` regression. No C variadic frame or
cursor is executed, and no unsafe Rust, dependency or lock change is added.
No issue, label, merge, tag, release, host installation or planning-file write.
No attribution trailers. No claim about a deployment.

Nomad obeyed the vm_stat conditional (8 GiB free+inactive+speculative) and
`CARGO_INCREMENTAL=0`, `-j4`. Its compiler build passed the guard; later guarded
interpreter/fetch attempts refused the memory floor and started no build. Locked
crate downloads went only to lane-local CARGO_HOME. The coordinator moved
builds/tests to kasumi CPUs 0–3: one Cargo invocation per shared-core lock,
rootless containers, 8 GiB, foreign-uid PID 1, and inherited taskset affinity.
The already-installed Rust sysroot is mounted read-only; the existing image
lacked fmt/clippy, and nothing was installed to repair it. Processes finish
through exec/wait. Windows and macOS executable interpreter rows remain CI's.
The explicitly requested lane-audit command is the sanctioned host-scratch
exception; no audit script was modified.

## 2. Inputs, verified

- Interpreter source is not its vendored spec identity (#280). This starts
  from interpreter `a1a394d`, crate 0.1.56, spec pin
  `294d626dd596122285d5df954762e3aee3e5db71`.
- New spec/corpus and submodule gitlink:
  `a5378e3fd5b6347f4a89a7d77dbe43aa576fee76`. All **1,251** snapshot files
  equal `git archive a5378e3f` byte-for-byte, both file sets checked.
  `lupin --version` uses its normal seven-character pin `a5378e3`; PIN
  contains the full commit. This is not a new interpreter release.
- The requested clause is `fc906408..a5378e3f -- spec/`: final C ellipsis,
  `VaList` and four raw builtins, named checked refusals, ten new anchors.
  Since the much older vendored pin, the complete registry grows
  **595 → 705**, **110 added, zero lost**. No anchor regeneration was used.
- Rust **1.97.1** was already installed on both build hosts. The black-box
  compiler was built from a5378e3f, not from the installed wolf 0.2.34.
  Both dev builds honestly say `0.2.35+dev.unknown`; their source pin is
  independently fixed here. SHA-256 read twice each:
  - macOS compiler: `57f31be4c2eb6672ebde034c688d2006c4091cb7448d14419f208d17bd9d49e1`
  - Linux compiler: `79dd02e03e5a91cb267abad77cb525e62f4f437d87aa7aa949eeb08864ec8fa5`
- All compiler execution records use **`conform-run --checked --json`**,
  never `wolf run --checked`. macOS's E0826 record is a macOS witness only.

## 3. Prediction

First commit **289a8bd**, before vendoring or implementation: ellipsis fails
as E0201; VaList is missing; the final C tail will parse, malformed positions
will be E0822, named calls/cursors will refuse without executing a frame,
and no anchor key will disappear. The baseline target was **1 passed,
4 failed** (all four predicted frontend gaps); the pin/anchor assertion was
the passing control. The prediction held.

The six old SOUNDNESS classifications already belonged to closed #289;
the seventh comparator item also had an existing issue, #117 item 2. The
initial expectation that it might need a new filing was corrected; none
was filed.

**R-is81-5**, predicted in **514e8fb**, concurred by the coordinator and
implemented in **46e5694**: the new pin exposed #278's cwd accounting row.
Route both os_cwd success arms through existing `produced_str`, not a waiver.
The charged row now prints all true; its escape twin becomes region-fault.
This also removes the sole nondeterministic export record, which had printed
the randomly named observation directory. No bundle normalization or
weakened determinism check was introduced. #278's other debts remain open.

**R-is81-6 prerequisite prediction, before its implementation:** the full
warning gate found `lints/shadow_prelude_local.lu` missing W0304. The pinned
`conf.resolve.ambient` clause explicitly includes locals, and prelude.json
marks clock_ms and VaList as warning-producing shadows. Recommend reading
those canonical flags and checking local let/var patterns, preserving
`allow(w0304)`, provisional exemptions and std suppression. Prediction:
exactly the unsuppressed clock_ms binder warns; the allowed twin stays
quiet. The corpus warning gate is not weakened.

## 4. Evidence index

`ci/is81-variadic/` holds the raw records, logs and set differences.

### Mirror

`tests/variadic_is81.rs` covers ellipsis placement; free VaList and `*VaList`;
24-byte/8-aligned layout and offsets 0/4/8/16; the six declarative target
settings and cfg-filtered lowering witness; E0823/24/25/26; promoted va_arg
types, fixed prefixes, arity and modes, non-C tails, shadowing and aliases;
the CLI's real-host refusal and version pin; all six vendored #289 rows;
and both cwd accounting/escape rows.

The compiler's primary messages and the named construct strings are matched
byte-for-byte, not the two tools' different renderers or phase placements.
The two call directions are distinct. All four cursor dispatches decline
before touching cursor storage; source-level va_start instead encounters the
containing variadic-call refusal first. Ordinary uncalled consumers remain
legal: only start needs the variadic frame. Unknown aliases stay unknown to
sema-lite rather than becoming guessed static errors.

### Every refreshed ledger (same gate, new measured cells)

- `tests/corpus_harness.rs`, `tests/cli.rs`: **1,225 files / 1,163 entries /
  62 members**, from 1,002 / 955 / 47; no file lost.
- `tests/conformance.rs`: add if_brace_next_line/E0201 and the variadic
  position/E0822; register the already-implemented module-shift E0706,
  generic-constructor E0301 and integer-not E0409. The isolated-buffer
  qualified-static exception is paired with a real-module E1301 test;
  the module gate is not waived.
- `tests/run_corpus.rs`: **763 → 938**, **175 added, none lost**. Exactly
  three old cells change, #280's scalar stand-ins replaced by their actual
  s252 List/str bodies: region_conflict_params, region_escape_local and
  region_freeze_write now trap region-fault. The new cwd-return row also
  traps after R-is81-5. Full bidirectional delta is banked.
- `tests/export.rs`: **705 total / 375 covered** anchors (from 595 / 313),
  **1,267 programs / 1,205 records** (from 1,044 / 997). Coverage ratchets
  only upward; re-export identity and CLI identity assertions remain.
- Eleven new fault snapshots, none changed or dropped, from the newly
  vendored integer-truth/shift probes. Their own directive/trap tests remain.
- README-only **a2b40c2** refreshes scope and counts. Manual transcripts
  refresh in **e346529**; install/build/command-list claims remain current.

Measured corpus: **938 run; 868 match; 133 dynamic counterpart; 68 static
conservatism; 90 unsupported; four standing mismatches** (DIV-2026-019,
-027 twice, -028). Zero header failures; 479 distinct conforms tags. No
new mismatch or divergence waiver. Snapshot/bundle file sets were not
silently trimmed.

### Seven-item triage, no duplicates

All six `memory/region_{copy_shares_str,outer_write_field,outer_write_map,
outer_write_str_push,outer_write_struct,str_call_index_store}.lu` map to
**wolf-interp#289**, closed through #296 at a1a394d. Their vendored bytes
match the existing regression fixtures exactly; the new-pin replay keeps
exit(0) and the expected stdout. `conc/proc_link_root_death.lu` maps to
**wolf-interp#117 item 2**, still open: compare permitted nonzero outcome
classes, not the implementation-specified number. Its directive remains
`run(exit=nonzero)`; no production comparator change is slipped into this
mirror. `triage.json` records every path separately.

### CI plant

Draft **#301**, branch `is81-plant-anchor`, **3e6fcfb**. The fixed gate is
committed first in **0fea7f2**. Partial dispatch **40164761102**, job
**121083466025**, step **pinned anchor keys agree both ways**, is red:

`lost anchors: ['abi.c.variadic.checked']`

The full key manifest still names 705; the plant publishes only 704. The
permanent gate now sees both empty differences. No full-matrix plant was
dispatched, no label added, no CI waited on. The orchestrator closes the
unmerged draft and retains its evidence branch.

### Build evidence boundaries

The first full contained replay intentionally exposed the old census,
phase tables, new snapshot rows, cwd gap and corresponding export drift.
A later overlay preserved old mtimes: its test-library hash rebuilt while
the separate normal binary hash stayed cached. That red corpus/bundle
record is retained as `*-cached-binary-red.json`, not called current green.
The final replay starts from the committed semantic head, checks a clean
tracked tree, touches staged inputs, cleans this package's release outputs,
and forces the declared build-stamp nonce. Source identity and timestamps
are recorded with the final logs. No pre-edit green is used as final proof.

Final local gate: PENDING_LOCAL_GATE.
Final CI run: PENDING_CI_RUN. It is the orchestrator's to watch, not a claimed
green here. No label is needed for wolf-interp's verdict-only tier full.
Audit last line: PENDING_AUDIT.

## 5. Done-when

- [x] Prediction first; spec/prelude/grammar/anchors at a5378e3f; full identity.
- [x] Syntax, checked refusals, static facts, C-layout metadata and CLI tests.
- [x] Both anchor set directions, CI plant, and all seven issue mappings.
- [x] No waiver for cwd; existing #278 prerequisite fixed and tested.
- [x] README read and refreshed; each changed ledger enumerated above.
- [ ] Final exhaustive local gate, evidence reconciliation and pruning.
- [ ] PR with commit bullets, test checklist and declared footprint.
- [ ] Final pushed head, one CI-state read and lane-audit handback.
- [ ] Orchestrator watches tier full, merges on green, closes #300.

Issues to file: none. Close **#300 at merge**. #280's stale vendored-snapshot
debt is addressed; coordinate its closure with the next pairing if it is
kept as a release tracker. Keep **#117** and **#278** open; add #278's cwd
sub-item comment naming **46e5694**, without closing its other work.
Prune list and final PR/head/run are in the handback. No issue was written
or closed by this lane.
