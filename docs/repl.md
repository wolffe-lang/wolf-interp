# The lupin REPL: session semantics and the transcript format

`lupin repl` (or bare `lupin`) is a line REPL over the reference interpreter.
It is the wolf-book's teaching vehicle and the first interactive wolf that
exists; the compiler has no REPL and is not getting one. `:mem` and `:trace`
are the memory-model introspection, and they turn D10's tiers into something a
learner can poke. The second deliverable, equally binding, is the **transcript
format** below: every REPL session printed in the book is CI-replayed against
this binary, and drift fails.

## 1. The loop

- Line-oriented. The prompt is `wolf> `; a line that cannot terminate yet
  continues under `....> `. Continuation is the *lexer's own*
  `[gram.lex.newline]` machinery, byte-exact (`lex::repl_input_complete`):
  an input continues while the mode/delimiter stack is open (an unclosed
  `(`/`[`/`{`/string/interpolation) or while the final token cannot end a
  statement (a trailing binary operator). `;` is the single-line-block
  separator exactly as spec/01 has it (E0002 owns the empty statement).
- Expressions evaluate and print `value : type`. `let`/`var`/`fn`/type
  declarations persist. `print` output appears in chronological order,
  before the value line of the expression that printed it.
- Every diagnostic renders with its **code and clause anchor**
  (`error[E0201]: … [gram.expr.primary] at 11..12`); every trap renders
  with its **kind, message and anchor**, ownership faults with both spans.
  Spans index the input line as typed. The Elm-grade catalog stays the
  compiler's (D22); the clause id is what the book cites.

## 2. Incremental definitions: the `[repl.*]` notes

The compiler's module rules (directory = module, no cycles, interface
files) do not apply at a prompt. The REPL is **one implicit module growing
over time**. These rules are REPL-spec notes with their own tag namespace,
deliberately outside spec/01..05: the REPL extends the spec, it does not
fork it. is09 exports them with the rest of the doc surface.

- **`[repl.def.shadow]`.** Redefinition is shadowing, not mutation. A new
  `fn f` binds fresh; closures and values that captured the old `f` keep
  it. There is no live-patching of existing values, which is I14's
  compiled-world story. (Mechanism: every prompt definition gets a
  generational internal name `f#N`; the surface name is a session binding
  to the current generation, and closures capture by value per
  `[gram.expr.closure]`.)
- **`[repl.type.gen]`.** Types are generational. Redefining `Point` mints
  a new nominal type; existing values keep their old identity and print
  with a stale-generation marker (`Point#1`). Values of the *current*
  generation print bare.
- **`[repl.type.mix]`.** Mixing generations is a type error with a hint:
  comparing a `Point#1` against the current `Point` reports that
  redefinition minted a new nominal type and suggests rebuilding the older
  value.
- **`[repl.let.rebind]`.** `let` rebinding drops the old binding. Owned
  resources (regions, `shared` counts) drop per the normal death rules,
  and `:mem` shows that immediately, which is itself a teaching moment.
- **`[repl.module]`.** D31/D32 do not apply at the prompt. `use` is
  refused with this note. `:load file.lu` is textual inclusion into the
  implicit module, nothing more. (`import c "…"` is the C membrane, D17,
  and not a module rule. It works at the prompt, so the is04 provenance
  machine is reachable from a `:trace` session.)
- **`[repl.trap.alive]`.** A trap, UB finding, or diagnostic prints and
  the session survives. The world is whatever the fault left behind, with
  no rollback, and `:mem` shows exactly that state. That is a teaching
  surface, not a bug. `:reset` is the fresh start. The survival holds on
  every fault; the reminder line (`the session survives …`) prints once per
  session — on the first fault, trap or UB — so a transcript with a run of
  faults does not repeat it.

## 3. The directive surface (v1: additions take corpus-directive review)

```
:type e            evaluate e, report its type
:mem               the memory model, live: regions, loans, shared, pools, tasks
:regions           the region tree alone
:trace on|off      record rule firings (clause-cited) into the ring buffer
:trace show [n]    show the last n recorded events (default 20)
:trace clear       empty the ring buffer
:rules [prefix]    the rule registry, optionally filtered by anchor prefix
:schedule seed     re-seed the scheduler's decision stream from here on
:load file.lu      textual inclusion into the implicit module ([repl.module])
:keys              the line editor's bindings ([repl.edit.*]; TTY sessions)
:reset             fresh world, empty module
:quit              leave
```

`:mem` output is deterministic (stable ids, sorted iteration), so
transcripts snapshot. Regions print as `#id `name` strategy state=… objects=…
[parent=…] [owner=…]`. `shared` cells print strong/weak counts. Pool slots
print generation and life stage, so a stale handle is *visible* before it
faults. `:trace` events carry source spans and clause ids; with the is04
machine engaged (`import c` + raw pointers) the Tree-Borrows
retag/read/write/invalidate events are on it.

## 4. The transcript format: the book contract

A transcript is exactly what a **piped** session prints: prompts, echoed
inputs, and output lines, LF-terminated (the repo forces `eol=lf`).

```
wolf> let x = 2
wolf> x + 3
5 : i32
wolf> :quit
```

- A line starting `wolf> ` is an input at the top level; `....> ` is a
  continuation line of the same input. Everything else is expected output.
- **Producing** a transcript: `lupin repl < inputs.txt > t.transcript`
  (pipe mode echoes inputs after the prompt, so captured stdout *is* the
  transcript). Interactive mode renders identically, the terminal itself
  showing the typing.
- **Replaying**: `lupin repl --script t.transcript` feeds the input
  lines to a fresh session, re-renders the whole transcript, and
  byte-compares. Any drift prints the first differing line and exits 1;
  byte-identity prints `replay: OK (N input(s), byte-identical)` and exits
  0. `--seed N` selects the schedule seed (default: unseeded = strict FIFO,
  `[conc.det.seed]`), so concurrent sessions replay deterministically.
- The pinned suite lives in `tests/repl/*.transcript`, replayed by
  `tests/repl_session.rs` on the full 3-OS CI matrix. bs00's runner
  consumes the same files unchanged. Format changes after bc01 starts
  require a book-side migration in the same change.
- Corpus-derived text: `:load` never echoes the loaded file's source, so
  transcripts do not embed corpus bytes and cannot churn on
  STYLE_VERSION bumps.

## 5. Exemplar walkthrough (the bs01-shaped dry run)

`tests/repl/region_lifecycle.transcript` is the committed exemplar: the
region lifecycle exactly as a book chapter would stage it.

1. `struct Config { limit: int }` defines a type for the session
   (`defined type `Config``).
2. `let r = region(rc)` then `:regions`. The region exists (`rc`,
   `state=suspended`, 0 objects): creating a region does not open it.
3. `let cfg = in r { Config { limit: 42 } }` then `:mem`. The open window
   allocated into it (`objects=1`), and it is suspended again after the
   block: `[mem.region.open.1]` live.
4. `let frozen = freeze r` then `:mem`, and `state=frozen`. The affine
   region value was *consumed* (`[mem.region.freeze.1]`). Reading
   `cfg.limit` still answers 42, because frozen data is readable forever
   (`[mem.region.edge.imm]`), including after `frozen` is rebound away.
5. A second region `q` is built and then dropped by rebinding
   (`[repl.let.rebind]`), and `:regions` shows it gone. A region dies as a
   unit, wholesale (`[mem.region.intra.2]`).

Every step is one input line plus one small `:mem` block: an author can
lift the transcript into prose by narrating each pair, and CI replays the
session against the binary so the chapter cannot rot.

## 6. The line editor (is25): the `[repl.edit.*]` notes

The prompt line-edits. These notes are the editing contract, written down
rather than left emergent (is08's discipline, applied to the reader). The
editor is a *reader*: it hands complete inputs to `Session::feed_line` and
owns no meaning — every semantic in sections 1–5 is untouched by it.

- **`[repl.edit.tty]`.** The editor engages only when stdin is a terminal.
  A piped session keeps the exact dumb reader, prompts (`wolf> `/`....> `),
  and echo — its captured stdout IS the transcript of section 4, and the
  three byte-compare gates hold on it. `--no-edit` and `TERM=dumb` also
  select the dumb reader; if raw mode cannot be entered, the session
  degrades to the dumb reader with a one-line stderr note — a prompt always
  opens. Under the editor, a continuation is one editable buffer rendered
  without the `....> ` marker; `....> ` remains transcript syntax, printed
  by the piped path exactly as before.
- **`[repl.edit.dep]`.** The editor is `rustyline` 18 (MIT; MIT composes
  with this repository's GPL-3.0-or-later — the combined work remains
  GPL-3.0-or-later). A dependency is *required*, not convenient:
  `unsafe_code = "forbid"` stands, so raw-mode termios cannot be hand-rolled
  here. Rejected alternative: `reedline` (more modern, nushell-shaped
  keybinding vocabulary, weaker GNU-readline correspondence — the ask was
  readline fidelity, and rustyline's `Validator` hook maps exactly onto
  `lex::repl_input_complete`). rustyline pulls no `wolf_*` crate and no
  wolf-lang git dependency (the Cargo.lock gate holds). Not everything came
  free: rustyline has no yank-last-arg, so `Alt-.`/`Alt-_` (with cycling)
  is this repo's own `ConditionalEventHandler` (`edit::YankLastArg`).
- **`[repl.edit.keys]`.** The GNU-readline vocabulary: motion
  (`Ctrl-A`/`Ctrl-E`, `Ctrl-B`/`Ctrl-F`, `Alt-b`/`Alt-f`,
  `Ctrl-Left`/`Ctrl-Right`, Home/End); kill+yank ring (`Ctrl-W`,
  `Alt-Backspace`, `Alt-d`, `Ctrl-K`, `Ctrl-U`, `Ctrl-Y`, `Alt-y`);
  `Alt-.`/`Alt-_` yank-last-arg cycling to older entries on repeat; case
  ops (`Alt-u`/`Alt-l`/`Alt-c`); `Ctrl-T`/`Alt-t` transpose, `Ctrl-L`
  clear-screen, `Ctrl-_` undo; `Ctrl-R` reverse search; TAB completion.
  `Ctrl-D` on a non-empty line is delete-char; on an empty line it exits
  (its is08 meaning, kept). `Ctrl-W`/`Ctrl-U`/backspace — the kernel
  freebies raw mode removes — are rebound, not lost. **No binding is
  claimed from crate documentation**: every one is verified by keystroke
  against the built binary (`tools/replkeys_probe.py`, unix PTY; the
  asked→bound→verified table is the sprint's evidence). `:keys` lists the
  bindings from inside a session.
- **`[repl.edit.history]`.** Up/Down recall in-session and across
  sessions. The file lives in the platform's state location —
  `$XDG_STATE_HOME/lupin/history` (default `~/.local/state/lupin/history`)
  on unix-likes, `%APPDATA%\lupin\history` on Windows; `LUPIN_HISTORY`
  overrides the path (empty value disables persistence). Capped at 1000
  entries, oldest evicted; empty inputs and consecutive duplicates are not
  recorded. A multi-line input is ONE history item — recall brings back the
  whole `fn`, not its last line — and the file format is one JSON string
  per line so embedded newlines survive the round trip. A corrupt line is
  skipped, never fatal. `Ctrl-R` searches the same list.
- **`[repl.edit.cancel]`.** `Ctrl-C` at the prompt abandons the current
  input — single-line or mid-continuation — and returns to a fresh
  `wolf> ` with the session's world intact: `[repl.trap.alive]` applied to
  editing, and the fix for wolf-interp#46 (the inescapable continuation).
  `Ctrl-C` never exits; `Ctrl-D` on an empty line does. The contract holds
  at the prompt: interrupting a long-*running* evaluation is not in scope
  (named residue).
- **`[repl.edit.complete]`.** TAB completes from three sources the process
  already holds: the `:` directives and their subcommands (`:trace
  on|off|show|clear`; `:rules` prefixes from the rule registry); the
  session's own bound names — surface names only, never the generational
  internals (`f#2`, `Point#1`) of `[repl.def.shadow]`/`[repl.type.gen]`;
  and filesystem paths after `:load`. Ambiguity lists the candidates
  rather than guessing.

## 7. What the REPL is not

Not colorized: no syntax highlighting and no highlighted prompt (named
residue — rustyline's `Highlighter` hook is cheap to add once wanted, and
`edit::EditHelper` already stubs it). No vi mode (rustyline ships one
behind its config; deliberately not exposed — residue, one config line if
asked for). No `~/.inputrc` reading (rustyline does not parse it; residue).
Not a debugger: `:trace` is a log, not a control surface. No
compiler-parity modules at the prompt. Session *history* persists across
restarts (`[repl.edit.history]`); session *state* does not.
