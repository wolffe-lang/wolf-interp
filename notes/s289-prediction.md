# s289 prediction: lupin's mirror of the diagnostics sweep's first change

The mirror's prediction was made on the compiler's side before this repository's first change:
wolf-lang's s289 prediction (`878f23b3`) and the first change's note, section "For the lupin
mirror" (`67bd054f`). This copy is this repository's record of it. It was written after the first
take of the check, and says where that take moved it (the last section). Base: wolf-interp trunk
`725d86da`; lupin 0.1.60 is the
release the compiler pairs with, and nothing between it and the base touches what follows. The
compiler's side is wolf-lang's s289 branch at `67bd054f` (wolf-lang#1085, #1083).

## What the base does

A capitalized name that nothing declares is read as an error tag, and this machine reads it as a
tag value wherever it stands (`eval::eval_path_expr`'s posture, load-bearing across the corpus).
Eight of the compiler's new corpus rows run here and exit 1, printing `error: OPEN`:

| row | the line | compiler | lupin 0.1.60 |
|---|---|---|---|
| `tag_undeclared_tail` | `OPEN` as the tail of `-> int` | E0301 at `OPEN` | exit 1 |
| `tag_undeclared_assign` | `d.kind = INPUT` | E0301 | exit 1 |
| `tag_undeclared_return` | `return CANCEL` in `-> int` | E0301 | exit 1 |
| `tag_undeclared_let` | `let x = OPEN` | E0301 | exit 1 |
| `tag_undeclared_payload` | `Bad(3)` as the tail of `-> int` | E0301 at `Bad` | exit 1 |
| `tag_undeclared_fallible_let` | `let x = Bad` in `-> !int` | E0301 | exit 1 |
| `tag_undeclared_module_item` | bare `OPEN` beside `use dialog` | E0301 | exit 0 |
| `tag_known_cannot_fail` | `return bad` in `-> int`, `bad` spelled in a `let`'s row | E0604 | unsupported ("`bad` does not resolve") |

The compiler's other four new rows (an unimported module named `worker` or `helper`, an undefined
stand-in called or used as a receiver) are "unsupported" here, which is no divergence; this
change leaves them.

## The change, predicted

A resolve-rung check, after the variant check, over every bare name the walk meets in an
expression:
- a capitalized one;
- or one its function spells in some row.

Each name is recorded with its position.

- **A raise:** the operand of `return`, or the function's tail through a block's, an `if`'s or a
  `match` arm's tail.
- **Lenient:** a call argument, an annotated `let`'s initializer, a closure's body. A row this
  machine cannot see may take a tag there, so the check says nothing.

A name the file binds, an item, an import, a module, an ambient name, a variant, an error-set alias,
or a generic parameter of the function resolves and is left alone. Of the rest:
- a tag the function spells, in a function that cannot fail, is **E0604** at the name;
- any other name is **E0301** at the name, unless it is a raise in a function that can fail (a
  raise, left to the machine).

Every row above then answers the compiler's code at the compiler's span (a new test,
`tests/diagnostics_sweep_s289.rs`, the rows and the compiler's records copied byte for byte, red at
the rows' commit). The corpus harness, the s288 mirror and the library's tests stay green. The
vendored corpus at the conformance pin holds no such name outside a row, since the compiler, which
reports the same thing, runs it clean.

**Risk:** the check over-approximates "resolves" per file, as the variant check does, so it errs
toward silence.

## What the first take moved

The first take (`6e5220a`) held the eight rows and the s288 mirror, and reddened one library test,
`a_container_type_argument_in_scope_resolves`. It read the `T` of `List[T]()` inside an `impl[T]`
method as an undeclared tag: the generic was the `impl`'s, not the method's, and a bracket's
argument is a type there. The fix (`ada66b3`) reads an `impl`'s or `trait`'s generics as names
in its members, and a bracket's argument as lenient. A capitalized constant used as an index
(`xs[MAX]`, `MAX` declared nowhere) is therefore the compiler's E0301 and silent here, on the
side of saying nothing.
