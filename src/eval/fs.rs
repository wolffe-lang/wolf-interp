//! The s38/s90 file family over REAL files — `[os.fs]`, `spec/11-os.md` §6
//! (is48).
//!
//! # Why this module exists at all
//!
//! Every release before 0.1.36 declined this whole tier by name: "this
//! machine has no filesystem by design". That posture was not laziness — it
//! read `[proto.cmp.defined-divergence]` as ruling the host's filesystem out
//! of the comparison surface — but it meant file input and output were never
//! going to be possible in the reference implementation, and the maintainer
//! ruled otherwise (BACKLOG B16): **build the tier.** Real files, no mocks.
//!
//! What the old posture got right survives here as the *containment* rule
//! below: this machine writes under its own working directory and nowhere
//! else. What it got wrong was the conclusion that a filesystem tier cannot
//! be compared at all. It can, and the corpus proves it, because the corpus's
//! own fs witnesses are written to be idempotent and to print RELATIONS
//! ("same_size", "kind", "cleaned") rather than host values.
//!
//! # Independence, stated where it lives
//!
//! Written from `spec/11-os.md` §6 (`[os.fs.open]` and `[os.fs.fstat]` in
//! full), from the five witness headers under `corpus/fs/`, and from
//! empirical probes of `wolf 0.2.12` at this very pin (`a7f517e`) — the route
//! [`super::os`] took for the process trio. **Never `wolf_rt::fs`.** §6 says
//! outright that the rest of the family "is specified by its witnesses under
//! `corpus/fs/` and by `wolf_rt::fs`'s per-call rows"; the second of those is
//! a door this repository does not open, so the witnesses and the observable
//! behaviour of the compiled lane are the whole source.
//!
//! The rows probed rather than guessed, each one a call this file makes:
//!
//! - a read past end-of-file is **`eof`** — no corpus witness reads twice, so
//!   this row is invisible to the census and is pinned by unit tests instead.
//!   `[os.fs.open]`'s mode-5 prose names it ("whose read is `eof` where no
//!   writer exists"), which is the clause that makes it more than a probe.
//! - `fs_read_text` over bytes that are not UTF-8 is **`utf8`**, never a
//!   trap and never a lossy string (`corpus/fs/bytes_dirs.lu` witnesses the
//!   refusal but takes the tag with `_`).
//! - a **directory** read as text is `io`; a directory handed to
//!   `fs_remove` is whatever the host's `unlink` says, which is `denied` on
//!   macOS (EPERM) and `io` on linux (EISDIR). Both lanes are std-backed, so
//!   they take that host split together rather than diverging over it.
//! - a mode outside the set is `invalid` **before the filesystem is
//!   touched**: `fs_open_mode("target/no-such-dir/x", 6)` is `invalid`, not
//!   `not_found` (probed).
//! - an append handle (mode 2) is not readable — `io` (probed); a
//!   create-new handle (mode 4) is.
//!
//! # Containment
//!
//! A path that is absolute or climbs out of the working directory is refused
//! **BY NAME** (`unsupported`), never by a row — a row would be a lie about
//! the host. This is [`super::net::socket_path`]'s posture, already ruled
//! once for unix socket paths, and it costs nothing the corpus asks for:
//! every witness writes under `target/`. The compiled lane does NOT contain
//! paths this way (probed: `wolf` will happily write `/tmp/x`), so this is a
//! stated, named narrowing of the surface and not an accident. It is
//! deliberate: the corpus walk, the differ, the explorer and the fuzzer all
//! run corpus programs in-process, and a fuzzed absolute path is the one bug
//! in this repository that could damage the machine it runs on.

use std::fs::{File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::diag::Span;

use super::rules::Rule;
use super::value::{ElemTy, IntTy, Value};
use super::{Machine, Signal};

/// A D30 row tag, ready for the builtin's `error(tag)`.
pub(crate) type Row = &'static str;

/// What an fs call answers when it does not answer a value.
#[derive(Debug)]
pub(crate) enum FsErr {
    /// A D30 row tag (`not_found`, `denied`, `exists`, `invalid`, `eof`,
    /// `utf8`, `io`).
    Row(Row),
    /// Outside the surface this machine serves: refused by name, never
    /// guessed and never dressed up as a row.
    Outside(String),
}

impl From<Row> for FsErr {
    fn from(tag: Row) -> Self {
        FsErr::Row(tag)
    }
}

pub(crate) type FsResult<T> = Result<T, FsErr>;

/// The open files this run holds, by handle. A closed file leaves its slot
/// (`None`) so the handle stays spent — handles are never reused, and a
/// forged one is `io`. The same shape as [`super::os::ChildTable`] and
/// `net::NetTable`, for the same reason: `corpus/fs/fstat.lu` pins both
/// `closed_is_io` and `forged_is_io`.
#[derive(Default)]
pub(crate) struct FsTable {
    slots: Vec<Option<File>>,
    /// The private working directory an OBSERVED program's paths resolve
    /// against. See [`FsTable::observation_root`]. `None` until first use,
    /// and never used at all by a live `lupin run`.
    root: Option<PathBuf>,
}

/// Removes the private root when the observation ends. Best effort: a
/// directory this machine could not clean is a stale temp directory, never a
/// failed run.
impl Drop for FsTable {
    fn drop(&mut self) {
        // The handles FIRST: a `Drop` body runs before the struct's fields
        // are dropped, so the open `File`s would still be open here — and
        // windows refuses to delete a file anything holds open, which would
        // leak the whole directory on one host and not the others.
        self.slots.clear();
        if let Some(root) = self.root.take() {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

impl FsTable {
    /// The working directory an OBSERVED program's relative paths resolve
    /// against: a private, empty project root, made once per observation.
    ///
    /// # Why an observed program does not share the process's cwd
    ///
    /// This machine runs many programs at once and by design: several test
    /// harnesses walk the whole corpus, `cargo test` runs test binaries in
    /// parallel, and `export::export` runs a full walk inside any of them.
    /// Once programs write REAL files, a shared cwd makes them interfere —
    /// measured, before this existed: `fs/fstat.lu` read `size=0` off a file
    /// another thread had just truncated, two concurrent exports appended to
    /// one `log.txt` twice (`log=one|one|twotwo size=14`), and a bundle's
    /// sha256 stopped being reproducible.
    ///
    /// Ordering the runs was tried first and is the wrong answer: one
    /// advisory lock per program serialises correctly, but
    /// `memory/byte_producers_ledger.lu` takes **74 seconds** in a debug
    /// build, and serialising it across every walker would have added
    /// something like twenty minutes to a CI run that is already three hours.
    /// A private directory removes the contention instead of scheduling it —
    /// no waiting, no lock files, and the oracle is reproducible BY
    /// CONSTRUCTION rather than by everyone remembering to take a lock.
    ///
    /// # Why it has a `target/`
    ///
    /// Every fs witness writes under `target/`, and most write there without
    /// creating it (`corpus/fs/roundtrip.lu`'s
    /// `fs_write_text("target/s38-fs-roundtrip.tmp", …)`), because both lanes
    /// run from a project root where `target/` is the build directory. So the
    /// private root is given the one directory that makes it a project root.
    /// The files are real, the syscalls are real and the rows are the host's;
    /// only the DIRECTORY they happen in is this embedding's choice, which is
    /// the same choice a shell makes by `cd`-ing somewhere before running a
    /// program.
    ///
    /// A live `lupin run` never comes here: a user's program writes in the
    /// user's own directory, exactly as `wolf run` does.
    fn observation_root(&mut self) -> Result<&Path, Row> {
        if self.root.is_none() {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // The name is kept SHORT on purpose. A unix socket path is
            // capped near 104 bytes by `sockaddr_un`, and
            // `corpus/net/unix_echo.lu` binds one under this root through
            // `target/`; macOS's temp directory alone is about fifty
            // characters, so a chattier name here would push a witness over
            // a limit that reports as an opaque bind failure.
            let root = std::env::temp_dir()
                .join("wolf-obs")
                .join(format!("{:x}-{serial:x}", std::process::id()));
            // A serial never repeats within a process and the pid separates
            // processes, so this directory is this observation's alone. It is
            // removed rather than reused if a previous run of this pid died
            // before its `Drop`.
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("target")).map_err(|_| "io")?;
            self.root = Some(root);
        }
        Ok(self.root.as_deref().expect("just set"))
    }

    /// Resolve one contained relative path against the observation root, or
    /// hand it back unchanged for a live run.
    pub(crate) fn resolve(&mut self, relative: &Path, live: bool) -> Result<PathBuf, Row> {
        if live {
            return Ok(relative.to_path_buf());
        }
        Ok(self.observation_root()?.join(relative))
    }

    /// Mint a handle. 1-based, so `0` and every negative are `io` for free.
    fn mint(&mut self, file: File) -> i128 {
        self.slots.push(Some(file));
        self.slots.len() as i128
    }

    fn slot(&mut self, handle: i128) -> Result<&mut Option<File>, Row> {
        let index = usize::try_from(handle)
            .ok()
            .and_then(|h| h.checked_sub(1))
            .ok_or("io")?;
        self.slots.get_mut(index).ok_or("io")
    }

    fn file(&mut self, handle: i128) -> Result<&mut File, Row> {
        self.slot(handle)?.as_mut().ok_or("io")
    }

    /// `fs_open`/`fs_create`/`fs_open_mode`: ONE call under three spellings
    /// (`[os.fs.open]`).
    pub(crate) fn open(&mut self, path: &Path, mode: i128) -> FsResult<i128> {
        // The mode is decided BEFORE the filesystem is touched, so a bad
        // mode is never a half-done open. The clause is explicit about it.
        let options = open_options(mode)?;
        let file = options.open(path).map_err(open_row)?;
        Ok(self.mint(file))
    }

    /// `fs_read(fd, n) -> str`: up to `n` bytes, decoded. `eof` at the end,
    /// `utf8` for bytes that are not text.
    pub(crate) fn read_text(&mut self, handle: i128, want: i128) -> FsResult<String> {
        let bytes = self.read_bytes(handle, want)?;
        String::from_utf8(bytes).map_err(|_| FsErr::Row("utf8"))
    }

    /// `fs_read_chunk(fd, n) -> List[byte]`: the byte twin of [`Self::read_text`].
    pub(crate) fn read_bytes(&mut self, handle: i128, want: i128) -> FsResult<Vec<u8>> {
        let file = self.file(handle)?;
        // A zero-length read is the empty answer and never touches the file:
        // asking the host for zero bytes would forge `eof` (probed — the
        // compiled lane answers `""` here, at end of file or not).
        if want <= 0 {
            return Ok(Vec::new());
        }
        let want = usize::try_from(want).unwrap_or(usize::MAX).min(READ_CAP);
        let mut buf = vec![0u8; want];
        let got = file.read(&mut buf).map_err(io_row)?;
        if got == 0 {
            // Wanted bytes, got none: the `eof` row.
            return Err(FsErr::Row("eof"));
        }
        buf.truncate(got);
        Ok(buf)
    }

    /// `fs_write(fd, text)`: the handle's own write, at the handle's cursor.
    pub(crate) fn write(&mut self, handle: i128, text: &str) -> FsResult<()> {
        let file = self.file(handle)?;
        file.write_all(text.as_bytes()).map_err(io_row)
    }

    /// `fs_fstat(fd) -> [kind, size, modified_ms]` (`[os.fs.fstat]`): ONE
    /// metadata read on the handle the open returned.
    pub(crate) fn fstat(&mut self, handle: i128) -> FsResult<[i128; 3]> {
        let file = self.file(handle)?;
        let meta = file.metadata().map_err(io_row)?;
        Ok([kind_of(&meta), size_of(&meta)?, modified_ms(&meta)?])
    }

    /// `fs_close(fd)`: the slot is spent, never reused. A second close is
    /// `io`, the family's rule for a handle nobody holds.
    pub(crate) fn close(&mut self, handle: i128) -> FsResult<()> {
        let slot = self.slot(handle)?;
        if slot.take().is_none() {
            return Err(FsErr::Row("io"));
        }
        Ok(())
    }
}

/// The largest single read this machine serves, mirroring the net tier's cap.
/// A read is short by contract (`fs_read` answers what is there), so a cap
/// costs a caller nothing but a second call.
const READ_CAP: usize = 1 << 20;

/// `[os.fs.open]`'s mode SET, and nothing outside it.
///
/// | mode | meaning |
/// |---|---|
/// | 0 | read |
/// | 1 | write (create, truncate) |
/// | 2 | append (create) |
/// | 3 | read-write (create, no truncate) |
/// | 4 | create-new (exclusive) |
/// | 5 | read, non-blocking |
///
/// Mode 2 is deliberately NOT readable and mode 4 is: probed on the compiled
/// lane at this pin, and the reason a shared helper spells each mode out
/// rather than folding them together.
fn open_options(mode: i128) -> Result<OpenOptions, Row> {
    let mut options = OpenOptions::new();
    match mode {
        0 => {
            options.read(true);
        }
        1 => {
            options.write(true).create(true).truncate(true);
        }
        2 => {
            options.append(true).create(true);
        }
        3 => {
            options.read(true).write(true).create(true);
        }
        4 => {
            options.read(true).write(true).create_new(true);
        }
        5 => {
            options.read(true);
            if let Some(flag) = nonblock_flag() {
                set_custom_flags(&mut options, flag);
            }
        }
        _ => return Err("invalid"),
    }
    Ok(options)
}

/// `O_NONBLOCK` for mode 5, by host.
///
/// There is no `libc` dependency here and `unsafe_code` is `forbid`, so the
/// constant is written down rather than imported — and then PINNED BY
/// BEHAVIOUR: `a_fifo_with_no_writer_answers_at_once` opens a real fifo and
/// fails if the flag is wrong, on whichever unix runs it. A wrong number
/// cannot pass that test, which is the only guarantee worth having about a
/// hand-copied constant.
///
/// `None` is the windows posture the clause states by name — mode 5 served as
/// mode 0 — and it is also the honest answer for any unix this table does not
/// name, rather than a guessed flag.
const fn nonblock_flag() -> Option<i32> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        Some(0o4000)
    }
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))]
    {
        Some(0o0004)
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )))]
    {
        None
    }
}

#[cfg(unix)]
fn set_custom_flags(options: &mut OpenOptions, flags: i32) {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(flags);
}

#[cfg(not(unix))]
fn set_custom_flags(_options: &mut OpenOptions, _flags: i32) {}

/// `[os.fs.fstat]`'s `kind`: 0 regular file, 1 directory, 2 anything else.
fn kind_of(meta: &Metadata) -> i128 {
    if meta.is_file() {
        0
    } else if meta.is_dir() {
        1
    } else {
        2
    }
}

/// `fs_size`'s answer, and `fs_fstat`'s middle word. A size outside `int` is
/// `io` — `fs_size`'s own rule, quoted by `[os.fs.fstat]`.
fn size_of(meta: &Metadata) -> Result<i128, Row> {
    in_int_domain(i128::from(meta.len()))
}

/// `fs_modified_ms`'s answer, and `fs_fstat`'s last word: milliseconds from
/// the Unix epoch, **negative before it** (the clause says so, so the
/// pre-epoch branch is written rather than clamped).
fn modified_ms(meta: &Metadata) -> Result<i128, Row> {
    let stamp = meta.modified().map_err(|_| "io")?;
    let millis = match stamp.duration_since(UNIX_EPOCH) {
        Ok(since) => i128::try_from(since.as_millis()).map_err(|_| "io")?,
        Err(before) => -i128::try_from(before.duration().as_millis()).map_err(|_| "io")?,
    };
    in_int_domain(millis)
}

/// A word that does not fit `int` is `io`, never a wrapped number.
fn in_int_domain(value: i128) -> Result<i128, Row> {
    let (low, high) = super::value::IntTy::INT.range();
    if value < low || value > high {
        return Err("io");
    }
    Ok(value)
}

/// The open family's rows (`[os.fs.open]`): `not_found`, `denied`, `exists`,
/// `io`. `invalid` never reaches here — it is decided before the open.
fn open_row(error: std::io::Error) -> FsErr {
    FsErr::Row(match error.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "denied",
        std::io::ErrorKind::AlreadyExists => "exists",
        _ => "io",
    })
}

/// The rows of a call on a handle: **`io`, whatever the host said**.
///
/// `[os.fs.fstat]` states the posture — "on a handle the hosts answer `io`
/// for nearly everything, the entry being already resolved" — and this arm
/// takes it literally rather than forwarding the host's error kind, for a
/// reason CI measured. Reading an APPEND handle is `EBADF` on macOS and
/// `ERROR_ACCESS_DENIED` on windows; forwarding the kind made one program
/// answer `io` on one host and `denied` on another, which is a portability
/// hole in a row set programs branch on. The compiled lane answers `io` here
/// too (probed at this pin: a write to a read handle, a read from a write
/// handle and a closed handle are all `io`), so the uniform row is the
/// compatible reading as well as the clause's.
///
/// The path calls keep their kinds — see [`path_row`]. There the entry is
/// what is being resolved, so `not_found` and `denied` are the ANSWER.
fn io_row(_error: std::io::Error) -> FsErr {
    FsErr::Row("io")
}

/// The path calls' rows. Shared by every `fs_*` spelling that takes a path,
/// because `[os.fs.fstat]` says "the row set is the path stat's, so one
/// handler serves both spellings".
pub(crate) fn path_row(error: &std::io::Error) -> FsErr {
    FsErr::Row(match error.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "denied",
        std::io::ErrorKind::AlreadyExists => "exists",
        _ => "io",
    })
}

/// Containment: a relative path that does not climb out of the working
/// directory, or a refusal BY NAME.
///
/// See the module header. `net::socket_path` ruled this once already for a
/// unix socket path; the reasoning is the same and the narrowing is stated
/// rather than silent.
pub(crate) fn contained(path: &str, name: &str) -> FsResult<PathBuf> {
    let candidate = Path::new(path);
    let escapes = candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
    if escapes {
        return Err(FsErr::Outside(format!(
            "`{name}(\"{path}\")` names a path outside the working directory; this machine \
             serves the fs tier over REAL files and admits only a relative path that does not \
             climb out of its own tree, so the shape is refused by name rather than observed"
        )));
    }
    Ok(candidate.to_path_buf())
}

// -- the path calls -------------------------------------------------------
//
// These take no handle and hold no state, so they are free functions rather
// than [`FsTable`] methods. Each one names the row set it can answer.

/// `fs_read_text(path) -> str`: the whole file, decoded. `utf8` for bytes
/// that are not text — never lossy, never a trap.
pub(crate) fn read_text(path: &Path) -> FsResult<String> {
    let bytes = std::fs::read(path).map_err(|e| path_row(&e))?;
    String::from_utf8(bytes).map_err(|_| FsErr::Row("utf8"))
}

/// `fs_read_bytes(path) -> List[byte]`: the whole file, undecoded.
pub(crate) fn read_bytes(path: &Path) -> FsResult<Vec<u8>> {
    std::fs::read(path).map_err(|e| path_row(&e))
}

/// `fs_write_text(path, text)`: create or truncate, then write.
pub(crate) fn write_text(path: &Path, text: &str) -> FsResult<()> {
    std::fs::write(path, text.as_bytes()).map_err(|e| path_row(&e))
}

/// `fs_write_bytes(path, bytes)`: the byte twin of [`write_text`].
pub(crate) fn write_bytes(path: &Path, bytes: &[u8]) -> FsResult<()> {
    std::fs::write(path, bytes).map_err(|e| path_row(&e))
}

/// `fs_remove(path)`: one entry, which must not be a directory.
pub(crate) fn remove(path: &Path) -> FsResult<()> {
    std::fs::remove_file(path).map_err(|e| path_row(&e))
}

/// `fs_rename(from, to)`: moves an entry WITHOUT reading it
/// (`corpus/fs/bytes_dirs.lu` moves bytes no text reader can hold).
pub(crate) fn rename(from: &Path, to: &Path) -> FsResult<()> {
    std::fs::rename(from, to).map_err(|e| path_row(&e))
}

/// `fs_create_dir_all(path)`: the whole chain, and an existing directory is
/// not an error.
pub(crate) fn create_dir_all(path: &Path) -> FsResult<()> {
    std::fs::create_dir_all(path).map_err(|e| path_row(&e))
}

/// `fs_remove_dir_all(path)`: the directory and everything under it.
pub(crate) fn remove_dir_all(path: &Path) -> FsResult<()> {
    std::fs::remove_dir_all(path).map_err(|e| path_row(&e))
}

/// `fs_read_dir(path) -> List[str]`: the entry NAMES, **sorted**.
///
/// The sort is a PROMISE, not luck — `corpus/fs/bytes_dirs.lu` says so in as
/// many words, and its expected stdout is the same on ext4, APFS and NTFS
/// only because of it. The order is bytewise over the name, which is what
/// puts `Alpha.txt` before `bin.dat` (probed against the compiled lane:
/// `.hidden` < `A.txt` < `_u.txt` < `b.txt` < `zsub`). Hidden entries are
/// included; `.` and `..` are not, `read_dir` never yielding them.
///
/// This is also the repository's own standing rule — "sort `read_dir` output
/// before it can influence any generated artifact" (CONTRIBUTING.md) — here
/// as an observable of the language rather than a harness discipline.
pub(crate) fn read_dir(path: &Path) -> FsResult<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|e| path_row(&e))? {
        let entry = entry.map_err(|e| path_row(&e))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| FsErr::Row("io"))?;
        names.push(name);
    }
    names.sort();
    Ok(names)
}

/// `fs_size(path) -> int`.
pub(crate) fn size(path: &Path) -> FsResult<i128> {
    let meta = std::fs::metadata(path).map_err(|e| path_row(&e))?;
    Ok(size_of(&meta)?)
}

/// `fs_modified_ms(path) -> int`, in `fs_fstat`'s unit so the two compare
/// without a conversion (`corpus/fs/fstat.lu` pins `same_mtime`).
pub(crate) fn modified_ms_of(path: &Path) -> FsResult<i128> {
    let meta = std::fs::metadata(path).map_err(|e| path_row(&e))?;
    Ok(modified_ms(&meta)?)
}

/// `fs_exists(path) -> bool`. INFALLIBLE: the witnesses call it with no `?`
/// (`corpus/fs/roundtrip.lu`'s `gone: {!fs_exists(path)}`), so an
/// unreadable parent answers `false` rather than raising.
pub(crate) fn exists(path: &Path) -> bool {
    path.exists()
}

/// `fs_is_dir(path) -> bool`. Infallible, as [`exists`].
pub(crate) fn is_dir(path: &Path) -> bool {
    path.is_dir()
}

/// `fs_is_file(path) -> bool`. Infallible, as [`exists`].
pub(crate) fn is_file(path: &Path) -> bool {
    path.is_file()
}

/// What a builtin arm hands back to `eval_call`.
type EvalOut = Result<Value, Signal>;

impl Machine {
    /// One fs builtin call, by name — the `builtin::call` arm delegates here
    /// so the family reads as one unit, the way the net tier does.
    ///
    /// # Errors
    ///
    /// `Signal::Unsupported` for shapes outside the declared surface
    /// (arguments no typed program produces, and paths that climb out of the
    /// working directory); everything else is a row VALUE, never a trap. The
    /// sema boundary is why: a wrong-typed operand is the static tier's
    /// property, and turning one into a row would hand a program's handler an
    /// arm the call never declared.
    pub(crate) fn fs_call(&mut self, name: &str, args: &[Value], span: Span) -> EvalOut {
        match name {
            // -- the open family: one call, three spellings ----------------
            "fs_open" | "fs_create" | "fs_open_mode" => {
                let path = self.fs_path_arg(args, 0, name)?;
                let mode = match name {
                    "fs_open" => 0,
                    "fs_create" => 1,
                    _ => int_arg(args, 1, name)?,
                };
                let answer = self.fs_contained(&path, name).and_then(|path| {
                    self.shared_fs()
                        .open(&path, mode)
                        .map(|fd| Value::Int(fd, IntTy::INT))
                });
                self.fs_answer(name, answer, span)
            }
            "fs_close" => {
                let fd = int_arg(args, 0, name)?;
                let answer = self.shared_fs().close(fd).map(|()| Value::Unit);
                self.fs_answer(name, answer, span)
            }
            // -- reads and writes on a handle ------------------------------
            "fs_read" => {
                let fd = int_arg(args, 0, name)?;
                let want = int_arg(args, 1, name)?;
                let answer = self.shared_fs().read_text(fd, want);
                let answer = self.fs_str(answer, "fs_read", span)?;
                self.fs_answer(name, answer, span)
            }
            "fs_read_chunk" => {
                let fd = int_arg(args, 0, name)?;
                let want = int_arg(args, 1, name)?;
                let answer = self.shared_fs().read_bytes(fd, want);
                let answer = self.fs_byte_list(answer, "fs_read_chunk", span)?;
                self.fs_answer(name, answer, span)
            }
            "fs_write" => {
                let text = self.fs_str_arg(args, 1, name)?;
                let fd = int_arg(args, 0, name)?;
                let answer = self.shared_fs().write(fd, &text).map(|()| Value::Unit);
                self.fs_answer(name, answer, span)
            }
            // `[os.fs.fstat]`: `[kind, size, modified_ms]` from ONE metadata
            // read on the handle the open returned.
            "fs_fstat" => {
                let fd = int_arg(args, 0, name)?;
                let answer = self.shared_fs().fstat(fd);
                let answer = match answer {
                    Ok(words) => {
                        let home = self.allocate(
                            span,
                            "fs_fstat",
                            super::region::ledger::container_bytes(words.len() as u64),
                        )?;
                        Ok(Value::list(
                            words
                                .into_iter()
                                .map(|word| super::value::Slot::live(Value::Int(word, IntTy::INT)))
                                .collect(),
                            Some(ElemTy::Int(IntTy::INT)),
                            Some(home),
                        ))
                    }
                    Err(err) => Err(err),
                };
                self.fs_answer(name, answer, span)
            }
            // -- the path calls --------------------------------------------
            "fs_read_text" => {
                let path = self.fs_path_arg(args, 0, name)?;
                let answer = self
                    .fs_contained(&path, name)
                    .and_then(|path| read_text(&path));
                let answer = self.fs_str(answer, "fs_read_text", span)?;
                self.fs_answer(name, answer, span)
            }
            "fs_read_bytes" => {
                let path = self.fs_path_arg(args, 0, name)?;
                let answer = self
                    .fs_contained(&path, name)
                    .and_then(|path| read_bytes(&path));
                let answer = self.fs_byte_list(answer, "fs_read_bytes", span)?;
                self.fs_answer(name, answer, span)
            }
            "fs_write_text" => {
                let text = self.fs_str_arg(args, 1, name)?;
                let path = self.fs_path_arg(args, 0, name)?;
                let answer = self
                    .fs_contained(&path, name)
                    .and_then(|path| write_text(&path, &text))
                    .map(|()| Value::Unit);
                self.fs_answer(name, answer, span)
            }
            "fs_write_bytes" => {
                let bytes = fs_bytes_arg(args, 1, name)?;
                let path = self.fs_path_arg(args, 0, name)?;
                let answer = match bytes {
                    Ok(bytes) => self
                        .fs_contained(&path, name)
                        .and_then(|path| write_bytes(&path, &bytes))
                        .map(|()| Value::Unit),
                    // An int outside the octet is the declared `invalid`
                    // row, the net byte writer's discipline.
                    Err(tag) => Err(FsErr::Row(tag)),
                };
                self.fs_answer(name, answer, span)
            }
            "fs_remove" | "fs_remove_dir_all" | "fs_create_dir_all" => {
                let path = self.fs_path_arg(args, 0, name)?;
                let answer = self
                    .fs_contained(&path, name)
                    .and_then(|path| match name {
                        "fs_remove" => remove(&path),
                        "fs_remove_dir_all" => remove_dir_all(&path),
                        _ => create_dir_all(&path),
                    })
                    .map(|()| Value::Unit);
                self.fs_answer(name, answer, span)
            }
            "fs_rename" => {
                let from = self.fs_path_arg(args, 0, name)?;
                let to = self.fs_path_arg(args, 1, name)?;
                let answer = self.fs_contained(&from, name).and_then(|from| {
                    self.fs_contained(&to, name)
                        .and_then(|to| rename(&from, &to))
                        .map(|()| Value::Unit)
                });
                self.fs_answer(name, answer, span)
            }
            "fs_size" | "fs_modified_ms" => {
                let path = self.fs_path_arg(args, 0, name)?;
                let answer = self
                    .fs_contained(&path, name)
                    .and_then(|path| {
                        if name == "fs_size" {
                            size(&path)
                        } else {
                            modified_ms_of(&path)
                        }
                    })
                    .map(|word| Value::Int(word, IntTy::INT));
                self.fs_answer(name, answer, span)
            }
            "fs_read_dir" => {
                let path = self.fs_path_arg(args, 0, name)?;
                let answer = self
                    .fs_contained(&path, name)
                    .and_then(|path| read_dir(&path));
                let answer = match answer {
                    Ok(names) => {
                        // The listing is a fresh container the machine mints
                        // with its length already known.
                        let home = self.allocate(
                            span,
                            "fs_read_dir",
                            super::region::ledger::container_bytes(names.len() as u64),
                        )?;
                        let mut slots = Vec::with_capacity(names.len());
                        for entry in names {
                            // Each name carries the home it was minted in,
                            // not `None`: a name that escapes its region
                            // faults like any other built `str`
                            // (`memory/region_str_concat_return.lu`).
                            let name_home = self.allocate(
                                span,
                                "fs_read_dir",
                                super::region::ledger::str_bytes(entry.len() as u64),
                            )?;
                            slots.push(super::value::Slot::live(Value::Str(
                                super::value::Str::built(entry, Some(name_home)),
                            )));
                        }
                        // No `ElemTy`: that context exists to decide what
                        // width a pushed literal adopts, and a `str` element
                        // adopts none (`value::ElemTy` is `Int` or `Byte`).
                        Ok(Value::list(slots, None, Some(home)))
                    }
                    Err(err) => Err(err),
                };
                self.fs_answer(name, answer, span)
            }
            // The three infallible predicates: the witnesses call them with
            // no `?` (`corpus/fs/roundtrip.lu`'s `gone: {!fs_exists(path)}`),
            // so they answer a bool and never a row. A path this machine
            // will not look at is still refused by name — a `false` there
            // would be a claim about the filesystem rather than about the
            // surface.
            "fs_exists" | "fs_is_dir" | "fs_is_file" => {
                let path = self.fs_path_arg(args, 0, name)?;
                let path = match self.fs_contained(&path, name) {
                    Ok(path) => path,
                    Err(FsErr::Outside(reason)) => return Err(Signal::Unsupported(reason)),
                    Err(FsErr::Row(_)) => unreachable!("containment answers no row"),
                };
                Ok(Value::Bool(match name {
                    "fs_exists" => exists(&path),
                    "fs_is_dir" => is_dir(&path),
                    _ => is_file(&path),
                }))
            }
            other => Err(Signal::Unsupported(format!(
                "`{other}` is not an fs builtin this machine knows"
            ))),
        }
    }

    /// The fs handle table, behind its lock.
    fn shared_fs(&self) -> std::sync::MutexGuard<'_, FsTable> {
        self.files()
    }

    /// The private root an OBSERVED program's paths resolve against, or
    /// `None` for a live `lupin run`.
    ///
    /// The net tier reads it so a UNIX SOCKET path lands in the same
    /// directory as everything else the program writes.
    /// `corpus/net/unix_echo.lu` is why it has to: the witness removes a
    /// stale socket with `fs_remove`, binds it with `net_listen_unix`, and
    /// ends with `cleaned = !fs_exists(path)`. If the two families disagreed
    /// about the directory, the pre-clean would sweep a path nothing binds
    /// and `cleaned` would be vacuously true — the witness would pass while
    /// checking nothing, and a stale socket in the real cwd would fail the
    /// bind at random.
    ///
    /// Carries the unix-socket family's `cfg`: the net tier is its only
    /// reader, and `[os.net.unix]` is served on unix alone, so on windows
    /// this would be dead code and `-D warnings` red.
    #[cfg(unix)]
    pub(crate) fn fs_observation_base(&self) -> Option<PathBuf> {
        if self.is_live() {
            return None;
        }
        self.files().resolve(Path::new(""), false).ok()
    }

    /// The directory this machine's relative paths resolve against — the
    /// answer `os_cwd` gives, so the two never disagree.
    ///
    /// # Errors
    ///
    /// `Err(())` when neither a private root nor the process cwd can be had;
    /// the caller turns that into `os_cwd`'s `io` row.
    pub(crate) fn fs_working_dir(&self) -> Result<PathBuf, ()> {
        if self.is_live() {
            return std::env::current_dir().map_err(|_| ());
        }
        self.files().resolve(Path::new(""), false).map_err(|_| ())
    }

    /// Containment AND resolution, as one method so every arm reads the same:
    /// the shape is checked, then resolved against this observation's private
    /// working directory ([`FsTable::observation_root`]) — or left alone for
    /// a live `lupin run`, which uses the user's own cwd.
    ///
    /// The two halves belong together: containment is what makes the root a
    /// real jail rather than a prefix, since a path that could climb out
    /// would escape it on the first `..`.
    fn fs_contained(&self, path: &str, name: &str) -> FsResult<PathBuf> {
        let relative = contained(path, name)?;
        let live = self.is_live();
        self.files().resolve(&relative, live).map_err(FsErr::Row)
    }

    /// A row becomes an error VALUE with a note; a by-name refusal becomes
    /// the honest `unsupported`. [`super::net`]'s `net_answer`, for the same
    /// reason it exists there: one funnel, so no arm can forget the note.
    fn fs_answer(&mut self, name: &str, answer: FsResult<Value>, span: Span) -> EvalOut {
        match answer {
            Ok(value) => Ok(value),
            Err(FsErr::Row(tag)) => {
                self.note(
                    Rule::ErrUnion,
                    span,
                    &format!("`{name}` yields the `{tag}` row"),
                );
                // The tag rides with the raising builtin's whole declared
                // row so a downstream handler's arms discriminate
                // (wolf-interp#47).
                Ok(super::builtin::error_value(name, tag))
            }
            Err(FsErr::Outside(reason)) => Err(Signal::Unsupported(reason)),
        }
    }

    /// A `str` answer, charged to the current region.
    fn fs_str(&mut self, answer: FsResult<String>, what: &str, span: Span) -> EvalResult {
        Ok(match answer {
            Ok(text) => {
                let home = self.allocate(
                    span,
                    what,
                    super::region::ledger::str_bytes(text.len() as u64),
                )?;
                Ok(Value::Str(super::value::Str::built(text, Some(home))))
            }
            Err(err) => Err(err),
        })
    }

    /// A `List[byte]` answer, minted at EXACT capacity.
    ///
    /// A reader knows its length before it mints, so it pays no growth
    /// history — `[type.byte]`'s 1-byte stride and nothing else. This is what
    /// makes `memory/byte_producers_ledger.lu`'s `read_tight` relation hold
    /// ("at most the payload plus a header"); a byte list built by pushing
    /// would charge the doubling history and blow it.
    fn fs_byte_list(&mut self, answer: FsResult<Vec<u8>>, what: &str, span: Span) -> EvalResult {
        Ok(match answer {
            Ok(bytes) => {
                let home = self.allocate(
                    span,
                    what,
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
        })
    }

    /// The `str` argument at `at`, cloned off the value.
    fn fs_path_arg(&self, args: &[Value], at: usize, name: &str) -> Result<String, Signal> {
        self.fs_str_arg(args, at, name)
    }

    fn fs_str_arg(&self, args: &[Value], at: usize, name: &str) -> Result<String, Signal> {
        match args.get(at) {
            Some(Value::Str(s)) => Ok(s.text.clone()),
            other => Err(Signal::Unsupported(format!(
                "`{name}`'s argument {at} must be a `str`, got {}",
                other.map_or_else(|| "nothing".to_owned(), super::value::Value::kind)
            ))),
        }
    }
}

/// What [`Machine::fs_str`] and [`Machine::fs_byte_list`] hand back: a region
/// charge can trap, and that trap is not a row.
type EvalResult = Result<FsResult<Value>, Signal>;

/// The `int` argument at `at`, or the honest refusal.
fn int_arg(args: &[Value], at: usize, name: &str) -> Result<i128, Signal> {
    match args.get(at) {
        Some(Value::Int(v, _)) => Ok(*v),
        other => Err(Signal::Unsupported(format!(
            "`{name}`'s argument {at} must be an integer, got {}",
            other.map_or_else(|| "nothing".to_owned(), super::value::Value::kind)
        ))),
    }
}

/// The `List[byte]` payload at `at`.
///
/// The discipline is the net byte writer's, deliberately: an int outside the
/// octet is the declared `invalid` ROW (a program can handle it), while a
/// wrong TYPE is `unsupported` (the static tier owns it, and a row the call
/// never declared would give a handler an unresolvable arm).
#[allow(clippy::type_complexity)]
fn fs_bytes_arg(args: &[Value], at: usize, name: &str) -> Result<Result<Vec<u8>, Row>, Signal> {
    let Some(slots) = args.get(at).and_then(Value::seq_slots) else {
        return Err(Signal::Unsupported(format!(
            "`{name}` takes a path and a `List[byte]` payload"
        )));
    };
    let mut bytes = Vec::with_capacity(slots.len());
    for slot in slots {
        match &slot.value {
            Value::Byte(b) => bytes.push(*b),
            Value::Int(v, _) if (0..=255).contains(v) => {
                bytes.push(u8::try_from(*v).expect("checked 0..=255"));
            }
            Value::Int(..) => return Ok(Err("invalid")),
            other => {
                return Err(Signal::Unsupported(format!(
                    "`{name}`'s payload elements must be bytes, got {}",
                    other.kind()
                )));
            }
        }
    }
    Ok(Ok(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory of this test's own, under the crate's `target/`.
    ///
    /// `CARGO_TARGET_TMPDIR` is an integration-test variable and does not
    /// exist for a lib unit test, so the manifest dir is the anchor. One
    /// directory per test, rebuilt each run, because these tests write real
    /// files and `cargo test` runs them in parallel.
    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("is48-fs-unit")
            .join(name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("stale scratch removed");
        }
        std::fs::create_dir_all(&dir).expect("scratch created");
        dir
    }

    #[test]
    fn the_mode_set_is_a_set_and_a_bad_mode_never_touches_the_filesystem() {
        // `[os.fs.open]`: "A mode outside the set is `invalid`, decided
        // BEFORE the filesystem is touched, so a bad mode is never a
        // half-done open."
        for mode in [0, 1, 2, 3, 4, 5] {
            assert!(open_options(mode).is_ok(), "mode {mode} is in the set");
        }
        for mode in [-1, 6, 99, i128::MAX, i128::MIN] {
            assert_eq!(open_options(mode).err(), Some("invalid"), "mode {mode}");
        }
    }

    #[test]
    fn a_bad_mode_beats_a_missing_path_to_the_answer() {
        // The ordering the clause fixes, as a test: the path below does not
        // exist, and the answer is still `invalid`, not `not_found`.
        let dir = scratch("fs-mode-order");
        let mut table = FsTable::default();
        let missing = dir.join("no-such-dir").join("x.txt");
        let answer = table.open(&missing, 99);
        assert!(matches!(answer, Err(FsErr::Row("invalid"))), "{answer:?}");
        let answer = table.open(&missing, 0);
        assert!(matches!(answer, Err(FsErr::Row("not_found"))), "{answer:?}");
    }

    #[test]
    fn a_forged_or_closed_handle_is_the_io_row() {
        // `corpus/fs/fstat.lu` pins both halves: `closed_is_io` and
        // `forged_is_io`.
        let dir = scratch("fs-handles");
        let path = dir.join("a.txt");
        write_text(&path, "12345").expect("written");
        let mut table = FsTable::default();

        for forged in [999_999, 0, -1, i128::MIN] {
            assert!(matches!(table.fstat(forged), Err(FsErr::Row("io"))));
            assert!(matches!(table.close(forged), Err(FsErr::Row("io"))));
            assert!(matches!(table.read_text(forged, 4), Err(FsErr::Row("io"))));
        }

        let fd = table.open(&path, 0).expect("opens");
        assert!(table.fstat(fd).is_ok());
        table.close(fd).expect("closes");
        assert!(matches!(table.fstat(fd), Err(FsErr::Row("io"))));
        assert!(matches!(table.close(fd), Err(FsErr::Row("io"))));
    }

    #[test]
    fn a_handle_is_never_reused() {
        // The spent slot is what makes `closed_is_io` stable: if handles
        // were reused, a stale handle would silently address a live file.
        let dir = scratch("fs-no-reuse");
        let path = dir.join("a.txt");
        write_text(&path, "x").expect("written");
        let mut table = FsTable::default();
        let first = table.open(&path, 0).expect("opens");
        table.close(first).expect("closes");
        let second = table.open(&path, 0).expect("opens");
        assert_ne!(first, second, "a closed handle's number came back");
    }

    #[test]
    fn a_read_past_the_end_is_the_eof_row_and_a_short_read_is_not() {
        // The row no corpus witness exercises (none of them reads twice).
        let dir = scratch("fs-eof");
        let path = dir.join("a.txt");
        write_text(&path, "12345").expect("written");
        let mut table = FsTable::default();
        let fd = table.open(&path, 0).expect("opens");
        // Short: wanted 16, got 5, and that is an ANSWER.
        assert_eq!(table.read_text(fd, 16).expect("reads"), "12345");
        // Now there is nothing left.
        assert!(matches!(table.read_text(fd, 16), Err(FsErr::Row("eof"))));
        assert!(matches!(table.read_bytes(fd, 16), Err(FsErr::Row("eof"))));
        // A zero-length read never asks the file, so it is never `eof`.
        assert_eq!(table.read_text(fd, 0).expect("zero reads"), "");
        table.close(fd).expect("closes");
    }

    #[test]
    fn bytes_that_are_not_text_are_the_utf8_row_on_both_readers() {
        // `corpus/fs/bytes_dirs.lu`'s "text refused" — with the tag named,
        // which the witness itself does not pin (it takes `_`).
        let dir = scratch("fs-utf8");
        let path = dir.join("b.bin");
        write_bytes(&path, &[128, 0, 255, 65]).expect("written");
        assert!(matches!(read_text(&path), Err(FsErr::Row("utf8"))));
        assert_eq!(read_bytes(&path).expect("reads"), vec![128, 0, 255, 65]);

        let mut table = FsTable::default();
        let fd = table.open(&path, 0).expect("opens");
        assert!(matches!(table.read_text(fd, 8), Err(FsErr::Row("utf8"))));
        table.close(fd).expect("closes");
    }

    #[test]
    fn the_six_modes_do_what_the_clause_says() {
        let dir = scratch("fs-modes");
        let mut table = FsTable::default();

        // 1: write, create, TRUNCATE.
        let truncated = dir.join("m1.txt");
        write_text(&truncated, "abcdef").expect("written");
        let fd = table.open(&truncated, 1).expect("opens");
        table.close(fd).expect("closes");
        assert_eq!(size(&truncated).expect("sized"), 0);

        // 2: append, create — two opens, neither reading what was there.
        let log = dir.join("m2.txt");
        for word in ["one|", "two"] {
            let fd = table.open(&log, 2).expect("opens");
            table.write(fd, word).expect("writes");
            table.close(fd).expect("closes");
        }
        assert_eq!(read_text(&log).expect("reads"), "one|two");

        // 3: read-write, create, NO truncate — and one shared cursor.
        let both = dir.join("m3.txt");
        write_text(&both, "abcdef").expect("written");
        let fd = table.open(&both, 3).expect("opens");
        assert_eq!(table.read_text(fd, 3).expect("reads"), "abc");
        table.write(fd, "XYZ").expect("writes");
        table.close(fd).expect("closes");
        assert_eq!(read_text(&both).expect("reads"), "abcXYZ");

        // 4: create-new, exclusive — the one atomicity every tier-1 target
        // promises, so a second one is `exists`.
        let fresh = dir.join("m4.txt");
        let fd = table.open(&fresh, 4).expect("opens");
        table.close(fd).expect("closes");
        assert!(matches!(table.open(&fresh, 4), Err(FsErr::Row("exists"))));

        // 5 on a REGULAR file is mode 0 in every respect — the parity
        // `corpus/fs/open_nonblock.lu` exists to pin.
        let plain = dir.join("m5.txt");
        write_text(&plain, "12345").expect("written");
        let zero = table.open(&plain, 0).expect("opens");
        let five = table.open(&plain, 5).expect("opens");
        assert_eq!(
            table.read_text(zero, 16).expect("reads"),
            table.read_text(five, 16).expect("reads")
        );
        assert_eq!(
            table.fstat(zero).expect("stats"),
            table.fstat(five).expect("stats")
        );
        table.close(zero).expect("closes");
        table.close(five).expect("closes");
    }

    #[test]
    fn an_append_handle_does_not_read_and_a_create_new_handle_does() {
        // Probed on the compiled lane at this pin; written down because it
        // is the one place the mode table is not symmetric.
        //
        // The ROW here was a host split until CI found it: the read fails
        // with `EBADF` on macOS and `ERROR_ACCESS_DENIED` on windows, and
        // forwarding the host's kind made this same program answer `io` on
        // one and `denied` on the other. [`io_row`] now answers `io` for
        // every handle-level failure, which is `[os.fs.fstat]`'s stated
        // posture and what the compiled lane answers.
        let dir = scratch("fs-mode-reads");
        let mut table = FsTable::default();
        let path = dir.join("a.txt");
        write_text(&path, "abc").expect("written");
        let appender = table.open(&path, 2).expect("opens");
        assert!(matches!(
            table.read_text(appender, 4),
            Err(FsErr::Row("io"))
        ));
        table.close(appender).expect("closes");

        let fresh = dir.join("new.txt");
        let fd = table.open(&fresh, 4).expect("opens");
        // Empty, so the read is `eof` — an ANSWER, not the `io` of a
        // handle that cannot read at all.
        assert!(matches!(table.read_text(fd, 4), Err(FsErr::Row("eof"))));
        table.close(fd).expect("closes");
    }

    #[test]
    fn fstat_reads_the_handle_and_agrees_with_the_path() {
        // `corpus/fs/fstat.lu`'s relations, as a unit test: the handle's
        // words are the path's words, in the path's units.
        let dir = scratch("fs-fstat");
        let path = dir.join("a.txt");
        write_text(&path, "12345").expect("written");
        let mut table = FsTable::default();
        let fd = table.open(&path, 0).expect("opens");
        let [kind, bytes, stamp] = table.fstat(fd).expect("stats");
        assert_eq!(kind, 0, "a regular file");
        assert_eq!(bytes, size(&path).expect("sized"));
        assert_eq!(stamp, modified_ms_of(&path).expect("stamped"));
        // Unit sanity, not clock precision — the witness's own relation.
        assert!(stamp > 1_577_836_800_000, "after 2020 in ms: {stamp}");
        table.close(fd).expect("closes");
    }

    #[cfg(unix)]
    #[test]
    fn fstat_calls_a_directory_handle_kind_one() {
        // Reachable on the unix hosts and not on windows, where `fs_open`
        // refuses a directory — `[os.fs.fstat]` states that split by name,
        // so the test carries the same `cfg` the clause does.
        let dir = scratch("fs-fstat-dir");
        let mut table = FsTable::default();
        let fd = table.open(&dir, 0).expect("a unix host opens a directory");
        assert_eq!(table.fstat(fd).expect("stats")[0], 1);
        table.close(fd).expect("closes");
    }

    #[test]
    fn a_listing_is_sorted_bytewise_and_keeps_hidden_entries() {
        // The promise `corpus/fs/bytes_dirs.lu` depends on, with the order
        // probed against the compiled lane spelled out.
        let dir = scratch("fs-listing");
        for name in ["b.txt", "A.txt", "_u.txt", ".hidden"] {
            write_text(&dir.join(name), "x").expect("written");
        }
        create_dir_all(&dir.join("zsub")).expect("made");
        assert_eq!(
            read_dir(&dir).expect("lists"),
            vec![".hidden", "A.txt", "_u.txt", "b.txt", "zsub"]
        );
    }

    #[test]
    fn the_path_calls_answer_the_rows_the_witnesses_pin() {
        let dir = scratch("fs-path-rows");
        let missing = dir.join("nope.txt");
        assert!(matches!(read_text(&missing), Err(FsErr::Row("not_found"))));
        assert!(matches!(read_bytes(&missing), Err(FsErr::Row("not_found"))));
        assert!(matches!(size(&missing), Err(FsErr::Row("not_found"))));
        assert!(matches!(
            modified_ms_of(&missing),
            Err(FsErr::Row("not_found"))
        ));
        assert!(matches!(remove(&missing), Err(FsErr::Row("not_found"))));
        assert!(matches!(read_dir(&missing), Err(FsErr::Row("not_found"))));
        assert!(matches!(
            remove_dir_all(&missing),
            Err(FsErr::Row("not_found"))
        ));
        assert!(matches!(
            rename(&missing, &dir.join("x.txt")),
            Err(FsErr::Row("not_found"))
        ));
        // A write into a directory chain that does not exist is the same row.
        assert!(matches!(
            write_text(&dir.join("no-such").join("x.txt"), "y"),
            Err(FsErr::Row("not_found"))
        ));
        // The predicates never raise.
        assert!(!exists(&missing));
        assert!(!is_dir(&missing));
        assert!(!is_file(&missing));
    }

    #[test]
    fn a_directory_read_as_text_is_a_row_and_never_a_panic() {
        let dir = scratch("fs-dir-as-text");
        // The tag is the host's (`io` everywhere this repository runs), and
        // what is pinned here is that it is a ROW: a directory handed to a
        // text reader never traps and never decodes to something.
        assert!(matches!(read_text(&dir), Err(FsErr::Row(_))));
        assert!(matches!(read_dir(&dir.join("a.txt")), Err(FsErr::Row(_))));
    }

    #[test]
    fn containment_refuses_by_name_and_never_by_a_row() {
        // A row would be a claim about the HOST; this is a claim about this
        // implementation's surface, so it is `Outside`, which becomes
        // `unsupported`.
        for bad in ["/tmp/x", "../x", "/", "target/../../x"] {
            let answer = contained(bad, "fs_write_text");
            assert!(
                matches!(answer, Err(FsErr::Outside(_))),
                "{bad}: {answer:?}"
            );
        }
        for good in ["target/x", "target/sub/x.txt", "x.txt", "./x.txt"] {
            assert!(contained(good, "fs_open").is_ok(), "{good}");
        }
    }

    #[test]
    fn a_size_outside_int_is_the_io_row() {
        // `fs_size`'s rule, quoted by `[os.fs.fstat]`. Exercised through the
        // domain check itself, there being no way to mint a 2^63-byte file.
        let (low, high) = super::super::value::IntTy::INT.range();
        assert_eq!(in_int_domain(high), Ok(high));
        assert_eq!(in_int_domain(low), Ok(low));
        assert_eq!(in_int_domain(high + 1), Err("io"));
        assert_eq!(in_int_domain(low - 1), Err("io"));
    }

    /// Mode 5's whole reason for existing, on a REAL fifo.
    ///
    /// `[os.fs.open]`: a read open "PARKS on a fifo with no writer — the
    /// whole hand, until some writer appears", and mode 5 "answers AT ONCE:
    /// a handle, which `[os.fs.fstat]` then classifies as `kind` 2".
    ///
    /// This is the test that pins [`nonblock_flag`]'s hand-written constant:
    /// a wrong number leaves the open blocking and the test fails on the
    /// timeout instead of hanging CI. The language has no `mkfifo`, so the
    /// fifo is made with the host's tool — which is exactly why the clause
    /// says this case is the crate tests' and not a corpus witness's.
    #[cfg(unix)]
    #[test]
    fn a_fifo_with_no_writer_answers_at_once_under_mode_five() {
        use std::sync::mpsc;
        use std::time::Duration;

        let Some(_flag) = nonblock_flag() else {
            // A unix this table does not name: mode 5 is served as mode 0
            // by name, and there is nothing here to prove.
            return;
        };

        let dir = scratch("fs-fifo");
        let fifo = dir.join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(made, "the host's mkfifo built a fifo at {}", fifo.display());

        let (tx, rx) = mpsc::channel();
        let probe = fifo.clone();
        std::thread::spawn(move || {
            let mut table = FsTable::default();
            let answer = table.open(&probe, 5).map(|fd| table.fstat(fd));
            // A send failure means the receiver timed out and gave up; the
            // thread simply ends.
            let _ = tx.send(answer.map(|stat| stat.expect("the fifo stats")));
        });

        let answered = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("mode 5 answers at once on a fifo with no writer; it PARKED");
        let stat = answered.expect("a handle, not a row");
        // `[os.fs.fstat]`: a fifo is `kind` 2, "anything else".
        assert_eq!(stat[0], 2, "a fifo classifies as kind 2");
    }
}
