# is86 prediction (lupin's mirror): a new session, wolf-lang s290

Committed before the first change. Base: trunk `789ae74` (lupin 0.1.60, the upstream pin
`5f1f1c94`). The compiler's change is wolf-lang#1089 (branch `s290`, head `cc0274c2`, open):
`os_spawn_session(exe, args, map, defaults) -> int ! {denied, invalid, io, not_found,
unsupported}`, `os_setsid() -> () ! {denied, io, unsupported}` and `os_sid() -> int !
{unsupported}`, in `[os.proc.session]`, `[os.term.session]` and the design note
`[os.proc.session.note]` of its `spec/11-os.md`.

## What this machine does at the base

Measured with the published lupin 0.1.60 (the `aarch64-apple-darwin` archive, sha256
`f3cae16fdb88a94702c0fd816d5eb90cdc8e8636c028dcf40f03868c5dc36bb8`): every program naming one of
the three stops at the resolve rung, verdict `unsupported`, with "`os_spawn_session` does not
resolve" (and the same for `os_setsid` and `os_sid`). wolf-lang's `corpus/os/session_rows.lu`
(blob `f669cf4c`, sha256 `b950c1ec5eeb1cc8bb2549029af8def81fbddae7e77952c185c42ebb4c6d91c6`)
prints nothing: `phase_reached` is `resolve`. None of the three names is in `AMBIENT_NAMES`,
the dispatcher has no arm for them, and `declared_row` gives them no row.

## The rule here (from the clauses, not from the compiler's code)

This machine reaches no host call any of the three needs: Rust's standard library has no stable
new-session spawn (`process_setsid` is unstable), no `setsid` on the calling process and no
`getsid`, and this crate forbids `unsafe`. So, as `[os.proc.session]` and `[os.term.session]`
say lupin's mirror does:

1. **The names resolve.** A program naming any of the three passes the resolve rung and runs.
2. **`os_spawn_session`'s SHAPE is answered on every host**, before anything else: `invalid` for
   every map `[os.proc.fds]` calls invalid (an odd length, a target outside 0..255, a target
   named twice, a source below -1, more than 64 pairs) and for `defaults` below 0 or with a bit
   outside the twenty listenable meanings (`[os.signal.set]`, bits 0..19). The map reader and
   the listenable mask are `os_spawn_job`'s own (`fd_map_of`, `SIG_LISTENABLE`).
3. **Then the HOST:** for a windows observation target, the `unsupported` row, after the shape,
   for all three calls (`os_setsid` and `os_sid` have no shape). The target is the observation's
   (`attrs::build_target()`), as for `os_pgid` and `os_set_pgid`: every answer here is decided
   without a host call, so a bundle observed as linux on a windows exporter answers as linux, and
   the windows rows can be held on every host.
4. **Every other call is refused by name:** verdict `unsupported`, the reason naming the call
   (`os_spawn_session`, `os_setsid`, `os_sid`) and the clause, and never "does not resolve". For
   the spawn that includes a forged source, an empty program and a well-formed map: the sources
   (`io`) and the program (`not_found`) come after the host in the clause's order, and this
   machine can make no child to reach them.
5. **The rows:** `declared_row` gives `os_spawn_session` the five tags, `os_setsid` `{denied, io,
   unsupported}`, `os_sid` `{unsupported}`, so a handler matching the row is exhaustive.
6. **The corpus file**, on linux and macOS: the four `invalid` lines (`map_odd`,
   `map_target_twice`, `defaults_outside`, `defaults_negative`), then the refusal at the
   forged-source spawn, naming `os_spawn_session`; `phase_reached` `run`. On a windows host
   (CI's leg; the file reads its host from `os_name`): `exit(0)` and the file's declared stdout
   (each windows row is `unsupported`, and `os_sid`'s `unsupported` reads as -1).
7. **wasm** declines the three by name with the rest of the hosted process tier.

## Rows, red first

`tests/session_s290.rs`, with the corpus file byte for byte as
`tests/fixtures/session_s290/session_rows.lu`. At the rows' commit (trunk's code) every row that
names a call fails by name, at "does not resolve" where a row, a refusal by name or the corpus
file's lines are due. At the change: every row passes. The wasm arm has no host row (its words
are `hosted_tier_decline`'s, already held by is76's test); the wasm library's clippy leg, run
here before the push, compiles it.

## What must not move

The vendored corpus (pin `5f1f1c94`) names none of the three, so the corpus ledger, the
divergence and conformance gates, the bundle and every other test are unchanged. No pin, CI,
spec or vendored file changes. The plant drops the `defaults` shape check: red by name at the
`defaults` shape rows, with the map rows and the refusals green.
