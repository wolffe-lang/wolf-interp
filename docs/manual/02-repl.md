# 2 — The REPL

Bare `lupin` opens a line REPL over the interpreter; `lupin repl` is the
explicit spelling. It is the only interactive wolf there is, since the
compiler has none. Expressions evaluate and print `value : type`.
Declarations persist. `print` output appears in order, before the value
line of the expression that printed it. The memory model is inspectable
from the prompt: `:mem`, `:regions` and `:trace` show the machine's state
live.

## A session

Everything below is one real session, replayed against the binary in CI.

```console
$ lupin
wolf> struct Config { limit: int }
defined type `Config`
wolf> let r = region(rc)
wolf> let cfg = in r { Config { limit: 42 } }
wolf> :regions
regions:
  #0 `program` arena state=open objects=0
  #1 `-` rc state=suspended objects=1
wolf> cfg.limit
42 : i32
wolf> :type cfg
Config
wolf> let s = "wolf"
wolf> let t = move s
wolf> s
trap(use-after-move): `s` was moved out and is uninitialized here [mem.tier0.move.2] at 0..1
  `s` moved here at 8..14
the session survives the trap; the world is as the fault left it [repl.trap.alive]
wolf> t
wolf : str
wolf> 1 / 0
trap(div-zero): division by zero is defined behavior in wolf: it traps [mem.ub.defined] at 0..5
wolf> 2 + 3
5 : i32
wolf> :quit
```

Worth noticing in that transcript:

- Creating a region does not open it. `region(rc)` shows up in `:regions`
  as `suspended`, and the `in r { … }` window allocated one object into it.
- A move is a move. Reading `s` afterwards traps, with both spans: the
  read, and the move it conflicts with. The value lives on in `t`.
- A trap does not end the session (`[repl.trap.alive]`). The world is
  whatever the fault left behind. Nothing rolls back, and that state is
  inspectable, which is the point. The survival guarantee holds on every
  fault; the reminder line is printed once per session (the first fault,
  trap or UB), so the second trap here shows only its trap line.

## Multi-line input

The lexer decides continuation, not a heuristic. An input continues under
the `....>` prompt while a delimiter is open or the last token cannot end a
statement.

```console
$ lupin repl
wolf> fn double(x: int) -> int {
....>     x * 2
....> }
defined fn `double`
wolf> double(21)
42 : i64
wolf> :quit
```

## One-shot evaluation

`lupin eval 'CODE'` (short spelling `-e`) evaluates a snippet in a fresh
session, prints what the REPL would print, and exits. The exit codes are
the front door's (chapter 1): `0` clean, `2` on a rejected snippet, `3` on
a trap or UB finding, `4` on `unsupported`.

```console
$ lupin eval 1+1
2 : i32
```

## Directives

```text
:type e            evaluate e, report its type
:mem               the memory model, live: regions, loans, shared, pools, tasks
:regions           the region tree alone
:trace on|off      record rule firings (clause-cited) into the ring buffer
:trace show [n]    show the last n recorded events (default 20)
:trace clear       empty the ring buffer
:rules [prefix]    the rule registry, optionally filtered by anchor prefix
:schedule seed     re-seed the scheduler's decision stream from here on
:load file.lu      textual inclusion into the implicit module
:reset             fresh world, empty module
:quit              leave
```

## Session rules

The REPL is one implicit module growing over time; the compiler's module
rules do not apply at a prompt. Redefining a function or type shadows the
old one. Closures and values that captured the old definition keep it, and
values of a superseded type print with a generation marker (`Point#1`).
`use` is refused at the prompt. `:load` is textual inclusion, nothing more.
The full semantics, including the `[repl.*]` notes and the transcript
format, are specified in [../repl.md](../repl.md).

## Transcripts

A transcript is exactly what a piped session prints. `lupin repl
< inputs.txt > session.transcript` produces one;
`lupin repl --script session.transcript` replays the inputs against a
fresh session and byte-compares the whole rendering, exiting `1` on any
drift. The pinned suite lives in `tests/repl/*.transcript`, and the
sessions printed in this chapter are checked the same way.
