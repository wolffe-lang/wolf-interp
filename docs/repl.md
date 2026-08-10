# The wolf-interp REPL — session semantics and the transcript format

`wolf-interp repl` is a line REPL over the reference interpreter: the
wolf-book's teaching vehicle and the first interactive wolf that exists (the
compiler has no REPL and never will). Its differentiating feature is
memory-model introspection — `:mem` and `:trace` turn D10's tiers from prose
into something a learner can poke — and its second deliverable, equally
binding, is the **transcript format** below: every REPL session printed in
the book is CI-replayed against this binary, and drift fails.

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

## 2. Incremental definitions — the `[repl.*]` notes

The compiler's module rules (directory = module, no cycles, interface
files) do not apply at a prompt; the REPL is **one implicit module growing
over time**. These rules are REPL-spec notes with their own tag namespace —
deliberately *not* spec/01..05 clauses: the REPL extends the spec, it does
not fork it. is09 exports them with the rest of the doc surface.

- **`[repl.def.shadow]`** — Redefinition is shadowing, not mutation. A new
  `fn f` binds fresh; closures and values that captured the old `f` keep
  it. No live-patching of existing values — that is I14's compiled-world
  story. (Mechanism: every prompt definition gets a generational internal
  name `f#N`; the surface name is a session binding to the current
  generation, and closures capture by value per `[gram.expr.closure]`.)
- **`[repl.type.gen]`** — Types are generational. Redefining `Point` mints
  a new nominal type; existing values keep their old identity and print
  with a stale-generation marker (`Point#1`). Values of the *current*
  generation print bare.
- **`[repl.type.mix]`** — Mixing generations is a type error with a hint:
  comparing a `Point#1` against the current `Point` reports that
  redefinition minted a new nominal type and suggests rebuilding the older
  value.
- **`[repl.let.rebind]`** — `let` rebinding drops the old binding; owned
  resources (regions, `shared` counts) drop per the normal death rules —
  visible in `:mem` immediately, which is itself a teaching moment.
- **`[repl.module]`** — D31/D32 do not apply at the prompt. `use` is
  refused with this note; `:load file.lu` is textual inclusion into the
  implicit module, nothing more. (`import c "…"` is the C membrane, D17,
  not a module rule — it works at the prompt so the is04 provenance
  machine is reachable from a `:trace` session.)
- **`[repl.trap.alive]`** — A trap, UB finding, or diagnostic prints and
  the session survives. The world is whatever the fault left behind — no
  rollback — and `:mem` shows exactly that state, which is a teaching
  surface, not a bug. `:reset` is the fresh start.

## 3. The directive surface (v1 — additions take corpus-directive review)

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
:reset             fresh world, empty module
:quit              leave
```

`:mem` output is deterministic — stable ids, sorted iteration — so
transcripts snapshot. Regions print as `#id `name` strategy state=… objects=…
[parent=…] [owner=…]`; `shared` cells print strong/weak counts; pool slots
print generation and life stage, so a stale handle is *visible* before it
faults. `:trace` events carry source spans and clause ids; with the is04
machine engaged (`import c` + raw pointers) the Tree-Borrows
retag/read/write/invalidate events are on it.

## 4. The transcript format — the book contract

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
- **Producing** a transcript: `wolf-interp repl < inputs.txt > t.transcript`
  (pipe mode echoes inputs after the prompt, so captured stdout *is* the
  transcript). Interactive mode renders identically, the terminal itself
  showing the typing.
- **Replaying**: `wolf-interp repl --script t.transcript` feeds the input
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

1. `struct Config { limit: int }` — a type for the session
   (`defined type `Config``).
2. `let r = region(rc)` then `:regions` — the region exists (`rc`,
   `state=suspended`, 0 objects): creating a region does not open it.
3. `let cfg = in r { Config { limit: 42 } }` then `:mem` — the open window
   allocated into it (`objects=1`), and it is suspended again after the
   block: `[mem.region.open.1]` live.
4. `let frozen = freeze r` then `:mem` — `state=frozen`. The affine region
   value was *consumed* (`[mem.region.freeze.1]`); reading `cfg.limit`
   still answers 42 — frozen data is readable forever
   (`[mem.region.edge.imm]`), including after `frozen` is rebound away.
5. A second region `q` is built and then dropped by rebinding
   (`[repl.let.rebind]`): `:regions` shows it gone — a region dies as a
   unit, wholesale (`[mem.region.intra.2]`).

Every step is one input line plus one small `:mem` block: an author can
lift the transcript into prose by narrating each pair, and CI replays the
session against the binary so the chapter cannot rot.

## 6. What the REPL is not

No completion or syntax highlighting beyond stock line editing (delta from
the contract's "rustyline-class editing": no external line-editing crate is
vendored — the dependency policy outweighs polish until book feedback asks
for it; recorded for the ic04 closeout). Not a debugger: `:trace` is a log,
not a control surface. No compiler-parity modules at the prompt. No session
persistence across restarts.
