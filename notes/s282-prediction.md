# s282 — lupin's mirror of the binder warning (W0801) and the select-arm refusal (E0806): prediction, before the first change

Written 2026-11-06, before any change to this tree. The rule itself is predicted in the
compiler's note, `notes/s282-prediction.md` in wolf-lang (branch `s282`, cut at trunk
`ed19904c`); this note says what lupin does with it. Branch `s282` here is cut at trunk
`f8c2ff7`.

## Base (lupin 0.1.59, the release archive; this trunk answers the same for these shapes)
- Every binder position (`let`, `var`, comma groups, nested tuples, struct fields, `for`)
  binds a capitalized name that spells a variant, as the compiler does, and no warning is
  reported: W0801 is honest-absent here (`lint::HONEST_ABSENT`, "case tables").
- `pick(0) else |Drink| 7` refuses at run: "the `else |pat|` pattern did not match the error
  value". The handler's name goes through the match-arm resolution rule, which reads an
  in-scope variant as a variant test. The compiler binds it (a handler binder is irrefutable).
- `Drink from ch` over `channel[Kind]` refuses at run: "the committed `select` arm's pattern
  did not match the received value".

## Predicted
1. **W0801 in the warning walk** (`src/lint.rs`), at the same positions, names and spans as the
   compiler: a statement `let`/`var` binder (each of a comma group), a `for` header and the
   `else |…|` handler's binding form, at any depth through tuples, struct fields and both sides
   of `@`; a name that begins with an uppercase letter and spells a variant of an enum declared
   in the module (`Module::variants`) or of an enum the file imports by item (`use m.Kind`,
   resolved through the importing file's own `use` items). The span is the name's own token.
   Parameters, closure parameters, item-level bindings, `match` arms and lowercase names stay
   silent. The std tree is not exempt, matching the compiler.
2. **The handler binds**: `else |Drink| …`, with `Drink` an in-scope variant, binds the error
   value (as the compiler does) instead of testing it. Every other handler shape is unchanged.
3. **E0806 for a `select` arm**: `Drink from ch`, by the same rule, is a static refusal at the
   resolve rung, at the name's span, naming `Kind.Drink` and the lowercase fix.
4. **The ledger.** W0801 joins the codes this machine runs for its binder half only. The corpus
   ledger test enforces it in one direction: a file whose `warns:` lacks W0801 must not warn
   W0801 here. The match-arm half (a capitalized arm name over a scrutinee with no cases, the
   one corpus row `lints/binder_capitalized.lu`) needs case tables, which this machine does not
   have, so a ledger W0801 with no binder behind it is not a disagreement. The exact spans are
   the mirror rows' job.

## Rows, red first at this trunk
- `tests/binder_variant_s282.rs` with fixtures under `tests/binder_variant_s282/`, the
  compiler's binder rows in miniature: each fixture's `conform-run --json` record carries
  W0801 at the compiler's spans (red at trunk: no W0801), and its verdict and stdout; the
  handler row runs (red at trunk: unsupported); the `select` row is `fail(E0806)` at the name
  (red at trunk: it runs to the run-time refusal); the controls carry no W0801 (green at trunk
  and after).
- The vendored corpus at this pin: no file gains a W0801 (none spells a variant in a binder),
  and the ledger test stays green.
- The plant, `s282-plant-*` (local, one commit, one file, one hunk): the `for` header's check
  dropped, red by name at the `for` row.
