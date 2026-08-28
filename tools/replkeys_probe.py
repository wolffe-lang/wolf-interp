#!/usr/bin/env python3
"""is25 evidence: drive `lupin` under a PTY and verify every keybinding fires.

The sprint's doctrine is that documented != observed: no binding is claimed
because rustyline's docs list it — each one is exercised against the built
binary and judged by what the SESSION answered, not by what the crate says.
Run it after `cargo build --release`:

    python3 tools/replkeys_probe.py [path/to/lupin]

Prints one row per binding — asked-for -> bound-to -> verified-by — and
exits nonzero if any expectation fails. Unix-only (pty.fork); the Windows
story is stated in docs/repl.md [repl.edit.keys].
"""

import os
import pty
import signal
import sys
import time

BIN = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.join(os.path.dirname(__file__), "..", "target", "release", "lupin")
)

ESC = b"\x1b"
UP, DOWN = b"\x1b[A", b"\x1b[B"
C_LEFT, C_RIGHT = b"\x1b[1;5D", b"\x1b[1;5C"
HOME, END = b"\x1b[H", b"\x1b[F"


def run(name, seq, argv=(), env_extra=None, wait=0.25, keep_history=False):
    """One PTY session: send each chunk, gather output, reap the child."""
    env = dict(os.environ)
    env["TERM"] = "xterm-256color"
    env.setdefault("LUPIN_HISTORY", f"/tmp/lupin-probe-{os.getpid()}-{name}")
    if env_extra:
        env.update(env_extra)
    if not keep_history:
        try:
            os.unlink(env["LUPIN_HISTORY"])
        except FileNotFoundError:
            pass
    pid, fd = pty.fork()
    if pid == 0:
        os.environ.clear()
        os.environ.update(env)
        os.execv(BIN, [BIN] + list(argv))
    time.sleep(0.6)
    out = b""
    os.set_blocking(fd, False)

    def drain():
        nonlocal out
        try:
            while True:
                chunk = os.read(fd, 65536)
                if not chunk:
                    break
                out += chunk
        except (BlockingIOError, OSError):
            pass

    for chunk in seq:
        os.write(fd, chunk)
        time.sleep(wait)
        drain()
    time.sleep(0.5)
    drain()
    alive = True
    try:
        alive = os.waitpid(pid, os.WNOHANG) == (0, 0)
    except ChildProcessError:
        alive = False
    if alive:
        os.kill(pid, signal.SIGKILL)
        try:
            os.waitpid(pid, 0)
        except OSError:
            pass
    os.close(fd)
    return out.decode(errors="replace"), alive, env["LUPIN_HISTORY"]


# (asked-for, bound-to, session, expect_substrings, expect_absent, note)
# `session` = (keys, argv, env). A case verifies by the SESSION'S ANSWER —
# the evaluated value line — not by echo bytes.
CASES = [
    ("Up (history recall)", "rustyline stock: LineUpOrPreviousHistory",
     [b"1 + 1\n", UP, b"\n", b":quit\n"], (), None,
     lambda o: o.count("2 : i32") >= 2, "recalled `1 + 1` re-evaluates"),
    ("Down (history forward)", "rustyline stock: LineDownOrNextHistory",
     [b"11\n", b"22\n", UP, UP, DOWN, b"\n", b":quit\n"], (), None,
     lambda o: o.count("22 : i32") >= 2, "Up Up Down lands on the newer entry"),
    ("Ctrl-A / Ctrl-E", "rustyline stock: Move(BeginningOfLine/EndOfLine)",
     [b"3", b"\x01", b"2+", b"\x05", b"+4", b"\n", b":quit\n"], (), None,
     lambda o: "9 : i32" in o, "edits land at both line edges: 2+3+4"),
    ("Ctrl-B / Ctrl-F", "rustyline stock: Move(Backward/ForwardChar)",
     [b"13", b"\x02", b"2", b"\x06", b"4", b"\n", b":quit\n"], (), None,
     lambda o: "1234 : i32" in o, "char motion: 13 -> 1234"),
    ("Alt-b / Alt-f", "rustyline stock: Move(Backward/ForwardWord)",
     [b"10 + 20", ESC + b"b", b"1", ESC + b"f", b"\n", b":quit\n"], (), None,
     lambda o: "130 : i32" in o, "word motion: 20 -> 120"),
    ("Ctrl-Left / Ctrl-Right", "rustyline stock: same word motion",
     [b"10 + 20", C_LEFT, b"3", C_RIGHT, b"\n", b":quit\n"], (), None,
     lambda o: "330 : i32" in o, "terminal word-motion variants"),
    ("Home / End", "rustyline stock: Move(BeginningOfLine/EndOfLine)",
     [b"3", HOME, b"1+", END, b"+5", b"\n", b":quit\n"], (), None,
     lambda o: "9 : i32" in o, "1+3+5"),
    ("Ctrl-W", "rustyline stock: Kill(BackwardWord, big-word)",
     [b"100 + 209", b"\x17", b"300", b"\n", b":quit\n"], (), None,
     lambda o: "400 : i32" in o, "word-erase kept out of raw mode's grave"),
    ("Alt-Backspace", "rustyline stock: Kill(BackwardWord)",
     [b"100 + 209", ESC + b"\x7f", b"300", b"\n", b":quit\n"], (), None,
     lambda o: "400 : i32" in o, "backward-kill-word"),
    ("Alt-d", "rustyline stock: Kill(ForwardWord)",
     [b"100 + 200", HOME, ESC + b"d", b"900", END, b"\n", b":quit\n"], (), None,
     lambda o: "1100 : i32" in o, "kill-word from line start"),
    ("Ctrl-K", "rustyline stock: Kill(EndOfLine)",
     [b"42+999", b"\x01", b"\x06\x06", b"\x0b", b"\n", b":quit\n"], (), None,
     lambda o: "42 : i32" in o, "kill to end leaves `42`"),
    ("Ctrl-U", "rustyline stock: Kill(BeginningOfLine)",
     [b"999", b"\x15", b"5", b"\n", b":quit\n"], (), None,
     lambda o: "5 : i32" in o and "999 : i32" not in o, "kill to start"),
    ("Ctrl-Y", "rustyline stock: Yank",
     [b"100+", b"\x15", b"\x19", b"1", b"\n", b":quit\n"], (), None,
     lambda o: "101 : i32" in o, "the kill comes back"),
    ("Alt-y (yank-pop)", "rustyline stock: YankPop",
     [b"11", b"\x15", b"22", b"\x15", b"\x19", ESC + b"y", b"\n", b":quit\n"], (), None,
     lambda o: "11 : i32" in o and "22 : i32" not in o, "ring cycles to older kill"),
    ("Alt-. (yank-last-arg)", "OUR ConditionalEventHandler (not in rustyline)",
     [b":schedule 555\n", ESC + b".", b"\n", b":quit\n"], (), None,
     lambda o: "555 : i32" in o, "previous input's last word returns"),
    ("Alt-. repeat cycles", "OUR handler: Replace() walking older entries",
     [b":schedule 111\n", b":schedule 222\n", ESC + b".", ESC + b".", b"\n", b":quit\n"], (), None,
     lambda o: "111 : i32" in o and "222 : i32" not in o, "second press = older entry"),
    ("Alt-_ (yank-last-arg)", "OUR handler on the second readline spelling",
     [b":schedule 777\n", ESC + b"_", b"\n", b":quit\n"], (), None,
     lambda o: "777 : i32" in o, "the Alt-_ synonym"),
    ("Alt-u (upcase-word)", "rustyline stock: UpcaseWord",
     [b"let WORD = 6\n", b"word", HOME, ESC + b"u", b"\n", b":quit\n"], (), None,
     lambda o: "6 : i32" in o, "word -> WORD resolves the binding"),
    ("Alt-l (downcase-word)", "rustyline stock: DowncaseWord",
     [b"let lower = 8\n", b"LOWER", HOME, ESC + b"l", b"\n", b":quit\n"], (), None,
     lambda o: "8 : i32" in o, "LOWER -> lower"),
    ("Alt-c (capitalize)", "rustyline stock: CapitalizeWord",
     [b"let Cap = 9\n", b"cap", HOME, ESC + b"c", b"\n", b":quit\n"], (), None,
     lambda o: "9 : i32" in o, "cap -> Cap"),
    ("Ctrl-T (transpose-chars)", "rustyline stock: TransposeChars",
     [b"12", b"\x14", b"\n", b":quit\n"], (), None,
     lambda o: "21 : i32" in o, "12 -> 21"),
    ("Alt-t (transpose-words)", "rustyline stock: TransposeWords",
     [b"8 - 2", ESC + b"t", b"\n", b":quit\n"], (), None,
     lambda o: "-6 : i32" in o, "8 - 2 -> 2 - 8"),
    ("Ctrl-L (clear-screen)", "rustyline stock: ClearScreen",
     [b"5+5", b"\x0c", b"\n", b":quit\n"], (), None,
     lambda o: "10 : i32" in o and "\x1b[H" in o and ("\x1b[J" in o or "\x1b[2J" in o),
     "screen cleared, buffer intact"),
    ("Ctrl-_ (undo)", "rustyline stock: Undo",
     [b"12", b"\x1f\x1f", b"7", b"\n", b":quit\n"], (), None,
     lambda o: "7 : i32" in o and "127 : i32" not in o and "12 : i32" not in o,
     "the insertions un-happen"),
    ("Ctrl-R (reverse search)", "rustyline stock: ReverseSearchHistory",
     [b"1234\n", b"55\n", b"\x12", b"23", b"\n", b"\n", b":quit\n"], (), None,
     lambda o: o.count("1234 : i32") >= 2, "search `23` finds and re-runs 1234"),
    ("TAB: directive", "OUR Completer, source 1 (`:` commands)",
     [b":he\t", b"\n", b":quit\n"], (), None,
     lambda o: ":type e" in o, "`:he<TAB>` -> `:help` runs"),
    ("TAB: ambiguity lists", "OUR Completer + CompletionType::List",
     [b":re\t\t", b"\x03", b":quit\n"], (), None,
     lambda o: ":regions" in o and ":reset" in o, "candidates listed, not guessed"),
    ("TAB: subcommand", "OUR Completer (`:trace on|off|show|clear`)",
     [b":trace s\t", b"\n", b":quit\n"], (), None,
     lambda o: "trace is off" in o, "`:trace s<TAB>` -> `show`"),
    ("TAB: session name", "OUR Completer, source 2 (Session names)",
     [b"let alphabet = 1\n", b"alphab\t", b"\n", b":quit\n"], (), None,
     lambda o: "1 : i32" in o, "`alphab<TAB>` -> the bound `alphabet`"),
    ("TAB: surface not f#2", "OUR Completer: generational rule",
     [b"fn foo() -> int { 1 }\n", b"fn foo() -> int { 2 }\n", b"fo\t", b"()", b"\n", b":quit\n"],
     (), None,
     lambda o: "2 : i64" in o and "foo#" not in o, "completes `foo`, never `foo#2`"),
    ("TAB: :load path", "OUR Completer, source 3 (fs paths)",
     [b":load exampl\t", b"\x03", b":quit\n"], (), None,
     lambda o: "examples/" in o, "path candidates from the repo tree"),
    ("Ctrl-D non-empty", "rustyline stock: Kill(ForwardChar) in emacs mode",
     [b"12", b"\x01", b"\x04", b"\n", b":quit\n"], (), None,
     lambda o: "2 : i32" in o, "delete-char, NOT exit"),
    ("Ctrl-D empty = exit", "termios VEOF -> EndOfFile",
     [b"\x04"], (), None,
     lambda o, alive: not alive, "empty-line EOF still leaves"),
    ("Ctrl-C cancels (#46)", "VINTR -> Interrupt -> input abandoned",
     [b"fn f() {\n", b"\x03", b"1+1\n", b":quit\n"], (), None,
     lambda o: "2 : i32" in o and "error[" not in o, "continuation escaped, session alive"),
    ("Multi-line = ONE entry", "OUR Validator over lex::repl_input_complete",
     [b"fn dbl(x: int) -> int {\n", b"    x * 2\n", b"}\n", UP, b"\n", b":quit\n"], (), None,
     lambda o: "redefined fn `dbl`" in o, "Up recalls the whole fn, not `}`"),
]


def main():
    failures = []
    print(f"probing {os.path.abspath(BIN)}\n")
    print(f"{'asked for':28} | {'bound to':46} | verdict")
    print("-" * 110)
    for case in CASES:
        asked, bound, seq, argv, env, check, note = case
        out, alive, _hist = run(asked.replace(" ", "_").replace("/", "-"), seq, argv, env)
        try:
            ok = check(out, alive) if check.__code__.co_argcount == 2 else check(out)
        except Exception as err:  # noqa: BLE001 - a probe never hides a crash
            ok, note = False, f"probe error: {err}"
        verdict = "VERIFIED" if ok else "FAILED"
        print(f"{asked:28} | {bound:46} | {verdict}: {note}")
        if not ok:
            failures.append((asked, out))

    # Persistence across sessions, one shared history file.
    hist = f"/tmp/lupin-probe-{os.getpid()}-persist"
    try:
        os.unlink(hist)
    except FileNotFoundError:
        pass
    env = {"LUPIN_HISTORY": hist}
    run("persist_a", [b"let persisted = 123\n", b":quit\n"], env_extra=env, keep_history=True)
    # `:quit` is recorded like any input (readline temperament), so the
    # target line is TWO recalls up in the fresh session.
    out, _, _ = run("persist_b", [UP, UP, b"\n", b"persisted\n", b":quit\n"],
                    env_extra=env, keep_history=True)
    ok = "123 : i32" in out
    print(f"{'History persists':28} | {'OUR HistoryStore + LUPIN_HISTORY/XDG/APPDATA':46} | "
          f"{'VERIFIED' if ok else 'FAILED'}: recall works in a NEW session")
    if not ok:
        failures.append(("persistence", out))
    with open(hist, encoding="utf-8") as fh:
        body = fh.read()
    ok = '"let persisted = 123"' in body and all(
        line.startswith('"') for line in body.splitlines() if line
    )
    print(f"{'History file format':28} | {'one JSON string per line (HistoryStore)':46} | "
          f"{'VERIFIED' if ok else 'FAILED'}: entries survive as single items")
    if not ok:
        failures.append(("history file", body))

    # Consecutive duplicates are not recorded.
    out, _, hist2 = run("dedupe", [b"9\n", b"9\n", b":quit\n"])
    with open(hist2, encoding="utf-8") as fh:
        body = fh.read()
    ok = body.count('"9"') == 1
    print(f"{'No consecutive dupes':28} | {'HistoryStore.push policy':46} | "
          f"{'VERIFIED' if ok else 'FAILED'}: `9` twice records once")
    if not ok:
        failures.append(("dedupe", body))

    # The gate: TERM=dumb and --no-edit take the dumb reader (no raw mode).
    out, _, _ = run("dumb_term", [b"1+1\n", b":quit\n"], env_extra={"TERM": "dumb"})
    ok = "\x1b[?2004h" not in out and "2 : i32" in out
    print(f"{'TERM=dumb -> dumb reader':28} | {'OUR gate in repl_loop':46} | "
          f"{'VERIFIED' if ok else 'FAILED'}: no raw-mode escapes emitted")
    if not ok:
        failures.append(("TERM=dumb", out))
    out, _, _ = run("no_edit", [b"1+1\n", b":quit\n"], argv=["repl", "--no-edit"])
    ok = "\x1b[?2004h" not in out and "2 : i32" in out
    print(f"{'--no-edit -> dumb reader':28} | {'OUR gate in repl_loop':46} | "
          f"{'VERIFIED' if ok else 'FAILED'}: flag forces plain reads")
    if not ok:
        failures.append(("--no-edit", out))

    print()
    if failures:
        print(f"{len(failures)} FAILED:")
        for name, out in failures:
            print(f"--- {name}:\n{out!r}\n")
        return 1
    print("every binding verified against the built binary.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
