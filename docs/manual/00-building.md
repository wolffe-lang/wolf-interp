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
finding to report upstream, never a local edit.

## Verifying the build

The test suite is the warranty. From a fresh clone:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- corpus
```

The corpus walk at the end checks every pinned conformance file against
this implementation and prints the ledger. Its last line counts
mismatches, and the count is zero on a healthy checkout:

```console
$ lupin corpus
…

327 file(s) under upstream/corpus: 301 entries, 26 member(s), 0 failure(s)
224 distinct conforms: anchor(s); every registered-namespace tag resolves against anchors.json

lupin: 223 entries reach the `run` rung; 205 match their `check:` expectation, 13 are the dynamic counterpart of the static code the corpus pins, 32 are static-conservatism entries (the compiler rejects statically what this machine never checks), 51 are out of scope, 0 mismatch
```

## Bumping the pin

A pin bump is a deliberate act. It lands in its own commit, CI-green:

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
(`vendor/README.md` has the exact commands). There is no quick version of
this. Put on something with a long slow movement.
