//! s253's host calls (wolf-lang#847; wolf-interp#277): which OS and which
//! architecture the program runs on (`[os.uname]`), and the core a
//! process's end may leave (`[os.proc.core]`).
//!
//! Written from the clauses and black-box runs of the compilers, never
//! `wolf_rt`. The unix half is nix's safe wrappers of `uname(2)`,
//! `getrlimit`/`setrlimit(RLIMIT_CORE)` and linux's
//! `prctl(PR_SET_DUMPABLE)`; the workspace's `unsafe_code = "forbid"`
//! stands. Windows has none of them: `os_name` and `os_arch` answer
//! `windows` and the build's architecture, the core calls `unsupported`.

/// `[os.uname]`'s two words for this host, read once a process: the
/// kernel's `sysname` and `machine`, lowercased, Darwin as `macos` and the
/// BSDs' and macOS's `arm64`/`amd64` as linux spells them.
pub(crate) fn host_words() -> Option<(&'static str, &'static str)> {
    static WORDS: std::sync::OnceLock<Option<(String, String)>> = std::sync::OnceLock::new();
    WORDS
        .get_or_init(read_words)
        .as_ref()
        .map(|(name, arch)| (name.as_str(), arch.as_str()))
}

#[cfg(all(unix, not(target_family = "wasm")))]
fn read_words() -> Option<(String, String)> {
    let uts = nix::sys::utsname::uname().ok()?;
    let name = uts.sysname().to_string_lossy().to_lowercase();
    let arch = uts.machine().to_string_lossy().to_lowercase();
    Some((spell_name(&name), spell_arch(&arch)))
}

#[cfg(windows)]
fn read_words() -> Option<(String, String)> {
    Some(("windows".to_owned(), std::env::consts::ARCH.to_owned()))
}

#[cfg(not(any(windows, all(unix, not(target_family = "wasm")))))]
fn read_words() -> Option<(String, String)> {
    None
}

/// A kernel's lowercased `sysname` in `[os.uname]`'s vocabulary.
#[cfg_attr(not(all(unix, not(target_family = "wasm"))), allow(dead_code))]
fn spell_name(sysname: &str) -> String {
    match sysname {
        "darwin" => "macos".to_owned(),
        other => other.to_owned(),
    }
}

/// A kernel's lowercased `machine` in `[os.uname]`'s vocabulary.
#[cfg_attr(not(all(unix, not(target_family = "wasm"))), allow(dead_code))]
fn spell_arch(machine: &str) -> String {
    match machine {
        "arm64" => "aarch64".to_owned(),
        "amd64" => "x86_64".to_owned(),
        other => other.to_owned(),
    }
}

/// `os_core_limit`: soft and hard, in bytes, -1 for unlimited; `None` where
/// the host has no such limit (windows: the `unsupported` row).
#[cfg(all(unix, not(target_family = "wasm")))]
pub(crate) fn core_limit() -> Option<(i64, i64)> {
    use nix::sys::resource::{Resource, getrlimit};
    let (soft, hard) = getrlimit(Resource::RLIMIT_CORE).ok()?;
    Some((from_rlim(soft), from_rlim(hard)))
}

#[cfg(not(all(unix, not(target_family = "wasm"))))]
pub(crate) fn core_limit() -> Option<(i64, i64)> {
    None
}

/// `os_set_core_limit` past its shape check: `Err(tag)` is the row.
#[cfg(all(unix, not(target_family = "wasm")))]
pub(crate) fn set_core_limit(soft: i64, hard: i64) -> Result<(), &'static str> {
    use nix::sys::resource::{Resource, setrlimit};
    match setrlimit(Resource::RLIMIT_CORE, to_rlim(soft), to_rlim(hard)) {
        Ok(()) => Ok(()),
        Err(nix::errno::Errno::EINVAL) => Err("invalid"),
        Err(_) => Err("denied"),
    }
}

#[cfg(not(all(unix, not(target_family = "wasm"))))]
pub(crate) fn set_core_limit(_soft: i64, _hard: i64) -> Result<(), &'static str> {
    Err("unsupported")
}

/// `os_core_off`: on linux the process's dumpable attribute to 0 (the one
/// switch a piped core pattern obeys); elsewhere the soft core limit to 0,
/// keeping the hard.
#[cfg(target_os = "linux")]
pub(crate) fn core_off() -> Result<(), &'static str> {
    nix::sys::prctl::set_dumpable(false).map_err(|_| "unsupported")
}

#[cfg(all(unix, not(target_os = "linux"), not(target_family = "wasm")))]
pub(crate) fn core_off() -> Result<(), &'static str> {
    use nix::sys::resource::{Resource, getrlimit, setrlimit};
    let (_, hard) = getrlimit(Resource::RLIMIT_CORE).map_err(|_| "unsupported")?;
    setrlimit(Resource::RLIMIT_CORE, 0, hard).map_err(|_| "unsupported")
}

#[cfg(not(all(unix, not(target_family = "wasm"))))]
pub(crate) fn core_off() -> Result<(), &'static str> {
    Err("unsupported")
}

#[cfg(all(unix, not(target_family = "wasm")))]
fn from_rlim(value: nix::libc::rlim_t) -> i64 {
    if value == nix::libc::RLIM_INFINITY {
        -1
    } else {
        i64::try_from(value).unwrap_or(i64::MAX)
    }
}

#[cfg(all(unix, not(target_family = "wasm")))]
fn to_rlim(value: i64) -> nix::libc::rlim_t {
    if value < 0 {
        nix::libc::RLIM_INFINITY
    } else {
        value as nix::libc::rlim_t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kernels_spellings_become_the_clauses_words() {
        assert_eq!(spell_name("darwin"), "macos");
        assert_eq!(spell_name("linux"), "linux");
        assert_eq!(spell_name("freebsd"), "freebsd");
        assert_eq!(spell_name("pax"), "pax");
        assert_eq!(spell_arch("arm64"), "aarch64");
        assert_eq!(spell_arch("amd64"), "x86_64");
        assert_eq!(spell_arch("x86_64"), "x86_64");
        assert_eq!(spell_arch("riscv64"), "riscv64");
    }
}
