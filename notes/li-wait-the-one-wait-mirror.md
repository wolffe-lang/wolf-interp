# li-wait — one wait, the set the kernel holds, a send that counts, a peer's credentials, mirrored (wolf-lang s290's second lane)

wolf-lang#1102 (merged at `ead70b82`) adds seven calls: `os_wait_ready(entries, set,
deadline_ms) -> (List[int], int) ! {invalid, io, unsupported}`, one wait over handles of both
tables and a meaning set that answers every ready element; `os_ready_open() -> int ! {io,
unsupported}`, `os_ready_set(r, handle, interest, key) -> () ! {invalid, io, unsupported}`,
`os_ready_wait(r, set, deadline_ms) -> (List[int], int) ! {invalid, io, unsupported}` and
`os_ready_close(r) -> () ! {io}`, the readiness set the kernel holds; `net_write_some(fd, bytes)
-> int ! {busy, closed, invalid, io}`, one send that says what it took; and `net_peer_cred(fd) ->
(int, int, int) ! {io, unsupported}`, a Unix-domain stream's peer (`[os.ready.wait]`,
`[os.ready.set]`, `[os.ready.facts]`, `[os.ready.note]`, `[os.net.write.some]`,
`[os.net.peer.cred]`). lupin 0.1.61 resolved none of the seven. The change's lane note, section
"For lupin (wolf-interp)", says what lupin answers; this is that mirror.

## 1. Forbidden

- No `unsafe`, no new dependency and no new crate feature; no `poll`, `epoll`, `kqueue` or peer
  credential call made for any of the seven.
- No pin, spec, vendored, CI or toolchain file changes: `vendor/upstream/` stays at `5f1f1c94`.
- No answer of another call moves: `net_write`, `net_wait`, `os_signal_wait_fds` and the rest
  answer as they did.
- The interpreter shares no implementation code with the compiler: the clauses, the corpus file,
  the change's lane note and its test's text are the inputs, never the compiler's crates.
- No label, merge, tag, release or issue closed by this branch; no attribution trailer.

## 2. Inputs, verified

| input | re-derived |
|---|---|
| trunk | `a10cbe5`, lupin 0.1.61, `vendor/upstream/PIN` `5f1f1c94` |
| the compiler's change | wolf-lang#1102, merged as `ead70b824e1ff1a0a1d353a2666aaa96d126b3fd`, wolf-lang's trunk at this note |
| the clauses | `spec/11-os.md` at `ead70b82` (blob `dca438b0`): the six clauses and seven lines of `[os.host.sigs]`; `spec/prelude.json` (blob `cfb33adf`) names the seven |
| the mirror's specification | `notes/s290-one-wait.md` at `ead70b82` (blob `1f4ae7e9`), section "For lupin (wolf-interp)" |
| the corpus file | `corpus/os/ready_rows.lu` at `ead70b82`: blob `7624a22d`, sha256 `c68193f194bcf53a67862720e925cb8947c456733cb254c6f1e05bba4d2a76b7`; `tests/fixtures/ready_s290/ready_rows.lu` is the same blob |
| the compiler's lupin row | `crates/wolf_driver/tests/ready_lanes.rs` (blob `f160f472`), `lupin_refuses_the_one_wait_by_name`: verdict `unsupported` and a reason that says "does not resolve" or names `os_wait_ready` |
| lupin 0.1.61 | the `aarch64-apple-darwin` release archive, sha256 `7236424618f5bbf1e5f7693cd6333df90b715e1401b08b795668cbb1425ed5af`: the corpus file stops at "`os_wait_ready` does not resolve", prints nothing, `lupin run` exits 4; each of the seven alone answers "does not resolve"; `lupin check` passes the file |
| the machine | `AMBIENT_NAMES`, the dispatcher, the wasm decline lists and `declared_row` named none of the seven; every stream the net table mints is non-blocking (`adopt_tcp`, the unix accept and dial); Rust's runtime ignores SIGPIPE in this process |
| README | read, current: it states nothing these changes move |

The section and the clauses agree on every row. The section leaves one order open (the payload's
width against the handle, for the send); the decision is below.

## 3. Prediction, scored

Committed before the first change (`273812c`, `notes/li-wait-prediction.md`).

- **The names resolve: hit.** Every row's program reaches its call.
- **The wait: hit.** Eighteen calls on the linux, macOS, bundle and windows targets in process:
  the set's shape `io` first, then the deadline `invalid`, then windows `unsupported`, then the
  wait nothing could end and the negative, forged, net-forged, write-forged and high-bit
  elements `io`; a closed pipe end and a closed listener `io` in four programs; nine well-formed
  waits (pipes, a standard stream, an empty list with a deadline and with a meaning set, a
  listener and a TCP stream with bits 61 and 62) refused by name on the three unix targets and
  `unsupported` as windows; on this host, the rows before the wait print and the wait over a
  Unix stream is refused by name.
- **The set: hit.** Nineteen calls on the four targets: `invalid` for an interest outside 0..3 or
  a negative key; windows `unsupported`; then `io` for the change, the wait (after its set's
  shape and its deadline; at once, a deadline of -1 included) and the close on every host. The
  open is refused by name on the three unix targets and on this host, and answers
  `unsupported` as windows.
- **The send: hit.** A forged, negative, zero or closed handle and both listeners answer `io`
  (with `os_error()` 0), an empty list to each `io`; one send of five bytes answers 5 on a TCP
  and a Unix-domain stream, the peer reads them whole, an empty list answers 0 and the peer's
  read with a 50 ms budget answers `timeout`. Writes of 64 KiB lists answer counts of 1..65536
  until `busy`, on both families, with `os_error()` 11 on linux and 35 on macOS; asked again with
  nothing read, `busy` again (the TCP stream too, on linux, though its row admits a count); the
  peer drains exactly what was counted and a send is taken again (`room=1 0`). After the peer
  left, the send answers `closed` with EPIPE or ECONNRESET, and an empty list 0.
- **The credentials: hit.** `io` for a forged, negative or closed handle, a TCP stream (dialed and
  accepted) and both listeners; a dialed and an accepted Unix stream refused by name;
  `unsupported` as windows, a live stream included.
- **The declared rows: hit.** Fourteen handlers over the seven calls: each bare arm name tests a
  tag of the call's row.
- **The corpus file: hit.** On linux and macOS it is refused by name at its first wait, nothing
  printed, through `conform-run` and `lupin run` (exit 4). With its host reading fixed to
  windows, observed as the windows target, all 31 lines are `true` and it exits 0 with its
  declared stdout.
- **Red first: hit.** At `a39d4ae` (trunk's code) all 17 rows failed, each at "does not resolve",
  in two takes on two blocks (`linux-red-a39d4ae.log`, `linux-red-a39d4ae-again.log`).
- **Not measured.** The prediction's host numbers beyond `os_error()` after `busy`: how many bytes
  a stream takes before `busy`, and which of EPIPE and ECONNRESET each family answers after the
  peer left (the rows hold the clause's words, either number). No windows host ran the rows: the
  windows rows are held in process as the windows target, and CI's windows runner runs the
  corpus file there, where it is predicted to exit 0 with its declared stdout.
- **Two defects of this lane's own, found by the takes:** a borrow error in the credentials arm
  (`linux-build-red-45f648e.log`, fixed before the commit left this machine), and the busy rows'
  line count, eight asserted where they print seven (`linux-green-a0ea816.log`: every line as
  predicted; fixed in the row, `3e24d2e`).
- **What must not move:** the vendored corpus names none of the seven; the gauntlet's corpus walk,
  `run_corpus` and the bundle answer that (the records are in the pull request).

## 4. Decisions

Each was decided before the code, from the options in the prediction; what is implemented is what
was recommended.

- **1. The target the host rows follow: A, the observation's target**
  (`attrs::build_target()`), as the session and pseudo-terminal calls' rows do; not the build's
  `cfg!(windows)`, which `os_signal_wait_fds` still reads. The windows rows are held in process
  on every host, and a bundle observed as linux answers as linux on any exporter.
- **2. A wait with nothing to poll: A, refused by name** with every other well-formed
  wait, as the section says, rather than served as a park until the deadline: no program needs
  this machine to sleep through a wait that names nothing, and serving it is new park surface
  (the kill look, the task baton) no witness reaches.
- **3. The set calls after the host row: A, `io`**, every set number forged, as the
  section says and as the clause says for the close ("where no set can be made every number is
  forged").
- **4. The send's payload against its handle: A, the width first**, whole, before the
  socket is asked, as this machine's `net_write_bytes` checks it. No typed program reaches the
  difference.
- **5. The send: A, one `write` on the stream this machine already holds non-blocking**;
  an interrupted write took nothing and is made again; a write of 0 from a non-empty list is
  `closed`, as `net_write`'s poll answers it. Not `send(2)` with `MSG_DONTWAIT` and `MSG_NOSIGNAL`
  through nix's `socket` feature: the same single call on a non-blocking descriptor, with no new
  crate feature, and Rust's runtime already ignores SIGPIPE here.
- **6. A Unix stream's credentials: A, refused by name**, as the section says, rather
  than served through nix's `socket` feature (`SO_PEERCRED`; `LOCAL_PEERCRED` and
  `LOCAL_PEERPID`): a new crate feature is its own decision, as the pseudo-terminal mirror
  recorded.
- **7. wolf-lang's lupin row: A, no wolf-lang change now** (section 5).

## 5. wolf-lang's lupin row

`lupin_refuses_the_one_wait_by_name` accepts verdict `unsupported` with a reason that says "does
not resolve" or names `os_wait_ready`. 0.1.61's reason, "`os_wait_ready` does not resolve",
passes both ways; this mirror's names `os_wait_ready` and never says "does not resolve", so the
row stays green. It cannot tell the two apart, and it cannot need to until wolf-lang's
`crates/wolf_driver/PAIRING` names a release carrying this mirror: CI's sibling lupin is the
release `PAIRING` names (0.1.61 today), downloaded by its digest. The change that moves `PAIRING`
should pin the row by version in the same commit, as `pty_lanes`' row is pinned:

```rust
/// The lupin releases that predate wolf-interp's mirror of the one wait:
/// each answers a program naming the seven "does not resolve".
const PRE_MIRROR_LUPIN: &[&str] = &["0.1.60", "0.1.61"];

// in lupin_refuses_the_one_wait_by_name, after record_fields:
assert_eq!(verdict, "unsupported", "lupin {version}: {text}");
if PRE_MIRROR_LUPIN.contains(&version.as_str()) {
    assert!(
        reason.contains("does not resolve"),
        "lupin {version} predates the mirror: {reason}"
    );
} else {
    assert!(
        reason.contains("`os_wait_ready`") && !reason.contains("does not resolve"),
        "lupin {version} refuses the first wait by name: {reason}"
    );
}
```

A local sibling built from wolf-interp's trunk before this change calls itself
`0.1.61+dev.<commit>`, which the list does not name, and would read red: build the sibling from a
trunk that carries this change. This repository's own row for the same predicate is
`the_corpus_rows_on_this_host`.

## 6. What a wolf 0.2.40 pairing still needs

- A lupin release carrying this change (the version bump, this entry moved from Unreleased into
  the release's, `ci/r44`'s release-metadata check renewed with the seven names).
- wolf-lang's `PAIRING` moved to that release, with the row of section 5 pinned in the same
  commit.
- Not needed for the pairing, and not done here: a pin advance. The vendored corpus is
  `5f1f1c94`; a pin that carries `corpus/os/ready_rows.lu` into it brings `W0304` for a
  declaration shadowing one of the seven (this machine reads the pinned `prelude.json`), and the
  bundle, observed as linux, refuses the file by name at its first wait.

## 7. Evidence index

`notes/li-wait-evidence/index.txt` lists every file: the red and green takes on Linux, the take on
macOS and the rigs. The Linux gauntlet record at the final head and the plant record on it are in
the pull request (a record cannot predate the head it names).

## 8. Done-when

- [x] the prediction committed before the first change (`273812c`)
- [x] the rows red at trunk's code, by name: 0 passed, 17 failed, twice (`a39d4ae`, Linux)
- [x] the rows green at the change, with fmt, clippy of every target, the library's 797 and ten
      neighbouring targets (`3e24d2e`, Linux)
- [x] the rows green on macOS, with seven neighbouring targets and clippy (`3e24d2e`)
- [ ] the Linux gauntlet at the final head, every `END … exit=0`
- [ ] the plant, red by name at its rows, its controls green
- [ ] the audit, once
