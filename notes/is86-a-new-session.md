# is86 — a new session, mirrored (wolf-lang s290)

wolf-lang#1089 (branch `s290`, head `cc0274c2`, open at this note) adds three calls:
`os_spawn_session(exe, args, map, defaults) -> int ! {denied, invalid, io, not_found,
unsupported}`, a spawn whose child leads a new session; `os_setsid() -> () ! {denied, io,
unsupported}`, this process into a session of its own; and `os_sid() -> int ! {unsupported}`, its
session id (`[os.proc.session]`, `[os.term.session]`, and the design note
`[os.proc.session.note]`). lupin 0.1.60 resolved none of the three. Both clauses say what lupin's
mirror answers: the shape rows, then windows, and a refusal by name for every other call, since
Rust's standard library has no stable new-session spawn (`process_setsid` is unstable), no
`setsid` on the calling process and no `getsid`, and this crate admits no `unsafe`. That is what
this change does.

## 1. Forbidden

- No `unsafe`, no new dependency and no new crate feature; no host call made for any of the three.
- No pin, spec, vendored, CI or toolchain file changes: `vendor/upstream/` stays at `5f1f1c94`.
- No answer of another call moves: `os_spawn_job`'s rows and refusals are as they were.
- The interpreter shares no implementation code with the compiler: the clauses, the corpus file
  and the compiler's published behaviour are the inputs, never its source.
- No label, merge, tag, release or issue closed by this branch; no attribution trailer.

## 2. Inputs, verified

| input | re-derived |
|---|---|
| trunk | `789ae74`, lupin 0.1.60, `vendor/upstream/PIN` `5f1f1c94` |
| the compiler's change | wolf-lang#1089, head `cc0274c2` (31 commits on its trunk), mergeable, open |
| the clauses | `spec/11-os.md` at `cc0274c2` (blob `e99ca56b`): `[os.proc.session]`, `[os.term.session]`, `[os.proc.session.note]`, and the three lines of `[os.host.sigs]` |
| the corpus file | `corpus/os/session_rows.lu` at `cc0274c2`: blob `f669cf4c`, sha256 `b950c1ec5eeb1cc8bb2549029af8def81fbddae7e77952c185c42ebb4c6d91c6`; `tests/fixtures/session_s290/session_rows.lu` is the same blob |
| the compiler's lupin leg | `crates/wolf_driver/tests/session_lanes.rs` at `cc0274c2`: a 0.1.60 sibling must say "does not resolve"; any other must answer `unsupported` with a reason containing `os_spawn_session` |
| lupin 0.1.60 | the `aarch64-apple-darwin` release archive, sha256 `f3cae16fdb88a94702c0fd816d5eb90cdc8e8636c028dcf40f03868c5dc36bb8`: each of the three "does not resolve", the corpus file prints nothing |
| the machine | `AMBIENT_NAMES`, the dispatcher, the wasm decline list and `declared_row` name none of the three; `fd_map_of` and `SIG_LISTENABLE` are `os_spawn_job`'s shape readers |
| README | read, current: it states nothing these calls change |

The PR's account of the mirror and the clauses agree on every row. The clauses decide one point the
account leaves implicit: `[os.term.session]`'s order of refusal puts windows' `unsupported` before
the host's answer for both `os_setsid` and `os_sid`, so a windows target answers that row here
too, as it does for `os_fork`.

## 3. Prediction, scored

Committed before the first change (`8d0e736`, `notes/is86-prediction.md`).

- **The names resolve: hit.**
- **The spawn's shape on every host: hit.** Ten shapes (an odd map, a target of 256 and of -1, a
  target twice, a source of -2, 65 pairs; `defaults` -1, 1048576, 1048577 and -1048576) answer
  `invalid` on this host and as the windows, linux and macOS targets.
- **Windows after the shape: hit.** Seven well-formed calls answer the `unsupported` row as the
  windows target.
- **Refused by name elsewhere: hit.** The same seven calls on this host, as linux and macOS and
  as the bundle's target: verdict `unsupported`, the reason naming the call, never "does not
  resolve".
- **The declared rows: hit.** A handler's bare arm names discriminate the row's tags.
- **The corpus file: hit, but for one word.** The four `invalid` lines, then the refusal at the
  forged-source spawn naming `os_spawn_session`. The record's `phase_reached` is `resolve`, not
  `run`: every refusal record of this machine reads `resolve` and names the run in
  `x-unsupported-at`, 0.1.60's "does not resolve" included. The row holds `x-unsupported-at` and
  the four lines, which 0.1.60 never prints.
- **wasm: hit.** The wasm library's clippy is clean with the three names in its decline list.
- **Red first: hit.** At the rows' commit (`dc2101c`, trunk's code) all eight rows failed, each
  at "does not resolve" (`mac-red-dc2101c.log`); at the change (`4e0e7a1`) all eight pass.
- **What must not move:** the vendored corpus names none of the three; the gauntlet's corpus walk
  and `run_corpus` answer that (the records are in the pull request).

## 4. Decisions

- **No pin advance.** The vendored pin is 517 commits behind wolf-lang's trunk; moving it is a
  whole-corpus change of its own. The corpus file rides as a fixture, byte for byte, as the
  mirrors before this one carried theirs. When wolf-lang#1089 merges, the fixture is checked
  against the merged commit's blob; the next pin advance carries the file into the vendored
  corpus, and `W0304` for a declaration that shadows one of the three names comes with that
  pin's `prelude.json`, which this machine reads.
- **The windows rows follow the observation's target** (`attrs::build_target()`), as `os_pgid`'s
  and `os_set_pgid`'s do, not the build's `cfg!(windows)` as `os_spawn_job`'s do: every answer
  here is decided without a host call, so a bundle observed as linux answers as linux on any
  exporter, and the windows rows are held in process on every host.
- **A forged source and an empty program are refused by name**, not answered `io` and
  `not_found`: the clauses put the sources and the program after the host, and the mirror they
  describe answers only the shape rows before refusing.
- **The reasons cite the clauses**, not the compiler's code: `[os.proc.session]` and
  `[os.proc.session.note]` for the spawn, `[os.term.session]` for the other two.
- **wolf-lang's lupin leg** asserts a reason containing `os_spawn_session` from any lupin but
  0.1.60. 0.1.60's own reason, "`os_spawn_session` does not resolve", contains it too, so that
  leg tells the mirror from the base by the version alone; the rows here hold the four lines and
  a reason that never says "does not resolve". This build still calls itself 0.1.60 until the
  release that carries it.

## 5. Evidence index

`notes/is86-evidence/index.txt` lists every file. The Linux gauntlet record at the final head and
the plant record on it are in the pull request (a record cannot predate the head it names).

## 6. Done-when

- [x] the prediction committed before the first change (`8d0e736`)
- [x] the rows red at trunk's code, by name: 0 passed, 8 failed (`dc2101c`)
- [x] the rows green at the change, with fmt, clippy, the library tests and the neighbouring OS
      rows (`4e0e7a1`, macOS); the wasm library's clippy and a windows-target clippy of every
      target clean
- [x] the cheap gates and the rows on Linux, in a light slot (`linux-cheap-a90507c.log`)
- [ ] the Linux gauntlet at the final head, every `END … exit=0`
- [ ] the plant, red by name at its rows, its controls green
- [ ] the audit, once
