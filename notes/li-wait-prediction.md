# li-wait prediction: lupin's mirror of the one wait (wolf-lang s290's second lane)

Committed before the first change. Base: trunk `a10cbe5` (lupin 0.1.61, the upstream pin
`5f1f1c94`).

## The compiler's change

wolf-lang#1102, merged as `ead70b824e1ff1a0a1d353a2666aaa96d126b3fd`, adds seven names to wolf's
prelude (`[os.ready.wait]`, `[os.ready.set]`, `[os.ready.facts]`, `[os.ready.note]`,
`[os.net.write.some]`, `[os.net.peer.cred]` of its `spec/11-os.md`, blob `dca438b0`, and seven
lines of `[os.host.sigs]`):

```text
os_wait_ready(List[int], int, int) -> (List[int], int) ! {invalid, io, unsupported}
os_ready_open() -> int ! {io, unsupported}
os_ready_set(int, int, int, int) -> () ! {invalid, io, unsupported}
os_ready_wait(int, int, int) -> (List[int], int) ! {invalid, io, unsupported}
os_ready_close(int) -> () ! {io}
net_write_some(int, List[byte]) -> int ! {busy, closed, invalid, io}
net_peer_cred(int) -> (int, int, int) ! {io, unsupported}
```

The section "For lupin (wolf-interp)" of its lane note (`notes/s290-one-wait.md`, blob
`1f4ae7e9`) is the specification of what this machine answers.

## What this machine does at the base

Measured with the published lupin 0.1.61 (the `aarch64-apple-darwin` release archive, sha256
`7236424618f5bbf1e5f7693cd6333df90b715e1401b08b795668cbb1425ed5af`): wolf-lang's
`corpus/os/ready_rows.lu` (blob `7624a22d`, sha256
`c68193f194bcf53a67862720e925cb8947c456733cb254c6f1e05bba4d2a76b7`) stops at "`os_wait_ready`
does not resolve", verdict `unsupported` at `run`, and prints nothing; `lupin run` exits 4. A
program naming any one of the seven stops at "`<name>` does not resolve". `lupin check` passes the
file: this machine resolves a builtin's name when the call runs, so a name's "type-check" here is
that resolution and the call's declared row, not a static rung.

## The rule here (from the section and the clauses, not from the compiler's code)

Six of the seven need a host call this machine cannot make: Rust's standard library has no
`poll`, `epoll` or `kqueue`, its `peer_cred` is unstable, and this crate forbids `unsafe`. The
seventh, `net_write_some`, is one write on a stream this machine already holds non-blocking. So:

1. **The names resolve**, each with the declared row of its `[os.host.sigs]` line, so a handler
   that matches the row is exhaustive and its arms discriminate:
   `os_wait_ready`, `os_ready_set`, `os_ready_wait` `{invalid, io, unsupported}`;
   `os_ready_open` and `net_peer_cred` `{io, unsupported}`; `os_ready_close` `{io}`;
   `net_write_some` `{busy, closed, invalid, io}`. wasm declines the seven by name (the five
   `os_` names with the hosted process tier, the two `net_` names with the net tier).
2. **`os_wait_ready`**, in the clause's order of refusal: a non-zero set the shape rule refuses
   (a bit outside the twenty listenable meanings, a negative set) `io`; a deadline below -1
   `invalid`; then a windows target `unsupported`; then no element with set 0 and deadline -1
   `io`; then a negative element, or one that names no live handle of its table (bit 61 a net
   handle, bit 62 write interest, either or both) `io`. Every other call is refused by name:
   std has no `poll`, and this machine never listens.
3. **The set**: `os_ready_open` answers `unsupported` on a windows target and is refused by name
   everywhere else. `os_ready_set`: an interest outside 0..3 or a negative key `invalid`; a
   windows target `unsupported`; then `io`. `os_ready_wait`: the set's shape `io`, a deadline
   below -1 `invalid`, a windows target `unsupported`, then `io`. `os_ready_close`: `io` on every
   host. No set can be made here, so every set number is forged, which is the clause's own
   sentence for the close ("where no set can be made every number is forged").
4. **`net_write_some`, served on every host:** a list of the wrong element width `invalid`,
   whole, before the socket is asked; a forged, closed or negative handle and a listener of
   either family `io`, with no host number; an empty list 0 (the handle checked, no send); then
   ONE write on the stream, which this machine made non-blocking when it minted it. The answer
   is the count the host took, at least 1 and at most the list's length; would-block is `busy`
   with the host's number in `os_error()`; a peer that has gone is `closed` (EPIPE,
   ECONNRESET) with its number; any other refusal `io` with its number. It never parks, and an
   armed `net_deadline` does not apply.
5. **`net_peer_cred`:** a windows target `unsupported`; then a forged, closed or negative handle,
   a TCP stream and a listener of either family `io`; a Unix-domain stream, dialed or accepted,
   refused by name.
6. **Every refusal by name** is verdict `unsupported` with a reason that names the call and its
   clause, never "does not resolve".

The windows rows follow the observation's target, as the session and pseudo-terminal mirrors'
do (decision 1 below).

## Rows, red first

`tests/ready_s290.rs`, with the corpus file byte for byte as
`tests/fixtures/ready_s290/ready_rows.lu`:

- the wait's shape and host rows, on the linux, macOS, bundle and windows targets in process;
- a closed handle of either table `io` in the wait;
- a well-formed wait refused by name on every unix target and on this host (pipes, a standard
  stream, a listener and a stream with bit 61, write interest, an empty list, a meaning set);
- the set's rows on every target, and the open refused by name on every unix target;
- `net_write_some`'s handle rows (forged, negative, closed, both listeners, an empty list to each);
- one send, counted, over a loopback TCP stream and a Unix-domain stream, read back whole;
- `busy` on a full Unix-domain stream and on a full loopback TCP stream, the host's EAGAIN in
  `os_error()`, still `busy` when asked again, and room after the peer drains what was counted;
- `closed` after the peer left, on both families, with EPIPE or ECONNRESET in `os_error()`, and
  0 for an empty list there;
- `net_peer_cred`'s rows, the Unix stream refused by name, the windows rows in process;
- the declared rows discriminate a handler's arms, for all seven;
- the corpus file on this host: refused by name at its first wait, nothing printed, through
  `conform-run` and `lupin run` (exit 4);
- the corpus file with its host reading set to windows, observed as the windows target: every
  line `true`, exit 0 with its declared stdout.

**At the rows' commit (trunk's code) every row fails**, each at "does not resolve". **At the
change every row passes**, on Linux and on macOS.

What the hosts will answer, as far as the clauses' measurements say (the rows hold only the
clause's words, never these numbers): a Unix-domain stream nobody reads answers `busy` after a
few 64 KiB writes on linux and after one partial write (8192 bytes) on macOS; a loopback TCP
stream after some megabytes on linux and less on macOS; `os_error()` after `busy` is 11 on linux
and 35 on macOS; after a Unix stream's peer left, EPIPE (32); after a TCP peer left, the first
write is taken and a later one answers `closed`.

## What must not move

The vendored corpus (pin `5f1f1c94`) names none of the seven, so the corpus ledger, the
divergence and conformance gates, the bundle and every other test are unchanged. No pin, CI,
spec, vendored, toolchain, dependency or crate-feature change; no `unsafe`. No other call's
answer moves: `net_write`, `net_wait` and `os_signal_wait_fds` answer as they did. This machine
reads `W0304`'s prelude names from the pinned `prelude.json`, so a declaration shadowing one of
the seven is not linted until a pin advance carries wolf-lang's.

**wolf-interp holds no prelude count of its own.** The counts that moved upstream (the host
table, 185 to 192; the prelude; `W0304`; the runtime symbols; the anchors) are wolf-lang's. This
machine's name tables are `AMBIENT_NAMES`, the dispatcher, `declared_row` and the wasm decline
arms, and all four move.

## The plant

`li-wait-plant-busy`, one commit on the final head, one hunk in `src/eval/net.rs`: the
would-block answer of `net_write_some` reported as `io`. Predicted red by name at the two `busy`
rows; its controls green: the handle rows, the one send, the `closed` rows and the refusals.

## wolf-lang's lupin row

`lupin_refuses_the_one_wait_by_name` accepts a reason that says "does not resolve" or names
`os_wait_ready`. With this change lupin's reason names `os_wait_ready` and never says "does not
resolve", so the row stays green. It cannot tell this mirror from 0.1.61 until wolf-lang's
`PAIRING` names a release that carries it, because CI's sibling lupin is `PAIRING`'s release.
Predicted: no wolf-lang change is needed now (decision 7 below).

## Decisions

- **1. The target the host rows follow.** (A) The observation's target
  (`attrs::build_target()`), as `os_spawn_session`'s and the pseudo-terminal calls' rows do: the
  windows rows are held in process on every host, and a bundle observed as linux answers as
  linux on any exporter. (B) The build's `cfg!(windows)`, as `os_signal_wait_fds` does.
  **Recommend A.**
- **2. A wait with nothing to poll** (no element, set 0, a deadline of 0 or more).
  (A) Refused by name with every other well-formed wait, as the section's mirror says. (B)
  Served: a park until the deadline answering `([], 0)`, with the clause's kill look and a task
  baton. **Recommend A**: no program needs this machine to sleep through a wait that names
  nothing, and B is new park surface with no witness that reaches it.
- **3. The set calls after the host row.** (A) `io`, every set number forged, as the
  section says and as the clause says for the close. (B) Refused by name. **Recommend A**: the
  answer is decided without the host, and B would refuse a call whose row the clause gives.
- **4. `net_write_some`'s payload against its handle.** (A) The width first, whole,
  before the socket is asked, as this machine's `net_write_bytes` does. (B) The handle first.
  **Recommend A.** No typed program reaches the difference: a `List[byte]` holds only bytes.
- **5. The send.** (A) One `write` on the stream this machine already holds
  non-blocking (every stream it mints is; Rust's runtime ignores SIGPIPE, so a peer gone is
  EPIPE, never a signal); a write the host interrupted took nothing and is made again; a write
  that takes 0 of a non-empty list is `closed`, as `net_write`'s poll answers it. (B) `send(2)`
  with `MSG_DONTWAIT` and `MSG_NOSIGNAL` through nix's `socket` feature. **Recommend A**: the
  same single call on a non-blocking descriptor, with no new crate feature.
- **6. The credentials of a Unix-domain stream.** (A) Refused by name, as the section
  says. (B) Served through nix's `socket` feature (`SO_PEERCRED`; `LOCAL_PEERCRED` and
  `LOCAL_PEERPID`). **Recommend A**: a new crate feature is its own decision, as the
  pseudo-terminal mirror recorded.
- **7. wolf-lang's lupin row.** (A) No wolf-lang change now: the change that moves
  `PAIRING` to a release carrying this mirror pins the row by version in the same commit,
  `0.1.60` and `0.1.61` "does not resolve", any other release refused by name naming
  `os_wait_ready` and never "does not resolve" (the text is in the lane note). (B) A separate
  wolf-lang pull request now, with the same text. **Recommend A**: until `PAIRING` moves, CI's
  sibling lupin is 0.1.61 and the tightened row would test nothing new; the bump is the change
  that makes it bite.
