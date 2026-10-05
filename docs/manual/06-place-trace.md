# 6 — The place trace

`--trace-places` shows what ownership did, statement by statement. After
every statement of every function activation, and after a function body's
tail expression, lupin writes one JSON line naming the state of every place
in the executing frame: each binding and, below it, every struct field,
every tuple element, and the list elements and map values the program
addressed as places.

The trace is a side channel. Bare, the lines go to stderr; with
`--trace-places=PATH` they go to PATH. They never reach stdout, and the
program's stdout, exit code and trap are the same with the flag on and off
(`ci/place-trace-identity.sh` proves it over the whole vendored corpus on
every CI host). `run` takes the flag too; `--json` refuses it.

The states are read from the slot states the machine itself checks before
it traps `use-after-move` (`[mem.tier0.move.2]`), so the trace and the trap
cannot disagree about a place.

## An example

`examples/pack.lu` gives a field away with `take`, stores a new value into
it, and then reads it into a new binding:

```text
fn adopt(take w: str) -> str { w }
…
	var p = Pack { lead: "ada", tail: "grace" }
	let a = adopt(take p.lead)
	p.lead = "lin"
	let c = p.lead
```

`take p.lead` moves the field, the store re-initializes it
(`[mem.tier0.move.4]`), and `let c = p.lead` *copies* it, because `str` is
`Copy` (`[mem.tier0.move.3]`), so `p.lead` stays live:

```console
$ lupin --trace-places examples/pack.lu
ada lin grace
lin
{"trace":1,"task":0,"fn":"main","depth":1,"at":"9:2","places":[{"path":"p","state":"live","type":"Pack"},{"path":"p.lead","state":"live","type":"str","value":"ada"},{"path":"p.tail","state":"live","type":"str","value":"grace"}],"events":[]}
{"trace":1,"task":0,"fn":"adopt","depth":2,"at":"6:32","tail":true,"places":[{"path":"w","state":"live","type":"str","value":"ada"}],"events":[]}
{"trace":1,"task":0,"fn":"main","depth":1,"at":"10:2","places":[{"path":"p","state":"live","type":"Pack"},{"path":"p.lead","state":"moved","type":"str","by":"take","at":"10:16"},{"path":"p.tail","state":"live","type":"str","value":"grace"},{"path":"a","state":"live","type":"str","value":"ada"}],"events":[{"ev":"move","path":"p.lead","by":"take","at":"10:16"}]}
{"trace":1,"task":0,"fn":"main","depth":1,"at":"11:2","places":[{"path":"p","state":"live","type":"Pack"},{"path":"p.lead","state":"live","type":"str","value":"lin","reinit":"11:2"},{"path":"p.tail","state":"live","type":"str","value":"grace"},{"path":"a","state":"live","type":"str","value":"ada"}],"events":[{"ev":"reinit","path":"p.lead","at":"11:2"}]}
{"trace":1,"task":0,"fn":"main","depth":1,"at":"12:2","places":[{"path":"p","state":"live","type":"Pack"},{"path":"p.lead","state":"live","type":"str","value":"lin","reinit":"11:2"},{"path":"p.tail","state":"live","type":"str","value":"grace"},{"path":"a","state":"live","type":"str","value":"ada"},{"path":"c","state":"live","type":"str","value":"lin"}],"events":[{"ev":"copy","path":"p.lead","by":"plain","at":"12:10","to":"c"}]}
{"trace":1,"task":0,"fn":"main","depth":1,"at":"13:2","places":[{"path":"p","state":"live","type":"Pack"},{"path":"p.lead","state":"live","type":"str","value":"lin","reinit":"11:2"},{"path":"p.tail","state":"live","type":"str","value":"grace"},{"path":"a","state":"live","type":"str","value":"ada"},{"path":"c","state":"live","type":"str","value":"lin"}],"events":[]}
{"trace":1,"task":0,"fn":"main","depth":1,"at":"14:2","places":[{"path":"p","state":"live","type":"Pack"},{"path":"p.lead","state":"live","type":"str","value":"lin","reinit":"11:2"},{"path":"p.tail","state":"live","type":"str","value":"grace"},{"path":"a","state":"live","type":"str","value":"ada"},{"path":"c","state":"live","type":"str","value":"lin"}],"events":[]}
{"trace":1,"task":0,"fn":"main","depth":1,"at":"17:2","tail":true,"places":[{"path":"p","state":"live","type":"Pack"},{"path":"p.lead","state":"live","type":"str","value":"lin","reinit":"11:2"},{"path":"p.tail","state":"live","type":"str","value":"grace"},{"path":"a","state":"live","type":"str","value":"ada"},{"path":"c","state":"live","type":"str","value":"lin"}],"events":[]}
```

## The schema, version 1

Each line is one JSON object. Positions are `"line:col"`, 1-based, with
columns counted in characters, the same spelling a trap uses
(`[conf.trap.render]`).

| line key | meaning |
|---|---|
| `trace` | the schema version, `1` |
| `task` | the task the statement ran on (`0` is `main`'s) |
| `fn` | the function the activation runs (`<closure>`, `<task>` otherwise) |
| `depth` | the activation's place on its task's stack (`main` is `1`) |
| `at` | where the statement (or tail) starts |
| `tail` | present and `true` when the line follows a function body's tail expression |
| `trap` | present when the statement trapped: the trap kind |
| `places` | the frame's places, in declaration order, each binding followed depth-first by its parts |
| `events` | what this statement did to those places, in order |

A shadowed binding is not listed; the binding a read would find is.
Item-level bindings are not in any frame and are not listed.

| place key | meaning |
|---|---|
| `path` | the place, as the place model names it: `p.lead`, `t[0]`, `xs[2]`, `m["k"]` |
| `state` | `live`, `moved`, or `uninit` |
| `type` | the value's type word: a scalar's type, `str`, a struct's name, `List`, `Map`, `tuple`, or a granule's kind (`region`, `Pool`, `handle`, `shared`, `weak`) |
| `value` | live scalars, `bool`, `char`, `byte` and `str` only: the value as `{x}` prints it |
| `len` | on a `List` or `Map`: how many elements it holds |
| `elided` | on a `List` or `Map`: how many addressed elements the line leaves out (more than sixteen) |
| `reinit` | on a live place: written again after a move, and where |
| `by` | on a moved place: `take`, `move`, `plain` (a non-`Copy` value initialized or stored elsewhere), `match`, `freeze`, or `mut` (a callee moved a `mut` argument out) |
| `at` | on a moved place, the move; on an uninit place, the move of the place it belongs to |
| `of` | on an uninit place: the enclosing place that moved, taking this one with it |

| event key | meaning |
|---|---|
| `ev` | `move`, `reinit`, or `copy` |
| `path` | the place |
| `by` | for `move`, as above; for `copy`, `plain` (a `Copy` value read where a move would be) or `copy` (`copy x`) |
| `at` | where it happened |
| `to` | the binding the value went to, when it was a `let`'s whole initializer |

A list or map lists only the elements the program addressed as places — an
element moved, copied into a binding, stored, or passed `mut` — lowest
index (or key) first and at most sixteen, so a long list stays a few
entries. Moving an element addresses it, so a moved element is listed
unless sixteen lower ones were addressed before it. An element read inside
an expression (`total + xs[i]`) reads the list, not the element's place,
and is not listed. Events report only the
executing frame's own places; a callee's appear on the callee's lines.
