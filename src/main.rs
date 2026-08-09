//! The `wolf-interp` command line.
//!
//! One subcommand is normative: `conform-run` implements
//! `spec/06-differential-protocol.md` `[proto.invoke]`. `lex` and `parse` are
//! the frontend's human doors — their output is ours and is not compared with
//! anything. `corpus` and `protocol validate` are the harness the rest of the
//! track is built on.
//!
//! Exit codes: `0` success, `1` the work ran and something failed the check,
//! `2` the tool itself could not run (missing file, bad flags) — matching the
//! compiler's convention so a differ can tell "the program is wrong" from
//! "the tool is wrong".

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use wolf_interp::corpus::{self, Outcome};
use wolf_interp::phase::Phase;
use wolf_interp::protocol::Verdict;
use wolf_interp::schema;
use wolf_interp::{lex, parse};

const EXIT_OK: u8 = 0;
const EXIT_CHECK_FAILED: u8 = 1;
const EXIT_TOOL_ERROR: u8 = 2;
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

#[derive(Debug, Parser)]
#[command(
    name = "wolf-interp",
    version,
    about = "The wolf reference interpreter",
    long_about = "The wolf reference interpreter: an independent implementation of the wolf \
specification, and the compiler's differential-testing oracle."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Walk the pinned corpus and report each file's directives.
    Corpus(CorpusArgs),
    /// Observe one program and emit a spec/06 observation record.
    ConformRun(ConformRunArgs),
    /// Tokenize one program (`spec/01` §1).
    Lex(FrontendArgs),
    /// Parse one program (`spec/01` §2-§6).
    Parse(FrontendArgs),
    /// Protocol utilities.
    Protocol {
        #[command(subcommand)]
        command: ProtocolCommand,
    },
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
    /// Request the deterministic schedule seeded per spec 03 §5.
    #[arg(long)]
    seed: Option<u64>,
    /// Emit the machine-readable observation record.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct FrontendArgs {
    /// The wolf program to read.
    file: PathBuf,
    /// Print the token stream (`lex`) or the production trace (`parse`).
    #[arg(long)]
    dump: bool,
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
    let code = match cli.command {
        Command::Corpus(args) => run_corpus(&args),
        Command::ConformRun(args) => run_conform_run(&args),
        Command::Lex(args) => run_lex(&args),
        Command::Parse(args) => run_parse(&args),
        Command::Protocol {
            command: ProtocolCommand::Validate { files },
        } => run_protocol_validate(&files),
    };
    ExitCode::from(code)
}

fn tool_error(message: &str) -> u8 {
    eprintln!("wolf-interp: {message}");
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

/// What the frontend makes of one corpus entry today, as one word for the
/// report. Members are never conform-run directly (`[conf.directive.member]`),
/// so they are not asked.
fn frontend_status(root: &Path, relative: &str) -> String {
    let Ok(source) = std::fs::read(root.join(relative)) else {
        return "unreadable".to_owned();
    };
    let observation = wolf_interp::frontend::observe(&source, None);
    format!("{}@{}", observation.verdict, observation.phase_reached)
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
                let status = frontend_status(&report.root, &file.path);
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
                     frontend={status:<24}  conforms={tags}",
                    file.path
                );
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
        report.root.display(),
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

    // The honest ledger: how far the frontend actually gets, counted rather
    // than claimed (`[proto.record.unsupported]` — scope gaps stay visible).
    let mut clean = 0usize;
    let mut rejected = 0usize;
    for file in &report.files {
        if !matches!(file.outcome, Outcome::Entry(_)) {
            continue;
        }
        if frontend_status(&report.root, &file.path).starts_with("fail") {
            rejected += 1;
        } else {
            clean += 1;
        }
    }
    println!();
    println!(
        "frontend: {clean} entr{} parse clean and stop at `unsupported` beyond `parse`; \
         {rejected} rejected with a pinned grammar-tier code (is01: no resolver, no types, \
         no evaluation)",
        if clean == 1 { "y" } else { "ies" }
    );
}

fn corpus_json(report: &corpus::CorpusReport) -> serde_json::Value {
    let files: Vec<serde_json::Value> = report
        .files
        .iter()
        .map(|file| match &file.outcome {
            Outcome::Entry(directives) => serde_json::json!({
                "file": file.path,
                "kind": "entry",
                "phase": directives.phase.map(|p| p.to_string()),
                "check": directives.check.as_ref().map(ToString::to_string),
                "conforms": directives.conforms,
                "interpreter_status": frontend_status(&report.root, &file.path),
            }),
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

fn run_conform_run(args: &ConformRunArgs) -> u8 {
    let source = match read_program(&args.file) {
        Ok(source) => source,
        Err(code) => return code,
    };

    let (record, detail) = wolf_interp::observe_record(&args.file, &source, args.phase);

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
        // Human mode is unspecified by the protocol; rich detail rides stderr.
        println!(
            "{}: verdict={} phase_reached={} seeded={}",
            record.file, record.verdict, record.phase_reached, record.seeded
        );
        if let Some(detail) = &detail {
            eprintln!("  {detail}");
        }
    }

    if matches!(record.verdict, Verdict::Unsupported)
        && args.phase.is_some_and(|p| p > Phase::Parse)
    {
        eprintln!(
            "note: --phase={} requested; this implementation completes `parse` and reports \
             `unsupported` beyond it (is01)",
            args.phase.expect("checked")
        );
    }
    if let Some(seed) = args.seed {
        eprintln!(
            "note: --seed={seed} requested; no seeded scheduling exists yet, so the record declares seeded=false ([proto.seed.flag])"
        );
    }

    // The record carries the program's outcome; the tool succeeded.
    EXIT_OK
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
            eprintln!("{}: {diag}", wolf_interp::slash_path(&args.file));
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
            eprintln!("{}: {diag}", wolf_interp::slash_path(&args.file));
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
