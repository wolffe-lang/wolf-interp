# 3 — Phases & records

## The ladder

The canonical phase ladder is the compiler's pipeline: `none`, `lex`,
`parse`, `resolve`, `typecheck`, `mem`, `wir`, `run`. This implementation
completes four of those rungs and enforces the rest dynamically:

| rung | wolf-interp |
|---|---|
| `lex`, `parse` | the frontend, written from `spec/01` alone |
| `resolve` | sema-lite: the module graph and visibility, nothing more |
| `typecheck`, `mem`, `wir` | not performed — the compiler's half |
| `run` | the tree-walk evaluator, the region store, the provenance oracle |

The design decision behind the gap: the interpreter implements full
*dynamic* semantics but only the static analysis needed to run programs.
Every property the type checker, borrow checker and region checker prove
statically is enforced as a runtime check here instead. The obligations
this split places on both sides are written down in
[../approximation-contract.md](../approximation-contract.md).

## What `unsupported` means

Asking for a phase this implementation does not perform yields the deepest
rung that *completed*, verdict `unsupported`, and the reason:

```console
$ wolf-interp conform-run examples/squares.lu --phase=typecheck
examples/squares.lu: verdict=unsupported phase_reached=resolve seeded=false
  unsupported: `--phase=typecheck` asks this implementation to stop after a phase it does not perform: the type checker, the borrow/region checkers and the IR are the compiler's half of the split. Every property they prove statically is enforced dynamically at `run` instead
note: --phase=typecheck requested; this implementation completes `resolve` and does not perform the static phases beyond it — they are enforced dynamically at `run` instead (see the ladder mapping in `frontend`)
```

`--phase=run` can still report `run`, because the run itself completed —
the skipped static rungs are exactly the properties enforced dynamically.
`unsupported` is also the verdict for anything outside scope (a std name
with no pinned semantics, a construct the compiler owns), never a crash and
never a trap: the trap vocabulary is reserved for faults of defined
executions.

## Reading a record

`--json` emits the observation record `spec/06-differential-protocol.md`
defines — the unit of comparison between implementations:

```console
$ wolf-interp conform-run examples/squares.lu --json
{"protocol":1,"impl":"wolf-interp","impl_version":"0.0.1","commit":"…","file":"examples/squares.lu","phase_reached":"run","seeded":false,"diagnostics":[],"verdict":"exit(0)","stdout_sha256":"42857004c6eb56e7ff16c5e877d9f83f2f8a280e2ae98ae6e13c20174c303ddb","stdout_inline":"sum of squares: 30\n"}
```

Field by field:

- `protocol` — the protocol version, `1`.
- `impl`, `impl_version`, `commit` — who produced the record. (`commit`
  varies per build; the `…` above is the manual's placeholder.)
- `file` — the program, always with `/` separators, on every platform.
- `phase_reached` — the deepest rung that completed, never more.
- `seeded` — `true` exactly when a deterministic schedule was requested.
- `diagnostics` — on a rejection, exactly one entry (code, span, severity):
  there is no error recovery, so there is never a second.
- `verdict` — `pass`, `fail(CODE)`, `exit(N)`, `trap(kind)`, `ub(anchor)`,
  or `unsupported`. A verdict never carries a payload beyond its
  constructor; reasons ride `x-` extension keys.
- `stdout_sha256`, `stdout_inline` — present when the program wrote output:
  the digest of all of it, the text up to 4096 bytes.
- `x-…` — extension keys: `x-trap-clause` and `x-trap-span` on a trap,
  `x-unsupported` with the reason, the `x-ub-*` family on an oracle
  finding. They participate in comparison only when both records carry
  them.

`wolf-interp protocol validate <record.json>…` checks any record — from
this implementation or another — against the same schema.
