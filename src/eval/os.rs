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
        let Some(program) = argv.first() else {
            // An empty argv names no program (the witnessed row).
            return Err("not_found");
        };
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
            Err(error) => Err(match error.kind() {
                std::io::ErrorKind::NotFound => "not_found",
                std::io::ErrorKind::PermissionDenied => "denied",
                _ => "io",
            }),
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
        let Some(program) = argv.first() else {
            return Err("not_found");
        };
        if program.is_empty() {
            return Err("not_found");
        }
        let [zero, one, two] = stdio;
        let wire = |f: Option<std::fs::File>| f.map_or_else(Stdio::null, Stdio::from);
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
            Err(error) => Err(match error.kind() {
                std::io::ErrorKind::NotFound => "not_found",
                std::io::ErrorKind::PermissionDenied => "denied",
                _ => "io",
            }),
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
    /// `-N` (the platform's number) where `wait` raises `signal`.
    pub(crate) fn wait_status(&mut self, handle: i128) -> Result<i128, Row> {
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
                        return Ok(-i128::from(sig));
                    }
                }
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
}
