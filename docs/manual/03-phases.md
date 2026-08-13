# 3 — Phases & records

## The ladder

The canonical phase ladder is the compiler's pipeline: `none`, `lex`,
`parse`, `resolve`, `typecheck`, `mem`, `wir`, `run`. This implementation
completes four of those rungs and enforces the rest dynamically:

| rung | lupin |
|---|---|
| `lex`, `parse` | the frontend, written from `spec/01` alone |
| `resolve` | sema-lite: the module graph, visibility, and `let` immutability (E0410) |
| `typecheck`, `mem`, `wir` | not performed (the compiler's half) |
| `run` | the tree-walk evaluator, the region store, the provenance oracle |

The design decision behind the gap: the interpreter implements full
*dynamic* semantics and only the static analysis it needs to run programs.
Every property the type checker, borrow checker and region checker prove
statically is enforced as a runtime check here instead. The obligations
this split places on both sides are written down in
[../approximation-contract.md](../approximation-contract.md).

## What `unsupported` means

Asking for a phase this implementation does not perform yields the deepest
rung that *completed*, verdict `unsupported`, and the reason:

```console
$ lupin conform-run examples/squares.lu --phase=typecheck
examples/squares.lu: verdict=unsupported phase_reached=resolve seeded=false
  unsupported: `--phase=typecheck` asks this implementation to stop after a phase it does not perform: the type checker, the borrow/region checkers and the IR are the compiler's half of the split. Every property they prove statically is enforced dynamically at `run` instead
note: --phase=typecheck requested; this implementation completes `resolve` and does not perform the static phases beyond it — they are enforced dynamically at `run` instead (see the ladder mapping in `frontend`)
```

`--phase=run` can still report `run`, because the run itself completed. The
skipped static rungs are exactly the properties enforced dynamically.
`unsupported` is also the verdict for anything outside scope: a std name
with no pinned semantics, a construct the compiler owns. It is never a
crash and never a trap. The trap vocabulary is reserved for faults of
defined executions.

## Reading a record

`--json` emits the observation record that
`spec/06-differential-protocol.md` defines. It is the unit of comparison
between implementations:

```console
$ lupin conform-run examples/squares.lu --json
{"protocol":1,"impl":"lupin","impl_version":"0.1.12","commit":"…","file":"examples/squares.lu","phase_reached":"run","seeded":false,"diagnostics":[],"warnings":[],"verdict":"exit(0)","stdout_sha256":"42857004c6eb56e7ff16c5e877d9f83f2f8a280e2ae98ae6e13c20174c303ddb","stdout_inline":"sum of squares: 30\n"}
```

Field by field:

- `protocol`: the protocol version, `1`.
- `impl`, `impl_version`, `commit`: who produced the record. `commit`
  varies per build, so the `…` above is the manual's placeholder.
- `file`: the program, always with `/` separators, on every platform.
- `phase_reached`: the deepest rung that completed, never more.
- `seeded`: `true` exactly when a deterministic schedule was requested.
- `diagnostics`: on a rejection, the failing entry first (code, span,
  severity). There is no error recovery, so there is never a second error.
  Warning observations follow it with `"severity": "warning"`.
- `warnings`: the warning observations as `{code, span}` after `#[allow]`
  suppression (`[proto.record.warn]`, since 0.1.6: the s68 shared-analysis
  subset). Present once the program loads. When the analyses never ran the
  field is absent, because an empty list would claim there were no
  warnings.
- `verdict`: `pass`, `fail(CODE)`, `exit(N)`, `trap(kind)`, `ub(anchor)`,
  or `unsupported`. A verdict never carries a payload beyond its
  constructor; reasons ride `x-` extension keys.
- `stdout_sha256`, `stdout_inline`: present when the program wrote output.
  The digest covers all of it, the text goes up to 4096 bytes.
- `x-…`: extension keys. `x-trap-clause` and `x-trap-span` on a trap,
  `x-unsupported` with the reason, the `x-ub-*` family on an oracle
  finding. They participate in comparison only when both records carry
  them.

`lupin protocol validate <record.json>…` checks any record, from this
implementation or another, against the same schema.
