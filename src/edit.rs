//! is25 — the line editor's own layer: completion, validation, history
//! policy, and yank-last-arg, as pure logic.
//!
//! The editor (rustyline, MIT — see `[repl.edit.dep]` in `docs/repl.md`)
//! engages only at a TTY (`[repl.edit.tty]`); the piped path keeps the exact
//! dumb reader and its byte-identical transcript. Everything here is a
//! *reader* concern: no function in this module decides what an input means —
//! that stays with [`crate::eval::repl::Session::feed_line`].
//!
//! The rustyline glue (the `Helper` impl, the `Alt-.` handler) lives at the
//! bottom; the logic it defers to is plain functions above it, unit-tested
//! without a terminal in the room.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rustyline::completion::Completer;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{
    Cmd, ConditionalEventHandler, Context, Event, EventContext, Movement, RepeatCount,
};

// ---------------------------------------------------------------------------
// the validator: one editable buffer per input
// ---------------------------------------------------------------------------

/// Is the edited buffer a complete input, by the same rules the session
/// applies line-by-line?
///
/// - A buffer whose first line is a `:` directive is always complete —
///   [`crate::eval::repl::Session::feed_line`] recognizes directives only at
///   the top level, before any continuation, so the editor must not hold one
///   open (holding `:quit` open is exactly the wolf-interp#46 trap).
/// - Anything else asks [`crate::lex::repl_input_complete`] over the buffer
///   with the newline the session would have appended — the lexer's own
///   `[gram.lex.newline]` machinery, not a paraphrase.
#[must_use]
pub fn editor_input_complete(buffer: &str) -> bool {
    let first = buffer.lines().next().unwrap_or("");
    if first.trim_start().starts_with(':') {
        return true;
    }
    let mut src = String::with_capacity(buffer.len() + 1);
    src.push_str(buffer);
    src.push('\n');
    crate::lex::repl_input_complete(&src)
}

// ---------------------------------------------------------------------------
// completion: the three sources
// ---------------------------------------------------------------------------

/// The directive names `Session::directive` dispatches on, sorted.
pub const DIRECTIVES: &[&str] = &[
    "help", "keys", "load", "mem", "q", "quit", "regions", "reset", "rules", "schedule", "trace",
    "type",
];

/// `:trace`'s subcommands, sorted.
pub const TRACE_SUBCOMMANDS: &[&str] = &["clear", "off", "on", "show"];

/// Completion candidates for `:rules <prefix>`: every registry anchor plus
/// every dotted prefix of one (so `:rules co<TAB>` can first widen to a
/// namespace), sorted and deduplicated.
#[must_use]
pub fn rules_anchor_candidates() -> Vec<String> {
    let mut set = std::collections::BTreeSet::new();
    for row in crate::eval::rules::registry() {
        let anchor = row.anchor;
        set.insert(anchor.to_owned());
        for (index, ch) in anchor.char_indices() {
            if ch == '.' {
                set.insert(anchor[..index].to_owned());
            }
        }
    }
    set.into_iter().collect()
}

/// Is `name` a completable surface name? The generational internals a
/// session mints (`f#2`, `Point#1` — `[repl.def.shadow]`, `[repl.type.gen]`)
/// are never offered: the user types the surface name and the session
/// resolves the generation.
#[must_use]
pub fn is_surface_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('#')
}

/// Completes `line` at byte position `pos`. Returns the byte offset the
/// candidates replace from, and the candidates, sorted. Ambiguity is the
/// caller's UI concern (the editor lists candidates rather than guessing).
///
/// The three sources, per the is25 contract:
/// 1. `:` directives and their subcommands (`:trace on|off|show|clear`,
///    `:rules <prefix>` over `anchors`);
/// 2. the session's own bound names (`names` — surface names only);
/// 3. filesystem paths after `:load`, via `list_paths` so the logic here
///    stays pure.
#[must_use]
pub fn complete_input(
    line: &str,
    pos: usize,
    names: &[String],
    anchors: &[String],
    list_paths: &dyn Fn(&str) -> Vec<String>,
) -> (usize, Vec<String>) {
    let head = &line[..pos];
    let indent = line.len() - line.trim_start().len();
    if pos >= indent && line[indent..].starts_with(':') {
        let colon = indent;
        // Where the directive word ends (first whitespace after the colon).
        let first_end = line[colon..]
            .find(char::is_whitespace)
            .map_or(line.len(), |at| colon + at);
        if pos <= first_end {
            // Completing the directive word itself, colon included.
            let word = &line[colon + 1..pos];
            let items: Vec<String> = DIRECTIVES
                .iter()
                .filter(|name| name.starts_with(word))
                .map(|name| format!(":{name}"))
                .collect();
            return (colon, items);
        }
        // Completing an argument of a known directive.
        let directive = &line[colon + 1..first_end];
        let word_start = head
            .char_indices()
            .rev()
            .find(|(_, ch)| ch.is_whitespace())
            .map_or(0, |(at, ch)| at + ch.len_utf8());
        let word = &head[word_start..];
        let items: Vec<String> = match directive {
            "trace" => TRACE_SUBCOMMANDS
                .iter()
                .filter(|sub| sub.starts_with(word))
                .map(|sub| (*sub).to_owned())
                .collect(),
            "rules" => anchors
                .iter()
                .filter(|anchor| anchor.starts_with(word))
                .cloned()
                .collect(),
            "load" => list_paths(word),
            // `:type` takes an expression: complete session names in it.
            "type" => return complete_name(line, pos, names),
            _ => Vec::new(),
        };
        return (word_start, items);
    }
    complete_name(line, pos, names)
}

/// Identifier completion against the session's surface names.
fn complete_name(line: &str, pos: usize, names: &[String]) -> (usize, Vec<String>) {
    let head = &line[..pos];
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '_')
        .last()
        .map_or(pos, |(at, _)| at);
    let word = &head[start..];
    let items: Vec<String> = names
        .iter()
        .filter(|name| is_surface_name(name) && name.starts_with(word))
        .cloned()
        .collect();
    (start, items)
}

/// Filesystem candidates for a `:load` path prefix: the entries of the
/// prefix's directory whose names extend it, directories suffixed `/`.
/// Sorted, so listings are deterministic.
#[must_use]
pub fn fs_path_candidates(prefix: &str) -> Vec<String> {
    let split = prefix
        .rfind(['/', '\\'])
        .map_or(("", prefix), |at| (&prefix[..=at], &prefix[at + 1..]));
    let (dir_part, file_part) = split;
    let dir: &Path = if dir_part.is_empty() {
        Path::new(".")
    } else {
        Path::new(dir_part)
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut items: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(file_part) {
                return None;
            }
            let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
            let sep = if is_dir { "/" } else { "" };
            Some(format!("{dir_part}{name}{sep}"))
        })
        .collect();
    items.sort();
    items
}

// ---------------------------------------------------------------------------
// history: the policy and the platform-correct location
// ---------------------------------------------------------------------------

/// The persistent-history size cap (`[repl.edit.history]`).
pub const HISTORY_CAP: usize = 1000;

/// The in-memory history the editor and the disk file agree on.
///
/// Policy (`[repl.edit.history]`): empty and whitespace-only inputs are
/// never recorded; a consecutive duplicate is not recorded again; the list
/// is capped (oldest evicted first); a multi-line input is ONE entry — the
/// serialization is one JSON string per line, so embedded newlines survive
/// a round trip intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryStore {
    entries: Vec<String>,
    cap: usize,
}

impl HistoryStore {
    /// An empty store with the given cap.
    #[must_use]
    pub fn new(cap: usize) -> HistoryStore {
        HistoryStore {
            entries: Vec::new(),
            cap,
        }
    }

    /// Parses the on-disk format: one JSON-encoded string per line. Lines
    /// that do not parse are skipped (a corrupt history never blocks a
    /// prompt), and the cap applies keeping the NEWEST entries.
    #[must_use]
    pub fn parse(text: &str, cap: usize) -> HistoryStore {
        let mut store = HistoryStore::new(cap);
        for line in text.lines() {
            if let Ok(entry) = serde_json::from_str::<String>(line) {
                store.push(&entry);
            }
        }
        store
    }

    /// The on-disk format: one JSON-encoded string per line, oldest first.
    #[must_use]
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            out.push_str(&serde_json::to_string(entry).expect("strings serialize"));
            out.push('\n');
        }
        out
    }

    /// Records one input. Returns whether it was recorded (empty inputs and
    /// consecutive duplicates are not).
    pub fn push(&mut self, entry: &str) -> bool {
        let entry = entry.strip_suffix('\n').unwrap_or(entry);
        if entry.trim().is_empty() {
            return false;
        }
        if self.entries.last().is_some_and(|last| last == entry) {
            return false;
        }
        self.entries.push(entry.to_owned());
        if self.entries.len() > self.cap {
            let excess = self.entries.len() - self.cap;
            self.entries.drain(..excess);
        }
        true
    }

    /// The recorded entries, oldest first.
    #[must_use]
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// The last whitespace-separated word of the entry `back` steps behind
    /// the newest (`back` wraps, so repeated `Alt-.` cycles). `None` only
    /// when the store is empty or the selected entry has no words.
    #[must_use]
    pub fn last_arg(&self, back: usize) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        let index = self.entries.len() - 1 - (back % self.entries.len());
        self.entries[index]
            .split_whitespace()
            .last()
            .map(str::to_owned)
    }
}

/// Where the persistent history lives, resolved from explicit inputs so the
/// per-platform branches are testable on any host (`[repl.edit.history]`):
///
/// - `override_path` (`LUPIN_HISTORY`) wins; the empty string disables
///   persistence outright;
/// - on Windows, `%APPDATA%\lupin\history`;
/// - elsewhere, `$XDG_STATE_HOME/lupin/history`, defaulting to
///   `$HOME/.local/state/lupin/history` (the XDG state dir — history is
///   state, not config).
///
/// `None` means "no persistence this session" — never an error, never a
/// hardcoded fallback path.
#[must_use]
pub fn resolve_history_path(
    override_path: Option<&str>,
    windows: bool,
    appdata: Option<&str>,
    xdg_state_home: Option<&str>,
    home: Option<&str>,
) -> Option<PathBuf> {
    if let Some(path) = override_path {
        if path.is_empty() {
            return None;
        }
        return Some(PathBuf::from(path));
    }
    if windows {
        return appdata
            .filter(|dir| !dir.is_empty())
            .map(|dir| Path::new(dir).join("lupin").join("history"));
    }
    if let Some(state) = xdg_state_home.filter(|dir| !dir.is_empty()) {
        return Some(Path::new(state).join("lupin").join("history"));
    }
    home.filter(|dir| !dir.is_empty()).map(|dir| {
        Path::new(dir)
            .join(".local")
            .join("state")
            .join("lupin")
            .join("history")
    })
}

/// [`resolve_history_path`] over the live environment.
#[must_use]
pub fn history_path() -> Option<PathBuf> {
    let get = |name: &str| std::env::var(name).ok();
    resolve_history_path(
        get("LUPIN_HISTORY").as_deref(),
        cfg!(windows),
        get("APPDATA").as_deref(),
        get("XDG_STATE_HOME").as_deref(),
        get("HOME").or_else(|| get("USERPROFILE")).as_deref(),
    )
}

/// Loads the history file, tolerating absence.
#[must_use]
pub fn load_history(path: Option<&Path>) -> HistoryStore {
    match path.and_then(|path| std::fs::read_to_string(path).ok()) {
        Some(text) => HistoryStore::parse(&text, HISTORY_CAP),
        None => HistoryStore::new(HISTORY_CAP),
    }
}

/// Saves the history file, creating parent directories. Failure is
/// swallowed: a read-only state dir must never break a prompt.
pub fn save_history(path: Option<&Path>, store: &HistoryStore) {
    let Some(path) = path else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, store.serialize());
}

// ---------------------------------------------------------------------------
// yank-last-arg (`Alt-.` / `Alt-_`), cycling on repeat
// ---------------------------------------------------------------------------

/// The state one `Alt-.` leaves for the next to detect a repeat: how far
/// back it reached, and the exact buffer it expects to still be looking at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YankArgState {
    /// Steps behind the newest history entry the shown argument came from.
    pub back: usize,
    /// How many characters the shown argument occupies in the buffer.
    pub inserted_chars: usize,
    /// The buffer and cursor a repeat must find untouched.
    pub expect_line: String,
    /// Companion byte position to `expect_line`.
    pub expect_pos: usize,
}

/// What one `Alt-.` should do to the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YankArgCmd {
    /// Insert `text` at the cursor (a fresh yank).
    Insert(String),
    /// Replace the `chars_back` characters before the cursor with `text`
    /// (a repeat, cycling to an older entry).
    Replace { chars_back: usize, text: String },
}

/// One `Alt-.` step, pure. `state` is the previous step's state, if any;
/// a repeat is recognized by the buffer standing exactly where that state
/// left it (any intervening edit or motion starts a fresh cycle from the
/// newest entry — GNU readline's `yank-last-arg` temperament).
#[must_use]
pub fn yank_last_arg_step(
    state: Option<&YankArgState>,
    line: &str,
    pos: usize,
    store: &HistoryStore,
) -> Option<(YankArgState, YankArgCmd)> {
    let repeat = state.filter(|s| s.expect_line == line && s.expect_pos == pos);
    let (back, replace_chars) = match repeat {
        Some(s) => (s.back + 1, s.inserted_chars),
        None => (0, 0),
    };
    let text = store.last_arg(back)?;
    let start = line[..pos]
        .char_indices()
        .rev()
        .take(replace_chars)
        .last()
        .map_or(pos, |(at, _)| at);
    let mut expect_line = String::with_capacity(line.len() + text.len());
    expect_line.push_str(&line[..start]);
    expect_line.push_str(&text);
    expect_line.push_str(&line[pos..]);
    let expect_pos = start + text.len();
    let state = YankArgState {
        back,
        inserted_chars: text.chars().count(),
        expect_line,
        expect_pos,
    };
    let cmd = if replace_chars == 0 {
        YankArgCmd::Insert(text)
    } else {
        YankArgCmd::Replace {
            chars_back: replace_chars,
            text,
        }
    };
    Some((state, cmd))
}

/// The `Alt-.`/`Alt-_` handler rustyline calls: [`yank_last_arg_step`]
/// with its state held across keystrokes and the history shared with the
/// prompt loop.
pub struct YankLastArg {
    store: Arc<Mutex<HistoryStore>>,
    state: Mutex<Option<YankArgState>>,
}

impl YankLastArg {
    /// A handler over the loop's shared history.
    #[must_use]
    pub fn new(store: Arc<Mutex<HistoryStore>>) -> YankLastArg {
        YankLastArg {
            store,
            state: Mutex::new(None),
        }
    }
}

impl ConditionalEventHandler for YankLastArg {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext<'_>,
    ) -> Option<Cmd> {
        let store = self.store.lock().ok()?;
        let mut state = self.state.lock().ok()?;
        match yank_last_arg_step(state.as_ref(), ctx.line(), ctx.pos(), &store) {
            None => Some(Cmd::Noop),
            Some((next, cmd)) => {
                *state = Some(next);
                Some(match cmd {
                    YankArgCmd::Insert(text) => Cmd::Insert(1, text),
                    YankArgCmd::Replace { chars_back, text } => Cmd::Replace(
                        // RepeatCount is u16; a yanked argument never
                        // approaches that, but saturate rather than panic.
                        Movement::BackwardChar(
                            RepeatCount::try_from(chars_back).unwrap_or(RepeatCount::MAX),
                        ),
                        Some(text),
                    ),
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// the rustyline Helper
// ---------------------------------------------------------------------------

/// The editor's helper: completion over the three sources, validation over
/// the lexer's continuation predicate, no hinting, no highlighting (named
/// residue — is25 explicitly leaves color out).
pub struct EditHelper {
    names: Vec<String>,
    anchors: Vec<String>,
}

impl EditHelper {
    /// A helper with the (static) `:rules` anchor candidates prebuilt.
    #[must_use]
    pub fn new() -> EditHelper {
        EditHelper {
            names: Vec::new(),
            anchors: rules_anchor_candidates(),
        }
    }

    /// Refreshes the session-name source (called before every read, so the
    /// completer always sees the world the last input left).
    pub fn set_names(&mut self, names: Vec<String>) {
        self.names = names;
    }
}

impl Default for EditHelper {
    fn default() -> EditHelper {
        EditHelper::new()
    }
}

impl Completer for EditHelper {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        Ok(complete_input(
            line,
            pos,
            &self.names,
            &self.anchors,
            &fs_path_candidates,
        ))
    }
}

impl Hinter for EditHelper {
    type Hint = String;
}

impl Highlighter for EditHelper {}

impl Validator for EditHelper {
    fn validate(&self, ctx: &mut ValidationContext<'_>) -> rustyline::Result<ValidationResult> {
        Ok(if editor_input_complete(ctx.input()) {
            ValidationResult::Valid(None)
        } else {
            ValidationResult::Incomplete
        })
    }
}

impl rustyline::Helper for EditHelper {}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_paths(_prefix: &str) -> Vec<String> {
        Vec::new()
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    // -- the validator over the lexer's own predicate -----------------------

    #[test]
    fn complete_and_incomplete_buffers_follow_the_lexer() {
        assert!(editor_input_complete("1 + 1"));
        assert!(editor_input_complete("let x = 2"));
        assert!(!editor_input_complete("fn f() {"), "open brace continues");
        assert!(!editor_input_complete("1 +"), "trailing operator continues");
        assert!(
            editor_input_complete("fn f() {\n    1\n}"),
            "the closed construct completes"
        );
        assert!(
            editor_input_complete("wolf jumped"),
            "input the parser will reject is still COMPLETE — evaluate and \
             report, never trap in a continuation (lex.rs's own doctrine)"
        );
    }

    #[test]
    fn a_directive_line_is_always_complete() {
        // Holding `:quit` open IS the #46 trap; the editor never does.
        assert!(editor_input_complete(":quit"));
        assert!(editor_input_complete("  :help"));
        assert!(editor_input_complete(":load some(file.lu"));
    }

    // -- completion: the directive source -----------------------------------

    #[test]
    fn the_directive_word_completes_with_its_colon() {
        let (start, items) = complete_input(":he", 3, &[], &[], &no_paths);
        assert_eq!(start, 0);
        assert_eq!(items, vec![":help".to_owned()]);
    }

    #[test]
    fn an_ambiguous_directive_prefix_lists_every_candidate() {
        let (_, items) = complete_input(":re", 3, &[], &[], &no_paths);
        assert_eq!(items, vec![":regions".to_owned(), ":reset".to_owned()]);
        let (_, items) = complete_input(":q", 2, &[], &[], &no_paths);
        assert_eq!(items, vec![":q".to_owned(), ":quit".to_owned()]);
    }

    #[test]
    fn trace_subcommands_complete() {
        let (start, items) = complete_input(":trace o", 8, &[], &[], &no_paths);
        assert_eq!(start, 7);
        assert_eq!(items, vec!["off".to_owned(), "on".to_owned()]);
        let (_, items) = complete_input(":trace s", 8, &[], &[], &no_paths);
        assert_eq!(items, vec!["show".to_owned()]);
    }

    #[test]
    fn rules_prefixes_complete_from_the_registry_candidates() {
        let anchors = names(&["conc", "conc.deadlock.def", "mem", "mem.tier0.move.1"]);
        let (start, items) = complete_input(":rules co", 9, &[], &anchors, &no_paths);
        assert_eq!(start, 7);
        assert_eq!(
            items,
            vec!["conc".to_owned(), "conc.deadlock.def".to_owned()]
        );
    }

    #[test]
    fn the_real_registry_yields_anchor_and_namespace_candidates() {
        let anchors = rules_anchor_candidates();
        assert!(anchors.iter().any(|a| a == "conc"), "namespace prefixes");
        assert!(
            anchors.iter().any(|a| a.starts_with("conc.")),
            "full anchors"
        );
    }

    #[test]
    fn load_paths_come_from_the_injected_lister() {
        let lister = |prefix: &str| {
            assert_eq!(prefix, "exam");
            vec!["examples/".to_owned()]
        };
        let (start, items) = complete_input(":load exam", 10, &[], &[], &lister);
        assert_eq!(start, 6);
        assert_eq!(items, vec!["examples/".to_owned()]);
    }

    #[test]
    fn fs_candidates_list_the_repo_docs_dir() {
        // Runs from the crate root under `cargo test`.
        let items = fs_path_candidates("doc");
        assert!(items.contains(&"docs/".to_owned()), "{items:?}");
        let items = fs_path_candidates("docs/repl");
        assert!(items.contains(&"docs/repl.md".to_owned()), "{items:?}");
    }

    // -- completion: the session-name source --------------------------------

    #[test]
    fn session_names_complete_at_program_text() {
        let bound = names(&["alpha", "alphabet", "beta"]);
        let (start, items) = complete_input("1 + alp", 7, &bound, &[], &no_paths);
        assert_eq!(start, 4);
        assert_eq!(items, vec!["alpha".to_owned(), "alphabet".to_owned()]);
    }

    #[test]
    fn generational_internals_are_never_offered() {
        // [repl.def.shadow]: the session holds `f#1`/`f#2` internally; the
        // completer offers the SURFACE name only.
        let bound = names(&["f", "f#1", "f#2"]);
        let (_, items) = complete_input("f", 1, &bound, &[], &no_paths);
        assert_eq!(items, vec!["f".to_owned()]);
        assert!(is_surface_name("f"));
        assert!(!is_surface_name("f#2"));
    }

    #[test]
    fn type_arguments_complete_session_names() {
        let bound = names(&["config", "cfg"]);
        let (start, items) = complete_input(":type cf", 8, &bound, &[], &no_paths);
        assert_eq!(start, 6);
        assert_eq!(items, vec!["cfg".to_owned()]);
    }

    #[test]
    fn an_unknown_word_offers_nothing() {
        let (_, items) = complete_input("zzz", 3, &names(&["alpha"]), &[], &no_paths);
        assert!(items.is_empty());
    }

    // -- history policy ------------------------------------------------------

    #[test]
    fn empty_and_consecutive_duplicate_entries_are_not_recorded() {
        let mut store = HistoryStore::new(10);
        assert!(!store.push(""));
        assert!(!store.push("   "));
        assert!(store.push("let x = 1"));
        assert!(!store.push("let x = 1"), "consecutive duplicate");
        assert!(store.push("let y = 2"));
        assert!(
            store.push("let x = 1"),
            "a NON-consecutive repeat records (readline behavior)"
        );
        assert_eq!(store.entries().len(), 3);
    }

    #[test]
    fn the_cap_evicts_oldest_first() {
        let mut store = HistoryStore::new(3);
        for n in 0..5 {
            assert!(store.push(&format!("entry {n}")));
        }
        assert_eq!(
            store.entries(),
            &[
                "entry 2".to_owned(),
                "entry 3".to_owned(),
                "entry 4".to_owned()
            ]
        );
    }

    #[test]
    fn a_multi_line_entry_survives_the_disk_round_trip_as_one_item() {
        let mut store = HistoryStore::new(10);
        store.push("fn double(x: int) -> int {\n    x * 2\n}");
        store.push("double(21)");
        let reread = HistoryStore::parse(&store.serialize(), 10);
        assert_eq!(reread, store);
        assert_eq!(reread.entries().len(), 2, "two items, not four lines");
        assert!(reread.entries()[0].contains('\n'), "newlines intact");
    }

    #[test]
    fn corrupt_history_lines_are_skipped_not_fatal() {
        let text = "\"good\"\nnot json at all\n\"also good\"\n";
        let store = HistoryStore::parse(text, 10);
        assert_eq!(
            store.entries(),
            &["good".to_owned(), "also good".to_owned()]
        );
    }

    // -- the platform-correct history location ------------------------------

    #[test]
    fn the_override_wins_and_empty_disables() {
        assert_eq!(
            resolve_history_path(Some("/tmp/h"), false, None, None, Some("/home/u")),
            Some(PathBuf::from("/tmp/h"))
        );
        assert_eq!(
            resolve_history_path(Some(""), false, None, None, Some("/home/u")),
            None
        );
    }

    #[test]
    fn unix_resolves_xdg_state_then_home() {
        assert_eq!(
            resolve_history_path(None, false, None, Some("/xdg/state"), Some("/home/u")),
            Some(PathBuf::from("/xdg/state/lupin/history"))
        );
        assert_eq!(
            resolve_history_path(None, false, None, None, Some("/home/u")),
            Some(PathBuf::from("/home/u/.local/state/lupin/history"))
        );
        assert_eq!(resolve_history_path(None, false, None, None, None), None);
    }

    #[test]
    fn windows_resolves_appdata_shaped() {
        let path = resolve_history_path(None, true, Some("C:/Users/u/AppData/Roaming"), None, None)
            .expect("appdata resolves");
        let text = path.to_string_lossy().replace('\\', "/");
        assert_eq!(text, "C:/Users/u/AppData/Roaming/lupin/history");
        assert_eq!(
            resolve_history_path(None, true, None, Some("/xdg"), Some("/home/u")),
            None,
            "no unixism leaks onto windows"
        );
    }

    // -- yank-last-arg, cycling ---------------------------------------------

    fn seeded_store() -> HistoryStore {
        let mut store = HistoryStore::new(10);
        store.push(":schedule 111");
        store.push("let x = 42");
        store.push("f(1, 99)");
        store
    }

    #[test]
    fn a_fresh_yank_inserts_the_newest_last_arg() {
        let store = seeded_store();
        let (state, cmd) = yank_last_arg_step(None, "", 0, &store).expect("history is non-empty");
        assert_eq!(cmd, YankArgCmd::Insert("99)".to_owned()));
        assert_eq!(state.back, 0);
        assert_eq!(state.expect_line, "99)");
        assert_eq!(state.expect_pos, 3);
    }

    #[test]
    fn a_repeat_cycles_to_older_entries_and_wraps() {
        let store = seeded_store();
        let (s1, _) = yank_last_arg_step(None, "", 0, &store).unwrap();
        let (s2, cmd2) =
            yank_last_arg_step(Some(&s1), &s1.expect_line, s1.expect_pos, &store).unwrap();
        assert_eq!(
            cmd2,
            YankArgCmd::Replace {
                chars_back: 3,
                text: "42".to_owned()
            },
            "the second press REPLACES the first yank with the older arg"
        );
        let (s3, cmd3) =
            yank_last_arg_step(Some(&s2), &s2.expect_line, s2.expect_pos, &store).unwrap();
        assert_eq!(
            cmd3,
            YankArgCmd::Replace {
                chars_back: 2,
                text: "111".to_owned()
            }
        );
        let (_s4, cmd4) =
            yank_last_arg_step(Some(&s3), &s3.expect_line, s3.expect_pos, &store).unwrap();
        assert_eq!(
            cmd4,
            YankArgCmd::Replace {
                chars_back: 3,
                text: "99)".to_owned()
            },
            "the cycle wraps back to the newest entry"
        );
    }

    #[test]
    fn an_intervening_edit_starts_a_fresh_cycle() {
        let store = seeded_store();
        let (s1, _) = yank_last_arg_step(None, "", 0, &store).unwrap();
        // The user typed something after the yank: the buffer no longer
        // matches, so the next press is a fresh yank of the newest arg.
        let (s2, cmd) = yank_last_arg_step(Some(&s1), "99) + 1", 7, &store).unwrap();
        assert_eq!(cmd, YankArgCmd::Insert("99)".to_owned()));
        assert_eq!(s2.back, 0);
    }

    #[test]
    fn the_prefix_survives_a_cycle() {
        let store = seeded_store();
        let (s1, _) = yank_last_arg_step(None, "print(", 6, &store).unwrap();
        assert_eq!(s1.expect_line, "print(99)");
        let (s2, _) =
            yank_last_arg_step(Some(&s1), &s1.expect_line, s1.expect_pos, &store).unwrap();
        assert_eq!(s2.expect_line, "print(42", "only the yanked span cycles");
    }

    #[test]
    fn a_multi_line_entry_yields_its_final_word() {
        let mut store = HistoryStore::new(10);
        store.push("fn g() {\n    7\n}");
        assert_eq!(store.last_arg(0), Some("}".to_owned()));
    }

    #[test]
    fn yank_on_empty_history_is_a_noop() {
        let store = HistoryStore::new(10);
        assert!(yank_last_arg_step(None, "", 0, &store).is_none());
    }
}
