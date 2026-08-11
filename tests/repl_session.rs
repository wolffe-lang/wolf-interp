//! The is08 acceptance suite: the REPL and its transcript contract.
//!
//! The claims, each a section:
//!
//! 1. **The pinned transcripts replay byte-identically** — every file under
//!    `tests/repl/` through `lupin repl --script`, on all three OSes
//!    (the repo forces `eol=lf`, so bytes are bytes). This is the book's
//!    no-rot gate: bs00's runner consumes the same files unchanged.
//! 2. **Replay is deterministic** — the same input stream through two fresh
//!    sessions produces byte-identical output (pipe mode IS the transcript
//!    producer, so this is "a session script replays byte-identically").
//! 3. **Drift fails** — a doctored transcript exits 1 with the drift named;
//!    without the red case, "byte-identical" could be true vacuously.
//! 4. **The redefinition matrix** and **error recovery** are pinned by the
//!    transcripts themselves (`redefinition.transcript`: shadowed `fn` with
//!    the old capture intact, stale type generation printing, the
//!    mix-error; `basics.transcript`: a parse error with code + anchor and
//!    the session alive after it; `moves.transcript`/`handle_stale.
//!    transcript`/`deadlock_when.transcript`: traps print their kind and
//!    clause and the world survives, [repl.trap.alive]).
//! 5. **The `repl` tag namespace is accepted** — the `[repl.*]` notes in
//!    `docs/repl.md` classify as a reserved forward namespace, which is
//!    what lets is09 export them.

use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_lupin")
}

fn transcript_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/repl")
}

fn transcripts() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(transcript_dir())
        .expect("tests/repl exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "transcript"))
        .collect();
    found.sort();
    found
}

/// Feeds `input` to a fresh piped session and returns its stdout bytes.
fn pipe_session(input: &str) -> Vec<u8> {
    use std::io::Write;
    let mut child = Command::new(binary())
        .arg("repl")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(input.as_bytes())
        .expect("stdin accepts the session");
    let output = child.wait_with_output().expect("the session ends");
    assert!(output.status.success(), "the session exits 0");
    output.stdout
}

// ---------------------------------------------------------------------------
// 1. the pinned transcripts replay byte-identically
// ---------------------------------------------------------------------------

#[test]
fn every_pinned_transcript_replays_byte_identically() {
    let transcripts = transcripts();
    assert!(
        transcripts.len() >= 10,
        "the acceptance floor is ten CI-replayed transcripts; found {}",
        transcripts.len()
    );
    for transcript in transcripts {
        let output = Command::new(binary())
            .args(["repl", "--script"])
            .arg(&transcript)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("the binary runs");
        assert!(
            output.status.success(),
            "{} drifted:\nstdout: {}\nstderr: {}",
            transcript.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("byte-identical"),
            "{}: {stdout}",
            transcript.display()
        );
    }
}

#[test]
fn the_acceptance_topics_are_all_covered() {
    // The sprint's own list, as filenames: values/moves, a region lifecycle
    // in `:mem`, `shared` counts moving, a stale handle visible then
    // faulting, a `:trace` session over an is04 provenance flag, a channel
    // ping-pong under the sim scheduler, and the redefinition matrix.
    let have: Vec<String> = transcripts()
        .iter()
        .filter_map(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    for want in [
        "moves",
        "region_lifecycle",
        "shared_counts",
        "handle_stale",
        "trace_prov",
        "chan_pingpong",
        "redefinition",
    ] {
        assert!(have.iter().any(|s| s == want), "missing {want}: {have:?}");
    }
}

// ---------------------------------------------------------------------------
// 2. determinism: one input stream, two sessions, identical bytes
// ---------------------------------------------------------------------------

#[test]
fn a_piped_session_is_deterministic() {
    let script = std::fs::read_to_string(transcript_dir().join("chan_pingpong.transcript"))
        .expect("the ping-pong transcript exists");
    // Reconstruct the raw input stream from the transcript's own prompts.
    let inputs: String = script
        .lines()
        .filter_map(|line| {
            line.strip_prefix("wolf> ")
                .or_else(|| line.strip_prefix("....> "))
        })
        .fold(String::new(), |acc, line| acc + line + "\n");
    let first = pipe_session(&inputs);
    let second = pipe_session(&inputs);
    assert_eq!(first, second, "two fresh sessions must not diverge");
    // And pipe mode is the transcript producer: its bytes ARE the file.
    assert_eq!(
        String::from_utf8_lossy(&first).replace("\r\n", "\n"),
        script,
        "the captured session is the pinned transcript"
    );
}

// ---------------------------------------------------------------------------
// 3. drift fails, with the drift named
// ---------------------------------------------------------------------------

#[test]
fn a_doctored_transcript_fails_replay_with_the_drift_named() {
    let pinned = std::fs::read_to_string(transcript_dir().join("basics.transcript"))
        .expect("basics.transcript exists");
    let doctored = pinned.replace("3 : i32", "4 : i32");
    assert_ne!(pinned, doctored, "the doctoring must bite");
    let dir = std::env::temp_dir().join(format!("wolf-repl-drift-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("doctored.transcript");
    std::fs::write(&path, doctored).expect("writable");
    let output = Command::new(binary())
        .args(["repl", "--script"])
        .arg(&path)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(1), "drift is exit 1");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("DRIFT"), "{stderr}");
    assert!(stderr.contains("4 : i32"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// 4. session semantics spot checks (the transcripts pin the full matrix)
// ---------------------------------------------------------------------------

#[test]
fn a_shadowed_fn_keeps_the_old_capture_and_a_trap_keeps_the_session() {
    let stdout = pipe_session(
        "fn f() -> int { 1 }\nlet g = fn() { f() }\nfn f() -> int { 2 }\nf()\ng()\n\
         let xs = List[int]()\nxs[3]\n1 + 1\n:quit\n",
    );
    let text = String::from_utf8_lossy(&stdout);
    // `f` declares `-> int`, so its result is typed by the signature since
    // 0.1.4 (issue #14) and renders at int's width; the bare `1 + 1` after
    // the trap is still an unconstrained literal and keeps the i32 default.
    assert!(text.contains("2 : i64"), "{text}");
    assert!(text.contains("1 : i64"), "{text}");
    assert!(text.contains("trap(bounds)"), "{text}");
    assert!(text.contains("[repl.trap.alive]"), "{text}");
    // The session answered after the trap.
    assert!(
        text.contains("2 : i32\nwolf> :quit") || text.contains("2 : i32"),
        "{text}"
    );
}

#[test]
fn the_continuation_prompt_follows_the_lexers_termination_rules() {
    // An unclosed brace and a trailing operator both continue; the closed
    // input evaluates ([gram.lex.newline], byte-exact via the lexer).
    let stdout = pipe_session("let x = 1 +\n2\nif x == 3 {\n10\n} else {\n20\n}\n:quit\n");
    let text = String::from_utf8_lossy(&stdout);
    assert!(text.contains("....> 2"), "{text}");
    assert!(text.contains("....> } else {"), "{text}");
    assert!(text.contains("10 : i32"), "{text}");
}

// ---------------------------------------------------------------------------
// 5. the `repl` namespace is a legal forward namespace
// ---------------------------------------------------------------------------

#[test]
fn the_repl_note_namespace_classifies_as_reserved() {
    use wolf_interp::anchor::{self, Namespace};
    for tag in [
        "repl.def.shadow",
        "repl.type.gen",
        "repl.type.mix",
        "repl.let.rebind",
        "repl.module",
        "repl.trap.alive",
    ] {
        assert_eq!(
            anchor::classify(tag),
            Ok(Namespace::Reserved),
            "{tag} must be exportable by is09"
        );
    }
}

#[test]
fn every_repl_note_in_the_doc_is_a_wellformed_tag() {
    // The doc is the normative surface for `[repl.*]`; a typo'd tag there
    // would export garbage. Harvest every `[repl.…]` occurrence and check.
    let doc = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/repl.md"))
        .expect("docs/repl.md exists");
    let mut found = 0;
    for (index, _) in doc.match_indices("[repl.") {
        let rest = &doc[index + 1..];
        let Some(end) = rest.find(']') else { continue };
        let tag = &rest[..end];
        if tag.contains('*') {
            // The namespace itself, mentioned as `[repl.*]`.
            continue;
        }
        assert!(
            wolf_interp::anchor::classify(tag).is_ok(),
            "`[{tag}]` in docs/repl.md is not a well-formed anchor"
        );
        found += 1;
    }
    assert!(
        found >= 6,
        "the six [repl.*] notes must appear; found {found}"
    );
}
