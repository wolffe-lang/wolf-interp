//! Doc-truth: the command/output pairs in README.md and `docs/manual/*.md`
//! are tests, not prose.
//!
//! Every fenced ` ```console ` block is executed here: `$ `-prefixed lines
//! are commands run against the built binary from the repository root,
//! everything else is the expected output — stdout then stderr —
//! byte-compared after the repo's normalizations (CRLF → LF,
//! `vendor/upstream/` reported as `upstream/`). `…` is the only placeholder:
//! alone on a line it matches zero or more elided lines; inside a line it
//! matches a span that legitimately varies (a build commit, a bundle hash).
//! A block whose command is `wolf-interp repl` is replayed as a piped
//! session: the `wolf> `/`....> ` lines are fed as input and the whole block
//! is the expected transcript. Blocks fenced `sh` or `text` are never
//! executed. The conventions are documented in `docs/README.md`.
//!
//! Commands may write only under `target/`, so a run leaves the tree clean.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_wolf-interp")
}

/// The scanned set: the README and every manual page, in a stable order.
fn doc_files() -> Vec<PathBuf> {
    let root = manifest_dir();
    let mut files = vec![root.join("README.md")];
    let manual = root.join("docs/manual");
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&manual)
        .expect("docs/manual exists")
        .map(|entry| entry.expect("readable dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    pages.sort();
    files.extend(pages);
    files
}

/// One fenced `console` block: where it is, and its lines.
struct Block {
    file: String,
    line: usize,
    lines: Vec<String>,
}

fn console_blocks(path: &Path) -> Vec<Block> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    let file = path
        .strip_prefix(manifest_dir())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let mut blocks = Vec::new();
    let mut current: Option<Block> = None;
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        if current.is_none() {
            if line == "```console" {
                current = Some(Block {
                    file: file.clone(),
                    line: idx + 1,
                    lines: Vec::new(),
                });
            }
        } else if line == "```" {
            blocks.push(current.take().expect("block in progress"));
        } else {
            current
                .as_mut()
                .expect("block in progress")
                .lines
                .push(line.to_owned());
        }
    }
    assert!(
        current.is_none(),
        "{file}: an unterminated ```console fence"
    );
    blocks
}

/// A command with the output block that follows it.
struct Pair {
    context: String,
    argv: Vec<String>,
    expected: Vec<String>,
}

fn pairs(block: &Block) -> Vec<Pair> {
    let mut pairs: Vec<Pair> = Vec::new();
    for line in &block.lines {
        if let Some(command) = line.strip_prefix("$ ") {
            assert!(
                !command.contains(['"', '\'', '|', '>', '<', '&']),
                "{} line {}: `{command}` — doc-truth commands are plain \
                 argv, no shell syntax",
                block.file,
                block.line
            );
            let argv: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
            assert_eq!(
                argv.first().map(String::as_str),
                Some("wolf-interp"),
                "{} line {}: every checked command invokes `wolf-interp`",
                block.file,
                block.line
            );
            pairs.push(Pair {
                context: format!("{} line {}: `{command}`", block.file, block.line),
                argv,
                expected: Vec::new(),
            });
        } else {
            let pair = pairs.last_mut().unwrap_or_else(|| {
                panic!(
                    "{} line {}: output before any `$ ` command",
                    block.file, block.line
                )
            });
            pair.expected.push(line.clone());
        }
    }
    pairs
}

/// The established normalizations: LF, forward slashes (windows prints
/// `upstream\corpus`), and the vendored fallback reported under its
/// canonical `upstream/` spelling.
fn normalize(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\\', "/")
        .replace("vendor/upstream/", "upstream/")
}

/// Line-wise match with the two placeholder forms of `…`.
fn matches_expected(expected: &[String], actual: &str) -> bool {
    let actual: Vec<&str> = actual.trim_end_matches('\n').lines().collect();
    let expected: Vec<&str> = {
        let mut lines: Vec<&str> = expected.iter().map(String::as_str).collect();
        while lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        lines
    };
    match_lines(&expected, &actual)
}

fn match_lines(expected: &[&str], actual: &[&str]) -> bool {
    match expected.split_first() {
        None => actual.is_empty(),
        Some((&"…", rest)) => {
            // Zero or more elided lines.
            (0..=actual.len()).any(|skip| match_lines(rest, &actual[skip..]))
        }
        Some((first, rest)) => actual
            .split_first()
            .is_some_and(|(head, tail)| match_line(first, head) && match_lines(rest, tail)),
    }
}

/// An inline `…` matches any span within the line.
fn match_line(expected: &str, actual: &str) -> bool {
    if !expected.contains('…') {
        return expected == actual;
    }
    let segments: Vec<&str> = expected.split('…').collect();
    let (first, last) = (segments[0], segments[segments.len() - 1]);
    if !actual.starts_with(first) {
        return false;
    }
    let mut rest = &actual[first.len()..];
    for segment in &segments[1..segments.len() - 1] {
        match rest.find(segment) {
            Some(at) => rest = &rest[at + segment.len()..],
            None => return false,
        }
    }
    rest.ends_with(last) || (segments.len() == 2 && last.is_empty())
}

/// Runs one documented command; returns the observed output, normalized.
fn run_pair(pair: &Pair) -> String {
    if pair.argv[1..] == ["repl"] {
        return run_repl(pair);
    }
    let output = Command::new(binary())
        .args(&pair.argv[1..])
        .current_dir(manifest_dir())
        .output()
        .unwrap_or_else(|err| panic!("{}: {err}", pair.context));
    format!("{}{}", normalize(&output.stdout), normalize(&output.stderr))
}

/// A `wolf-interp repl` pair is a transcript: feed the prompt lines as
/// input, expect the piped session to render the block byte-for-byte.
fn run_repl(pair: &Pair) -> String {
    let inputs: Vec<&str> = pair
        .expected
        .iter()
        .filter_map(|line| {
            line.strip_prefix("wolf> ")
                .or_else(|| line.strip_prefix("....> "))
        })
        .collect();
    let mut child = Command::new(binary())
        .arg("repl")
        .current_dir(manifest_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| panic!("{}: {err}", pair.context));
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(format!("{}\n", inputs.join("\n")).as_bytes())
        .unwrap_or_else(|err| panic!("{}: {err}", pair.context));
    let output = child
        .wait_with_output()
        .unwrap_or_else(|err| panic!("{}: {err}", pair.context));
    format!("{}{}", normalize(&output.stdout), normalize(&output.stderr))
}

#[test]
fn every_documented_output_is_the_binary_output() {
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for path in doc_files() {
        for block in console_blocks(&path) {
            for pair in pairs(&block) {
                let actual = run_pair(&pair);
                checked += 1;
                if !matches_expected(&pair.expected, &actual) {
                    failures.push(format!(
                        "{}\n--- documented:\n{}\n--- observed:\n{}",
                        pair.context,
                        pair.expected.join("\n"),
                        actual
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} documented pair(s) drifted from the binary:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    // Extraction rot guard: the docs carry a known-sized set of pairs; a
    // parser regression that silently found none would otherwise pass.
    assert!(checked >= 12, "only {checked} pair(s) extracted");
}

/// The manual's command tour and `wolf-interp --help` state the same
/// one-liners (is11: help text and manual may not drift apart).
#[test]
fn the_command_tour_matches_help() {
    let manual = std::fs::read_to_string(manifest_dir().join("docs/manual/README.md"))
        .expect("the manual index exists");
    let mut tour = Vec::new();
    for line in manual.lines() {
        // Rows shaped `| `cmd` | description |` in the Commands table.
        if let Some(row) = line.strip_prefix("| `") {
            if row.starts_with("command") {
                continue;
            }
            let (command, rest) = row.split_once("` | ").expect("well-formed table row");
            let description = rest.trim_end_matches(" |");
            tour.push((command.to_owned(), description.to_owned()));
        }
    }
    assert!(
        !tour.is_empty(),
        "the manual index carries the command tour"
    );

    let output = Command::new(binary())
        .arg("--help")
        .current_dir(manifest_dir())
        .output()
        .expect("wolf-interp --help runs");
    let help = normalize(&output.stdout);
    let mut commands = Vec::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line == "Commands:" {
            in_commands = true;
            continue;
        }
        if in_commands {
            if line.is_empty() {
                break;
            }
            let mut words = line.split_whitespace();
            let name = words.next().expect("a command name").to_owned();
            let description = words.collect::<Vec<_>>().join(" ");
            if name != "help" {
                commands.push((name, description));
            }
        }
    }

    for (name, description) in &commands {
        let documented = tour
            .iter()
            .find(|(command, _)| command == name)
            .unwrap_or_else(|| panic!("`{name}` is missing from the manual's command tour"));
        assert_eq!(
            &documented.1, description,
            "`{name}`: the manual tour and --help disagree"
        );
    }
    assert_eq!(
        tour.len(),
        commands.len(),
        "the tour lists a command --help does not"
    );
}
