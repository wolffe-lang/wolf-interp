# s282 — lupin mirrors the binder warning (W0801) and the select-arm refusal (E0806)

The compiler now warns W0801 at a bare name in a binder position that spells a variant of an
enum in scope, in either letter case (`let (Drink, n) = pair`, `let (fast, n)` over
`enum Mode { fast, slow }`), and refuses such a name in a `select` arm's receive (E0806); its PR is wolf-lang's of the same name, and its clause is
`[gram.pat.binder]` there. The rule needs resolution alone (scope decides, not type), so this
machine runs it at the same rung: its warning walk reports W0801 at the compiler's spans, the
`select` arm is a static E0806, and an `else |…|` handler whose name spells a variant now binds
the error, as the compiler binds it, where it read a test and refused at run. W0801's match-arm
half (a capitalized arm name over a scrutinee with no cases) needs case tables and stays
honest-absent, so the corpus ledger enforces W0801 one way.

## Forbidden

Footprint: module

The diff is `src/` (the new `binder.rs`, the warning walk in `lint.rs`, the handler in
`eval/mod.rs`), the corpus ledger test, the mirror rows and their fixtures, the changelog and
notes: one repository's program, no CI, pin, spec or vendored-tree change. Clean room: the
spec at this pin, the compiler's corpus rows as black-box answers (`wolf conform-run --json` on
the three compiled tiers), and lupin's own source. No label, merge, tag, release, issue filed or
closed, and no attribution trailer. The plant is pushed without a PR. Nothing ran under `/tmp`
or `/var/folders`. No commit was made on the Linux host.

## Inputs, verified

- Base: trunk `f8c2ff7` (the upstream pin `5f1f1c94`). The compiler's branch `s282` at wolf-lang,
  its rows (`corpus/lints/binder_variant_*.lu`, `corpus/conc/select_arm_*.lu`) and their records
  on the checked machine, native and release.
- lupin 0.1.59 (the release archive, digest equal to the release page, read twice) answers the
  binder rows the compiler's way except three: no W0801 anywhere (honest-absent), the handler
  row refuses at run ("the `else |pat|` pattern did not match the error value"), and the
  `select` row refuses at run ("the committed `select` arm's pattern did not match").
- `Module::variants` (variant → the module's enums declaring it) is what the runtime's variant
  pattern rule reads; the warning walk had no binder check; `lint::HONEST_ABSENT` listed W0801.
- README read, current: it states nothing about the warning analyses.

## Prediction

Committed before the first change (`a1f3ace`, `notes/s282-prediction.md`). Scored:

- **The rows red at trunk: hit.** At `7823d8a` (the rows, trunk's code) `binder_variant_s282`
  fails five rows by name (no W0801 at the let, `for` and imported rows; `unsupported` for the
  handler and the `select` rows) and passes the two controls
  (`notes/s282-evidence/mac-interp-red-7823d8a.log`).
- **W0801 in the walk: hit.** At `d76649c` all seven rows pass: W0801 at exactly the compiler's
  spans on every binder row, nothing on the controls (`mac-interp-fix-d76649c.log`). Built from
  this branch, lupin answers the compiler's own corpus rows with the same verdicts, stdout and
  W0801 spans as the three compiled tiers (wolf-lang's `notes/s282-evidence/mac-measure-447ba6d8.log`,
  its last step).
- **In scope: as predicted.** The module's enums (`Module::variants`, sorted by name as the
  compiler orders them), then the enums the file imports by item, read off the file's own `use`
  items (`use` is file-scoped), found in their module the way the error-alias reader finds an
  alias. The std tree is not exempt, matching the compiler.
- **Every letter case: a miss, decided against the prediction.** The prediction had only
  capitalized names count. The compiler's rule is the one its `match` payloads use to decide
  that a bare name tests a variant, which compares names as written with no letter-case test;
  this machine's own variant rule is the same (`declares_variant`, `src/eval/mod.rs:6637`,
  looks the name up in `Module::variants`). So `binder.rs` and the walk drop their capital
  test (`b0b521c`), and the rename a help names is the snake-case spelling, with a trailing `_`
  when that is the name itself (`fast` becomes `fast_`). The compiler's lowercase rows
  (`lints/binder_variant_lowercase.lu`, `conc/select_arm_lowercase_variant.lu`) are mirrored
  byte for byte, red first (`310c4dd`, `ab87b56`): at `ab87b56` (the capitalized-only code)
  the mirror rows are 7 passed and 2 failed by name, no W0801 against `[566, 570]` and
  `[630, 634]`, and the `select` row `unsupported` against `fail(E0806)`
  (`mac-interp-red-ab87b56.log`); at `b0b521c` all 9 pass, with fmt, clippy, the library
  tests and the lint rows green (`mac-interp-fix-b0b521c.log`).
- **The handler binds: hit.** `else |Drink| 7` runs and prints `7` where it refused. Only a bare
  name that spells a variant the module declares changes; every other handler shape (a row tag,
  a lowercase name) goes through the arm rule as before.
- **E0806: hit.** A static refusal at the resolve rung, at the name's span, in either letter
  case, naming `Kind.Drink`, with another name for the receive as its help.
- **The ledger: as predicted.** `lint::PARTIAL` holds W0801; the warns-ledger test enforces it
  one way. No vendored corpus file has a capitalized binder name or a variant-spelled `select`
  receive or handler (a search of the vendored tree), so neither ledger moves.
- **The direction: this machine's W0801 is a subset of the compiler's.** The compiler may
  report W0801 where lupin does not; lupin never reports one the compiler does not. lupin built
  at `b0b521c` and the compiler at its case-blind head were run over every file of the
  compiler's corpus, these mirror fixtures and the compiler lane's probes, each stopping at a
  static rung, and their W0801 spans compared file by file (wolf-lang's
  `notes/s282-evidence/mac-remeasure-83a64499.log`): no span is lupin-only. Two are
  compiler-only. One is the match-arm half, `lints/binder_capitalized.lu` (449..452), which
  needs case tables; it is why the ledger stays one way. The other is a real gap in the
  binder half: an enum declared as `type Kind = enum { Drink, Food }` has no variants in
  `Module::variants`, so a binder over `Drink` warns on the compiler and not here (and a
  `match` arm `Drink =>` over it does not apply, `unsupported`, at trunk too). It is in no
  corpus file; it is filed as an issue. Every other file with a W0801 has equal span sets.
- **A miss:** the first green take's `cargo fmt --check` failed on two long lines;
  `436f213` formats them.

## Evidence index

`notes/s282-evidence/`:
- `mac-interp-red-7823d8a.log`: the mirror rows at trunk's code, five red by name;
- `mac-interp-fix-d76649c.log`: the mirror rows green; fmt (red, two lines), clippy (all
  targets, `-D warnings`), the library tests, the lint mirror rows (`lint_is61`), the release
  build kept for the compiler's four-machine rows;
- `mac-interp-red-ab87b56.log`: the lowercase-variant rows at the capitalized-only code, two
  red by name;
- `mac-interp-fix-b0b521c.log`: the case-blind code, every mirror row green, and the gates.

The records at the final head live outside the tree. The PR body quotes every `END` line of the
gauntlet record and of the plant record.

## Done-when

Commits:
- `a1f3ace`: the prediction, before the first change.
- `7f005e7`, `7823d8a`: the fixtures (the compiler's rows, byte for byte) and the mirror rows,
  red first.
- `af694ea`: `binder.rs`, the reading of "in scope" and of a binder pattern's names.
- `da63361`: W0801 at binder names in the warning walk; E0806 at a `select` receive; W0801 moves
  from `HONEST_ABSENT` to `PARTIAL`.
- `3ea0af1`: the handler binds a variant-spelled name.
- `d76649c`: the corpus ledger enforces `PARTIAL` one way.
- `5a39494`: the changelog. `436f213`: formatting.
- `310c4dd`, `ab87b56`: the compiler's lowercase-variant rows mirrored and pinned, red first.
- `b0b521c`: binder names read in either letter case. `b18b73e`: the changelog.
- This note and its evidence.

Test targets:
- [x] the prediction committed before the first change (`a1f3ace`)
- [x] the mirror rows red at trunk's code by name (`mac-interp-red-7823d8a.log`); the
      lowercase-variant rows red at the capitalized-only code (`mac-interp-red-ab87b56.log`)
- [x] the mirror rows green; clippy; the library tests; the lint mirror rows
      (`mac-interp-fix-d76649c.log`; case-blind, with fmt, `mac-interp-fix-b0b521c.log`)
- [x] the compiler's rows on lupin built from this branch: verdicts, stdout and W0801 spans equal
- [x] lupin's W0801 a subset of the compiler's over the corpus, the fixtures and the probes
- [ ] `run_corpus` and `lupin corpus` at the head (the Mac, before the record)
- [ ] the gauntlet record at the final head, every `END … exit=0` (in the PR body)
- [ ] the plant red by name (in the PR body)
- [ ] the audit, once

Issues, filed once both PRs are open, each with its program and output (linked from the PR, no
closing keyword):
- lupin cannot import a function by item (`use kinds.name`: "`name` does not resolve").
- lupin refuses (E0301) the use of a name a struct pattern's shorthand bound when an enum in
  scope declares a variant of that name (`let Q { Drink, n } = …`, then `Drink + n`); the
  compiler binds it.
- lupin reads an `else |Boom|` handler name that spells a tag of the operand's row as a test
  and refuses statically (E0809); the compiler binds a bare handler name.
- At the next pin advance the vendored corpus gains the compiler's binder and `select` rows;
  the mirror rows here are those rows byte for byte, so the advance should move no answer, and
  the exact-code lists in `tests/conformance.rs` should gain E0806.
- lupin does not register the variants of an enum declared as `type Kind = enum { … }`.
