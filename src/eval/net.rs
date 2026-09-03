//! The s39 net builtin family — blocking TCP v0 over `std::net` (is18).
//!
//! # Independence, stated where it lives
//!
//! Written against the prelude signatures (`net_listen(str) -> int ! {io}`;
//! `net_port(int) -> int ! {io}`; `net_accept(int) -> int ! {timeout, io}`;
//! `net_connect(str) -> int ! {refused, timeout, io}`; `net_read(int, int)
//! -> str ! {closed, timeout, utf8, io}`; `net_write(int, str) -> unit !
//! {closed, io}`; the s106 byte pair `net_read_bytes(int, int) ->
//! List[int] ! {closed, timeout, io}` and `net_write_bytes(int, List[int])
//! -> unit ! {closed, invalid, io}` (is30, wolf-interp#52 / wolf-std
//! F-0102 — no `utf8` row anywhere on the byte tier, and `invalid` is the
//! WHOLE pre-write check: an element outside 0..=255 rejects before
//! anything reaches the wire); `net_close(int) -> unit ! {io}`;
//! `net_deadline(int, int)
//! -> unit ! {io}`), the four corpus witnesses under `corpus/net/`, and
//! empirical probes of the compiled lanes — never `wolf_rt::net`. The
//! pinned facts:
//!
//! - errors are D30 ROWS, never traps: an unparseable or unbindable
//!   address is `io` (probed: `net_listen("garbage")` is `io`), a dial
//!   nobody answers is `refused` (witnessed), a forged or wrong-kind fd is
//!   `io` (probed: `net_read(99, …)` and `net_accept` on a stream are
//!   `io`), a double close is `io` (probed), the peer's finish is `closed`
//!   — both the EOF read and the broken-pipe write (probed: the FIRST
//!   write after a peer close lands in the socket buffer and succeeds; the
//!   second answers `closed`, which is TCP's own shape and both lanes sit
//!   on the same stack).
//! - `net_read(fd, n)` is ONE receive of up to `n` bytes (the witnesses
//!   read 16 and get shorter messages whole); `n <= 0` answers the empty
//!   string without touching the socket (never a false `closed`). The
//!   bytes VALIDATE: mis-encoded data is the recoverable `utf8` row, never
//!   a trap and never a forged `str`.
//! - `net_deadline(fd, ms)` arms (`ms > 0`) or clears (`ms <= 0`) a
//!   per-socket budget; every subsequent parking call on that socket
//!   resolves as its row's `timeout` tag when the budget fires first
//!   (witnessed: `read_deadline.lu`'s 40ms against a silent peer).
//!
//! # Scope: loopback + port 0
//!
//! The declared v0 surface this machine implements is the corpus's own
//! discipline — bind loopback at port 0 (the OS picks; no fixed ports),
//! dial loopback. Anything else — a non-loopback host, a fixed listen
//! port — is refused BY NAME (`unsupported`, never a guessed row): an
//! interpreter observing the open network would put the host into a
//! differential comparison, and a fixed port is a collision generator
//! across the corpus's many walks.
//!
//! # Blocking honesty
//!
//! The scheduler runs one task at a time on a baton, so a socket call
//! that parked its OS thread would park the whole machine. Every possibly
//! parking call therefore POLLS nonblocking sockets in a loop:
//!
//! - single-task: a 1ms host sleep between polls;
//! - task tier: a [`super::sched::Sched::net_yield`] between polls — the
//!   baton passes through an ordinary scheduling decision, so the peer
//!   that will resolve this accept/read gets to run
//!   (`net/spawn_accept.lu`'s design question, answered with the
//!   machine's own scheduling);
//! - an armed deadline resolves the poll as the `timeout` row when it
//!   fires first;
//! - with NO deadline, a poll that outlives [`NET_RAIL_MS`] declines
//!   (`unsupported`, the fuel posture): a verdict is refused rather than
//!   a hang, and never a wrong answer.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
// `[os.net.unix]`'s half only: a socket PATH exists where the family does.
// Unconditional, these are three unused imports on windows and `-D warnings`
// is a gate, not a preference.
#[cfg(unix)]
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crate::diag::Span;

use super::rules::Rule;
use super::value::{IntTy, Value};
use super::{Machine, Signal};

/// The no-deadline poll rail: generous against the corpus's 40ms budgets,
/// far under any CI timeout. Reaching it declines the verdict by name.
pub(crate) const NET_RAIL_MS: u64 = 10_000;

/// A row tag, ready for the builtin's error value.
type Row = &'static str;

/// One socket's poll answered, or not yet.
enum Poll<T> {
    Ready(T),
    NotYet,
}

/// What one net primitive resolved to.
type NetResult<T> = Result<T, NetErr>;

/// A row, or an honest by-name refusal.
#[derive(Debug)]
pub(crate) enum NetErr {
    /// A D30 row tag (`io`, `refused`, `closed`, `timeout`, `utf8`).
    Row(Row),
    /// Cancellation delivered at the blocking point: the call resolves to
    /// the same error value a channel op would ([conc.cancel.points]).
    Cancelled,
    /// Outside the declared v0 surface: refuse by name, never guess.
    Outside(String),
}

/// The sockets this run holds, by fd. A closed socket leaves its slot so
/// the fd stays spent — fds are never reused, and a forged one is `io`.
#[derive(Default)]
pub(crate) struct NetTable {
    slots: Vec<Option<NetSock>>,
}

struct NetSock {
    kind: SockKind,
    /// The armed per-socket budget, ms. `None` is unarmed.
    deadline_ms: Option<u64>,
}

enum SockKind {
    Listener(TcpListener),
    Stream(TcpStream),
    /// `[os.net.unix]` (s136, wolf-lang#227): an `AF_UNIX` stream listener,
    /// carrying the path it bound. The path rides the socket because the
    /// clause's cleanup posture is "the runtime created the socket file, so
    /// the runtime removes it" — `net_close` of a LISTENER unlinks, and
    /// nothing else does.
    #[cfg(unix)]
    UnixListener(std::os::unix::net::UnixListener, PathBuf),
    #[cfg(unix)]
    UnixStream(std::os::unix::net::UnixStream),
}

/// A connected socket, whichever family it belongs to.
///
/// `[os.net.unix]`: "The fd either answers is an ordinary net stream: …
/// `net_read`/`net_write`, the byte pair, `net_deadline` and `net_close`
/// serve it call for call." One trait object is that sentence — the read and
/// write polls below are written once and neither one knows the family.
trait Duplex: Read + Write {}
impl<T: Read + Write> Duplex for T {}

impl NetSock {
    /// The connected socket behind this fd, or `None` when the fd names a
    /// listener (which cannot read or write: the wrong-kind `io` row).
    fn duplex(&mut self) -> Option<&mut dyn Duplex> {
        match &mut self.kind {
            SockKind::Stream(stream) => Some(stream),
            #[cfg(unix)]
            SockKind::UnixStream(stream) => Some(stream),
            _ => None,
        }
    }
}

impl NetTable {
    fn fd(&mut self, sock: NetSock) -> i128 {
        self.slots.push(Some(sock));
        self.slots.len() as i128
    }

    fn slot(&mut self, fd: i128) -> Result<&mut Option<NetSock>, NetErr> {
        let index = usize::try_from(fd)
            .ok()
            .and_then(|fd| fd.checked_sub(1))
            .ok_or(NetErr::Row("io"))?;
        self.slots.get_mut(index).ok_or(NetErr::Row("io"))
    }

    fn sock(&mut self, fd: i128) -> Result<&mut NetSock, NetErr> {
        self.slot(fd)?.as_mut().ok_or(NetErr::Row("io"))
    }

    /// `net_listen`: bind loopback, port 0 only, nonblocking from birth.
    fn listen(&mut self, addr: &str) -> NetResult<i128> {
        let parsed: SocketAddr = addr.parse().map_err(|_| NetErr::Row("io"))?;
        if !parsed.ip().is_loopback() {
            return Err(NetErr::Outside(format!(
                "`net_listen(\"{addr}\")` binds a non-loopback address; the v0 surface \
                 this machine implements is loopback + port 0 only (the corpus's own \
                 discipline), so the shape is refused by name rather than observed"
            )));
        }
        if parsed.port() != 0 {
            return Err(NetErr::Outside(format!(
                "`net_listen(\"{addr}\")` binds a FIXED port; the v0 surface this machine \
                 implements is port 0 only (the OS picks), so the shape is refused by \
                 name rather than made a collision generator"
            )));
        }
        let listener = TcpListener::bind(parsed).map_err(|_| NetErr::Row("io"))?;
        listener
            .set_nonblocking(true)
            .map_err(|_| NetErr::Row("io"))?;
        Ok(self.fd(NetSock {
            kind: SockKind::Listener(listener),
            deadline_ms: None,
        }))
    }

    /// `net_port`: the socket's own (local) port.
    fn port(&mut self, fd: i128) -> NetResult<i128> {
        let sock = self.sock(fd)?;
        let addr = match &sock.kind {
            SockKind::Listener(listener) => listener.local_addr(),
            SockKind::Stream(stream) => stream.local_addr(),
            // `[os.net.unix]`: "`net_port` on it is `io` — a path has no
            // port." Stated by the clause rather than derived, so it is
            // answered here rather than left to a failing `local_addr`.
            #[cfg(unix)]
            SockKind::UnixListener(..) | SockKind::UnixStream(_) => return Err(NetErr::Row("io")),
        };
        addr.map(|addr| i128::from(addr.port()))
            .map_err(|_| NetErr::Row("io"))
    }

    /// `net_connect`: dial loopback. Blocking is fine here — a loopback
    /// dial resolves immediately (into the backlog, or as `refused`).
    fn connect(&mut self, addr: &str) -> NetResult<i128> {
        let parsed: SocketAddr = addr.parse().map_err(|_| NetErr::Row("io"))?;
        if !parsed.ip().is_loopback() {
            return Err(NetErr::Outside(format!(
                "`net_connect(\"{addr}\")` dials a non-loopback host; the v0 surface this \
                 machine implements is loopback only, so the shape is refused by name \
                 rather than put the open network into a differential comparison"
            )));
        }
        match TcpStream::connect(parsed) {
            Ok(stream) => {
                stream
                    .set_nonblocking(true)
                    .map_err(|_| NetErr::Row("io"))?;
                Ok(self.fd(NetSock {
                    kind: SockKind::Stream(stream),
                    deadline_ms: None,
                }))
            }
            Err(error) => Err(NetErr::Row(match error.kind() {
                std::io::ErrorKind::ConnectionRefused => "refused",
                std::io::ErrorKind::TimedOut => "timeout",
                _ => "io",
            })),
        }
    }

    /// One accept poll: the new stream's fd, or not yet.
    fn poll_accept(&mut self, fd: i128) -> NetResult<Poll<i128>> {
        let sock = self.sock(fd)?;
        let accepted = match &sock.kind {
            SockKind::Listener(listener) => match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(true)
                        .map_err(|_| NetErr::Row("io"))?;
                    SockKind::Stream(stream)
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(Poll::NotYet);
                }
                Err(_) => return Err(NetErr::Row("io")),
            },
            // `[os.net.unix]`: `net_accept` serves a unix listener call for
            // call, and the stream it answers is an ordinary net stream.
            #[cfg(unix)]
            SockKind::UnixListener(listener, _) => match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(true)
                        .map_err(|_| NetErr::Row("io"))?;
                    SockKind::UnixStream(stream)
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(Poll::NotYet);
                }
                Err(_) => return Err(NetErr::Row("io")),
            },
            // A stream cannot accept: wrong-kind fd, the `io` row (probed).
            _ => return Err(NetErr::Row("io")),
        };
        Ok(Poll::Ready(self.fd(NetSock {
            kind: accepted,
            deadline_ms: None,
        })))
    }

    /// One read poll: up to `n` bytes, validated. `Ok(0)` from the socket
    /// is the peer's finish — the `closed` row (probed).
    fn poll_read(&mut self, fd: i128, n: usize) -> NetResult<Poll<String>> {
        match self.poll_read_bytes(fd, n)? {
            Poll::NotYet => Ok(Poll::NotYet),
            Poll::Ready(buf) => match String::from_utf8(buf) {
                Ok(text) => Ok(Poll::Ready(text)),
                // Bytes off a socket are data; mis-encoded data is a
                // recoverable outcome, exactly like `str_from_utf8`.
                Err(_) => Err(NetErr::Row("utf8")),
            },
        }
    }

    /// One receive poll of up to `n` RAW bytes — the byte tier's read
    /// (s106, F-0102). No `utf8` row anywhere: a lone `0x80` is data.
    fn poll_read_bytes(&mut self, fd: i128, n: usize) -> NetResult<Poll<Vec<u8>>> {
        let sock = self.sock(fd)?;
        let Some(stream) = sock.duplex() else {
            return Err(NetErr::Row("io"));
        };
        let mut buf = vec![0u8; n];
        match stream.read(&mut buf) {
            Ok(0) => Err(NetErr::Row("closed")),
            Ok(read) => {
                buf.truncate(read);
                Ok(Poll::Ready(buf))
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(Poll::NotYet),
            Err(error) => Err(NetErr::Row(closed_or_io(&error))),
        }
    }

    /// One write poll from `at`: how far the write is now, or done.
    fn poll_write(&mut self, fd: i128, bytes: &[u8], at: &mut usize) -> NetResult<Poll<()>> {
        let sock = self.sock(fd)?;
        let Some(stream) = sock.duplex() else {
            return Err(NetErr::Row("io"));
        };
        while *at < bytes.len() {
            match stream.write(&bytes[*at..]) {
                Ok(0) => return Err(NetErr::Row("closed")),
                Ok(wrote) => *at += wrote,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(Poll::NotYet);
                }
                Err(error) => return Err(NetErr::Row(closed_or_io(&error))),
            }
        }
        Ok(Poll::Ready(()))
    }

    /// `net_close`: the fd is spent; closing it again is `io` (probed).
    ///
    /// `[os.net.unix]`'s cleanup posture rides here: "the runtime created the
    /// socket file, so the runtime removes it — `net_close` of a unix
    /// LISTENER unlinks its path; a stream's close never touches the path".
    /// A process that dies without closing leaves the file, which the next
    /// bind refuses as `exists`, and that is the nginx/haproxy posture the
    /// clause states rather than implies.
    fn close(&mut self, fd: i128) -> NetResult<()> {
        let slot = self.slot(fd)?;
        let Some(sock) = slot.take() else {
            return Err(NetErr::Row("io"));
        };
        #[cfg(unix)]
        if let SockKind::UnixListener(listener, path) = sock.kind {
            // Drop the listener FIRST so the fd is gone before the path is,
            // then unlink. A failed unlink is not the caller's error — the
            // socket is closed either way, and the clause makes the leftover
            // file the next bind's `exists`.
            drop(listener);
            let _ = std::fs::remove_file(&path);
        }
        // No unix family here, so the socket is only a socket: dropping it is
        // the whole of its close, and naming the drop keeps `-D warnings`
        // honest on a host where the arm above does not exist.
        #[cfg(not(unix))]
        drop(sock);
        Ok(())
    }

    /// `net_listen_unix` (`[os.net.unix]`, s136/wolf-lang#227): bind and
    /// listen an `AF_UNIX` stream socket at `path`.
    ///
    /// The row vocabulary distinguishes the two things a caller must tell
    /// apart, which is what #227 was filed about. **`unsupported` is the
    /// HOST** — the runtime does not serve the family there, refused by name
    /// and never a bare `io`; every other row is the PATH: an existing path
    /// is `exists` (a stale socket file is the operator's to remove — the
    /// runtime never clobbers a path it did not bind), a missing directory
    /// `not_found`, a permission the caller lacks `denied`.
    #[cfg(unix)]
    fn listen_unix(&mut self, path: &str) -> NetResult<i128> {
        let path = socket_path(path, "net_listen_unix")?;
        // "an existing path is `exists`" — asked BEFORE the bind, because
        // `bind(2)` answers `EADDRINUSE` for a live socket and for a stale
        // file alike, and the clause makes both the same row anyway.
        if path.symlink_metadata().is_ok() {
            return Err(NetErr::Row("exists"));
        }
        let listener =
            std::os::unix::net::UnixListener::bind(&path).map_err(|e| unix_bind_row(&e))?;
        listener
            .set_nonblocking(true)
            .map_err(|_| NetErr::Row("io"))?;
        Ok(self.fd(NetSock {
            kind: SockKind::UnixListener(listener, path),
            deadline_ms: None,
        }))
    }

    /// `net_connect_unix` (`[os.net.unix]`): dial an `AF_UNIX` stream socket.
    /// At dial, no socket file is `not_found`, a file nobody listens on is
    /// `refused`, and `denied` is the permission the caller lacks.
    #[cfg(unix)]
    fn connect_unix(&mut self, path: &str) -> NetResult<i128> {
        let path = socket_path(path, "net_connect_unix")?;
        let stream =
            std::os::unix::net::UnixStream::connect(&path).map_err(|e| unix_dial_row(&e))?;
        stream
            .set_nonblocking(true)
            .map_err(|_| NetErr::Row("io"))?;
        Ok(self.fd(NetSock {
            kind: SockKind::UnixStream(stream),
            deadline_ms: None,
        }))
    }

    /// `net_deadline`: arm (`ms > 0`) or clear (`ms <= 0`) the budget.
    fn deadline(&mut self, fd: i128, ms: i128) -> NetResult<()> {
        let sock = self.sock(fd)?;
        sock.deadline_ms = if ms > 0 {
            Some(u64::try_from(ms).unwrap_or(u64::MAX))
        } else {
            None
        };
        Ok(())
    }

    /// The budget the parking loop honors for this fd right now.
    fn armed(&mut self, fd: i128) -> Option<u64> {
        self.slot(fd).ok()?.as_ref()?.deadline_ms
    }
}

/// The socket path a unix-domain call may bind or dial.
///
/// The v0 discipline the TCP family states as "loopback + port 0" has a
/// path-shaped twin: a socket path is a host filesystem object, and this
/// machine declines the host's filesystem by design (wolf-interp#18 item 6,
/// `[proto.cmp.defined-divergence]` — an interpreter observing the HOST's
/// filesystem puts the host into a differential comparison). Binding an
/// arbitrary absolute path would walk straight through that posture, so the
/// admitted shape is a RELATIVE path that does not climb out of the working
/// directory — `target/x.sock`, which is what the corpus witness writes.
/// Anything else is refused BY NAME (`unsupported`, the by-name refusal and
/// never the `unsupported` ROW, which would be a lie about the host).
#[cfg(unix)]
fn socket_path(path: &str, name: &str) -> NetResult<PathBuf> {
    let candidate = Path::new(path);
    let escapes = candidate.is_absolute()
        || candidate.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
    if escapes {
        return Err(NetErr::Outside(format!(
            "`{name}(\"{path}\")` names a path outside the working directory; a unix socket \
             path is a host filesystem object, and this machine's declined fs surface \
             (wolf-interp#18 item 6) admits only a relative path that does not climb out — \
             the shape is refused by name rather than observed"
        )));
    }
    Ok(candidate.to_path_buf())
}

/// `[os.net.unix]`'s bind rows: a missing directory is `not_found`, a
/// permission the caller lacks is `denied`, an existing path is `exists`
/// (asked before the bind, so this arm sees it only from a race).
#[cfg(unix)]
fn unix_bind_row(error: &std::io::Error) -> NetErr {
    NetErr::Row(match error.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "denied",
        std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::AddrInUse => "exists",
        _ => "io",
    })
}

/// `[os.net.unix]`'s dial rows: no socket file is `not_found`, a file nobody
/// listens on is `refused`, `denied` as at bind.
#[cfg(unix)]
fn unix_dial_row(error: &std::io::Error) -> NetErr {
    NetErr::Row(match error.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "denied",
        std::io::ErrorKind::ConnectionRefused => "refused",
        _ => "io",
    })
}

/// `closed` for the peer-finish error kinds, `io` for the rest.
fn closed_or_io(error: &std::io::Error) -> Row {
    match error.kind() {
        std::io::ErrorKind::BrokenPipe
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::ConnectionAborted => "closed",
        _ => "io",
    }
}

impl Machine {
    /// One net builtin call, by name — the `builtin::call` arm delegates
    /// here so the family reads as one unit.
    ///
    /// # Errors
    ///
    /// `Signal::Unsupported` for shapes outside the declared surface (and
    /// arguments no typed program produces); everything else is a row
    /// VALUE, never a trap.
    pub(crate) fn net_call(&mut self, name: &str, args: &[Value], span: Span) -> EvalOut {
        match name {
            "net_listen" | "net_connect" => {
                let Some(Value::Str(addr)) = args.first() else {
                    return Err(Signal::Unsupported(format!(
                        "`{name}` takes an address `str` like \"127.0.0.1:0\""
                    )));
                };
                let answer = if name == "net_listen" {
                    self.net().listen(addr)
                } else {
                    self.net().connect(addr)
                };
                self.net_answer(name, answer.map(|fd| Value::Int(fd, IntTy::INT)), span)
            }
            "net_listen_unix" | "net_connect_unix" => {
                let Some(Value::Str(path)) = args.first() else {
                    return Err(Signal::Unsupported(format!(
                        "`{name}` takes a socket path `str` like \"target/x.sock\""
                    )));
                };
                // `[os.net.unix]`'s host posture, this machine's half:
                // linux and macOS serve the family, and every other host
                // answers the `unsupported` ROW — refused BY NAME, never a
                // bare `io`, which is the distinction #227 was filed to make
                // measurable from inside a program.
                #[cfg(unix)]
                let answer = if name == "net_listen_unix" {
                    self.net().listen_unix(path)
                } else {
                    self.net().connect_unix(path)
                };
                #[cfg(not(unix))]
                let answer = {
                    let _ = path;
                    Err::<i128, NetErr>(NetErr::Row("unsupported"))
                };
                self.net_answer(name, answer.map(|fd| Value::Int(fd, IntTy::INT)), span)
            }
            "net_port" => {
                let fd = int_arg(args, 0, name)?;
                let answer = self.net().port(fd);
                self.net_answer(name, answer.map(|port| Value::Int(port, IntTy::INT)), span)
            }
            "net_accept" => {
                let fd = int_arg(args, 0, name)?;
                let answer = self.net_park(fd, span, |table| table.poll_accept(fd))?;
                self.net_answer(name, answer.map(|fd| Value::Int(fd, IntTy::INT)), span)
            }
            "net_read" => {
                let fd = int_arg(args, 0, name)?;
                let n = int_arg(args, 1, name)?;
                if n <= 0 {
                    // Zero bytes wanted: the empty string, and the socket is
                    // never asked (a 0-length receive would forge `closed`).
                    return Ok(Value::Str(String::new()));
                }
                let n = usize::try_from(n).unwrap_or(usize::MAX).min(1 << 20);
                let answer = self.net_park(fd, span, |table| table.poll_read(fd, n))?;
                let answer = match answer {
                    Ok(text) => {
                        self.allocate(
                            span,
                            "net_read",
                            super::region::ledger::str_bytes(text.len() as u64),
                        )?;
                        Ok(Value::Str(text))
                    }
                    Err(err) => Err(err),
                };
                self.net_answer(name, answer, span)
            }
            "net_write" => {
                let fd = int_arg(args, 0, name)?;
                let Some(Value::Str(text)) = args.get(1) else {
                    return Err(Signal::Unsupported(format!(
                        "`{name}` takes an fd and a `str` payload"
                    )));
                };
                let bytes = text.clone().into_bytes();
                let mut at = 0usize;
                let answer =
                    self.net_park(fd, span, move |table| table.poll_write(fd, &bytes, &mut at))?;
                self.net_answer(name, answer.map(|()| Value::Unit), span)
            }
            "net_read_bytes" => {
                // The byte tier's read (s106; wolf-interp#52 / wolf-std
                // F-0102): one receive of up to `n` RAW bytes as
                // `List[byte]` (s136, wolf-lang#231 — `List[int]` from s115
                // to s135). The str call's own shape minus the `utf8` row —
                // bytes are data and validate nothing — and the read's result
                // is the write's argument BY TYPE, so an echo converts
                // nothing (`corpus/net/echo_bytes.lu`).
                let fd = int_arg(args, 0, name)?;
                let n = int_arg(args, 1, name)?;
                if n <= 0 {
                    // Zero bytes wanted: the empty list, and the socket is
                    // never asked (a 0-length receive would forge `closed`).
                    return Ok(Value::list(Vec::new(), None, Some(self.current_region())));
                }
                let n = usize::try_from(n).unwrap_or(usize::MAX).min(1 << 20);
                let answer = self.net_park(fd, span, |table| table.poll_read_bytes(fd, n))?;
                let answer = match answer {
                    Ok(bytes) => {
                        // Minted at EXACT capacity: a reader knows its length
                        // before it mints, so it pays no growth history —
                        // `[type.byte]`'s 1-byte stride and nothing else.
                        let home = self.allocate(
                            span,
                            "net_read_bytes",
                            super::region::ledger::byte_buffer_bytes(bytes.len() as u64),
                        )?;
                        Ok(Value::list(
                            bytes
                                .into_iter()
                                .map(|b| super::value::Slot::live(Value::Byte(b)))
                                .collect(),
                            None,
                            Some(home),
                        ))
                    }
                    Err(err) => Err(err),
                };
                self.net_answer(name, answer, span)
            }
            "net_write_bytes" => {
                // The byte tier's write (s106; wolf-interp#52 / wolf-std
                // F-0102): the whole `List[byte]` is written or the call
                // raises (s136, wolf-lang#231 — `List[int]` before). The
                // `invalid` row survives the retype without a caller who can
                // reach it from typed source: an element outside `0..=255` is
                // now unrepresentable, since a `byte` IS the octet, so the
                // pre-write check answers for the untyped shapes machinery can
                // still build. When it does fire it is still WHOLE — nothing
                // reaches the wire, no mask, no truncation, no partial send.
                let fd = int_arg(args, 0, name)?;
                let Some(slots) = args.get(1).and_then(Value::seq_slots) else {
                    return Err(Signal::Unsupported(format!(
                        "`{name}` takes an fd and a `List[byte]` payload"
                    )));
                };
                let mut bytes = Vec::with_capacity(slots.len());
                for slot in slots {
                    match &slot.value {
                        Value::Byte(b) => bytes.push(*b),
                        Value::Int(v, _) if (0..=255).contains(v) => {
                            bytes.push(u8::try_from(*v).expect("checked 0..=255"));
                        }
                        Value::Int(..) => {
                            return self.net_answer(name, Err(NetErr::Row("invalid")), span);
                        }
                        other => {
                            return Err(Signal::Unsupported(format!(
                                "`{name}`'s payload elements must be bytes, got {}",
                                other.kind()
                            )));
                        }
                    }
                }
                let mut at = 0usize;
                let answer =
                    self.net_park(fd, span, move |table| table.poll_write(fd, &bytes, &mut at))?;
                self.net_answer(name, answer.map(|()| Value::Unit), span)
            }
            "net_close" => {
                let fd = int_arg(args, 0, name)?;
                let answer = self.net().close(fd);
                self.net_answer(name, answer.map(|()| Value::Unit), span)
            }
            "net_deadline" => {
                let fd = int_arg(args, 0, name)?;
                let ms = int_arg(args, 1, name)?;
                let answer = self.net().deadline(fd, ms);
                self.net_answer(name, answer.map(|()| Value::Unit), span)
            }
            other => Err(Signal::Unsupported(format!(
                "`{other}` is not a net builtin this machine knows"
            ))),
        }
    }

    /// A row becomes an error VALUE with a note; a by-name refusal becomes
    /// the honest `unsupported`.
    fn net_answer(&mut self, name: &str, answer: NetResult<Value>, span: Span) -> EvalOut {
        match answer {
            Ok(value) => Ok(value),
            Err(NetErr::Row(tag)) => {
                self.note(
                    Rule::ErrUnion,
                    span,
                    &format!("`{name}` yields the `{tag}` row"),
                );
                // The tag rides with the raising builtin's whole declared
                // row so a downstream handler's arms discriminate
                // (wolf-interp#47, wolf-std F-0097).
                Ok(super::builtin::error_value(name, tag))
            }
            Err(NetErr::Cancelled) => Ok(super::sched::cancelled_error()),
            Err(NetErr::Outside(reason)) => Err(Signal::Unsupported(reason)),
        }
    }

    /// The parking loop (module doc, "Blocking honesty"): poll until ready,
    /// a row, the fd's armed deadline (the `timeout` row), or the rail.
    fn net_park<T>(
        &mut self,
        fd: i128,
        span: Span,
        mut poll: impl FnMut(&mut NetTable) -> NetResult<Poll<T>>,
    ) -> Result<NetResult<T>, Signal> {
        let started = Instant::now();
        let budget = self.net().armed(fd).map(Duration::from_millis);
        loop {
            match poll(&mut self.net()) {
                Ok(Poll::Ready(value)) => return Ok(Ok(value)),
                Err(err) => return Ok(Err(err)),
                Ok(Poll::NotYet) => {}
            }
            if let Some(budget) = budget
                && started.elapsed() >= budget
            {
                // The armed budget fired first: the parking call resolves
                // as its row's `timeout` tag.
                return Ok(Err(NetErr::Row("timeout")));
            }
            if started.elapsed() >= Duration::from_millis(NET_RAIL_MS) {
                // No deadline and nothing will resolve this: decline the
                // verdict rather than hang, and never invent an answer.
                return Err(Signal::Unsupported(format!(
                    "a net call parked past the machine's {NET_RAIL_MS}ms rail with no \
                     armed deadline; a verdict is declined rather than a hang"
                )));
            }
            if self.concurrent() {
                // The task tier: hand the baton through an ordinary
                // scheduling decision so the peer can make progress. A
                // cancellation or kill racing in resolves exactly as it
                // would at a channel's blocking point.
                let wake = self.sched_block(|sched, task| sched.net_yield(task));
                match wake {
                    super::sched::Wake::Killed => return Err(Signal::ProcKilled),
                    super::sched::Wake::Cancelled => {
                        self.fire(
                            Rule::CancelPoint,
                            span,
                            "cancellation delivered at a net blocking point",
                        );
                        return Ok(Err(NetErr::Cancelled));
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// The `int` argument at `at`, or the honest refusal.
fn int_arg(args: &[Value], at: usize, name: &str) -> Result<i128, Signal> {
    match args.get(at) {
        Some(Value::Int(v, _)) => Ok(*v),
        other => Err(Signal::Unsupported(format!(
            "`{name}`'s argument {at} must be an integer, got {}",
            other.map_or_else(|| "nothing".to_owned(), |v| v.kind())
        ))),
    }
}

/// What a builtin arm hands back to `eval_call`.
type EvalOut = Result<Value, Signal>;

#[cfg(test)]
mod tests {
    use super::*;

    fn ready<T>(answer: NetResult<Poll<T>>) -> Option<T> {
        match answer {
            Ok(Poll::Ready(value)) => Some(value),
            _ => None,
        }
    }

    /// Poll until ready or a row, with a test-local budget — the machine's
    /// parking loop, miniaturized for table-level tests.
    fn poll_until<T>(mut poll: impl FnMut() -> NetResult<Poll<T>>) -> NetResult<T> {
        let started = std::time::Instant::now();
        loop {
            match poll() {
                Ok(Poll::Ready(value)) => return Ok(value),
                Err(err) => return Err(err),
                Ok(Poll::NotYet) => {
                    assert!(
                        started.elapsed() < Duration::from_secs(10),
                        "test poll outlived its budget"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    fn row<T>(answer: NetResult<T>) -> Row {
        match answer {
            Err(NetErr::Row(tag)) => tag,
            Err(NetErr::Outside(reason)) => panic!("refusal, not a row: {reason}"),
            Err(NetErr::Cancelled) => panic!("cancelled, not a row"),
            Ok(_) => panic!("an answer, not a row"),
        }
    }

    #[test]
    fn bytes_off_the_socket_validate_and_mis_encoded_data_is_the_utf8_row() {
        // The one row the wolf surface cannot reach through `net_write`
        // (which only sends valid `str`): a raw peer sends a lone
        // continuation byte, and the read answers `utf8` — a recoverable
        // VALUE, never a trap and never a forged `str`.
        let mut table = NetTable::default();
        let srv = table.listen("127.0.0.1:0").expect("binds");
        let port = table.port(srv).expect("a port");
        let mut raw =
            TcpStream::connect(("127.0.0.1", u16::try_from(port).expect("fits"))).expect("dials");
        raw.write_all(&[0xFF, 0xFE]).expect("raw bytes sent");
        let conn = poll_until(|| table.poll_accept(srv)).expect("accepts");
        assert_eq!(row(poll_until(|| table.poll_read(conn, 16))), "utf8");
    }

    #[test]
    fn the_peer_finishing_is_the_closed_row_on_read() {
        let mut table = NetTable::default();
        let srv = table.listen("127.0.0.1:0").expect("binds");
        let port = table.port(srv).expect("a port");
        let raw =
            TcpStream::connect(("127.0.0.1", u16::try_from(port).expect("fits"))).expect("dials");
        let conn = poll_until(|| table.poll_accept(srv)).expect("accepts");
        drop(raw);
        assert_eq!(row(poll_until(|| table.poll_read(conn, 16))), "closed");
    }

    #[test]
    fn fds_are_spent_never_reused_and_forged_ones_are_io() {
        let mut table = NetTable::default();
        let srv = table.listen("127.0.0.1:0").expect("binds");
        table.close(srv).expect("closes");
        assert_eq!(row(table.close(srv)), "io");
        assert_eq!(row(table.port(srv)), "io");
        assert_eq!(row(table.port(99)), "io");
        assert_eq!(row(table.port(0)), "io");
        assert_eq!(row(table.port(-1)), "io");
        // A fresh socket gets a fresh fd, not the spent one.
        let next = table.listen("127.0.0.1:0").expect("binds");
        assert_ne!(next, srv);
    }

    #[test]
    fn accept_on_a_stream_and_read_on_a_listener_are_the_io_row() {
        let mut table = NetTable::default();
        let srv = table.listen("127.0.0.1:0").expect("binds");
        let port = table.port(srv).expect("a port");
        let cli = table.connect(&format!("127.0.0.1:{port}")).expect("dials");
        assert_eq!(row(table.poll_accept(cli).map(|p| ready(Ok(p)))), "io");
        assert_eq!(row(table.poll_read(srv, 4).map(|p| ready(Ok(p)))), "io");
    }

    #[test]
    fn the_scope_refusals_name_their_shape_instead_of_guessing_a_row() {
        let mut table = NetTable::default();
        // Unparseable is the io ROW (probed: the compiled lanes answer io).
        assert_eq!(row(table.listen("garbage")), "io");
        // Non-loopback and fixed ports refuse BY NAME.
        let outside = |answer: NetResult<i128>| match answer {
            Err(NetErr::Outside(reason)) => reason,
            _ => panic!("expected a by-name refusal"),
        };
        assert!(outside(table.listen("0.0.0.0:0")).contains("non-loopback"));
        assert!(outside(table.listen("127.0.0.1:8080")).contains("FIXED port"));
        assert!(outside(table.connect("8.8.8.8:53")).contains("non-loopback"));
    }

    #[test]
    fn deadlines_arm_and_clear_per_socket() {
        let mut table = NetTable::default();
        let srv = table.listen("127.0.0.1:0").expect("binds");
        assert_eq!(table.armed(srv), None);
        table.deadline(srv, 40).expect("arms");
        assert_eq!(table.armed(srv), Some(40));
        table.deadline(srv, -5).expect("clears");
        assert_eq!(table.armed(srv), None);
        assert_eq!(row(table.deadline(99, 40)), "io");
    }
}
