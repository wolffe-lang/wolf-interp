# wolf-interp

<img src="assets/wolf-logo.svg" alt="the wolf mark" width="120" align="right"/>

The reference interpreter for the wolf language. It is an independent,
executable reading of the specification, and the oracle the compiler
([wolf-lang](https://github.com/wolffe-lang/wolf-lang)) is differentially
tested against. The two implementations share no code. What they share is the
pinned spec, the pinned corpus, and the observation protocol they are
compared through. Wolf source files use the `.lu` extension. This repo builds
a binary named `lupin`.

Licensed under [GPL-3.0-or-later](LICENSE).

## Building

```sh
git clone https://github.com/wolffe-lang/wolf-interp
cd wolf-interp
cargo build --release
```

The binary lands at `target/release/lupin`, which is how the transcripts
below spell it. The toolchain is pinned by `rust-toolchain.toml`. The spec
and corpus come from a pinned wolf-lang checkout: the `upstream/` submodule
when it is initialized, otherwise the tracked snapshot under
`vendor/upstream/`. A bare clone works without touching submodules
([manual](docs/manual/00-building.md)). `--version` names the pairing, which
is the binary, the package, and the posture, at the stated upstream pin:

```console
$ lupin --version
lupin 0.1.18… (wolf-interp, reference interpreter at pin …)
```

A build made exactly at its release tag prints the bare version; any other
build — this one included, unless you checked out the tag — carries a
`+dev.<commit>` suffix, so an off-tag build never claims to be the release
(D57).

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

Running the file takes no subcommand. The program's output passes through,
and its `exit(N)` becomes the process exit code:

```console
$ lupin examples/squares.lu
sum of squares: 30
```

Honest failure output is part of the product. `examples/overflow.lu`
overflows an `i32`. Arithmetic is checked in every build profile, so the
program traps. The diagnostic goes to stderr and cites the spec clause it
enforces. The process exits `3`:

```console
$ lupin examples/overflow.lu
examples/overflow.lu: trap(overflow): `+` produced 2147483648, outside `i32` — checked arithmetic traps in every profile (X3); spell intended overflow `wrapping[i32]` [arith.checked] at 6:5
```

The exit codes are documented in the
[manual](docs/manual/01-running-programs.md): the program's own `exit(N)`,
`2` on a static-phase rejection, `3` on a trap, `4` on `unsupported`.
`lupin -` reads a program from stdin the same way.

The `//!` header is a conformance directive. The file states its own
expected outcome, in the grammar the corpus uses. `conform-run` is the
protocol surface. It runs the program and reports what it observed:

```console
$ lupin conform-run examples/squares.lu
examples/squares.lu: verdict=exit(0) phase_reached=run seeded=false
sum of squares: 30
```

The first line is the observation: the verdict, and the deepest pipeline
phase that completed. The rest is the program's output.

## The REPL

Bare `lupin` starts an interactive session, and `lupin repl` is the explicit
spelling. Declarations persist, and a trap does not end the session. `:mem`,
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

The interpreter implements the dynamic semantics in full, and only the
static analysis needed to run programs. The type checker, borrow checker and
region checker are the compiler's half. Every property they prove statically
is enforced dynamically here, so an ownership violation surfaces as a runtime
trap where the compiler would refuse the program outright
([manual](docs/manual/03-phases.md)). Of the 241 entry files in the pinned
corpus, 171 reach the `run` rung. The corpus walk (`lupin corpus`) prints the
exact ledger.

## Documentation

`docs/README.md` is the index. User-facing material lives in
[docs/manual/](docs/manual/README.md). The engineering documents (the
approximation contract, the divergence log, the bundle format) live beside it
in `docs/`. Every command/output pair in this README and in the manual is
real output from the pinned build, enforced by a test. The spec is normative.
This implementation is one reading of it.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the independence doctrine, the
gates, and the commit conventions.
