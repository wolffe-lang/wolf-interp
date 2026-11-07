# s285 prediction (lupin's mirror): a literal pattern outside its scrutinee's type

Committed before the first change. Base: trunk `17bd522` (the upstream pin `5f1f1c94`, wolf
0.2.38's conformance pin). The compiler's change is wolf-lang#1071 (branch `s285b`): a literal
pattern its scrutinee's type cannot hold is E0415 at the literal on the checked machine, native
and release; a signed literal pattern is E0207 "a literal pattern has no sign"; its clause is
`[gram.pat.lit]` there.

## What this machine does at the base

Measured with the published lupin 0.1.60 on the compiler's seven rows (wolf-lang
`corpus/typecheck/numlit_pattern_*.lu`, `grammar/match_pattern_signed_literal.lu`,
`rows/match_literal_pattern_edges.lu`):
- the five E0415 rows run and never match: the arm's literal is compared as a number with the
  scrutinee's value, whatever the scrutinee's type (`(Value::Int(a, _), Value::Int(b, _)) => a ==
  b` in `match_pattern`), so `300` over a `u8` is a test that is never true;
- the signed row runs: the parser reads `-1` in pattern position as the negated literal
  (`parse_literal_only`, at a pattern's start and at a range's high end), and `-1` over an `i8`
  holding -1 matches;
- the edge row (every primitive's maximum and zero; `u64` above `i64::MAX`) runs correctly.

## The rule here (R-s285-8 = B, decided)

This machine has no checker, so it answers E0415 as it answers every other E0415: by the dynamic
counterpart, the overflow trap, at run (`[type.numlit.value]` calls lupin's trap its answer for a
binding's literal). Predicted:

1. **The arm traps.** When `match` tests an integer literal pattern, or a range's end, against an
   integer value whose type cannot hold the literal, the run traps `overflow` at the literal's span
   (`x-trap-span`), naming the scrutinee's type, the clause and the compiler's code. The value's
   type is the one the machine already carries (`IntTy`); an unconstrained literal reads as its
   `i32` default (`[arith.literal.default]`), as the compiler's defaulting does. A `wrapping[T]`
   scrutinee ranges at `T`. A literal that fits tests exactly as before.
2. **The first trap is the compiler's first E0415.** A program stops at its first trap, so each
   E0415 row traps at the span of the compiler's first E0415 there: `numlit_pattern_wraps` at
   `537..540`, `numlit_pattern_edges` at `702..705`, `numlit_pattern_positions` at `714..717`,
   `numlit_pattern_range` at `749..752`, `numlit_pattern_default` at `627..637`.
3. **A signed literal pattern is E0207, static, at the compiler's span and in its words.** The
   parser refuses `-` and a numeric literal in pattern position, at a pattern's start and at a
   range's high end: "a literal pattern has no sign", spanning the signed literal, with the guard
   spelling (`n if n == -1`) as its help. This machine reports the first diagnostic
   (`[proto.record.first]`), so the signed row is `fail(E0207)` at `607..609`, the compiler's first.
4. **The edge row is unchanged:** `exit(0)` and the compiler's stdout.

## Rows, red first

`tests/literal_pattern_s285/<shape>/main.lu`, each the compiler's corpus row byte for byte, and
`tests/literal_pattern_s285.rs`. At the rows' commit (trunk's code): the five E0415 shapes fail by
name (`exit(…)` where `trap(overflow)` is due), the signed shape fails (`exit(2)` where
`fail(E0207)` is due), the edge shape passes. At the fix: all seven pass.

## What must not move

No vendored corpus file writes a literal pattern outside its type or a signed literal pattern
(the compiler lane's census of this tree, and a search of it), so the corpus ledger, the
divergence and conformance gates, and every other test are unchanged. The plant drops the arm's
check; it is red by name at the #1058 shape.
