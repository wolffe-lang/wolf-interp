# 0 — Installing & building

## Toolchain

The build needs Rust. `rust-toolchain.toml` pins the exact version, and
`rustup` picks it up on the first `cargo` invocation. There are no other
build dependencies, and no build scripts beyond the one that records the
git commit.

## Clone and build

```sh
git clone https://github.com/wolffe-lang/wolf-interp
cd wolf-interp
cargo build --release
```

The binary lands at `target/release/lupin`. The manual spells it `lupin`
throughout, so substitute the path or put `target/release` on `PATH`.

## The pinned spec and corpus

The interpreter consumes two data trees from the wolf-lang repository at a
pinned revision: `spec/` (the language specification) and `corpus/` (the
conformance programs). They are available two ways, and the binary picks
whichever is present:

- `upstream/` is a git submodule pinned to the exact revision. Initialize
  it with `git submodule update --init upstream`. To keep the compiler's
  sources out of your tree, sparse-check it out:

  ```sh
  git -C upstream sparse-checkout init --cone
  git -C upstream sparse-checkout set spec corpus
  ```

- `vendor/upstream/` is a tracked snapshot of the same two trees at the
  same pin, byte-identical to the submodule. This keeps a bare build and
  CI independent of a compiler checkout (`vendor/README.md`); neither
  needs to initialize the submodule.

The corpus is read-only in both forms. A corpus file that looks wrong is a
finding to report upstream; do not patch it here.

## Verifying the build

Run the four gates from a fresh clone:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- corpus
```

The corpus walk at the end checks every pinned conformance file against
this implementation and prints the ledger. Its last line counts
mismatches; on a healthy checkout every mismatch it counts is one that
is explicitly accounted for. On Unix the count is `5`: one exact
reset/reuse accounting forward pin in `tests/run_corpus.rs`, plus the
four filed findings in `docs/divergence-log.md`: DIV-2026-029, the vendored
`memory/elem_loop_induction_written_mut.lu`, which writes its `for` header's
name and pins E1002 where this machine now answers E0410 (`[gram.pat.let]`,
wolf-lang `a57276f2`) until the next pin advance carries its `var i = i`;
DIV-2026-027's two failing `assert_msg_*` rows, whose
`stdout=` pins the dropped rendering of an assert message this machine
renders as a line (wolf-lang#556), and DIV-2026-028, kw11's plain racy
counter, whose `check: pass` cannot spell the `trap(race)` this machine
answers and `[conc.mm.race.3]` permits (wolf-lang#603); DIV-2026-019, the
broken-sibling parse-code disagreement, closed at the `b16353e8` bump, where
a wrecked group is E0202 at its opener as on the compiler (wolf-interp#304);
DIV-2026-026, `memory/list_session_struct.lu` reading the
element its `let` moved, retired at the `ec56a08f` bump, where the row
copies the element instead; DIV-2026-022 and -023 retired at the `a7f517e` bump, where
wolf-lang#341 re-pinned the two seed headers that had gone stale against
their clauses. The gate in `tests/run_corpus.rs` checks that the reset
forward pin still has exactly its recorded output, and retires it when
the accounting agrees:

On Unix hosts:

```console unix
$ lupin corpus
…

1253 file(s) under upstream/corpus: 1191 entries, 62 member(s), 0 failure(s)
499 distinct conforms: anchor(s); every registered-namespace tag resolves against anchors.json

lupin: 957 entries reach the `run` rung; 889 match their `check:` expectation, 135 are the dynamic counterpart of the static code the corpus pins, 66 are static-conservatism entries (the compiler rejects statically what this machine never checks), 96 are out of scope, 5 mismatch
```

On Windows, `os/ids_read.lu` and `os/signal_reserved_rows.lu` handle the
host's `unsupported` error rows and finish normally. On Unix this
interpreter declines `os_uid` and `os_signal_realtime` by name instead.
The descriptor-relative calls and the new root, priority and signal-action
rows also take their specified Windows branches. Four new OS headers carry
the Unix stdout: the raw CLI counts their different Windows output as
mismatches, while the tests check the exact Windows expectations from the
spec. These are named observations, not skipped corpus entries:

```console windows
$ lupin corpus
…

1253 file(s) under upstream/corpus: 1191 entries, 62 member(s), 0 failure(s)
499 distinct conforms: anchor(s); every registered-namespace tag resolves against anchors.json

lupin: 966 entries reach the `run` rung; 894 match their `check:` expectation, 135 are the dynamic counterpart of the static code the corpus pins, 66 are static-conservatism entries (the compiler rejects statically what this machine never checks), 87 are out of scope, 9 mismatch
```

## Bumping the pin

A pin bump lands in its own commit, CI-green:

```sh
git -C upstream fetch origin trunk
git -C upstream checkout <rev>          # an explicit revision, never a branch
cargo test                              # the corpus-size and anchor tests speak
git add upstream
git commit -m "pin: bump wolf-lang to <rev>"
```

`tests/corpus_harness.rs` asserts the corpus file count and that every
`conforms:` tag resolves against the pinned `spec/anchors.json`. If the
upstream corpus grew or a clause anchor moved, the bump commit is where you
find out. The vendored snapshot is re-vendored in the same commit
(`vendor/README.md` has the commands). It takes a while; put on something
with a long slow movement.
