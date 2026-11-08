# s288 — lupin mirrors the compiler's front-end rulings (wolf-lang#1079 and #1080)

wolf-lang's compiled tiers now refuse, at the front end, what the specification now says is
wrong: a write to a name a pattern binds, a write to a `let` through an arm that only tested its
name, a variant or a field declared twice, a name bound twice in one pattern or parameter list,
or-pattern alternatives that bind different names or one name at two types, an error row that
spells a tag twice or meets an alias's tag with another payload, and a type that contains itself
by value. lupin 0.1.60 ran all of them. This change gives the compiler's code at the compiler's
span for every row of the two changes' lists, at this machine's one static rung, `resolve`
(`[proto.cmp.rung]` makes a resolve-rung emission of a checker code agreement).

## Forbidden

Footprint: module

The diff is `src/` (the alias rewrite, the member-name pass, the assignment walk and the resolve
chain in `sema.rs`; the refusal ladder in `frontend.rs`; the pattern-names hook in `rowmatch.rs`;
a span-free type spelling in `ast.rs`; two new passes, `patnames.rs` and `recursion.rs`), the
mirror rows and their fixtures, the corpus and conformance ledgers, one filed divergence
(`differ.rs`, `docs/divergence-log.md`), the changelog and notes. `vendor/upstream/` is not
touched, and there is no CI, pin or spec change. Clean room: the spec at wolf-lang `a57276f2`,
`docs/diagnostics.md` there, the compiler's corpus rows, records and rendered diagnostics as
black-box answers, and lupin's own source; no compiler crate was read. No label, merge, tag,
release, issue filed or closed, and no attribution trailer. The plant is pushed without a PR.
Nothing ran under `/tmp` or `/var/folders`. No commit was made on a Linux host.

## Inputs, verified

- **Base:** trunk `1ed91d1` (s285), at the upstream pin `5f1f1c94`.
- **The compiler's changes:** wolf-lang#1079 (`d59086f7`) and #1080 (`a57276f2`), both merged,
  their bodies' "For the lupin mirror" lists, and the clauses `[gram.pat.nullary.scope]`,
  `[gram.item.type.names]`, `[gram.pat.names]`, `[gram.pat.or]`, `[gram.pat.let]`,
  `[gram.type.row.dup]`, the amended `[type.err.alias.union]` and `[type.rec]`, with the E0302,
  E0401, E0404, E0410, E0601, E0609, E0827 and E0828 entries of `docs/diagnostics.md`.
- **The compiler's records:** a debug build of `a57276f2` in its own worktree (binary sha256
  `a9a07c70…0d6c`). Its 50 rows are copied byte for byte (sha256 read on both sides) under
  `tests/front_end_rulings_s288/<row>/main.lu`, each beside `compiler.json`, its record from
  `conform-run --json --checked`, the `file` written as the corpus path. `--native` and
  `--release` gave the same verdict and the same diagnostics on every row, and the spans equal
  those of wolf-lang's four-machine test (`crates/wolf_driver/tests/front_end_rulings_lanes.rs`).
- **The compiler's edges:** 23 programs of this lane's own (`probe_<shape>/`), measured the same
  way on three tiers before the mirror was written, for what the rows leave open (below).
- **lupin 0.1.60:** the release archives (`aarch64-apple-darwin` sha256 `f3cae16f…6bb8`,
  `x86_64-unknown-linux-gnu` `e7d4c4f1…637e`), each read twice and equal to the release page.
- **The machine:** E0410 was the lexical walk `sema::reassign_check`, which declared every name a
  pattern binds as assignable and read every bare arm name as a binding. The resolve rung found
  E0302 between items only, E0601 only for an alias entry's payload, and nothing for E0404, E0609,
  E0827 or E0828. The alias rewrite merged a row's own repeated tag silently and kept the first of
  two payloads. The record carries every diagnostic a refusal lists, the error first and the
  warnings after it.
- README read, current: it states nothing these rules change.

## Prediction

Committed before the first change (`7949b51`, `notes/s288-prediction.md`). Scored:

- **The base: hit, with one miss of wording.** lupin runs 36 of the 37 refusal rows with the exits
  the prediction named, and answers the 13 controls as the compiler does but for the three
  agreeing or-pattern rows, which the compiled machines refuse at `mem` and lupin runs. The miss:
  `or_pattern_binds_missing`'s `unsupported` comes during the run (`x-unsupported-at: run`, the
  arm's `x` does not resolve when `B` matched), not from a resolve check.
- **The rows red first: hit.** At the rows' commit (`49fce84`, trunk's code),
  `mac-red-49fce84.log`: 11 passed, 36 failed, every failure at the verdict assertion with the
  base's answer, and every control already equal to the compiler's diagnostics, warnings
  included.
- **Where each code lives: hit,** every one at the predicted pass and span:
  - E0410 in the assignment walk, at the assignment's place, for the seven `[gram.pat.let]` rows
    and the three nullary-arm rows (`9508b66`);
  - E0302 at the later member's name (`4d6a6b6`);
  - E0601 at the repeated tag's name, and E0609 at the row's own spelling or the later alias
    entry, in the alias rewrite, which now reads every row (`92906b8`);
  - E0827, E0828 and E0401 in `rowmatch`'s body walk (`f3b6951`);
  - E0404 at the member through which the first-declared type reaches itself (`f5b7962`).
- **Every finding of a code: hit.** The rows hold `type_decl_member_twice`'s two E0302s, and the
  probes the measured repeats: `{x, x, x}` two E0601s, three `A`s two E0302s, `(x, x, x)` two
  E0827s, two lacking alternatives two E0828s (`dbd8fd3` reports each family whole).
- **The rows green: hit.** At the code head all 63 tests pass (the 47 rows and 16 probe tests;
  `linux-check-c8f3c48.log`): verdict, first error, and the diagnostics set equal to the
  compiler's, byte for byte once sorted by span.
- **The vendored corpus: hit, two files move and nothing else** (`census.txt`, 1,253 files,
  base against head). `rows/negative/dup_tags.lu` goes from `exit(7)` to the `fail(E0601)` it pins
  and leaves the run ledger (`cbdc792`). `memory/elem_loop_induction_written_mut.lu` goes from
  `trap(exclusivity)` to `fail(E0410)`. The prediction had its vendored copy take wolf-lang's
  `var i = i`; by decision (R-s288-1) the snapshot stays as vendored and the row is filed as
  DIV-2026-029 until the next pin advance carries the copy (`cac3fdb`). wolf-lang's copies moved
  the same way: `dup_tags.lu` was E0601 on the compiled tiers before and after, and the witness's
  E1002 holds there through `var i = i`.
- **lupin's own tests: a miss.** Predicted none would move. Two conformance tests did, both at
  `dup_tags.lu`, which they held to `Pass` at resolve and `unsupported` deeper: E0601 now joins
  their table of codes this machine decides at resolve, keyed on the row's clause (`cbdc792`).
  The library tests (797), the binder rows (9), the literal-pattern rows (7) and the corpus harness
  (10) did not move.
- **A miss of the rig, not of the machine:** the first build of the fix named its scope entry
  `Local`, which `sema.rs` already uses for a type alias; it was renamed before any test ran.
  Clippy's `manual_contains` and `cargo fmt` each wanted one change before the checks were green.
- **A miss of the rig, found by the first Linux take:** the gauntlet's `independence-lock` step
  regenerates the lockfile offline (CI regenerates it online), and `clipboard-win 5.4.1`, which
  the committed `Cargo.lock` pins, has since been yanked, so an offline resolution refused it: `END
  independence-lock exit=101` at the first head, before any test step. The take is void. The
  prefetch, outside the locks, now also stages the crates the regenerated lockfile picks
  (`clipboard-win 5.4.3` among them) and restores the committed `Cargo.lock` byte for byte. The
  committed lockfile still builds (`--locked` uses a yanked version it already pins), and CI's
  online regeneration picks the same replacements.
- **The plant:** in the PR body (a record cannot predate the head it names).

## Decisions

- **R-s288-1 = C:** `vendor/upstream/` stays the snapshot at its pin. The witness that writes its
  `for` header's name is filed as DIV-2026-029, and the next pin advance re-vendors wolf-lang's
  copy and retires the entry. The footprint stays `module`.
- **R-s288-2 = A:** the rows compare the `{code, span, severity}` set, sorted by span, byte for
  byte, plus the first error. This machine writes the error first and its warnings after it
  (`[proto.cmp.phase]` compares the first error); the compiler writes span order. The order stays.
- **R-s288-3:** the compiler at `a57276f2` does not return on `enum L[T] { Nil, Cons(T, L[(T,
  T)]) }`, a recursion that only grows its arguments, which `[type.rec]` says the check does not
  report. Filed as wolf-lang#1081. This machine's walk stops at 32 inline levels or a
  4096-character type spelling, its own bound, not the spec's, and answers the program `exit(0)`
  (`a_recursion_that_grows_its_arguments_is_not_reported`).
- **R-s288-4:** whether a bare arm name tests or binds is this machine's evaluator's rule, so the
  checks and the run cannot disagree. It tests when it names a variant of an enum the module
  declares, or, at the top of the arm, a tag of the row the scrutinee is statically known to carry
  (`rowmatch`'s reader). The compiler decides by the scrutinee's type. They differ where an arm
  names a variant of an enum other than the scrutinee's type (a test here, a binding there) or of
  an imported enum (a binding here): there this machine reports no E0827 or E0410 the compiler
  might, the permissive direction, and never one the compiler would not.
- **R-s288-5:** E0609 between the layers of a nested row with no alias (`T ! {a} ! {b}`,
  `rows/negative/nested_row_conflict.lu`) is not on the list and stays as it was; it is a
  follow-on.
- **Where an E0401 is judged:** only a type a declaration states, a variant's payload at its
  position or a struct's field, as spelled. A type that mentions a generic parameter or an alias
  is never judged, so `type Id = int` and `int` cannot read as two types here.
- **What is never judged as a row's own tag:** an alias entry (`{A, A}` runs, as measured), a name
  a `use` binds (it may be a foreign alias, which only the whole-program pass can read), and a
  qualified tag (`io.Eof`). The last two are the permissive direction.
- **The anchors:** none of the new clauses is in the pinned spec, so each diagnostic cites the
  nearest pinned anchor (`gram.item.let`, `gram.item.type`, `gram.pat`, `gram.type.row`,
  `type.err.alias.union`) and names its clause in its words, as s285 did.
- **Not on the list, unchanged:** a closure's unmarked parameter stays assignable
  (wolf-lang#1076), and a generic parameter declared twice is not refused (wolf-lang#1074).

## Evidence index

`notes/s288-evidence/index.txt` lists every file:
- `mac-wolf-build-a57276f.log`: the compiler build whose records the fixtures carry;
- `mac-red-49fce84.log`: the rows at trunk's code, red by name;
- `linux-check-c8f3c48.log`, `linux-census-1ed91d14.log`, `census.txt`: the code head's checks
  and the corpus census against trunk;
- `rig/`: the scripts as run, the Linux gauntlet's among them.

The Linux gauntlet record at the final head and the plant record on it live outside the tree, and
the PR body quotes every `END` line of each.

## Done-when

Commits:
- `7949b51`: the prediction, before the first change.
- `b858049`: the 50 rows and the compiler's records. `49fce84`: the rows, red first.
  `791d0ba`: their formatting.
- `cb9f9bc`: a type, a path and a row spelled without spans.
- `92906b8`: every error row judged for a tag spelled twice (E0601) and an alias payload it
  contradicts (E0609).
- `4d6a6b6`: a variant or a field declared twice (E0302).
- `9508b66`: a pattern's names bind as `let` does, and a tested arm name binds nothing (E0410).
- `f3b6951`: a name bound twice (E0827), or-pattern alternatives that disagree (E0828, E0401).
- `f5b7962`: a type that contains itself by value (E0404).
- `dbd8fd3`: each family reported at the resolve rung, every finding of its code.
- `cbdc792`: `dup_tags.lu` held to the E0601 it pins, in the run ledger and conformance.
- `cac3fdb`: the vendored witness filed as DIV-2026-029.
- `e65b7e5`: the 23 probes of the compiler's edges.
- `c8f3c48`: the changelog.
- This note and its evidence.

Test targets:
- [x] the prediction committed before the first change (`7949b51`)
- [x] the rows red at trunk's code, by name: 11 passed, 36 failed (`mac-red-49fce84.log`)
- [x] the rows and probes green at the code head, the compiler's diagnostics byte for byte
      (63 tests), with fmt, clippy (all targets, `-D warnings`), the library tests, the binder
      and literal-pattern rows, the corpus harness and conformance (`linux-check-c8f3c48.log`)
- [x] the vendored corpus walk green, two files moved and named (`census.txt`)
- [ ] the Linux gauntlet record at the final head, every `END … exit=` (in the PR body)
- [ ] the plant red by name (in the PR body)
- [ ] the audit, once

Follow-on, decided: wolf-lang's four-machine test holds lupin to its present answers until the
pin moves. When wolf-lang next pairs with a lupin release that carries this change, those rows
switch to the compiler's code by themselves: every refusal row's record here carries the
compiler's code at the compiler's spans, `type_decl_member_twice`'s two E0302s included. In the
other direction, the next pin advance past `a57276f2` re-vendors the witness with `var i = i`:
DIV-2026-029 retires, and the row returns to the run ledger as `trap(exclusivity)`. The rows of
#1079 and #1080 arrive with it as these fixtures byte for byte.
