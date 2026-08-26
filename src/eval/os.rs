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

use std::process::{Child, Command, Stdio};

/// The children this run has spawned, by handle. A reaped child leaves its
/// slot (`None`) so its handle stays spent — handles are never reused.
#[derive(Default)]
pub(crate) struct ChildTable {
    slots: Vec<Option<Child>>,
}

/// A row tag, ready for the builtin's `error(tag)`.
pub(crate) type Row = &'static str;

impl ChildTable {
    /// `os_spawn`: argv-array only, null-wired stdio.
    ///
    /// # Errors
    ///
    /// `not_found` (empty argv, or a program path naming nothing),
    /// `denied`, `io`.
    pub(crate) fn spawn(&mut self, argv: &[String]) -> Result<i128, Row> {
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
    pub(crate) fn kill(&mut self, handle: i128) -> Result<(), Row> {
        match self.slot(handle)?.as_mut() {
            Some(child) => child.kill().map_err(|_| "io"),
            None => Err("io"),
        }
    }

    fn slot(&mut self, handle: i128) -> Result<&mut Option<Child>, Row> {
        let index = usize::try_from(handle)
            .ok()
            .and_then(|h| h.checked_sub(1))
            .ok_or("io")?;
        self.slots.get_mut(index).ok_or("io")
    }
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
}
