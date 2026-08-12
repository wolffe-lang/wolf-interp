# 1 — Running programs

## The front door

`lupin FILE.lu` runs the file. The program's stdout passes through live,
and the process exit code reports the program itself:

```console
$ lupin examples/squares.lu
sum of squares: 30
```

| exit | meaning |
|---|---|
| `N` | the program ran and exited with its own `exit(N)` |
| `2` | a static-phase rejection — the diagnostic prints to stderr (tool errors — missing file, bad flags — also exit `2`) |
| `3` | the program ran and trapped, or the UB oracle reported a finding |
| `4` | outside this implementation's scope: `unsupported`, with the reason on stderr |

`lupin -` reads a program from stdin with the same semantics. `run` is the
explicit subcommand spelling, and carries the scheduler controls
(`--seed=N`, `--schedule=SEED|ev:…` — chapter 3's determinism section) plus
`--json`, which emits the spec/06 observation record instead of passing
output through (exactly what `conform-run --json` does). One collision
rule: a subcommand name wins over a file of the same name, so a file
literally named `repl` runs as `lupin run repl`.

`lupin check FILE...` is the frontend-only fast path — lex, parse, resolve,
diagnostics to stderr, exit `0` or `2`, nothing executed:

```console
$ lupin check examples/squares.lu
examples/squares.lu: ok
```

## conform-run

`conform-run` is the protocol door: it observes one `.lu` file and reports
what happened. Human mode prints a one-line observation followed by the
program's output:

```console
$ lupin conform-run examples/squares.lu
examples/squares.lu: verdict=exit(0) phase_reached=run seeded=false
sum of squares: 30
```

The name is deliberate: the tool implements `[proto.invoke]` from
`spec/06-differential-protocol.md`, and the observation — not the program's
exit code — is its product. `--json` emits the full observation record;
chapter 3 reads one field by field.

`conform-run` exits `0` whenever it produced a well-formed observation —
including for a program that was rejected or trapped, because the *record*
carries the program's outcome. It exits `2` when the tool itself could not
run (missing file, bad flag), and `1` when the work ran and failed its own
check (a red corpus walk, a rejected record, an unstable exploration).

The `lex` and `parse` doors are ordinary frontends by contrast: they exit
`65` on a rejected program and print the diagnostic to stderr.

## The std root

`use std.X[.Y]` resolves against a **std root** when one is configured:
`--std-root DIR` (on `run`, `check` and `conform-run`) reads the module
directory `<DIR>/X[/Y]/`, and the `LUPIN_STD` environment variable is the
flagless spelling every door honours — the interpreter half of the
compiler's `--std-root`/`WOLF_STD` mechanism (s26), so the same source
text resolves under both implementations. Nested paths work
(`use std.x.deque_int` reads `<DIR>/x/deque_int/`), the path's last
segment is the bound name, and without a root the loader falls back to
the flat `<package root>/<last segment>` directory — sibling modules and
flat mirrors keep working unchanged. The flag wins over the environment
variable when both are given.

## Traps

A trap is a fault of a defined execution: the program was legal, ran, and
hit a rule the language enforces at runtime. Every trap names its kind, a
message, the clause it enforces, and the span. At the front door the
diagnostic prints to stderr and the process exits `3`:

```console
$ lupin examples/overflow.lu
examples/overflow.lu: trap(overflow): `+` produced 2147483648, outside `i32` — checked arithmetic traps in every profile (X3); spell intended overflow `wrapping[i32]` [arith.checked] at 107..113
```

Ownership faults carry both spans — the use and the move it conflicts with:

```console
$ lupin conform-run examples/moved.lu
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
| `overflow` | checked arithmetic left the type's range — numeric `as` casts included (a narrowing or float→int cast that does not fit traps here) |
| `div-zero` | division or remainder by zero |
| `bounds` | index outside a sequence |
| `use-after-move` | read of a moved-out binding |
| `exclusivity` | overlapping `mut` access to the same place |
| `region-fault` | a region operation its state forbids |
| `stale-handle` | a generational pool handle outlived its slot |
| `alloc-contract` | an allocation contract violated |
| `assert` | a failed assertion; `assert(cond, msg)` prints `msg` to stdout first, and only evaluates it on the failing path (`[conf.trap.assert]`) |
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
file. `lupin corpus` walks the pinned corpus and checks every header
against what this implementation actually observes.

## Determinism and schedules

A concurrent program runs under a strict-FIFO scheduler by default.
`--seed=N` requests the deterministic schedule of spec/03 §5: one seed
selects the whole decision stream, the record declares `seeded: true`, and
the same seed replays byte-identically. `--explore=N` runs every
inequivalent schedule instead of one and reports whether the program's
outcome is schedule-dependent; a dependent program is a finding and exits
`1`. Exploration admits exactly what `run` admits: a program the static
ladder rejects (an E11xx capture-law finding, say) refuses to explore
with the same diagnostic and exit `2` — a schedule space only exists for
an admitted program. `conform-run --help` lists the exploration budgets.
