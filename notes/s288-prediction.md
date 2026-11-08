# s288 prediction (lupin's mirror): the front-end rulings of wolf-lang#1079 and #1080

Committed before the first change. Base: trunk `1ed91d1` (the upstream pin `5f1f1c94`). The
compiler's changes merged at wolf-lang trunk `d59086f7` (#1079) and `a57276f2` (#1080). Their
rows, 50 corpus files at `a57276f2`, are copied byte for byte under
`tests/front_end_rulings_s288/<row>/main.lu` (sha256 read on both sides), each beside
`compiler.json`, the compiler's record of it: `wolf conform-run --json --checked` from a debug
build of `a57276f2` (sha256 `a9a07c70…0d6c`), its `file` written as the corpus path. `--native`
and `--release` gave the same verdict and the same diagnostics on all 50.

The clean room is s285's: the spec at `a57276f2`, `docs/diagnostics.md` there, the compiler's
corpus rows, records and rendered diagnostics as black-box answers, and lupin's own source. No
compiler crate is read.

## What lupin does at the base

lupin 0.1.60 (the release archive, sha256 `f3cae16f…6bb8`, equal to the release page) answers the
rows as the two PR bodies say: it runs 36 of the 37 refusal rows (E0410 rows `exit(9)`, `exit(4)`,
`exit(12)`, `exit(5)`, `exit(5)`, `exit(9)`, `exit(6)`, `exit(3)`; E0302 `exit(3)`, `exit(5)`,
`exit(0)`, `exit(0)`; E0827 `exit(2)` or `exit(5)`; E0828 left `exit(1)`, E0401 `exit(1)`; E0601
`exit(7)`, `exit(1)`, `exit(1)`, `exit(0)`; E0609 `exit(1)` twice; E0404 `exit(1)`, `exit(7)`,
`exit(5)`, and `exit(0)` for the unused, generic and error-union shapes), and calls the 37th,
`or_pattern_binds_missing`, `unsupported` at resolve ("`x` does not resolve"). It answers the 13
controls as the compiler does, except the three agreeing or-pattern rows, which the compiled
machines refuse at `mem` by their ledger and lupin runs (`exit(3)`, `exit(3)`, `exit(21)`). It
already gives E0610 at the compiler's span on the three alias-loop rows.

Trunk's code is 0.1.60's for every construct these rows use (s285 changed literal patterns only),
so trunk answers the same.

## Where each code lives here, and its span

lupin has one static rung, resolve (sema-lite), and `[proto.cmp.rung]` makes a resolve-rung
emission of a checker code agreement when code and span match. Every code below is reported
there. Each pass reports every finding of its own code, as the compiler does (measured: an enum
with three `A`s is two E0302s, `{x, x, x}` two E0601s, `(x, x, x)` two E0827s, three alternatives
lacking a name two E0828s).

1. **E0410** (`[gram.pat.let]`, `[gram.pat.nullary.scope]`): `sema::reassign_check`, the existing
   lexical walk. A name a `match` arm, a `for` header, an `else |…|` handler, a `select` receive
   or an `@` binds is declared as a pattern binding, not assignable; a bare arm name that tests
   (below) declares nothing, so the arm's body sees the `let` outside. The span is the
   assignment's place, as for today's E0410: the bare name, or the dotted field path (`Done.a`,
   `p.a`). Compound assignment is the same statement shape. Function parameters stay the mode
   system's (`mut`, `take`), and a closure's parameters stay assignable (wolf-lang#1076 is open).
2. **Test or binding**: a bare arm name tests when this machine's evaluator reads it as a test:
   it names a variant of an enum the module declares (`Module::variants`), or it names a tag of
   the row the scrutinee is statically known to carry (`rowmatch`'s reader: a call to a declared
   fn, a local bound by one, a `Map` index). Everything else binds. One rule for the evaluator and
   the checks, so they cannot disagree.
3. **E0302** (`[gram.item.type.names]`): a new member-name pass beside `sema::dup_check`, over every
   `struct`, `enum` and `type … = struct|enum` the module declares, used or not. The span is the
   later declaration's name; the message names the first.
4. **E0827** (`[gram.pat.names]`): a new pattern-names pass in `rowmatch`'s body walk, which
   already visits every binding position with the scrutinee's static row in hand: `let`/`var`
   binders, arms, `for` headers, `else |…|` handlers, `select` receives, and function and closure
   parameter lists. The span is the later binding's name (a binder, a struct field's shorthand, an
   `@` name, a parameter). An or-pattern's alternatives are each one pattern, and the or-pattern
   binds each of its names once in the pattern around it.
5. **E0828 and E0401** (`[gram.pat.or]`): the same pass. E0828 at each alternative that lacks a
   name another binds (the alternative's span), naming the name. E0401 at the later binding of a
   name whose declared types differ, where both are known from a declaration: a variant's
   payload position, a struct field. A type the pass cannot read (a generic parameter, a value
   with no declared shape) is never judged.
6. **E0601** (`[gram.type.row.dup]`): `sema::expand_error_aliases`, at load, into the module's
   alias diagnostics (`error_alias_check` reports them). Every row a module spells is read, with or
   without an alias, and every alias's own row, used or not. The span is the repeated spelling's
   tag name (measured: `{Io, Io(str)}` is E0601 at the second `Io`, without its payload). Alias
   entries are not spellings of the row: `{A, A}` runs (measured).
7. **E0609** (`[type.err.alias.union]`): the same rewrite, where an alias's tags meet the row's
   own: at the row's own spelling, with its payload, when one side is the row's own, in either
   order; at the later alias entry when two aliases disagree; and the same inside an alias's own
   declaration (measured: `error C = {A, B}` at `B`, `error C = {A, Bad(str)}` at `Bad(str)`).
8. **E0404** (`[type.rec]`): a new by-value cycle pass over the module's type declarations, in
   declaration order, used or not. Inline: a struct's fields, an enum's payloads, a tuple's
   elements, an error union's ok type and tag payloads, a `distinct` type's base, `wrapping[T]`,
   an alias (transparent: `type Pair = (int, N)` names no step of the cycle, measured). Not
   inline: `List`, `Map`, `Pool`, `handle`, `weak`, `shared`, raw pointers, channels, mutexes,
   function types, and any path this pass cannot resolve. A generic declaration is walked at its
   arguments. One report per cycle, at the member (field or variant name) through which the
   cycle's first-declared type reaches itself, naming the cycle by its nominal types
   (`L -> Box -> L`, `N -> M -> N` through `distinct`). The walk stops at 32 inline levels, so a
   recursion that only grows its arguments is not reported, as the clause says.

The anchors: the clauses above are not in the pinned spec (`5f1f1c94`), so each diagnostic cites
the nearest pinned anchor (`gram.item.let`, `gram.item.type`, `gram.pat`, `gram.type.row`,
`type.err.alias.union`) and names the new clause in its words, as s285 did for `[gram.pat.lit]`.

## Rows, red first

`tests/front_end_rulings_s288.rs`, one test per row of the compiler's four-machine test
(`crates/wolf_driver/tests/front_end_rulings_lanes.rs`), by the same names. A refusal row asserts
the verdict, the first error, and the record's diagnostics against `compiler.json`: the same
`{code, span, severity}` entries, warnings included, compared once both are sorted by span. This
machine writes the error first and the warnings after it, and the compiler writes them in span
order (`[proto.cmp.phase]` compares the first error only); the rows hold the set, byte for byte,
and the order is R-s288-2. A control asserts this machine's run.

At the rows' commit (trunk's code): the 37 refusal rows, in 36 tests (one test holds `{x, x}` and
`{Io(str), Io}`), fail by name, each at the verdict (a run, or `unsupported` for
`or_pattern_binds_missing`), and the 13 control rows, in 11 tests, pass. At the fix: all 47 tests
pass.

## What must move, and what must not

- **The vendored corpus:** two files move. `rows/negative/dup_tags.lu` goes from `exit(7)` to
  `fail(E0601)`, which its `check:` pins, so it leaves the run ledger as a static refusal.
  `memory/elem_loop_induction_written_mut.lu` writes its `for` header's `i`, which is now E0410
  where its `check:` pins E1002; its vendored copy takes wolf-lang `a57276f2`'s copy byte for byte
  (`var i = i`), and then traps `exclusivity` as before. Nothing else moves: of the vendored
  files, only these two and `grammar/match_arm_or_over_product.lu` (a `conforms:` line; its
  alternatives bind `n` at `int` both) differ from `a57276f2`'s corpus, and the compiler's census
  at `a57276f2` moved no other corpus file. On the compiled tiers `dup_tags.lu` was always
  E0601, and the witness moved the same way (E1002 holds through the copy).
- **lupin's own tests:** none is expected to move. A test program that writes a pattern's
  binding, repeats a name in a pattern or a parameter list, or holds a type inside itself by value
  would be one; any found is a miss of this prediction and is named.
- **The dynamic machine:** unchanged. Every new refusal is static, before anything runs.
- **The plant:** one hunk drops the pattern-binding half of the E0410 walk; it is red by name at
  the seven `[gram.pat.let]` rows, and the nullary-arm rows and every other row stay green.
