# wolf-interp

<img src="assets/wolf-logo.svg" alt="the wolf mark" width="120" align="right"/>

The reference interpreter for the wolf language: an independent, executable
reading of the specification, and the oracle the compiler
([wolf-lang](https://github.com/wolffe-lang/wolf-lang)) is differentially
tested against. The two implementations share no code — only the pinned spec
and corpus, and the observation protocol they are compared through. Wolf
source files use the `.lu` extension. wolf-interp builds a binary named
`lupin`.

Dual-licensed MIT or Apache-2.0.

## Building

```sh
git clone https://github.com/wolffe-lang/wolf-interp
cd wolf-interp
cargo build --release
```

The binary lands at `target/release/lupin`; the transcripts below spell it
`lupin`. The toolchain is pinned by `rust-toolchain.toml`. The spec and
corpus come from a pinned wolf-lang checkout — the `upstream/` submodule
when initialized, otherwise the tracked snapshot under `vendor/upstream/` — so
a bare clone works without touching submodules
([manual](docs/manual/00-building.md)). `--version` names the pairing:
the binary, the package, and the posture — this is the wolf **reference
interpreter** at the stated upstream pin:

```console
$ lupin --version
lupin 0.1.8 (wolf-interp, reference interpreter at pin …)
```

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

Running the file is one word (`lupin FILE.lu` needs no subcommand); the
program's output passes through and its `exit(N)` is the process exit code:

```console
$ lupin examples/squares.lu
sum of squares: 30
```

Honest failure output is part of the product. `examples/overflow.lu`
overflows an `i32`; arithmetic is checked in every build profile, so the
program traps — the diagnostic cites the spec clause it enforces, prints to
stderr, and the process exits `3`:

```console
$ lupin examples/overflow.lu
examples/overflow.lu: trap(overflow): `+` produced 2147483648, outside `i32` — checked arithmetic traps in every profile (X3); spell intended overflow `wrapping[i32]` [arith.checked] at 107..113
```

The exit codes are documented in the
[manual](docs/manual/01-running-programs.md): the program's own `exit(N)`,
`2` on a static-phase rejection, `3` on a trap, `4` on `unsupported`.
`lupin -` reads a program from stdin the same way.

The `//!` header is a conformance directive: the file states its own
expected outcome, in the grammar the corpus uses. `conform-run` is the
protocol surface — it runs the program and reports what it observed:

```console
$ lupin conform-run examples/squares.lu
examples/squares.lu: verdict=exit(0) phase_reached=run seeded=false
sum of squares: 30
```

The first line is the observation — the verdict and the deepest pipeline
phase that completed. The rest is the program's output.

## The REPL

Bare `lupin` starts an interactive session (`lupin repl` is the explicit
spelling); declarations persist, traps do not end the session, and `:mem`,
`:regions` and `:trace` show the memory model live. A walkthrough is in the
[manual](docs/manual/02-repl.md). `lupin eval 'CODE'` (or `-e`) evaluates
one snippet the same way and exits.

```console
$ lupin
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
| `run` | Run one program; output passes through live and the exit code is the program's (also `lupin FILE.lu`, no subcommand) |
| `eval` | Evaluate a snippet in a fresh session and print its value as the REPL would |
| `check` | Check files through the frontend only (lex, parse, resolve) and report diagnostics |
| `repl` | Interactive session; `--script` replays a recorded transcript |
| `conform-run` | Observe one program and emit a spec/06 observation record |
| `corpus` | Walk the pinned corpus and check every directive against this implementation |
| `lex`, `parse` | Run one frontend phase and dump its evidence |
| `diff-run` | Compare this implementation against the pinned compiler, corpus-wide |
| `conformance` | Export the versioned conformance bundle, or check an implementation against one |
| `fuzz` | Differential testing over generated programs, with reduction of anything divergent |
| `protocol` | Validate observation records against the spec/06 schema |

A subcommand name wins over a file of the same name: a file literally named
`repl` runs as `lupin run repl`. `lupin <command> --help` lists the flags;
the [manual](docs/manual/README.md) covers each command with worked
transcripts.

## Scope

The interpreter implements the dynamic semantics in full and only the static
analysis needed to run programs. The type checker, borrow checker and region
checker are the compiler's half; every property they prove statically is
enforced dynamically here, so an ownership violation is a runtime trap rather
than a compile error ([manual](docs/manual/03-phases.md)). Of the 148 entry
files in the pinned corpus, 96 currently reach the `run` phase; the corpus
walk (`lupin corpus`) prints the exact ledger.

## Documentation

`docs/README.md` is the index. User-facing material lives in
[docs/manual/](docs/manual/README.md); the engineering documents — the
approximation contract, the divergence log, the bundle format — live beside
it in `docs/`. Every command/output pair in the README and the manual is real
output from the pinned build, enforced by a test. The spec is normative; this
implementation is one reading of it.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the independence doctrine, the
gates, and the commit conventions.
