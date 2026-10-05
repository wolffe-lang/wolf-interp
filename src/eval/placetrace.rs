//! The place trace (is72): `lupin --trace-places`.
//!
//! After every statement — and after a function body's tail expression — the
//! machine writes one JSON line naming the state of every place in the
//! executing frame: each binding, and below it every struct field and tuple
//! element, and every list element or map value the program touched or that
//! is not live. The schema is versioned (`"trace":1`) and documented in
//! `docs/manual/06-place-trace.md`.
//!
//! # One source of truth
//!
//! A place's `state` is read from its [`SlotState`] — the very field
//! `read_claim_checked` and `move_path` consult before they trap
//! `use-after-move` — so the trace and the trap cannot disagree. What the
//! slot does not carry (which spelling moved it, where it was re-initialized,
//! that a `Copy` read copied it) is kept here, in side records that exist
//! only while tracing.
//!
//! # Never a behaviour
//!
//! Nothing here is consulted by the evaluator. The hooks only append to the
//! side records, and the walk that renders a line reads the frames without
//! `resolve` (whose `Arc::make_mut` would diverge a shared list). A write
//! to the sink that fails is dropped: the trace is a side channel and must
//! never decide an exit code. `tests/place_trace.rs` and
//! `ci/place-trace-identity.sh` hold the corpus to that.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io::Write;
use std::sync::Mutex;

use super::place::{MapKey, Path, Proj};
use super::value::{Slot, SlotState, Value};
use super::{EResult, Machine, Signal};
use crate::diag::Span;

/// The schema version every line carries as `"trace"`.
pub const TRACE_VERSION: u32 = 1;

/// The spelling that moved a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveBy {
    /// A `take` argument or a `take` store (`f(take p.x)`, `xs[i] = take v`),
    /// or a `take self` receiver.
    Take,
    /// The `move` expression.
    Move,
    /// A plain initializer or store of a non-`Copy` value (`let q = p`).
    Plain,
    /// A `match` arm that bound a non-`Copy` part of its scrutinee.
    Match,
    /// `freeze r`, which consumes its region value.
    Freeze,
    /// A `mut` argument (or `mut self` receiver) the callee moved out and
    /// never stored back.
    Mut,
}

impl MoveBy {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MoveBy::Take => "take",
            MoveBy::Move => "move",
            MoveBy::Plain => "plain",
            MoveBy::Match => "match",
            MoveBy::Freeze => "freeze",
            MoveBy::Mut => "mut",
        }
    }
}

/// How a `Copy` read was spelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyBy {
    /// A `Copy` value read where a non-`Copy` one would have moved.
    Plain,
    /// The `copy x` expression.
    Copy,
}

impl CopyBy {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            CopyBy::Plain => "plain",
            CopyBy::Copy => "copy",
        }
    }
}

/// What a statement did to one of its frame's places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Move(MoveBy),
    Reinit,
    Copy(CopyBy),
}

/// One event, as the machine recorded it. `frame` is the path's frame index:
/// a line reports only the events on its own frame's places.
#[derive(Debug, Clone)]
pub struct Event {
    pub kind: EventKind,
    pub frame: usize,
    pub path: String,
    pub at: Span,
    /// The binding or place the value went to, when the read was a
    /// binding's or a store's whole right-hand side.
    pub to: Option<String>,
}

/// How many elements of one container a line lists, at most. A loop that
/// addresses every element of a 65,536-element list would otherwise write
/// the whole list after every statement: quadratic output, and
/// `memory/byte_list_ledger.lu` ran past a 120-second timeout with the
/// trace on (ci/place-trace-identity.sh, kasumi, 2026-10-05).
pub const MAX_ELEMENTS: usize = 16;

/// One addressed element of a container, ordered so a line lists the
/// lowest indices (or keys) first.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Elem {
    Index(i128),
    /// A map key: its kind's rank, then its value, so `int` keys sort as
    /// numbers.
    Key(u8, i128, String),
}

impl Elem {
    fn of(step: &Proj) -> Option<Elem> {
        match step {
            Proj::Index(i) => Some(Elem::Index(*i)),
            Proj::Key(MapKey::Bool(b)) => Some(Elem::Key(0, i128::from(*b), String::new())),
            Proj::Key(MapKey::Int(i)) => Some(Elem::Key(1, *i, String::new())),
            Proj::Key(MapKey::Char(c)) => {
                Some(Elem::Key(2, i128::from(u32::from(*c)), String::new()))
            }
            Proj::Key(MapKey::Str(text)) => Some(Elem::Key(3, 0, text.clone())),
            _ => None,
        }
    }

    fn names(&self, key: &MapKey) -> bool {
        Elem::of(&Proj::Key(key.clone())).as_ref() == Some(self)
    }
}

/// A place's identity across statements: the task, the activation's
/// never-reused frame serial, and the path's spelling.
type Key = (usize, u64, String);

#[derive(Default)]
struct Records {
    /// The last move of each place, with its spelling.
    moves: BTreeMap<Key, (MoveBy, Span)>,
    /// Every move site's spelling: the fallback for a moved slot that
    /// travelled inside a value away from the place that moved it.
    by_span: HashMap<(usize, usize), MoveBy>,
    /// Places live again after a move, and where.
    reinit: BTreeMap<Key, Span>,
    /// List elements and map values the program addressed (moving one
    /// addresses it), by container.
    touched: BTreeMap<Key, BTreeSet<Elem>>,
    /// Source texts by file name, read once, for `line:col`.
    texts: BTreeMap<String, Option<String>>,
}

/// The tracer one run shares across its tasks.
pub struct PlaceTrace {
    sink: Mutex<Box<dyn Write + Send>>,
    records: Mutex<Records>,
    /// The entry file's name as the loader knows it, and its text.
    entry: (String, String),
}

impl std::fmt::Debug for PlaceTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PlaceTrace")
    }
}

impl PlaceTrace {
    /// A tracer writing to `sink`, resolving positions in the entry file
    /// against `entry_text` (other files are read from disk on demand).
    #[must_use]
    pub fn new(sink: Box<dyn Write + Send>, entry_text: String) -> PlaceTrace {
        PlaceTrace {
            sink: Mutex::new(sink),
            records: Mutex::new(Records::default()),
            entry: (String::new(), entry_text),
        }
    }

    fn records(&self) -> std::sync::MutexGuard<'_, Records> {
        self.records
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn set_entry_name(&mut self, name: &str) {
        name.clone_into(&mut self.entry.0);
    }

    pub(super) fn note_move(&self, key: Key, by: MoveBy, at: Span) {
        let mut records = self.records();
        records.by_span.insert((at.start, at.end), by);
        records.reinit.remove(&key);
        records.moves.insert(key, (by, at));
    }

    /// A write landed on `key`: its descendants hold the new value's parts,
    /// so nothing recorded about the old ones applies. `reinit` is the
    /// write's site when the place was moved before it.
    pub(super) fn note_write(&self, key: Key, reinit: Option<Span>) {
        let mut records = self.records();
        forget_below(&mut records.moves, &key);
        forget_below(&mut records.reinit, &key);
        match reinit {
            Some(at) => {
                records.reinit.insert(key, at);
            }
            None => {
                records.reinit.remove(&key);
            }
        }
    }

    /// A fresh binding named `key` — a shadowing `let` reuses a name in the
    /// same frame, and nothing recorded about the old binding is the new
    /// one's.
    pub(super) fn note_declare(&self, key: &Key) {
        let mut records = self.records();
        records.moves.remove(key);
        records.reinit.remove(key);
        records.touched.remove(key);
        forget_below(&mut records.moves, key);
        forget_below(&mut records.reinit, key);
        forget_below(&mut records.touched, key);
    }

    /// `step` of the container `container` was addressed.
    pub(super) fn note_touch(&self, container: Key, step: &Proj) {
        if let Some(elem) = Elem::of(step) {
            self.records()
                .touched
                .entry(container)
                .or_default()
                .insert(elem);
        }
    }

    /// Writes one line. A failed write is dropped (a side channel never
    /// decides an outcome).
    pub(super) fn emit(&self, line: &str) {
        let mut sink = self
            .sink
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = sink.write_all(line.as_bytes());
        let _ = sink.write_all(b"\n");
        let _ = sink.flush();
    }

    /// `line:col` of `offset` in `file` (the entry when `None`), or
    /// `@offset` when the file cannot be read.
    pub(super) fn position(&self, file: Option<&str>, offset: usize) -> String {
        let file = file.filter(|name| *name != self.entry.0);
        let Some(name) = file else {
            let (line, col) = crate::diag::line_col(&self.entry.1, offset);
            return format!("{line}:{col}");
        };
        let mut records = self.records();
        let text = records
            .texts
            .entry(name.to_owned())
            .or_insert_with(|| std::fs::read_to_string(name).ok());
        match text {
            Some(text) => {
                let (line, col) = crate::diag::line_col(text, offset);
                format!("{line}:{col}")
            }
            None => format!("@{offset}"),
        }
    }
}

/// Removes every key strictly below `key` (`p.x`, `p[0]` under `p`).
fn forget_below<V>(map: &mut impl Forget<V>, key: &Key) {
    map.forget_below(key);
}

trait Forget<V> {
    fn forget_below(&mut self, key: &Key);
}

fn is_below(candidate: &Key, key: &Key) -> bool {
    candidate.0 == key.0
        && candidate.1 == key.1
        && candidate.2.len() > key.2.len()
        && candidate.2.starts_with(key.2.as_str())
        && matches!(candidate.2.as_bytes()[key.2.len()], b'.' | b'[')
}

impl<V> Forget<V> for BTreeMap<Key, V> {
    fn forget_below(&mut self, key: &Key) {
        self.retain(|candidate, _| !is_below(candidate, key));
    }
}

/// A JSON string literal.
pub(super) fn json_str(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

/// The type word a place reports.
fn type_name(value: &Value) -> String {
    match value {
        Value::Tuple(_) => "tuple".to_owned(),
        Value::Range { .. } => "range".to_owned(),
        Value::Fn(_) => "fn".to_owned(),
        Value::Closure(_) => "closure".to_owned(),
        Value::Error(e) => e.tag.clone(),
        Value::Module(_) => "module".to_owned(),
        Value::Builtin(_) => "builtin".to_owned(),
        Value::Scope(_) => "scope".to_owned(),
        Value::Proc(_) => "proc".to_owned(),
        Value::Duration(_) => "duration".to_owned(),
        other => other.kind(),
    }
}

/// The printable form a live scalar reports; `None` for an aggregate or a
/// granule, which report their type (or kind) alone.
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::Unit
        | Value::Bool(_)
        | Value::Int(..)
        | Value::Float(_)
        | Value::Char(_)
        | Value::Byte(_)
        | Value::Str(_) => Some(value.to_string()),
        _ => None,
    }
}

/// What the walk needs from the machine to render one frame.
pub(super) struct FrameView<'a> {
    pub task: usize,
    pub serial: u64,
    pub file: Option<&'a str>,
}

/// Renders every place of one binding into `out`, depth-first.
pub(super) fn render_binding(
    trace: &PlaceTrace,
    view: &FrameView<'_>,
    name: &str,
    slot: &Slot,
    out: &mut Vec<String>,
) {
    let records = trace.records();
    let mut lines = Vec::new();
    walk(
        &records,
        view,
        &Path::local(0, name),
        slot,
        None,
        &mut lines,
    );
    drop(records);
    for line in lines {
        out.push(line.finish(trace, view));
    }
}

/// One place's facts, positions still as spans until the records lock is
/// released (`position` takes it again for the text cache).
struct Pending {
    path: String,
    state: &'static str,
    ty: String,
    value: Option<String>,
    by: Option<MoveBy>,
    at: Option<Span>,
    of: Option<String>,
    reinit: Option<Span>,
    /// A `List`'s or `Map`'s element count, and how many addressed
    /// elements past [`MAX_ELEMENTS`] the line leaves out.
    len: Option<usize>,
    elided: usize,
}

impl Pending {
    fn finish(self, trace: &PlaceTrace, view: &FrameView<'_>) -> String {
        let mut line = format!(
            "{{\"path\":{},\"state\":\"{}\",\"type\":{}",
            json_str(&self.path),
            self.state,
            json_str(&self.ty)
        );
        if let Some(value) = &self.value {
            let _ = write!(line, ",\"value\":{}", json_str(value));
        }
        if let Some(len) = self.len {
            let _ = write!(line, ",\"len\":{len}");
        }
        if self.elided > 0 {
            let _ = write!(line, ",\"elided\":{}", self.elided);
        }
        if let Some(of) = &self.of {
            let _ = write!(line, ",\"of\":{}", json_str(of));
        }
        if let Some(by) = self.by {
            let _ = write!(line, ",\"by\":\"{}\"", by.as_str());
        }
        if let Some(at) = self.at {
            let _ = write!(line, ",\"at\":\"{}\"", trace.position(view.file, at.start));
        }
        if let Some(at) = self.reinit {
            let _ = write!(
                line,
                ",\"reinit\":\"{}\"",
                trace.position(view.file, at.start)
            );
        }
        line.push('}');
        line
    }
}

fn walk(
    records: &Records,
    view: &FrameView<'_>,
    path: &Path,
    slot: &Slot,
    moved_above: Option<(&str, Span)>,
    out: &mut Vec<Pending>,
) {
    let spelled = path.to_string();
    let key: Key = (view.task, view.serial, spelled.clone());
    let ty = type_name(&slot.value);
    let pending = match (moved_above, slot.state) {
        (Some((of, at)), _) => Pending {
            path: spelled.clone(),
            state: "uninit",
            ty,
            value: None,
            by: None,
            at: Some(at),
            of: Some(of.to_owned()),
            reinit: None,
            len: None,
            elided: 0,
        },
        (None, SlotState::Moved(at)) => {
            let by = match records.moves.get(&key) {
                Some((by, recorded)) if *recorded == at => Some(*by),
                _ => records.by_span.get(&(at.start, at.end)).copied(),
            };
            Pending {
                path: spelled.clone(),
                state: "moved",
                ty,
                value: None,
                by,
                at: Some(at),
                of: None,
                reinit: None,
                len: None,
                elided: 0,
            }
        }
        (None, SlotState::Live) => Pending {
            path: spelled.clone(),
            state: "live",
            ty,
            value: scalar_text(&slot.value),
            by: None,
            at: None,
            of: None,
            reinit: records.reinit.get(&key).copied(),
            len: None,
            elided: 0,
        },
    };
    let mut pending = pending;
    let addressed = records.touched.get(&key);
    let mut listed: Vec<&Elem> = Vec::new();
    if let Value::List(..) | Value::Map(_) = &slot.value {
        let count = match &slot.value {
            Value::List(items, _, _) => items.len(),
            Value::Map(pairs) => pairs.len(),
            _ => 0,
        };
        pending.len = Some(count);
        if let Some(addressed) = addressed {
            listed = addressed.iter().take(MAX_ELEMENTS).collect();
            pending.elided = addressed.len().saturating_sub(MAX_ELEMENTS);
        }
    }
    out.push(pending);
    // Below a moved place every part is uninitialized: its storage went
    // with the move, and `resolve` names this place as the move site.
    let below: Option<(&str, Span)> = match (moved_above, slot.state) {
        (Some(above), _) => Some(above),
        (None, SlotState::Moved(at)) => Some((spelled.as_str(), at)),
        (None, SlotState::Live) => None,
    };
    match &slot.value {
        Value::Struct { fields, .. } => {
            for (name, field) in fields {
                let child = path.clone().project(Proj::Field(name.clone()));
                walk(records, view, &child, field, below, out);
            }
        }
        Value::Tuple(items) => {
            for (index, item) in items.iter().enumerate() {
                let child = path
                    .clone()
                    .project(Proj::Index(i128::try_from(index).unwrap_or(i128::MAX)));
                walk(records, view, &child, item, below, out);
            }
        }
        // Only the addressed elements, lowest first and at most
        // MAX_ELEMENTS: every way an element stops being live (a move, a
        // `mut` write-back) addresses it, so no element walk is needed.
        Value::List(items, _, _) => {
            for elem in listed {
                let Elem::Index(index) = elem else { continue };
                let Some(item) = usize::try_from(*index).ok().and_then(|i| items.get(i)) else {
                    continue;
                };
                let child = path.clone().project(Proj::Index(*index));
                walk(records, view, &child, item, below, out);
            }
        }
        Value::Map(pairs) => {
            for elem in listed {
                let Some((map_key, value)) = pairs.iter().find_map(|(key, value)| {
                    MapKey::of(key)
                        .filter(|map_key| elem.names(map_key))
                        .map(|map_key| (map_key, value))
                }) else {
                    continue;
                };
                let child = path.clone().project(Proj::Key(map_key));
                walk(records, view, &child, value, below, out);
            }
        }
        _ => {}
    }
}

/// Renders one event.
pub(super) fn render_event(trace: &PlaceTrace, file: Option<&str>, event: &Event) -> String {
    let mut line = String::from("{");
    match &event.kind {
        EventKind::Move(by) => {
            let _ = write!(line, "\"ev\":\"move\",\"path\":{}", json_str(&event.path));
            let _ = write!(line, ",\"by\":\"{}\"", by.as_str());
        }
        EventKind::Reinit => {
            let _ = write!(line, "\"ev\":\"reinit\",\"path\":{}", json_str(&event.path));
        }
        EventKind::Copy(by) => {
            let _ = write!(line, "\"ev\":\"copy\",\"path\":{}", json_str(&event.path));
            let _ = write!(line, ",\"by\":\"{}\"", by.as_str());
        }
    }
    let _ = write!(line, ",\"at\":\"{}\"", trace.position(file, event.at.start));
    if let Some(to) = &event.to {
        let _ = write!(line, ",\"to\":{}", json_str(to));
    }
    line.push('}');
    line
}

// -- the machine's side of the trace -----------------------------------------
//
// Every hook below starts by asking whether a tracer exists and returns at
// once when none does, so a run without `--trace-places` pays one `Option`
// test per hook and records nothing.

impl Machine {
    /// The side-record key of `path`: `None` for a global (item-level
    /// bindings are not in any frame) or a path whose frame has gone.
    fn trace_key(&self, path: &Path) -> Option<Key> {
        if path.frame == usize::MAX {
            return None;
        }
        let frame = self.frames.get(path.frame)?;
        Some((self.task, frame.serial, path.to_string()))
    }

    fn push_place_event(&mut self, kind: EventKind, path: &Path, at: Span) {
        if path.frame == usize::MAX {
            return;
        }
        self.place_events.push(Event {
            kind,
            frame: path.frame,
            path: path.to_string(),
            at,
            to: None,
        });
    }

    /// `path` was moved out at `at`, spelled `by`.
    pub(super) fn trace_move(&mut self, path: &Path, by: MoveBy, at: Span) {
        let Some(trace) = self.shared.place_trace.clone() else {
            return;
        };
        self.trace_touch(path);
        if let Some(key) = self.trace_key(path) {
            trace.note_move(key, by, at);
        }
        self.push_place_event(EventKind::Move(by), path, at);
    }

    /// `path` was written at `at`; `was_moved` says the slot was moved out
    /// (or never live) just before, which makes the write a
    /// re-initialization (`[mem.tier0.move.4]`).
    pub(super) fn trace_write(&mut self, path: &Path, was_moved: bool, at: Span) {
        let Some(trace) = self.shared.place_trace.clone() else {
            return;
        };
        self.trace_touch(path);
        if let Some(key) = self.trace_key(path) {
            trace.note_write(key, was_moved.then_some(at));
        }
        if was_moved {
            self.push_place_event(EventKind::Reinit, path, at);
        }
    }

    /// A `Copy` value was read out of `path` where a move would otherwise
    /// have happened (or by `copy x`): the place stays live.
    pub(super) fn trace_copy(&mut self, path: &Path, by: CopyBy, at: Span) {
        if self.shared.place_trace.is_none() {
            return;
        }
        self.trace_touch(path);
        self.push_place_event(EventKind::Copy(by), path, at);
    }

    /// Records the element and map-value places along `path`, so a line
    /// shows the parts of a container the program actually addressed.
    pub(super) fn trace_touch(&self, path: &Path) {
        let Some(trace) = &self.shared.place_trace else {
            return;
        };
        if path.frame == usize::MAX
            || !path
                .projections
                .iter()
                .any(|step| matches!(step, Proj::Index(_) | Proj::Key(_)))
        {
            return;
        }
        let Some(frame) = self.frames.get(path.frame) else {
            return;
        };
        let mut prefix = Path::local(path.frame, path.base.clone());
        for step in &path.projections {
            if matches!(step, Proj::Index(_) | Proj::Key(_)) {
                trace.note_touch((self.task, frame.serial, prefix.to_string()), step);
            }
            prefix = prefix.project(step.clone());
        }
    }

    /// A binding named `name` was just declared in the current frame.
    pub(super) fn trace_declare(&self, name: &str) {
        let Some(trace) = &self.shared.place_trace else {
            return;
        };
        if let Some(frame) = self.frames.last() {
            trace.note_declare(&(self.task, frame.serial, name.to_owned()));
        }
    }

    /// Writes the line for one finished statement (or fn-body tail) of
    /// frame `frame`: its places, and the events since `mark` that touched
    /// them. `to` names the binding a statement's initializer went to,
    /// with the initializer's span.
    pub(super) fn trace_line<T>(
        &mut self,
        mark: usize,
        frame: usize,
        at: Span,
        tail: bool,
        result: &EResult<T>,
        to: Option<(Span, &str)>,
    ) {
        let Some(trace) = self.shared.place_trace.clone() else {
            return;
        };
        let mut events: Vec<Event> = self
            .place_events
            .drain(mark.min(self.place_events.len())..)
            .filter(|event| event.frame == frame)
            .collect();
        let Some(activation) = self.frames.get(frame) else {
            return;
        };
        if let Some((span, name)) = to {
            for event in &mut events {
                if event.at == span && !matches!(event.kind, EventKind::Reinit) {
                    event.to = Some(name.to_owned());
                }
            }
        }
        let file = self
            .shared
            .program
            .modules
            .get(&activation.module)
            .and_then(|module| module.item_files.get(&activation.name))
            .map(String::as_str);
        let view = FrameView {
            task: self.task,
            serial: activation.serial,
            file,
        };
        // The bindings a read would find: the innermost scope's latest
        // entry for each name (`resolve`'s `rposition`), listed where that
        // binding was declared.
        let all: Vec<(&String, &Slot)> = activation
            .scopes
            .iter()
            .flat_map(|scope| scope.locals.iter().map(|(name, slot)| (name, slot)))
            .collect();
        let mut places = Vec::new();
        for (index, (name, slot)) in all.iter().enumerate() {
            if all[index + 1..].iter().any(|(later, _)| later == name) {
                continue;
            }
            render_binding(&trace, &view, name, slot, &mut places);
        }
        let fn_name = if activation.name.is_empty() {
            "<task>"
        } else {
            activation.name.as_str()
        };
        let mut line = format!(
            "{{\"trace\":{TRACE_VERSION},\"task\":{},\"fn\":{},\"depth\":{frame},\"at\":\"{}\"",
            self.task,
            json_str(fn_name),
            trace.position(file, at.start)
        );
        if tail {
            line.push_str(",\"tail\":true");
        }
        if let Err(Signal::Trap(fault)) = result {
            let _ = write!(line, ",\"trap\":\"{}\"", fault.kind);
        }
        line.push_str(",\"places\":[");
        line.push_str(&places.join(","));
        line.push_str("],\"events\":[");
        let rendered: Vec<String> = events
            .iter()
            .map(|event| render_event(&trace, file, event))
            .collect();
        line.push_str(&rendered.join(","));
        line.push_str("]}");
        trace.emit(&line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_is_a_strict_path_prefix() {
        let key = |p: &str| (0usize, 1u64, p.to_owned());
        assert!(is_below(&key("p.lead"), &key("p")));
        assert!(is_below(&key("p[0]"), &key("p")));
        assert!(is_below(&key("p.a.b"), &key("p.a")));
        assert!(!is_below(&key("p"), &key("p")));
        assert!(!is_below(&key("pq"), &key("p")));
        assert!(!is_below(&key("q.x"), &key("p")));
        assert!(!is_below(&(1, 1, "p.x".to_owned()), &key("p")));
        assert!(!is_below(&(0, 2, "p.x".to_owned()), &key("p")));
    }

    #[test]
    fn a_write_forgets_what_was_recorded_below_it() {
        let trace = PlaceTrace::new(Box::new(std::io::sink()), String::new());
        let key = |p: &str| (0usize, 1u64, p.to_owned());
        trace.note_move(key("p.lead"), MoveBy::Take, Span::new(1, 2));
        trace.note_write(key("p.tail"), Some(Span::new(3, 4)));
        trace.note_write(key("p"), None);
        let records = trace.records();
        assert!(records.moves.is_empty());
        assert!(records.reinit.is_empty());
        // The span fallback outlives the place: a moved slot can travel.
        assert_eq!(records.by_span.get(&(1, 2)), Some(&MoveBy::Take));
    }

    #[test]
    fn a_declaration_forgets_the_shadowed_binding() {
        let trace = PlaceTrace::new(Box::new(std::io::sink()), String::new());
        let key = |p: &str| (0usize, 1u64, p.to_owned());
        trace.note_write(key("x"), Some(Span::new(0, 1)));
        trace.note_touch(key("x"), &Proj::Index(3));
        trace.note_touch(key("x[0]"), &Proj::Index(1));
        trace.note_declare(&key("x"));
        let records = trace.records();
        assert!(records.reinit.is_empty());
        assert!(records.touched.is_empty());
    }

    #[test]
    fn scalars_print_and_aggregates_name_their_type() {
        assert_eq!(scalar_text(&Value::int(3)).as_deref(), Some("3"));
        assert_eq!(
            scalar_text(&Value::Str(crate::eval::value::Str::new("ada"))).as_deref(),
            Some("ada")
        );
        assert_eq!(scalar_text(&Value::Tuple(Vec::new())), None);
        assert_eq!(type_name(&Value::Tuple(Vec::new())), "tuple");
        assert_eq!(type_name(&Value::Bool(true)), "bool");
    }
}
