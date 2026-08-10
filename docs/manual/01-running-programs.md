# 1 — Running programs

## conform-run

`conform-run` is the door for running a program: it observes one `.lu` file
and reports what happened. Human mode prints a one-line observation followed
by the program's output:

```console
$ wolf-interp conform-run examples/squares.lu
examples/squares.lu: verdict=exit(0) phase_reached=run seeded=false
sum of squares: 30
```

The name is deliberate: the tool implements `[proto.invoke]` from
`spec/06-differential-protocol.md`, and the observation — not the program's
exit code — is its product. `--json` emits the full observation record;
chapter 3 reads one field by field.

## Exit codes

`conform-run` exits `0` whenever it produced a well-formed observation —
including for a program that was rejected or trapped, because the *record*
carries the program's outcome. It exits `2` when the tool itself could not
run (missing file, bad flag), and `1` when the work ran and failed its own
check (a red corpus walk, a rejected record, an unstable exploration).

The `lex` and `parse` doors are ordinary frontends by contrast: they exit
`65` on a rejected program and print the diagnostic to stderr.

## Traps

A trap is a fault of a defined execution: the program was legal, ran, and
hit a rule the language enforces at runtime. Every trap names its kind, a
message, the clause it enforces, and the span:

```console
$ wolf-interp conform-run examples/overflow.lu
examples/overflow.lu: verdict=trap(overflow) phase_reached=run seeded=false
  trap(overflow): `+` produced 2147483648, outside `i32` — checked arithmetic traps in every profile (X3); spell intended overflow `wrapping[i32]` [arith.checked] at 107..113
```

Ownership faults carry both spans — the use and the move it conflicts with:

```console
$ wolf-interp conform-run examples/moved.lu
examples/moved.lu: verdict=trap(use-after-move) phase_reached=run seeded=false
  trap(use-after-move): `s` was moved out and is uninitialized here [mem.tier0.move.2] at 128..129; `s` moved here at 109..115
```

The compiler rejects that program statically (E1001); this implementation
performs no borrow checking and enforces the same property at runtime
instead — chapter 3 explains the split.

The trap vocabulary is closed at twelve kinds (`[conf.trap.set]`); adding
one requires revising the spec:

| kind | fault |
|---|---|
| `overflow` | checked arithmetic left the type's range |
| `div-zero` | division or remainder by zero |
| `bounds` | index outside a sequence |
| `use-after-move` | read of a moved-out binding |
| `exclusivity` | overlapping `mut` access to the same place |
| `region-fault` | a region operation its state forbids |
| `stale-handle` | a generational pool handle outlived its slot |
| `alloc-contract` | an allocation contract violated |
| `assert` | a failed assertion |
| `race` | a data race the dynamic machine observed |
| `ub` | the UB oracle's finding, when pinned as a trap expectation |
| `deadlock` | every task blocked with no runnable successor |

## The directive header

Corpus files — and the shipped `examples/` — open with a `//!` block that
states the file's own expected outcome, in the grammar `spec/05` §2a pins
(`[conf.directive.*]`):

```text
check:    pass | fail(CODE) | run( exit=N | exit=trap | exit=trap(kind) [, stdout="…"] )
phase:    none | lex | parse | resolve | typecheck | mem | wir | run
conforms: anchor, anchor, …
member:   true | false
```

The header matters even outside the corpus, because of the module rule:
directory = module, so sibling `.lu` files are one module unless each
carries its own entry header. `member: true` marks a file that belongs to a
multi-file module case and is only exercised through its directory's entry
file. `wolf-interp corpus` walks the pinned corpus and checks every header
against what this implementation actually observes.

## Determinism and schedules

A concurrent program runs under a strict-FIFO scheduler by default.
`--seed=N` requests the deterministic schedule of spec/03 §5: one seed
selects the whole decision stream, the record declares `seeded: true`, and
the same seed replays byte-identically. `--explore=N` runs every
inequivalent schedule instead of one and reports whether the program's
outcome is schedule-dependent; a dependent program is a finding and exits
`1`. `conform-run --help` lists the exploration budgets.
