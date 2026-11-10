# is87 — a pseudo-terminal, mirrored (wolf-lang s292), and argv_is84's ETXTBSY (wolf-interp#360)

wolf-lang#1091 (branch `s292`, head `5e89fdbe`, open at this note) adds five calls:
`os_term_open_pty() -> (int, int) ! {io, unsupported}`, a pseudo-terminal's master and slave;
`os_term_pty_name(fd) -> str ! {io, unsupported}`, the slave's name for a master;
`os_term_set_size(fd, rows, columns, xpixels, ypixels) -> () ! {invalid, io, unsupported}`, the
window's size set; and `os_term_attr(fd, op) -> int` and `os_term_set_attr(fd, op, value) -> ()`,
`! {invalid, io, unsupported}`, one of a terminal's settings by RFC 4254 section 8's opcode
(`[os.term.pty]`, `[os.term.size.set]`, `[os.term.attr]`, the design note `[os.term.pty.note]`).
lupin 0.1.60 resolved none of the five. The pull request's section "For the lupin mirror" says
what lupin answers: every row decided before a terminal is reached, and a refusal by name for the
rest. That is the first change here. The second is the test fix wolf-interp#360 asks for:
`argv_is84`'s shebang test failed now and then with `ETXTBSY`.

## 1. Forbidden

- No `unsafe`, no new dependency and no new crate feature; no host call made for any of the five.
- No pin, spec, vendored, CI or toolchain file changes: `vendor/upstream/` stays at `5f1f1c94`.
- No answer of another call moves: the other `[os.term]` calls answer as they did.
- The interpreter shares no implementation code with the compiler: the clauses, the corpus file
  and the pull request's text are the inputs, never the compiler's crates.
- The test fix changes the test only: no retry, no lock, no change to lupin.
- No label, merge, tag, release or issue closed by this branch; no attribution trailer.

## 2. Inputs, verified

| input | re-derived |
|---|---|
| trunk | `921b7e4`, lupin 0.1.60, `vendor/upstream/PIN` `5f1f1c94` |
| the compiler's change | wolf-lang#1091, head `5e89fdbe` (49 commits on its trunk `8a3d1ac8`), open, mergeable; the head did not move during this change |
| the clauses | `spec/11-os.md` at `5e89fdbe` (blob `413de821`): `[os.term.pty]`, `[os.term.size.set]`, `[os.term.attr]`, `[os.term.pty.note]`, and the five lines of `[os.host.sigs]` |
| the corpus file | `corpus/os/pty_rows.lu` at `5e89fdbe`: blob `0d74b900`, sha256 `3bcacf544bb848cfbb4674dbec93cb17f40c209e140d71b280e19dc471653c24`; `tests/fixtures/pty_s292/pty_rows.lu` is the same blob |
| the compiler's lupin leg | the pull request's text: `pty_lanes`' lupin row expects `exit(0)` from the corpus file from any release other than 0.1.60 |
| lupin 0.1.60 | the `aarch64-apple-darwin` release archive, sha256 `f3cae16fdb88a94702c0fd816d5eb90cdc8e8636c028dcf40f03868c5dc36bb8`: the corpus file stops at "`os_term_set_size` does not resolve", prints nothing, and `lupin run` exits 4 |
| the machine | `AMBIENT_NAMES`, the dispatcher, the wasm decline list and `declared_row` named none of the five; `FsTable::is_terminal` (std's `IsTerminal`) is the handle reader `os_term_set_mode` already uses |
| wolf-interp#360 | the failure at trunk `d8c23bbd`, ubuntu, shard 3: `Os { code: 26, kind: ExecutableFileBusy }` at the shebang test's run |
| README | read, current: it states nothing these changes move |

The section and the clauses agree on every row. Two points the clauses leave to the section, and
the section decides: the name answers `io` only for a closed or forged handle (the clause calls a
file `io` too, but telling a master from a file is the host's `ptsname_r`, which this machine
cannot ask), and the target's lacking ops are each target's own list (linux and PAX, macOS).

## 3. Prediction, scored

Committed before the first change (`75ed627`, `notes/is87-prediction.md`).

- **The names resolve: hit.**
- **The shapes, on every host: hit.** 56 shape calls (nine size calls with a field outside
  0..65535, fifteen ops outside the vocabulary for each of the two settings calls, seventeen set
  values outside their op's range) answer `invalid` on this host and as the windows, linux, macOS
  and bundle targets.
- **Then the host: hit.** Every well-formed call answers `unsupported` as the windows target;
  linux and the bundle answer it for 11, 15 and 17, macOS for 15, 16, 37, 52 and 71, and every
  other op of the 56 reaches the handle (`io` on a forged one); a set of 128 or 129 to a speed
  outside the table is `unsupported`, one in it `io`.
- **Then the handle: hit.** A closed handle, a forged one, -1 and a never-opened 7 answer `io` for
  all four calls that take one; a file and a null stdin answer `io` for the size and the two
  settings calls; the name on a file or a standard stream is refused by name.
- **Refused by name: hit.** The open on every unix target and this host; on a pty the test opens
  around lupin, the size, a flag read, a speed read, a flag set, a speed set and the name, each
  after the three rows decided before the terminal (`shape=invalid`, `lacking=unsupported`,
  `speed=unsupported`) print.
- **The declared rows: hit.** A handler's bare arm names discriminate the rows' tags.
- **The corpus file: hit.** `exit(0)`, `phase_reached` `run`, the declared stdout, through
  `conform-run` and `lupin run`.
- **Red first: hit.** At `8351db6` (trunk's code) all 15 rows failed, each at "does not resolve"
  (`linux-red-8351db6.log`); at `d6d5886` all 15 pass at the first take.
- **argv_is84's row: hit on linux.** At `8351db6` the new row failed: 7 of 200 scripts refused with
  `ETXTBSY`; under the stress loop 40 of 40 runs, 202 of 8,000 scripts, and the original shebang
  test failed in 1 of the 40 runs with the issue's "Text file busy". At `d6d5886`: 200 of 200
  runs green, four at once (`linux-stress-*.log`). **macOS: not measured.** No macOS build ran
  for this change (the evidence index says why); the prediction that macOS never refuses that
  exec stands untested here, and CI's macOS runner runs the row.
- **What must not move:** the vendored corpus names none of the five; the gauntlet's corpus walk
  and `run_corpus` answer that (the records are in the pull request).

## 4. Decisions

Each was decided before the code, from the options below; what is implemented is what was
recommended.

- **The target the host rows follow.** (A) the observation's target
  (`attrs::build_target()`), as every `[os.term]` call and every pure host refusal here does
  (`os_mirror_target_is83`): the windows rows and each target's lacking ops are held in process on
  every host, and a bundle observed as linux answers as linux on any exporter. (B) the host this
  machine runs on, which `os_name()` reads: the corpus file would then agree with itself under any
  observation, but the windows rows could be held only on a windows host. **Recommend A;
  implemented.** The cost, stated: the corpus file chooses its lacking ops by `os_name()`, so read
  in process as another target than its host it prints its mismatch lines. A pin that carries
  the file into the vendored corpus meets that in the bundle (observed as linux on a macOS or
  windows exporter); that pin's lane decides whether the bundle keeps the file or the file's host
  reading follows the target.
- **The name on an open handle that is not a terminal.** (A) refused by name, as the
  section says ("a closed or forged handle `io`; any other refused by name"). (B) `io`, as the
  clause answers for a file, decided by std's `IsTerminal`. **Recommend A; implemented**: the
  section is the specification of this mirror, and B would decide "not a master" by a different
  question than the host's `ptsname_r`.
- **Serving the calls.** (A) refuse every call that reaches a terminal by name, as the
  section says. (B) serve them through the `nix` crate's safe pty and termios calls, which would
  need two new crate features. **Recommend A; implemented**: the section is the mirror's
  specification, and a new feature is its own decision.
- **argv_is84's fix.** (A) the test process never holds a write descriptor on a file it
  runs: `cp` writes the script under a name removed first, from a side file the test wrote, and is
  reaped before the run. (B) retry the run on `ETXTBSY` a bounded number of times. (C) serialize
  every spawn in the binary against every script write with a lock. **Recommend A; implemented**:
  it removes the cause; B hides it, and C must reach every spawn in the binary, present and
  future.
- **No pin advance.** The vendored pin is behind wolf-lang's trunk, and moving it is a
  whole-corpus change of its own. The corpus file rides as a fixture, byte for byte, as the
  mirrors before this one carried theirs. When wolf-lang#1091 merges, the fixture is checked
  against the merged commit's blob.

## 5. Evidence index

`notes/is87-evidence/index.txt` lists every file: the red and green takes, the stress loops and
the rigs, all on Linux. The Linux gauntlet record at the final head and the plant record on it
are in the pull request (a record cannot predate the head it names).

## 6. Done-when

- [x] the prediction committed before the first change (`75ed627`)
- [x] the pty rows red at trunk's code, by name: 0 passed, 15 failed (`8351db6`)
- [x] argv_is84's row red on linux at the old writer (`8351db6`), and the stress loop's red
- [x] the rows green at the change, with fmt, clippy, the library tests and the neighbouring rows
      (`d6d5886`, Linux)
- [x] argv_is84 green over 200 runs, four at once (`d6d5886`, Linux)
- [ ] the Linux gauntlet at the final head, every `END … exit=0`
- [ ] the plant, red by name at its rows, its controls green
- [ ] the audit, once
