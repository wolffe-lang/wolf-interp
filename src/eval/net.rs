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
//! -> unit ! {io}`; the s141 pair whose clauses the SPEC pins outright —
//! `net_writev(int, List[List[byte]]) -> unit ! {closed, io}` and
//! `net_nodelay(int, bool) -> unit ! {io}`, `[os.net.writev]` and
//! `[os.net.nodelay]`), the corpus witnesses under `corpus/net/`, and
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
//!   (witnessed: `read_deadline.lu`'s 40ms against a silent peer) — or, on
//!   a call whose declared row has no `timeout`, as `io`. `[os.net.io]`
//!   states that coarsening ("its `timeout` coarsened, as the call
//!   declares") and `budget_row` is where it happens; the three writes are
//!   the calls it applies to, because that clause is what put a write's
//!   whole drain under the budget.
//! - `[os.net.nodelay]`: every TCP stream this table mints — accepted,
//!   dialed, or taken off the queue by a `net_wait` — has `TCP_NODELAY`
//!   set. `adopt_tcp` is the one place that happens, so the default cannot
//!   be forgotten at a fourth mint site.
//! - `[os.net.io]`, the posture half: this machine POLLS the syscall first
//!   and waits only on not-yet, which is what the clause names it doing.
//!   That half cost no source motion, exactly as `[os.net.accept]` did not
//!   at the previous pin; the budget coarsening above is the half that did.
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

use std::collections::VecDeque;
use std::io::{IoSlice, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
// `[os.net.unix]`'s half only: a socket PATH exists where the family does.
// Unconditional, these are three unused imports on windows and `-D warnings`
// is a gate, not a preference.
#[cfg(unix)]
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crate::diag::Span;

use super::rules::Rule;
use super::value::{ElemTy, IntTy, Value};
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
    /// `[os.net.listen.opts]`: this listener was bound with `reuse_port`,
    /// so it is a member of a GROUP and a later bind of its address joins
    /// the group instead of colliding with it. False on every stream and on
    /// every option-less listener, which is `net_listen`'s own posture.
    reuse_port: bool,
    /// `[os.net.wait]`'s level-triggered readiness, on a listener.
    ///
    /// This machine has no `poll(2)` to ask (no `unsafe`, and `std::net`
    /// exposes no readiness surface), so a listener's "can `net_accept`
    /// proceed without blocking?" is answered by ACCEPTING and holding the
    /// connection here. That keeps the clause's two sentences true where a
    /// program can see them: asking consumes nothing (a second wait finds
    /// the same stashed connection and answers the same), and the accept
    /// that follows drains it. `net_accept` takes from this queue before it
    /// touches the socket, and closing the listener drops whatever is left
    /// — which is what closing a listener does to its backlog anyway.
    pending: VecDeque<SockKind>,
    /// The same question on a STREAM, and the same answer one asymmetry
    /// further down.
    ///
    /// `TcpStream::peek` is stable and answers "is there anything to read?"
    /// without taking it, which is the whole of the TCP half. `UnixStream`'s
    /// `peek` is not stable, so the unix half asks by READING and holds what
    /// it found here; `poll_read_bytes` drains this before it touches the
    /// socket, so the byte stream a program sees is unchanged and the
    /// clause's sentence stays true — asking consumes nothing a program can
    /// observe. `eof` is the same trick for the peer's finish: the read that
    /// follows is still the one that reports `closed`.
    held: Vec<u8>,
    eof: bool,
}

impl NetSock {
    /// A fresh socket slot: unarmed, in no group, nothing stashed.
    fn new(kind: SockKind) -> Self {
        Self {
            kind,
            deadline_ms: None,
            reuse_port: false,
            pending: VecDeque::new(),
            held: Vec::new(),
            eof: false,
        }
    }
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

/// `[os.net.nodelay]`, the DEFAULT half: "every TCP stream the runtime hands
/// a program — accepted or dialed — has `TCP_NODELAY` set."
///
/// Every TCP stream this table mints goes through here, so the default is one
/// function rather than three call sites that have to remember. The
/// non-blocking flag rides along because `[os.net.io]` puts it in the same
/// sentence ("the runtime sets the flag on acquisition rather than trusting a
/// kernel's inheritance rule"), and this machine has always set it: the pair
/// is what "acquisition" means here.
///
/// A unix-domain stream is NOT minted here on purpose. The option is TCP's,
/// and a `UnixStream` has no Nagle to turn off.
fn adopt_tcp(stream: TcpStream) -> NetResult<SockKind> {
    stream
        .set_nonblocking(true)
        .map_err(|_| NetErr::Row("io"))?;
    stream.set_nodelay(true).map_err(|_| NetErr::Row("io"))?;
    Ok(SockKind::Stream(stream))
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
        Ok(self.fd(NetSock::new(SockKind::Listener(listener))))
    }

    /// `net_listen_with` (`[os.net.listen.opts]`, s137/wolf-lang#234): the
    /// two options a prefork server needs, on the ordinary listener.
    ///
    /// # The group is MODELLED, and the model is exactly the clause's two
    /// guarantees
    ///
    /// `SO_REUSEPORT` has to be set *before* the bind, and there is no way
    /// to reach `setsockopt` from `std` — this crate forbids `unsafe`, so
    /// the option itself is unreachable here. What the clause actually
    /// promises a caller is narrower than the option: **every dial is
    /// accepted by SOME member, and the survivor takes every dial after the
    /// others close.** What the kernel does *inside* a live group is
    /// explicitly the host's (linux hashes the 4-tuple across the group,
    /// macOS hands every SYN to the newest bound socket) and a program may
    /// not depend on it, which is why `corpus/net/reuse_port.lu` asserts
    /// the two guarantees and prints nothing about delivery.
    ///
    /// So a group here is ONE listening socket with a duplicated handle per
    /// member (`TcpListener::try_clone`) — the inherit shape from
    /// `[os.proc.inherit]`, one queue with several accepters — and both
    /// guarantees hold by construction: every member accepts from the same
    /// queue, and the queue outlives any member that closes. It names no
    /// host, because a model has no host to name; the runtime's own
    /// `unsupported` row on windows is the option's, not the guarantee's.
    ///
    /// # Why this call admits a FIXED port where `net_listen` does not
    ///
    /// `net_listen`'s v0 surface is loopback + port 0, so a fixed port is
    /// refused by name and never a collision generator. But `exists` and
    /// `denied` are rows about a NAMED port — "another socket already holds
    /// this address", "you may not have this one" — and a machine that
    /// refuses every fixed port can never answer either. The clause hands
    /// this call those two rows precisely so a server can tell them apart,
    /// so the option call admits a fixed LOOPBACK port; a caller reaches it
    /// with a number the OS gave it (`net_port` of a port-0 bind), which is
    /// what the witness does. Non-loopback stays refused by name.
    ///
    /// `backlog` is the `listen(2)` queue hint. `std::net` binds and listens
    /// in one call with its own depth and exposes no setter, so the hint is
    /// accepted and not applied — no observation in the protocol can see a
    /// queue depth, and `<= 0` asks for the default anyway.
    fn listen_with(&mut self, addr: &str, reuse_port: bool, backlog: i128) -> NetResult<i128> {
        let _ = backlog;
        let parsed: SocketAddr = addr.parse().map_err(|_| NetErr::Row("io"))?;
        if !parsed.ip().is_loopback() {
            return Err(NetErr::Outside(format!(
                "`net_listen_with(\"{addr}\", …)` binds a non-loopback address; the v0 \
                 surface this machine implements is loopback only (the corpus's own \
                 discipline), so the shape is refused by name rather than observed"
            )));
        }
        if reuse_port
            && parsed.port() != 0
            && let Some(member) = self.join_group(parsed)
        {
            return Ok(member);
        }
        let listener = TcpListener::bind(parsed).map_err(|error| NetErr::Row(bind_row(&error)))?;
        listener
            .set_nonblocking(true)
            .map_err(|_| NetErr::Row("io"))?;
        let mut sock = NetSock::new(SockKind::Listener(listener));
        sock.reuse_port = reuse_port;
        Ok(self.fd(sock))
    }

    /// A second hand on the group holding `addr`, or `None` when no member
    /// of this run holds it under the option (in which case the caller
    /// binds, and a foreign holder answers `exists`).
    fn join_group(&mut self, addr: SocketAddr) -> Option<i128> {
        let cloned = self.slots.iter().find_map(|slot| {
            let sock = slot.as_ref()?;
            if !sock.reuse_port {
                return None;
            }
            let SockKind::Listener(listener) = &sock.kind else {
                return None;
            };
            if listener.local_addr().ok()? != addr {
                return None;
            }
            listener.try_clone().ok()
        })?;
        cloned.set_nonblocking(true).ok()?;
        let mut sock = NetSock::new(SockKind::Listener(cloned));
        sock.reuse_port = true;
        Some(self.fd(sock))
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
            // `[os.net.nodelay]`: a DIALED stream carries the default too,
            // which is the half a client-side witness sees.
            Ok(stream) => Ok(self.fd(NetSock::new(adopt_tcp(stream)?))),
            Err(error) => Err(NetErr::Row(match error.kind() {
                std::io::ErrorKind::ConnectionRefused => "refused",
                std::io::ErrorKind::TimedOut => "timeout",
                _ => "io",
            })),
        }
    }

    /// One accept poll: the new stream's fd, or not yet.
    ///
    /// A connection a `net_wait` already took off the queue to answer its
    /// readiness question is handed over FIRST — that is what makes
    /// `[os.net.wait]`'s "the accept drains it" true here.
    fn poll_accept(&mut self, fd: i128) -> NetResult<Poll<i128>> {
        if let Some(stashed) = self.sock(fd)?.pending.pop_front() {
            return Ok(Poll::Ready(self.fd(NetSock::new(stashed))));
        }
        let sock = self.sock(fd)?;
        let accepted = match &sock.kind {
            SockKind::Listener(listener) => match listener.accept() {
                Ok((stream, _)) => adopt_tcp(stream)?,
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
        Ok(Poll::Ready(self.fd(NetSock::new(accepted))))
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
        // Whatever a `net_wait` had to take in order to answer comes back
        // first, in order, before the socket is asked again (see
        // [`NetSock::held`]). Empty on every TCP socket, always.
        if !sock.held.is_empty() {
            let take = n.min(sock.held.len());
            return Ok(Poll::Ready(sock.held.drain(..take).collect()));
        }
        if sock.eof {
            return Err(NetErr::Row("closed"));
        }
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

    /// One vectored write poll from `cursor`: `[os.net.writev]`.
    ///
    /// The clause makes this `net_write` "in every other respect", so the
    /// shape is `poll_write`'s: drive the syscall until it drains or the
    /// kernel says would-block, and resume from where the kernel stopped.
    /// The cursor is `(part, byte in that part)` — "the drain resumed from
    /// the byte the kernel stopped at in whichever part it stopped in" —
    /// because a gather can stop in the middle of any part and the next poll
    /// must present the remainder of THAT part first.
    ///
    /// Empty parts are permitted and send nothing: they are skipped rather
    /// than handed to the kernel (a zero-length `IoSlice` would make a
    /// `write_vectored` that took nothing indistinguishable from a stalled
    /// one), and a call whose parts are all empty completes without a
    /// syscall — the loop's first skip walks off the end.
    ///
    /// **One row differs from `poll_write` and the clause says which.**
    /// `write_vectored` answering `Ok(0)` with bytes still to send is std's
    /// `WriteZero`, and `[os.net.writev]` gives this call `net_write`'s rows
    /// "exactly — `closed` for a peer that has gone, `io` for the rest".
    /// The compiler's tables answer `io` there (wolf-interp#67), so `io` is
    /// the row here; `poll_write`'s `Ok(0)` stays `closed`, which is the
    /// peer's finish on a single-buffer write.
    fn poll_writev(
        &mut self,
        fd: i128,
        parts: &[Vec<u8>],
        cursor: &mut (usize, usize),
    ) -> NetResult<Poll<()>> {
        let sock = self.sock(fd)?;
        // A listener (of either family) has no write: the `io` row, which is
        // `listener_is_io` in `corpus/net/writev_gather.lu`.
        let Some(stream) = sock.duplex() else {
            return Err(NetErr::Row("io"));
        };
        drive_writev(stream, parts, cursor)
    }

    /// `[os.net.nodelay]`'s call: set `TCP_NODELAY` either way on a TCP
    /// stream.
    ///
    /// "a listener, a unix-domain stream (the option is TCP's), a forged or a
    /// closed handle is `io`" — all four land on the same row here, three of
    /// them by the match falling through and the fourth by [`NetTable::sock`].
    fn nodelay(&mut self, fd: i128, on: bool) -> NetResult<()> {
        match &self.sock(fd)?.kind {
            SockKind::Stream(stream) => stream.set_nodelay(on).map_err(|_| NetErr::Row("io")),
            _ => Err(NetErr::Row("io")),
        }
    }

    /// `[os.net.wait]`, one socket: can this fd be READ without blocking?
    ///
    /// A listener is ready when an accept would not block; a stream when a
    /// read would answer bytes **or `closed`**. Both halves are the
    /// clause's: "a stream whose peer has closed is READY — the `net_read`
    /// that follows is what reports `closed`, which is how a loop learns to
    /// drop it", and `POLLHUP`/`POLLERR` count as ready for the same
    /// reason. Asking consumes nothing a program can observe: the listener
    /// half stashes what it accepted (see [`NetSock::pending`]) and the
    /// stream half PEEKS, which leaves the bytes on the socket.
    fn poll_ready(&mut self, fd: i128) -> NetResult<bool> {
        let sock = self.sock(fd)?;
        if !sock.pending.is_empty() {
            return Ok(true);
        }
        let accepted = match &mut sock.kind {
            // A connection `net_wait` takes off the queue to answer its
            // readiness question is a stream the runtime hands the program
            // one `net_accept` later, so it is minted under the same default.
            SockKind::Listener(listener) => match listener.accept() {
                Ok((stream, _)) => adopt_tcp(stream)?,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(false);
                }
                Err(_) => return Err(NetErr::Row("io")),
            },
            #[cfg(unix)]
            SockKind::UnixListener(listener, _) => match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(true)
                        .map_err(|_| NetErr::Row("io"))?;
                    SockKind::UnixStream(stream)
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Ok(false);
                }
                Err(_) => return Err(NetErr::Row("io")),
            },
            SockKind::Stream(stream) => return Ok(peek_is_ready(stream.peek(&mut [0u8; 1]))),
            #[cfg(unix)]
            SockKind::UnixStream(stream) => {
                if !sock.held.is_empty() || sock.eof {
                    return Ok(true);
                }
                let mut buf = [0u8; 4096];
                let read = stream.read(&mut buf);
                return match read {
                    Ok(0) => {
                        sock.eof = true;
                        Ok(true)
                    }
                    Ok(n) => {
                        sock.held.extend_from_slice(&buf[..n]);
                        Ok(true)
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
                    // The `POLLERR` posture: ready, and the read that follows
                    // is the one that names it.
                    Err(_) => Ok(true),
                };
            }
        };
        sock.pending.push_back(accepted);
        Ok(true)
    }

    /// Every fd in the set names a live socket, or the whole call is `io`.
    ///
    /// `[os.net.wait]`: a forged or closed handle ANYWHERE in the set is
    /// `io` and **nothing is waited on** — so the set is validated whole
    /// before a single socket is asked, and a bad member never leaves a
    /// stashed connection behind on a good one.
    fn wait_validate(&mut self, fds: &[i128]) -> NetResult<()> {
        for fd in fds {
            self.sock(*fd)?;
        }
        Ok(())
    }

    /// The SUBSET of `fds` that can be read now, in the caller's order.
    fn ready_now(&mut self, fds: &[i128]) -> NetResult<Vec<i128>> {
        let mut ready = Vec::new();
        for fd in fds {
            if self.poll_ready(*fd)? {
                ready.push(*fd);
            }
        }
        Ok(ready)
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
        Ok(self.fd(NetSock::new(SockKind::UnixListener(listener, path))))
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
        Ok(self.fd(NetSock::new(SockKind::UnixStream(stream))))
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
/// path-shaped twin: a socket path is a host filesystem object, so the
/// admitted shape is a RELATIVE path that does not climb out of the working
/// directory — `target/x.sock`, which is what the corpus witness writes.
/// Anything else is refused BY NAME (`unsupported`, the by-name refusal and
/// never the `unsupported` ROW, which would be a lie about the host).
///
/// This rule was written when the machine declined the host's filesystem
/// outright (wolf-interp#18 item 6, `[proto.cmp.defined-divergence]`). is48
/// built the fs tier on the maintainer's ruling, and the rule OUTLIVED its
/// original premise rather than retiring with it: [`super::fs::contained`] is
/// the same check, for the reason that survives — the corpus walk, the
/// differ, the explorer and the fuzzer all run corpus programs in-process, so
/// a path that climbs out is the one bug here that could damage the machine
/// it runs on. The two checks are spelled separately on purpose: they answer
/// to different clauses (`[os.net.unix]` and `[os.fs]`), and either could
/// narrow without the other.
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
             path is a host filesystem object, and this machine's fs surface (`eval::fs`, \
             is48) admits only a relative path that does not climb out — the shape is \
             refused by name rather than observed"
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

/// `[os.net.listen.opts]`'s bind rows: an address another socket already
/// holds is `exists` — the "already in use" answer wolf-lang#234 asked for,
/// spelled in the vocabulary `[os.net.unix]` gave the family — a privileged
/// port the caller may not bind is `denied`, and the rest is `io`.
fn bind_row(error: &std::io::Error) -> Row {
    match error.kind() {
        std::io::ErrorKind::AddrInUse => "exists",
        std::io::ErrorKind::PermissionDenied => "denied",
        _ => "io",
    }
}

/// `[os.net.wait]` for a TCP stream: what a non-blocking PEEK means.
///
/// `Ok(0)` is the peer's finish and is READY, not quiet — the clause is
/// explicit that the `net_read` after it is what reports `closed`. Any
/// other error is ready too, for the reason `POLLERR` is: the read that
/// follows is the one that names it.
fn peek_is_ready(peeked: std::io::Result<usize>) -> bool {
    match peeked {
        Ok(_) => true,
        Err(error) => error.kind() != std::io::ErrorKind::WouldBlock,
    }
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
            // `[os.net.listen.opts]` (s137, wolf-lang#234). The clause's own
            // equality holds here call for call: `net_listen_with(addr,
            // false, 0)` IS `net_listen(addr)` — same bind, same handle,
            // same rows minus the two this call can also answer.
            "net_listen_with" => {
                let Some(Value::Str(addr)) = args.first() else {
                    return Err(Signal::Unsupported(format!(
                        "`{name}` takes an address `str` like \"127.0.0.1:0\""
                    )));
                };
                let Some(Value::Bool(reuse_port)) = args.get(1) else {
                    return Err(Signal::Unsupported(format!(
                        "`{name}`'s second argument is the `reuse_port` bool"
                    )));
                };
                let (addr, reuse_port) = (addr.clone(), *reuse_port);
                let backlog = int_arg(args, 2, name)?;
                let answer = self.net().listen_with(&addr, reuse_port, backlog);
                self.net_answer(name, answer.map(|fd| Value::Int(fd, IntTy::INT)), span)
            }
            // `[os.proc.inherit]`'s child half (s137, wolf-lang#235).
            // Refused BY NAME with the construct s137 published, and the
            // refusal has TWO reasons that arrive at the same place: this is
            // the interpreter running the program, so a descriptor handed to
            // "the program's child" would be handed to the interpreter's;
            // and adopting a number means `FromRawFd`, which is `unsafe`,
            // which this crate forbids. Never the `unsupported` ROW — that
            // would be a claim about the HOST, and the host serves this.
            "net_adopt_listener" => Err(Signal::Unsupported(
                "listener adoption in checked execution".to_owned(),
            )),
            // `[os.net.wait]` (s137, wolf-lang#127). Alone in s137 this
            // clause names no host and no refusal, so it serves on every
            // lane this machine has.
            "net_wait" => {
                let Some(Value::List(items, _, _)) = args.first() else {
                    return Err(Signal::Unsupported(format!(
                        "`{name}` takes a `List[int]` of handles and a deadline in ms"
                    )));
                };
                let mut fds = Vec::with_capacity(items.len());
                for slot in items.iter() {
                    let Value::Int(fd, _) = &slot.value else {
                        return Err(Signal::Unsupported(format!(
                            "`{name}`'s set holds {}, not a handle",
                            slot.value.kind()
                        )));
                    };
                    fds.push(*fd);
                }
                let deadline_ms = int_arg(args, 1, name)?;
                let answer = self.net_wait_set(&fds, deadline_ms, span)?;
                let answer = match answer {
                    Ok(ready) => {
                        let home = self.allocate(
                            span,
                            "net_wait",
                            super::region::ledger::container_bytes(ready.len() as u64),
                        )?;
                        Ok(Value::list(
                            ready
                                .into_iter()
                                .map(|fd| super::value::Slot::live(Value::Int(fd, IntTy::INT)))
                                .collect(),
                            Some(ElemTy::Int(IntTy::INT)),
                            Some(home),
                        ))
                    }
                    Err(err) => Err(err),
                };
                self.net_answer(name, answer, span)
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
            // `[os.net.accept]` (s138, wolf-lang#242 — the clause arrived at
            // the v0.2.6 pin and names this machine's posture directly: "its
            // budgeted accept polls a non-blocking listener and retries
            // `would_block` against the same budget"). Nothing moved to meet
            // it, because that posture is what `poll_accept` + `net_park`
            // have been since is18: the listener is non-blocking, a take that
            // finds nothing is `Poll::NotYet` and NOT a row, and `net_park`
            // measures from ONE `started` against ONE `armed(fd)` — a lost
            // race re-waits inside the budget the call began with, never a
            // fresh one. A wake is not a connection, and this machine can
            // lose the race to its OWN second hand: see
            // `tests/net_accept_race.rs`.
            "net_accept" => {
                let fd = int_arg(args, 0, name)?;
                let answer = self.net_park(name, fd, span, |table| table.poll_accept(fd))?;
                self.net_answer(name, answer.map(|fd| Value::Int(fd, IntTy::INT)), span)
            }
            "net_read" => {
                let fd = int_arg(args, 0, name)?;
                let n = int_arg(args, 1, name)?;
                if n <= 0 {
                    // Zero bytes wanted: the empty string, and the socket is
                    // never asked (a 0-length receive would forge `closed`).
                    return Ok(Value::Str(super::value::Str::default()));
                }
                let n = usize::try_from(n).unwrap_or(usize::MAX).min(1 << 20);
                let answer = self.net_park(name, fd, span, |table| table.poll_read(fd, n))?;
                let answer = match answer {
                    Ok(text) => {
                        self.allocate(
                            span,
                            "net_read",
                            super::region::ledger::str_bytes(text.len() as u64),
                        )?;
                        Ok(Value::Str(text.into()))
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
                let bytes = text.text.clone().into_bytes();
                let mut at = 0usize;
                let answer = self.net_park(name, fd, span, move |table| {
                    table.poll_write(fd, &bytes, &mut at)
                })?;
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
                let answer = self.net_park(name, fd, span, |table| table.poll_read_bytes(fd, n))?;
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
                            Some(ElemTy::Byte),
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
                let answer = self.net_park(name, fd, span, move |table| {
                    table.poll_write(fd, &bytes, &mut at)
                })?;
                self.net_answer(name, answer.map(|()| Value::Unit), span)
            }
            // `[os.net.writev]` (s141, wolf-lang#254): the gathered write.
            // `net_write_bytes` with the payload one level deeper, and one
            // syscall where the parts would have been several.
            "net_writev" => {
                let fd = int_arg(args, 0, name)?;
                let Some(outer) = args.get(1).and_then(Value::seq_slots) else {
                    return Err(Signal::Unsupported(format!(
                        "`{name}` takes an fd and a `List[List[byte]]` payload"
                    )));
                };
                let mut parts: Vec<Vec<u8>> = Vec::with_capacity(outer.len());
                for slot in outer {
                    let Some(inner) = slot.value.seq_slots() else {
                        return Err(Signal::Unsupported(format!(
                            "`{name}`'s parts must each be a `List[byte]`, got {}",
                            slot.value.kind()
                        )));
                    };
                    let mut part = Vec::with_capacity(inner.len());
                    for element in inner {
                        match &element.value {
                            Value::Byte(b) => part.push(*b),
                            Value::Int(v, _) if (0..=255).contains(v) => {
                                part.push(u8::try_from(*v).expect("checked 0..=255"));
                            }
                            // NOT the `invalid` row, and the clause is
                            // explicit about why: "`invalid` is
                            // `net_write_bytes`'s refusal of a list that is
                            // not a `List[byte]`, which a typed
                            // `List[List[byte]]` cannot present and this call
                            // does not declare." A row the call never
                            // declared would be a tag no handler's arms can
                            // resolve, so the untyped shapes machinery can
                            // still build are refused BY NAME instead.
                            other => {
                                return Err(Signal::Unsupported(format!(
                                    "`{name}`'s parts hold {}, and the call declares no \
                                     `invalid` row to answer with — a typed \
                                     `List[List[byte]]` cannot present this, so the shape \
                                     is refused by name rather than given a row \
                                     `[os.net.writev]` does not carry",
                                    other.kind()
                                )));
                            }
                        }
                    }
                    parts.push(part);
                }
                let mut cursor = (0usize, 0usize);
                let answer = self.net_park(name, fd, span, move |table| {
                    table.poll_writev(fd, &parts, &mut cursor)
                })?;
                self.net_answer(name, answer.map(|()| Value::Unit), span)
            }
            // `[os.net.nodelay]` (s141, wolf-lang#254). The DEFAULT half is
            // in `adopt_tcp`, on every TCP stream this table mints; this is
            // the call that sets it either way afterwards.
            "net_nodelay" => {
                let fd = int_arg(args, 0, name)?;
                let Some(Value::Bool(on)) = args.get(1) else {
                    return Err(Signal::Unsupported(format!(
                        "`{name}`'s second argument is the `on` bool"
                    )));
                };
                let on = *on;
                let answer = self.net().nodelay(fd, on);
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

    /// `[os.net.wait]`'s loop: the ready subset, or the empty answer.
    ///
    /// Four sentences of the clause live in this function. **An EMPTY answer
    /// is the deadline expiring with nothing ready — an answer, not a
    /// failure** (`{io}` is the whole row set and no timeout tag is in it).
    /// **`deadline_ms`**: negative waits until something is ready, `0` asks
    /// and returns at once, positive is milliseconds. **`io`** is a forged
    /// or closed handle anywhere in the set, with nothing waited on, or an
    /// empty set with an unbounded deadline — nothing could ever end that
    /// wait, and a program that means to sleep says `time_sleep_ms`. And
    /// the call touches no task state: it parks nothing and registers
    /// nothing, so it mixes with `spawn` freely; in the task tier its wait
    /// hands the baton on exactly as every other net call's does.
    fn net_wait_set(
        &mut self,
        fds: &[i128],
        deadline_ms: i128,
        span: Span,
    ) -> Result<NetResult<Vec<i128>>, Signal> {
        if let Err(err) = self.net().wait_validate(fds) {
            return Ok(Err(err));
        }
        if fds.is_empty() && deadline_ms < 0 {
            return Ok(Err(NetErr::Row("io")));
        }
        let started = Instant::now();
        let budget = (deadline_ms > 0)
            .then(|| Duration::from_millis(u64::try_from(deadline_ms).unwrap_or(u64::MAX)));
        loop {
            match self.net().ready_now(fds) {
                Err(err) => return Ok(Err(err)),
                Ok(ready) if !ready.is_empty() => return Ok(Ok(ready)),
                Ok(_) => {}
            }
            if deadline_ms == 0 {
                return Ok(Ok(Vec::new()));
            }
            if let Some(budget) = budget
                && started.elapsed() >= budget
            {
                return Ok(Ok(Vec::new()));
            }
            if started.elapsed() >= Duration::from_millis(NET_RAIL_MS) {
                // The unbounded wait, unbounded: decline the verdict rather
                // than hang, exactly as a deadline-free park does.
                return Err(Signal::Unsupported(format!(
                    "`net_wait` waited past the machine's {NET_RAIL_MS}ms rail with an \
                     unbounded deadline; a verdict is declined rather than a hang"
                )));
            }
            if let Some(err) = self.net_tick(span)? {
                return Ok(Err(err));
            }
        }
    }

    /// One pass of a net wait: in the task tier hand the baton on through an
    /// ordinary scheduling decision so the peer that will resolve this can
    /// run, otherwise sleep a host millisecond. `Some(_)` is a cancellation
    /// delivered at this blocking point ([conc.cancel.points]).
    fn net_tick(&mut self, span: Span) -> Result<Option<NetErr>, Signal> {
        if self.concurrent() {
            let wake = self.sched_block(|sched, task| sched.net_yield(task));
            match wake {
                super::sched::Wake::Killed => return Err(Signal::ProcKilled),
                super::sched::Wake::Cancelled => {
                    self.fire(
                        Rule::CancelPoint,
                        span,
                        "cancellation delivered at a net blocking point",
                    );
                    return Ok(Some(NetErr::Cancelled));
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(1));
        Ok(None)
    }

    /// The parking loop (module doc, "Blocking honesty"): poll until ready,
    /// a row, the fd's armed deadline (the `timeout` row), or the rail.
    fn net_park<T>(
        &mut self,
        name: &str,
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
                // as its row's `timeout` tag — WHERE THE CALL DECLARES ONE.
                //
                // `[os.net.io]` (s141) is the clause that made this
                // distinction visible, and it is the only source motion the
                // clause cost this machine. A write's budget now covers the
                // whole drain, so `net_write`/`net_write_bytes`/`net_writev`
                // can reach this line — and their declared row is
                // `{closed, io}`, with no `timeout` in it. The clause says
                // what comes out: "the row is `net_write`'s `io` (its
                // `timeout` coarsened, as the call declares)". Raising a
                // bare `timeout` here would hand a handler a tag its own
                // `match` cannot resolve as a tag (`[gram.expr.tagident]`
                // makes the arms exactly as wide as the DECLARED row), which
                // is wolf-interp#47's defect at a new address.
                return Ok(Err(NetErr::Row(budget_row(name))));
            }
            if started.elapsed() >= Duration::from_millis(NET_RAIL_MS) {
                // No deadline and nothing will resolve this: decline the
                // verdict rather than hang, and never invent an answer.
                return Err(Signal::Unsupported(format!(
                    "a net call parked past the machine's {NET_RAIL_MS}ms rail with no \
                     armed deadline; a verdict is declined rather than a hang"
                )));
            }
            // The task tier hands the baton through an ordinary scheduling
            // decision so the peer that will resolve this accept/read gets
            // to run; a cancellation or kill racing in resolves exactly as
            // it would at a channel's blocking point.
            if let Some(err) = self.net_tick(span)? {
                return Ok(Err(err));
            }
        }
    }
}

/// The vectored drain itself, over any connected socket.
///
/// Split out from [`NetTable::poll_writev`] so the RESUMPTION can be tested
/// against a writer that stops where a kernel would. A gather that fits in
/// the send buffer never resumes, and a gather that does not is 8 MiB of
/// `List[byte]` in the interpreter's own value representation — so a witness
/// written in wolf cannot reach this path at a size the harness can afford.
/// `net::tests::a_trickling_writer_resumes_mid_part` reaches it directly.
fn drive_writev(
    stream: &mut dyn Duplex,
    parts: &[Vec<u8>],
    cursor: &mut (usize, usize),
) -> NetResult<Poll<()>> {
    loop {
        while cursor.0 < parts.len() && cursor.1 >= parts[cursor.0].len() {
            cursor.0 += 1;
            cursor.1 = 0;
        }
        if cursor.0 >= parts.len() {
            return Ok(Poll::Ready(()));
        }
        let mut slices: Vec<IoSlice<'_>> = Vec::with_capacity(parts.len() - cursor.0);
        slices.push(IoSlice::new(&parts[cursor.0][cursor.1..]));
        for part in &parts[cursor.0 + 1..] {
            if !part.is_empty() {
                slices.push(IoSlice::new(part));
            }
        }
        match stream.write_vectored(&slices) {
            Ok(0) => return Err(NetErr::Row("io")),
            Ok(wrote) => {
                let mut left = wrote;
                while left > 0 && cursor.0 < parts.len() {
                    let remaining = parts[cursor.0].len() - cursor.1;
                    if remaining == 0 {
                        cursor.0 += 1;
                        cursor.1 = 0;
                        continue;
                    }
                    let take = left.min(remaining);
                    cursor.1 += take;
                    left -= take;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Ok(Poll::NotYet);
            }
            Err(error) => return Err(NetErr::Row(closed_or_io(&error))),
        }
    }
}

/// The row a fired `net_deadline` budget answers on `name`.
///
/// `[os.net.io]`: `timeout` where the call declares one, coarsened to `io`
/// where it does not. The declared row is read from
/// [`super::builtin::declared_row`] rather than listed again here, so a call
/// added to that table cannot acquire a second, divergent opinion about its
/// own row — the whole reason the table exists (wolf-interp#47).
fn budget_row(name: &str) -> Row {
    let row = super::builtin::declared_row(name);
    if row.contains(&"timeout") {
        "timeout"
    } else {
        debug_assert!(
            row.contains(&"io"),
            "`{name}` declares neither `timeout` nor `io`, so a fired budget has no row"
        );
        "io"
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

    /// A writer that stops where a kernel would: it accepts at most `bite`
    /// bytes per call and answers `WouldBlock` on every other call, so a
    /// gather is handed over in fragments that fall wherever they fall —
    /// including in the middle of a part, which is the case
    /// `[os.net.writev]` states and no affordable wolf witness can reach.
    struct Trickle {
        bite: usize,
        stall: bool,
        seen: Vec<u8>,
    }

    impl std::io::Read for Trickle {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }

    impl Write for Trickle {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let take = self.bite.min(buf.len());
            self.seen.extend_from_slice(&buf[..take]);
            Ok(take)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        // `write_vectored`'s DEFAULT forwards only the first non-empty slice
        // to `write`, which would make this fake test half the code. The real
        // gather is exercised by taking bytes across the slice boundary.
        fn write_vectored(&mut self, slices: &[IoSlice<'_>]) -> std::io::Result<usize> {
            self.stall = !self.stall;
            if self.stall {
                return Err(std::io::Error::from(std::io::ErrorKind::WouldBlock));
            }
            let mut left = self.bite;
            let mut wrote = 0;
            for slice in slices {
                if left == 0 {
                    break;
                }
                let take = left.min(slice.len());
                self.seen.extend_from_slice(&slice[..take]);
                wrote += take;
                left -= take;
            }
            Ok(wrote)
        }
    }

    #[test]
    fn a_trickling_writer_resumes_mid_part_and_the_gather_arrives_in_order() {
        // `[os.net.writev]`: "the drain resumed from the byte the kernel
        // stopped at in whichever part it stopped in". A 7-byte bite over
        // parts of 10, 0, 3 and 11 bytes stops inside every one of them at
        // some point, and the empty part must contribute nothing without
        // stalling the cursor.
        let parts: Vec<Vec<u8>> = vec![
            b"0123456789".to_vec(),
            Vec::new(),
            b"abc".to_vec(),
            b"WXYZWXYZWXY".to_vec(),
        ];
        let mut writer = Trickle {
            bite: 7,
            stall: true,
            seen: Vec::new(),
        };
        let mut cursor = (0usize, 0usize);
        let mut polls = 0;
        loop {
            polls += 1;
            assert!(polls < 100, "the drain did not converge");
            match drive_writev(&mut writer, &parts, &mut cursor) {
                Ok(Poll::Ready(())) => break,
                Ok(Poll::NotYet) => {}
                Err(err) => panic!("unexpected row: {err:?}"),
            }
        }
        let want: Vec<u8> = parts.concat();
        assert_eq!(writer.seen, want, "the gather is the parts, in order");
        assert!(
            polls > 1,
            "the fake never stalled, so nothing about resumption was tested"
        );
    }

    #[test]
    fn a_gather_of_nothing_completes_without_a_syscall() {
        // "Empty parts are permitted and send nothing; a call whose parts are
        // all empty is a completed write with no syscall." Both degenerate
        // shapes, and the assertion is that the writer was never asked.
        for parts in [Vec::new(), vec![Vec::new(), Vec::new()]] {
            let mut writer = Trickle {
                bite: 7,
                seen: Vec::new(),
                // `write_vectored` FLIPS this on entry, so it is still `true`
                // afterwards exactly when the writer was never called — which
                // is what "a completed write with no syscall" means here.
                stall: true,
            };
            let mut cursor = (0usize, 0usize);
            assert!(matches!(
                drive_writev(&mut writer, &parts, &mut cursor),
                Ok(Poll::Ready(()))
            ));
            assert!(writer.seen.is_empty());
            assert!(writer.stall, "the writer was called, so a syscall happened");
        }
    }

    #[test]
    fn a_fired_budget_answers_io_where_the_call_declares_no_timeout() {
        // `[os.net.io]`'s coarsening, at the function that decides it. The
        // reads declare `timeout` and keep it; the three writes do not and
        // are coarsened to `io`; `net_accept` keeps its own.
        assert_eq!(budget_row("net_read"), "timeout");
        assert_eq!(budget_row("net_read_bytes"), "timeout");
        assert_eq!(budget_row("net_accept"), "timeout");
        assert_eq!(budget_row("net_write"), "io");
        assert_eq!(budget_row("net_write_bytes"), "io");
        assert_eq!(budget_row("net_writev"), "io");
    }

    #[test]
    fn nodelay_is_the_tcp_stream_s_option_and_every_other_handle_is_io() {
        // `[os.net.nodelay]`: the option is TCP's, so a listener is `io`, a
        // forged handle is `io`, and a closed one is `io`. The default half
        // (`adopt_tcp`) is not observable from `std`, which is why the
        // corpus witness asserts the toggle rather than the flag.
        let mut table = NetTable::default();
        let srv = table.listen("127.0.0.1:0").expect("binds");
        let port = table.port(srv).expect("has a port");
        let cli = table
            .connect(&format!("127.0.0.1:{port}"))
            .expect("dials loopback");
        assert!(table.nodelay(cli, false).is_ok());
        assert!(table.nodelay(cli, true).is_ok());
        assert_eq!(row(table.nodelay(srv, true)), "io");
        assert_eq!(row(table.nodelay(99_999, true)), "io");
        table.close(cli).expect("closes");
        assert_eq!(row(table.nodelay(cli, true)), "io");
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
