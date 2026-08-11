# 0 — Installing & building

## Toolchain

The build needs Rust; `rust-toolchain.toml` pins the exact version, and
`rustup` picks it up automatically on the first `cargo` invocation. There
are no other build dependencies and no build scripts beyond recording the
git commit.

## Clone and build

```sh
git clone https://github.com/tenseleyFlow/wolf-interp
cd wolf-interp
cargo build --release
```

The binary lands at `target/release/lupin` (the wolf-interp package builds
a binary named `lupin`). The manual spells it `lupin`; substitute the path,
or put `target/release` on `PATH`.

## The pinned spec and corpus

The interpreter consumes two data trees from the wolf-lang repository at a
pinned revision: `spec/` (the language specification) and `corpus/` (the
conformance programs). They are available two ways, and the binary picks
whichever is present:

- `upstream/` — a git submodule pinned to the exact revision. Initialize it
  with `git submodule update --init upstream`. To keep the compiler's
  sources out of your tree, sparse-check it out:

  ```sh
  git -C upstream sparse-checkout init --cone
  git -C upstream sparse-checkout set spec corpus
  ```

- `vendor/upstream/` — a tracked snapshot of the same two trees at the same
  pin, byte-identical to the submodule. It exists because the submodule is
  private and CI cannot clone it (`vendor/README.md`). A bare clone works
  from this snapshot without touching submodules at all.

The corpus is read-only in both forms: a corpus file that looks wrong is a
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
this implementation and prints the ledger; its last line counts mismatches,
and the count is zero on a healthy checkout:

```console
$ lupin corpus
…

183 file(s) under upstream/corpus: 165 entries, 18 member(s), 0 failure(s)
172 distinct conforms: anchor(s); every registered-namespace tag resolves against anchors.json

lupin: 117 entries reach the `run` rung; 89 match their `check:` expectation, 5 are the dynamic counterpart of the static code the corpus pins, 35 are static-conservatism entries (the compiler rejects statically what this machine never checks), 36 are out of scope, 0 mismatch
```

## Bumping the pin

A pin bump is a deliberate act, in its own commit, landing CI-green:

```sh
git -C upstream fetch origin trunk
git -C upstream checkout <rev>          # an explicit revision, never a branch
cargo test                              # the corpus-size and anchor tests speak
git add upstream
git commit -m "pin: bump wolf-lang to <rev>"
```

`tests/corpus_harness.rs` asserts the corpus file count and that every
`conforms:` tag resolves against the pinned `spec/anchors.json`; if the
upstream corpus grew or a clause anchor moved, the bump commit is where you
find out. The vendored snapshot is re-vendored in the same commit
(`vendor/README.md` has the exact commands).
