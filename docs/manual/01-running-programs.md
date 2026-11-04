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
| `2` | a static-phase rejection; the diagnostic prints to stderr. Tool errors (missing file, bad flags) also exit `2` |
| `3` | the program ran and trapped, or the UB oracle reported a finding |
| `4` | outside this implementation's scope: `unsupported`, with the reason on stderr |

`lupin -` reads a program from stdin with the same semantics. `run` is the
explicit subcommand spelling. It carries the scheduler controls `--seed=N`
and `--schedule=SEED|ev:…` (the determinism section below), plus `--json`,
which emits the spec/06 observation record instead of passing output
through. That is what `conform-run --json` does, at the front door. One
collision rule: a subcommand name wins over a file of the same name, so a
file literally named `repl` runs as `lupin run repl`.

`lupin check FILE...` is the frontend-only fast path: lex, parse, resolve,
diagnostics to stderr, exit `0` or `2`. Nothing runs.

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

The tool implements `[proto.invoke]` from
`spec/06-differential-protocol.md`, and its product is the observation. The
program's exit code rides inside the record. `--json` emits the full
record; chapter 3 reads one field by field.

`conform-run` exits `0` whenever it produced a well-formed observation,
including for a program that was rejected or trapped, because the record
carries the program's outcome. It exits `2` when the tool itself could not
run (missing file, bad flag). It exits `1` when the work ran and failed its
own check: a red corpus walk, a rejected record, an unstable exploration.

The `lex` and `parse` doors are ordinary frontends. They exit `2` on a
rejected program, as `run`, `check` and `eval` do, and print the diagnostic
to stderr. `[conf.exit.static]` names one number for a rejection at every
front door; through 0.1.38 these two exited `65`.

## The std root

`use std.X[.Y]` resolves against a std root when one is configured.
`--std-root DIR` (on `run`, `check` and `conform-run`) reads the module
directory `<DIR>/X[/Y]/`, and the `LUPIN_STD` environment variable is the
flagless spelling every door honours. This is the interpreter half of the
compiler's `--std-root`/`WOLF_STD` mechanism (s26), so the same source text
resolves under both implementations. Nested paths work
(`use std.x.deque_int` reads `<DIR>/x/deque_int/`), and the path's last
segment is the bound name. Without a root the loader falls back to the flat
`<package root>/<last segment>` directory, so sibling modules and flat
mirrors keep working unchanged. The flag wins over the environment variable
when both are given.

## Program arguments

`lupin FILE.lu ARGS…` and `lupin run [OPTIONS] FILE.lu ARGS…` run a
program with ordinary shell arguments. Every word after FILE belongs to
the program, unparsed, including `--help`, `--`, and an empty argument.
Put lupin's options **before** FILE. `--` ends lupin's options, so
`lupin -- -x.lu a` (or `lupin run -- -x.lu a`) runs a file named `-x.lu`.
`lupin - a b` reads source from stdin with the same argument convention.

argv[0] is FILE **as given**, not a canonical path or lupin's executable:
`lupin ./tool.lu a b` supplies `["./tool.lu", "a", "b"]`.
`env_args()` reads argv[1..] as text, `env_arg0()` reads argv[0] alone,
and `env_args_bytes()` reads every argument as bytes, argv[0] first.
Unix OS bytes pass through verbatim; Windows uses WTF-8 (`[os.env.argv]`).
No lossy decoding occurs. A text reader meeting non-UTF-8 traps
`argument-utf8`, naming the OS argv index; `env_args()` never decodes
argv[0], and `env_arg0()` never decodes the trailing arguments.

For exact conformance inputs, repeated `--argv-hex=HEX` remains available
on `run` and `conform-run` (`[exec.checked.argv]`). It replaces the whole
argv, **including argv[0]**. HEX is an even number of hexadecimal digits;
empty HEX is an empty argument; NUL is refused. On `run`, put this option
before FILE. Combining explicit hex input with trailing words is refused
by name rather than guessing which input wins. `conform-run` keeps its
protocol options (including options after FILE), accepts no trailing
program words, and without hex input still has absent argv, never lupin's
own arguments. For example:

```text
$ lupin conform-run --json name.lu --argv-hex=ff --argv-hex=6f6b
… "verdict":"trap(argument-utf8)" … "trap_message":"argument 0 is not UTF-8; read it with env_args_bytes()" …
```

## Executable scripts

On Linux, start the file with `#!/usr/bin/env lupin`, make it executable
(`chmod +x tool.lu`), and run `./tool.lu a b`. With lupin on PATH the
program sees `["./tool.lu", "a", "b"]`. On PAX use `#!/bin/lupin` or
`#!/bin/env lupin`, with the corresponding binaries installed. The
interpreter line is trivia only at byte zero (`[gram.lex.shebang]`);
`#![` remains an inner attribute, not a comment. Host execution and
interpreter lookup are the host's responsibility; lupin does not emulate
a shell or split a shebang's option string.

### Why this convention

The comparison uses published documentation only. A script name and the
interpreter executable's argv[0] are different things in several tools;
Node does **not** preserve the script spelling as Python does.

| tool | script name / program arguments | option boundary and shebang | non-UTF-8 arguments |
|---|---|---|---|
| [Python CLI](https://docs.python.org/3/using/cmdline.html), [sys.argv](https://docs.python.org/3/library/sys.html#sys.argv) | `sys.argv[0]` is the script name as given; following entries are script args | script ends interpreter options; Unix `#!` scripts are supported | Unix filesystem decoding with `surrogateescape`; `os.fsencode` recovers bytes |
| [Node CLI](https://nodejs.org/api/cli.html), [process.argv](https://nodejs.org/api/process.html#processargv) | executable first, **absolute script path second**, then args; `process.argv0` is original executable argv[0] | CLI synopsis puts options before entry; `--` ends Node options; host runs executable scripts | linked API promises strings, not raw-byte round trips |
| [Ruby options](https://docs.ruby-lang.org/en/3.4/ruby/options_md.html), [globals](https://docs.ruby-lang.org/en/3.4/globals_rdoc.html) | `$0` is script name, `ARGV` holds given arguments separately | interpreter options precede program; `-s` explicitly parses script switches; `-x` finds an embedded `#!…ruby` | linked pages do not specify raw-byte fidelity |
| [Perl run](https://perldoc.perl.org/perlrun), [variables](https://perldoc.perl.org/perlvar) | `$0` is program name; `@ARGV` excludes it | `--` ends options; optional `-s` consumes script switches; Perl also examines switches on `#!` | `-CA` requests UTF-8 argv; no general raw-byte contract in these pages |
| [Deno CLI](https://docs.deno.com/runtime/getting_started/command_line_interface/), [Deno.args](https://docs.deno.com/api/deno/~/Deno.args) | `Deno.args` contains script arguments, not script name | runtime flags before filename; later words are script args | linked pages do not specify raw-byte fidelity or a shebang contract |
| [go run](https://pkg.go.dev/cmd/go#hdr-Compile_and_run_Go_program), [os.Args](https://pkg.go.dev/os#Args) | `package [arguments…]`; `os.Args` starts with program name, not a documented source spelling | build flags before package; no `--` requirement or shebang contract stated here | linked command page gives no byte policy |
| [cargo run](https://doc.rust-lang.org/cargo/commands/cargo-run.html), [args_os](https://doc.rust-lang.org/std/env/fn.args_os.html) | runs a binary, not a source filename; first argument traditionally executable path | `--` separates Cargo's flags from all binary args; no script/shebang contract | `args_os` accepts non-Unicode; text `args` can panic |
| [POSIX sh -c](https://pubs.opengroup.org/onlinepubs/9799919799/utilities/sh.html) | `command_name` becomes `$0`; remaining operands become `$1…` | `--` ends shell options; command operands are not shell flags; `#!` command-file results are unspecified by POSIX | arguments are strings of bytes; locale affects character interpretation |

The compiler's `wolf run --help` likewise says words after the entry file
are the program's argv. Its generated executable's invocation name need
not be the source filename: lupin deliberately chooses the source spelling
for scripts. This is a command convention, not a change to `[os.env.argv]`.

**Migration:** `lupin run FILE --json`, `FILE --seed=N`, and
`FILE --trace-places` now give those words to the program. Use
`lupin run --json FILE`, `lupin run --seed=N FILE`, or
`lupin run --trace-places FILE`. Lupin does not steal a program's flags to
issue a warning; `run --help` states the boundary.

## Traps

A trap is a fault of a defined execution: the program was legal, ran, and
hit a rule the language enforces at runtime. Every trap names its kind, a
message, the clause it enforces, and where — `line:col`, 1-based, columns
counted in characters (`[conf.trap.render]`; the raw byte span stays on
`--json`'s `x-trap-span`). At the front door the diagnostic prints to
stderr and the process exits `3`. That status is per-machine by
`[conf.trap.exit]` (D60): the native tier exits 134 for the same fault, so
conforming tools compare the *kind* and leave the status number alone:

```console
$ lupin examples/overflow.lu
examples/overflow.lu: trap(overflow): `+` produced 2147483648, outside `i32` — checked arithmetic traps in every profile (X3); spell intended overflow `wrapping[i32]` [arith.checked] at 6:5
```

Ownership faults carry both spans: the use, and the move it conflicts with.

```console
$ lupin conform-run examples/moved.lu
examples/moved.lu: verdict=trap(use-after-move) phase_reached=run seeded=false
  trap(use-after-move): `s` was moved out and is uninitialized here [mem.tier0.move.2] at 7:13; `s` moved here at 6:13
```

The compiler rejects that program statically (E1001). This implementation
performs no borrow checking and enforces the same property at runtime
instead. Chapter 3 explains the split.

The trap vocabulary is closed at fourteen kinds (`[conf.trap.set]`). Adding
one requires revising the spec:

| kind | fault |
|---|---|
| `overflow` | checked arithmetic left the type's range, numeric `as` casts included (a narrowing or float→int cast that does not fit traps here) |
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
| `stack-overflow` | a native stack's guard page was hit; lupin never raises it, and reads it in another machine's records |
| `argument-utf8` | `env_args()` or `env_arg0()` read an argument that is not UTF-8; the message names the OS argv index (`[os.env.argv]`) |

## The directive header

Corpus files open with a `//!` block that states the file's own expected
outcome, in the grammar `spec/05` §2a pins (`[conf.directive.*]`). The
shipped `examples/` carry one too:

```text
check:    pass | fail(CODE) | run( exit=N | exit=trap | exit=trap(kind) [, stdout="…"] )
phase:    none | lex | parse | resolve | typecheck | mem | wir | run
conforms: anchor, anchor, …
member:   true | false
```

The header matters even outside the corpus, because of the module rule:
directory = module, so sibling `.lu` files are one module unless a file
opts OUT as a standalone entry (`[conf.directive.standalone]`, D59).
The standalone set has four spellings: a `//! member: false` line
(the ordinary opt-out — several programs sharing one scratch directory is
one header line per program), the `check:` + `phase:` entry pair, a
script announcement (a `#!` first line or `pkg { … }` frontmatter), or a
`_test.lu` file name. An explicit `member:` key always decides, the named
entry always compiles, and a standalone mark never shrinks anyone else's
build — plain siblings are shared members of every build, so two programs
sharing a directory each mark themselves. `member: true` marks a file
that belongs to a multi-file module case and is only exercised through
its directory's entry file. `lupin corpus` walks the pinned corpus and
checks every header against what this implementation observes.

## Determinism and schedules

A concurrent program runs under a strict-FIFO scheduler by default.
`--seed=N` requests the deterministic schedule of spec/03 §5: one seed
selects the whole decision stream, the record declares `seeded: true`, and
the same seed replays byte-identically. `conform-run --explore=N` explores
up to N inequivalent schedules instead of observing one run, and reports
whether the program's outcome is schedule-dependent; a dependent program is
a finding and exits `1`. Exploration admits what `run` admits, since a
schedule space exists only for an admitted program: a program the static
ladder rejects (an E11xx capture-law finding, say) refuses to explore, with
the same diagnostic and exit `2`. `conform-run --help` lists the
exploration budgets.

Everything a replay artifact needs is in the machine-readable forms, so a
rig can build one without scraping the human report (wolf-interp#53). Each
outcome of `--explore --json` carries three replay members: `replay`, the
copy-pasteable flag; `schedule`, the decision stream as `ev:c0,c1,…` (the
spelling `--schedule=` takes, and the only one that survives a stream too
deep for a packed seed); and `seed`, that packed 62-bit value or `null`. A record produced under `--seed=N` or `--schedule=ev:…` echoes the
request back as the extension key `x-seed` or `x-schedule`, so
`conform-run --json --seed=S > artifact.json` is self-contained: the file
names the schedule it replays. Both keys are `x-` extensions
(`[proto.record.ext]`), since the protocol's own field set is wolf-lang's to
grow, and an absent extension key is never a divergence
(`[proto.cmp.defined-divergence]`), so a seeded record still compares
clean against a counterparty that emits neither.
