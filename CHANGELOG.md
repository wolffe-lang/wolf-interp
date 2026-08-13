# Changelog

## 0.1.10 — 2026-08-12

THE RUN TIER FINALLY COMPARES: the mid-end/whole-program re-pin. Pin
bumped `0b4e79c` → `613c3dc` (latest green trunk, CI run 31651577695:
the wave landed s42 (the mid-end optimizer), s43 (whole-program —
clusters, body dedup, a frozen summary index) and s63 (diagnostics
polish: 144 codes / 30 warnings, error cascades capped with
`--error-limit=N`), plus the dv01 prose pass). The corpus grows 4:
263 → 267 files (`conc/select_two_timeouts.lu`, the #64 GVN
cross-arm-dominance litmus, and the s42 kernel tier
`kernels/hot_counter.lu`, `kernels/hot_scale_versioned.lu`,
`kernels/churn_b3.lu`). All four reach `run` on this machine at first
sight and match their `check:`, so the run ledger grows 4. Coverage
ratchets 102 → 103: `[conc.select.timeout]` is covered for the first
time, by the new select litmus alone. `spec/` is UNCHANGED in this
range — anchors stay 315 — so no clause needed a new reading. Bundle
304 programs / 282 records.

- **`diff-run --counterparty-tier=default|checked|native|release`, and
  the gap it closes.** The counterparty's `conform-run` is one process
  contract over several engines, selected by flag. The runner passed
  **no flag** through 0.1.9, so the compiler stopped at `unsupported`
  @`wir` and the counterparty reached `run` on **0 of 245 entries**:
  the entire dynamic half of the corpus was ledgered as conservatism
  without ever being compared. The gap was named for `--native` at
  0.1.9 and stayed open a round; it covered `--checked` too, which
  nobody had noticed. Now measured: the counterparty reaches `run` on
  120 files at `checked`, 113 at `native`, 104 at `release`, and both
  machines *execute* 107, 107 and 98 respectively. Our own side is
  always invoked plainly — this machine has one engine.
- **THE RELEASE LANE IS COMPARABLE, AND THE MID-END CHANGED NOTHING
  OBSERVABLE.** `conform-run --release` runs s42's optimizer and s43's
  whole-program layer; compared against this machine over all 98 files
  both execute, it agrees everywhere, and the one divergence it reports
  is the same one `--checked` and `--native` report, byte for byte. A
  transformation that altered behavior would have surfaced as a
  release-only finding; there is none. Honest scope: linux x86-64, one
  platform, unseeded, and the `kernels/` tier is three programs — this
  shows that 98 observable behaviors survived optimization, not that
  the optimizer is correct. The release lane declines the conc tier by
  name (9 files), the compiler's documented posture, conservatism never
  divergence.
- **DIV-2026-017 FILED — the first finding a run-reaching lane ever
  produced.** `lints/raw_interp_braces.lu`: `r"{who}"` prints `{who}`
  here and `"{who}` on the compiler, whose raw-literal decode keeps the
  opening quote of the two-character `r"` delimiter.
  `[gram.lex.str.raw]` is explicit and the corpus header's own
  `stdout="{who}"` agrees with this machine, so triage case 2 —
  compiler bug, filed upstream. Identical on all three of its
  run-reaching tiers, which is how it is known not to be the mid-end's
  doing. The file's `phase: wir` pin was written when BOTH executors had
  the bug; it now masks a one-sided one and should advance to `run` with
  the fix.
- **wolf-interp#16 FIXED — an enum variant is a value, not a raise.**
  The silent-wrong-answer of the pass. A declared enum's variant and a
  structural error tag are both tag-shaped, and the machine built them
  as the same value, so `fn id(v: W) -> W ! {none} { v }` had its
  ordinary return read as an error: `?` propagated it and `else` fired
  on the VALUE path, so the miss always won and the two paths were
  indistinguishable to the caller. `ErrorValue` now records where its
  name resolved (`enum_variant`), sema's `Module::variants` — the same
  table the variant-pattern rule reads — decides it at the two
  construction sites, the flag rides through payload application, and
  `is_error` (the only question `?` and `else` ask) reads it. Equality,
  patterns and rendering ignore it. Confirmed against wolfgang's
  `--native` and `--release` lanes, which both print `kind num` for the
  reproducer.
- **wolf-interp#17 FIXED — the cast target is resolved, and `str` is not
  a cast source.** `s as nonsense` ran to `exit(0)` with the string
  passed through unchanged: the type expression was never resolved, so a
  typo in a cast target was invisible. Now E0301 at `resolve` spanning
  the **type name**, matching the counterparty span for span
  (`[55,63]` for `nonsense`, `[55,60]` for `bytes` — no `bytes` type
  exists in this language either). The judgement is narrow on purpose:
  only a single unqualified lower-case path that is neither a built-in
  scalar nor a name the module declares, so `2 as Meters`, `x as T` in a
  generic body and every qualified path still decline to be judged.
  `s as int` is E0805 at the whole cast expression (`[50,58]`, the
  counterparty's span); the shape sema-lite cannot classify — a `str`
  from a call return, which is the typecheck rung this machine does not
  perform — declines loudly at `run` instead of passing a string off as
  a number.
- **wolf-lang#71's premise corrected: this machine never said E0202.**
  The issue records lupin rejecting the `(mut xs).push(1)` two-task
  program with E0202. `E0202` is this machine's `E_UNEXPECTED_EOF`
  (`src/diag.rs:186`) — a *parse* code, never a capture-law verdict —
  and at this pin lupin does not reject the program at all: it runs and
  prints `0`. On the assignment spelling (`n = 1`) both machines already
  answer **E1101**, at their own rungs, which `[proto.cmp.rung]` makes
  agreement. **The proposal: E1101, on both sides** — this machine wants
  no distinct code, and the interpreter half is to treat a `(mut x)`
  receiver-lend of a captured binding as a write and emit E1101 there
  too. Deliberately not done this round: the code is corpus-pinned and
  landing a span before the compiler's would trade a missing diagnostic
  for a span divergence. A second finding recorded as this machine's
  own: closures capture by value, so the mut-lend program prints `0`
  here against wolfgang's `2` — a real stdout divergence on a program no
  corpus file witnesses.
- **The record's `commit` stamp stops going stale.** `build.rs` watched
  `.git/HEAD`, which on a branch holds `ref: refs/heads/<branch>` and
  does not change when a commit lands, so the stamp froze at whatever
  commit last forced a rebuild and records claimed a revision that had
  not produced them — the dishonesty `[proto.record.fields]` asks that
  file to prevent. It now also watches the ref HEAD points at, and
  `packed-refs`, each only when present.
- **The three s71 clauses, verified clause by clause at the new pin.**
  `[mem.str.empty]`: `count("")` is 0, `split("")` yields one piece and
  that piece is the whole string, `replace("", t)` is the identity,
  `repeat(0)` is `""` — conforming, no lane refusing, no lane trapping.
  `[mem.str.repeat]`: `repeat(-1)` is `trap(assert)`, not `bounds`. The
  `else |Tag(p)|` row-coverage rule: `rows/negative/handler_uncovered.lu`
  is `fail(E0809)`@`resolve` span `[518,523]` — the counterparty's span —
  and the payload-binding run half `rows/else_tag_payload.lu` runs with
  its exact expected bytes. All three conform; nothing needed changing.
- **Surfaces:** `--version` prints `lupin 0.1.10 (wolf-interp, reference
  interpreter at pin 613c3dc)`. Corpus walk 267 files / 0 mismatch (158
  match, 8 dynamic counterparts, 32 conservatism, 47 out-of-scope);
  bundle 304 programs / 282 records, anchors covered 103 of 315;
  differential GREEN at `default` with an empty filing list, and
  filed-green at the three run-reaching tiers. Noted, not acted on:
  upstream's own `--version` still names `lupin 0.1.8 … pin 7886559` as
  its paired reference interpreter, two releases behind.

## 0.1.9 — 2026-08-12

THE CONC TIER RUNS ON BOTH MACHINES: the c09-wave re-pin. Pin bumped
`26fa98e` → `0b4e79c` (latest green trunk: the wave landed s41
(release tier), s51 (package manager), s73 (native concurrency) and
four fmt idempotence fixes; trunk head `17ea078` is CI-red on both test
lanes and was not taken). The corpus grows 1: 262 → 263 files
(`test/conc_schedules_test.lu`, the s73 `--schedules=N` dogfood
witness); nine files' headers advance to phase `run` (8× `conc/` +
`procs.lu`; wolfgang executes concurrency natively now, and this
machine's run-ledger claims for all nine predate the wave, so both
machines now execute the tier), `memory/prov_holy_grail.lu` moves
typecheck → mem,
and two conc files gain `-> !int` rows (E0604/D30). Anchors stay 315,
ratchet stays 102; bundle 300 programs / 278 records.

- **The conc tier, machine to machine: 10 of 10 agree.** The wave's
  first comparison over a corpus tier where BOTH implementations
  execute concurrency. `wolf conform-run --native --seed=N` vs `lupin
  conform-run --seed=N` over all ten run-phase conc-tier files at seeds
  {0, 42}: every pair agrees on verdict, exit and output bytes, both
  records `"seeded": true`, a `[proto.seed.equal]` comparison, honored
  end to end. Honest scope: two seeds, linux x86-64, wolfgang's debug
  tier; the harness's own diff-run lane still invokes plain
  `conform-run` (wolfgang's native rung is opt-in) and ledgers the ten
  as counterparty-unsupported conservatism. The native comparison ran
  direct, documented in the thirteenth differential entry.
- **Thirteenth corpus differential: 241 entries, 0 divergences.**
  Counterparty built CLEAN at the pin with `libwolf_rt.a` provisioned;
  395 conservatism-ledger entries (66 rejects-beyond, 130
  run-unmatched, 152 counterparty-unsupported, 47 interp-unsupported).
  `FILED_DIVERGENCES` is empty again.
- **DIV-2026-016 RESOLVED; wolf-lang#61 closed.** The CLEAN build
  answers `fail(E0809)@typecheck`, same span `[518,523]`. The E0806
  answer does not reproduce at the release sha or this pin, agreeing
  with the issue's own reproduction attempt. `[proto.cmp.rung]`
  agreement with this machine's resolve rung; the 0.1.8 observation is
  attributed to a stale/intermediate-pin counterparty build. An
  evidence-hygiene lesson (fresh counterparty target per re-pin), not
  a semantics one.
- **E1014's trap-map row landed upstream: 0.1.8's filed nit, closed.**
  The pinned `[conf.trap.map]` exclusivity row now names "E1014's
  read-mode write barrier, D39" outright, so this machine's D39 trap
  mapping (`ledger::dynamic_meaning` E1014 → `exclusivity`) is
  document-stated rather than family-inferred. No behavior change;
  the citation argument in the comment retired.
- **Surfaces:** `--version` prints
  `lupin 0.1.9 (wolf-interp, reference interpreter at pin 0b4e79c)`.
  Corpus walk 263 files / 0 mismatch (154 match, 8 dynamic
  counterparts, 32 conservatism, 47 out-of-scope); bundle 300 programs /
  278 records, anchors covered 102 of 315; differential lane GREEN
  with an empty filing list. Open lupin issues #16/#17 re-verified at
  this pin: #16's shape now fails loud (`unsupported` at the else
  handler) but the handler still fires on the value path, still open;
  #17's unknown-`as`-target pass-through still reproduces, still open.

## 0.1.8 — 2026-08-11

The v0.1.0 RELEASE PAIRING build: this is the version wolf 0.1.0's
`--version` line names as its reference interpreter. Pin bumped
`e94b879` → `3d5cee6` (wave seven: r01 prep + s71 release polish; the
corpus grows 5: the `[mem.str.empty]`/`[mem.str.repeat]` witnesses, the
`else |Tag(p)|` pair, the ctfe fold witness; the spec grows
`[mem.str.empty]`, `[mem.str.repeat]` and §10 `[gram.version]`
(grammar/1)). Then the ONE lawful mid-pass re-pin `3d5cee6` → `26fa98e`
when s72's mode teeth merged upstream (the corpus grows 3 more: the
D39/D40/overlap fail-files `memory/read_param_write.lu` E1014,
`memory/mut_read_overlap.lu` E1002, `memory/list_mutate_while_iter.lu`
E1013; the spec grows `[mem.iter.excl]` and the trap map's E1013 row).
Net: 254 → 262 files, 306 → 315 anchors, ratchet 97 → 102. The D39/D40
mirrors below were built on the rulings' authority (wolf planning
02-decisions.md, 2026-08-12) while s72 ran concurrently; the re-pin
brought their spec text and fail-files into the pin, and all three
fail-files pair with this machine's traps as dynamic counterparts
(`ledger::dynamic_meaning` gains E1013/E1014 → `exclusivity`; the
pinned map prose names E1013 but not yet E1014, a noted upstream-worthy
nit).

- **D39's dynamic mirror: a write through a read-mode binding traps
  `exclusivity`.** Every call frame now carries its read-mode parameter
  list and `write_path` is the barrier: whole-parameter stores,
  projection writes (`p.x = 9`), compound assigns, and a mutating
  method's receiver write-back all trap with the parameter's declaration
  as the second span and a `mut`-plus-call-site-spelling teach. The kind
  is `exclusivity`, `[conf.trap.map]`'s family for mode violations
  (`[mem.tier0.excl.1]` already gives every read/write conflict that
  kind; D39 names no new one). A body local shadowing the parameter's
  name stays an ordinary local. The caller-side overlap half
  (`f(mut a, a.x)`) was verified and kept. approximation-contract §6.12.
- **D40's dynamic mirror: mutating a container while a `for` loop
  iterates it traps `exclusivity` at the mutation (S-11 RESOLVED;
  closes wolf-interp#9 as fixed).** `for x in xs` holds a read claim on
  the container's place for the loop's whole extent; push/pop/clear, an
  element write, a whole-container assignment, or a `mut` pass inside
  the body conflicts with it and traps, the message naming the loop and
  teaching D40's fix-its (collect-then-apply, or the index loop;
  wolfgang's E1013 row in `[conf.trap.map]`). The wolf-interp#9 program
  that ran `exit(0)` over the loop-entry snapshot traps now; reads
  beside the claim stay legal; the claim dies with the loop on every
  exit path. approximation-contract §6.8 rewritten to the ruled
  semantics; the S-11 filing closes in docs/divergence-log.md.
- **The s71 clause backlog.** `[mem.str.empty]`: the searching family is
  defined on an empty needle, so `count("")` is 0, `split("")` yields
  the whole string as one piece, and `replace("", t)` is the identity;
  the three `unsupported` declines die. `[mem.str.repeat]`: a negative
  repeat count is a caller contract violation, taking the deterministic
  `assert` trap and retiring the sc03-era `bounds` spelling (the 0.1.7 corpus
  mismatch on `faults/repeat_negative.lu` closes). E0809 at the resolve
  rung: an `else` handler pattern must cover the operand's whole error
  row, judged where sema-lite can see the row (a direct unshadowed call
  to a same-module fn with a declared closed row, the E1007
  discipline); `rows/negative/handler_uncovered.lu` pins it, and the
  payload-binding run half (`rows/else_tag_payload.lu`) already agreed.
- **comptime posture unchanged, verified at the new pin:** every corpus
  comptime file that calls a `comptime fn` declines loudly and by name
  (`` `expand` is a `comptime fn`; … the compiler's engine (s16) ``),
  including the new `comptime/fold_reaches_lane.lu`. The fold table is
  the compiler's; nothing here half-implements it.
- **One divergence found, filed: DIV-2026-016 / wolf-lang#61.**
  wolfgang's own `conform-run` answers `fail(E0806)@typecheck` (the
  generic refutability diagnostic) on `rows/negative/
  handler_uncovered.lu`, the file its own corpus pins `fail(E0809)`
  for; this machine matches the pin at resolve, same span. Codes
  differ, so `[proto.cmp.rung]` cannot bridge it; the counterparty is
  the defendant (both records attached to the issue). The lane is
  GREEN with the filing; the entry closes when wolfgang's conform-run
  answers its own pin.
- **Surfaces:** `--version` prints
  `lupin 0.1.8 (wolf-interp, reference interpreter at pin 26fa98e)`.
  Corpus walk 262 files / 0 mismatch (153 match, 8 dynamic
  counterparts, 32 conservatism, 47 out-of-scope); bundle 299 programs /
  277 records, anchors covered 102 of 315; differential lane GREEN, and
  the one divergence is DIV-2026-016, filed.

## 0.1.7 — 2026-08-11

The release-runway pass for r01 (wolf v0.1.0 names this version as the
reference interpreter). Pin bumped `13b811f` → `e94b879` (wave six: s40
os/time/json, s70 match tier + X3 value paths, s69 idiom lints; the
corpus grows `lints/` ×15, `os/` ×4, `json/` ×2, `faults/` ×5,
`time/` ×1 and the match-tier witnesses: 221 → 254 files; the registry
grows `[proto.cmp.rung]`, 305 → 306 anchors, ratchet 96 → 97). Two
issues closed, every open divergence family closed, the differential
compares CLEAN.

- **#21: container-element literals adopt `int`, and `List[i32]`
  checks its writes.** Both directions of the s70 reassignment. A
  literal pushed into a `List` adopts the locked 64-bit `int` (the
  `Range` precedent) instead of staying literal and defaulting `i32` at
  the pop-binding, so `push(2147483647); pop; +1` prints `2147483648`
  now, as wolfgang does. The constructor's bracket type argument
  (`List[i32]()`, read off the callee's syntax; also `List[T]`
  annotations through `coerce`) travels on the value, so pushes
  range-check at the element's width, element loads feed checked
  arithmetic at `i32`, and the compound `l[0] *= 2` traps BEFORE the
  write lands. The five `faults/overflow_*` litmuses and
  `memory/list_elem_assign.lu` all pin, both lanes.
- **#22: the explorer runs the same admission ladder as `run`.** The
  `--explore` path bypassed the E11xx statics and certified programs
  "observably deterministic" that the same binary refuses to run
  (rp01's finding; the ch13 lost-update shape). `frontend::admit` is
  now the ONE admission question (module laws, then the pin statics,
  then the raise check), asked by `observe_with`, `explore_file` and
  `explore_source` alike, so `conform-run --explore` refuses
  `store_buffer`/`chan_unsendable`/`when_nested` with the run door's
  own diagnostics (exit 2, no certificate). `--explore` also honors
  `--std-root` now.
- **`[proto.cmp.rung]` adopted; DIV-2026-011/-012/-014/-015 ALL
  CLOSE.** The ruling the last four rounds routed upstream landed in
  spec/06: fail(code+span) parity at any shared-ladder rung is
  agreement, exactly one verdict wide. `compare_deep` and
  `compare::compare` implement the clause, the eleven rung-placement
  divergences compare clean, and `FILED_DIVERGENCES` is EMPTY for the
  first time since the fourth round. En route the fail comparison
  learned to read the first **error**-severity diagnostic. A
  counterparty may interleave lints ahead of its rejection in
  `diagnostics` (`[proto.record.warn]`), and a lint's span is
  `[proto.cmp.warn]`'s surface, never the rejection's (the
  `resolve/cycle` shape at this pin).
- **The s69 idiom lints.** Ten of the eleven run here with
  counterparty-identical spans: W0310–W0315 (naming, docs, module
  shape), W0603/W0604 (rows), W1002/W1003 (mode hygiene). E0802's
  literal-precise dead-arm analysis lands with them (duplicated
  str/bool/int literals, `_` after bool's two literals, the
  `pattern_shape` degradation). W0316 (ancestor import) is
  honest-absent: its only
  witness shape needs the dotted nested-module loading this machine's
  loader does not perform.
- **The s40 tier, postured.** env and time are implemented on the
  checked-lane posture: overlay env (never the host's), empty argv
  (the stdin posture mirrored), cwd as process state, X12 monotonic
  time, and `os_exit`'s defer-skipping termination (`Signal::Exit`).
  So `os/args_cwd`, `os/env_roundtrip`, `os/exit_code` and
  `time/monotonic` run to their pinned outputs. The process trio is
  exec surface declined by design; the json kernels are the
  counterparty's reference parser, declined instead of guessed. All
  fifteen names resolve, so every refusal is "unsupported feature",
  never "unknown name".
- **Counts.** 254 corpus files, 232 entries: 149 match, 5 dynamic
  counterparts, 32 conservatism, 46 out of scope, 0 mismatches.
  diff-run at the pin (CLEAN wolfc build at `e94b879`): 232 entries,
  **0 divergences**. Bundle: 291 programs, 269 records, 97/306 anchors.
- **Surfaces.** `--version` names the pairing posture per r01 row 7:
  `lupin 0.1.7 (wolf-interp, reference interpreter at pin e94b879)`.

## 0.1.6 — 2026-08-11

The wave-four re-pin and the lint wave (issues #19, #20). Pin bumped
`f0da6e6` → `13b811f` (#41 capture law, #42 checked-lane determinism,
s34 procs, s35 io reactor, s39/s40/#40 native str/List/fs, s68 lints:
the corpus grows `lints/` ×12, `conc/` ×3, `projects/` ×3, `net/` ×2,
`comptime/` ×1, `test/` ×1: 199 → 221 files; the registry grows
`[conc.chan.default]` and `[mem.region.freeze.4]`, 303 → 305 anchors,
and the `test` namespace joins the reserved forward set). Two issues
closed, one divergence family filed, one closed.

- **#20: reads through frozen containers are legal.** The spec ruled
  (`[mem.region.freeze.4]`, appended for exactly this defect): every
  read through frozen data is an ordinary read, and a value-semantics
  machine must not count writing back an *unmodified* method receiver
  as a write. `eval_method`'s write-back is now conditional on the
  receiver actually changing, so the book's ch10 shape
  (`frozen[0].body.words()`) reads legally where 0.1.5 trapped
  `region-fault`; a genuinely mutating method through a frozen home
  still traps (`region_freeze_write.lu` unchanged).
- **#19: the s68 lint wave.** This machine now runs warning analyses:
  the eleven shared-analysis lints (W0304–W0309, W0401, W0602, W1101,
  W1102, W1302) plus the `#[allow]` self-lints W0302/W0303, in the new
  `lint` module. Every fixture span is byte-identical to the
  counterparty at the pin, `#[allow]` suppresses identically (item- and
  statement-granular), the record's `warnings` array is populated per
  `[proto.record.warn]`, and the same observations ride `diagnostics`
  at warning severity. The compiler-only four (W0402, W0601, W0801,
  W1001) stay honest-absent, written down in `lint::HONEST_ABSENT`; the
  corpus `warns:` ledgers are enforced for the implemented set. The
  same walk carries the pin's static realignments: **E1101/E1102/E1103**
  (the #41 capture law: a task's write to a captured name with `when`
  bodies exempt, a spelled `List`/`Map` channel payload, a lexically
  nested `when`) and **E0004** (`1.e5` stays an *error*, the s68
  correction) all reject at this machine's resolve rung with the
  counterparty's codes and spans, so `store_buffer`/`chan_unsendable`/
  `when_nested` leave the run ledger for the fail column, and the last
  E000x unsupported(interp) conservatism row closes. §7.8's
  `[ub.assume.noalias]` citation is confirmed `[mem.unsafe.raw.2]`,
  with W1302 as its compile-time face.
- **Realignments at the pin:** `[conc.chan.default]` adopts this
  machine's rendezvous default as normative (`channel[T]()` was already
  capacity 0 here; the clause cites the behavior, and the ctor now
  cites the clause); E0412/E0413 spans realign to the counterparty's `:spec`
  shape; the `test` anchor namespace lands (`test/assert_test.lu`
  walks); the s34 proc pair and two of the three P-project witnesses
  run (`projects/count.lu` needs the fs tier this machine declines by
  design).
- Tenth differential: 203 entries, **11 divergences, all filed, every
  one the same finding**: same code, same span, this machine at
  resolve vs wolfc at typecheck/mem (DIV-2026-011/-012/-014, plus new
  **DIV-2026-015** for the four realigned statics; one `[proto.cmp]`
  ruling closes all four families). **DIV-2026-013 closes** (wolfc's
  s38 conform-run wiring landed; `unsupported` there now, never a
  divergence). 325 conservatism entries. Bundle: 258 programs, 240
  records (the issue #20 freeze-read twin joins the suite tier);
  coverage ratchet raised 90 → 96 over 305 anchors.

## 0.1.5 — 2026-08-11

The unsafe-tier batch (issue #18, the book's ch09 differential) and the
five-lane re-pin. Pin bumped `ad6cef7` → `f0da6e6` (s32 tasks, s33
channels, s37 str core, s38 fmt/io/fs, s67 warnings: the corpus grows
`strings/` ×9, `lints/` ×4, `fs/` ×2, `io/` ×1: 183 → 199 files; the
registry grows the `diag.*` block, `[mem.str.get]` and
`[proto.record.warn]`/`[proto.cmp.warn]`, 290 → 303 anchors). One
issue closed (#18, six items), three divergences filed.

- **#18 (1): the unsafe ring is enforced.** Raw-tier operations
  outside `unsafe` blocks reject at this machine's resolve rung with
  the counterparty's code and span: C calls (E1301 at the call,
  `[384,395]` on `unsafe_raw_outside.lu`, byte-identical), raw reads
  and writes through pointer-holding locals (the place, `[452,456]`
  shape), int→pointer casts, `borrow … from`, `assume noalias`, and
  the provenance operations. Sema-lite tracks what it can *see*
  (literal-bound locals, allocator calls, the book's laundered
  `unsafe { … }` initializer) and never guesses. Rung placement vs
  wolfc's mem emission is **DIV-2026-012** (the DIV-2026-011 question).
- **#18 (2): nothing casts to `bool`.** The cast matrix's bool column
  closed: `n as bool` is E0805 at the whole cast expression, inside
  `unsafe` too (observed parity at the pin). §7/T1's one modelled
  production door closes with it: the T1 trigger/twin retired,
  `UbRow::T1` is `Coverage::Unreachable` with its reason, and the
  detection logic stays for frontend-bypassing callers. `bool as _`
  and `_ as str` reject statically where the class is visible
  (`typecheck/cast_bad.lu` now matches its pin). P1's protector-form
  suite pair retired with its `*u8` signatures; the protector
  acceptance evidence moved inline (machine-direct), and P2's
  trigger/twin rebuilt on `freeze r`, in-language, same row.
- **#18 (3): `*T` never crosses a signature.** E1302 at the parameter
  name (`[329,330]` on `unsafe_sig.lu`, byte-identical), return types
  at the type span. Also DIV-2026-012.
- **#18 (4): the C intrinsics check their arguments.** Exact arity
  for the modelled five; size/count arguments must be non-negative
  integers; `c.memset`'s byte argument no longer defaults silently, and
  every refusal names the construct.
- **#18 (5): the §7.4 format specs, to parity.** New `fmtspec`
  module: `[[fill]align][+][0][width][.precision][type]`, with zero-pad
  AFTER the sign (`{n:08}` is the flag plus width; the absorb-into-
  width reading was the filed bug), `+` with zero taking it,
  sign-magnitude bases, `e`/`E` signed two-digit exponents, str
  precision on code-point boundaries, shortest-round-trip f64 default
  (the `std.fmt.decimal.to_str` layout; floats render `3`, not `3.0`).
  Malformed specs are **E0412** and type-mismatched specs **E0413**,
  statically at the literal where sema-lite sees the hole's class;
  E0411 statically refuses `s[i]` char indexing. The three corpus
  fail-files match their pins; wolfc's conform-run at this pin cannot
  reach its own emissions there, filed as **DIV-2026-014**,
  counterparty suspected.
- **#18 (6): the fs/io posture.** No filesystem by design: the s38
  `fs_*` family and `read_line` resolve and decline with the construct
  named. `eprint`/`eprint_raw` are real: one fmt machinery, two fds,
  stderr live-gated like stdout's pass-through and never hashed;
  `io/eprint.lu` runs to its pinned stdout. wolfc's conform-run at the
  pin E0301-rejects its own s38 files, filed as **DIV-2026-013**.
- **Realignment:** the s37 str surface lands in full (`get`, the
  `[mem.str.get]` boundary primitive, oob = reversed = split-code-point
  = `none`, hits bit-identical to the checked slice; then `find`/`rfind`,
  `bytes`, `split`/`count`/`replace`, `strip_prefix`/`strip_suffix`,
  `trim_start`/`trim_end`, `ends_with`, negative `repeat` trapping
  `bounds`), `^n` end-relative endpoints resolve before the domain
  question, and `[proto.record.warn]` is wire-complete: the additive
  `warnings` array (schema, comparison per `[proto.cmp.warn]`,
  honest-absent, since this machine runs no warning analyses yet and
  says so by omission), the `warns:` corpus directive, and the `diag`
  anchor namespace. The lints tier runs warning-clean; spec/01 §9's
  bare `[diag]` heading token is exempted from the registry
  cross-check as a namespace, not a clause (routed upstream).
- Ninth differential: 181 entries, 18 members, **10 divergences, all
  filed** (011 open; 012 rung placement ×4; 013/014 the counterparty's
  conform-run surface lagging its own corpus at the pin), 289
  conservatism entries. Bundle: 235 programs, 217 records; coverage
  ratchet raised 86 → 90.

## 0.1.4 — 2026-08-10

The held maintenance wave, released once s30 shipped upstream. Pin
bumped `d147a54` → `ad6cef7` (s29+s30: the corpus grows
`memory/unsafe_c_alloc_native.lu`, `rows/eu_main_err_exit.lu`,
`typecheck/float_nan_cmp.lu` and `resolve/same_name/`: 177 → 183 files;
anchors hold at 290; the two E0410 fail-files re-pin `phase:` resolve →
parse). Four issues closed, one divergence resolved, one filed.

- **#15 (silent-wrong, ba:blocker): the X1 call-site mode law has its
  missing half.** `f(x)` where the signature demands `f(mut x)` ran to a
  wrong answer silently (the writeback never happened). The book caught
  it teaching chapter 7. E1007's static rule is now sema-lite's at the
  resolve rung, all four disagreement shapes (missing `mut`/`take`,
  extra mode, wrong word), matching wolfc's code, span, and message
  shapes, exactly as E0410 in 0.1.2; `[conf.trap.map]` gives E1007 no
  dynamic meaning, and a wrong answer is not a semantics candidate, so
  rejection is the honest stop. The dynamic residue, calls through
  function values, is refused at the call, never run wrong.
  `memory/mode_missing_mut.lu` leaves the run ledger; the book's ch07
  repro is a regression test. Rung placement vs wolfc's `mem` emission
  is **DIV-2026-011** (same code, same span; routed upstream).
- **#13: `c.calloc(n, size)` allocates `n * size` bytes.** The modelled
  C heap gave it `n`; s29's native differential (real glibc) caught the
  disagreement, the first soundness candidate it produced, and a lupin
  bug. Overflow in the size computation is `unsupported` (real calloc
  says NULL; no null surface is pinned). `malloc`/`memset`/`memcpy`
  audited correct. `unsafe_c_alloc_native.lu` runs `exit(0)`.
- **#14: integer literals consult their context.** `-9223372036854775808`
  is writable in every annotated spelling: literals stay unconstrained
  through negation and literal-only arithmetic (i128-checked), a
  declared return type types the value a call returns, and
  `[arith.literal.default]`'s i32 rule (with a range check) applies
  where the literal meets its binding. `var k = 0` remains i32, the
  rule wolfc implements, now documented (approximation-contract §6.11).
- **#11: closed after the sc04 reopen.** The 0.1.3 cast matrix holds at
  this pin: the reopening program prints `3.0` / `converts` and exits 0;
  `(3 as f64) == 3` is false. What the reopen's evidence showed the
  matrix still misses is filed separately as #17: cast target types are
  not *resolved* (`s as nonsense` no-ops).
- **DIV-2026-010 closed.** s29 moved wolfc's E0410 to the resolve rung
  (with the `[conc.when.body]` exemption this machine flagged as
  wolf-lang#21) and re-pinned the corpus directives; the eighth
  differential compares both files clean. Eighth round: 165 entries, 18
  members, **1 divergence** (DIV-2026-011, filed), 268 conservatism
  entries.
- Realignment: `float_nan_cmp.lu` (IEEE `!=` is unordered; this
  machine's f64 model already agreed, and wolf-lang#22 was the
  compiler's half), `eu_main_err_exit.lu` (`error: Boom` + exit 1, agreeing with
  the native shim bit-for-bit), `resolve/same_name/` (two `len`s stay
  distinct). Bundle: 222 programs, 204 records; coverage ratchet raised
  84 → 86.

## 0.1.3 — 2026-08-10

The rows half, and the s27/s28 catch-up. Pin bumped `a0c4564` →
`d147a54` (the corpus grows `rows/qmark_defer.lu` and
`faults/assert_msg_holds.lu`: 175 → 177 files; the spec grows
`[mem.iter.*]`, `[mem.str.*]`, `[conf.trap.assert]` and the postfix-row
type grammar: 281 → 290 anchors). Three issues closed, one divergence
resolved, one filed.

- **#12: postfix rows, all three halves.** `type ::= type '!' error_row`
  parses in **every** type position (param, `let`/`var` annotation,
  nested); a bare lowercase name at a raise site resolves against the
  enclosing function's declared return row (`return none` under
  `-> int ! {none}` raises the tag); and resolution is **eager**: sema's
  `raise_check` refuses an unresolvable tag at the resolve rung whatever
  path the input takes, so the sc02 false-certification trap
  (`unsupported` only when the raise was *hit*) is structurally closed.
  Lowercase identifiers over tag-shaped scrutinees dispatch as row-tag
  patterns when they name a module-declared row tag; `else |err|` keeps
  its binder. The acceptance: wolf-std `std.option`'s six helpers (`or`,
  `expect`, `flatten`, `to_list`, `exists`, `is_none`, the F-0002 family,
  unwritable since sc01) execute under lupin, lowercase `none` included
  (`tests/rows_option.rs`).
- **#11 (silent-wrong): numeric casts convert.** `n as f64` produced an
  int that compared equal to ints and unequal to the float it claimed to
  be. `as` between numeric types now converts in every direction:
  int→float exact, float→int truncating toward zero with an X3 range
  check (NaN/∞/out-of-range trap `overflow`), int→int narrowing
  range-checks, `wrapping[T]`/`saturating[T]` targets reduce by their
  mode, `as f32` rounds through f32 precision (the one-f64 float model,
  approximation-contract §6.9). The non-bridges refuse like wolfc's
  E0805 (no truthiness, no `int as str`). `tests/cast_matrix.rs` pins
  the matrix, both directions of every pair.
- **#10: slice-of-binding receivers.** `d[0..1].upper()` refused at
  resolve (`d["0..1"]` does not denote a place) because the range key was
  stringified into a map-key projection. A slice expression is a value,
  not a place: `place_of` refuses it, the method call falls into the
  by-value receiver path, and `binding[range].method()` runs exactly like
  `literal[range].method()` always did.
- **The s27 spec realignments.** `[mem.iter.for]`: `for` over an
  `impl Iter for T` value desugars to the clause's drive loop
  (`next(mut self) -> T ! {done}` through call-by-value-result; range-for
  unchanged). Impl-block **method dispatch** lands with it, s17
  resolution order included (inherent wins; `Speak.speak(d)` reaches the
  shadowed trait method; trait default bodies stay `unsupported`).
  `[conf.trap.assert]`: `assert` is an intrinsic, never shadowed by a
  module fn, two-arg form's message evaluated **only** on the failing
  path, rendered to stdout before the trap (the counterparty's #19
  shape, from this side). `[mem.str.order]`: the executed byte-
  lexicographic ordering is now clause-backed and witness-tested. Twelve
  more corpus entries reach the run rung than at 0.1.2 (114 of 161;
  matches 76 → 84, out-of-scope 46 → 36, 0 mismatch).
- **DIV-2026-010 re-verified: still open at this pin.** A CLEAN wolfc
  build at `d147a54` reports `fail(E0410)@typecheck` where the corpus
  pins `phase: resolve`, and the sixth round's two divergences stand
  unchanged, filed and non-gating. The fix is in flight upstream (s29's
  resolve-rung `letcheck`, landed past this pin with CI still running at
  the close of this pass);
  wolf-lang#21 carries this machine's heads-up that the new walker must
  exempt `when`-body assignments per `[conc.when.body]` (its draft
  E0410-rejects `when (a, b) { a += 10 }` on `let`-bound Mutex operands
  that `conc/when_multi.lu` and `procs.lu` pin `run(exit=0)`). The
  seventh corpus differential is GREEN-with-filing: 161 entries, 2
  divergences, both DIV-2026-010.
- Bundle: 216 programs, 200 records, 290 anchors, ratchet floor 84
  (coverage +1: `[mem.model.order]` through `qmark_defer`'s own
  `conforms:` line; the nine new s27 anchors enter the debt list:
  their behaviors are exercised in `tests/`, and lifting them into
  `conforms:`-tagged suite programs is the next bundle's work).

## 0.1.2 — 2026-08-10

The lupin maintenance pass: five filed issues, four fixed, one routed
upstream. Pin bumped `cbde620` → `a0c4564` (the corpus grows the E0410
fail-files and the unsafe/checked memory tier: 164 → 175 files).

- **#5 (silent-wrong): bare-ident match patterns dispatch.** An
  identifier that names an in-scope enum variant is a *variant pattern*
  (matching the tag spelled bare or enum-qualified, payload half
  included); a capitalized identifier over a tag-shaped scrutinee is a
  structural row-tag pattern (D30); everything else binds, including a
  capitalized name over a non-error scrutinee, which is the
  counterparty's reading too. First-arm-always is dead:
  `match Ordering.Greater { Less => 1, Equal => 2, Greater => 3 }`
  yields 3, `corpus/typecheck/match_missing.lu` moves to its honest
  `exit(1)` in the run ledger, and a match no arm of which applies is
  `unsupported`, never a wrong answer. The out-of-grammar mirror image,
  bare *dotted* path patterns (`Ordering.Less =>`), now rejects at parse
  with the counterparty's exact E0201 shape (zero-width span at the token
  after the path). Approximation-contract §6.7 records the dynamic
  approximation. Same-scope `let` shadowing also reads the *latest*
  binding now (the `rposition` repair), which is `let_shadow_var_ok.lu`'s
  pinned `exit(0)`.
- **#7 (false UB): both `ub(mem.ub)` shapes of wolf-std F-0013 were one
  defect in `Provenance::drop_frame`:** it still parsed the pre-task
  `<frame>:<path>` place-key shape (keys are `t<task>:<frame>:<path>`
  since is06) and so dropped *nothing*: a callee's parameter binding
  survived its call, and the next call reusing that frame index and
  parameter name resolved its accesses through the stale tag: a Disabled
  read (shape a, the interpolated `mut` argument) or a protected-sibling
  foreign write (shape b, the allocating read-mode call). `drop_frame`
  now takes `(task, frame)` and forgets that task's frame and deeper,
  exactly. Both filed shapes are staged as regression tests
  (`tests/std_root.rs`), the transition table is untouched, and the ub
  matrix + ok-twins stay green, with no true detection weakened.
- **#8: `let` reassignment rejects at the resolve rung.** Sema-lite
  tracks binding mutability (params and pattern bindings are the mode
  system's business; `when` bodies assign through the acquired cell;
  shadowing rebinds): plain and compound assignment to a `let`-bound
  name is **E0410** with a `var` fix-it, span on the assigned place,
  byte-identical code+span to wolfc on the pin's fail-files. The
  interpreter half of wolf-lang#2, closing the last half of the
  divergence.
- **#6: lupin has a std root.** `--std-root DIR` on `run`/`check`/
  `conform-run`, `LUPIN_STD` as the flagless spelling: `use std.X[.Y]`
  resolves `<DIR>/X[/Y]/` through the normal loader (nested paths
  included; `std/x/deque_int` ships at that depth), the path's last
  segment is the bound name, and without a root the flat
  `<package root>/<last segment>` fallback keeps mirrors and sibling
  modules working. Mirrors wolfc's s26 `--std-root`/`WOLF_STD`; the
  wolf-std rig's flat-mirror interim can retire.
- **#9, mutation during `for` iteration: routed upstream, not
  legislated.** The pinned spec says nothing about `for`'s operand
  (no move, no extent-hold, no copy), and the implementations picked
  different readings (wolfc: E1001 static move; lupin: loop-entry
  snapshot, runs). Filed as **S-11** in the divergence log with both
  behaviors and the three candidate rulings; approximation-contract §6.8
  documents the machine's snapshot semantics. Compiler half wolf-lang#15;
  wolf-std keeps the divergence visible in CI.
- Conformance bundle: 214 programs / 198 records at pin `a0c4564`;
  anchor ratchet 79 → 83; manual, export ledger and corpus-harness
  ledger updated in the same breath.

## 0.1.1 — 2026-08-10

The bs00x maintenance pass: four filed issues, three fixed, one routed
upstream.

- **#1 (top severity): the DPOR closed-frontier miss is fixed.** Two
  scheduler defects starved the explorer's backtrack sets: a send that
  committed a `select` arm consumed the selecter's registration on every
  channel of the select but recorded only the sent-on channel (the
  conflict alphabet missed the coupling; `op_select_consume` is the fix),
  and channel sends published the sender's vector clock without ticking it
  past the snapshot (spawn and mutex release already ticked), so the
  sender's later ops carried false happens-before edges. The filed
  reproducer (wolf-book ex17-1, the lost-update server) now shows both
  outcomes (`balance=50` and `balance=100`) inside a *closed* frontier,
  identical to naive DFS, and is pinned as a regression litmus
  (`tests/explore_machine.rs::the_select_coupled_lost_update_is_inside_the_closed_frontier`).
  The pinned `conc/` exploration ledger re-ran with **no count movement**
  and every verdict-stability oracle still green.
- **#2: write-after-freeze now traps on value paths.** Struct values
  carry the region charged at their allocation site
  (`Value::Struct::home`), and `write_path` refuses a write through a
  container homed in a `Frozen` region: `region-fault
  [mem.region.freeze.1]`, before anything is mutated. That is E1012's
  shape executed, agreeing with wolfc's static rejection. Reads and rebinding
  stay legal (`tests/faults/region_freeze_value_write.lu` + its
  `region_freeze_rebind_ok.lu` twin; approximation-contract §6.1 records
  the remaining list/map gap). `memory/region_freeze_write.lu` moves from
  `exit(7)` to `trap(region-fault)` in the run ledger.
- **#3: `when`-arity code aligned to wolfc.** The malformed-`when`
  sentence now carries **E0201** (the established generic expected-token
  assignment) instead of the invented E0203, which collides with wolfc's
  toplevel-decl family. Message and `[gram.expr.conc]` anchor unchanged;
  `diag::UNPINNED_CODES` (the published choices table) updated.
- **#4, cross-task capture-by-copy: routed upstream, not patched.**
  spec/03 makes the mut-capture shape a compile error (E1101) and states
  no runtime meaning; inventing a spawn-time trap would be legislating.
  Capture-by-copy stays the documented interpreter semantics
  (approximation-contract §10.2, corpus exemplar `conc/store_buffer.lu`),
  and the gap is filed as **S-10** in the divergence log for a spec ruling
  (the E1004/E1005 precedent).
- Conformance bundle: 203 programs / 187 records (the two new freeze
  litmuses); manual and export ledger updated in the same breath.

## 0.1.0 — 2026-08-10

The binary is named `lupin`; the package and repository stay `wolf-interp`.

- `lupin FILE.lu` runs a file; `lupin -` reads a program from stdin; bare
  `lupin` opens the REPL. The exit code reports the program: its own
  `exit(N)`, 2 on a static-phase rejection, 3 on a trap or UB finding, 4 on
  `unsupported`.
- `run` is the explicit subcommand spelling and carries `--seed`,
  `--schedule` and `--json` (the spec/06 observation record at the front
  door). A subcommand name wins over a file of the same name.
- `lupin eval 'CODE'` (short spelling `-e`) evaluates a snippet in a fresh
  session and prints what the REPL would print.
- `lupin check FILE...` runs the frontend only (lex, parse, resolve).
- Observation records and bundle manifests report `"impl": "lupin"` with
  `impl_version` 0.1.0; `--version` prints the version, the package, and
  the upstream pin.
