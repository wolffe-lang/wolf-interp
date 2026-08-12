//! The REPL session — is08's line REPL over the interpreter.
//!
//! One implicit module growing over time, with the incremental-definition
//! semantics written down as the `[repl.*]` notes in `docs/repl.md` rather
//! than left emergent (the sprint's Target 2):
//!
//! - **`[repl.def.shadow]`** — redefinition is shadowing, not mutation: a new
//!   `fn f` binds fresh under a new internal generation (`f#2`); closures and
//!   values that captured the old `f` keep it. Implemented by giving every
//!   prompt-defined function a generational internal name and binding the
//!   *surface* name to the current generation as a session local — closures
//!   capture locals by value (`[gram.expr.closure]`), so an old capture holds
//!   `f#1` forever while new mentions resolve to `f#2`.
//! - **`[repl.type.gen]`** — types are generational: redefining `Point`
//!   mints a new nominal type; existing values keep their old identity and
//!   print with a stale-generation marker (`Point#1`).
//! - **`[repl.type.mix]`** — mixing generations is a type error with a hint
//!   (see the `==`/`!=` guard in the evaluator).
//! - **`[repl.let.rebind]`** — `let` rebinding drops the old binding; owned
//!   resources (regions, `shared` counts) drop per normal rules, visible in
//!   `:mem` immediately.
//! - **`[repl.module]`** — D31/D32 module rules do NOT apply at the prompt;
//!   `:load` is textual inclusion into the implicit module, nothing more.
//! - **`[repl.trap.alive]`** — a trap or diagnostic prints (with its code
//!   and clause anchor) and the session survives: the world is whatever the
//!   fault left behind, which is itself a teaching surface (`:mem` shows it).
//!
//! Continuation is the lexer's own `[gram.lex.newline]` machinery, byte-exact
//! ([`crate::lex::repl_input_complete`]): a line that cannot terminate yet
//! continues.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::ast::{Item, ItemKind, PatKind, Pattern, Stmt, StmtKind};
use crate::sema::Def;

use super::value::{Slot, Value};
use super::{Frame, Machine, Scope, Signal, Trace};

/// The synthetic wrapper a prompt input is parsed inside. Every input shares
/// it, so every span in every rendered diagnostic is adjusted by the same
/// [`PREFIX`] length and transcripts stay deterministic.
const PREFIX: &str = "fn __repl__() -> int {\n";

/// How many trace events the `:trace` ring buffer keeps.
const TRACE_RING: usize = 4096;

/// The class of the first fault a session's inputs produced — `lupin eval`'s
/// door onto the documented process exit codes. The session itself survives
/// every one of these (`[repl.trap.alive]`); a one-shot `eval` reads the
/// class afterwards and exits accordingly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionFault {
    /// The input was rejected before it ran (a lex/parse diagnostic): exit 2.
    Static,
    /// A trap or a UB finding — the run reached a fault: exit 3.
    Dynamic,
    /// Outside this implementation's scope: exit 4.
    Unsupported,
}

/// What feeding one line did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fed {
    /// The input evaluated (or was a directive); these lines are its output.
    Output(Vec<String>),
    /// The line cannot terminate yet ([`crate::lex::repl_input_complete`]);
    /// the session is waiting for a continuation line.
    More,
    /// `:quit`.
    Quit,
}

/// A live REPL session: the dynamic machine's world, persistent across
/// inputs.
pub struct Session {
    machine: Machine,
    seed: Option<u64>,
    /// The buffered continuation lines of an incomplete input.
    pending: String,
    /// Per-name definition generation for prompt-defined functions
    /// (`[repl.def.shadow]`).
    fn_gens: BTreeMap<String, u32>,
    /// Per-name generation for prompt-defined nominal types
    /// (`[repl.type.gen]`). Mirrored into the machine's `repl_types` map so
    /// struct literals mint values of the current generation.
    type_gens: BTreeMap<String, u32>,
    /// The `:trace` ring buffer, drained from the machine after every input.
    trace: std::collections::VecDeque<String>,
    tracing: bool,
    /// The first [`SessionFault`] any input produced (`:reset` clears it with
    /// the rest of the world). The interactive loop never reads this; `lupin
    /// eval` does.
    fault: Option<SessionFault>,
}

impl Session {
    /// A fresh session: empty implicit module, fresh world, optional schedule
    /// seed (`[conc.det.seed]` — transcripts replay byte-identically).
    #[must_use]
    pub fn new(seed: Option<u64>) -> Session {
        let program = crate::sema::load_source("repl", "").expect("the empty program always loads");
        let mut machine = Machine::with_seed(&program, seed);
        machine.frames.push(Frame {
            module: String::new(),
            scopes: vec![Scope::default()],
            row: Vec::new(),
        });
        Session {
            machine,
            seed,
            pending: String::new(),
            fn_gens: BTreeMap::new(),
            type_gens: BTreeMap::new(),
            trace: std::collections::VecDeque::new(),
            tracing: false,
            fault: None,
        }
    }

    /// The class of the first fault any fed input produced, if any — the
    /// exit-code door for one-shot evaluation. The session survives faults
    /// (`[repl.trap.alive]`); this only reports that one happened.
    #[must_use]
    pub fn fault(&self) -> Option<SessionFault> {
        self.fault
    }

    fn note_fault(&mut self, fault: SessionFault) {
        self.fault.get_or_insert(fault);
    }

    /// The prompt for the next line: continuation shows as `....>`.
    #[must_use]
    pub fn prompt(&self) -> &'static str {
        if self.pending.is_empty() {
            "wolf> "
        } else {
            "....> "
        }
    }

    /// Feeds one line. Directives act immediately; program text accumulates
    /// until the lexer's termination rules say the input is complete.
    pub fn feed_line(&mut self, line: &str) -> Fed {
        if self.pending.is_empty() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(':') {
                return self.directive(rest);
            }
        }
        self.pending.push_str(line);
        self.pending.push('\n');
        if !crate::lex::repl_input_complete(&self.pending) {
            return Fed::More;
        }
        let input = std::mem::take(&mut self.pending);
        Fed::Output(self.eval_input(&input))
    }

    // -- directives ---------------------------------------------------------

    fn directive(&mut self, rest: &str) -> Fed {
        let mut parts = rest.splitn(2, char::is_whitespace);
        let head = parts.next().unwrap_or_default();
        let arg = parts.next().map(str::trim).unwrap_or_default();
        let out = match head {
            "quit" | "q" => return Fed::Quit,
            "help" => Session::help(),
            "reset" => {
                *self = Session::new(self.seed);
                vec!["session reset: fresh world, empty implicit module".to_owned()]
            }
            "type" if !arg.is_empty() => self.type_of(arg),
            "type" => vec!["usage: :type expr".to_owned()],
            "mem" => self.mem_dump(true),
            "regions" => self.mem_dump(false),
            "trace" => self.trace_directive(arg),
            "rules" => Session::rules(arg),
            "schedule" => self.schedule(arg),
            "load" if !arg.is_empty() => self.load(arg),
            "load" => vec!["usage: :load file.lu".to_owned()],
            other => vec![format!(
                "unknown directive `:{other}`; :help lists the surface"
            )],
        };
        Fed::Output(out)
    }

    fn help() -> Vec<String> {
        [
            ":type e            evaluate e, report its type",
            ":mem               the memory model, live: regions, loans, shared, pools, tasks",
            ":regions           the region tree alone",
            ":trace on|off      record rule firings (clause-cited) into the ring buffer",
            ":trace show [n]    show the last n recorded events (default 20)",
            ":trace clear       empty the ring buffer",
            ":rules [prefix]    the rule registry, optionally filtered by anchor prefix",
            ":schedule seed     re-seed the scheduler's decision stream from here on",
            ":load file.lu      textual inclusion into the implicit module ([repl.module])",
            ":reset             fresh world, empty module",
            ":quit              leave",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
    }

    fn trace_directive(&mut self, arg: &str) -> Vec<String> {
        let mut parts = arg.split_whitespace();
        match parts.next() {
            Some("on") => {
                self.tracing = true;
                self.machine.tracing = Trace::All;
                self.machine.shared.tracing = Trace::All;
                vec!["trace on: every rule firing is recorded with its clause anchor".to_owned()]
            }
            Some("off") => {
                self.tracing = false;
                self.machine.tracing = Trace::Off;
                self.machine.shared.tracing = Trace::Off;
                vec!["trace off".to_owned()]
            }
            Some("clear") => {
                self.trace.clear();
                vec!["trace cleared".to_owned()]
            }
            Some("show") | None => {
                let n = parts
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(20);
                let start = self.trace.len().saturating_sub(n);
                let mut out: Vec<String> = self.trace.iter().skip(start).cloned().collect();
                if out.is_empty() {
                    out.push(if self.tracing {
                        "trace: no events recorded yet".to_owned()
                    } else {
                        "trace is off (`:trace on` to record)".to_owned()
                    });
                }
                out
            }
            Some(other) => vec![format!("unknown `:trace {other}`; on|off|show [n]|clear")],
        }
    }

    fn rules(prefix: &str) -> Vec<String> {
        let rows = super::rules::registry();
        if prefix.is_empty() {
            let mut by_ns: BTreeMap<&str, usize> = BTreeMap::new();
            for row in &rows {
                let ns = row.anchor.split('.').next().unwrap_or_default();
                *by_ns.entry(ns).or_default() += 1;
            }
            let mut out = vec![format!(
                "{} rules, every one citing a clause anchor; :rules <prefix> to list",
                rows.len()
            )];
            for (ns, count) in by_ns {
                out.push(format!("  {ns}: {count}"));
            }
            return out;
        }
        let mut out: Vec<String> = rows
            .iter()
            .filter(|row| row.anchor.starts_with(prefix))
            .map(|row| format!("[{}] {:?}: {}", row.anchor, row.rule, row.description))
            .collect();
        if out.is_empty() {
            out.push(format!("no rules cite an anchor starting `{prefix}`"));
        }
        out
    }

    fn schedule(&mut self, arg: &str) -> Vec<String> {
        match arg.parse::<u64>() {
            Ok(seed) => {
                self.machine.shared.sched.reseed(seed);
                self.machine.drain_sched();
                vec![format!(
                    "scheduler re-seeded: seed={seed} governs every decision from here on \
                     ([conc.det.seed])"
                )]
            }
            Err(_) => vec!["usage: :schedule seed   (a u64)".to_owned()],
        }
    }

    fn load(&mut self, path: &str) -> Vec<String> {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(e) => return vec![format!("cannot read `{path}`: {e}")],
        };
        // [repl.module]: textual inclusion into the implicit module — the
        // file's items and statements evaluate as if typed at the prompt.
        let mut out = self.eval_input(&source);
        out.insert(0, format!("loaded `{path}`"));
        out
    }

    fn type_of(&mut self, expr: &str) -> Vec<String> {
        match self.eval_unit(expr) {
            Evaluated::Value(value) => vec![self.render_type(&value)],
            Evaluated::Lines(lines) => lines,
        }
    }

    // -- evaluation ---------------------------------------------------------

    /// Evaluates one complete input (one or more statements, the last
    /// expression printing as `value : type`), returning the output lines.
    fn eval_input(&mut self, input: &str) -> Vec<String> {
        let mut out = Vec::new();
        match self.parse_input(input) {
            Err(lines) => return lines,
            Ok(stmts) => {
                let last_expr =
                    matches!(stmts.last().map(|stmt| &stmt.kind), Some(StmtKind::Expr(_)));
                let count = stmts.len();
                for (index, stmt) in stmts.into_iter().enumerate() {
                    let printable = last_expr && index + 1 == count;
                    let done = self.eval_stmt(&stmt, printable, &mut out);
                    if !done {
                        break;
                    }
                }
            }
        }
        self.flush_world(&mut out);
        out
    }

    /// Evaluates one input as a single expression (`:type`'s door).
    fn eval_unit(&mut self, source: &str) -> Evaluated {
        let mut out = Vec::new();
        let stmts = match self.parse_input(source) {
            Err(lines) => return Evaluated::Lines(lines),
            Ok(stmts) => stmts,
        };
        let value = match stmts.as_slice() {
            [stmt] => {
                if let StmtKind::Expr(expr) = &stmt.kind {
                    let expr = expr.clone();
                    self.reset_fuel();
                    self.machine.eval(&expr)
                } else {
                    return Evaluated::Lines(vec![
                        ":type takes an expression, not a declaration".to_owned(),
                    ]);
                }
            }
            _ => {
                return Evaluated::Lines(vec![":type takes exactly one expression".to_owned()]);
            }
        };
        match value {
            Ok(value) => {
                self.flush_world(&mut out);
                if out.is_empty() {
                    Evaluated::Value(value)
                } else {
                    // Side effects surfaced (the expression printed, trapped a
                    // trace, …): show them, then the type.
                    out.push(self.render_type(&value));
                    Evaluated::Lines(out)
                }
            }
            Err(signal) => {
                self.render_signal(&signal, None, &mut out);
                self.flush_world(&mut out);
                Evaluated::Lines(out)
            }
        }
    }

    /// Parses one input inside the synthetic wrapper, returning its
    /// statements (the tail expression, if any, is folded into a trailing
    /// expression statement so exactly one shape comes back).
    fn parse_input(&mut self, input: &str) -> Result<Vec<Stmt>, Vec<String>> {
        let wrapped = format!("{PREFIX}{input}\n}}\n");
        let parsed = match crate::parse::parse_source(&wrapped) {
            Ok(parsed) => parsed,
            Err(diag) => {
                self.note_fault(SessionFault::Static);
                return Err(vec![render_diag(&diag)]);
            }
        };
        for item in &parsed.unit.items {
            if let ItemKind::Fn(decl) = &item.kind
                && decl.name.name == "__repl__"
            {
                let Some(body) = &decl.body else { continue };
                let mut stmts = body.stmts.clone();
                if let Some(tail) = &body.tail {
                    stmts.push(Stmt {
                        attrs: Vec::new(),
                        kind: StmtKind::Expr((**tail).clone()),
                        span: tail.span,
                        anchor: "gram.expr.block",
                    });
                }
                return Ok(stmts);
            }
        }
        self.note_fault(SessionFault::Static);
        Err(vec!["input did not parse as statements".to_owned()])
    }

    /// Evaluates one statement; `printable` means "this is the input's final
    /// expression — print its value and type". Returns whether evaluation may
    /// continue with the next statement.
    fn eval_stmt(&mut self, stmt: &Stmt, printable: bool, out: &mut Vec<String>) -> bool {
        self.reset_fuel();
        match &stmt.kind {
            StmtKind::Item(item) => {
                self.install_item(item, out);
                true
            }
            StmtKind::Binding(binding) => {
                // [repl.let.rebind]: the old binding drops NOW, resources
                // and all, visibly in `:mem`.
                self.rebind_drop(&binding.pattern, out);
                match self.machine.exec(stmt) {
                    Ok(()) => true,
                    Err(signal) => {
                        self.flush_stdout(out);
                        self.render_signal(&signal, Some(stmt), out);
                        false
                    }
                }
            }
            StmtKind::Expr(expr) if printable => match self.machine.eval(expr) {
                Ok(Value::Unit) => true,
                Ok(value) => {
                    self.flush_stdout(out);
                    out.push(format!(
                        "{} : {}",
                        self.render_value(&value),
                        self.render_type(&value)
                    ));
                    true
                }
                Err(signal) => {
                    self.flush_stdout(out);
                    self.render_signal(&signal, Some(stmt), out);
                    false
                }
            },
            _ => match self.machine.exec(stmt) {
                Ok(()) => true,
                Err(signal) => {
                    self.flush_stdout(out);
                    self.render_signal(&signal, Some(stmt), out);
                    false
                }
            },
        }
    }

    fn install_item(&mut self, item: &Item, out: &mut Vec<String>) {
        match &item.kind {
            ItemKind::Fn(decl) => {
                let name = decl.name.name.clone();
                let generation = self.fn_gens.entry(name.clone()).or_insert(0);
                *generation += 1;
                let generation = *generation;
                let internal = format!("{name}#{generation}");
                self.install_def(&name, Def::Fn(decl.clone()));
                self.install_def(&internal, Def::Fn(decl.clone()));
                // [repl.def.shadow]: the surface name is a session local
                // bound to the CURRENT generation; closures capture locals
                // by value, so an existing capture keeps the old one.
                self.remove_root_binding(&name);
                self.machine.declare(&name, Slot::live(Value::Fn(internal)));
                if generation == 1 {
                    out.push(format!("defined fn `{name}`"));
                } else {
                    out.push(format!(
                        "redefined fn `{name}` — shadowing, not mutation: existing captures \
                         keep the previous definition [repl.def.shadow]"
                    ));
                }
            }
            ItemKind::Struct(def) => {
                let Some(ident) = &def.name else {
                    out.push("recorded anonymous struct (no dynamic semantics here)".to_owned());
                    return;
                };
                let name = ident.name.clone();
                let generation = self.type_gens.entry(name.clone()).or_insert(0);
                *generation += 1;
                let generation = *generation;
                let internal = format!("{name}#{generation}");
                self.install_def(&name, Def::Struct(def.clone()));
                self.install_def(&internal, Def::Struct(def.clone()));
                self.machine
                    .shared
                    .repl_types
                    .lock()
                    .expect("repl types lock")
                    .insert(name.clone(), generation);
                if generation == 1 {
                    out.push(format!("defined type `{name}`"));
                } else {
                    out.push(format!(
                        "redefined type `{name}` — a new nominal type (generation {generation}); \
                         existing values keep `{name}#{}` and print with the stale marker \
                         [repl.type.gen]",
                        generation - 1
                    ));
                }
            }
            ItemKind::TypeAlias(alias) => {
                self.install_def(&alias.name.name.clone(), Def::Opaque("type"));
                out.push(format!(
                    "recorded type alias `{}` (no dynamic semantics here)",
                    alias.name.name
                ));
            }
            ItemKind::Enum(def) => {
                let name = def
                    .name
                    .as_ref()
                    .map_or("<anonymous>".to_owned(), |ident| ident.name.clone());
                if name != "<anonymous>" {
                    self.install_def(&name, Def::Opaque("enum"));
                }
                out.push(format!(
                    "recorded enum `{name}` (no dynamic semantics here)"
                ));
            }
            ItemKind::Trait(def) => {
                self.install_def(&def.name.name.clone(), Def::Opaque("trait"));
                out.push(format!(
                    "recorded trait `{}` (no dynamic semantics here)",
                    def.name.name
                ));
            }
            ItemKind::Impl(_) => {
                out.push("recorded impl (no dynamic semantics here)".to_owned());
            }
            ItemKind::Use(_) => {
                // [repl.module]: D31/D32 do not apply at the prompt.
                out.push(
                    "the module rules do not apply at the prompt ([repl.module]); \
                     `:load file.lu` is the textual-inclusion door"
                        .to_owned(),
                );
            }
            ItemKind::ImportC(header) => {
                // `import c "…"` is the C membrane (D17), not a module rule:
                // it works at the prompt so the is04 provenance machine is
                // reachable from a `:trace` session.
                let name: String = header.parts.iter().fold(String::new(), |acc, part| {
                    if let crate::ast::StrPart::Text(text) = part {
                        acc + text
                    } else {
                        acc
                    }
                });
                let program = Arc::make_mut(&mut self.machine.shared.program);
                if let Some(root) = program.modules.get_mut("")
                    && !root.c_headers.contains(&name)
                {
                    root.c_headers.push(name.clone());
                }
                out.push(format!("imported C header `{name}` (host intrinsics)"));
            }
            ItemKind::Binding(binding) => {
                // Item-level bindings arrive as statements from the wrapper;
                // this arm is for completeness.
                self.rebind_drop(&binding.pattern, out);
                let stmt = Stmt {
                    attrs: Vec::new(),
                    kind: StmtKind::Binding(binding.clone()),
                    span: binding.span,
                    anchor: "gram.item.let",
                };
                if let Err(signal) = self.machine.exec(&stmt) {
                    self.render_signal(&signal, None, out);
                }
            }
        }
    }

    fn install_def(&mut self, name: &str, def: Def) {
        let program = Arc::make_mut(&mut self.machine.shared.program);
        if let Some(root) = program.modules.get_mut("") {
            root.items.insert(name.to_owned(), (def, true));
        }
    }

    /// Sweeps and removes an about-to-be-shadowed root binding
    /// (`[repl.let.rebind]`): regions free, `shared` counts drop — the
    /// normal death rules, immediately.
    fn rebind_drop(&mut self, pattern: &Pattern, out: &mut Vec<String>) {
        let mut names = Vec::new();
        collect_bound_names(pattern, &mut names);
        for name in names {
            if self.remove_root_binding(&name) {
                out.push(format!(
                    "`{name}` rebound: the old binding dropped ([repl.let.rebind])"
                ));
            }
        }
    }

    /// Removes `name` from the session's root scope, sweeping its resources.
    /// Returns whether a live binding existed.
    fn remove_root_binding(&mut self, name: &str) -> bool {
        let Some(frame) = self.machine.frames.first_mut() else {
            return false;
        };
        let Some(scope) = frame.scopes.first_mut() else {
            return false;
        };
        let mut dropped = Vec::new();
        let mut existed = false;
        scope.locals.retain(|(bound, slot)| {
            if bound == name {
                existed = existed || slot.is_live();
                dropped.push((bound.clone(), slot.clone()));
                false
            } else {
                true
            }
        });
        if !dropped.is_empty() {
            let morgue = Scope {
                locals: dropped,
                ..Scope::default()
            };
            self.machine.reclaim(&morgue);
        }
        existed
    }

    fn reset_fuel(&mut self) {
        self.machine.shared.steps.store(0, Ordering::Relaxed);
    }

    // -- rendering ----------------------------------------------------------

    /// Drains the machine's stdout buffer into the output, so printed bytes
    /// land in chronological order relative to value and diagnostic lines.
    fn flush_stdout(&mut self, out: &mut Vec<String>) {
        let stdout = std::mem::take(&mut *self.machine.shared.stdout.lock().expect("stdout lock"));
        if !stdout.is_empty() {
            let text = String::from_utf8_lossy(&stdout);
            for line in text.split_inclusive('\n') {
                out.push(line.trim_end_matches('\n').to_owned());
            }
        }
    }

    /// Drains stdout and the trace buffer into the output/ring. Called after
    /// every input; [`Session::flush_stdout`] runs mid-input wherever a value
    /// or diagnostic line is about to be appended.
    fn flush_world(&mut self, out: &mut Vec<String>) {
        self.flush_stdout(out);
        let trace = std::mem::take(&mut *self.machine.shared.trace.lock().expect("trace lock"));
        for line in trace {
            if self.trace.len() == TRACE_RING {
                self.trace.pop_front();
            }
            self.trace.push_back(adjust_trace_spans(&line));
        }
    }

    /// Renders a value with the session's generation-aware type names: the
    /// CURRENT generation prints bare, stale generations keep their marker.
    fn render_value(&self, value: &Value) -> String {
        self.strip_current_generations(&value.to_string())
    }

    fn render_type(&self, value: &Value) -> String {
        self.strip_current_generations(&value.kind())
    }

    fn strip_current_generations(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for (name, generation) in &self.type_gens {
            let current = format!("{name}#{generation}");
            text = text.replace(&current, name);
        }
        text
    }

    /// Renders a signal as output; the session survives every one
    /// (`[repl.trap.alive]`). Every diagnostic-shaped line carries its
    /// code/kind and its clause anchor.
    fn render_signal(&mut self, signal: &Signal, stmt: Option<&Stmt>, out: &mut Vec<String>) {
        let _ = stmt;
        match signal {
            Signal::Trap(trap) => {
                self.note_fault(SessionFault::Dynamic);
                let mut line = format!(
                    "trap({}): {} [{}]",
                    trap.kind,
                    trap.message,
                    trap.rule.anchor()
                );
                let span = adjust(trap.span.start)..adjust(trap.span.end);
                line.push_str(&format!(" at {}..{}", span.start, span.end));
                out.push(line);
                if let Some((secondary, note)) = &trap.secondary {
                    out.push(format!(
                        "  {note} at {}..{}",
                        adjust(secondary.start),
                        adjust(secondary.end)
                    ));
                }
                out.push(
                    "the session survives the trap; the world is as the fault left it \
                     [repl.trap.alive]"
                        .to_owned(),
                );
            }
            Signal::Ub(finding) => {
                self.note_fault(SessionFault::Dynamic);
                // The D2 pairing, at the prompt: the row, its clause, and the
                // optimization the row licenses.
                let mut line = format!(
                    "ub({}) §7/{}: {} [{}] at {}..{}",
                    finding.anchor(),
                    finding.row,
                    finding.message,
                    finding.row.clause(),
                    adjust(finding.span.start),
                    adjust(finding.span.end)
                );
                if let Some(span) = finding.tag_span {
                    line.push_str(&format!(
                        "; tag created at {}..{}",
                        adjust(span.start),
                        adjust(span.end)
                    ));
                }
                out.push(line);
                out.push(format!("  licenses {}", finding.row.optimization()));
                out.push(
                    "the session survives the finding; the world is as the access left it \
                     [repl.trap.alive]"
                        .to_owned(),
                );
            }
            Signal::Return(Value::Error(err)) => {
                out.push(format!("error `{}` [err.union]", err.tag));
            }
            Signal::Return(value) => {
                out.push(format!(
                    "{} : {}",
                    self.render_value(value),
                    self.render_type(value)
                ));
            }
            Signal::Break(_) | Signal::Continue => {
                self.note_fault(SessionFault::Static);
                out.push("`break`/`continue` outside a loop [gram.expr.flow]".to_owned());
            }
            Signal::ProcKilled => {
                self.note_fault(SessionFault::Dynamic);
                out.push(
                    "the root domain was killed ([conc.proc.root]); :reset for a fresh world"
                        .to_owned(),
                );
            }
            Signal::Unsupported(reason) => {
                self.note_fault(SessionFault::Unsupported);
                out.push(format!("unsupported: {reason}"));
            }
            Signal::Exit(code) => {
                self.note_fault(SessionFault::Dynamic);
                out.push(format!(
                    "the program asked to exit({code}) ([conf.trap.exit]); the session                      survives — :quit ends it"
                ));
            }
        }
    }

    // -- `:mem` / `:regions` ------------------------------------------------

    /// The memory model, live (Target 3). Deterministic: stable ids, sorted
    /// iteration, so transcripts snapshot.
    fn mem_dump(&mut self, full: bool) -> Vec<String> {
        let mut out = Vec::new();
        {
            let store = self.machine.store();
            out.push("regions:".to_owned());
            for region in store.regions() {
                if region.state == super::region::RegionState::Freed {
                    continue;
                }
                let name = region.name.clone().unwrap_or_else(|| {
                    if region.id == 0 {
                        "program".to_owned()
                    } else {
                        "-".to_owned()
                    }
                });
                let owner = match region.claim {
                    None => String::new(),
                    Some(super::region::TaskClaim::InChannel) => " owner=in-channel".to_owned(),
                    Some(super::region::TaskClaim::Task(task)) => format!(" owner=task{task}"),
                };
                let parent = match region.parent {
                    Some(parent) => format!(" parent=#{parent}"),
                    None => String::new(),
                };
                let strategy = region.strategy.to_string();
                let state = match region.state {
                    super::region::RegionState::Open => "open",
                    super::region::RegionState::Suspended => "suspended",
                    super::region::RegionState::Frozen => "frozen",
                    super::region::RegionState::Freed => "freed",
                };
                out.push(format!(
                    "  #{} `{}` {strategy} state={state} objects={}{}{}",
                    region.id, name, region.allocations, parent, owner,
                ));
            }
            if full {
                let cells = store.live_cells();
                if !cells.is_empty() {
                    out.push("shared cells:".to_owned());
                    for id in cells {
                        if let Some(cell) = store.cell(id) {
                            out.push(format!(
                                "  shared#{id} strong={} weak={}",
                                cell.strong, cell.weak
                            ));
                        }
                    }
                }
                let pools = store.pools();
                if !pools.is_empty() {
                    out.push("pools:".to_owned());
                    for pool in pools {
                        out.push(format!(
                            "  pool#{}[{}] in region #{}",
                            pool.id, pool.elem, pool.region
                        ));
                        for (index, slot) in pool.slots.iter().enumerate() {
                            let life = match slot.life {
                                super::region::SlotLife::Reserved => "reserved",
                                super::region::SlotLife::Live => "live",
                                super::region::SlotLife::Free => "free",
                            };
                            out.push(format!(
                                "    [{index}] generation={} {life}",
                                slot.generation
                            ));
                        }
                    }
                }
            }
        }
        if full {
            if !self.machine.access.is_empty() {
                out.push("loans (live borrows / `mut` holds):".to_owned());
                for line in self.machine.access.dump() {
                    out.push(format!("  {line}"));
                }
            }
            if self.machine.concurrent() {
                out.push("tasks and procs:".to_owned());
                for line in self.machine.shared.sched.dump() {
                    out.push(format!("  {line}"));
                }
            }
        }
        out
    }
}

enum Evaluated {
    Value(Value),
    Lines(Vec<String>),
}

/// Adjusts a span offset out of the synthetic wrapper's prefix, so rendered
/// positions index the input the user actually typed.
fn adjust(offset: usize) -> usize {
    offset.saturating_sub(PREFIX.len())
}

/// Rewrites the leading `start..end` span of a trace line out of the
/// synthetic wrapper's prefix, so `:trace show` positions index what the
/// user typed. Lines without a parsable span (scheduler notes with span 0)
/// pass through unchanged.
fn adjust_trace_spans(line: &str) -> String {
    let trimmed = line.trim_start();
    let Some((head, rest)) = trimmed.split_once(' ') else {
        return line.to_owned();
    };
    let Some((start, end)) = head.split_once("..") else {
        return line.to_owned();
    };
    let (Ok(start), Ok(end)) = (start.parse::<usize>(), end.parse::<usize>()) else {
        return line.to_owned();
    };
    if start < PREFIX.len() && end < PREFIX.len() {
        return line.to_owned();
    }
    format!(
        "{:>6}..{:<6} {}",
        adjust(start),
        adjust(end),
        rest.trim_start()
    )
}

fn render_diag(diag: &crate::diag::Diag) -> String {
    format!(
        "error[{}]: {} [{}] at {}..{}",
        diag.code,
        diag.message,
        diag.anchor,
        adjust(diag.span.start),
        adjust(diag.span.end)
    )
}

fn collect_bound_names(pattern: &Pattern, out: &mut Vec<String>) {
    match &*pattern.kind {
        PatKind::Binding(ident) => out.push(ident.name.clone()),
        PatKind::At { name, pattern } => {
            out.push(name.name.clone());
            collect_bound_names(pattern, out);
        }
        PatKind::Variant { fields, .. } => {
            for field in fields {
                collect_bound_names(field, out);
            }
        }
        PatKind::Tuple(fields) => {
            for field in fields {
                collect_bound_names(field, out);
            }
        }
        PatKind::Or(alternatives) => {
            for alternative in alternatives {
                collect_bound_names(alternative, out);
            }
        }
        PatKind::Wildcard | PatKind::Literal(_) => {}
    }
}
