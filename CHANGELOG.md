# Changelog

## 0.1.20 — 2026-08-31

THE ARMS AGREE (is31). Match arms take the product domain, and with
them the last pattern asymmetry between the two machines closes: the
c06 refusal family s130 retired on the compiler's side had a
deliberately symmetric twin here, and it dies in the same motion
(s130, wolf-lang#179). Released against pin `b80d239` — wolf-lang's
s130 merge; no `v0.2.1` tag existed at the release step, so the merge
sha is the pin, and the spec tree it carries is BYTE-IDENTICAL to
0.1.19's `83f83bb` (404 anchors, nothing gained, nothing dropped —
s130's whole delta is lowering, corpus and CHANGELOG). Census at this
release: 463 files / 430 entries / 33 members; 338 reach run, 313
match, 16 dynamic counterparts, 42 conservatism, 58 out of scope, and
the one standing mismatch is still DIV-2026-019, filed. Every one of
the 455 files carried over from 0.1.19 is verdict-IDENTICAL, class
for class.

- **Struct patterns work in `match` arms (`[gram.pat.struct]`,
  s130/#179).** An arm is a CONJUNCTION of field tests over the
  value's own shape, exactly as a tuple arm is a conjunction of
  element tests: literal fields test, shorthand and renamed fields
  bind, `..` ignores the rest on purpose, and sub-patterns nest —
  through enum payloads (`Dot(Point { x, y: 0 })`, `S((a, b))`),
  through `@`-bindings over products (`q @ Point { x: 0, .. }`, whose
  binds a guard can then read), and through each other. The field-set
  rules hold in arm position exactly as they do in a binder, so an
  unknown field still declines by E0403's name and a
  missing-without-`..` / duplicate / empty one by E0814's, never
  guessed past. Where 0.1.19 answered `unsupported` — "deferred with
  the product match domain" — the arm now runs.
- **The arm boundary takes the WHOLE scrutinee.** `[mem.tier0.move.1]`'s
  initialization reading is what gives a BINDER its field-wise story
  (0.1.19's element-move work); no clause extends partial moves to
  arms, so neither machine invents finer-grained arm semantics. An arm
  that binds a non-`Copy` piece moves the scrutinee whole, and the
  field no arm touched is use-after-move afterwards — E1001's dynamic
  counterpart at the same site. Testing is not taking: the scrutinee
  is only read to run the arm chain, an all-`Copy` arm leaves it live,
  a failing guard takes nothing, and a scrutinee that is no place
  moves nothing.
- **E0802 reaches product arms (`[ty.match.reachable]`).** The
  reachability walk widens column-wise: an earlier unguarded arm kills
  a later one when it covers it column by column, an all-binder
  product is the catch-all later arms die behind, and the scalar
  `_`-after-`true`/`false` rule generalizes to "two arms that split a
  bool COLUMN and constrain nothing else close the shape". Literal
  precision is kept: every column this static walk cannot judge is
  opaque, so it neither covers nor is covered, and a guarded arm still
  covers nothing.
- **The differential, before and after.** The seven struct-bearing
  witnesses of #179's table, at 0.1.19 and at this release:

  | witness | 0.1.19 | 0.1.20 |
  |---|---|---|
  | `grammar/struct_pattern_match_arm` | `unsupported@resolve` (out-of-scope) | `exit(0)` — match |
  | `grammar/match_arm_product_nested` | `unsupported@resolve` | `exit(0)` — match |
  | `grammar/match_arm_at_binding` | `unsupported@resolve` | `exit(0)` — match |
  | `grammar/match_arm_deep_tree` | `unsupported@resolve` | `exit(0)` — match |
  | `typecheck/match_arm_product_unreachable` | `unsupported@resolve` | `exit(0)` + E0802 — match |
  | `memory/match_arm_whole_move` | `unsupported@resolve` | `trap(use-after-move)` — E1001's counterpart |
  | `typecheck/match_arm_product_nonexhaustive` | `unsupported@resolve` | `exit(0)` — conservatism |

  The tuple twin (`tuple_pattern_match_arm`) and
  `match_arm_str_in_product` already agreed and still do. Six of the
  seven join the agreement class outright; the seventh is honest
  conservatism, not agreement — exhaustiveness is the type checker's
  and E0801 has no dynamic half, so `match_arm_product_nonexhaustive`
  sits beside `match_missing` and `match_str_nonexhaustive` in the
  same column they have always occupied.
- **The c06 residue, stated row by row.** The compiler's NATIVE pipe
  still refuses four shapes by name; this machine runs all four, and
  the checked lane runs the first two, so the first two are a recorded
  non-nesting rather than a divergence: an enum or row test inside a
  product (deep trees), a str literal at product depth, a float
  literal at product depth. The **or-pattern** rows —
  `(0, true) | (1, false)` over a product, and `(0 | 1, true)` inside
  one — are the flagged pair: refused by name natively, run here, and
  the checked lane's posture on them is not something this repo can
  measure. Filed on wolf-lang#179 for the residue's own sprint, not
  fixed silently in either direction.

## 0.1.19 — 2026-08-31

THE SHAPE BINDS (is30). Struct patterns land whole-pipe from the
`[gram.pat.struct]` text alone (s129, wolf-lang#179), the destructure
tier they generalize learns the element-move discipline it owed since
s128, and the s106 net byte tier resolves at last (F-0102). Released
against pin `83f83bb` — wolf-lang's s129 merge, two bumps past
0.1.18's `addcd7f` — and the pin question 0.1.18 left open is closed
twice over: the first bump (v0.2.0, `c88ab64`) retired the
FILED_REGISTRY_HOLES waiver for wolf-lang#177 the day r03's
spec-extract fix re-gained `gram.lex.ident` (403 anchors, zero export
notices), and the second bump took the s129 merge itself the moment it
reached origin, so the full witness set rides the census (404 anchors,
`+gram.pat.struct`). Census at this release: 455 files / 422 entries /
33 members; 329 reach run, 306 match, 15 dynamic counterparts, 41
conservatism, 59 out of scope, and the one standing mismatch is still
DIV-2026-019, filed. Every pre-existing file is verdict-identical
across the span except two deliberate movers named below.

- **Destructures move element-wise (`[mem.tier0.move.2]`, the s128
  discipline).** `let (x, _) = p` moves `p.0` ONLY: each element is
  its own place, a wildcard touches nothing, copy-shaped leaves still
  copy, and the untouched elements stay readable — where 0.1.18 moved
  the whole tuple and trapped the sibling read. The corpus twins pin
  both halves (`destructure_partial_live` runs to "1 2";
  `destructure_partial_move` traps use-after-move at the element that
  DID move, E1001's counterpart), and the differ's standing
  "element-story gap" asymmetry closes with them.
- **Struct patterns (`[gram.pat.struct]`, s129/#179).**
  `Point { x, y: p, .. }` in every binder position — `let`/`var`, D63
  comma groups, `for` headers — with shorthand binding the field's own
  name, explicit sub-patterns nesting through structs and tuples both
  ways, fields in any order, and `..` ignoring the rest on purpose.
  Named fields consume their own `Proj::Field` sub-place per the tuple
  precedent: omission, `..` and wildcard fields stay live, and the
  fault twin traps at the field that moved with the counterparty's
  span. The refusal classes decline by the code that owns them
  upstream — unknown field (E0403), missing-without-`..` / duplicate /
  empty (E0814) — never guessed past. Match ARMS defer symmetric with
  the compiler's own c06 product-domain refusal: the arm witness
  answers `unsupported` on both machines, and the two advance together
  the day the product match domain lands. The ten-file s129 witness
  set compares clean across the machines: six byte-identical
  agreements (the binder sweep and partial-live among them), one
  dynamic counterpart, two refusals-by-name, one
  unsupported-both-machines — zero divergences.
- **The #184 twin joins the agreement class.** lupin ran the
  lent-view byte slice all along; with fd42622's compiler fix in the
  pin, `byte_view_slice_lent` answers the same bytes on both machines
  (`4 119 4 108 2 0`) and the whole slice quartet agrees, fault twin
  included.
- **The net byte tier resolves (wolf-interp#52, wolf-std F-0102).**
  `net_read_bytes`/`net_write_bytes` land as the str calls' own shape
  with `List[int]` marshalling: one receive of up to `n` raw bytes
  with NO utf8 row (a lone `0x80` is data), whole-or-raise writes
  behind the WHOLE pre-write check (an element outside 0..=255 is the
  `invalid` row and nothing reaches the wire — §14's fs vocabulary,
  which wolf-std's facade adopts verbatim), rows declared for #47's
  arm discrimination. The two byte-tier corpus witnesses
  (`net/byte_roundtrip`, `net/line_reader_bytes`) leave the
  conservatism ledger for the match column — the release's two
  deliberate verdict movers — and wolf-std's compiler-lanes-only rows
  can go three-lane at its next pin.

## 0.1.18 — 2026-08-30

THE FIRST ARM YIELDS (is29). The two thrice-measured lupin-side
correctness debts behind wolf-std's four `divergent(…)` ledger rows,
paid from the spec clauses and the F-0079 lineage, never from the
compiler's source: a multi-arm handler over a BUILTIN-raised row took
its first arm for every tag (#47, wolf-std F-0097), and take-mode
reuse — a static E1001 on both compiler rungs — executed here to its
dynamic outcome (#48, wolf-std F-0098).

Released against pin `addcd7f`, unchanged from 0.1.17 — wolf-lang has
no `v0.2.0` tag at this release, so the FILED_REGISTRY_HOLES waiver
for wolf-lang#177 (`gram.lex.ident`) carries to is30 and the pin
question re-opens there. Every pre-existing corpus file is
verdict-identical before and after every commit in this span: the
judge counts are 310 run / 287 match / 13 counterpart / 41
conservatism / 55 out of scope / 1 mismatch — byte-for-byte the
0.1.17 per-file table, three times over.

- **The row rides with the raising builtin (#47).** The #29 mechanism
  at its third address: entry-file raises discriminated (s70),
  imported-module raises were fixed by 0.1.13's arm-selection pass
  (the row travels with the value), and a BUILTIN's raise carried
  `row: []` — so a handler's every lowercase arm read as a binding
  and the first arm matched every tag, silently, exit 0. Builtin
  error values now mint with the raising builtin's WHOLE declared row
  (`eval::builtin::declared_row` — the net and process prelude
  signatures the module docs pin, the env/json/cwd/utf8 mint-site
  closures), so sibling arms resolve as tags and the value's own tag
  finds its own arm in either order. Witnessed in both arm orders on
  the issue's reproducer (one dead port, dialed twice: `-1`/`-1`
  where 0.1.17 answered `-1`/`-9`) and hermetically on `env_get`'s
  two-tag row. The lint walk's `operand_row` reads the same table, so
  the static arm rule and the dynamic one keep answering alike. The
  spec pins none of these signatures — filed as wolf-lang#181 per the
  is26 pattern rather than absorbed.
- **Take-mode reuse joins the static rung (#48).** The E1007
  discipline at the moved place: an explicitly moded argument or
  receiver over a whole binding an earlier call-site `take` marker
  consumed is `fail(E1001)` at the reuse argument — the counterparty's
  code, span and message shape, observed at `addcd7f` — instead of
  executing to the trap map's answer. Straight-line certainty only:
  re-initialization and shadowing clear (`[mem.tier0.move.4]`), moves
  inside branches, loops, closures and `defer` never leak past them,
  field-granular takes are not tracked, and a bare unmarked READ of a
  moved-from place stays `[mem.tier0.move.2]`'s dynamic
  `trap(use-after-move)` — which is what keeps
  `memory/move_use_after.lu` on its DynamicCounterpart verdict and
  `faults/use_after_move_field.lu` on its pinned trap, unmoved.
- **W0317 lands the lupin half of wolf-lang#167's D61 row.** The
  kindness lint (`[gram.expr.index.origin]`): an int literal fed to a
  List local's `.get` inside a 1-origin scope warns at the literal —
  span parity with the compiler on the corpus witness (`[468,469]` on
  `lints/index_origin_get.lu`), the statement marker narrows and the
  innermost wins, a non-literal index and a user type's `get` stay
  silent (probed: the compiler does not warn there either). The lint
  replays the parser's own `index_origin_of` reading, one marker
  grammar for two consumers.

Downstream, pre-recorded in wolf-std's row comments and verified on
this build directly against wolf-std's tests/ before tagging:
`net/refused_row` and `net/closed_row` print their directives'
stdout (`handled: -1` / `peer-gone: handled`, then the propagated
tag), and `net/use_after_close` + `process/use_after_wait` are
`fail(E1001)` where 0.1.17 trapped `use-after-move` / diverted at
`start()` — the four `divergent(…)` words flip to `run` at sc-track's
next binary bump, which is the red the runner was built to read.

## 0.1.17 — 2026-08-29

THE INTERPRETER KEEPS TWO PROMISES (is28). Two user-visible fixes,
both measured live on 0.1.16: a scratch directory where every file
says `//! member: false` still collided on `main` (#49 — the run-door
half of D59 was never landed), and the origin marker the language
ruled in D61 either failed at lex (`#![index(1)]` → E0101) or —
the dangerous class — was a silently ignored statement attribute that
ran `grammar/index_origin_scopes.lu` to the WRONG answer
(wolf-lang#169). Both promises implemented from the ruling texts and
the spec clauses, never from the compiler's source.

Released against pin `addcd7f` (the s126 wave; one bump, `e561c6f` →
`addcd7f`). Corpus 422 → 430 files (397 entries + 33 members);
coverage ratchets 157 → 159; anchors 396 → 397. All six run-reaching
`index_origin_*` witnesses MATCH at first sight — including the
byte-exact stdout pins on the file-wide and scopes witnesses — and
the two fail pins land the spec's own codes (E0211 by position, E0813
by name). Every pre-existing corpus file is verdict-identical before
and after: the judge counts moved 304→310 run / 279→287 match /
13/41/55/1 unchanged — exactly the eight new witnesses, nothing else.

- **The four standalone spellings, in module formation
  (`[conf.directive.standalone]`, D59 — #49).** A file opts out of
  its directory's module by an explicit `member: false`, the
  `check:`+`phase:` entry pair (0.1.16's only exclusion), a script
  announcement (`#!` line or `pkg { … }` frontmatter), or a
  `_test.lu` name; an explicit `member:` key always decides; the
  named entry always compiles; std/dep trees stay whole-package. The
  asymmetry is kept: a standalone mark opts the FILE out and never
  shrinks anyone's build — a plain `main` beside a standalone `main`
  still collides, and the E0302 note now names the escape.
- **The three E0301 situations, this machine's voice.** A name that
  IS defined next door in a standalone entry answers with the file,
  the marker, and the fix; an import whose files all opted out lists
  them; a directory with no `.lu` files gets a formation note instead
  of a layout assertion.
- **`#![` and `#[index(…)]` lex, parse, scope (D61 —
  wolf-lang#169).** `#![` is one token and the shebang narrows around
  it (`#!` not followed by `[`; the script witness stays green); the
  file-wide form is legal only as the file's first non-trivia
  construct (E0211 anywhere else, by position); the statement form
  scopes the annotated node's full lexical extent, nesting legal,
  innermost wins, `#[index(0)]` restores the default; bad arguments,
  duplicates, and unknown INNER attributes are E0813 by name — never
  ignored, the faulty marker takes no effect.
- **The shift, exactly per `[gram.expr.index.origin]`.** Subscript
  reads, writes, and slice PLAIN starts lower by one CHECKED
  subtraction; plain ends lower unchanged (the inclusivity coupling —
  `..=` is redundant-but-legal); `^n`, open sides, `.len`, bare
  ranges, `.get`, map keys, pool handles, tuple members and the
  unsafe tier do not move. `xs[int.min]` under origin 1 traps
  `overflow` before the bounds question (X3, D56's kind). The human
  trap line renders the WRITER's numbers inside a 1-origin scope
  (`index 0 is outside…`, `2..9 (origin 1)`) — the writer's-mode duty
  D61 puts on the machine whose report prints numbers.
- **`char` assignment copies (#50, D58).** `d = c` then `{c}` printed
  `xx` under wolf and trapped use-after-move here; a `char` is now a
  copy value in the tier-0 move discipline exactly as `int` is.
- **Comma-grouped binders (D63 rider).** `let`/`var` admit
  `binder (',' binder)*`, each binder with its own `=`; the group is
  the left-to-right sequence of single bindings. One-initializer-
  many-names and Python's bare tuple are refused by name with both
  correct spellings offered; `const` keeps its single-binder
  production.
- **D62 witnesses (rider).** Nothing to build — `+`/`+=` on two strs
  is the language and this machine's behavior IS the ruling. The
  legal-chain run witness and the three refused mixes (`str + int`,
  `str + char`, `int + str`, refused by name) land in-repo
  (`tests/d62/`, upstream-ready) as the differential counterpart
  waiting for the compiler half (wolf-lang#172); #51 closes from
  that side.
- **An upstream finding, filed.** c1f54f2's anchors regen DROPPED
  `gram.lex.ident` from `anchors.json` while spec/01 §1.3 still
  defines it — wolf-lang#177, carried as a `FILED_REGISTRY_HOLES`
  notice on every export until the registry re-gains the anchor.

## 0.1.16 — 2026-08-28

THE INTERPRETER NAMES THE LINE (is27, the lupin half of s125's
trap-site pair — sequenced after 0.1.15 as that contract required). At
0.1.15 a trap said `at 51..58` — byte offsets no editor jump-to and no
human counts out (#158, measured in the s125 report). The renderer HAD
the span; the gap was purely rendering. This release renders it:
implemented from `[conf.trap.render]`, never from the compiler's
rendering (the independence doctrine).

Released against pin `e561c6f` (the s122–s125 wave; one bump,
`a900b8c` → `e561c6f`). Corpus 403 → 422 files (389 entries + 33
members — including the corpus's FIRST four bare members, D59);
coverage ratchets 153 → 157; anchors 393 → 396; twelve of the
thirteen new run-reaching entries match at first sight.

- **The file doors say `line:col`.** Every human fault line `run`,
  `check`, `lex`, `parse` and `conform-run`'s human mode print — trap,
  UB finding, static diagnostic, secondary spans included — spells its
  location `line:col`: 1-based line, 1-based column counted in
  **characters**, the spelling the fault snapshots pinned first (one
  span grammar per tool, `[conf.trap.render]`). `examples/overflow.lu`
  now says `[arith.checked] at 6:5` where 0.1.15 said `at 107..113`.
- **Byte spans stay on the structured door, untouched.** `--json`
  records are byte-identical to 0.1.15's (identity fields aside):
  `[proto.record.diag]` spans and `x-trap-span` remain byte offsets.
  wolf-std's runner parses records only, and a filtered `std-test`
  smoke against this build stayed green.
- **The two machines name the same place.** `tests/faults/trap_site.lu`
  pins the s125 witness shape: the trap expression sits at 6:5; the
  compiled tier reports `  at ./trap_site.lu:6:5` (`[conf.trap.report]`,
  exit 134) and this machine `[mem.ub.defined] at 6:5` (exit 3) — the
  KIND is the contract, the statuses are per-machine documented facts
  (`[conf.trap.exit]`, D60; **exit 3 does not move**). The differential
  runner's trap-map site column stays a named follow-on.
- **A module sibling's fault names its file.** A diagnostic raised in
  a `use`d module file renders `line:col` against THAT file's text and
  says so: `entry.lu: E0201: … at 4:13 (in module file `mangled.lu`)`.
  The record's reason now names the module file relative to the
  entry's directory — an absolute path there made byte-identical
  re-exports impossible the moment the corpus grew a broken-sibling
  witness (records travel; the export gate diffs them).
- **The REPL keeps entry-relative byte offsets, deliberately.** A
  session has no stable line numbering — each entry restarts at offset
  0 and an ownership fault's second span may point into an earlier
  entry — so `docs/repl.md` documents the offset spelling as the
  prompt's own coordinate; `[conf.trap.render]` is a file-door clause
  (its exit-status sentence cannot even apply to a session that
  survives the trap, `[repl.trap.alive]`).
- **D59 membership lands in the walk (`[conf.directive.standalone]`).**
  An explicit `member:` decides; otherwise the entry pair does — a
  plain `.lu` file with no directives is a member of its directory's
  module by default (`resolve/bare_sibling/` is the witness; half an
  entry pair stays an error either way). The corpus-walk half of #49;
  script-header and `_test.lu` module formation remain open there.
- **DIV-2026-019 filed.** The broken-sibling witness pins the
  counterparty's `fail(E0202)` (it reads the junk to EOF) where this
  machine stops at the first bad token (`fail(E0201)`@parse) — same
  rung, span-or-code class, and the spec assigns neither code to junk
  recovery: filed as a spec gap, waived by the filing, gating resumes
  when it resolves. E0202 itself is corpus-pinned now and left
  `diag::UNPINNED_CODES`.

## 0.1.15 — 2026-08-28

THE SCALAR RELEASE (is26, one sprint behind s121/D58). At 0.1.14 the
lexer refused `'` outright — `fail(E0101)`, "`'` begins no token" — so
every char witness the compiler landed was wolfc-lane evidence only:
wolf accepted programs its oracle could not even lex, the weakest
position a differential oracle can hold. This release teaches lupin the
scalar, independently, from `[type.char]`, `[gram.lex.char]` and
`[mem.str.chars]` — never from the compiler's source.

Released against pin `a900b8c` (the s117–s121 wave; one bump,
`90c90df` → `a900b8c`). Corpus 385 → 403 files (374 entries + 29
member files), all 14 new run-reaching entries matching at first sight
because the scalar landed here BEFORE the bump; coverage ratchets
144 → 153. The seven char witnesses — `char_battery`, `char_order`,
`char_interp`, `chars_walk`, and the three `faults/char_cast_*` twins —
compare for the first time: **0 → 7**, and the walk holds 0 mismatches.

- **Char literals lex (`[gram.lex.char]`, D58.5).** `'a'` at every
  UTF-8 width, with the string escape set plus `\'`
  (`\n \t \r \\ \' \" \0 \xNN \u{1–6 hex}`). The malformed shapes are
  **E0110** named refusals, one report per literal: empty,
  multi-scalar (a base-plus-combining-accent pair is two scalars — a
  char is a scalar, not a grapheme), unterminated before end of line,
  and a `\u` naming a non-scalar — the surrogate gap and past-0x10FFFF
  are refused AT THE LITERAL, the lex-time twin of the cast's trap.
  E0110 moved to this clause from the unpinned unterminated-interp
  code, which yielded and became E0112.

- **`char` is a value, and not an integer (D58.1/.3).** `Value::Char`
  carries the scalar; equality and order are total, by scalar value,
  locale-free (`'z' < 'é'`). Arithmetic, mixed comparisons, and
  numeric-literal adoption (`let c: char = 65`) are refused by name —
  the permissive-direction divergence that is hardest to notice is the
  one this machine refuses loudest. `match` over char rides scalar
  identity; CHAR_LIT parses in primary, pattern, attr and
  const-argument positions.

- **The casts, with the trap (`[type.char.cast]`, D58.4).**
  `char as int` is total; `int as char` traps `overflow` (D56's closed
  family) on negative, on the surrogate gap `0xD800..=0xDFFF`, and
  above `0x10FFFF` — with the gap edges `0xD7FF`/`0xE000` and the last
  scalar `0x10FFFF` legal and witnessed. Everything else is refused by
  name: only `int` bridges into `char`.

- **`chars()` yields `List[char]` (`[mem.str.chars]`, D58.7).** The
  scalars in string order; the width identity holds — the byte extent
  of a scalar is a function of its value, and a cursor advanced that
  way lands exactly on the boundaries `get` accepts (`chars_walk`
  witnesses it over 1/2/3/4-byte scalars). `{c}` interpolation prints
  the CHARACTER, never the number (spelled `{c as int}`); a spec on a
  char hole takes the str surface, width in bytes.

- **The spec tension is filed, not settled.** `\u{…}`'s one-to-six
  digit cap is prose-only against `CHAR_ESC`'s unbounded
  `HEX_DIGIT+`, and the string tier states no cap at all; this machine
  takes the prose reading and records the choice (`gram.lex.char` in
  the CHOICES register) rather than copying the compiler.

- **Portability honesty (macOS).** The net edge probe asserted the
  write-after-peer-close row on the very next write; the failing write
  actually waits on the peer's RST, which macOS delivers a few ms
  late. The probe now retries on a bounded clock — same transcript,
  no race assertion. First release cut on macOS arm64.

## 0.1.14 — 2026-08-27

THE CATCH-UP RELEASE (r02, sprints is14 → is25). 0.1.13 shipped on
2026-08-15 — and then twelve sprints landed, 58 commits, with the upstream
pin re-vendored **eight times**, while the version, this file, and the tag
never moved. Two materially different interpreters could both answer
`lupin 0.1.13` while declaring different conformance pins: the version had
stopped identifying a state. This entry is the correction, and it is
honest about the gap: one release covering the whole span, grouped by
theme — not twelve retroactive versions pretending each had shipped.

Released against pin `90c90df` (the s114–s116 wave). The span advanced the
pin `02c1e88` → `c9da6d9` → `b522b8a` → `1b149ba` → `87405ac` → `21b129e`
→ `da8582d` → `77466a3` → `90c90df`; each re-vendor sits under its
sprint's merge in the log, and each registered what its wave demanded (the
`ct` namespace at `da8582d`, `type` at `77466a3`, `os` and `pkg` at
`90c90df`). Corpus 294 → 385 files, the walk green at the pin (385
file(s), 0 failure(s)), and the run-ledger holds 0 mismatches throughout.

- **The version now tells the truth (D57, this release's own change).**
  `--version` distinguishes a release build from everything else: built
  exactly at its `v{version}` tag, lupin prints the bare version; built
  anywhere else — trunk, a branch, no git at all — it prints
  `0.1.14+dev.<commit>`. A trunk build never claims to be the release
  again, and wolf-lang's PAIRING gate now compares the declared pin
  whenever the sibling reports a release build, which is the check that
  would have caught this whole gap on day one.

- **The divergence families closed from the slower side (is14–is17).**
  `?` keeps its widening promise (#33); the dispatch floor under an
  `impl` is the trait's default (#32); the list spine is copy-on-write
  and every write diverges its copy (#28); a container knows its home,
  so freed-region access faults (#25) with the home consult gated on
  region teeth (one atomic load until a free or freeze); a returned
  region transfers instead of tearing down (#35); a `mut`-receiver
  method demands the call-site marker (#37); a stale captured-place read
  refuses instead of answering (#36); prim impls dispatch, so the
  trait-qualified call reaches `impl Text for int` (#34).

- **The interpreter crossed behind: net, json, process (is18).** The
  s39/s40 std families run — sockets over `std::net` (nonblocking polls
  under the baton), the process trio over `std::process` (wait reaps,
  kill never tombstones), and the query tier on lupin's own RFC 8259
  reading. Plus the twins: a nested named `fn` binds like a `let` (#38)
  and module identity is the full path, so same-leaf modules coexist
  (#39).

- **The literal tier and the width rules (is21, is23, D52/D54).** Int
  literals adopt a float expectation and the operator bridge propagates
  it (D54); a declared row resolves its tags one position wider — the
  D52 mirror; wrapping shift counts mask to the type's bit width (#42);
  the expected-type flow reaches container-argument position, so a
  full-range wrapping literal no longer traps there (#43).

- **The lint and arm corrections (is19, is24).** E0812 lands
  (explicit-application arity is one count against another); W0316 is
  implemented, not absent; a match arm over a tag-shaped scrutinee
  resolves as the tag, not a binding (#44); the capture law healed with
  the arm fix and the `free_names` gap was filed rather than papered
  over (#45).

- **The scheduler keeps time (is20).** A sleeping task parks and the
  park bound is the earliest pending wakeup (#40); provenance prune
  revisits only dirty allocations, not the program's whole history
  (#41). The witnesses moved 503ms → 66ms and 279s → 3s.

- **The line editor (is25).** The REPL prompt gets readline at a TTY:
  history (JSON-line, capped, deduped, in the platform state dir),
  three-source TAB completion (surface names, never `f#N`), word/kill/
  case ops, `Alt-.` yank-last-arg cycling, and a Ctrl-C that cancels the
  pending line instead of killing the session (#46). Pipes keep the dumb
  reader verbatim; `--no-edit` and `TERM=dumb` keep it too; a raw-mode
  failure degrades with a note. rustyline 18 (MIT, no default features)
  carries the glue; the editor's own layer is pure logic with PTY
  keystroke evidence behind it. `:keys` lists the bindings. Also from
  the span: the REPL's trap-survival reminder prints once per session,
  not once per trap (is22).

## 0.1.13 — 2026-08-15

THE ARM-SELECTION PASS (sprint is13). One silent wrong answer and one
complexity bug; no corpus verdict moves, and every entry file of the
pinned corpus records byte-identically before and after — the only file
whose record changes is the one added to catch the miss.

Released against pin `02c1e88`, which advances `upstream` across the
s88/s89/s90 wave: the corpus grows 283 -> 294 files, the bundle 298 ->
308 records, anchors 316 -> 323, and coverage ratchets 107 -> 114.
`grammar/range_bare.lu` pins E0201, so that code left this
implementation's UNPINNED_CODES — the corpus is its authority now.
The differential is GREEN at the new pin: 308 entries, 0 divergences.

- **wolf-interp#29 FIXED (wolf-std F-0079): a multi-arm `else`-match
  handler took its FIRST ARM for every tag when the row was raised in an
  imported module.** No diagnostic, exit 0, a confident wrong answer —
  the shape that reads correctly answering the wrong question. The same
  program gave `10 20 30` under wolfgang and `10 10 10` here.

  Whether a bare lowercase identifier in a pattern is a *row-tag
  pattern* or a *binding* is a question about the scrutinee's row, and
  the checker answers it from the row type. This machine asked the
  **matching module's own signatures** (`sema::Module::row_tags`,
  collected per module from that module's own `fn` headers). A handler
  in the entry file over `tagmod.miss()`'s row therefore found no
  declared `alpha`, read every arm's pattern as a fresh binding — and a
  binding matches anything — so the first arm won for every tag.
  Reversing the arms reversed the output, which is what tells "first arm
  always" from "one tag happened to be right".

  The row now **travels with the value it was raised through**
  (`ErrorValue::row`, recorded at the raise site from the enclosing
  function's declared return row, and carried through `?`, through
  payload application, through every copy). A row that crosses a module
  boundary is still the same row, so arm resolution asks the value which
  row it came from instead of asking the far side's declarations again.
  The module's own vocabulary stays in the union behind it, so a tag
  with no declared row still resolves where its module declares it — and
  a lowercase name the row does *not* declare still binds: `else |err|`
  keeps its binder and an `other =>` rest arm keeps catching.

  `corpus/rows/cross_module_arms/` is the witness — three arms in both
  orders over an imported module's row, a `?` hop on the far side, and
  the two binder shapes as the counterweight. It runs `10 20 30` on both
  compiler rungs and here; before the fix this machine printed
  `10 10 10` forward and `30 30 30` reversed. `rows/propagate` was the
  corpus's only cross-module handler and it took `else |_|`, which is
  why nothing noticed. The unit-sized half, which does not wait on a pin
  bump, is `tests/rows_option.rs`'s
  `a_row_raised_across_a_module_boundary_still_dispatches_by_tag`.

- **wolf-interp#28 half-FIXED (wolf-std F-0078): the index read no
  longer copies the container.** `xs[i]` evaluated `xs` in order to pick
  one element out of it, and evaluating a place-valued `xs` deep-copies
  the whole thing: O(n) per read, O(n²) per walk. The same lend as #24's
  receiver fix, on the index-read path — the read is charged at exactly
  the moment and in exactly the order it always was, the base's own fuel
  step included, and the container steps out of its slot only for the
  length of one `builtin::index` call, which cannot re-enter the
  machine. Slices keep the copy (`s[a..b]` reads its endpoints off the
  *syntax*, the exclusion `lendable` already makes for `str.get(a..b)`),
  as does a base that is not a plain place path — there would be nowhere
  to put the value back.

  Measured, a `List[int]` of N built by push then read back by index
  (release build, CPU seconds, min of three, this box):

  | N | before | after |
  |---|---|---|
  | 2 000 | 0.156 s | 0.042 s |
  | 4 000 | 0.576 s | 0.080 s |
  | 8 000 | 2.317 s | 0.157 s |
  | 32 000 | 21.500 s | 0.333 s |

  Four times the work per doubling down to under two — the curve the
  issue's `for v in xs` walk already had — and **65× at 32k**.

  One shape moved, and it moved onto the compiler's answer. The copy
  path snapshotted the container *before* evaluating the index, so a
  write hidden in the index expression was invisible to the read that
  followed it: `xs[bump(mut xs) - 1]` trapped `bounds` against the stale
  length in interpolation position, while the identical `let v =
  xs[bump(mut xs) - 1]` answered 9 — this machine disagreeing with
  itself over two spellings of one expression. The lend is taken *after*
  the index is evaluated (#24's ordering), so both answer 9, which is
  what wolfgang answers. Pinned in `eval::tests`.

  **The other half of #28 stays open**: a read-mode `List` argument
  still copies, on the way in and again on the way out
  (call-by-value-result reads every parameter's final value back). That
  is the same value-model question as #25 and is not a lend away — a
  callee runs arbitrary user code, which can reach the caller's place by
  other names.

- **wolf-interp#25 not taken.** Closing it means giving region-homed
  containers identity in the value model — a change to how values are
  represented, not an arm selection or a lend. It stays declared in the
  approximation contract, §6.13 and §6.14.1.

## 0.1.12 — 2026-08-13

THE OWN-LEDGER PASS, then the re-pin. Two of this machine's own defects
first — wolf-interp#24 fixed, wolf-interp#25 measured and deliberately
deferred — and then pin `f8dca42` → `4e316ad` (latest green trunk, CI
run 31742412670, `headSha` matching). Corpus grows 3: 280 → 283 files.
Ratchet holds 107, anchors hold 316 (the range touches **no** `spec/`
file), bundle 320 programs / 298 records.

- **wolf-interp#24 FIXED: `List.push` was quadratic, and the cost was
  never in `push`.** `items.push(Slot::live(arg))` is an amortized O(1)
  `Vec` append and always was. The cost was the *call*: `eval_method`
  copied the receiver out of its slot (`read_path`), copied it a second
  time to compare against (`original_receiver`), compared the two whole
  values to decide whether the method had written, and copied the result
  back — **four traversals of the whole list to append one element**. So
  every `List`-returning std function was quadratic on the reference
  lane, and wolf-std could not write around it.

  The fix is to **lend** the receiver instead of copying it (`Lend` in
  `src/eval/mod.rs`). A builtin container — `List`, `Map`, `str` —
  always dispatches to `builtin::method` (`method_of` answers only for
  `Value::Struct`), and no container arm of `builtin::method` re-enters
  the machine, so the value can leave its slot for the duration of the
  call and return to it. Nothing observable moves: the read is charged
  at the same moment in the same order, and the lend is taken *after*
  the arguments are evaluated so `xs.push(xs.len)` still reads `xs`
  through its parent while the receiver's tag is Reserved
  (`corpus/memory/prov_two_phase.lu`'s two-phase window, untouched).

  Two details carry the correctness. Whether the lend ends in a *write*
  is decided by an O(1) witness — `List.push` and `List.pop` are the
  only two arms of `builtin::method` that mutate a receiver
  (`builtin::mutates_receiver`), and both change the element count
  exactly when they change the value — so `[mem.region.freeze.4]`
  (issue #20, a read-only method must not write back) still holds
  without the whole-value comparison. And a *mutating* lend asks
  `writeback_would_trap` first: `write_path` faults before it stores, so
  on that path the receiver must survive the call unchanged, which only
  a copy can promise — the lend is declined there and the old path runs.
  In a debug build the whole-value comparison is kept as a
  `debug_assert_eq!` against the witness, so the entire test suite and
  corpus check the classification on every method call rather than
  trusting it.

  Measured, same program at four sizes (`var xs = List[int]()`, N
  pushes, release build, this box):

  | N | before | after |
  |---|---|---|
  | 4 000 | 0.40 s | 0.038 s |
  | 8 000 | 1.44 s | 0.055 s |
  | 16 000 | 6.18 s | 0.100 s |
  | 32 000 | 30.33 s | 0.191 s |

  Quadratic (4× per doubling) to linear (~1.9× per doubling); **159× at
  32k**, and the compiler's two run-reaching rungs do 32k in 0.14 s, so
  the oracle is now within a small factor of the thing it is oracle for
  instead of 200× off it. The gate that matters is not the clock: the
  full `lupin corpus` report — human and `--json` — is **byte-identical
  before and after the change**.

- **wolf-interp#25 MEASURED, and deferred as a named design task.** A
  container escaping its region still runs to `exit(0)` here and is
  E1010 upstream. The issue diagnosed this as containers lacking
  identity in the value model; re-measured, that is half the story, and
  the other half is more useful: **the struct sibling escapes too**
  (`memory/region_escape_local.lu`, also `exit(0)`), and
  `Value::Struct` *already carries* `home: Option<RegionId>`. The real
  gap is that nothing ever checks a tier-0 value's home against its
  region's liveness — `home` has exactly one reader today
  (`frozen_container`, on the write path, asking only about `freeze`),
  and `RegionState::Freed` is consulted for pools and handles and never
  for a value. `docs/approximation-contract.md` §6.14.1 now carries the
  three-part design that closes it, concretely enough to execute:
  give `List`/`Map` a `home`, give `home` a second reader on both the
  read and write paths, and hand-write `PartialEq` so identity never
  leaks into a value's equality. NOT done here on purpose — it changes
  the trap surface, and landing it in the same pass as a rewritten
  method-call path and a pin bump would make any resulting divergence
  un-bisectable.

- **The re-pin: `str_from_utf8` is the only implementation work the
  wave asked for.** Three new corpus files, one per sprint. s80's
  `memory/foreign_root_aliasing.lu` — the witness for a **real
  miscompile**, where the release tier answered `x=5 y=5` for a program
  whose answer is `x=5 y=7` — runs clean here at first sight and always
  did: there was never a bug on this side, because the aliasing question
  the optimizer got wrong is one an interpreter never has to ask. It is
  the cleanest demonstration of what the oracle is for that this project
  has produced. s81's `strings/equality_lanes.lu` likewise ran clean.
- **`str_from_utf8`, implemented against a spec that does not mention
  it.** s81 adds the language's first bytes-to-`str` path — the byte
  SOURCE to match s77's byte VIEW — and it validates, because an
  unchecked one is the forging hole for "every `str` is valid UTF-8".
  It has **no `spec/` clause**: it is specified by its prelude signature
  (`List[int] -> str ! {utf8}`), its doc comments, and one corpus
  witness. That gap is declared rather than papered over. Beyond the
  witness's seven refusals, **38 ugly inputs were probed against both
  machines and all 38 agree byte for byte**: lone continuations (0x80,
  0xBF), truncations at 2/3/4 bytes, overlongs (C0 AF, C1 BF, E0 80 AF,
  F0 80 80 AF), surrogates (D800, DFFF) against valid U+D7FF,
  past-U+10FFFF (F4 90 80 80, F5, F7), never-bytes (FE, FF, C0, F8),
  non-byte elements (256, −1, −128, 1000000), interior NUL, the empty
  list, and the boundary scalars U+0080/U+07FF/U+0800/U+FFFF/U+10000/
  U+10FFFF. The failure is the `utf8` row with an empty payload — tag
  identity confirmed on both machines, and the counterparty's own
  unreachable-arm warning proves the row is exactly `{utf8}`.
- **All three counterparty tiers GREEN, and the lanes grew.** `checked`,
  `native` and `release` each report exactly one divergence, the same
  one, byte for byte: DIV-2026-017. Both machines now execute **117**
  (checked), **119** (native) and **109** (release) of the 261 entries,
  up from 107/107/98 — the counterparty reaches `run` on 130/125/115,
  up from 120/113/104. Upstream s82 consumes these numbers; they moved,
  and the release lane moved most.
- **Known, and NOT this release's doing: the mutation arm of `fuzz_smoke`
  got slow at this pin.** `mutated_corpus_files_never_crash` mutates 3000
  randomly picked corpus files and runs each through all four rungs; the
  pinned seed is fixed but the FILE SET is not, so 280 → 283 files
  reshuffles which mutants get generated, and this pin's draw includes
  some that run to `Machine::FUEL`'s 50-million-step rail. The rail is
  the wrong size for a harness that runs 3000 programs: one mutant that
  reaches it costs **6.1 s** in a release build and **37 s** in the debug
  build `cargo test` actually uses, so ~40 runaways out of 3000 spend the
  whole budget. Measured: the test does not finish inside 25 minutes.
  Verified NOT to be the lend's doing by re-running it with `src/`
  stashed at this same pin: equally slow. CI sets no step timeout, so it
  slows the build rather than failing it, but `cargo test` is no longer a
  gate anyone will wait out. Fix before the next pin: a per-case
  wall-clock (or step) bound in `exercise`, well under the language's own
  rail — the harness is testing that the frontend ANSWERS, not that the
  fuel rail works, and `[gram.lex.rails]` has its own litmus for that.
- **#76 (DIV-2026-017) re-confirmed OPEN and unchanged.**
  `lints/raw_interp_braces.lu`: `r"{who}"` prints `{who}` here and
  `"{who}` upstream, whose raw-literal decode keeps the opening quote of
  the two-character `r"` delimiter. Identical on all three run-reaching
  tiers, which is still how it is known not to be the mid-end's doing.
  `[gram.lex.str.raw]` and the corpus header's own `stdout="{who}"` both
  agree with this machine. Unfixed upstream across three sprints.

## 0.1.11 — 2026-08-13

THE ORACLE HELD: the semantics-wave re-pin. Pin bumped `613c3dc` →
`f8dca42` (latest green trunk, CI run 31676830124; the two red runs in
the range were not taken). This is the largest semantic movement the
compiler has had in one wave — s74 (the correctness cluster), s75
(`List` element access lowers to a load, bounds checks caller-side and
eliminated when a relational range channel proves them), s76
(containers allocate in the **ambient region** at the allocation site,
dynamically scoped per D12), s77 (`s.bytes()` is a **view** over the
receiver's own storage), s78 (an affine relational channel in the range
analysis), plus s53 (script mode) and the D43/D44 rulings. The corpus
grows 13: 267 → 280 files. Ratchet 103 → 107, anchors 315 → 316, bundle
317 programs / 295 records.

- **The wave moved the compiler's lowering out from under three
  semantic areas, and this machine's independent reading already agreed
  on all three.** Ten of the thirteen new corpus files reach `run` here
  at FIRST SIGHT with no new semantics written on this side —
  including all four of the wave's own semantic witnesses
  (`memory/region_container_reclaim.lu`,
  `memory/region_container_freeze_ok.lu`, `strings/byte_view.lu`,
  `strings/slice_boundary_sweep.lu`). The corpus-wide differential
  found **no new divergence** at any tier.
- **Probe 1 — region-scoped container lifetime (s76): agree on every
  defined shape, one gap declared.** A container freed with its region,
  a callee allocating into its *caller's* region (D12), growth across
  region chunks, `freeze` outliving the building block, nested regions:
  identical on `lupin`, `--native` and `--release`. s76 moved *toward*
  this machine's dynamic reading. The escape is the gap:
  `memory/region_escape_container.lu` is E1010 on every compiler lane
  and `exit(0)` here, and — unlike the handle/pool escape, which traps
  `region-fault` — this machine does not catch it dynamically either.
  Conservatism in the ledger's sense, since no conforming program can
  observe it, but a real modelling gap, now declared as approximation
  contract **§6.13** rather than left implied.
- **Probe 2 — byte views and the slice domain (s77): agree,
  exhaustively.** The domain was swept, not sampled: all 100 endpoint
  pairs of `s.get(a..b)` over the mixed-width `é€` from −2 to 7,
  **including the entire negative half the corpus file does not
  reach**, are byte-identical across `lupin`/`--native`/`--release` —
  six defined pairs, and a *miss* rather than a wrap-around for every
  negative endpoint, which is the `lo <=u hi <=u len` unsigned reading
  agreeing on both sides. The trapping form `s[a..b]` was swept over 29
  ugly pairs (negative, inverted, mid-codepoint, past-end,
  degenerate-empty, open-ended, inclusive, `^n` from-end): **28 of 29
  agree exactly.** `bytes()` is unsigned 0..=255 on both, byte-length
  on both.
- **Probe 3 — line-atomic print (D43): agree, and this machine was the
  prior art.** The interpreter renders a whole line and hands it to one
  `out()` call, with no yield point inside a `print`, so it was
  line-atomic by construction before D43 was ruled. Measured rather
  than asserted: eight tasks × 40 long multi-segment interpolated
  lines, 20 runs of `--native` = **6400 lines, 0 torn, across 20
  DISTINCT interleavings** (the interleavings differ every run, which
  is what proves the threads race and the probe is not measuring a
  serialization). Same program here: 3200 lines, 0 torn, one
  interleaving. Nothing to file.
- **DIV-2026-018 FILED as wolf-lang#88 — the compiler admits a bare `..`
  range that the grammar excludes.** The 29th ugly pair: `s[..]` is
  `fail(E0201)`@parse here and `exit(0)` printing `ok 5` there, on all three
  run-reaching tiers. `[gram.expr.primary]`'s
  `range_expr ::= r_end (('..'|'..=') r_end?)? | ('..'|'..=') r_end`
  has two alternatives and **neither admits a bare `..`** — the first
  needs a leading endpoint, the second a trailing one. Triage case 2,
  compiler bug, and the over-acceptance is in the **parser**, not the
  slice path: `s[..=]` also parses and answers `trap(bounds)`, while
  `s.get(..)`, `let r = ..` and `xs[..]` parse and then decline
  `unsupported`@`resolve` with no diagnostic. Class `verdict`, not a
  soundness candidate. No corpus file witnesses it, so it takes no
  `FILED_DIVERGENCES` entry and is recorded in the log until one
  exists. The counter-reading (that the grammar should gain `..`) is
  recorded with it; this machine is not moving its parser ahead of the
  ruling.
- **wolf-lang#76 CONFIRMED, unchanged.** DIV-2026-017 re-verified at
  the new pin against a counterparty rebuilt from a **deleted**
  `target/`: still `"{who}` there and `{who}` here, **byte-identical
  shas to the original filing**, still the same on `--checked`,
  `--native` and `--release`. Nothing in the wave touched the lexer's
  literal decode. Still one-sided in this machine's favour — the corpus
  header's own `stdout="{who}"` agrees with us — and the file's
  `phase: wir` pin still masks it.
- **`[gram.lex.shebang]` READ — the one clause in the wave that needed
  work here, and it broke fourteen files on arrival.** s53's new spec
  delta makes a `#!` line at byte offset 0, and no other offset, trivia.
  `grammar/shebang.lu` arriving meant the whole `grammar/` **directory
  module** failed to resolve — every one of its 14 sibling files
  answered `fail(E0101)` at the stray `#`, a 14-file corpus regression
  from one unread clause. Read in two places, because a shebang is
  trivia to the language *and* to the corpus tooling: the lexer skips it
  to end-of-line, leaving the `\n` for `[gram.lex.newline]`'s terminator
  machinery exactly as a `//` comment does; and
  `directive::parse_header` skips it so the `//!` block can start one
  line down. Both key on offset zero (`self.pos == 0` / `index == 0`),
  so a file that opens with a rejected BOM puts its `#!` at offset 3 and
  does not get one.
- **wolf-lang#71's interpreter half LANDED.** 0.1.10 deferred it on
  purpose — "it lands once wolf-lang#71's fix fixes the span to match".
  s74 landed the compiler's half, so this round landed ours.
  `lint::Walk::capture_lend` routes both lend spellings through the
  same door as assignment: the X1 moded receiver `(mut xs).push(1)` and
  the call-site argument mode `f(mut n)`, which "must never drift apart
  again". Spans are the counterparty's byte for byte — `[913,915]`,
  `[562,563]`, `[545,546]`. W1101 is deliberately NOT emitted for the
  lend spellings (its text is about a write landing on the task's own
  copy, an assignment's shape), matching both the counterparty and the
  corpus headers' `warns:` lines. This also retires the second finding
  recorded under #71: the mut-lend program printed `0` here and `2`
  there, and **neither machine runs it now** — both reject it with the
  same code and span, so the stdout divergence is unreachable.
- **The `--counterparty-tier` lane audited, independently of itself.**
  The lane that found the entire dynamic corpus going uncompared was
  re-verified by measuring both sides' `phase_reached` per entry per
  tier outside the runner — deriving the audit from the lane it audits
  would be circular. It is still comparing what it claims. It also
  showed something the 0.1.10 table did not: **the three run-reaching
  lanes are not nested.** `checked` reaches `run` on more files than
  `native` (127 vs 122) yet compares fewer (114 vs 116), because
  `checked` declines the conc tier while `native` declines most of the
  unsafe/region/shared tier. The honest coverage figure is the
  **union — 129 files**, with 101 compared by all three; one lane alone
  would miss up to 15. Of this machine's 185 run-reaching entries, **56
  are met by no counterparty lane at all** — the residue to drive down.
- **Surfaces:** `--version` prints `lupin 0.1.11 (wolf-interp, reference
  interpreter at pin f8dca42)`. Corpus walk 280 files / 0 mismatch (170
  match, 8 dynamic counterparts, 33 conservatism, 47 out-of-scope);
  bundle 317 programs / 295 records, anchors covered 107 of 316;
  differential GREEN at all four tiers, one filed divergence at the
  three run-reaching ones. Full suite 588 tests green.

## 0.1.10 — 2026-08-12

THE RUN TIER FINALLY COMPARES: the mid-end/whole-program re-pin. Pin
bumped `0b4e79c` → `613c3dc` (latest green trunk, CI run 31651577695:
the wave landed s42 (the mid-end optimizer), s43 (whole-program —
clusters, body dedup, a frozen summary index) and s63 (diagnostics
polish: 144 codes / 30 warnings, error cascades capped with
`--error-limit=N`), plus the dv01 prose pass). The corpus grows 4:
263 → 267 files (`conc/select_two_timeouts.lu`, the #64 GVN
cross-arm-dominance litmus, and the s42 kernel tier
`kernels/hot_counter.lu`, `kernels/hot_scale_versioned.lu`,
`kernels/churn_b3.lu`). All four reach `run` on this machine at first
sight and match their `check:`, so the run ledger grows 4. Coverage
ratchets 102 → 103: `[conc.select.timeout]` is covered for the first
time, by the new select litmus alone. `spec/` is UNCHANGED in this
range — anchors stay 315 — so no clause needed a new reading. Bundle
304 programs / 282 records.

- **`diff-run --counterparty-tier=default|checked|native|release`, and
  the gap it closes.** The counterparty's `conform-run` is one process
  contract over several engines, selected by flag. The runner passed
  **no flag** through 0.1.9, so the compiler stopped at `unsupported`
  @`wir` and the counterparty reached `run` on **0 of 245 entries**:
  the entire dynamic half of the corpus was ledgered as conservatism
  without ever being compared. The gap was named for `--native` at
  0.1.9 and stayed open a round; it covered `--checked` too, which
  nobody had noticed. Now measured: the counterparty reaches `run` on
  120 files at `checked`, 113 at `native`, 104 at `release`, and both
  machines *execute* 107, 107 and 98 respectively. Our own side is
  always invoked plainly — this machine has one engine.
- **THE RELEASE LANE IS COMPARABLE, AND THE MID-END CHANGED NOTHING
  OBSERVABLE.** `conform-run --release` runs s42's optimizer and s43's
  whole-program layer; compared against this machine over all 98 files
  both execute, it agrees everywhere, and the one divergence it reports
  is the same one `--checked` and `--native` report, byte for byte. A
  transformation that altered behavior would have surfaced as a
  release-only finding; there is none. Honest scope: linux x86-64, one
  platform, unseeded, and the `kernels/` tier is three programs — this
  shows that 98 observable behaviors survived optimization, not that
  the optimizer is correct. The release lane declines the conc tier by
  name (9 files), the compiler's documented posture, conservatism never
  divergence.
- **DIV-2026-017 FILED — the first finding a run-reaching lane ever
  produced.** `lints/raw_interp_braces.lu`: `r"{who}"` prints `{who}`
  here and `"{who}` on the compiler, whose raw-literal decode keeps the
  opening quote of the two-character `r"` delimiter.
  `[gram.lex.str.raw]` is explicit and the corpus header's own
  `stdout="{who}"` agrees with this machine, so triage case 2 —
  compiler bug, filed upstream. Identical on all three of its
  run-reaching tiers, which is how it is known not to be the mid-end's
  doing. The file's `phase: wir` pin was written when BOTH executors had
  the bug; it now masks a one-sided one and should advance to `run` with
  the fix.
- **wolf-interp#16 FIXED — an enum variant is a value, not a raise.**
  The silent-wrong-answer of the pass. A declared enum's variant and a
  structural error tag are both tag-shaped, and the machine built them
  as the same value, so `fn id(v: W) -> W ! {none} { v }` had its
  ordinary return read as an error: `?` propagated it and `else` fired
  on the VALUE path, so the miss always won and the two paths were
  indistinguishable to the caller. `ErrorValue` now records where its
  name resolved (`enum_variant`), sema's `Module::variants` — the same
  table the variant-pattern rule reads — decides it at the two
  construction sites, the flag rides through payload application, and
  `is_error` (the only question `?` and `else` ask) reads it. Equality,
  patterns and rendering ignore it. Confirmed against wolfgang's
  `--native` and `--release` lanes, which both print `kind num` for the
  reproducer.
- **wolf-interp#17 FIXED — the cast target is resolved, and `str` is not
  a cast source.** `s as nonsense` ran to `exit(0)` with the string
  passed through unchanged: the type expression was never resolved, so a
  typo in a cast target was invisible. Now E0301 at `resolve` spanning
  the **type name**, matching the counterparty span for span
  (`[55,63]` for `nonsense`, `[55,60]` for `bytes` — no `bytes` type
  exists in this language either). The judgement is narrow on purpose:
  only a single unqualified lower-case path that is neither a built-in
  scalar nor a name the module declares, so `2 as Meters`, `x as T` in a
  generic body and every qualified path still decline to be judged.
  `s as int` is E0805 at the whole cast expression (`[50,58]`, the
  counterparty's span); the shape sema-lite cannot classify — a `str`
  from a call return, which is the typecheck rung this machine does not
  perform — declines loudly at `run` instead of passing a string off as
  a number.
- **wolf-lang#71's premise corrected: this machine never said E0202.**
  The issue records lupin rejecting the `(mut xs).push(1)` two-task
  program with E0202. `E0202` is this machine's `E_UNEXPECTED_EOF`
  (`src/diag.rs:186`) — a *parse* code, never a capture-law verdict —
  and at this pin lupin does not reject the program at all: it runs and
  prints `0`. On the assignment spelling (`n = 1`) both machines already
  answer **E1101**, at their own rungs, which `[proto.cmp.rung]` makes
  agreement. **The proposal: E1101, on both sides** — this machine wants
  no distinct code, and the interpreter half is to treat a `(mut x)`
  receiver-lend of a captured binding as a write and emit E1101 there
  too. Deliberately not done this round: the code is corpus-pinned and
  landing a span before the compiler's would trade a missing diagnostic
  for a span divergence. A second finding recorded as this machine's
  own: closures capture by value, so the mut-lend program prints `0`
  here against wolfgang's `2` — a real stdout divergence on a program no
  corpus file witnesses.
- **The record's `commit` stamp stops going stale.** `build.rs` watched
  `.git/HEAD`, which on a branch holds `ref: refs/heads/<branch>` and
  does not change when a commit lands, so the stamp froze at whatever
  commit last forced a rebuild and records claimed a revision that had
  not produced them — the dishonesty `[proto.record.fields]` asks that
  file to prevent. It now also watches the ref HEAD points at, and
  `packed-refs`, each only when present.
- **The three s71 clauses, verified clause by clause at the new pin.**
  `[mem.str.empty]`: `count("")` is 0, `split("")` yields one piece and
  that piece is the whole string, `replace("", t)` is the identity,
  `repeat(0)` is `""` — conforming, no lane refusing, no lane trapping.
  `[mem.str.repeat]`: `repeat(-1)` is `trap(assert)`, not `bounds`. The
  `else |Tag(p)|` row-coverage rule: `rows/negative/handler_uncovered.lu`
  is `fail(E0809)`@`resolve` span `[518,523]` — the counterparty's span —
  and the payload-binding run half `rows/else_tag_payload.lu` runs with
  its exact expected bytes. All three conform; nothing needed changing.
- **Surfaces:** `--version` prints `lupin 0.1.10 (wolf-interp, reference
  interpreter at pin 613c3dc)`. Corpus walk 267 files / 0 mismatch (158
  match, 8 dynamic counterparts, 32 conservatism, 47 out-of-scope);
  bundle 304 programs / 282 records, anchors covered 103 of 315;
  differential GREEN at `default` with an empty filing list, and
  filed-green at the three run-reaching tiers. Noted, not acted on:
  upstream's own `--version` still names `lupin 0.1.8 … pin 7886559` as
  its paired reference interpreter, two releases behind.

## 0.1.9 — 2026-08-12

THE CONC TIER RUNS ON BOTH MACHINES: the c09-wave re-pin. Pin bumped
`26fa98e` → `0b4e79c` (latest green trunk: the wave landed s41
(release tier), s51 (package manager), s73 (native concurrency) and
four fmt idempotence fixes; trunk head `17ea078` is CI-red on both test
lanes and was not taken). The corpus grows 1: 262 → 263 files
(`test/conc_schedules_test.lu`, the s73 `--schedules=N` dogfood
witness); nine files' headers advance to phase `run` (8× `conc/` +
`procs.lu`; wolfgang executes concurrency natively now, and this
machine's run-ledger claims for all nine predate the wave, so both
machines now execute the tier), `memory/prov_holy_grail.lu` moves
typecheck → mem,
and two conc files gain `-> !int` rows (E0604/D30). Anchors stay 315,
ratchet stays 102; bundle 300 programs / 278 records.

- **The conc tier, machine to machine: 10 of 10 agree.** The wave's
  first comparison over a corpus tier where BOTH implementations
  execute concurrency. `wolf conform-run --native --seed=N` vs `lupin
  conform-run --seed=N` over all ten run-phase conc-tier files at seeds
  {0, 42}: every pair agrees on verdict, exit and output bytes, both
  records `"seeded": true`, a `[proto.seed.equal]` comparison, honored
  end to end. Honest scope: two seeds, linux x86-64, wolfgang's debug
  tier; the harness's own diff-run lane still invokes plain
  `conform-run` (wolfgang's native rung is opt-in) and ledgers the ten
  as counterparty-unsupported conservatism. The native comparison ran
  direct, documented in the thirteenth differential entry.
- **Thirteenth corpus differential: 241 entries, 0 divergences.**
  Counterparty built CLEAN at the pin with `libwolf_rt.a` provisioned;
  395 conservatism-ledger entries (66 rejects-beyond, 130
  run-unmatched, 152 counterparty-unsupported, 47 interp-unsupported).
  `FILED_DIVERGENCES` is empty again.
- **DIV-2026-016 RESOLVED; wolf-lang#61 closed.** The CLEAN build
  answers `fail(E0809)@typecheck`, same span `[518,523]`. The E0806
  answer does not reproduce at the release sha or this pin, agreeing
  with the issue's own reproduction attempt. `[proto.cmp.rung]`
  agreement with this machine's resolve rung; the 0.1.8 observation is
  attributed to a stale/intermediate-pin counterparty build. An
  evidence-hygiene lesson (fresh counterparty target per re-pin), not
  a semantics one.
- **E1014's trap-map row landed upstream: 0.1.8's filed nit, closed.**
  The pinned `[conf.trap.map]` exclusivity row now names "E1014's
  read-mode write barrier, D39" outright, so this machine's D39 trap
  mapping (`ledger::dynamic_meaning` E1014 → `exclusivity`) is
  document-stated rather than family-inferred. No behavior change;
  the citation argument in the comment retired.
- **Surfaces:** `--version` prints
  `lupin 0.1.9 (wolf-interp, reference interpreter at pin 0b4e79c)`.
  Corpus walk 263 files / 0 mismatch (154 match, 8 dynamic
  counterparts, 32 conservatism, 47 out-of-scope); bundle 300 programs /
  278 records, anchors covered 102 of 315; differential lane GREEN
  with an empty filing list. Open lupin issues #16/#17 re-verified at
  this pin: #16's shape now fails loud (`unsupported` at the else
  handler) but the handler still fires on the value path, still open;
  #17's unknown-`as`-target pass-through still reproduces, still open.

## 0.1.8 — 2026-08-11

The v0.1.0 RELEASE PAIRING build: this is the version wolf 0.1.0's
`--version` line names as its reference interpreter. Pin bumped
`e94b879` → `3d5cee6` (wave seven: r01 prep + s71 release polish; the
corpus grows 5: the `[mem.str.empty]`/`[mem.str.repeat]` witnesses, the
`else |Tag(p)|` pair, the ctfe fold witness; the spec grows
`[mem.str.empty]`, `[mem.str.repeat]` and §10 `[gram.version]`
(grammar/1)). Then the ONE lawful mid-pass re-pin `3d5cee6` → `26fa98e`
when s72's mode teeth merged upstream (the corpus grows 3 more: the
D39/D40/overlap fail-files `memory/read_param_write.lu` E1014,
`memory/mut_read_overlap.lu` E1002, `memory/list_mutate_while_iter.lu`
E1013; the spec grows `[mem.iter.excl]` and the trap map's E1013 row).
Net: 254 → 262 files, 306 → 315 anchors, ratchet 97 → 102. The D39/D40
mirrors below were built on the rulings' authority (wolf planning
02-decisions.md, 2026-08-12) while s72 ran concurrently; the re-pin
brought their spec text and fail-files into the pin, and all three
fail-files pair with this machine's traps as dynamic counterparts
(`ledger::dynamic_meaning` gains E1013/E1014 → `exclusivity`; the
pinned map prose names E1013 but not yet E1014, a noted upstream-worthy
nit).

- **D39's dynamic mirror: a write through a read-mode binding traps
  `exclusivity`.** Every call frame now carries its read-mode parameter
  list and `write_path` is the barrier: whole-parameter stores,
  projection writes (`p.x = 9`), compound assigns, and a mutating
  method's receiver write-back all trap with the parameter's declaration
  as the second span and a `mut`-plus-call-site-spelling teach. The kind
  is `exclusivity`, `[conf.trap.map]`'s family for mode violations
  (`[mem.tier0.excl.1]` already gives every read/write conflict that
  kind; D39 names no new one). A body local shadowing the parameter's
  name stays an ordinary local. The caller-side overlap half
  (`f(mut a, a.x)`) was verified and kept. approximation-contract §6.12.
- **D40's dynamic mirror: mutating a container while a `for` loop
  iterates it traps `exclusivity` at the mutation (S-11 RESOLVED;
  closes wolf-interp#9 as fixed).** `for x in xs` holds a read claim on
  the container's place for the loop's whole extent; push/pop/clear, an
  element write, a whole-container assignment, or a `mut` pass inside
  the body conflicts with it and traps, the message naming the loop and
  teaching D40's fix-its (collect-then-apply, or the index loop;
  wolfgang's E1013 row in `[conf.trap.map]`). The wolf-interp#9 program
  that ran `exit(0)` over the loop-entry snapshot traps now; reads
  beside the claim stay legal; the claim dies with the loop on every
  exit path. approximation-contract §6.8 rewritten to the ruled
  semantics; the S-11 filing closes in docs/divergence-log.md.
- **The s71 clause backlog.** `[mem.str.empty]`: the searching family is
  defined on an empty needle, so `count("")` is 0, `split("")` yields
  the whole string as one piece, and `replace("", t)` is the identity;
  the three `unsupported` declines die. `[mem.str.repeat]`: a negative
  repeat count is a caller contract violation, taking the deterministic
  `assert` trap and retiring the sc03-era `bounds` spelling (the 0.1.7 corpus
  mismatch on `faults/repeat_negative.lu` closes). E0809 at the resolve
  rung: an `else` handler pattern must cover the operand's whole error
  row, judged where sema-lite can see the row (a direct unshadowed call
  to a same-module fn with a declared closed row, the E1007
  discipline); `rows/negative/handler_uncovered.lu` pins it, and the
  payload-binding run half (`rows/else_tag_payload.lu`) already agreed.
- **comptime posture unchanged, verified at the new pin:** every corpus
  comptime file that calls a `comptime fn` declines loudly and by name
  (`` `expand` is a `comptime fn`; … the compiler's engine (s16) ``),
  including the new `comptime/fold_reaches_lane.lu`. The fold table is
  the compiler's; nothing here half-implements it.
- **One divergence found, filed: DIV-2026-016 / wolf-lang#61.**
  wolfgang's own `conform-run` answers `fail(E0806)@typecheck` (the
  generic refutability diagnostic) on `rows/negative/
  handler_uncovered.lu`, the file its own corpus pins `fail(E0809)`
  for; this machine matches the pin at resolve, same span. Codes
  differ, so `[proto.cmp.rung]` cannot bridge it; the counterparty is
  the defendant (both records attached to the issue). The lane is
  GREEN with the filing; the entry closes when wolfgang's conform-run
  answers its own pin.
- **Surfaces:** `--version` prints
  `lupin 0.1.8 (wolf-interp, reference interpreter at pin 26fa98e)`.
  Corpus walk 262 files / 0 mismatch (153 match, 8 dynamic
  counterparts, 32 conservatism, 47 out-of-scope); bundle 299 programs /
  277 records, anchors covered 102 of 315; differential lane GREEN, and
  the one divergence is DIV-2026-016, filed.

## 0.1.7 — 2026-08-11

The release-runway pass for r01 (wolf v0.1.0 names this version as the
reference interpreter). Pin bumped `13b811f` → `e94b879` (wave six: s40
os/time/json, s70 match tier + X3 value paths, s69 idiom lints; the
corpus grows `lints/` ×15, `os/` ×4, `json/` ×2, `faults/` ×5,
`time/` ×1 and the match-tier witnesses: 221 → 254 files; the registry
grows `[proto.cmp.rung]`, 305 → 306 anchors, ratchet 96 → 97). Two
issues closed, every open divergence family closed, the differential
compares CLEAN.

- **#21: container-element literals adopt `int`, and `List[i32]`
  checks its writes.** Both directions of the s70 reassignment. A
  literal pushed into a `List` adopts the locked 64-bit `int` (the
  `Range` precedent) instead of staying literal and defaulting `i32` at
  the pop-binding, so `push(2147483647); pop; +1` prints `2147483648`
  now, as wolfgang does. The constructor's bracket type argument
  (`List[i32]()`, read off the callee's syntax; also `List[T]`
  annotations through `coerce`) travels on the value, so pushes
  range-check at the element's width, element loads feed checked
  arithmetic at `i32`, and the compound `l[0] *= 2` traps BEFORE the
  write lands. The five `faults/overflow_*` litmuses and
  `memory/list_elem_assign.lu` all pin, both lanes.
- **#22: the explorer runs the same admission ladder as `run`.** The
  `--explore` path bypassed the E11xx statics and certified programs
  "observably deterministic" that the same binary refuses to run
  (rp01's finding; the ch13 lost-update shape). `frontend::admit` is
  now the ONE admission question (module laws, then the pin statics,
  then the raise check), asked by `observe_with`, `explore_file` and
  `explore_source` alike, so `conform-run --explore` refuses
  `store_buffer`/`chan_unsendable`/`when_nested` with the run door's
  own diagnostics (exit 2, no certificate). `--explore` also honors
  `--std-root` now.
- **`[proto.cmp.rung]` adopted; DIV-2026-011/-012/-014/-015 ALL
  CLOSE.** The ruling the last four rounds routed upstream landed in
  spec/06: fail(code+span) parity at any shared-ladder rung is
  agreement, exactly one verdict wide. `compare_deep` and
  `compare::compare` implement the clause, the eleven rung-placement
  divergences compare clean, and `FILED_DIVERGENCES` is EMPTY for the
  first time since the fourth round. En route the fail comparison
  learned to read the first **error**-severity diagnostic. A
  counterparty may interleave lints ahead of its rejection in
  `diagnostics` (`[proto.record.warn]`), and a lint's span is
  `[proto.cmp.warn]`'s surface, never the rejection's (the
  `resolve/cycle` shape at this pin).
- **The s69 idiom lints.** Ten of the eleven run here with
  counterparty-identical spans: W0310–W0315 (naming, docs, module
  shape), W0603/W0604 (rows), W1002/W1003 (mode hygiene). E0802's
  literal-precise dead-arm analysis lands with them (duplicated
  str/bool/int literals, `_` after bool's two literals, the
  `pattern_shape` degradation). W0316 (ancestor import) is
  honest-absent: its only
  witness shape needs the dotted nested-module loading this machine's
  loader does not perform.
- **The s40 tier, postured.** env and time are implemented on the
  checked-lane posture: overlay env (never the host's), empty argv
  (the stdin posture mirrored), cwd as process state, X12 monotonic
  time, and `os_exit`'s defer-skipping termination (`Signal::Exit`).
  So `os/args_cwd`, `os/env_roundtrip`, `os/exit_code` and
  `time/monotonic` run to their pinned outputs. The process trio is
  exec surface declined by design; the json kernels are the
  counterparty's reference parser, declined instead of guessed. All
  fifteen names resolve, so every refusal is "unsupported feature",
  never "unknown name".
- **Counts.** 254 corpus files, 232 entries: 149 match, 5 dynamic
  counterparts, 32 conservatism, 46 out of scope, 0 mismatches.
  diff-run at the pin (CLEAN wolfc build at `e94b879`): 232 entries,
  **0 divergences**. Bundle: 291 programs, 269 records, 97/306 anchors.
- **Surfaces.** `--version` names the pairing posture per r01 row 7:
  `lupin 0.1.7 (wolf-interp, reference interpreter at pin e94b879)`.

## 0.1.6 — 2026-08-11

The wave-four re-pin and the lint wave (issues #19, #20). Pin bumped
`f0da6e6` → `13b811f` (#41 capture law, #42 checked-lane determinism,
s34 procs, s35 io reactor, s39/s40/#40 native str/List/fs, s68 lints:
the corpus grows `lints/` ×12, `conc/` ×3, `projects/` ×3, `net/` ×2,
`comptime/` ×1, `test/` ×1: 199 → 221 files; the registry grows
`[conc.chan.default]` and `[mem.region.freeze.4]`, 303 → 305 anchors,
and the `test` namespace joins the reserved forward set). Two issues
closed, one divergence family filed, one closed.

- **#20: reads through frozen containers are legal.** The spec ruled
  (`[mem.region.freeze.4]`, appended for exactly this defect): every
  read through frozen data is an ordinary read, and a value-semantics
  machine must not count writing back an *unmodified* method receiver
  as a write. `eval_method`'s write-back is now conditional on the
  receiver actually changing, so the book's ch10 shape
  (`frozen[0].body.words()`) reads legally where 0.1.5 trapped
  `region-fault`; a genuinely mutating method through a frozen home
  still traps (`region_freeze_write.lu` unchanged).
- **#19: the s68 lint wave.** This machine now runs warning analyses:
  the eleven shared-analysis lints (W0304–W0309, W0401, W0602, W1101,
  W1102, W1302) plus the `#[allow]` self-lints W0302/W0303, in the new
  `lint` module. Every fixture span is byte-identical to the
  counterparty at the pin, `#[allow]` suppresses identically (item- and
  statement-granular), the record's `warnings` array is populated per
  `[proto.record.warn]`, and the same observations ride `diagnostics`
  at warning severity. The compiler-only four (W0402, W0601, W0801,
  W1001) stay honest-absent, written down in `lint::HONEST_ABSENT`; the
  corpus `warns:` ledgers are enforced for the implemented set. The
  same walk carries the pin's static realignments: **E1101/E1102/E1103**
  (the #41 capture law: a task's write to a captured name with `when`
  bodies exempt, a spelled `List`/`Map` channel payload, a lexically
  nested `when`) and **E0004** (`1.e5` stays an *error*, the s68
  correction) all reject at this machine's resolve rung with the
  counterparty's codes and spans, so `store_buffer`/`chan_unsendable`/
  `when_nested` leave the run ledger for the fail column, and the last
  E000x unsupported(interp) conservatism row closes. §7.8's
  `[ub.assume.noalias]` citation is confirmed `[mem.unsafe.raw.2]`,
  with W1302 as its compile-time face.
- **Realignments at the pin:** `[conc.chan.default]` adopts this
  machine's rendezvous default as normative (`channel[T]()` was already
  capacity 0 here; the clause cites the behavior, and the ctor now
  cites the clause); E0412/E0413 spans realign to the counterparty's `:spec`
  shape; the `test` anchor namespace lands (`test/assert_test.lu`
  walks); the s34 proc pair and two of the three P-project witnesses
  run (`projects/count.lu` needs the fs tier this machine declines by
  design).
- Tenth differential: 203 entries, **11 divergences, all filed, every
  one the same finding**: same code, same span, this machine at
  resolve vs wolfc at typecheck/mem (DIV-2026-011/-012/-014, plus new
  **DIV-2026-015** for the four realigned statics; one `[proto.cmp]`
  ruling closes all four families). **DIV-2026-013 closes** (wolfc's
  s38 conform-run wiring landed; `unsupported` there now, never a
  divergence). 325 conservatism entries. Bundle: 258 programs, 240
  records (the issue #20 freeze-read twin joins the suite tier);
  coverage ratchet raised 90 → 96 over 305 anchors.

## 0.1.5 — 2026-08-11

The unsafe-tier batch (issue #18, the book's ch09 differential) and the
five-lane re-pin. Pin bumped `ad6cef7` → `f0da6e6` (s32 tasks, s33
channels, s37 str core, s38 fmt/io/fs, s67 warnings: the corpus grows
`strings/` ×9, `lints/` ×4, `fs/` ×2, `io/` ×1: 183 → 199 files; the
registry grows the `diag.*` block, `[mem.str.get]` and
`[proto.record.warn]`/`[proto.cmp.warn]`, 290 → 303 anchors). One
issue closed (#18, six items), three divergences filed.

- **#18 (1): the unsafe ring is enforced.** Raw-tier operations
  outside `unsafe` blocks reject at this machine's resolve rung with
  the counterparty's code and span: C calls (E1301 at the call,
  `[384,395]` on `unsafe_raw_outside.lu`, byte-identical), raw reads
  and writes through pointer-holding locals (the place, `[452,456]`
  shape), int→pointer casts, `borrow … from`, `assume noalias`, and
  the provenance operations. Sema-lite tracks what it can *see*
  (literal-bound locals, allocator calls, the book's laundered
  `unsafe { … }` initializer) and never guesses. Rung placement vs
  wolfc's mem emission is **DIV-2026-012** (the DIV-2026-011 question).
- **#18 (2): nothing casts to `bool`.** The cast matrix's bool column
  closed: `n as bool` is E0805 at the whole cast expression, inside
  `unsafe` too (observed parity at the pin). §7/T1's one modelled
  production door closes with it: the T1 trigger/twin retired,
  `UbRow::T1` is `Coverage::Unreachable` with its reason, and the
  detection logic stays for frontend-bypassing callers. `bool as _`
  and `_ as str` reject statically where the class is visible
  (`typecheck/cast_bad.lu` now matches its pin). P1's protector-form
  suite pair retired with its `*u8` signatures; the protector
  acceptance evidence moved inline (machine-direct), and P2's
  trigger/twin rebuilt on `freeze r`, in-language, same row.
- **#18 (3): `*T` never crosses a signature.** E1302 at the parameter
  name (`[329,330]` on `unsafe_sig.lu`, byte-identical), return types
  at the type span. Also DIV-2026-012.
- **#18 (4): the C intrinsics check their arguments.** Exact arity
  for the modelled five; size/count arguments must be non-negative
  integers; `c.memset`'s byte argument no longer defaults silently, and
  every refusal names the construct.
- **#18 (5): the §7.4 format specs, to parity.** New `fmtspec`
  module: `[[fill]align][+][0][width][.precision][type]`, with zero-pad
  AFTER the sign (`{n:08}` is the flag plus width; the absorb-into-
  width reading was the filed bug), `+` with zero taking it,
  sign-magnitude bases, `e`/`E` signed two-digit exponents, str
  precision on code-point boundaries, shortest-round-trip f64 default
  (the `std.fmt.decimal.to_str` layout; floats render `3`, not `3.0`).
  Malformed specs are **E0412** and type-mismatched specs **E0413**,
  statically at the literal where sema-lite sees the hole's class;
  E0411 statically refuses `s[i]` char indexing. The three corpus
  fail-files match their pins; wolfc's conform-run at this pin cannot
  reach its own emissions there, filed as **DIV-2026-014**,
  counterparty suspected.
- **#18 (6): the fs/io posture.** No filesystem by design: the s38
  `fs_*` family and `read_line` resolve and decline with the construct
  named. `eprint`/`eprint_raw` are real: one fmt machinery, two fds,
  stderr live-gated like stdout's pass-through and never hashed;
  `io/eprint.lu` runs to its pinned stdout. wolfc's conform-run at the
  pin E0301-rejects its own s38 files, filed as **DIV-2026-013**.
- **Realignment:** the s37 str surface lands in full (`get`, the
  `[mem.str.get]` boundary primitive, oob = reversed = split-code-point
  = `none`, hits bit-identical to the checked slice; then `find`/`rfind`,
  `bytes`, `split`/`count`/`replace`, `strip_prefix`/`strip_suffix`,
  `trim_start`/`trim_end`, `ends_with`, negative `repeat` trapping
  `bounds`), `^n` end-relative endpoints resolve before the domain
  question, and `[proto.record.warn]` is wire-complete: the additive
  `warnings` array (schema, comparison per `[proto.cmp.warn]`,
  honest-absent, since this machine runs no warning analyses yet and
  says so by omission), the `warns:` corpus directive, and the `diag`
  anchor namespace. The lints tier runs warning-clean; spec/01 §9's
  bare `[diag]` heading token is exempted from the registry
  cross-check as a namespace, not a clause (routed upstream).
- Ninth differential: 181 entries, 18 members, **10 divergences, all
  filed** (011 open; 012 rung placement ×4; 013/014 the counterparty's
  conform-run surface lagging its own corpus at the pin), 289
  conservatism entries. Bundle: 235 programs, 217 records; coverage
  ratchet raised 86 → 90.

## 0.1.4 — 2026-08-10

The held maintenance wave, released once s30 shipped upstream. Pin
bumped `d147a54` → `ad6cef7` (s29+s30: the corpus grows
`memory/unsafe_c_alloc_native.lu`, `rows/eu_main_err_exit.lu`,
`typecheck/float_nan_cmp.lu` and `resolve/same_name/`: 177 → 183 files;
anchors hold at 290; the two E0410 fail-files re-pin `phase:` resolve →
parse). Four issues closed, one divergence resolved, one filed.

- **#15 (silent-wrong, ba:blocker): the X1 call-site mode law has its
  missing half.** `f(x)` where the signature demands `f(mut x)` ran to a
  wrong answer silently (the writeback never happened). The book caught
  it teaching chapter 7. E1007's static rule is now sema-lite's at the
  resolve rung, all four disagreement shapes (missing `mut`/`take`,
  extra mode, wrong word), matching wolfc's code, span, and message
  shapes, exactly as E0410 in 0.1.2; `[conf.trap.map]` gives E1007 no
  dynamic meaning, and a wrong answer is not a semantics candidate, so
  rejection is the honest stop. The dynamic residue, calls through
  function values, is refused at the call, never run wrong.
  `memory/mode_missing_mut.lu` leaves the run ledger; the book's ch07
  repro is a regression test. Rung placement vs wolfc's `mem` emission
  is **DIV-2026-011** (same code, same span; routed upstream).
- **#13: `c.calloc(n, size)` allocates `n * size` bytes.** The modelled
  C heap gave it `n`; s29's native differential (real glibc) caught the
  disagreement, the first soundness candidate it produced, and a lupin
  bug. Overflow in the size computation is `unsupported` (real calloc
  says NULL; no null surface is pinned). `malloc`/`memset`/`memcpy`
  audited correct. `unsafe_c_alloc_native.lu` runs `exit(0)`.
- **#14: integer literals consult their context.** `-9223372036854775808`
  is writable in every annotated spelling: literals stay unconstrained
  through negation and literal-only arithmetic (i128-checked), a
  declared return type types the value a call returns, and
  `[arith.literal.default]`'s i32 rule (with a range check) applies
  where the literal meets its binding. `var k = 0` remains i32, the
  rule wolfc implements, now documented (approximation-contract §6.11).
- **#11: closed after the sc04 reopen.** The 0.1.3 cast matrix holds at
  this pin: the reopening program prints `3.0` / `converts` and exits 0;
  `(3 as f64) == 3` is false. What the reopen's evidence showed the
  matrix still misses is filed separately as #17: cast target types are
  not *resolved* (`s as nonsense` no-ops).
- **DIV-2026-010 closed.** s29 moved wolfc's E0410 to the resolve rung
  (with the `[conc.when.body]` exemption this machine flagged as
  wolf-lang#21) and re-pinned the corpus directives; the eighth
  differential compares both files clean. Eighth round: 165 entries, 18
  members, **1 divergence** (DIV-2026-011, filed), 268 conservatism
  entries.
- Realignment: `float_nan_cmp.lu` (IEEE `!=` is unordered; this
  machine's f64 model already agreed, and wolf-lang#22 was the
  compiler's half), `eu_main_err_exit.lu` (`error: Boom` + exit 1, agreeing with
  the native shim bit-for-bit), `resolve/same_name/` (two `len`s stay
  distinct). Bundle: 222 programs, 204 records; coverage ratchet raised
  84 → 86.

## 0.1.3 — 2026-08-10

The rows half, and the s27/s28 catch-up. Pin bumped `a0c4564` →
`d147a54` (the corpus grows `rows/qmark_defer.lu` and
`faults/assert_msg_holds.lu`: 175 → 177 files; the spec grows
`[mem.iter.*]`, `[mem.str.*]`, `[conf.trap.assert]` and the postfix-row
type grammar: 281 → 290 anchors). Three issues closed, one divergence
resolved, one filed.

- **#12: postfix rows, all three halves.** `type ::= type '!' error_row`
  parses in **every** type position (param, `let`/`var` annotation,
  nested); a bare lowercase name at a raise site resolves against the
  enclosing function's declared return row (`return none` under
  `-> int ! {none}` raises the tag); and resolution is **eager**: sema's
  `raise_check` refuses an unresolvable tag at the resolve rung whatever
  path the input takes, so the sc02 false-certification trap
  (`unsupported` only when the raise was *hit*) is structurally closed.
  Lowercase identifiers over tag-shaped scrutinees dispatch as row-tag
  patterns when they name a module-declared row tag; `else |err|` keeps
  its binder. The acceptance: wolf-std `std.option`'s six helpers (`or`,
  `expect`, `flatten`, `to_list`, `exists`, `is_none`, the F-0002 family,
  unwritable since sc01) execute under lupin, lowercase `none` included
  (`tests/rows_option.rs`).
- **#11 (silent-wrong): numeric casts convert.** `n as f64` produced an
  int that compared equal to ints and unequal to the float it claimed to
  be. `as` between numeric types now converts in every direction:
  int→float exact, float→int truncating toward zero with an X3 range
  check (NaN/∞/out-of-range trap `overflow`), int→int narrowing
  range-checks, `wrapping[T]`/`saturating[T]` targets reduce by their
  mode, `as f32` rounds through f32 precision (the one-f64 float model,
  approximation-contract §6.9). The non-bridges refuse like wolfc's
  E0805 (no truthiness, no `int as str`). `tests/cast_matrix.rs` pins
  the matrix, both directions of every pair.
- **#10: slice-of-binding receivers.** `d[0..1].upper()` refused at
  resolve (`d["0..1"]` does not denote a place) because the range key was
  stringified into a map-key projection. A slice expression is a value,
  not a place: `place_of` refuses it, the method call falls into the
  by-value receiver path, and `binding[range].method()` runs exactly like
  `literal[range].method()` always did.
- **The s27 spec realignments.** `[mem.iter.for]`: `for` over an
  `impl Iter for T` value desugars to the clause's drive loop
  (`next(mut self) -> T ! {done}` through call-by-value-result; range-for
  unchanged). Impl-block **method dispatch** lands with it, s17
  resolution order included (inherent wins; `Speak.speak(d)` reaches the
  shadowed trait method; trait default bodies stay `unsupported`).
  `[conf.trap.assert]`: `assert` is an intrinsic, never shadowed by a
  module fn, two-arg form's message evaluated **only** on the failing
  path, rendered to stdout before the trap (the counterparty's #19
  shape, from this side). `[mem.str.order]`: the executed byte-
  lexicographic ordering is now clause-backed and witness-tested. Twelve
  more corpus entries reach the run rung than at 0.1.2 (114 of 161;
  matches 76 → 84, out-of-scope 46 → 36, 0 mismatch).
- **DIV-2026-010 re-verified: still open at this pin.** A CLEAN wolfc
  build at `d147a54` reports `fail(E0410)@typecheck` where the corpus
  pins `phase: resolve`, and the sixth round's two divergences stand
  unchanged, filed and non-gating. The fix is in flight upstream (s29's
  resolve-rung `letcheck`, landed past this pin with CI still running at
  the close of this pass);
  wolf-lang#21 carries this machine's heads-up that the new walker must
  exempt `when`-body assignments per `[conc.when.body]` (its draft
  E0410-rejects `when (a, b) { a += 10 }` on `let`-bound Mutex operands
  that `conc/when_multi.lu` and `procs.lu` pin `run(exit=0)`). The
  seventh corpus differential is GREEN-with-filing: 161 entries, 2
  divergences, both DIV-2026-010.
- Bundle: 216 programs, 200 records, 290 anchors, ratchet floor 84
  (coverage +1: `[mem.model.order]` through `qmark_defer`'s own
  `conforms:` line; the nine new s27 anchors enter the debt list:
  their behaviors are exercised in `tests/`, and lifting them into
  `conforms:`-tagged suite programs is the next bundle's work).

## 0.1.2 — 2026-08-10

The lupin maintenance pass: five filed issues, four fixed, one routed
upstream. Pin bumped `cbde620` → `a0c4564` (the corpus grows the E0410
fail-files and the unsafe/checked memory tier: 164 → 175 files).

- **#5 (silent-wrong): bare-ident match patterns dispatch.** An
  identifier that names an in-scope enum variant is a *variant pattern*
  (matching the tag spelled bare or enum-qualified, payload half
  included); a capitalized identifier over a tag-shaped scrutinee is a
  structural row-tag pattern (D30); everything else binds, including a
  capitalized name over a non-error scrutinee, which is the
  counterparty's reading too. First-arm-always is dead:
  `match Ordering.Greater { Less => 1, Equal => 2, Greater => 3 }`
  yields 3, `corpus/typecheck/match_missing.lu` moves to its honest
  `exit(1)` in the run ledger, and a match no arm of which applies is
  `unsupported`, never a wrong answer. The out-of-grammar mirror image,
  bare *dotted* path patterns (`Ordering.Less =>`), now rejects at parse
  with the counterparty's exact E0201 shape (zero-width span at the token
  after the path). Approximation-contract §6.7 records the dynamic
  approximation. Same-scope `let` shadowing also reads the *latest*
  binding now (the `rposition` repair), which is `let_shadow_var_ok.lu`'s
  pinned `exit(0)`.
- **#7 (false UB): both `ub(mem.ub)` shapes of wolf-std F-0013 were one
  defect in `Provenance::drop_frame`:** it still parsed the pre-task
  `<frame>:<path>` place-key shape (keys are `t<task>:<frame>:<path>`
  since is06) and so dropped *nothing*: a callee's parameter binding
  survived its call, and the next call reusing that frame index and
  parameter name resolved its accesses through the stale tag: a Disabled
  read (shape a, the interpolated `mut` argument) or a protected-sibling
  foreign write (shape b, the allocating read-mode call). `drop_frame`
  now takes `(task, frame)` and forgets that task's frame and deeper,
  exactly. Both filed shapes are staged as regression tests
  (`tests/std_root.rs`), the transition table is untouched, and the ub
  matrix + ok-twins stay green, with no true detection weakened.
- **#8: `let` reassignment rejects at the resolve rung.** Sema-lite
  tracks binding mutability (params and pattern bindings are the mode
  system's business; `when` bodies assign through the acquired cell;
  shadowing rebinds): plain and compound assignment to a `let`-bound
  name is **E0410** with a `var` fix-it, span on the assigned place,
  byte-identical code+span to wolfc on the pin's fail-files. The
  interpreter half of wolf-lang#2, closing the last half of the
  divergence.
- **#6: lupin has a std root.** `--std-root DIR` on `run`/`check`/
  `conform-run`, `LUPIN_STD` as the flagless spelling: `use std.X[.Y]`
  resolves `<DIR>/X[/Y]/` through the normal loader (nested paths
  included; `std/x/deque_int` ships at that depth), the path's last
  segment is the bound name, and without a root the flat
  `<package root>/<last segment>` fallback keeps mirrors and sibling
  modules working. Mirrors wolfc's s26 `--std-root`/`WOLF_STD`; the
  wolf-std rig's flat-mirror interim can retire.
- **#9, mutation during `for` iteration: routed upstream, not
  legislated.** The pinned spec says nothing about `for`'s operand
  (no move, no extent-hold, no copy), and the implementations picked
  different readings (wolfc: E1001 static move; lupin: loop-entry
  snapshot, runs). Filed as **S-11** in the divergence log with both
  behaviors and the three candidate rulings; approximation-contract §6.8
  documents the machine's snapshot semantics. Compiler half wolf-lang#15;
  wolf-std keeps the divergence visible in CI.
- Conformance bundle: 214 programs / 198 records at pin `a0c4564`;
  anchor ratchet 79 → 83; manual, export ledger and corpus-harness
  ledger updated in the same breath.

## 0.1.1 — 2026-08-10

The bs00x maintenance pass: four filed issues, three fixed, one routed
upstream.

- **#1 (top severity): the DPOR closed-frontier miss is fixed.** Two
  scheduler defects starved the explorer's backtrack sets: a send that
  committed a `select` arm consumed the selecter's registration on every
  channel of the select but recorded only the sent-on channel (the
  conflict alphabet missed the coupling; `op_select_consume` is the fix),
  and channel sends published the sender's vector clock without ticking it
  past the snapshot (spawn and mutex release already ticked), so the
  sender's later ops carried false happens-before edges. The filed
  reproducer (wolf-book ex17-1, the lost-update server) now shows both
  outcomes (`balance=50` and `balance=100`) inside a *closed* frontier,
  identical to naive DFS, and is pinned as a regression litmus
  (`tests/explore_machine.rs::the_select_coupled_lost_update_is_inside_the_closed_frontier`).
  The pinned `conc/` exploration ledger re-ran with **no count movement**
  and every verdict-stability oracle still green.
- **#2: write-after-freeze now traps on value paths.** Struct values
  carry the region charged at their allocation site
  (`Value::Struct::home`), and `write_path` refuses a write through a
  container homed in a `Frozen` region: `region-fault
  [mem.region.freeze.1]`, before anything is mutated. That is E1012's
  shape executed, agreeing with wolfc's static rejection. Reads and rebinding
  stay legal (`tests/faults/region_freeze_value_write.lu` + its
  `region_freeze_rebind_ok.lu` twin; approximation-contract §6.1 records
  the remaining list/map gap). `memory/region_freeze_write.lu` moves from
  `exit(7)` to `trap(region-fault)` in the run ledger.
- **#3: `when`-arity code aligned to wolfc.** The malformed-`when`
  sentence now carries **E0201** (the established generic expected-token
  assignment) instead of the invented E0203, which collides with wolfc's
  toplevel-decl family. Message and `[gram.expr.conc]` anchor unchanged;
  `diag::UNPINNED_CODES` (the published choices table) updated.
- **#4, cross-task capture-by-copy: routed upstream, not patched.**
  spec/03 makes the mut-capture shape a compile error (E1101) and states
  no runtime meaning; inventing a spawn-time trap would be legislating.
  Capture-by-copy stays the documented interpreter semantics
  (approximation-contract §10.2, corpus exemplar `conc/store_buffer.lu`),
  and the gap is filed as **S-10** in the divergence log for a spec ruling
  (the E1004/E1005 precedent).
- Conformance bundle: 203 programs / 187 records (the two new freeze
  litmuses); manual and export ledger updated in the same breath.

## 0.1.0 — 2026-08-10

The binary is named `lupin`; the package and repository stay `wolf-interp`.

- `lupin FILE.lu` runs a file; `lupin -` reads a program from stdin; bare
  `lupin` opens the REPL. The exit code reports the program: its own
  `exit(N)`, 2 on a static-phase rejection, 3 on a trap or UB finding, 4 on
  `unsupported`.
- `run` is the explicit subcommand spelling and carries `--seed`,
  `--schedule` and `--json` (the spec/06 observation record at the front
  door). A subcommand name wins over a file of the same name.
- `lupin eval 'CODE'` (short spelling `-e`) evaluates a snippet in a fresh
  session and prints what the REPL would print.
- `lupin check FILE...` runs the frontend only (lex, parse, resolve).
- Observation records and bundle manifests report `"impl": "lupin"` with
  `impl_version` 0.1.0; `--version` prints the version, the package, and
  the upstream pin.
