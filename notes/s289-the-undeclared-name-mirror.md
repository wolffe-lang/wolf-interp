# s289: the undeclared-name mirror (wolf-lang#1085)

The compiler's diagnostics sweep, first change (wolf-lang's s289 branch, the PR that names
wolf-lang#1085), reports a capitalized name that nothing declares as **E0301** wherever no error
row takes it, and a tag the function spells where the function cannot fail as **E0604**. This
machine read every such name as a tag value and ran the program (`error: OPEN`, exit 1), so each
of the compiler's eight new rows was a divergence. This change mirrors both at the resolve rung,
with the compiler's code at the compiler's span.

## Forbidden

Footprint: module

The diff is the resolve rung's walk and one new check (`src/sema.rs`), `can_fail` made visible to
it (`src/rowmatch.rs`), a mirror test with its eight rows and the compiler's records, the changelog
and notes. No vendored corpus, pin, CI or toolchain change. Clean room: the spec, the compiler's
published rows and records, black-box runs of the compiler, and no other implementation's source.
No label, merge, tag, release, issue closed, and no attribution trailer. Nothing ran under `/tmp`
or `/var/folders`, and no commit was made on a Linux host.

## Inputs, verified

- **Base:** wolf-interp trunk `725d86da`. lupin 0.1.60 is the paired release. Nothing between
  them touches the reading of a bare name or the ambient table.
- **The compiler's side:** wolf-lang's s289 branch at `67bd054f`. The eight rows are copied byte
  for byte, with `wolf conform-run --json --checked` records from a debug build of that commit on
  arm64 macOS; native and release agree with the checked machine on every row.
- **Builds:** every build ran in a worktree pinned to a commit, on the second arm64 macOS machine,
  behind its build lock.

## Prediction

`notes/s289-prediction.md` (the mirror's prediction, made on the compiler's side before this
repository's first change, and what the first take moved):
- At the rows' commit, `fae400a`, the eight mirror tests were red by name: lupin exits 1, 1, 1,
  1, 1, 1, 0 and "unsupported" where the compiler says E0301 or E0604.
- At the check's first take, `6e5220a`, the eight were green and one library test went red: an
  `impl`'s generic inside a bracket, `List[T]()`. It is held by `ada66b3`.
- At `ada66b3` the mirror test (8), the s288 mirror (64), the library's tests (796), the corpus
  harness, fmt and clippy are green.

## Evidence index

`notes/s289-evidence/`:
- `red-fae400a-air.log`: the rows red at their commit.
- `fix-6e5220a-air.log`: the first take, the library test red.
- `fix-ada66b3-air.log`: the take at the fix, green.

The gauntlet's record at this change's head is cited in the PR.

## Done-when

Commits:
- `eaf4641`: the eight rows and the compiler's records; `fae400a`: the test that holds this
  machine to them.
- `4c076a7`: `can_fail` for the resolve rung; `6e5220a`: the check; `ada66b3`: an `impl`'s
  generics and a bracket's arguments.
- `7315d56`: the prediction's record; `285db60`: the changelog; this note and its evidence.

Test targets:
- [x] the rows red at their commit, by name (`red-fae400a-air.log`)
- [x] the rows green at the fix; the s288 mirror, the library's tests, the corpus harness, fmt and
      clippy green (`fix-ada66b3-air.log`)
- [ ] the gauntlet (every step's `END`) at this head on Linux x86-64
- [ ] the plant red by name at `an_undeclared_name_as_a_tail_is_e0301`, its controls green
- [x] README read, current: it names no diagnostic this change adds
