# s282 — lupin mirrors the binder warning (W0801) and the select-arm refusal (E0806)

The compiler now warns W0801 at a capitalized bare name in a binder position that spells a
variant of an enum in scope (`let (Drink, n) = pair`), and refuses such a name in a `select`
arm's receive (E0806); its PR is wolf-lang's of the same name, and its clause is
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
  alias. Only capitalized names. The std tree is not exempt, matching the compiler.
- **The handler binds: hit.** `else |Drink| 7` runs and prints `7` where it refused. Only a bare
  name that spells a variant the module declares changes; every other handler shape (a row tag,
  a lowercase name) goes through the arm rule as before.
- **E0806: hit.** A static refusal at the resolve rung, at the name's span, naming
  `Kind.Drink`, with the lowercase receive as its help.
- **The ledger: as predicted.** `lint::PARTIAL` holds W0801; the warns-ledger test enforces it
  one way. No vendored corpus file has a capitalized binder name or a variant-spelled `select`
  receive or handler (a search of the vendored tree), so neither ledger moves.
- **A miss:** the first green take's `cargo fmt --check` failed on two long lines;
  `436f213` formats them.

## Evidence index

`notes/s282-evidence/`:
- `mac-interp-red-7823d8a.log`: the mirror rows at trunk's code, five red by name;
- `mac-interp-fix-d76649c.log`: the mirror rows green; fmt (red, two lines), clippy (all
  targets, `-D warnings`), the library tests, the lint mirror rows (`lint_is61`), the release
  build kept for the compiler's four-machine rows.

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
- This note and its evidence.

Test targets:
- [x] the prediction committed before the first change (`a1f3ace`)
- [x] the mirror rows red at trunk's code by name (`mac-interp-red-7823d8a.log`)
- [x] the mirror rows green; clippy; the library tests; the lint mirror rows
      (`mac-interp-fix-d76649c.log`)
- [x] the compiler's rows on lupin built from this branch: verdicts, stdout and W0801 spans equal
- [ ] `run_corpus` and `lupin corpus` at the head (the Mac, before the record)
- [ ] the gauntlet record at the final head, every `END … exit=0` (in the PR body)
- [ ] the plant red by name (in the PR body)
- [ ] the audit, once

Issues, for the orchestrator to file:
- lupin cannot import a function by item (`use kinds.name`: "`name` does not resolve").
- lupin reports E0301 on a struct literal whose field is capitalized (`Q { Drink: 5 }`).
- lupin reads an `else |Boom|` handler name that spells a capitalized tag of the operand's row
  as a test and refuses statically (E0809); the compiler binds every handler name.
- The vendored corpus gains the compiler's binder rows at the next pin advance; the mirror rows
  here are those rows byte for byte, so the advance should move no answer.
