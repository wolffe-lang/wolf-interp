# wolf-interp

The wolf reference interpreter: an independent implementation of the wolf
language specification, and the compiler's differential-testing oracle.

Independence is the point: this repo shares **no** frontend or semantics
code with the compiler ([wolf-lang](https://github.com/tenseleyFlow/wolf-lang)).
The only shared artifacts are the spec and corpus it pins, and the
differential protocol (spec/06) both implementations speak.

Dual-licensed MIT or Apache-2.0.

---

## Status: is04 — the UB / provenance oracle

wolf programs **run**, and their aliasing is **checked**. `spec/02-memory-model.md`
is implemented as a dynamic machine end to end: §2's values, moves and modes
(is02), §3–§4's regions, pools and `shared` cells (is03), and now §5–§7's unsafe
tier — a Tree-Borrows-shaped provenance machine and the closed UB enumeration,
each row detectable and paired with the optimization it licenses. Every rule is
enforced as a runtime check and every fault cites the clause it enforces.

After this sprint, "zero UB in safe code" and "unsafe is simpler than safe" are
CI assertions rather than prose.

- **lexer + parser** (`src/lex.rs`, `src/parse.rs`) — the whole of
  `spec/01-grammar.md`, written from that document and nothing else, with no
  error recovery: the first error wins, carrying a span and the `[gram.…]`
  clause that failed (is01).
- **sema-lite** (`src/sema.rs`) — the D32 module graph and nothing more:
  directory = module, `use` binds a sibling module of the package root,
  `pub`/`pub(pkg)` gate cross-module access, signatures taken at face value.
- **evaluator** (`src/eval/`) — MVS values with per-slot `Live`/`Moved` state
  (field-granular), `read`/`mut`/`take` parameter modes with **dynamic**
  exclusivity checking over exact paths, `copy`, re-initialization, closures,
  expression-oriented control flow, checked arithmetic, `!T` error unions as
  values with `?`/`else`/`defer`/`errdefer`. No unwinding anywhere.
- **region store** (`src/eval/region.rs`) — `spec/02` §3–§4 as data: the region
  table with its four states, affine region values, ambient allocation, wholesale
  free, the cross-region edge table, deep `freeze`, RC cells and generational
  pools (is03).
- **provenance machine** (`src/eval/prov.rs`) — `spec/02` §6 as a machine:
  per-allocation **tag trees**, the `[mem.prov.state]` transition table per
  location, **protectors** for a call's extent, `assume noalias`, the
  `borrow r from ptr` door, int↔ptr exposure with **angelic** wildcard
  resolution, and `[mem.prov.region]`'s composition with the region store. §7's
  closed enumeration is a checked matrix: every row detected and paired with a
  near-miss twin, or listed as deferred with a reason (is04).
- **rule registry** (`src/eval/rules.rs`) — every dynamic rule with its clause
  anchor and one sentence, 71 of them. `conform-run --trace` logs each rule as it
  fires; `tests/rule_registry.rs` proves 100% anchor coverage against the pinned
  spec and demonstrates that a planted anchorless rule fails the same gate.

### The sema boundary

**The load-bearing design decision of this track.** The interpreter implements
full *dynamic* semantics but only the static analysis needed to run programs.
It does **not** implement the type checker, the borrow checker, or the region
checker; every safety property those prove statically is enforced dynamically
here instead. The dynamic side of `spec/02` §3–§4 — the region table, the
cross-region edge table, `freeze`, `shared` refcounts and generational
`handle`s — is `src/eval/region.rs`, and the obligations it places on the
compiler's static half are written down in
[docs/approximation-contract.md](docs/approximation-contract.md). Two consequences, both codified in the protocol:

- a program the compiler rejects that runs clean here is an **expected verdict
  class** — static conservatism, ledgered rather than counted as agreement;
- a sema-lite failure (unresolvable name, ambiguous dispatch, a type error the
  checker owns) is verdict `unsupported`, with the reason on the
  `x-unsupported` extension key — never a crash, and never a trap, because the
  trap vocabulary is for faults of *defined* executions.

The approximation direction is one-way: **the compiler accepts ⇒ the
interpreter must not fault; never the converse.**

### The phase ladder, mapped

The canonical ladder is the compiler's pipeline. What this implementation
completes (the full table, with rationale, is `src/frontend.rs`'s module doc):

| rung | wolf-interp |
|---|---|
| `lex`, `parse` | the frontend |
| `resolve` | sema-lite |
| `typecheck`, `mem`, `wir` | **not performed** — the compiler's half |
| `run` | the tree-walk evaluator, the region store and the provenance oracle |

`--phase=typecheck` therefore reports `resolve` + `unsupported`: it asks this
implementation to stop after a phase it never performs, and
`[proto.record.phase]` says report the deepest phase that **completed**.
`--phase=run` can report `run` even though the static rungs were skipped,
because the run itself completed — those rungs are exactly the properties
enforced dynamically instead.

### UB verdicts

An oracle finding is **not** a trap. `[proto.record.verdict]` gives it the
verdict `ub(anchor)`, and `[proto.record.ub]` makes it a comparison participant:
one side reporting `ub(…)` where the other runs defined is a
**soundness-candidate** divergence, the highest-severity class there is. The
record carries the §7 row (`x-ub-row`), the clause, the access span, the
tag-creation span, the borrow-tree slice, and — the D2 pairing, executable — the
optimization the row licenses (`x-ub-licenses`), because `[mem.ub.closed]` makes
"zero rows without a named optimization" an invariant.

`[conf.trap.map]` also lists `ub` among the closed eleven trap kinds: that is
the **checked build's** observable, which is why a corpus file can pin
`run(exit=trap(ub))` and this implementation's richer verdict satisfies it. The
argument is in `ledger::ub_is_the_oracle_verdict`.

### Against the pinned corpus

66 of 114 entry files reach `run`. Among them: `hello.lu` → `exit(0)` printing
`hello, wolf`, `overflow.lu` → `trap(overflow)`, `memory/div_zero.lu` →
`trap(div-zero)`, `memory/oob_bounds.lu` → `trap(bounds)`,
`memory/defer_order.lu` → `exit(0)` with `body first second`, `wordcount.lu`
→ `exit(2)`, the three D32 module cases, and — since is03 — the Tier-1/2
litmuses: `memory/region_ambient_ok.lu`, `memory/region_multiopen_ok.lu`,
`memory/region_multiopen_swap.lu`, `memory/region_iso_edge_ok.lu`,
`memory/shared_ok.lu` all `exit(0)`, and `memory/handle_stale.lu` →
`trap(stale-handle)`. Since is04 the unsafe litmuses run too:
`memory/unsafe_noalias.lu` → `exit(0)` (a *true* `assume noalias` is defined
behavior) and `memory/unsafe_ub_uaf.lu` → **`ub(mem.ub)`** with `x-ub-row: P1`,
the oracle's first verdict. `memory/prov_two_phase.lu` and
`memory/prov_holy_grail.lu` are now provenance-*checked* rather than merely
executed — `--trace=prov` shows the Reserved receiver surviving the argument
read, and the Frozen protector holding for the call. Every file whose `check:`
is a run expectation and which this machine evaluates matches it exactly; there
are zero mismatches (`cargo run -- corpus` prints the ledger, and
`tests/run_corpus.rs` enforces it).

Two of those entries are the *dynamic counterpart* of a static code the corpus
pins: `memory/move_use_after.lu` (`fail(E1001)` ⇄ `trap(use-after-move)`) and
`memory/excl_overlap.lu` (`fail(E1002)` ⇄ `trap(exclusivity)`) — the two
mappings `[conf.trap.map]` states. is03 produced the dynamic half of **E1004**
and **E1005** as well and could not classify them; the current pin states both
kinds — the repair this repo's finding asked for — so the ledger classifies four
counterparts now, not two.

`corpus/regions.lu` stays `unsupported`, for **half** the reason it used to.
`build_config()` was declared nowhere in the corpus; the current pin declares
it. What survives is the rest of the same expression: `frozen.get(config)` and
`.is_valid()` appear in no pinned document, `&&` does not short-circuit past
them, and so `run(exit=0)` is still unsatisfiable for any implementation. Filed
as a finding, not worked around — see `docs/approximation-contract.md` §5.5.

`corpus/ffi.lu` now executes its whole `unsafe` block: the C library is modelled
as a small closed set of **host intrinsics** inside the provenance machine
(`malloc`/`calloc`/`free`/`memset`/`memcpy` — never real FFI, contract in
`docs/approximation-contract.md` §8), so allocation, `memset`, the raw store and
the free all run with exposure bookkeeping traced. It stops at the inline `asm`
block, which no pinned document gives a meaning.

### Error codes

`spec/01` §9 reserves E0001–E0008 and `[gram.lex.str]` names E0108; those are
not ours to choose and we emit them where the spec says. Every other code this
implementation emits is **our invention**, listed in `diag::UNPINNED_CODES`
with the clause it serves. Cross-implementation disagreement on unpinned codes
is expected, and it is what drives codes into the spec — see the standing rule
in [CONTRIBUTING.md](CONTRIBUTING.md).

`parse::CHOICES` is the companion list: places where `spec/01` does not
determine the parse, and what this implementation does instead.
`parse::CHOICES_RESOLVED` is its history — the entries a spec amendment has
since closed, kept as a table so a filed gap has a visible fate. Six of is01's
eight choices are in it as of the current pin.

## Getting the pin

`upstream/` is a git submodule pinned to an exact wolf-lang revision. Only
`upstream/spec` and `upstream/corpus` are ever consumed — data, never code.

```sh
git clone https://github.com/tenseleyFlow/wolf-interp
cd wolf-interp
git submodule update --init upstream
```

The submodule carries the whole wolf-lang repository. To keep the compiler's
sources out of your working tree entirely — recommended, and the shape CI
should be read as having — sparse-check it out:

```sh
git -C upstream sparse-checkout init --cone
git -C upstream sparse-checkout set spec corpus
```

### Bumping the pin

A pin bump is a deliberate act, in its own commit, landing CI-green:

```sh
git -C upstream fetch origin trunk
git -C upstream checkout <rev>          # an explicit revision, never a branch
cargo test                              # the corpus-size and anchor tests speak
git add upstream
git commit -m "pin: bump wolf-lang to <rev>"
```

`tests/corpus_harness.rs` asserts the corpus file count and that every
`conforms:` tag in a registered namespace resolves against the pinned
`spec/anchors.json`. Those assertions are the point of the bump commit: if the
upstream corpus grew or a clause anchor moved, the bump is where you find out.

## Commands

```sh
cargo run -- corpus [--root <dir>] [--spec <dir>] [--json]
cargo run -- conform-run <file.lu> [--phase=<p>] [--seed=N] [--json] [--trace[=all|mem|prov]]
cargo run -- lex   <file.lu> [--dump]
cargo run -- parse <file.lu> [--dump]
cargo run -- protocol validate <record.json>...
```

`lex --dump` prints the token stream, `parse --dump` prints a production trace
and `conform-run --trace` prints the evaluation rules as they fire — each line
of all three citing the clause anchor behind it. `--trace` takes a namespace:
`mem` keeps the memory-model rules (every region event), `prov` keeps the
Tier-3 ones (every retag, permission transition, exposure, protector and UB
row). Both filters are derived from the anchor rather than from a hand-kept
list, so neither can drift away from the rule registry. Both formats are
**ours**; nothing in the protocol consumes them, and they are deliberately not
modelled on anything the compiler prints. In human mode a rejected program
exits `65`; under `conform-run` a rejection is a *record* and the tool still
exits `0`.

`conform-run` implements `spec/06-differential-protocol.md` `[proto.invoke]`.
It exits `0` whenever it produced a well-formed observation record — the
*record* carries the program's outcome — and `2` when the tool itself could
not run. `1` means the work ran and failed its check (a red corpus walk, a
rejected record).

Today's records, in full — a run, a rejection, and a scope gap:

```json
{"protocol":1,"impl":"wolf-interp","impl_version":"0.0.1","commit":"…",
 "file":"upstream/corpus/hello.lu","phase_reached":"run","seeded":false,
 "diagnostics":[],"verdict":"exit(0)",
 "stdout_sha256":"dc97e7cd…","stdout_inline":"hello, wolf\n"}

{"protocol":1,"impl":"wolf-interp","impl_version":"0.0.1","commit":"…",
 "file":"upstream/corpus/grammar/semicolon.lu","phase_reached":"parse","seeded":false,
 "diagnostics":[{"code":"E0002","span":[222,223],"severity":"error"}],
 "verdict":"fail(E0002)","stdout_sha256":null,"stdout_inline":null}

{"protocol":1,"impl":"wolf-interp","impl_version":"0.0.1","commit":"…",
 "file":"upstream/corpus/procs.lu","phase_reached":"resolve","seeded":false,
 "diagnostics":[],"verdict":"unsupported","stdout_sha256":null,"stdout_inline":null,
 "x-unsupported":"concurrency is spec/03 and campaign ic03; nothing here schedules"}

{"protocol":1,"impl":"wolf-interp","impl_version":"0.0.1","commit":"…",
 "file":"upstream/corpus/memory/unsafe_ub_uaf.lu","phase_reached":"run","seeded":false,
 "diagnostics":[],"verdict":"ub(mem.ub)","stdout_sha256":null,"stdout_inline":null,
 "x-ub-row":"P1","x-ub-clause":"mem.prov.state","x-ub-span":[500,504],
 "x-ub-tag-span":[369,380],"x-ub-licenses":"O1: `mut` params lower to `noalias` …",
 "x-ub-tree":["alloc#0 `c.malloc(8)` 8 byte(s), FREED, owned by region #0",
              "  tag#0 c.malloc(8)#root Disabled exposed"]}
```

`phase_reached` never exceeds the deepest rung that *completed*, and `seeded`
is `false` no matter what `--seed` asks for. Both are true statements about
this implementation; unhonoured requests are acknowledged on stderr. A `fail`
carries exactly one diagnostic, because there is no recovery and
`[proto.cmp.phase]` compares only the first. A verdict never carries a payload
(`[proto.record.verdict]`); reasons ride `x-` keys, which participate in
comparison only when both records have them.

## The directive grammar

The leading `//!` block of a corpus file. A line is a directive when its first
`:`-delimited token is one of the four keys; every other `//!` line is prose
(prose contains colons all the time, so an unknown key is never an error).

```text
check:    pass
        | fail(CODE)
        | run( exit=N | exit=trap | exit=trap(kind) [, stdout="…"] )
phase:    none | lex | parse | resolve | typecheck | mem | wir | run
conforms: anchor, anchor, …
member:   true | false
```

- `kind` is one of the closed eleven in `[conf.trap.set]`; `phase` is a rung of
  the canonical ladder. Unknown values are errors.
- Duplicate keys are errors.
- `member: true` marks a file belonging to a multi-file module case (directory
  = module; the package root is the entry file's directory). It is exercised
  through its directory's entry file and never conform-run directly, so it
  carries neither `check:` nor `phase:` — it may carry `conforms:`.
- Every other file is an entry and must carry both `check:` and `phase:`.

As of the current pin this grammar **is** normative: `spec/05` §2a publishes it
as `[conf.directive.*]`, which is the amendment this repo's independent
implementation was written against and helped queue. The parser here matches
the published clauses; a divergence from the compiler's parser is still a
finding, not a bug to paper over.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- corpus
```

`cargo test` includes the suites that make the implementation defensible:

- `run_corpus.rs` — the `run` rung against every corpus expectation, plus the
  **run ledger**: the exact set of files that evaluate, written out so progress
  and regression are both visible, and the "zero UB in safe code" assertion over
  every unsafe-free program in the corpus;
- `ub_coverage.rs` — the D2 coverage matrix. Every row of `[mem.ub]`'s closed
  enumeration is detected and paired with a near-miss twin (`tests/ub/`, corpus
  dialect, upstream-ready), or listed as deferred with a reason; a **planted**
  uncovered row is shown to fail the gate;
- `prov_machine.rs` — the provenance machine through whole programs: the corpus
  litmuses traced, the SB-regression suite (a Reserved reborrow survives a
  foreign read — the Tree-Borrows-not-Stacked-Borrows choice, demonstrably
  encoded), the protector guarantee, the `--trace=prov` namespace, and one
  snapshot per UB report;
- `rule_registry.rs` — 100% clause coverage of the evaluation rules against the
  pinned `spec/anchors.json`, with a planted anchorless rule as the negative
  control;
- `fault_snapshots.rs` — one program per reachable trap identity
  (`tests/faults/`, corpus dialect, upstream-ready), with its fault rendering
  snapshotted: kind, clause anchor, and both spans;
- `region_machine.rs` — is03's acceptance: every §3/§4 fault class paired with
  a **near-miss twin** that must run clean (`tests/faults/ok/`), the leak
  assertion (every region freed at a clean exit) and the forest invariant over
  the whole corpus, and the five D3 optimizer-fact witnesses
  (`tests/witness/`) whose `--trace=mem` output must cite the rules that
  license each fact;
- `conformance.rs` — every corpus file's expectation at the lex, parse and
  resolve rungs;
- `spec_extract.rs` — tests re-derived from the pinned markdown on every run: a
  spec edit that moves the keyword list, the precedence table, the nesting rail
  or a counter-example fails here;
- `fuzz_smoke.rs` — totality over garbage bytes, token soup and mutated corpus
  files, through the evaluator: no panics, and every termination is a verdict;
- `divergence.rs` — the filing pipeline, seeded.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the independence doctrine, the
snapshot ritual, and the commit conventions.
