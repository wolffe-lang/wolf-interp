//! The corpus directive parser.
//!
//! **Independently reimplemented** — see CONTRIBUTING.md. The compiler track
//! has its own parser for the same grammar; a divergence between the two is a
//! finding, which is only true if neither reads the other.
//!
//! # Grammar
//!
//! A corpus file may open with a block of `//!` lines. A line in that block is
//! a *directive* when its first `:`-delimited token is one of the four known
//! keys; every other `//!` line is prose (prose routinely contains colons, so
//! an unknown key is never an error — it is just prose).
//!
//! ```text
//! check:    pass
//!         | fail(CODE)
//!         | run( exit=N | exit=trap | exit=trap(kind) [, stdout="…"] )
//! phase:    none | lex | parse | resolve | typecheck | mem | wir | run
//! conforms: anchor, anchor, …
//! member:   true | false
//! ```
//!
//! `kind` is drawn from the closed eleven-kind vocabulary of
//! `spec/05-conformance.md` `[conf.trap.set]`; `phase` from the canonical
//! ladder. Both sets are closed: an unknown value is an error.
//!
//! # Semantics
//!
//! - A duplicate key is an error, always.
//! - `member: true` marks a file that belongs to a multi-file module case
//!   (directory = module; the package root is the entry file's directory). It
//!   is compiled through its directory's entry file and is **never**
//!   conform-run directly, so it carries no `check:` and no `phase:` — either
//!   alongside `member: true` is an error. It may carry `conforms:`.
//! - A non-member file is an entry: it must carry both `check:` and `phase:`.
//!
//! Until the queued spec/05 amendment lands, the grammar above is the contract
//! this module implements.

use std::fmt;

use crate::anchor;
use crate::phase::Phase;
use crate::trap::TrapKind;

/// The four directive keys. Anything else on a `//!` line is prose.
pub const KEYS: [&str; 4] = ["check", "phase", "conforms", "member"];

/// What a corpus file is expected to do (`check:`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// Reaches the requested phase cleanly.
    Pass,
    /// Rejected with this diagnostic code.
    Fail(String),
    /// Executes, with this termination (and optionally this stdout).
    Run {
        exit: ExitSpec,
        stdout: Option<String>,
    },
}

/// How a `run(…)` program is expected to terminate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitSpec {
    /// `exit=N` — a plain process exit status.
    Code(u8),
    /// `exit=trap` (kind unspecified) or `exit=trap(kind)`.
    Trap(Option<TrapKind>),
}

impl fmt::Display for ExitSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExitSpec::Code(n) => write!(f, "exit={n}"),
            ExitSpec::Trap(None) => f.write_str("exit=trap"),
            ExitSpec::Trap(Some(kind)) => write!(f, "exit=trap({kind})"),
        }
    }
}

impl fmt::Display for Check {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Check::Pass => f.write_str("pass"),
            Check::Fail(code) => write!(f, "fail({code})"),
            Check::Run { exit, stdout: None } => write!(f, "run({exit})"),
            Check::Run {
                exit,
                stdout: Some(text),
            } => write!(f, "run({exit}, stdout={text:?})"),
        }
    }
}

/// The parsed header of one corpus file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Directives {
    /// `check:` — present on every entry file, absent on members.
    pub check: Option<Check>,
    /// `phase:` — present on every entry file, absent on members.
    pub phase: Option<Phase>,
    /// `conforms:` — clause anchors, in source order. May be empty.
    pub conforms: Vec<String>,
    /// `member: true` — belongs to a multi-file module case.
    pub member: bool,
    /// Non-directive `//!` lines, in source order, `//!` and one leading space
    /// stripped. Retained so callers can prove prose was not misparsed.
    pub prose: Vec<String>,
}

impl Directives {
    /// True when this file is an entry: a file conform-run directly.
    #[must_use]
    pub fn is_entry(&self) -> bool {
        !self.member
    }
}

/// A malformed header, located at the offending line where one exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveError {
    /// 1-based line number, or `None` for whole-header errors.
    pub line: Option<usize>,
    pub message: String,
}

impl DirectiveError {
    fn at(line: usize, message: impl Into<String>) -> DirectiveError {
        DirectiveError {
            line: Some(line),
            message: message.into(),
        }
    }

    fn header(message: impl Into<String>) -> DirectiveError {
        DirectiveError {
            line: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for DirectiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => write!(f, "header: {}", self.message),
        }
    }
}

impl std::error::Error for DirectiveError {}

/// One `key: value` sighting, kept with its line so duplicates can point at
/// the offender rather than the file.
struct Seen<T> {
    value: T,
    line: usize,
}

/// Parses the leading `//!` directive block of a corpus source.
///
/// # Errors
///
/// Returns [`DirectiveError`] for an unknown `phase:`/trap value, a malformed
/// `check:`, a bad anchor, a duplicate key, a member file carrying
/// `check:`/`phase:`, or an entry file missing either.
pub fn parse_header(source: &str) -> Result<Directives, DirectiveError> {
    let mut check: Option<Seen<Check>> = None;
    let mut phase: Option<Seen<Phase>> = None;
    let mut conforms: Option<Seen<Vec<String>>> = None;
    let mut member: Option<Seen<bool>> = None;
    let mut prose = Vec::new();

    for (index, raw) in source.lines().enumerate() {
        let lineno = index + 1;
        // The block is the *leading* run of `//!` lines: the first line that
        // is not one ends it. (A stray `//!` deeper in the file is a comment,
        // not a directive.)
        let Some(rest) = raw.strip_prefix("//!") else {
            break;
        };
        let rest = rest.trim();
        if rest.is_empty() {
            prose.push(String::new());
            continue;
        }

        let Some((key, value)) = split_directive(rest) else {
            prose.push(rest.to_owned());
            continue;
        };

        match key {
            "check" => {
                reject_duplicate(check.as_ref(), "check", lineno)?;
                check = Some(Seen {
                    value: parse_check(value).map_err(|m| DirectiveError::at(lineno, m))?,
                    line: lineno,
                });
            }
            "phase" => {
                reject_duplicate(phase.as_ref(), "phase", lineno)?;
                let parsed = Phase::parse(value)
                    .ok_or_else(|| DirectiveError::at(lineno, format!("unknown phase `{value}`; canonical ladder is none, lex, parse, resolve, typecheck, mem, wir, run")))?;
                phase = Some(Seen {
                    value: parsed,
                    line: lineno,
                });
            }
            "conforms" => {
                reject_duplicate(conforms.as_ref(), "conforms", lineno)?;
                conforms = Some(Seen {
                    value: parse_conforms(value).map_err(|m| DirectiveError::at(lineno, m))?,
                    line: lineno,
                });
            }
            "member" => {
                reject_duplicate(member.as_ref(), "member", lineno)?;
                let parsed = match value {
                    "true" => true,
                    "false" => false,
                    other => {
                        return Err(DirectiveError::at(
                            lineno,
                            format!("`member:` takes `true` or `false`, not `{other}`"),
                        ));
                    }
                };
                member = Some(Seen {
                    value: parsed,
                    line: lineno,
                });
            }
            _ => unreachable!("split_directive only yields known keys"),
        }
    }

    let is_member = member.as_ref().is_some_and(|m| m.value);
    if is_member {
        if let Some(seen) = &check {
            return Err(DirectiveError::at(
                seen.line,
                "a `member: true` file is exercised through its module's entry file and is never conform-run directly, so it carries no `check:`",
            ));
        }
        if let Some(seen) = &phase {
            return Err(DirectiveError::at(
                seen.line,
                "a `member: true` file is exercised through its module's entry file and is never conform-run directly, so it carries no `phase:`",
            ));
        }
    } else {
        if check.is_none() {
            return Err(DirectiveError::header(
                "entry file has no `check:` directive (add one, or mark the file `member: true`)",
            ));
        }
        if phase.is_none() {
            return Err(DirectiveError::header(
                "entry file has no `phase:` directive (add one, or mark the file `member: true`)",
            ));
        }
    }

    Ok(Directives {
        check: check.map(|s| s.value),
        phase: phase.map(|s| s.value),
        conforms: conforms.map(|s| s.value).unwrap_or_default(),
        member: is_member,
        prose,
    })
}

fn reject_duplicate<T>(
    existing: Option<&Seen<T>>,
    key: &str,
    lineno: usize,
) -> Result<(), DirectiveError> {
    match existing {
        Some(seen) => Err(DirectiveError::at(
            lineno,
            format!(
                "duplicate `{key}:` directive (first seen on line {})",
                seen.line
            ),
        )),
        None => Ok(()),
    }
}

/// Splits a header line into `(key, value)` when it opens with a known key.
/// Prose is anything else — including prose that happens to contain a colon.
fn split_directive(line: &str) -> Option<(&str, &str)> {
    let (head, tail) = line.split_once(':')?;
    let key = head.trim();
    if KEYS.contains(&key) {
        Some((key, tail.trim()))
    } else {
        None
    }
}

fn parse_check(value: &str) -> Result<Check, String> {
    if value == "pass" {
        return Ok(Check::Pass);
    }
    if let Some(inner) = call_args(value, "fail") {
        let code = inner.trim();
        if code.is_empty() {
            return Err("`fail()` needs a diagnostic code, e.g. `fail(E1002)`".to_owned());
        }
        if !code.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!(
                "`fail({code})`: a diagnostic code is alphanumeric, e.g. `E1002`"
            ));
        }
        return Ok(Check::Fail(code.to_owned()));
    }
    if let Some(inner) = call_args(value, "run") {
        return parse_run(inner);
    }
    Err(format!(
        "unknown `check:` value `{value}`; expected `pass`, `fail(CODE)` or `run(…)`"
    ))
}

/// `name(…)` → the text between the outermost parentheses.
fn call_args<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    let rest = value.strip_prefix(name)?;
    let rest = rest.strip_prefix('(')?;
    rest.strip_suffix(')')
}

fn parse_run(inner: &str) -> Result<Check, String> {
    let mut exit: Option<ExitSpec> = None;
    let mut stdout: Option<String> = None;

    for arg in split_args(inner)? {
        let arg = arg.trim();
        if arg.is_empty() {
            return Err("empty argument in `run(…)`".to_owned());
        }
        let (key, value) = arg
            .split_once('=')
            .ok_or_else(|| format!("`run(…)` argument `{arg}` is not `key=value`"))?;
        match key.trim() {
            "exit" => {
                if exit.is_some() {
                    return Err("duplicate `exit=` in `run(…)`".to_owned());
                }
                exit = Some(parse_exit(value.trim())?);
            }
            "stdout" => {
                if stdout.is_some() {
                    return Err("duplicate `stdout=` in `run(…)`".to_owned());
                }
                stdout = Some(parse_quoted(value.trim())?);
            }
            other => {
                return Err(format!(
                    "unknown `run(…)` argument `{other}`; expected `exit` or `stdout`"
                ));
            }
        }
    }

    let exit = exit.ok_or_else(|| "`run(…)` requires `exit=`".to_owned())?;
    Ok(Check::Run { exit, stdout })
}

fn parse_exit(value: &str) -> Result<ExitSpec, String> {
    if value == "trap" {
        return Ok(ExitSpec::Trap(None));
    }
    if let Some(kind) = call_args(value, "trap") {
        let kind = kind.trim();
        let parsed = TrapKind::parse(kind).ok_or_else(|| {
            format!(
                "unknown trap kind `{kind}`; the vocabulary is closed: {}",
                TrapKind::vocabulary()
            )
        })?;
        return Ok(ExitSpec::Trap(Some(parsed)));
    }
    let code: u8 = value.parse().map_err(|_| {
        format!("`exit={value}`: expected an exit status in 0..=255, `trap`, or `trap(kind)`")
    })?;
    Ok(ExitSpec::Code(code))
}

/// Splits `run(…)` arguments on top-level commas — commas nested inside
/// `trap(…)` or inside a quoted `stdout` string do not separate arguments.
fn split_args(inner: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for ch in inner.chars() {
        if in_string {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                current.push(ch);
            }
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| "unbalanced `)` in `run(…)`".to_owned())?;
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }

    if in_string {
        return Err("unterminated string in `run(…)`".to_owned());
    }
    if depth != 0 {
        return Err("unbalanced `(` in `run(…)`".to_owned());
    }
    if !current.trim().is_empty() || !args.is_empty() {
        args.push(current);
    }
    Ok(args)
}

/// A double-quoted directive string with `\\`, `\"`, `\n`, `\t` escapes.
fn parse_quoted(value: &str) -> Result<String, String> {
    let body = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .ok_or_else(|| format!("expected a double-quoted string, got `{value}`"))?;

    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            if ch == '"' {
                return Err("unescaped `\"` inside a directive string".to_owned());
            }
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => return Err(format!("unknown escape `\\{other}` in a directive string")),
            None => return Err("directive string ends in a lone `\\`".to_owned()),
        }
    }
    Ok(out)
}

fn parse_conforms(value: &str) -> Result<Vec<String>, String> {
    if value.trim().is_empty() {
        return Err("`conforms:` needs at least one anchor".to_owned());
    }
    let mut anchors = Vec::new();
    for raw in value.split(',') {
        let tag = raw.trim();
        if tag.is_empty() {
            return Err("empty anchor in `conforms:` (stray comma?)".to_owned());
        }
        anchor::classify(tag).map_err(|e| e.to_string())?;
        if anchors.contains(&tag.to_owned()) {
            return Err(format!("anchor `{tag}` listed twice in `conforms:`"));
        }
        anchors.push(tag.to_owned());
    }
    Ok(anchors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(source: &str) -> Directives {
        parse_header(source).expect("header should parse")
    }

    fn err(source: &str) -> String {
        parse_header(source)
            .expect_err("header should not parse")
            .to_string()
    }

    // ---- check: -----------------------------------------------------------

    #[test]
    fn check_pass() {
        let d = ok("//! check: pass\n//! phase: parse\n");
        assert_eq!(d.check, Some(Check::Pass));
        assert_eq!(d.phase, Some(Phase::Parse));
    }

    #[test]
    fn check_fail_carries_the_code() {
        let d = ok("//! check: fail(E1002)\n//! phase: resolve\n");
        assert_eq!(d.check, Some(Check::Fail("E1002".to_owned())));
    }

    #[test]
    fn check_fail_needs_a_code() {
        assert!(err("//! check: fail()\n//! phase: lex\n").contains("needs a diagnostic code"));
        assert!(err("//! check: fail(E 2)\n//! phase: lex\n").contains("alphanumeric"));
    }

    #[test]
    fn check_run_exit_code() {
        let d = ok("//! check: run(exit=2)\n//! phase: run\n");
        assert_eq!(
            d.check,
            Some(Check::Run {
                exit: ExitSpec::Code(2),
                stdout: None
            })
        );
    }

    #[test]
    fn check_run_exit_status_is_bounded() {
        assert!(err("//! check: run(exit=256)\n//! phase: run\n").contains("0..=255"));
        assert!(err("//! check: run(exit=-1)\n//! phase: run\n").contains("0..=255"));
    }

    #[test]
    fn check_run_bare_trap() {
        let d = ok("//! check: run(exit=trap)\n//! phase: run\n");
        assert_eq!(
            d.check,
            Some(Check::Run {
                exit: ExitSpec::Trap(None),
                stdout: None
            })
        );
    }

    #[test]
    fn check_run_every_trap_kind_is_accepted() {
        for kind in TrapKind::ALL {
            let src = format!("//! check: run(exit=trap({kind}))\n//! phase: run\n");
            let d = ok(&src);
            assert_eq!(
                d.check,
                Some(Check::Run {
                    exit: ExitSpec::Trap(Some(kind)),
                    stdout: None
                })
            );
        }
    }

    #[test]
    fn check_run_rejects_unknown_trap_kinds() {
        let message = err("//! check: run(exit=trap(segfault))\n//! phase: run\n");
        assert!(
            message.contains("unknown trap kind `segfault`"),
            "{message}"
        );
        assert!(message.contains("vocabulary is closed"), "{message}");
        // A near-miss spelling is still outside the closed set.
        assert!(err("//! check: run(exit=trap(div_zero))\n//! phase: run\n").contains("div_zero"));
    }

    #[test]
    fn check_run_with_stdout() {
        let d = ok("//! check: run(exit=0, stdout=\"hello, wolf\")\n//! phase: run\n");
        assert_eq!(
            d.check,
            Some(Check::Run {
                exit: ExitSpec::Code(0),
                stdout: Some("hello, wolf".to_owned())
            })
        );
    }

    #[test]
    fn stdout_commas_do_not_split_arguments() {
        let d = ok("//! check: run(exit=0, stdout=\"a, b, c\")\n//! phase: run\n");
        let Some(Check::Run { stdout, .. }) = d.check else {
            panic!("expected run(…)");
        };
        assert_eq!(stdout.as_deref(), Some("a, b, c"));
    }

    #[test]
    fn stdout_escapes() {
        let d = ok("//! check: run(exit=0, stdout=\"a\\nb\\t\\\"c\\\"\\\\\")\n//! phase: run\n");
        let Some(Check::Run { stdout, .. }) = d.check else {
            panic!("expected run(…)");
        };
        assert_eq!(stdout.as_deref(), Some("a\nb\t\"c\"\\"));
        assert!(
            err("//! check: run(exit=0, stdout=\"a\\qb\")\n//! phase: run\n")
                .contains("unknown escape")
        );
    }

    #[test]
    fn run_argument_hygiene() {
        assert!(err("//! check: run()\n//! phase: run\n").contains("requires `exit=`"));
        assert!(err("//! check: run(stdout=\"x\")\n//! phase: run\n").contains("requires `exit=`"));
        assert!(
            err("//! check: run(exit=0, exit=1)\n//! phase: run\n").contains("duplicate `exit=`")
        );
        assert!(
            err("//! check: run(exit=0, stderr=\"x\")\n//! phase: run\n")
                .contains("unknown `run(…)` argument")
        );
        assert!(err("//! check: run(exit=0, )\n//! phase: run\n").contains("empty argument"));
        assert!(
            err("//! check: run(exit=0, stdout=\"oops)\n//! phase: run\n")
                .contains("unterminated string")
        );
    }

    #[test]
    fn unknown_check_shapes_are_rejected() {
        assert!(err("//! check: succeed\n//! phase: run\n").contains("unknown `check:` value"));
        assert!(err("//! check: PASS\n//! phase: run\n").contains("unknown `check:` value"));
    }

    // ---- phase: -----------------------------------------------------------

    #[test]
    fn every_ladder_rung_parses() {
        for phase in Phase::LADDER {
            let d = ok(&format!("//! check: pass\n//! phase: {phase}\n"));
            assert_eq!(d.phase, Some(phase));
        }
    }

    #[test]
    fn unknown_phases_are_rejected() {
        let message = err("//! check: pass\n//! phase: codegen\n");
        assert!(message.contains("unknown phase `codegen`"), "{message}");
        assert!(message.contains("canonical ladder"), "{message}");
    }

    // ---- conforms: --------------------------------------------------------

    #[test]
    fn conforms_is_a_comma_separated_anchor_list() {
        let d = ok("//! check: pass\n//! phase: parse\n//! conforms: gram.amb.bang, str.interp\n");
        assert_eq!(d.conforms, vec!["gram.amb.bang", "str.interp"]);
    }

    #[test]
    fn conforms_is_optional() {
        assert!(
            ok("//! check: pass\n//! phase: parse\n")
                .conforms
                .is_empty()
        );
    }

    #[test]
    fn conforms_rejects_bad_anchors() {
        assert!(
            err("//! check: pass\n//! phase: parse\n//! conforms:\n")
                .contains("at least one anchor")
        );
        assert!(
            err("//! check: pass\n//! phase: parse\n//! conforms: mem.a,,mem.b\n")
                .contains("empty anchor")
        );
        assert!(
            err("//! check: pass\n//! phase: parse\n//! conforms: nope.a\n")
                .contains("unknown namespace")
        );
        assert!(
            err("//! check: pass\n//! phase: parse\n//! conforms: mem.a, mem.a\n")
                .contains("listed twice")
        );
    }

    // ---- member: ----------------------------------------------------------

    #[test]
    fn member_files_need_nothing_else() {
        let d = ok("//! member: true\n");
        assert!(d.member);
        assert!(!d.is_entry());
        assert_eq!(d.check, None);
        assert_eq!(d.phase, None);
    }

    #[test]
    fn member_files_may_carry_conforms_and_prose() {
        let d =
            ok("//! member: true\n//! conforms: mod.vis.pub\n//!\n//! The `geometry` module.\n");
        assert!(d.member);
        assert_eq!(d.conforms, vec!["mod.vis.pub"]);
        assert_eq!(
            d.prose,
            vec![String::new(), "The `geometry` module.".to_owned()]
        );
    }

    #[test]
    fn member_files_are_never_conform_run_directly() {
        assert!(err("//! member: true\n//! check: pass\n").contains("never conform-run directly"));
        assert!(err("//! member: true\n//! phase: parse\n").contains("never conform-run directly"));
    }

    #[test]
    fn member_false_is_an_entry() {
        let d = ok("//! member: false\n//! check: pass\n//! phase: parse\n");
        assert!(!d.member);
        assert!(d.is_entry());
    }

    #[test]
    fn member_takes_a_boolean() {
        assert!(err("//! member: yes\n").contains("`true` or `false`"));
    }

    #[test]
    fn entries_require_check_and_phase() {
        assert!(err("//! phase: parse\n").contains("no `check:`"));
        assert!(err("//! check: pass\n").contains("no `phase:`"));
        assert!(err("// not a directive block\n").contains("no `check:`"));
    }

    // ---- duplicates -------------------------------------------------------

    #[test]
    fn duplicate_keys_are_errors() {
        for source in [
            "//! check: pass\n//! check: pass\n//! phase: parse\n",
            "//! check: pass\n//! phase: parse\n//! phase: lex\n",
            "//! check: pass\n//! phase: parse\n//! conforms: mem.a\n//! conforms: mem.b\n",
            "//! member: true\n//! member: true\n",
        ] {
            let message = parse_header(source)
                .expect_err("duplicate must fail")
                .to_string();
            assert!(message.contains("duplicate"), "{message}");
            assert!(message.contains("first seen on line"), "{message}");
        }
    }

    // ---- block & prose ----------------------------------------------------

    #[test]
    fn prose_with_colons_is_not_a_directive() {
        let d = ok(concat!(
            "//! check: pass\n",
            "//! phase: parse\n",
            "//! NOTE: exact API names ride the s05 spec\n",
            "//! §7/P1: use-after-free through a raw pointer\n",
            "//! Two ready arms: the choice is seeded\n",
        ));
        assert_eq!(d.check, Some(Check::Pass));
        assert_eq!(d.prose.len(), 3);
    }

    #[test]
    fn the_block_ends_at_the_first_non_directive_line() {
        let d = ok("//! check: pass\n//! phase: parse\nfn main() {}\n//! phase: run\n");
        assert_eq!(d.phase, Some(Phase::Parse));
    }

    #[test]
    fn a_trailing_carriage_return_is_not_a_parse_hazard() {
        // Guards the CRLF checkout the compiler track was bitten by; `.lines()`
        // strips `\n`, so the parser must tolerate the stray `\r` itself.
        let d = ok("//! check: pass\r\n//! phase: parse\r\n");
        assert_eq!(d.check, Some(Check::Pass));
        assert_eq!(d.phase, Some(Phase::Parse));
    }

    #[test]
    fn errors_carry_line_numbers() {
        let e = parse_header("//! check: pass\n//! phase: nope\n").expect_err("must fail");
        assert_eq!(e.line, Some(2));
        let e = parse_header("//! check: pass\n").expect_err("must fail");
        assert_eq!(e.line, None);
    }

    #[test]
    fn checks_render_back_to_directive_syntax() {
        assert_eq!(Check::Pass.to_string(), "pass");
        assert_eq!(Check::Fail("E1002".to_owned()).to_string(), "fail(E1002)");
        assert_eq!(
            Check::Run {
                exit: ExitSpec::Trap(Some(TrapKind::DivZero)),
                stdout: None
            }
            .to_string(),
            "run(exit=trap(div-zero))"
        );
        assert_eq!(
            Check::Run {
                exit: ExitSpec::Code(0),
                stdout: Some("hi".to_owned())
            }
            .to_string(),
            "run(exit=0, stdout=\"hi\")"
        );
    }
}
