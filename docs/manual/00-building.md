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
  same pin, byte-identical to the submodule. It exists because the
  submodule is private and CI cannot clone it (`vendor/README.md`). A bare
  clone works from this snapshot without touching submodules at all.

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
is already triaged and filed in `docs/divergence-log.md`, or a row ruled
ahead of the pin (the count is `4` at the current pin on an x86_64 host:
`comptime.lu`, whose `#[noalloc]` the closed attribute set refuses E0817
while the vendored 0.2.21 row still carries it — wolf-lang e951afbb, in
the 0.2.22 pairing, drops it; on an aarch64 host also `ffi.lu`, whose
`#[cfg(target = "x86_64")]` asm is dropped there, so the row runs to
`exit(1)` until the same commit's aarch64 twin arrives, which is why the
run, out-of-scope and mismatch counts below are elided; DIV-2026-019, the broken-sibling parse-code
disagreement, and DIV-2026-027's two failing `assert_msg_*` rows, whose
`stdout=` pins the dropped rendering of an assert message this machine
renders as a line (wolf-lang#556); DIV-2026-026, `memory/list_session_struct.lu` reading the
element its `let` moved, retired at the `ec56a08f` bump, where the row
copies the element instead; DIV-2026-022 and -023 retired at the `a7f517e` bump, where
wolf-lang#341 re-pinned the two seed headers that had gone stale against
their clauses; the gate in `tests/run_corpus.rs` waives only the filed
set and, by name, the rows ruled ahead of the pin):

```console
$ lupin corpus
…

892 file(s) under upstream/corpus: 846 entries, 46 member(s), 0 failure(s)
379 distinct conforms: anchor(s); every registered-namespace tag resolves against anchors.json

lupin: … entries reach the `run` rung; 629 match their `check:` expectation, 83 are the dynamic counterpart of the static code the corpus pins, 66 are static-conservatism entries (the compiler rejects statically what this machine never checks), … are out of scope, … mismatch
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
