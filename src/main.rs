//! The `lupin` command line (the wolf-interp package's binary, is12).
//!
//! The front door: `lupin FILE.lu` runs the file, `lupin -` reads a program
//! from stdin, bare `lupin` opens the REPL. A subcommand name wins over a
//! file of the same name (`lupin run repl` runs a file literally named
//! `repl`).
//!
//! One subcommand is normative: `conform-run` implements
//! `spec/06-differential-protocol.md` `[proto.invoke]`. `lex` and `parse` are
//! the frontend's human doors — their output is ours and is not compared with
//! anything. `corpus` and `protocol validate` are the harness the rest of the
//! track is built on.
//!
//! Exit codes, harness surfaces: `0` success, `1` the work ran and something
//! failed the check, `2` the tool itself could not run (missing file, bad
//! flags) — matching the compiler's convention so a differ can tell "the
//! program is wrong" from "the tool is wrong". The front door (`run`, `eval`,
//! `check`) instead reports the *program*: its own `exit(N)`, `2` on a
//! static-phase rejection, `3` on a trap or UB finding, `4` on
//! `unsupported`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};

use wolf_interp::corpus::{self, Outcome};
use wolf_interp::differ::{self, Counterparty, DeepClass, DeepDivergence, LedgerEntry};
use wolf_interp::eval::Trace;
use wolf_interp::fuzz::{self, Mode};
use wolf_interp::ledger::{self, Judgement};
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::schema;
use wolf_interp::{lex, parse};

const EXIT_OK: u8 = 0;
const EXIT_CHECK_FAILED: u8 = 1;
const EXIT_TOOL_ERROR: u8 = 2;
/// Front door (`run`/`eval`/`check`): a static-phase rejection. Shares the
/// number with tool errors deliberately — "this program did not get to run".
const EXIT_STATIC: u8 = 2;
/// Front door: the program ran and hit a trap, or the UB oracle's finding.
const EXIT_FAULT: u8 = 3;
/// Front door: the program is outside this implementation's scope
/// (`[proto.record.unsupported]` — the reason prints to stderr).
const EXIT_UNSUPPORTED: u8 = 4;
/// A rejected program in human mode. `[gram]`-tier failures are the compiler
/// convention's 65 (EX_DATAERR); `conform-run` never uses it, because there a
/// rejection is a *record* and the tool still exits 0 (`[proto.invoke.exit]`).
const EXIT_REJECTED: u8 = 65;

// Prefer the live submodule; fall back to the tracked vendored snapshot
// (vendor/README.md — CI cannot clone the private submodule).
use wolf_interp::upstream_root;

fn default_corpus_root() -> PathBuf {
    Path::new(upstream_root()).join("corpus")
}

fn default_spec_root() -> PathBuf {
    Path::new(upstream_root()).join("spec")
}

/// `--version`'s tail: `lupin 0.1.18 (wolf-interp, reference interpreter at
/// pin addcd7f)` — the crate version, the package this binary is built
/// from, and the pairing posture r01 row 7 asks the version line to name:
/// this binary is the wolf reference interpreter AT the stated upstream
/// spec/corpus pin, the sha every observation is made against.
///
/// The version carries `+dev.<commit>` unless the build was made exactly at
/// its own release tag (D57, decided in build.rs): a trunk build never
/// claims to be the release, so the bare version names exactly one
/// interpreter state and the wolf-side pairing can trust what it reads.
fn version_string() -> &'static str {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION.get_or_init(|| {
        format!(
            "{}{} (wolf-interp, reference interpreter at pin {})",
            env!("CARGO_PKG_VERSION"),
            env!("WOLF_INTERP_BUILD_SUFFIX"),
            wolf_interp::upstream_pin_short()
        )
    })
}

#[derive(Debug, Parser)]
#[command(
    name = "lupin",
    version = version_string(),
    about = "lupin — the wolf reference interpreter",
    long_about = "lupin — the wolf reference interpreter: an independent implementation of the \
wolf specification, and the compiler's differential-testing oracle. Built by the wolf-interp \
package.",
    after_help = "The front door: `lupin FILE.lu` runs the file (`lupin -` reads the program \
from stdin; bare `lupin` opens the REPL). Its exit code is the program's own exit(N); a \
static-phase rejection exits 2, a trap or UB finding exits 3, `unsupported` exits 4. A \
subcommand name wins over a file of the same name — run a file literally named `repl` as \
`lupin run repl`."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Run FILE directly (`lupin FILE.lu` = `lupin run FILE.lu`); `-` reads
    /// the program from stdin. With nothing at all, the REPL opens.
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
    /// Evaluate CODE in a fresh session and print its value as the REPL
    /// would (`lupin -e '1 + 1'` = `lupin eval '1 + 1'`).
    #[arg(short = 'e', value_name = "CODE", conflicts_with = "file")]
    eval: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run one program; output passes through live and the exit code is the program's
    Run(RunArgs),
    /// Evaluate a snippet in a fresh session and print its value as the REPL would
    Eval(EvalArgs),
    /// Check files through the frontend only (lex, parse, resolve) and report diagnostics
    Check(CheckArgs),
    /// Walk the pinned corpus and check every directive against this implementation.
    Corpus(CorpusArgs),
    /// Export the versioned conformance bundle, or check an implementation against one.
    Conformance {
        #[command(subcommand)]
        command: ConformanceCommand,
    },
    /// Observe one program and emit a spec/06 observation record.
    ConformRun(ConformRunArgs),
    /// Tokenize one program (`spec/01` §1).
    Lex(FrontendArgs),
    /// Parse one program (`spec/01` §2-§6).
    Parse(FrontendArgs),
    /// Validate observation records against the spec/06 schema.
    Protocol {
        #[command(subcommand)]
        command: ProtocolCommand,
    },
    /// Compare this implementation against the pinned compiler, corpus-wide.
    DiffRun(DiffRunArgs),
    /// Differential testing over generated programs, with reduction of anything divergent.
    Fuzz(FuzzArgs),
    /// Interactive session; `--script` replays a recorded transcript.
    Repl(ReplArgs),
}

#[derive(Debug, Args)]
struct RunArgs {
    /// The wolf program to run, or `-` to read it from stdin.
    file: PathBuf,
    /// Request the deterministic schedule seeded per spec 03 §5
    /// (`[conc.det.seed]`) — the same seed replays the same run.
    #[arg(long)]
    seed: Option<u64>,
    /// Replay one exact schedule: a seed value (as `--seed`) or an explicit
    /// decision stream `ev:c0,c1,…`.
    #[arg(long, value_name = "SEED|ev:…")]
    schedule: Option<String>,
    /// Emit the spec/06 observation record instead of passing output
    /// through — what `conform-run --json` does, at the front door (the
    /// record carries the outcome and the tool exits 0).
    #[arg(long)]
    json: bool,
    /// Resolve `use std.X[.Y]` against `<DIR>/X[/Y]/` — the std tree's root.
    /// Falls back to the `LUPIN_STD` environment variable (the compiler's
    /// `--std-root`/`WOLF_STD` mechanism, interpreter half).
    #[arg(long, value_name = "DIR")]
    std_root: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct EvalArgs {
    /// The snippet to evaluate (`lupin -e CODE` is the short spelling).
    code: String,
}

#[derive(Debug, Args)]
struct CheckArgs {
    /// The files to check. Diagnostics print to stderr; exit 0 clean, 2 on
    /// any rejection.
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Resolve `use std.X[.Y]` against `<DIR>/X[/Y]/` — the std tree's root.
    /// Falls back to the `LUPIN_STD` environment variable.
    #[arg(long, value_name = "DIR")]
    std_root: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ReplArgs {
    /// Replay a transcript (input lines + expected output blocks) and diff;
    /// any drift exits 1. The format is documented in docs/repl.md.
    #[arg(long)]
    script: Option<PathBuf>,
    /// The schedule seed for the session's scheduler (`[conc.det.seed]`).
    #[arg(long)]
    seed: Option<u64>,
    /// Disable the line editor: plain line reads even at a TTY (the piped
    /// path's reader, `[repl.edit.tty]`).
    #[arg(long)]
    no_edit: bool,
}

#[derive(Debug, Args)]
struct CorpusArgs {
    /// Corpus root to walk.
    #[arg(long, default_value_os_t = default_corpus_root())]
    root: PathBuf,
    /// Spec root, for checking `conforms:` tags against `anchors.json`.
    #[arg(long, default_value_os_t = default_spec_root())]
    spec: PathBuf,
    /// Emit JSON instead of the human report.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ConformRunArgs {
    /// The wolf program to observe.
    file: PathBuf,
    /// Stop after this rung of the canonical ladder.
    #[arg(long)]
    phase: Option<Phase>,
    /// Request the deterministic schedule seeded per spec 03 §5
    /// (`sched-ev/0`): the record declares `"seeded": true` and the same
    /// seed replays the identical decision stream (`[proto.seed.flag]`).
    #[arg(long)]
    seed: Option<u64>,
    /// Replay one exact schedule: either a seed value (as `--seed`) or an
    /// explicit decision stream `ev:c0,c1,…` — the explorer's counterexample
    /// spelling when a stream does not fit the 62-bit packed-seed payload.
    #[arg(long, value_name = "SEED|ev:…")]
    schedule: Option<String>,
    /// is07: systematically explore up to N inequivalent schedules (DPOR over
    /// the sched-ev/0 decision stream) instead of observing one run. Reports
    /// schedules explored, reduction stats, and verdict stability; any
    /// schedule-dependent behavior is a finding and exits 1.
    #[arg(long, value_name = "N")]
    explore: Option<u64>,
    /// Exploration: maximum decisions per schedule (depth cap).
    #[arg(long, default_value_t = 4096)]
    explore_steps: usize,
    /// Exploration: CHESS-style preemption bound (max non-FIFO choices).
    #[arg(long, value_name = "P")]
    explore_preemptions: Option<u32>,
    /// Exploration: wall-clock budget in milliseconds.
    #[arg(long, value_name = "MS")]
    explore_wall_ms: Option<u64>,
    /// Exploration: full-branching DFS baseline instead of DPOR.
    #[arg(long)]
    explore_naive: bool,
    /// Exploration: disable state-hash convergence pruning.
    #[arg(long)]
    explore_no_prune: bool,
    /// Exploration: re-verify every state-hash hit against its stored
    /// canonical preimage (the 128-bit-collision guard).
    #[arg(long)]
    paranoid: bool,
    /// Emit the machine-readable observation record.
    #[arg(long)]
    json: bool,
    /// Resolve `use std.X[.Y]` against `<DIR>/X[/Y]/` — the std tree's root.
    /// Falls back to the `LUPIN_STD` environment variable (the compiler's
    /// `--std-root`/`WOLF_STD` mechanism, interpreter half; issue #6).
    #[arg(long, value_name = "DIR")]
    std_root: Option<PathBuf>,
    /// Log evaluation rules as they fire, with their clause anchors, to
    /// stderr. `--trace` logs all of them; `--trace=mem` keeps only the
    /// memory-model rules — every region event (create/open/suspend/freeze/
    /// free, edge checks, RC ops, handle faults); `--trace=prov` keeps the
    /// Tier-3 ones — every retag, permission transition, exposure, protector
    /// and UB row, which is what s23's shipped miri-lite diffs against.
    /// Human-facing; never part of the record.
    #[arg(long, num_args = 0..=1, default_missing_value = "all", value_name = "FILTER")]
    trace: Option<Trace>,
}

#[derive(Debug, Args)]
struct FrontendArgs {
    /// The wolf program to read.
    file: PathBuf,
    /// Print the token stream (`lex`) or the production trace (`parse`).
    #[arg(long)]
    dump: bool,
}

#[derive(Debug, Args)]
struct DiffRunArgs {
    /// Corpus root to walk (entries only; `member: true` files compile
    /// through their entry and are never conform-run directly).
    #[arg(long, default_value_os_t = default_corpus_root())]
    corpus: PathBuf,
    /// The counterparty `wolf` binary. Defaults to the conventional build
    /// products of `cargo build -p wolf_driver` inside `upstream/`.
    #[arg(long)]
    compiler: Option<PathBuf>,
    /// Hard-fail instead of SKIPping when no counterparty binary exists.
    #[arg(long)]
    require_counterparty: bool,
    /// Which of the counterparty's engines answers. `default` drives no flag
    /// and stops at its static pipeline (`unsupported` at `wir`), so the run
    /// tier compares nowhere; `checked`, `native` and `release` each reach
    /// `run`. `release` runs s42's mid-end and s43's whole-program layer, so
    /// it is the lane that tests whether optimization preserves observable
    /// behavior. `native`/`release` need `libwolf_rt.a` beside the binary.
    #[arg(long, value_enum, default_value_t = CounterpartyTierArg::Default)]
    counterparty_tier: CounterpartyTierArg,
    /// Wall-clock budget per invocation, either side. A timeout is a
    /// verdict, not an error.
    #[arg(long, default_value_t = 30_000)]
    timeout_ms: u64,
    /// Write the JSONL divergence report here (one line per divergence,
    /// ordered by `[proto.cmp.severity]`; empty file = green).
    #[arg(long)]
    report: Option<PathBuf>,
    /// Write the conservatism ledger here (JSONL, one entry per line).
    #[arg(long)]
    ledger: Option<PathBuf>,
    /// Print the JSONL report to stdout instead of the human summary.
    #[arg(long)]
    json: bool,
    /// Print a `[proto.cmp.triage]` filing template for every unfiled
    /// divergence.
    #[arg(long)]
    filing: bool,
}

/// The CLI spelling of [`differ::CounterpartyTier`]. Separate because clap's
/// derive belongs to the binary and the tier belongs to the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CounterpartyTierArg {
    Default,
    Checked,
    Native,
    Release,
}

impl From<CounterpartyTierArg> for differ::CounterpartyTier {
    fn from(arg: CounterpartyTierArg) -> Self {
        match arg {
            CounterpartyTierArg::Default => Self::Default,
            CounterpartyTierArg::Checked => Self::Checked,
            CounterpartyTierArg::Native => Self::Native,
            CounterpartyTierArg::Release => Self::Release,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FuzzMode {
    Defined,
    Boundary,
    Mixed,
}

#[derive(Debug, Args)]
struct FuzzArgs {
    /// How many programs to generate.
    #[arg(long, default_value_t = 1000)]
    count: u64,
    /// Campaign seed. Same seed, same programs, every platform — replay a
    /// night's findings from its recorded seed.
    #[arg(long, default_value_t = 0x1505)]
    seed: u64,
    /// Generation mode; `mixed` alternates.
    #[arg(long, value_enum, default_value_t = FuzzMode::Mixed)]
    mode: FuzzMode,
    /// The counterparty `wolf` binary; without one the campaign runs the
    /// self-oracle checks only (and says so).
    #[arg(long)]
    compiler: Option<PathBuf>,
    /// Hard-fail instead of degrading to self-oracle mode when no
    /// counterparty binary exists.
    #[arg(long)]
    require_counterparty: bool,
    /// Wall-clock budget per subprocess invocation.
    #[arg(long, default_value_t = 10_000)]
    timeout_ms: u64,
    /// Where generated cases and minimized reproducers land.
    #[arg(long, default_value = "target/fuzz")]
    out: PathBuf,
}

#[derive(Debug, Subcommand)]
enum ConformanceCommand {
    /// Emit the self-contained, byte-deterministic conformance bundle:
    /// corpus + suites + reference records + vocabularies + anchor registry
    /// + coverage matrix + MANIFEST (schema 1, docs/conformance-bundle.md).
    Export(ConformanceExportArgs),
    /// Run ANY conform-run-speaking implementation against a bundle and
    /// report per `[proto.cmp]` — is05's differ with the counterparty
    /// pluggable. Integrity-checks the bundle first.
    Check(ConformanceCheckArgs),
}

#[derive(Debug, Args)]
struct ConformanceExportArgs {
    /// The bundle directory to (re)create. Deterministic: re-export at the
    /// same commits is byte-identical, on every platform.
    #[arg(long, default_value = "target/conformance-bundle")]
    out: PathBuf,
    /// Emit the machine-readable summary instead of the human one.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ConformanceCheckArgs {
    /// The bundle directory (as produced by `conformance export`).
    bundle: PathBuf,
    /// The implementation under test: a command invoked as
    /// `<CMD> conform-run <file> --json` (`[proto.invoke]`).
    #[arg(long = "impl", value_name = "CMD", conflicts_with = "replay")]
    impl_cmd: Option<PathBuf>,
    /// Replay recorded observations (a records JSONL) as the counterparty —
    /// the consumption dry-run: exercises the full pull → run → diff path
    /// with no second implementation in the room.
    #[arg(long, value_name = "RECORDS")]
    replay: Option<PathBuf>,
    /// Wall-clock budget per invocation. A timeout is a verdict, not an
    /// error.
    #[arg(long, default_value_t = 30_000)]
    timeout_ms: u64,
    /// Write the JSONL divergence report here (ordered by
    /// `[proto.cmp.severity]`; empty file = green).
    #[arg(long)]
    report: Option<PathBuf>,
    /// Print the JSONL report to stdout instead of the human summary.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum ProtocolCommand {
    /// Validate observation records against spec/06 `[proto.record]`.
    Validate {
        /// Record files to validate. Each holds one JSON object.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // The front-door dispatch (is12): a subcommand name always wins; a bare
    // file runs; `-e CODE` evaluates; nothing at all opens the REPL.
    let code = match (cli.command, cli.eval, cli.file) {
        (Some(command), _, _) => run_command(command),
        (None, Some(code), _) => run_eval(&EvalArgs { code }),
        (None, None, Some(file)) => run_run(&RunArgs {
            file,
            seed: None,
            schedule: None,
            json: false,
            // The bare front door has no flags; `LUPIN_STD` still applies.
            std_root: None,
        }),
        (None, None, None) => run_repl(&ReplArgs {
            script: None,
            seed: None,
            no_edit: false,
        }),
    };
    ExitCode::from(code)
}

fn run_command(command: Command) -> u8 {
    match command {
        Command::Run(args) => run_run(&args),
        Command::Eval(args) => run_eval(&args),
        Command::Check(args) => run_check(&args),
        Command::Corpus(args) => run_corpus(&args),
        Command::Conformance {
            command: ConformanceCommand::Export(args),
        } => run_conformance_export(&args),
        Command::Conformance {
            command: ConformanceCommand::Check(args),
        } => run_conformance_check(&args),
        Command::ConformRun(args) => run_conform_run(&args),
        Command::Lex(args) => run_lex(&args),
        Command::Parse(args) => run_parse(&args),
        Command::Protocol {
            command: ProtocolCommand::Validate { files },
        } => run_protocol_validate(&files),
        Command::DiffRun(args) => run_diff_run(&args),
        Command::Fuzz(args) => run_fuzz(&args),
        Command::Repl(args) => run_repl(&args),
    }
}

// ---------------------------------------------------------------------------
// the is12 front door — run, eval, check
// ---------------------------------------------------------------------------

/// `run`'s schedule request: `--seed` wins, then `--schedule`, then the
/// strict-FIFO default — the same precedence `conform-run` uses.
fn sched_request(
    seed: Option<u64>,
    schedule: Option<&str>,
) -> Result<wolf_interp::eval::SchedRequest, u8> {
    match (seed, schedule) {
        (Some(seed), _) => Ok(wolf_interp::eval::SchedRequest::Seed(seed)),
        (None, Some(text)) => parse_schedule(text).map_err(|message| tool_error(&message)),
        (None, None) => Ok(wolf_interp::eval::SchedRequest::Default),
    }
}

/// Renders a static diagnostic against the text its spans index: the entry
/// source, unless the observation's reason says a *sibling module file*
/// failed — then that file's text (`line:col` against the wrong file would
/// lie). The reason names the module file relative to the entry's directory
/// (a record travels between machines); an unreadable module file falls
/// back to the offset spelling, which misleads no one about lines it
/// cannot know.
fn render_static_diag(
    diag: &wolf_interp::diag::Diag,
    reason: Option<&str>,
    entry_file: Option<&Path>,
    entry_text: &str,
) -> String {
    if let Some(module) = reason
        .and_then(|r| r.strip_prefix("in module file `"))
        .and_then(|r| r.strip_suffix('`'))
    {
        let resolved = match entry_file.and_then(Path::parent) {
            Some(dir) if Path::new(module).is_relative() => dir.join(module),
            _ => PathBuf::from(module),
        };
        // Name the file the line:col points into — the whole point of the
        // spelling is that a reader can jump to it.
        return match std::fs::read_to_string(&resolved) {
            Ok(text) => format!("{} (in module file `{module}`)", diag.render(&text)),
            Err(_) => format!("{diag} (in module file `{module}`)"),
        };
    }
    diag.render(entry_text)
}

/// `lupin FILE.lu` / `lupin run FILE.lu` / `lupin -`: the program's stdout
/// passes through live and the process exit code reports the program —
/// `exit(N)` as N, a static rejection as 2, a trap or UB finding as 3,
/// `unsupported` as 4.
fn run_run(args: &RunArgs) -> u8 {
    let stdin = args.file.as_os_str() == "-";
    let source = if stdin {
        use std::io::Read;
        let mut source = Vec::new();
        match std::io::stdin().read_to_end(&mut source) {
            Ok(_) => source,
            Err(e) => return tool_error(&format!("could not read stdin: {e}")),
        }
    } else {
        match read_program(&args.file) {
            Ok(source) => source,
            Err(code) => return code,
        }
    };
    let request = match sched_request(args.seed, args.schedule.as_deref()) {
        Ok(request) => request,
        Err(code) => return code,
    };

    if args.json {
        // The record surface, verbatim: the record carries the outcome and
        // the tool exits 0 (`[proto.invoke.exit]`).
        let (record, _) = if stdin {
            wolf_interp::observe_record_stdin(&source, &request)
        } else {
            wolf_interp::observe_record_scheduled(
                &args.file,
                &source,
                None,
                wolf_interp::eval::Trace::Off,
                &request,
                args.std_root.as_deref(),
            )
        };
        let value = match serde_json::to_value(&record) {
            Ok(value) => value,
            Err(e) => return tool_error(&format!("could not build the observation record: {e}")),
        };
        if let Err(errors) = schema::validate(&value) {
            return tool_error(&format!("refusing to emit a malformed record: {errors}"));
        }
        return match record.to_json_line() {
            Ok(line) => {
                println!("{line}");
                EXIT_OK
            }
            Err(e) => tool_error(&format!("could not serialize the record: {e}")),
        };
    }

    let display = if stdin {
        "<stdin>".to_owned()
    } else {
        wolf_interp::slash_path(&args.file)
    };
    let file = (!stdin).then_some(args.file.as_path());
    let observation =
        wolf_interp::frontend::observe_live(file, &source, &request, args.std_root.as_deref());
    // The human fault lines carry `line:col` (`[conf.trap.render]`); the
    // lossy decode only matters for a non-UTF-8 source, which never gets
    // past the lexer's E0107 at offset zero.
    let text = String::from_utf8_lossy(&source);
    match &observation.verdict {
        // The program's own exit status is the process's.
        Verdict::Exit(status) => *status,
        Verdict::Trap(_) => {
            if let Some(trap) = &observation.trap {
                eprintln!("{display}: {}", trap.render(&text));
            }
            EXIT_FAULT
        }
        Verdict::Ub(_) => {
            if let Some(finding) = &observation.ub {
                eprintln!("{display}: {}", finding.render(&text));
            }
            EXIT_FAULT
        }
        Verdict::Fail(_) => {
            if let Some(diag) = &observation.detail {
                eprintln!(
                    "{display}: {}",
                    render_static_diag(diag, observation.reason.as_deref(), file, &text)
                );
            }
            EXIT_STATIC
        }
        Verdict::Unsupported => {
            eprintln!(
                "{display}: unsupported: {}",
                observation
                    .reason
                    .as_deref()
                    .unwrap_or("outside this implementation's scope")
            );
            EXIT_UNSUPPORTED
        }
        // Unreachable without a `--phase` cap, which `run` does not take.
        Verdict::Pass => EXIT_OK,
    }
}

/// `lupin eval 'CODE'` (`-e`): a fresh REPL session evaluates the snippet
/// and prints exactly what the REPL would; the exit code reports the
/// snippet on the front door's scale.
fn run_eval(args: &EvalArgs) -> u8 {
    use wolf_interp::eval::repl::{Fed, Session, SessionFault};
    let mut session = Session::new(None);
    let mut pending = false;
    for line in args.code.lines() {
        match session.feed_line(line) {
            Fed::Quit => return EXIT_OK,
            Fed::More => pending = true,
            Fed::Output(out) => {
                pending = false;
                for rendered in out {
                    println!("{rendered}");
                }
            }
        }
    }
    if pending {
        return tool_error("the snippet is incomplete — a delimiter or continuation is open");
    }
    match session.fault() {
        None => EXIT_OK,
        Some(SessionFault::Static) => EXIT_STATIC,
        Some(SessionFault::Dynamic) => EXIT_FAULT,
        Some(SessionFault::Unsupported) => EXIT_UNSUPPORTED,
    }
}

/// `lupin check FILE...`: the frontend only — lex, parse, resolve — with
/// diagnostics on stderr. Exit 0 when every file is clean, 2 otherwise.
fn run_check(args: &CheckArgs) -> u8 {
    let mut rejected = 0usize;
    for file in &args.files {
        let source = match read_program(file) {
            Ok(source) => source,
            Err(code) => return code,
        };
        let observation = wolf_interp::frontend::observe_file(
            file,
            &source,
            Some(Phase::Resolve),
            wolf_interp::eval::Trace::Off,
            &wolf_interp::eval::SchedRequest::Default,
            args.std_root.as_deref(),
        );
        let display = wolf_interp::slash_path(file);
        match &observation.verdict {
            Verdict::Pass => println!("{display}: ok"),
            Verdict::Unsupported => {
                rejected += 1;
                eprintln!(
                    "{display}: unsupported: {}",
                    observation
                        .reason
                        .as_deref()
                        .unwrap_or("outside this implementation's scope")
                );
            }
            _ => {
                rejected += 1;
                if let Some(diag) = &observation.detail {
                    eprintln!(
                        "{display}: {}",
                        render_static_diag(
                            diag,
                            observation.reason.as_deref(),
                            Some(file),
                            &String::from_utf8_lossy(&source)
                        )
                    );
                }
            }
        }
    }
    if rejected == 0 { EXIT_OK } else { EXIT_STATIC }
}

fn run_repl(args: &ReplArgs) -> u8 {
    match &args.script {
        Some(script) => replay_transcript(script, args.seed),
        None => repl_loop(args.seed, args.no_edit),
    }
}

/// The live loop. The reader is chosen here and ONLY here (is25,
/// `[repl.edit.tty]`): a piped session keeps the exact dumb reader — its
/// captured stdout IS a valid transcript, byte-compared by three CI gates —
/// while a TTY gets the line editor. `--no-edit` and `TERM=dumb` also keep
/// the dumb reader, and an editor that cannot start degrades to it with a
/// one-line note rather than failing to open a prompt.
fn repl_loop(seed: Option<u64>, no_edit: bool) -> u8 {
    use std::io::IsTerminal;
    let interactive = std::io::stdin().is_terminal();
    let mut session = wolf_interp::eval::repl::Session::new(seed);
    let term_dumb = std::env::var("TERM").is_ok_and(|term| term == "dumb");
    if interactive && !no_edit && !term_dumb {
        match edit_loop(&mut session) {
            Ok(code) => return code,
            Err(note) => {
                eprintln!("lupin: line editing unavailable ({note}); plain line reads");
            }
        }
    }
    dumb_loop(&mut session, interactive)
}

/// The dumb reader — the is08 loop, verbatim. Pipe-friendly on purpose: the
/// prompt and (in pipe mode) the echoed input are printed, so a piped
/// session's captured stdout IS a valid transcript — capture-then-grep in
/// CI, no PTY anywhere.
fn dumb_loop(session: &mut wolf_interp::eval::repl::Session, interactive: bool) -> u8 {
    use std::io::{BufRead, Write};
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        print!("{}", session.prompt());
        let _ = std::io::stdout().flush();
        let Some(Ok(line)) = lines.next() else {
            // EOF: end the line the prompt started, then leave.
            println!();
            return EXIT_OK;
        };
        let line = line.strip_suffix('\r').unwrap_or(&line).to_owned();
        if !interactive {
            // Echo, so captured output is a transcript.
            println!("{line}");
        }
        match session.feed_line(&line) {
            wolf_interp::eval::repl::Fed::Quit => return EXIT_OK,
            wolf_interp::eval::repl::Fed::More => {}
            wolf_interp::eval::repl::Fed::Output(out) => {
                for rendered in out {
                    println!("{rendered}");
                }
            }
        }
    }
}

/// The line editor's loop (is25). The editor is a READER: it hands complete
/// inputs to the same [`wolf_interp::eval::repl::Session::feed_line`] the
/// dumb loop uses, line by line, and owns no meaning of its own.
///
/// `Err` means the editor could not start (raw mode unavailable) — the
/// caller degrades to the dumb reader. Once running, `Ctrl-C` abandons the
/// input and the session survives (`[repl.edit.cancel]`, closing the #46
/// continuation trap); `Ctrl-D` on an empty line — and only there — exits.
fn edit_loop(session: &mut wolf_interp::eval::repl::Session) -> Result<u8, String> {
    use rustyline::config::{CompletionType, Config};
    use rustyline::error::ReadlineError;
    use rustyline::history::MemHistory;
    use rustyline::{Editor, EventHandler, KeyEvent, Modifiers};
    use std::sync::{Arc, Mutex};
    use wolf_interp::edit::{
        self, EditHelper, HISTORY_CAP, YankLastArg, load_history, save_history,
    };

    let config = Config::builder()
        .max_history_size(HISTORY_CAP)
        .map_err(|e| e.to_string())?
        .history_ignore_dups(true)
        .map_err(|e| e.to_string())?
        .history_ignore_space(false)
        .completion_type(CompletionType::List)
        .auto_add_history(false)
        .build();
    let mut editor: Editor<EditHelper, MemHistory> =
        Editor::with_history(config, MemHistory::new()).map_err(|e| e.to_string())?;
    editor.set_helper(Some(EditHelper::new()));

    // Persistent history ([repl.edit.history]): one store feeds the editor's
    // recall, the disk file, and the Alt-. handler.
    let path = edit::history_path();
    let store = load_history(path.as_deref());
    for entry in store.entries() {
        let _ = editor.add_history_entry(entry);
    }
    let store = Arc::new(Mutex::new(store));
    for key in [
        KeyEvent::new('.', Modifiers::ALT),
        KeyEvent::new('_', Modifiers::ALT),
    ] {
        editor.bind_sequence(
            key,
            EventHandler::Conditional(Box::new(YankLastArg::new(Arc::clone(&store)))),
        );
    }

    loop {
        if let Some(helper) = editor.helper_mut() {
            helper.set_names(session.completion_names());
        }
        match editor.readline(session.prompt()) {
            Ok(buffer) => {
                if !buffer.trim().is_empty() {
                    let _ = editor.add_history_entry(&buffer);
                    if let Ok(mut store) = store.lock()
                        && store.push(&buffer)
                    {
                        save_history(path.as_deref(), &store);
                    }
                }
                // Feed line by line — the exact protocol the piped loop
                // speaks, so the editor cannot introduce meaning.
                for line in buffer.split('\n') {
                    match session.feed_line(line) {
                        wolf_interp::eval::repl::Fed::Quit => return Ok(EXIT_OK),
                        wolf_interp::eval::repl::Fed::More => {}
                        wolf_interp::eval::repl::Fed::Output(out) => {
                            for rendered in out {
                                println!("{rendered}");
                            }
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C: the input is abandoned, the session and its world
                // survive ([repl.edit.cancel]). Never an exit.
                session.cancel_pending();
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D on an empty line: the one exit besides `:quit`.
                println!();
                return Ok(EXIT_OK);
            }
            Err(e) => {
                // A read failure mid-session (terminal gone, io error):
                // leave cleanly rather than spin.
                eprintln!("lupin: read error: {e}");
                return Ok(EXIT_TOOL_ERROR);
            }
        }
    }
}

/// `--script t.transcript`: replay the transcript's input lines through a
/// fresh session, re-render the whole transcript, and byte-compare. Drift
/// fails with the first differing line — the book's no-rot gate.
fn replay_transcript(path: &Path, seed: Option<u64>) -> u8 {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) => return tool_error(&format!("cannot read {}: {e}", path.display())),
    };
    let expected = raw.replace("\r\n", "\n");
    let mut session = wolf_interp::eval::repl::Session::new(seed);
    let mut produced = String::new();
    let mut inputs = 0usize;
    let mut quit = false;
    for line in expected.lines() {
        let input = line
            .strip_prefix("wolf> ")
            .or_else(|| line.strip_prefix("....> "));
        let Some(input) = input else { continue };
        if quit {
            break;
        }
        inputs += 1;
        produced.push_str(session.prompt());
        produced.push_str(input);
        produced.push('\n');
        match session.feed_line(input) {
            wolf_interp::eval::repl::Fed::Quit => quit = true,
            wolf_interp::eval::repl::Fed::More => {}
            wolf_interp::eval::repl::Fed::Output(out) => {
                for rendered in out {
                    produced.push_str(&rendered);
                    produced.push('\n');
                }
            }
        }
    }
    if produced == expected {
        println!("replay: OK ({inputs} input(s), byte-identical)");
        EXIT_OK
    } else {
        let mut want = expected.lines();
        let mut got = produced.lines();
        let mut at = 0usize;
        loop {
            at += 1;
            match (want.next(), got.next()) {
                (Some(w), Some(g)) if w == g => {}
                (w, g) => {
                    eprintln!(
                        "replay: DRIFT at line {at} of {}\n  expected: {}\n  got:      {}",
                        path.display(),
                        w.unwrap_or("<end of transcript>"),
                        g.unwrap_or("<end of session output>"),
                    );
                    break;
                }
            }
        }
        EXIT_CHECK_FAILED
    }
}

fn tool_error(message: &str) -> u8 {
    eprintln!("lupin: {message}");
    EXIT_TOOL_ERROR
}

fn run_corpus(args: &CorpusArgs) -> u8 {
    let report = match corpus::walk(&args.root, Some(&args.spec)) {
        Ok(report) => report,
        Err(e) => {
            return tool_error(&format!(
                "{e}\n  hint: run `git submodule update --init upstream` (or use the tracked vendor/upstream snapshot)"
            ));
        }
    };

    if args.json {
        match serde_json::to_string_pretty(&corpus_json(&report)) {
            Ok(text) => println!("{text}"),
            Err(e) => return tool_error(&format!("could not serialize the report: {e}")),
        }
    } else {
        print_corpus_report(&report);
    }

    if report.is_green() {
        EXIT_OK
    } else {
        EXIT_CHECK_FAILED
    }
}

/// What this implementation makes of one corpus entry, and how that sits
/// against the file's own `check:` directive.
///
/// Members are never conform-run directly (`[conf.directive.member]`), so they
/// are not asked.
fn observe_entry(
    root: &Path,
    relative: &str,
    directives: &wolf_interp::directive::Directives,
) -> (String, Option<Judgement>) {
    let path = root.join(relative);
    let Ok(source) = std::fs::read(&path) else {
        return ("unreadable".to_owned(), None);
    };
    let (record, observed) = wolf_interp::observe_record(&path, &source, None);
    let status = format!("{}@{}", record.verdict, record.phase_reached);
    // The directive matcher judges "the program's stdout"
    // (`[conf.directive.check]`), which the observation carries whatever the
    // verdict — a trap pin with `stdout="…"` compares the bytes printed
    // before the fault, which the wire record deliberately omits.
    let judgement = directives
        .check
        .as_ref()
        .map(|check| ledger::judge(check, &record, &observed.stdout));
    (status, judgement)
}

fn print_corpus_report(report: &corpus::CorpusReport) {
    let width = report
        .files
        .iter()
        .map(|f| f.path.chars().count())
        .max()
        .unwrap_or(0);

    for file in &report.files {
        match &file.outcome {
            Outcome::Entry(directives) => {
                let (status, judgement) = observe_entry(&report.root, &file.path, directives);
                let verdict = judgement
                    .as_ref()
                    .map_or_else(|| "-".to_owned(), |j| j.label().to_owned());
                let phase = directives
                    .phase
                    .map_or_else(|| "-".to_owned(), |p| p.to_string());
                let check = directives
                    .check
                    .as_ref()
                    .map_or_else(|| "-".to_owned(), ToString::to_string);
                let tags = if directives.conforms.is_empty() {
                    "-".to_owned()
                } else {
                    directives.conforms.join(", ")
                };
                println!(
                    "entry   {:<width$}  phase={phase:<9}  check={check:<34}  \
                     lupin={status:<28}  {verdict:<12}  conforms={tags}",
                    file.path
                );
                if let Some(judgement) = &judgement
                    && judgement.is_mismatch()
                {
                    println!("        {judgement}");
                }
            }
            Outcome::Member(directives) => {
                let tags = if directives.conforms.is_empty() {
                    "-".to_owned()
                } else {
                    directives.conforms.join(", ")
                };
                println!(
                    "member  {:<width$}  exercised through its module's entry file; conforms={tags}",
                    file.path
                );
            }
            Outcome::Failed(reason) => {
                println!("FAILED  {:<width$}  {reason}", file.path);
            }
        }
    }

    let failures = report.failures();
    println!();
    println!(
        "{} file(s) under {}: {} entr{}, {} member(s), {} failure(s)",
        report.total(),
        wolf_interp::slash_path(&report.root),
        report.entries(),
        if report.entries() == 1 { "y" } else { "ies" },
        report.members(),
        failures.len()
    );

    let tags = report.tag_counts();
    if report.anchors_checked {
        if report.unknown_anchors.is_empty() {
            println!(
                "{} distinct conforms: anchor(s); every registered-namespace tag resolves against anchors.json",
                tags.len()
            );
        } else {
            println!(
                "{} distinct conforms: anchor(s); {} unknown in a registered namespace:",
                tags.len(),
                report.unknown_anchors.len()
            );
            for tag in &report.unknown_anchors {
                println!("  unknown anchor: {tag}");
            }
        }
    } else {
        println!(
            "{} distinct conforms: anchor(s); anchors.json unavailable, tags checked for shape only",
            tags.len()
        );
    }

    // The honest ledger: what this implementation actually produced, counted
    // rather than claimed (`[proto.record.unsupported]` — scope gaps stay
    // visible, never silent).
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut reached_run = 0usize;
    for file in &report.files {
        let Outcome::Entry(directives) = &file.outcome else {
            continue;
        };
        let (status, judgement) = observe_entry(&report.root, &file.path, directives);
        if status.ends_with("@run") {
            reached_run += 1;
        }
        if let Some(judgement) = judgement {
            *counts.entry(judgement.label()).or_default() += 1;
        }
    }
    println!();
    println!(
        "lupin: {reached_run} entr{} reach the `run` rung; \
         {} match their `check:` expectation, {} are the dynamic counterpart of the static \
         code the corpus pins, {} are static-conservatism entries (the compiler rejects \
         statically what this machine never checks), {} are out of scope, {} mismatch",
        if reached_run == 1 { "y" } else { "ies" },
        counts.get("match").copied().unwrap_or(0),
        counts.get("dynamic-counterpart").copied().unwrap_or(0),
        counts.get("conservatism").copied().unwrap_or(0),
        counts.get("out-of-scope").copied().unwrap_or(0),
        counts.get("MISMATCH").copied().unwrap_or(0),
    );
}

fn corpus_json(report: &corpus::CorpusReport) -> serde_json::Value {
    let files: Vec<serde_json::Value> = report
        .files
        .iter()
        .map(|file| match &file.outcome {
            Outcome::Entry(directives) => {
                let (status, judgement) = observe_entry(&report.root, &file.path, directives);
                serde_json::json!({
                    "file": file.path,
                    "kind": "entry",
                    "phase": directives.phase.map(|p| p.to_string()),
                    "check": directives.check.as_ref().map(ToString::to_string),
                    "conforms": directives.conforms,
                    "interpreter_status": status,
                    "judgement": judgement.map(|j| serde_json::json!({
                        "class": j.label(),
                        "detail": j.detail(),
                    })),
                })
            }
            Outcome::Member(directives) => serde_json::json!({
                "file": file.path,
                "kind": "member",
                "conforms": directives.conforms,
            }),
            Outcome::Failed(reason) => serde_json::json!({
                "file": file.path,
                "kind": "failed",
                "reason": reason,
            }),
        })
        .collect();

    serde_json::json!({
        "root": report.root.display().to_string(),
        "total": report.total(),
        "entries": report.entries(),
        "members": report.members(),
        "failures": report.failures().len(),
        "anchors_checked": report.anchors_checked,
        "unknown_anchors": report.unknown_anchors,
        "green": report.is_green(),
        "files": files,
    })
}

// ---------------------------------------------------------------------------
// conformance — the is09 exportable suite
// ---------------------------------------------------------------------------

fn run_conformance_export(args: &ConformanceExportArgs) -> u8 {
    let options = wolf_interp::export::ExportOptions::conventional(&args.out);
    let summary = match wolf_interp::export::export(&options) {
        Ok(summary) => summary,
        Err(message) => return tool_error(&message),
    };
    if args.json {
        let value = serde_json::json!({
            "out": wolf_interp::slash_path(&args.out),
            "pin": summary.pin,
            "files": summary.files,
            "programs": summary.programs,
            "records": summary.records,
            "anchors_total": summary.anchors_total,
            "anchors_covered": summary.anchors_covered,
            "forward_tags": summary.forward_tags,
            "bundle_sha256": summary.bundle_sha256,
        });
        println!("{value}");
    } else {
        println!(
            "conformance bundle (schema {}) exported to {}",
            wolf_interp::export::BUNDLE_SCHEMA,
            args.out.display()
        );
        println!("  pin            {}", summary.pin);
        println!(
            "  files          {} ({} programs, {} reference records)",
            summary.files, summary.programs, summary.records
        );
        println!(
            "  coverage       {} / {} registered anchors cited; {} debt; {} forward tags",
            summary.anchors_covered,
            summary.anchors_total,
            summary.anchors_total - summary.anchors_covered,
            summary.forward_tags
        );
        println!("  bundle_sha256  {}", summary.bundle_sha256);
    }
    EXIT_OK
}

fn run_conformance_check(args: &ConformanceCheckArgs) -> u8 {
    let counterparty = match (&args.impl_cmd, &args.replay) {
        (Some(cmd), None) => wolf_interp::export::CheckImpl::Command(cmd.clone()),
        (None, Some(records)) => wolf_interp::export::CheckImpl::Replay(records.clone()),
        _ => return tool_error("pass exactly one of --impl <CMD> or --replay <RECORDS>"),
    };
    let bundle = match wolf_interp::export::open_bundle(&args.bundle) {
        Ok(bundle) => bundle,
        Err(message) => return tool_error(&message),
    };
    eprintln!(
        "notice: bundle {} at pin {} verified (bundle_sha256 {})",
        args.bundle.display(),
        bundle.pin,
        bundle.bundle_sha256
    );
    let outcome = match wolf_interp::export::check(
        &bundle,
        &counterparty,
        Duration::from_millis(args.timeout_ms),
    ) {
        Ok(outcome) => outcome,
        Err(message) => return tool_error(&message),
    };
    if let Some(path) = &args.report
        && let Err(code) = write_jsonl(
            path,
            outcome.divergences.iter().map(DeepDivergence::to_json),
        )
    {
        return code;
    }
    let gating = |d: &DeepDivergence| d.class == DeepClass::SoundnessCandidate || d.filed.is_none();
    if args.json {
        for d in &outcome.divergences {
            println!("{}", d.to_json());
        }
        return if outcome.divergences.iter().filter(|d| gating(d)).count() == 0 {
            EXIT_OK
        } else {
            EXIT_CHECK_FAILED
        };
    }
    let as_diff = DiffOutcome {
        divergences: outcome.divergences,
        ledger: outcome.ledger,
        compared: outcome.compared,
        skipped_members: 0,
    };
    report_diff(&as_diff, false)
}

fn read_program(path: &Path) -> Result<Vec<u8>, u8> {
    // `[proto.invoke.exit]`: tool-level failures exit nonzero with NO record.
    if !path.is_file() {
        return Err(tool_error(&format!(
            "`{}` is not a readable file",
            path.display()
        )));
    }
    std::fs::read(path)
        .map_err(|e| tool_error(&format!("could not read `{}`: {e}", path.display())))
}

/// Parses `--schedule`'s value: a bare seed or an `ev:` decision stream.
fn parse_schedule(text: &str) -> Result<wolf_interp::eval::SchedRequest, String> {
    use wolf_interp::eval::SchedRequest;
    if let Some(stream) = text.strip_prefix("ev:") {
        let choices: Result<Vec<usize>, _> = if stream.is_empty() {
            Ok(Vec::new())
        } else {
            stream.split(',').map(str::parse).collect()
        };
        return choices
            .map(SchedRequest::Stream)
            .map_err(|e| format!("`--schedule={text}` is not a decision stream: {e}"));
    }
    text.parse()
        .map(SchedRequest::Seed)
        .map_err(|e| format!("`--schedule={text}` is neither a seed nor `ev:…`: {e}"))
}

fn run_conform_run(args: &ConformRunArgs) -> u8 {
    let source = match read_program(&args.file) {
        Ok(source) => source,
        Err(code) => return code,
    };

    if let Some(budget) = args.explore {
        return run_explore(args, budget);
    }

    let request = match (args.seed, args.schedule.as_deref()) {
        (Some(seed), _) => wolf_interp::eval::SchedRequest::Seed(seed),
        (None, Some(text)) => match parse_schedule(text) {
            Ok(request) => request,
            Err(message) => return tool_error(&message),
        },
        (None, None) => wolf_interp::eval::SchedRequest::Default,
    };
    let (record, observed) = wolf_interp::observe_record_scheduled(
        &args.file,
        &source,
        args.phase,
        args.trace.unwrap_or_default(),
        &request,
        args.std_root.as_deref(),
    );

    // Never emit a record this implementation's own validator would reject.
    let value = match serde_json::to_value(&record) {
        Ok(value) => value,
        Err(e) => return tool_error(&format!("could not build the observation record: {e}")),
    };
    if let Err(errors) = schema::validate(&value) {
        return tool_error(&format!("refusing to emit a malformed record: {errors}"));
    }

    if args.json {
        match record.to_json_line() {
            Ok(line) => println!("{line}"),
            Err(e) => return tool_error(&format!("could not serialize the record: {e}")),
        }
    } else {
        // Human mode is unspecified by the protocol; rich detail rides stderr
        // with `line:col` locations (`[conf.trap.render]` — the record itself
        // keeps byte spans, `[proto.record.diag]`/`[proto.record.ext]`).
        println!(
            "{}: verdict={} phase_reached={} seeded={}",
            record.file, record.verdict, record.phase_reached, record.seeded
        );
        let text = String::from_utf8_lossy(&source);
        if let Some(detail) = &observed.diagnostic {
            let reason = record
                .extensions
                .get("x-unsupported")
                .and_then(|v| v.as_str());
            eprintln!(
                "  {}",
                render_static_diag(detail, reason, Some(&args.file), &text)
            );
        }
        if let Some(trap) = &observed.trap {
            eprintln!("  {}", trap.render(&text));
        }
        if let Some(finding) = &observed.ub {
            eprintln!("  {}", finding.render(&text));
        }
        if let Some(reason) = record.extensions.get("x-unsupported") {
            eprintln!("  unsupported: {}", reason.as_str().unwrap_or_default());
        }
        if let Some(inline) = &record.stdout_inline {
            print!("{inline}");
        }
    }

    for line in &observed.trace {
        eprintln!("trace {line}");
    }

    if matches!(record.verdict, Verdict::Unsupported)
        && args
            .phase
            .is_some_and(|p| p > wolf_interp::frontend::DEEPEST_STATIC)
        && args.phase != Some(Phase::Run)
    {
        eprintln!(
            "note: --phase={} requested; this implementation completes `{}` and does not \
             perform the static phases beyond it — they are enforced dynamically at `run` \
             instead (see the ladder mapping in `frontend`)",
            args.phase.expect("checked"),
            wolf_interp::frontend::DEEPEST_STATIC,
        );
    }

    // The record carries the program's outcome; the tool succeeded.
    EXIT_OK
}

/// `conform-run --explore=N`: is07's bounded exhaustive interleaving search.
fn run_explore(args: &ConformRunArgs, budget: u64) -> u8 {
    if args.phase.is_some() && args.phase != Some(Phase::Run) {
        return tool_error("--explore explores the run rung; drop --phase or pass --phase=run");
    }
    let options = wolf_interp::explore::Options {
        max_schedules: budget,
        max_steps: args.explore_steps,
        max_preemptions: args.explore_preemptions,
        wall: args.explore_wall_ms.map(Duration::from_millis),
        naive: args.explore_naive,
        prune: !args.explore_no_prune,
        paranoid: args.paranoid,
        ..wolf_interp::explore::Options::default()
    };
    let report = match wolf_interp::explore_file(&args.file, &options, args.std_root.as_deref()) {
        Ok(report) => report,
        Err(message) => {
            return tool_error(&format!(
                "{}: cannot explore — {message}",
                wolf_interp::slash_path(&args.file)
            ));
        }
    };
    let display = wolf_interp::slash_path(&args.file);
    if args.json {
        match serde_json::to_string(&wolf_interp::explore::to_json(&display, &report, &options)) {
            Ok(line) => println!("{line}"),
            Err(e) => return tool_error(&format!("could not serialize the report: {e}")),
        }
    } else {
        print!(
            "{}",
            wolf_interp::explore::render(&display, &report, &options)
        );
    }
    if report.green() {
        EXIT_OK
    } else {
        EXIT_CHECK_FAILED
    }
}

fn run_lex(args: &FrontendArgs) -> u8 {
    let source = match read_program(&args.file) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let lexed = lex::lex_bytes(&source);
    if args.dump {
        print!("{}", lex::dump(&lexed));
    }
    match lexed.first_error() {
        None => {
            if !args.dump {
                println!(
                    "{}: {} token(s), no lexical faults",
                    wolf_interp::slash_path(&args.file),
                    lexed.tokens.len()
                );
            }
            EXIT_OK
        }
        Some(diag) => {
            eprintln!(
                "{}: {}",
                wolf_interp::slash_path(&args.file),
                diag.render(&String::from_utf8_lossy(&source))
            );
            EXIT_REJECTED
        }
    }
}

fn run_parse(args: &FrontendArgs) -> u8 {
    let source = match read_program(&args.file) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let text = match std::str::from_utf8(&source) {
        Ok(text) => text,
        Err(_) => {
            eprintln!(
                "{}: source is not UTF-8",
                wolf_interp::slash_path(&args.file)
            );
            return EXIT_REJECTED;
        }
    };
    match parse::parse_source(text) {
        Ok(parsed) => {
            if args.dump {
                print!("{}", parse::trace(&parsed.unit));
            } else {
                println!(
                    "{}: {} item(s), parsed clean",
                    wolf_interp::slash_path(&args.file),
                    parsed.unit.items.len()
                );
            }
            EXIT_OK
        }
        Err(diag) => {
            // First error wins; there is no recovery and no second opinion.
            eprintln!(
                "{}: {}",
                wolf_interp::slash_path(&args.file),
                diag.render(text)
            );
            EXIT_REJECTED
        }
    }
}

fn run_protocol_validate(files: &[PathBuf]) -> u8 {
    let mut rejected = 0usize;
    for file in files {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(e) => return tool_error(&format!("could not read `{}`: {e}", file.display())),
        };
        let display = display_path(file);
        match serde_json::from_str::<serde_json::Value>(&text) {
            Err(e) => {
                rejected += 1;
                println!("reject  {display}");
                println!("        not JSON: {e}");
            }
            Ok(value) => match schema::validate(&value) {
                Ok(()) => println!("accept  {display}"),
                Err(errors) => {
                    rejected += 1;
                    println!("reject  {display}");
                    for error in errors.0 {
                        println!("        {error}");
                    }
                }
            },
        }
    }

    if rejected == 0 {
        EXIT_OK
    } else {
        EXIT_CHECK_FAILED
    }
}

fn display_path(path: &Path) -> String {
    wolf_interp::slash_path(path)
}

// ---------------------------------------------------------------------------
// diff-run — the is05 differential runner
// ---------------------------------------------------------------------------

/// Resolves the counterparty, or prints the loud SKIP and decides the exit.
fn counterparty_or_skip(explicit: Option<&Path>, require: bool) -> Result<PathBuf, u8> {
    match differ::detect_counterparty(explicit) {
        Counterparty::Found(path) => {
            eprintln!("notice: counterparty compiler: {}", path.display());
            Ok(path)
        }
        Counterparty::Missing { tried } => {
            eprint!("{}", differ::skip_notice(&tried));
            if require {
                eprintln!("lupin: --require-counterparty was passed and no compiler binary exists");
                Err(EXIT_CHECK_FAILED)
            } else {
                Err(EXIT_OK)
            }
        }
    }
}

struct DiffOutcome {
    divergences: Vec<DeepDivergence>,
    ledger: Vec<LedgerEntry>,
    compared: usize,
    skipped_members: usize,
}

fn diff_corpus(args: &DiffRunArgs, compiler: &Path) -> Result<DiffOutcome, u8> {
    let this = std::env::current_exe()
        .map_err(|e| tool_error(&format!("cannot locate this binary: {e}")))?;
    let report = corpus::walk(&args.corpus, None)
        .map_err(|e| tool_error(&format!("cannot walk `{}`: {e}", args.corpus.display())))?;
    let timeout = Duration::from_millis(args.timeout_ms);

    let mut out = DiffOutcome {
        divergences: Vec::new(),
        ledger: Vec::new(),
        compared: 0,
        skipped_members: 0,
    };
    for file in &report.files {
        match &file.outcome {
            Outcome::Member(_) => {
                out.skipped_members += 1;
                continue;
            }
            Outcome::Failed(reason) => {
                return Err(tool_error(&format!(
                    "{}: unreadable corpus header: {reason}",
                    file.path
                )));
            }
            Outcome::Entry(_) => {}
        }
        let full = args.corpus.join(&file.path);
        let source = std::fs::read(&full).unwrap_or_default();
        let has_unsafe = source_has_unsafe(&source);
        // Our side has one engine, so it is always invoked plainly; the tier
        // selects which of the counterparty's engines answers.
        let a = differ::invoke(&this, &full, timeout);
        let b = differ::invoke_tier(compiler, &full, timeout, args.counterparty_tier.into());
        let comparison = differ::compare_invocations(&file.path, &a, &b, has_unsafe);
        out.compared += 1;
        if let Some(divergence) = comparison.divergence {
            out.divergences.push(divergence);
        }
        out.ledger.extend(comparison.ledger);
    }
    // `[proto.cmp.severity]`: the report orders by class, then by file so two
    // runs of the same trees diff cleanly.
    out.divergences
        .sort_by(|x, y| (x.class, &x.file).cmp(&(y.class, &y.file)));
    Ok(out)
}

/// The safe-tier filter the corpus tests already use: a file with no
/// `unsafe`, no `asm`, and no C import is safe-tier by construction.
fn source_has_unsafe(source: &[u8]) -> bool {
    let text = String::from_utf8_lossy(source);
    text.contains("unsafe") || text.contains("import c ") || text.contains("asm ")
}

/// Renders the human summary; returns the exit code per the gating posture:
/// soundness candidates always gate; anything else gates only unfiled.
fn report_diff(outcome: &DiffOutcome, args_filing: bool) -> u8 {
    let mut by_class: BTreeMap<&str, usize> = BTreeMap::new();
    for d in &outcome.divergences {
        *by_class.entry(d.class.as_str()).or_default() += 1;
    }
    let mut by_ledger: BTreeMap<String, usize> = BTreeMap::new();
    for entry in &outcome.ledger {
        let key = match entry {
            LedgerEntry::Unsupported { side, .. } => {
                format!("unsupported({})", side.as_str())
            }
            LedgerEntry::RejectsBeyond { side, .. } => {
                format!("rejects-beyond({})", side.as_str())
            }
            LedgerEntry::RunUnmatched { .. } => "run-unmatched".to_owned(),
        };
        *by_ledger.entry(key).or_default() += 1;
    }

    println!(
        "differential: {} entr{} compared, {} member(s) exercised through their entries",
        outcome.compared,
        if outcome.compared == 1 { "y" } else { "ies" },
        outcome.skipped_members
    );
    println!("divergences: {}", outcome.divergences.len());
    for (class, count) in &by_class {
        println!("  {class}: {count}");
    }
    println!("conservatism ledger: {} entr{}", outcome.ledger.len(), {
        if outcome.ledger.len() == 1 {
            "y"
        } else {
            "ies"
        }
    });
    for (kind, count) in &by_ledger {
        println!("  {kind}: {count}");
    }

    let mut gating = 0usize;
    for d in &outcome.divergences {
        let filed = d
            .filed
            .map(|id| format!(" [filed: {id}]"))
            .unwrap_or_default();
        println!(
            "{}  {}  a={}  b={}  {}{filed}",
            d.class,
            d.file,
            d.a,
            d.b,
            d.rung.map_or("-", Phase::as_str),
        );
        let gates = d.class == DeepClass::SoundnessCandidate || d.filed.is_none();
        if gates {
            gating += 1;
        }
        if args_filing && d.filed.is_none() {
            println!("{}", d.filing());
        }
    }

    if gating == 0 {
        println!(
            "differential: GREEN — every divergence is filed in docs/divergence-log.md \
             and none is a soundness candidate"
        );
        EXIT_OK
    } else {
        println!(
            "differential: {gating} gating divergence(s) — soundness candidates gate always; \
             everything else gates until filed with a triage in docs/divergence-log.md"
        );
        EXIT_CHECK_FAILED
    }
}

fn write_jsonl(path: &Path, lines: impl Iterator<Item = serde_json::Value>) -> Result<(), u8> {
    let mut text = String::new();
    for line in lines {
        text.push_str(&line.to_string());
        text.push('\n');
    }
    std::fs::write(path, text)
        .map_err(|e| tool_error(&format!("cannot write `{}`: {e}", path.display())))
}

fn run_diff_run(args: &DiffRunArgs) -> u8 {
    let compiler = match counterparty_or_skip(args.compiler.as_deref(), args.require_counterparty) {
        Ok(path) => path,
        Err(code) => return code,
    };
    // The lane is part of the observation: the same corpus against `default`
    // and against `release` are different claims, so a log must say which one
    // it is looking at.
    let tier: differ::CounterpartyTier = args.counterparty_tier.into();
    eprintln!(
        "notice: counterparty tier: {} (conform-run{}{})",
        tier.label(),
        if tier.flags().is_empty() { "" } else { " " },
        tier.flags().join(" ")
    );
    let outcome = match diff_corpus(args, &compiler) {
        Ok(outcome) => outcome,
        Err(code) => return code,
    };
    if let Some(path) = &args.report
        && let Err(code) = write_jsonl(
            path,
            outcome.divergences.iter().map(DeepDivergence::to_json),
        )
    {
        return code;
    }
    if let Some(path) = &args.ledger
        && let Err(code) = write_jsonl(path, outcome.ledger.iter().map(LedgerEntry::to_json))
    {
        return code;
    }
    if args.json {
        for d in &outcome.divergences {
            println!("{}", d.to_json());
        }
        let gating = outcome
            .divergences
            .iter()
            .filter(|d| d.class == DeepClass::SoundnessCandidate || d.filed.is_none())
            .count();
        return if gating == 0 {
            EXIT_OK
        } else {
            EXIT_CHECK_FAILED
        };
    }
    report_diff(&outcome, args.filing)
}

// ---------------------------------------------------------------------------
// fuzz — generation, comparison, reduction
// ---------------------------------------------------------------------------

fn run_fuzz(args: &FuzzArgs) -> u8 {
    if let Err(e) = std::fs::create_dir_all(&args.out) {
        return tool_error(&format!("cannot create `{}`: {e}", args.out.display()));
    }
    let compiler = match differ::detect_counterparty(args.compiler.as_deref()) {
        Counterparty::Found(path) => {
            eprintln!("notice: counterparty compiler: {}", path.display());
            Some(path)
        }
        Counterparty::Missing { tried } => {
            eprint!("{}", differ::skip_notice(&tried));
            if args.require_counterparty {
                eprintln!("lupin: --require-counterparty was passed and no compiler binary exists");
                return EXIT_CHECK_FAILED;
            }
            eprintln!(
                "notice: fuzz continues in self-oracle mode (defined programs must exit 0; \
                 no unsafe-free program may report ub)"
            );
            None
        }
    };
    let this = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => return tool_error(&format!("cannot locate this binary: {e}")),
    };
    let timeout = Duration::from_millis(args.timeout_ms);

    println!(
        "fuzz: seed={:#x} count={} mode={:?} counterparty={}",
        args.seed,
        args.count,
        args.mode,
        compiler.is_some(),
    );

    let mut findings = 0usize;
    let mut divergent_classes: BTreeMap<&str, usize> = BTreeMap::new();
    let mut ledger_entries = 0usize;
    let case_path = args.out.join("case.lu");
    for index in 0..args.count {
        let mode = match args.mode {
            FuzzMode::Defined => Mode::Defined,
            FuzzMode::Boundary => Mode::Boundary,
            FuzzMode::Mixed => {
                if index % 2 == 0 {
                    Mode::Defined
                } else {
                    Mode::Boundary
                }
            }
        };
        let program = fuzz::generate(&mut fuzz::Rng::for_case(args.seed, index), mode);
        let source = fuzz::render(&program);

        let problem: Option<(&'static str, String)> = if let Some(compiler) = &compiler {
            if std::fs::write(&case_path, &source).is_err() {
                return tool_error("cannot write the generated case");
            }
            let a = differ::invoke(&this, &case_path, timeout);
            let b = differ::invoke(compiler, &case_path, timeout);
            let comparison = differ::compare_invocations("fuzz/case.lu", &a, &b, false);
            ledger_entries += comparison.ledger.len();
            // In defined mode the generator *promises* a well-typed program,
            // so a counterparty rejection is never conservatism — it indicts
            // the generator or the compiler, and either way a human looks.
            let rejected_beyond = comparison.ledger.iter().find_map(|entry| match entry {
                LedgerEntry::RejectsBeyond {
                    side: wolf_interp::differ::Side::Counterparty,
                    code,
                    phase,
                    ..
                } if mode == Mode::Defined => Some((code.clone(), *phase)),
                _ => None,
            });
            comparison
                .divergence
                .map(|d| {
                    *divergent_classes.entry(d.class.as_str()).or_default() += 1;
                    (d.class.as_str(), format!("a={} b={}", d.a, d.b))
                })
                .or_else(|| {
                    rejected_beyond.map(|(code, phase)| {
                        *divergent_classes.entry("verdict").or_default() += 1;
                        (
                            "verdict",
                            format!(
                                "counterparty rejects a defined-by-construction program: \
                                 {code} at {phase}"
                            ),
                        )
                    })
                })
        } else {
            self_oracle(&source, mode)
        };

        if let Some((class, detail)) = problem {
            findings += 1;
            // Reduce with the same oracle the finding came from, then keep
            // the minimized case beside its seed so it replays.
            let mut still = |candidate: &fuzz::GProgram| {
                let candidate_source = fuzz::render(candidate);
                if let Some(compiler) = &compiler {
                    if std::fs::write(&case_path, &candidate_source).is_err() {
                        return false;
                    }
                    let a = differ::invoke(&this, &case_path, timeout);
                    let b = differ::invoke(compiler, &case_path, timeout);
                    let comparison = differ::compare_invocations("fuzz/case.lu", &a, &b, false);
                    comparison
                        .divergence
                        .as_ref()
                        .is_some_and(|d| d.class.as_str() == class)
                        || (mode == Mode::Defined
                            && comparison.ledger.iter().any(|entry| {
                                matches!(
                                    entry,
                                    LedgerEntry::RejectsBeyond {
                                        side: wolf_interp::differ::Side::Counterparty,
                                        ..
                                    }
                                )
                            }))
                } else {
                    self_oracle(&candidate_source, mode).is_some_and(|(c, _)| c == class)
                }
            };
            let minimized = fuzz::reduce(&program, &mut still);
            let repro = args.out.join(format!("finding-{index}.lu"));
            let text = format!(
                "// fuzz finding: class={class} seed={:#x} index={index} mode={}\n// {detail}\n{}",
                args.seed,
                mode.as_str(),
                fuzz::render(&minimized)
            );
            if std::fs::write(&repro, text).is_err() {
                return tool_error("cannot write the minimized reproducer");
            }
            println!(
                "FINDING {class} at index {index} (minimized: {})",
                repro.display()
            );
            println!("  {detail}");
        }
    }

    println!(
        "fuzz: {} case(s), {} finding(s), {} conservatism entr{}",
        args.count,
        findings,
        ledger_entries,
        if ledger_entries == 1 { "y" } else { "ies" }
    );
    for (class, count) in &divergent_classes {
        println!("  {class}: {count}");
    }
    if findings == 0 {
        EXIT_OK
    } else {
        EXIT_CHECK_FAILED
    }
}

/// The counterparty-less oracle: what a generated program must do by
/// construction. Defined mode promises a clean exit; every mode promises an
/// unsafe-free program never reports UB and always parses.
fn self_oracle(source: &str, mode: Mode) -> Option<(&'static str, String)> {
    let observation = wolf_interp::frontend::observe(source.as_bytes(), None);
    match (&observation.verdict, mode) {
        (Verdict::Ub(anchor), _) => Some((
            "soundness-candidate",
            format!("ub({anchor}) in an unsafe-free generated program"),
        )),
        (Verdict::Fail(code), _) => Some((
            "verdict",
            format!("the generator emitted a program this frontend rejects: {code}"),
        )),
        (Verdict::Exit(0), _) => None,
        (verdict, Mode::Defined) => Some((
            "verdict",
            format!("a defined-by-construction program produced {verdict}"),
        )),
        (_, Mode::Boundary) => None,
    }
}
