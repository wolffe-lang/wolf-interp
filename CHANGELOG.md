# Changelog

## 0.1.1 — 2026-08-10

The bs00x maintenance pass: four filed issues, three fixed, one routed
upstream.

- **#1 (top severity) — the DPOR closed-frontier miss is fixed.** Two
  scheduler defects starved the explorer's backtrack sets: a send that
  committed a `select` arm consumed the selecter's registration on every
  channel of the select but recorded only the sent-on channel (the
  conflict alphabet missed the coupling — `op_select_consume` is the fix),
  and channel sends published the sender's vector clock without ticking it
  past the snapshot (spawn and mutex release already ticked), so the
  sender's later ops carried false happens-before edges. The filed
  reproducer (wolf-book ex17-1, the lost-update server) now shows both
  outcomes — `balance=50` and `balance=100` — inside a *closed* frontier,
  identical to naive DFS, and is pinned as a regression litmus
  (`tests/explore_machine.rs::the_select_coupled_lost_update_is_inside_the_closed_frontier`).
  The pinned `conc/` exploration ledger re-ran with **no count movement**
  and every verdict-stability oracle still green.
- **#2 — write-after-freeze now traps on value paths.** Struct values
  carry the region charged at their allocation site
  (`Value::Struct::home`), and `write_path` refuses a write through a
  container homed in a `Frozen` region: `region-fault
  [mem.region.freeze.1]`, before anything is mutated — E1012's shape
  executed, agreeing with wolfc's static rejection. Reads and rebinding
  stay legal (`tests/faults/region_freeze_value_write.lu` + its
  `region_freeze_rebind_ok.lu` twin; approximation-contract §6.1 records
  the remaining list/map gap). `memory/region_freeze_write.lu` moves from
  `exit(7)` to `trap(region-fault)` in the run ledger.
- **#3 — `when`-arity code aligned to wolfc.** The malformed-`when`
  sentence now carries **E0201** (the established generic expected-token
  assignment) instead of the invented E0203, which collides with wolfc's
  toplevel-decl family. Message and `[gram.expr.conc]` anchor unchanged;
  `diag::UNPINNED_CODES` (the published choices table) updated.
- **#4 — cross-task capture-by-copy: routed upstream, not patched.**
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
