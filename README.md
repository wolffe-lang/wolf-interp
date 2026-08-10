# wolf-interp

The reference interpreter for the wolf language: an independent, executable
reading of the specification, and the oracle the compiler
([wolf-lang](https://github.com/tenseleyFlow/wolf-lang)) is differentially
tested against. The two implementations share no code — only the pinned spec
and corpus, and the observation protocol they are compared through. Wolf
source files use the `.lu` extension.

Dual-licensed MIT or Apache-2.0.

## Building

```sh
git clone https://github.com/tenseleyFlow/wolf-interp
cd wolf-interp
cargo build --release
```

The binary lands at `target/release/wolf-interp`; the transcripts below spell
it `wolf-interp`. The toolchain is pinned by `rust-toolchain.toml`. The spec
and corpus come from a pinned wolf-lang checkout — the `upstream/` submodule
when initialized, otherwise the tracked snapshot under `vendor/upstream/` — so
a bare clone works without touching submodules
([manual](docs/manual/00-building.md)).

## Running a program

`examples/squares.lu`, in full:

```wolf
//! check: run(exit=0, stdout="sum of squares: 30")
//! phase: run

struct Point { x: int, y: int }

fn main() -> !int {
    var sum = 0
    region frame {
        var pts = List[Point]()
        for i in 0..5 { pts.push(Point { x: i, y: i * i }) }
        for p in pts { sum += p.y }
    }
    print("sum of squares: {sum}")
    0
}
```

The `//!` header is a conformance directive: the file states its own expected
outcome, in the grammar the corpus uses
([manual](docs/manual/01-running-programs.md)). `conform-run` runs the
program and reports what it observed:

```console
$ wolf-interp conform-run examples/squares.lu
examples/squares.lu: verdict=exit(0) phase_reached=run seeded=false
sum of squares: 30
```

The first line is the observation — the verdict and the deepest pipeline
phase that completed. The rest is the program's output.

Honest failure output is part of the product. `examples/overflow.lu`
overflows an `i32`; arithmetic is checked in every build profile, so the
program traps, and the trap cites the spec clause it enforces:

```console
$ wolf-interp conform-run examples/overflow.lu
examples/overflow.lu: verdict=trap(overflow) phase_reached=run seeded=false
  trap(overflow): `+` produced 2147483648, outside `i32` — checked arithmetic traps in every profile (X3); spell intended overflow `wrapping[i32]` [arith.checked] at 107..113
```

## The REPL

`wolf-interp repl` starts an interactive session; declarations persist, traps
do not end the session, and `:mem`, `:regions` and `:trace` show the memory
model live. A walkthrough is in the [manual](docs/manual/02-repl.md).

```console
$ wolf-interp repl
wolf> let s = "wolf"
wolf> let t = move s
wolf> s
trap(use-after-move): `s` was moved out and is uninitialized here [mem.tier0.move.2] at 0..1
  `s` moved here at 8..14
the session survives the trap; the world is as the fault left it [repl.trap.alive]
wolf> t
wolf : str
wolf> :quit
```

## Commands

| command | what it does |
|---|---|
| `conform-run` | Observe one program and emit a spec/06 observation record |
| `repl` | Interactive session; `--script` replays a recorded transcript |
| `corpus` | Walk the pinned corpus and check every directive against this implementation |
| `lex`, `parse` | Run one frontend phase and dump its evidence |
| `diff-run` | Compare this implementation against the pinned compiler, corpus-wide |
| `conformance` | Export the versioned conformance bundle, or check an implementation against one |
| `fuzz` | Differential testing over generated programs, with reduction of anything divergent |
| `protocol` | Validate observation records against the spec/06 schema |

`wolf-interp <command> --help` lists the flags; the
[manual](docs/manual/README.md) covers each command with worked transcripts.

## Scope

The interpreter implements the dynamic semantics in full and only the static
analysis needed to run programs. The type checker, borrow checker and region
checker are the compiler's half; every property they prove statically is
enforced dynamically here, so an ownership violation is a runtime trap rather
than a compile error ([manual](docs/manual/03-phases.md)). Of the 148 entry
files in the pinned corpus, 96 currently reach the `run` phase; the corpus
walk (`wolf-interp corpus`) prints the exact ledger.

## Documentation

`docs/README.md` is the index. User-facing material lives in
[docs/manual/](docs/manual/README.md); the engineering documents — the
approximation contract, the divergence log, the bundle format — live beside
it in `docs/`. Every command/output pair in the README and the manual is real
output from the pinned build, enforced by a test. The spec is normative; this
implementation is one reading of it.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the independence doctrine, the
gates, and the commit conventions.
