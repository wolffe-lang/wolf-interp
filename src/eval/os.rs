//! The s40 process trio — `os_spawn` / `os_wait` / `os_kill` over
//! `std::process` (is18).
//!
//! # Independence, stated where it lives
//!
//! Written against the prelude signatures (`os_spawn(List[str]) -> int !
//! {not_found, denied, io}`; `os_wait(int) -> int ! {signal, io}`;
//! `os_kill(int) -> unit ! {io}`), the corpus witness
//! (`corpus/os/spawn_rows.lu`) and empirical probes of the compiled lanes —
//! never `wolf_rt::os`. The pinned facts:
//!
//! - argv-array ONLY; there is deliberately no shell-string spawn anywhere,
//!   by construction (injection off the table). An EMPTY argv names no
//!   program: the `not_found` row (witnessed), exactly like a program path
//!   that resolves to nothing (probed).
//! - v0 stdio is null-wired — the child inherits no stream of the
//!   interpreter's, so a child's output can never leak into a record's
//!   stdout comparison surface.
//! - **`wait` reaps** (the #50 lesson): a handle whose child was waited is
//!   spent — a second `wait`, or a `kill`, on it is the `io` row (probed).
//!   `signal` is a child that died without an exit code (killed on unix);
//!   the reap still happened — the row rides the same wait that freed the
//!   zombie.
//! - **`kill` never tombstones**: the handle stays valid, and the
//!   subsequent `wait` both reaps and reports (`signal`, or the platform's
//!   termination code where every process has one).
//! - a forged handle is `io`, never a trap (witnessed).

use std::collections::BTreeMap;
use std::process::{Child, Command, Stdio};

/// The children this run has spawned, by handle. A reaped child leaves its
/// slot (`None`) so its handle stays spent — handles are never reused.
#[derive(Default)]
pub(crate) struct ChildTable {
    slots: Vec<Option<Child>>,
    /// Number from the last spawn's program step (s260, #291).
    spawn_error: i128,
    /// s225 (`[os.env.unset]`): the machine's environment overlay, as a
    /// child is handed it — the host's environment with every `env_set`
    /// set and every `env_unset` removed, as a native child is handed its
    /// parent's.
    overlay: BTreeMap<String, Option<String>>,
    /// is76 (wolf-interp#249): the slots whose child LEADS a process group
    /// of its own (`os_spawn_job` with group 0) — the groups this run made,
    /// and so the only ones [`ChildTable::end_unreaped`] may end whole.
    leaders: std::collections::BTreeSet<usize>,
}

/// One process [`ChildTable::end_unreaped`] ended: its id, and whether its
/// process group (which it led) was ended with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ended {
    pub(crate) pid: u32,
    pub(crate) group: bool,
}

/// A row tag, ready for the builtin's `error(tag)`.
pub(crate) type Row = &'static str;

impl ChildTable {
    /// s225: record one overlay write (`Some`) or removal (`None`) for the
    /// children spawned after it.
    pub(crate) fn overlay(&mut self, name: &str, value: Option<&str>) {
        self.overlay
            .insert(name.to_owned(), value.map(str::to_owned));
    }

    /// s225: hand `command` the overlay.
    fn hand_env(&self, command: &mut Command) {
        for (name, value) in &self.overlay {
            match value {
                Some(v) => {
                    command.env(name, v);
                }
                None => {
                    command.env_remove(name);
                }
            }
        }
    }

    pub(crate) fn spawn_error(&self) -> i128 {
        self.spawn_error
    }

    /// R-is81-4: do not hand an unrecognized executable to a libc spawn
    /// path that may retry it under a shell (Darwin does). Missing files,
    /// directories and execute-permission refusals still belong to spawn.
    /// This deliberately does not support Linux binfmt_misc formats.
    #[cfg(unix)]
    fn rejects_executable_header(&self, program: &str, cwd: Option<&std::path::Path>) -> bool {
        use nix::fcntl::{AT_FDCWD, AtFlags};
        use nix::unistd::{AccessFlags, faccessat};
        use std::io::Read as _;

        if program.is_empty() || program.contains('\0') {
            return false;
        }
        let cwd = cwd.unwrap_or_else(|| std::path::Path::new("."));
        let candidates: Vec<std::path::PathBuf> = if program.contains('/') {
            vec![cwd.join(program)]
        } else {
            let path = match self.overlay.get("PATH") {
                Some(Some(path)) => path.into(),
                Some(None) => "/usr/bin:/bin".into(),
                None => std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
            };
            std::env::split_paths(&path)
                .map(|dir| cwd.join(dir).join(program))
                .collect()
        };
        for path in candidates {
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            // Ask using effective IDs, as exec does; do not read a denied
            // candidate's bytes and substitute ENOEXEC for its EACCES.
            if !metadata.is_file()
                || faccessat(AT_FDCWD, &path, AccessFlags::X_OK, AtFlags::AT_EACCESS).is_err()
            {
                continue;
            }
            let Ok(file) = std::fs::File::open(path) else {
                return false;
            };
            let mut prefix = Vec::with_capacity(4);
            if file.take(4).read_to_end(&mut prefix).is_err() {
                return false;
            }
            // The first executable PATH candidate decides, including an
            // unrecognized format: it must not fall through to another one.
            return !has_known_executable_header(&prefix);
        }
        false
    }

    /// `os_spawn`: argv-array only, null-wired stdio.
    ///
    /// # Errors
    ///
    /// `not_found` (empty argv, or a program path naming nothing),
    /// `denied`, `io`.
    #[cfg(test)]
    pub(crate) fn spawn(&mut self, argv: &[String]) -> Result<i128, Row> {
        self.spawn_in(argv, None)
    }

    /// [`ChildTable::spawn`] in `cwd` — the program's moved working
    /// directory (s215, `[os.fs.chdir]`: a spawned child starts in it).
    pub(crate) fn spawn_in(
        &mut self,
        argv: &[String],
        cwd: Option<&std::path::Path>,
    ) -> Result<i128, Row> {
        self.spawn_error = 0;
        let Some(program) = argv.first() else {
            // An empty argv names no program (the witnessed row).
            return Err("not_found");
        };
        #[cfg(unix)]
        if self.rejects_executable_header(program, cwd) {
            self.spawn_error = i128::from(nix::errno::Errno::ENOEXEC as i32);
            return Err("io");
        }
        let mut command = Command::new(program);
        command
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        self.hand_env(&mut command);
        match command.spawn() {
            Ok(child) => {
                self.slots.push(Some(child));
                Ok(self.slots.len() as i128)
            }
            Err(error) => {
                self.spawn_error = error.raw_os_error().map_or(0, i128::from);
                Err(match error.kind() {
                    std::io::ErrorKind::NotFound => "not_found",
                    std::io::ErrorKind::PermissionDenied => "denied",
                    _ => "io",
                })
            }
        }
    }

    /// `os_wait`: blocks until the child exits, REAPS it, and answers its
    /// exit code.
    ///
    /// # Errors
    ///
    /// `signal` for a child that died without an exit code (the reap still
    /// happened — the slot is spent either way); `io` for a forged or
    /// already-reaped handle, or a wait the host refuses.
    /// s215 (`[os.proc.fds]`): spawn `argv` with the child's descriptors
    /// 0, 1 and 2 set from `stdio` — `Some(file)` the child receives that
    /// file AS the descriptor, `None` this machine's posture for an
    /// unmapped one (the null device: a child's output never leaks into a
    /// record's stdout) — in `cwd` when the program has moved its working
    /// directory. A map that names a descriptor above 2, or closes one,
    /// never reaches here: the builtin refuses it by name, because placing
    /// either needs `unsafe` this crate forbids.
    pub(crate) fn spawn_fds(
        &mut self,
        argv: &[String],
        stdio: [Option<std::fs::File>; 3],
        cwd: Option<&std::path::Path>,
    ) -> Result<i128, Row> {
        self.spawn_job(argv, stdio, cwd, None)
    }

    /// s219 (`[os.proc.job]`): `spawn_fds` with a process group — `Some(0)`
    /// leads a new group, `Some(g)` joins `g` — through std's own
    /// `process_group`, the one job-control step stable Rust offers without
    /// `unsafe`. The terminal handoff and the default dispositions are the
    /// caller's to refuse by name before this is reached.
    pub(crate) fn spawn_job(
        &mut self,
        argv: &[String],
        stdio: [Option<std::fs::File>; 3],
        cwd: Option<&std::path::Path>,
        group: Option<i32>,
    ) -> Result<i128, Row> {
        self.spawn_error = 0;
        let Some(program) = argv.first() else {
            return Err("not_found");
        };
        if program.is_empty() {
            return Err("not_found");
        }
        let [zero, one, two] = stdio;
        let wire = |f: Option<std::fs::File>| f.map_or_else(Stdio::null, Stdio::from);
        #[cfg(unix)]
        if self.rejects_executable_header(program, cwd) {
            self.spawn_error = i128::from(nix::errno::Errno::ENOEXEC as i32);
            return Err("io");
        }
        let mut command = Command::new(program);
        command
            .args(&argv[1..])
            .stdin(wire(zero))
            .stdout(wire(one))
            .stderr(wire(two));
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        self.hand_env(&mut command);
        #[cfg(unix)]
        if let Some(g) = group {
            use std::os::unix::process::CommandExt as _;
            command.process_group(g);
        }
        #[cfg(not(unix))]
        if group.is_some() {
            return Err("unsupported");
        }
        match command.spawn() {
            Ok(child) => {
                if group == Some(0) {
                    self.leaders.insert(self.slots.len());
                }
                self.slots.push(Some(child));
                Ok(self.slots.len() as i128)
            }
            Err(error) => {
                self.spawn_error = error.raw_os_error().map_or(0, i128::from);
                Err(match error.kind() {
                    std::io::ErrorKind::NotFound => "not_found",
                    std::io::ErrorKind::PermissionDenied => "denied",
                    _ => "io",
                })
            }
        }
    }

    /// is76 (wolf-interp#249): end every child this run started and has not
    /// reaped, and reap it — called when the run ends in this machine's OWN
    /// refusal (`unsupported`), never on an exit or a trap.
    ///
    /// A refusal ends a program the compiler's machines would have carried
    /// on with: the program that started a job and then called
    /// `os_signal_send` (refused by name, ruling #79 R8) would have signalled
    /// and waited its job there. Left alone, the job outlived the
    /// interpreter (s240 measured a `sh` and its `sleep` running in their
    /// group after lupin 0.1.52 exited). An exit or a trap is the program's
    /// own end, and its children outlive it exactly as a native process's
    /// do, so those are left alone.
    ///
    /// A child that leads a group this run made is ended WITH its group —
    /// the job's own children are in it — by `kill -s KILL -- -PGID`
    /// through `sh` (the POSIX `kill` utility; std has no `kill(2)` and this
    /// machine admits no `unsafe`). The group is named only while its
    /// leader is unreaped, so the id is still this run's child's and cannot
    /// name a stranger's group. Every other child is ended alone
    /// (`Child::kill`, SIGKILL on unix). Each is then waited, so nothing is
    /// left a zombie of this process.
    pub(crate) fn end_unreaped(&mut self) -> Vec<Ended> {
        let mut ended = Vec::new();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let Some(mut child) = slot.take() else {
                continue;
            };
            let pid = child.id();
            let group = self.leaders.contains(&index) && end_group(pid);
            let _ = child.kill();
            let _ = child.wait();
            ended.push(Ended { pid, group });
        }
        ended
    }

    pub(crate) fn wait(&mut self, handle: i128) -> Result<i128, Row> {
        let slot = self.slot(handle)?;
        let Some(child) = slot.as_mut() else {
            // Reaped: the handle is spent (probed — wait-after-wait is io).
            return Err("io");
        };
        match child.wait() {
            Ok(status) => {
                let code = status.code();
                // The reap: the zombie is gone and the handle is spent.
                *slot = None;
                match code {
                    Some(code) => Ok(i128::from(code)),
                    None => Err("signal"),
                }
            }
            Err(_) => Err("io"),
        }
    }

    /// `os_kill`: terminate the child. The handle survives — `wait` still
    /// reaps and reports (kill never tombstones).
    ///
    /// # Errors
    ///
    /// `io` for a forged, reaped, or already-exited handle.
    /// s219 (`[os.proc.status]`): the child's process id; `io` for a forged
    /// or reaped handle.
    pub(crate) fn pid(&mut self, handle: i128) -> Result<i128, Row> {
        match self.slot(handle)?.as_ref() {
            Some(child) => Ok(i128::from(child.id())),
            None => Err("io"),
        }
    }

    /// s219 (`[os.proc.status]`): `wait`, but a death by signal N answers
    /// `-N` (the platform's number) where `wait` raises `signal` — core or
    /// not (ruling #88 R3 D).
    pub(crate) fn wait_status(&mut self, handle: i128) -> Result<i128, Row> {
        self.wait_status_as(handle, false)
    }

    /// s246 (`[os.proc.status]`, ruling #88 R3 D; wolf-interp#263): the
    /// same wait, every answer and row [`ChildTable::wait_status`]'s,
    /// except that a death by signal N that dumped core (std's
    /// `ExitStatusExt::core_dumped`, the host's `WCOREDUMP`) answers
    /// `-(256 + N)`.
    pub(crate) fn wait_status_core(&mut self, handle: i128) -> Result<i128, Row> {
        self.wait_status_as(handle, true)
    }

    fn wait_status_as(&mut self, handle: i128, core: bool) -> Result<i128, Row> {
        let slot = self.slot(handle)?;
        let Some(child) = slot.as_mut() else {
            return Err("io");
        };
        match child.wait() {
            Ok(status) => {
                *slot = None;
                if let Some(code) = status.code() {
                    return Ok(i128::from(code));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt as _;
                    if let Some(sig) = status.signal() {
                        if core && status.core_dumped() {
                            return Ok(-(256 + i128::from(sig)));
                        }
                        return Ok(-i128::from(sig));
                    }
                }
                #[cfg(not(unix))]
                let _ = core;
                Err("io")
            }
            Err(_) => Err("io"),
        }
    }

    pub(crate) fn kill(&mut self, handle: i128) -> Result<(), Row> {
        match self.slot(handle)?.as_mut() {
            Some(child) => child.kill().map_err(|_| "io"),
            None => Err("io"),
        }
    }

    /// s242 (`[os.proc.poll]`): the children not yet reaped — every slot
    /// still holding one, ended or not.
    pub(crate) fn live(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    fn slot(&mut self, handle: i128) -> Result<&mut Option<Child>, Row> {
        let index = usize::try_from(handle)
            .ok()
            .and_then(|h| h.checked_sub(1))
            .ok_or("io")?;
        self.slots.get_mut(index).ok_or("io")
    }
}

/// The formats admitted by R-is81-4's prefix guard. This is not an ELF or
/// Mach-O validator; recognized formats remain the kernel's decision.
#[cfg(unix)]
fn has_known_executable_header(prefix: &[u8]) -> bool {
    prefix.starts_with(b"#!")
        || [
            [0x7f, b'E', b'L', b'F'],
            [0xfe, 0xed, 0xfa, 0xce],
            [0xfe, 0xed, 0xfa, 0xcf],
            [0xce, 0xfa, 0xed, 0xfe],
            [0xcf, 0xfa, 0xed, 0xfe],
            [0xca, 0xfe, 0xba, 0xbe],
        ]
        .iter()
        .any(|magic| prefix.starts_with(magic))
}

/// SIGKILL to the process group `pgid` (its leader's pid), through the
/// POSIX `kill` utility of `sh`: `kill -s KILL -- -PGID`, then once more
/// 50 ms later. The second send is for a child the group's members were
/// forking while the first was delivered: a host need not hold a fork in
/// flight against a group signal, and macOS did not — the job's `sleep`
/// outlived the first send on the macOS runner (tier full run 39001823155,
/// shard 2). The leader is still unreaped all the while (the caller reaps
/// it after), so the group id stays this run's. True when the first send
/// answered 0. No host but unix has groups; elsewhere nothing is sent.
fn end_group(pgid: u32) -> bool {
    if !cfg!(unix) {
        return false;
    }
    Command::new("sh")
        .arg("-c")
        .arg(format!(
            "kill -s KILL -- -{pgid} || exit 1; sleep 0.05; kill -s KILL -- -{pgid} 2>/dev/null; exit 0"
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A quick real child, portable across the tier-1 hosts: this very test
    /// binary, asked to list a filter that matches nothing.
    fn quick_argv() -> Vec<String> {
        let exe = std::env::current_exe().expect("a current exe");
        vec![
            exe.to_string_lossy().into_owned(),
            "--list".to_owned(),
            "no_such_test_filter_".to_owned(),
        ]
    }

    #[test]
    fn an_empty_argv_is_the_not_found_row() {
        assert_eq!(ChildTable::default().spawn(&[]), Err("not_found"));
    }

    #[test]
    fn a_program_naming_nothing_is_the_not_found_row() {
        let argv = vec!["definitely/not/a/real/program-zz".to_owned()];
        assert_eq!(ChildTable::default().spawn(&argv), Err("not_found"));
    }

    #[test]
    fn a_forged_handle_is_the_io_row_for_wait_and_kill() {
        let mut table = ChildTable::default();
        assert_eq!(table.wait(99), Err("io"));
        assert_eq!(table.kill(99), Err("io"));
        assert_eq!(table.wait(0), Err("io"));
        assert_eq!(table.wait(-1), Err("io"));
    }

    #[test]
    fn wait_reaps_and_the_handle_is_spent() {
        // The zombie discipline, on a REAL child: one wait answers the exit
        // code and frees the zombie; the second wait and a late kill answer
        // `io` because the handle is spent, not because anything leaked.
        let mut table = ChildTable::default();
        let handle = table.spawn(&quick_argv()).expect("the test binary spawns");
        assert_eq!(table.wait(handle), Ok(0));
        assert_eq!(table.wait(handle), Err("io"));
        assert_eq!(table.kill(handle), Err("io"));
    }

    #[test]
    fn kill_never_tombstones_the_following_wait_reaps() {
        // Kill, then wait: the wait still owns the reap. On unix a killed
        // child has no exit code (the `signal` row); windows gives every
        // termination a code — both are the wait ANSWERING, never `io`.
        let mut table = ChildTable::default();
        let handle = table.spawn(&quick_argv()).expect("spawns");
        // The child may or may not have exited already; kill either way.
        let _ = table.kill(handle);
        let answer = table.wait(handle);
        assert!(
            matches!(answer, Ok(_) | Err("signal")),
            "wait after kill answered {answer:?}"
        );
        // Reaped now: spent handle.
        assert_eq!(table.wait(handle), Err("io"));
    }

    /// s219: a child that exits has its code; one killed has -N; a second
    /// wait and a forged handle are `io`; the pid is the child's.
    #[cfg(unix)]
    #[test]
    fn wait_status_and_pid() {
        let mut t = ChildTable::default();
        let h = t
            .spawn_job(&["false".to_owned()], [None, None, None], None, Some(0))
            .expect("false spawns");
        assert!(t.pid(h).expect("a pid") > 0);
        assert_eq!(t.wait_status(h), Ok(1));
        assert_eq!(t.wait_status(h), Err("io"));
        assert_eq!(t.pid(h), Err("io"));
        let k = t
            .spawn_job(
                &["sleep".to_owned(), "30".to_owned()],
                [None, None, None],
                None,
                None,
            )
            .expect("sleep spawns");
        t.kill(k).expect("kill");
        assert_eq!(t.wait_status(k), Ok(-9));
        assert_eq!(t.pid(4096), Err("io"));
    }

    /// s246 (wolf-interp#263): the sibling wait answers as the released one
    /// for an exit and for a signal that writes no core (`KILL`); a second
    /// wait and a forged handle are `io`. (The core itself is
    /// `tests/core_is77.rs`'s, in a container.)
    #[cfg(unix)]
    #[test]
    fn wait_status_core_without_a_core_is_wait_status() {
        let mut t = ChildTable::default();
        let h = t
            .spawn_job(&["false".to_owned()], [None, None, None], None, Some(0))
            .expect("false spawns");
        assert_eq!(t.wait_status_core(h), Ok(1));
        assert_eq!(t.wait_status_core(h), Err("io"));
        let k = t
            .spawn_job(
                &["sleep".to_owned(), "30".to_owned()],
                [None, None, None],
                None,
                None,
            )
            .expect("sleep spawns");
        t.kill(k).expect("kill");
        assert_eq!(t.wait_status_core(k), Ok(-9));
        assert_eq!(t.wait_status_core(4096), Err("io"));
    }
}

#[cfg(all(test, unix))]
mod executable_header_is81 {
    use super::has_known_executable_header;
    #[test]
    fn scripts_elf_and_both_mach_endiannesses_are_recognized() {
        for prefix in [
            b"#!".as_slice(),
            b"\x7fELF",
            b"\xfe\xed\xfa\xce",
            b"\xfe\xed\xfa\xcf",
            b"\xce\xfa\xed\xfe",
            b"\xcf\xfa\xed\xfe",
            b"\xca\xfe\xba\xbe",
        ] {
            assert!(has_known_executable_header(prefix), "{prefix:?}");
        }
        for prefix in [b"".as_slice(), b"#", b"echo", b"\0\0\0\0", b"\x7fEL"] {
            assert!(!has_known_executable_header(prefix), "{prefix:?}");
        }
    }
}
