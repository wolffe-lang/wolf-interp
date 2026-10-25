# is81 — the variadic mirror

Crawl 54, 2026-10-25. Contract: planning repo
`sprints/interpreter/is81-the-variadic-mirror.md`. Branch `is81`, base
`a1a394d`. Earlier `docs/is81-evidence/`, `ci/is81/` and `tests/mirror_is81.rs`
belong to the completed outer-write pass; this pass preserves them and uses
`ci/is81-variadic/` and `tests/variadic_is81.rs`.

Footprint: cross-cutting

## 1. Forbidden

No compiler implementation source, shared implementation or test machinery.
Only the specified `lower_corpus.rs` regression is read as an input contract.
No C variadic execution, unsafe Rust, dependency, installation, merge, label,
issue write or planning-repo write. No attribution trailers. On nomad, only
this lane directory is writable; each heavy command checks vm_stat for 8 GiB
free+inactive+speculative, uses CARGO_INCREMENTAL=0 and cargo -j4, and ends
its processes. Linux/Windows evidence is CI's. No CI wait beyond one initial
run-state read per push; the orchestrator watches the final run.

## 2. Inputs, verified

- wolf-interp trunk `a1a394d`, crate 0.1.56; previous vendored spec pin
  `294d626dd596122285d5df954762e3aee3e5db71`, not interpreter identity (#280).
- New snapshot: wolf-lang `a5378e3fd5b6347f4a89a7d77dbe43aa576fee76`.
  Clause delta `fc906408..a5378e3f -- spec/`; ABI C variadic clause,
  grammar, prelude and ten new anchors. s263's differ-interpretation record.
- wolf-interp #300 and #280; #289 already closed for all six region rows.
- Installed Rust 1.97.1, rustfmt and clippy; no installation authorized.

## 3. Prediction

Written before the first implementation or test change: ellipsis currently
fails parsing as E0201; VaList and list builtins are missing. The new parser
will admit the final ellipsis only on C membranes, preserve the compiler's
E0822 position failure, and checked calls/builtins will refuse by name with
no native execution. Re-vendoring will add the ten variadic anchors without
losing any prior key. The six SOUNDNESS rows already belong to fixed #289;
the root-death exact-nonzero discrepancy needs separate comparator triage.

## 4. Evidence index

Pending: local red/green tests, anchor key-set differences both ways, named
checked diagnostic records, triage, CI plant and final run, audit last line.

## 5. Done-when

- [ ] Snapshot/pin a5378e3f, syntax/prelude and named checked refusals.
- [ ] Corpus phase agreement, anchor completeness and one dropped-anchor CI plant.
- [ ] Seven differ items mapped to existing issues or proposed new issues.
- [ ] README read/current or refreshed, focused tests and applicable gates.
- [ ] PR five sections, commit bullets, tests, footprint, final run and audit.
- [ ] Orchestrator watches CI, merges and closes #300 (and #280 as appropriate).
