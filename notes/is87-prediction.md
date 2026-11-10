# is87 prediction: lupin's mirror of the pty calls (wolf-lang s292), and argv_is84's ETXTBSY

Committed before the first change. Base: trunk `921b7e4` (lupin 0.1.60, the upstream pin
`5f1f1c94`). Two changes on one branch.

## 1. The pty mirror

The compiler's change is wolf-lang#1091 (branch `s292`, head `5e89fdbe`, open, 49 commits on its
trunk `8a3d1ac8`). It adds `os_term_open_pty() -> (int, int) ! {io, unsupported}`,
`os_term_pty_name(fd) -> str ! {io, unsupported}`, `os_term_set_size(fd, rows, columns, xpixels,
ypixels) -> () ! {invalid, io, unsupported}`, `os_term_attr(fd, op) -> int ! {invalid, io,
unsupported}` and `os_term_set_attr(fd, op, value) -> () ! {invalid, io, unsupported}`
(`[os.term.pty]`, `[os.term.size.set]`, `[os.term.attr]`, the design note `[os.term.pty.note]`
of its `spec/11-os.md`, blob `413de821`). The pull request's section "For the lupin mirror" is
the specification of what this machine answers.

### What this machine does at the base

Measured with the published lupin 0.1.60 (the `aarch64-apple-darwin` archive, sha256
`f3cae16fdb88a94702c0fd816d5eb90cdc8e8636c028dcf40f03868c5dc36bb8`): wolf-lang's
`corpus/os/pty_rows.lu` (blob `0d74b900`, sha256
`3bcacf544bb848cfbb4674dbec93cb17f40c209e140d71b280e19dc471653c24`) stops at the resolve rung,
verdict `unsupported`, "`os_term_set_size` does not resolve", and prints nothing; `lupin run`
exits 4. None of the five names is in `AMBIENT_NAMES`, the dispatcher has no arm for them, the
wasm decline list names none, and `declared_row` gives them no row.

### The rule here (from the section and the clauses, not from the compiler's code)

No call here makes a host call: Rust's standard library has no `posix_openpt`, `ptsname`,
`tcgetattr`, `tcsetattr` or window-size `ioctl`, and this crate forbids `unsafe`. So, in the
clauses' order of refusal:

1. **The names resolve.** A program naming any of the five passes the resolve rung and runs.
2. **The SHAPE, on every host, before anything else** (`invalid`):
   - `os_term_set_size`: any of `rows`, `columns`, `xpixels`, `ypixels` outside 0..65535;
   - `os_term_attr` and `os_term_set_attr`: an `op` outside {1..18, 30..42, 50..62, 70..75,
     90..93, 128, 129}; for the set, a `value` outside its op's range (a character, ops 1..18,
     outside -1..255; a flag or a size, ops 30..93, outside 0..1; a speed, 128 and 129, outside
     0..4294967295).
3. **Then the HOST** (`unsupported`): a windows target for all five; for the two settings calls,
   an op the target lacks (linux and PAX 11, 15, 17; macOS 15, 16, 37, 52, 71); for the set, a
   speed the runtime's table does not name (0, 50, 75, 110, 134, 150, 200, 300, 600, 1200, 1800,
   2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400). The target is the observation's
   (`attrs::build_target()`), as for the other `[os.term]` calls: a darwin target reads macOS's
   set and every other unix target linux's, as the corpus file's own `lacks` does.
4. **Then the HANDLE** (`io`): a closed or forged handle for the four that take one, and for
   `os_term_set_size`, `os_term_attr` and `os_term_set_attr` one that is not a terminal (std's
   `IsTerminal`). `os_term_pty_name` answers `io` for a closed or forged handle only; any other
   handle is refused by name, as the section says.
5. **Every call past those rows is refused by name**: verdict `unsupported`, the reason naming
   the call and its clause, never "does not resolve". That is `os_term_open_pty` on every host
   but windows, and the other four on a live terminal handle (a read of a speed included).
6. **The rows:** `declared_row` gives `os_term_open_pty` and `os_term_pty_name` `{io,
   unsupported}` and the other three `{invalid, io, unsupported}`, so a handler matching the row
   is exhaustive and its arms discriminate.
7. **wasm** declines the five by name with the rest of the hosted process tier.

### Rows, red first

`tests/pty_s292.rs`, with the corpus file byte for byte as
`tests/fixtures/pty_s292/pty_rows.lu`. At the rows' commit (trunk's code) every row that names a
call fails, at "does not resolve". At the change every row passes:
- the shapes, `invalid` on this host and as the windows, linux, macOS and bundle targets;
- windows' `unsupported` after the shape, as the windows target;
- each unix target's lacking ops `unsupported`, the others `io` on a forged handle;
- a set speed outside the table `unsupported`, one in it `io` on a forged handle;
- a closed or forged handle `io` (the name's included), a file handle and a null stdin `io` for the
  three that take a terminal, and the name on a file refused by name;
- on a real terminal (a pty the test opens around lupin, as is83's terminal row does), each of the
  four refused by name; the open refused by name on every unix target;
- the declared rows discriminate a handler's arms;
- the corpus file runs to `exit(0)` with its declared stdout on this host, through `conform-run`
  and `lupin run`.

### What must not move

The vendored corpus (pin `5f1f1c94`) names none of the five, so the corpus ledger, the divergence
and conformance gates, the bundle and every other test are unchanged. No pin, CI, spec or vendored
file changes. The plant drops the lacking-op step, so a lacking op falls through to the handle:
red by name at the lacking-op row and at the corpus row on this host, with the shapes, windows,
the speed rows and the refusals green.

### A consequence to state, not to fix here

The corpus file chooses each host's lacking ops by `os_name()`, which reads the host this machine
runs on, while the rows follow the observation's target. Read in process as another target than
the host, the file prints its mismatch lines. That is no row of this change (the file is run on its
own host), but a pin that carries the file into the vendored corpus meets it in the bundle, which
is observed as linux on every exporter. The note records the options.

## 2. argv_is84's ETXTBSY (wolf-interp#360)

`executable_env_shebang_sees_script_name_and_args` writes its script with `std::fs::write` in the
test process and then runs it. The other tests of the binary spawn children from other threads at
the same time. A child forked while the script's write descriptor is open holds a copy until its
own exec (close-on-exec closes it only then), and linux refuses to exec a file some process holds
open for writing: `ETXTBSY`, "Text file busy".

**The fix, at the cause:** the test process never holds a write descriptor on a file it then
runs. The script is written by a helper process (`cp` from a source file the test wrote, into a
name removed first, so the inode is new), whose descriptor no sibling of the test can inherit, and
the helper is reaped before the run. No retry, no lock.

**The row, red first:** a new row in the same binary runs the same writer while sibling threads
spawn children in a loop, and runs each script it writes, many times. At the rows' commit the
writer is today's (in the test process): the row is predicted red on linux, `Text file busy` at
least once, and green on macOS, which does not refuse that exec (measured on the Mac's take). At
the fix it is green everywhere. The stress loop: the binary run at the rows' commit until it
fails, then at the fix at least 200 times under load on linux, with no failure.
