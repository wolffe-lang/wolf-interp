//! The s39 net builtin family — blocking TCP v0 over `std::net` (is18).
//!
//! # Independence, stated where it lives
//!
//! Written against the prelude signatures (`net_listen(str) -> int ! {io}`;
//! `net_port(int) -> int ! {io}`; `net_accept(int) -> int ! {timeout, io}`;
//! `net_connect(str) -> int ! {refused, timeout, io}`; `net_read(int, int)
//! -> str ! {closed, timeout, utf8, io}`; `net_write(int, str) -> unit !
//! {closed, io}`; `net_close(int) -> unit ! {io}`; `net_deadline(int, int)
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
        let SockKind::Listener(listener) = &sock.kind else {
            // A stream cannot accept: wrong-kind fd, the `io` row (probed).
            return Err(NetErr::Row("io"));
        };
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(true)
                    .map_err(|_| NetErr::Row("io"))?;
                Ok(Poll::Ready(self.fd(NetSock {
                    kind: SockKind::Stream(stream),
                    deadline_ms: None,
                })))
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(Poll::NotYet),
            Err(_) => Err(NetErr::Row("io")),
        }
    }

    /// One read poll: up to `n` bytes, validated. `Ok(0)` from the socket
    /// is the peer's finish — the `closed` row (probed).
    fn poll_read(&mut self, fd: i128, n: usize) -> NetResult<Poll<String>> {
        let sock = self.sock(fd)?;
        let SockKind::Stream(stream) = &mut sock.kind else {
            return Err(NetErr::Row("io"));
        };
        let mut buf = vec![0u8; n];
        match stream.read(&mut buf) {
            Ok(0) => Err(NetErr::Row("closed")),
            Ok(read) => {
                buf.truncate(read);
                match String::from_utf8(buf) {
                    Ok(text) => Ok(Poll::Ready(text)),
                    // Bytes off a socket are data; mis-encoded data is a
                    // recoverable outcome, exactly like `str_from_utf8`.
                    Err(_) => Err(NetErr::Row("utf8")),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(Poll::NotYet),
            Err(error) => Err(NetErr::Row(closed_or_io(&error))),
        }
    }

    /// One write poll from `at`: how far the write is now, or done.
    fn poll_write(&mut self, fd: i128, bytes: &[u8], at: &mut usize) -> NetResult<Poll<()>> {
        let sock = self.sock(fd)?;
        let SockKind::Stream(stream) = &mut sock.kind else {
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
    fn close(&mut self, fd: i128) -> NetResult<()> {
        let slot = self.slot(fd)?;
        if slot.take().is_none() {
            return Err(NetErr::Row("io"));
        }
        Ok(())
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
                        self.allocate(span, "net_read");
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
