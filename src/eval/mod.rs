//! The tree-walking evaluator — `spec/02-memory-model.md` §2 as a machine.
//!
//! # What this is
//!
//! Every ownership rule in `[mem.tier0]` is enforced here as a **dynamic
//! check**, and every fault cites the clause it enforces ([`rules::Rule`]).
//! The compiler proves these properties statically and rejects; this machine
//! observes them at run time and traps. The approximation direction is
//! one-way and stated in the sprint: *the compiler accepts ⇒ the interpreter
//! must not fault; never the converse.* A program the compiler rejects that
//! runs clean here is static conservatism, not a bug.
//!
//! # There is no unwinding
//!
//! D30 is load-bearing: `!T` is a tagged value, `?` is a return, `else` is a
//! branch, `errdefer` is a scope-exit list consulted on one of two paths. The
//! Rust-level [`Signal`] type carries `return`/`break`/`continue`/trap out of a
//! walk, and *that* is not unwinding either — it is a `Result` on the way back
//! up. A Rust `panic!` anywhere in this module is by definition an interpreter
//! bug; `tests/fuzz_smoke.rs` treats it as one.
//!
//! # Deliberately not clever
//!
//! No caching, no bytecode, no interning, per the sprint's non-targets: "a
//! tree-walk with per-access state checks is the design, not a compromise".

pub mod builtin;
pub mod place;
pub mod rules;
pub mod value;

use std::collections::BTreeMap;

use crate::ast::{
    Arg, AssignOp, BinOp, Binding, Block, ClosureParam, ElseHandler, Expr, ExprKind, FnDecl,
    IndexArg, Interpolation, Member, ParamKind, ParamMode, PatKind, Pattern, Stmt, StmtKind,
    StrPart, Type, TypeArg, TypeKind, UnOp,
};
use crate::diag::Span;
use crate::sema::{Def, Program};
use crate::trap::TrapKind;

use place::{Access, AccessSet, Held, Path, Proj};
use rules::Rule;
use value::{ArithMode, ClosureValue, ErrorValue, IntTy, Slot, SlotState, Value};

/// A trap: a fault of a *defined* execution, named by the closed vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trap {
    pub kind: TrapKind,
    /// The rule that fired, and through it the clause anchor.
    pub rule: Rule,
    /// Where the fault happened.
    pub span: Span,
    /// The second span the sprint asks every ownership fault to print — the
    /// move site for `use-after-move`, the conflicting access for
    /// `exclusivity`.
    pub secondary: Option<(Span, String)>,
    pub message: String,
}

impl std::fmt::Display for Trap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "trap({}): {} [{}] at {}",
            self.kind,
            self.message,
            self.rule.anchor(),
            self.span
        )?;
        if let Some((span, note)) = &self.secondary {
            write!(f, "; {note} at {span}")?;
        }
        Ok(())
    }
}

/// Non-local control, carried up a `Result`'s error arm. Not unwinding.
#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Return(Value),
    Break(Value),
    Continue,
    Trap(Box<Trap>),
    /// Outside is02's coverage, or a sema-lite failure (unresolvable name,
    /// ambiguous dispatch, a type error the checker owns). Becomes verdict
    /// `unsupported` with the reason on an `x-` key — never a crash, and never
    /// a trap, because the trap vocabulary is for faults of *defined*
    /// executions (`[conf.trap.map]`).
    Unsupported(String),
}

type EResult<T> = Result<T, Signal>;

/// What applying a callable produced.
#[derive(Debug, Clone)]
struct Applied {
    value: Value,
    /// The callee's parameters as they stood when the call ended — the
    /// "result" half of call-by-value-result, which is how `mut` is inout
    /// under value semantics (`[mem.tier0.mode.mut]`).
    params: Vec<Value>,
}

/// The evaluated arguments of one call.
#[derive(Debug, Clone)]
struct Args {
    /// One value per argument, in source order.
    values: Vec<Value>,
    /// `(argument index, path)` for each `mut` argument — where the callee's
    /// final parameter value is stored back when the call ends.
    writebacks: Vec<(usize, Path)>,
    /// How many accesses this call pushed onto the access set, to release.
    held: usize,
}

/// A method call's receiver: a place when the receiver denotes one (so a
/// mutating method's effect lands back in it), a plain expression otherwise.
#[derive(Debug, Clone)]
enum Receiver {
    Place(Path),
    Expr(Expr),
}

fn unsupported<T>(reason: impl Into<String>) -> EResult<T> {
    Err(Signal::Unsupported(reason.into()))
}

/// How a program ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Exit(u8),
    Trap(Box<Trap>),
    Unsupported(String),
}

/// Everything one run produced.
#[derive(Debug, Clone)]
pub struct Run {
    pub outcome: Outcome,
    pub stdout: Vec<u8>,
    /// One line per rule firing, when `--trace` asked for it.
    pub trace: Vec<String>,
}

/// A lexical scope: its locals and its scope-exit effects.
#[derive(Debug, Default)]
struct Scope {
    locals: Vec<(String, Slot)>,
    /// `(on_error, expr)` in registration order; run in reverse
    /// (`[mem.shared.drop.1]`).
    defers: Vec<(bool, Expr)>,
    /// Accesses held for this scope's extent (local borrows).
    held: usize,
}

#[derive(Debug)]
struct Frame {
    module: String,
    scopes: Vec<Scope>,
}

/// The abstract machine.
pub struct Machine<'p> {
    program: &'p Program,
    frames: Vec<Frame>,
    access: AccessSet,
    globals: BTreeMap<String, Slot>,
    stdout: Vec<u8>,
    trace: Vec<String>,
    tracing: bool,
    /// Steps taken, against [`Machine::FUEL`].
    steps: u64,
}

impl<'p> Machine<'p> {
    /// Evaluation steps a program may take before the machine gives up.
    ///
    /// A non-terminating program has no verdict — `[proto.record.verdict]`
    /// offers none — so the machine declines rather than hangs a CI job. It is
    /// generous enough that nothing in the pinned corpus comes close.
    pub const FUEL: u64 = 50_000_000;

    #[must_use]
    pub fn new(program: &'p Program) -> Machine<'p> {
        Machine {
            program,
            frames: Vec::new(),
            access: AccessSet::new(),
            globals: BTreeMap::new(),
            stdout: Vec::new(),
            trace: Vec::new(),
            tracing: false,
            steps: 0,
        }
    }

    #[must_use]
    pub fn tracing(mut self, on: bool) -> Machine<'p> {
        self.tracing = on;
        self
    }

    /// Runs the program's `main`.
    pub fn run(mut self) -> Run {
        let outcome = self.run_main();
        Run {
            outcome,
            stdout: self.stdout,
            trace: self.trace,
        }
    }

    fn run_main(&mut self) -> Outcome {
        self.frames.push(Frame {
            module: String::new(),
            scopes: vec![Scope::default()],
        });

        // Item-level `let`/`var`/`const` evaluate once, in declaration order.
        let bindings = self.program.root().bindings.clone();
        for name in bindings {
            let Some(Def::Binding(binding)) = self.program.lookup("", &name, false).cloned() else {
                continue;
            };
            match self.eval(&binding.value) {
                Ok(value) => {
                    self.globals.insert(name, Slot::live(value));
                }
                Err(signal) => return self.finish(Err(signal)),
            }
        }

        let Some(Def::Fn(main)) = self.program.lookup("", "main", false).cloned() else {
            return Outcome::Unsupported(
                "the program has no `main` in its root module (D32: directory = module)".to_owned(),
            );
        };

        // `fn main(args: List[str])` receives the process arguments; conform-run
        // passes none, which is what `wordcount.lu`'s `check: run(exit=2)`
        // expects to observe.
        let args = if main.params.is_empty() {
            Vec::new()
        } else {
            vec![Value::List(Vec::new())]
        };
        let result = self
            .call_fn(&main, "", args, main.span)
            .map(|applied| applied.value);
        self.finish(result)
    }

    fn finish(&mut self, result: EResult<Value>) -> Outcome {
        match result {
            Ok(Value::Int(v, _)) => Outcome::Exit(u8::try_from(v.rem_euclid(256)).unwrap_or(0)),
            Ok(Value::Unit) => Outcome::Exit(0),
            Ok(Value::Error(err)) => {
                // `main` returned an error rather than an int. The protocol has
                // no verdict for "returned an error", and the spec does not fix
                // a status; 1 is the conventional one and the divergence, if
                // any, is a stdout/exit comparison the differ will surface.
                self.write_out(&format!("error: {}\n", err.tag));
                Outcome::Exit(1)
            }
            Ok(other) => Outcome::Unsupported(format!(
                "`main` returned {}; the exit status comes from an `int`",
                other.kind()
            )),
            Err(Signal::Trap(trap)) => Outcome::Trap(trap),
            Err(Signal::Unsupported(reason)) => Outcome::Unsupported(reason),
            Err(Signal::Return(value)) => {
                let result = Ok(value);
                self.finish(result)
            }
            Err(Signal::Break(_) | Signal::Continue) => {
                Outcome::Unsupported("`break`/`continue` outside a loop".to_owned())
            }
        }
    }

    // -- bookkeeping -------------------------------------------------------

    fn fire(&mut self, rule: Rule, span: Span, detail: &str) {
        if self.tracing {
            self.trace.push(format!(
                "{:>6}..{:<6} {} {}",
                span.start, span.end, rule, detail
            ));
        }
    }

    fn step(&mut self) -> EResult<()> {
        self.steps += 1;
        if self.steps > Machine::FUEL {
            return unsupported(format!(
                "the program did not terminate within {} evaluation steps",
                Machine::FUEL
            ));
        }
        Ok(())
    }

    fn trap<T>(
        &mut self,
        kind: TrapKind,
        rule: Rule,
        span: Span,
        message: impl Into<String>,
        secondary: Option<(Span, String)>,
    ) -> EResult<T> {
        let message = message.into();
        self.fire(Rule::TrapVocabulary, span, &format!("{kind}: {message}"));
        Err(Signal::Trap(Box::new(Trap {
            kind,
            rule,
            span,
            secondary,
            message,
        })))
    }

    fn frame(&self) -> usize {
        self.frames.len() - 1
    }

    fn push_scope(&mut self) {
        let held = self.access.len();
        if let Some(frame) = self.frames.last_mut() {
            frame.scopes.push(Scope {
                held,
                ..Scope::default()
            });
        }
    }

    fn pop_scope(&mut self) {
        if let Some(frame) = self.frames.last_mut()
            && let Some(scope) = frame.scopes.pop()
        {
            // A borrow's dynamic extent ends at its binding's death
            // (`[mem.tier0.borrow.1]`).
            let extra = self.access.len().saturating_sub(scope.held);
            self.access.release(extra);
        }
    }

    fn declare(&mut self, name: &str, slot: Slot) {
        if let Some(frame) = self.frames.last_mut()
            && let Some(scope) = frame.scopes.last_mut()
        {
            scope.locals.push((name.to_owned(), slot));
        }
    }

    fn local_exists(&self, name: &str) -> bool {
        self.frames.last().is_some_and(|frame| {
            frame
                .scopes
                .iter()
                .any(|scope| scope.locals.iter().any(|(n, _)| n == name))
        })
    }

    /// The slot a path denotes, if the path is well-formed against the current
    /// state. `None` means "no such place", which the caller turns into a
    /// sema-lite gap rather than a trap.
    fn slot_mut(&mut self, path: &Path) -> Option<&mut Slot> {
        self.resolve(path).map(|(slot, _)| slot)
    }

    /// The slot a path denotes, plus the *earliest* move along the way.
    ///
    /// The second half is the whole of "reads of `Moved` trap": `a` moving out
    /// makes `a.x` unreadable too, because `a.x`'s storage went with it, and
    /// the fault has to name the move site that actually happened rather than
    /// the leaf's own state.
    fn resolve(&mut self, path: &Path) -> Option<(&mut Slot, Option<(Span, usize)>)> {
        let mut slot: &mut Slot = if path.frame == usize::MAX {
            self.globals.get_mut(&path.base)?
        } else {
            let frame = self.frames.get_mut(path.frame)?;
            let mut found = None;
            for scope in frame.scopes.iter_mut().rev() {
                if let Some(index) = scope.locals.iter().position(|(n, _)| *n == path.base) {
                    found = Some(&mut scope.locals[index].1);
                    break;
                }
            }
            found?
        };

        let mut moved = match slot.state {
            SlotState::Moved(at) => Some((at, 0usize)),
            SlotState::Live => None,
        };
        for (depth, step) in path.projections.iter().enumerate() {
            slot = match (&mut slot.value, step) {
                (Value::Struct { fields, .. }, Proj::Field(name)) => {
                    let index = fields.iter().position(|(f, _)| f == name)?;
                    &mut fields[index].1
                }
                (Value::Tuple(items) | Value::List(items), Proj::Index(i)) => {
                    let index = usize::try_from(*i).ok()?;
                    items.get_mut(index)?
                }
                (Value::Map(pairs), Proj::Key(key)) => {
                    let index = pairs.iter().position(|(k, _)| k.to_string() == *key)?;
                    &mut pairs[index].1
                }
                _ => return None,
            };
            if moved.is_none()
                && let SlotState::Moved(at) = slot.state
            {
                moved = Some((at, depth + 1));
            }
        }
        Some((slot, moved))
    }

    /// Checks a new access against everything currently held
    /// (`[mem.tier0.excl.1]`).
    fn check_access(&mut self, path: &Path, access: Access, span: Span) -> EResult<()> {
        if let Some(held) = self.access.conflict(path, access) {
            let (held_path, held_access, held_span) =
                (held.path.to_string(), held.access, held.span);
            let rule = if path.projections.is_empty() && held_path == path.to_string() {
                Rule::Exclusivity
            } else {
                Rule::PathDisjoint
            };
            return self.trap(
                TrapKind::Exclusivity,
                rule,
                span,
                format!(
                    "`{path}` is accessed as `{access}` while `{held_path}` is held as `{held_access}`; \
                     the paths conflict"
                ),
                Some((held_span, format!("`{held_path}` held here"))),
            );
        }
        Ok(())
    }

    /// Reads through a path: exclusivity first, then the slot's own state.
    fn read_path(&mut self, path: &Path, span: Span) -> EResult<Value> {
        self.check_access(path, Access::Shared, span)?;
        let display = path.to_string();
        match self.resolve(path) {
            None => unsupported(format!("`{display}` does not denote a place at run time")),
            Some((slot, None)) => {
                let value = slot.value.clone();
                self.fire(Rule::PlacePath, span, &format!("read `{display}`"));
                Ok(value)
            }
            Some((_, Some((moved_at, depth)))) => {
                // Name the path that actually moved, which may be a *prefix* of
                // the one being read: moving `p.x` is what makes `p.x.n`
                // unreadable, and blaming `p.x.n` would send the reader to the
                // wrong line.
                let moved_path = prefix(path, depth);
                self.trap(
                    TrapKind::UseAfterMove,
                    Rule::UseAfterMove,
                    span,
                    format!("`{display}` was moved out and is uninitialized here"),
                    Some((moved_at, format!("`{moved_path}` moved here"))),
                )
            }
        }
    }

    /// Moves out of a path: the source place becomes uninitialized
    /// (`[mem.tier0.move.1]`).
    fn move_path(&mut self, path: &Path, span: Span) -> EResult<Value> {
        self.check_access(path, Access::Exclusive, span)?;
        let display = path.to_string();
        let outcome = match self.resolve(path) {
            None => None,
            Some((slot, None)) => Some(Ok(slot.take_value(span))),
            Some((_, Some((at, depth)))) => Some(Err((at, depth))),
        };
        match outcome {
            None => unsupported(format!("`{display}` does not denote a place at run time")),
            Some(Ok(value)) => {
                self.fire(Rule::Move, span, &format!("move out of `{display}`"));
                Ok(value)
            }
            Some(Err((moved_at, depth))) => {
                let moved_path = prefix(path, depth);
                self.trap(
                    TrapKind::UseAfterMove,
                    Rule::UseAfterMove,
                    span,
                    format!("`{display}` was already moved out"),
                    Some((moved_at, format!("`{moved_path}` moved here"))),
                )
            }
        }
    }

    /// Writes through a path, making the place live again if it was moved from
    /// (`[mem.tier0.move.4]`).
    fn write_path(&mut self, path: &Path, value: Value, span: Span) -> EResult<()> {
        self.check_access(path, Access::Exclusive, span)?;
        let display = path.to_string();
        // A map grows on assignment to an absent key; every other path must
        // already denote a place.
        if self.slot_mut(path).is_none() && !self.insert_map_key(path, span)? {
            return unsupported(format!("`{display}` does not denote a place at run time"));
        }
        let Some(slot) = self.slot_mut(path) else {
            return unsupported(format!("`{display}` does not denote a place at run time"));
        };
        let was_moved = !slot.is_live();
        slot.state = SlotState::Live;
        slot.value = value;
        let rule = if was_moved {
            Rule::Reinit
        } else {
            Rule::Assign
        };
        self.fire(rule, span, &format!("write `{display}`"));
        Ok(())
    }

    /// Grows a map when a path's last step is an absent key. Returns whether it
    /// did.
    fn insert_map_key(&mut self, path: &Path, _span: Span) -> EResult<bool> {
        let Some(Proj::Key(key)) = path.projections.last() else {
            return Ok(false);
        };
        let key = key.clone();
        let mut parent = path.clone();
        parent.projections.pop();
        let Some(slot) = self.slot_mut(&parent) else {
            return Ok(false);
        };
        let Value::Map(pairs) = &mut slot.value else {
            return Ok(false);
        };
        pairs.push((Value::Str(key), Slot::live(Value::Unit)));
        Ok(true)
    }

    fn write_out(&mut self, text: &str) {
        self.stdout.extend_from_slice(text.as_bytes());
    }

    // -- calls -------------------------------------------------------------

    /// What a call produced: its value, and the *final* values of its
    /// parameters.
    ///
    /// `mut` is exclusive **inout** (`[mem.tier0.mode.mut]`), and with value
    /// semantics the only way inout is observable is call-by-value-result: the
    /// callee's parameter is copied back into the caller's place when the call
    /// ends. Exclusivity for the call's extent is exactly what makes that
    /// equivalent to sharing the storage — which is why the check and the
    /// write-back are the same mechanism.
    fn call_fn(
        &mut self,
        decl: &FnDecl,
        module: &str,
        args: Vec<Value>,
        span: Span,
    ) -> EResult<Applied> {
        let Some(body) = &decl.body else {
            return unsupported(format!(
                "`{}` has no body (extern or trait signature); the interpreter has no C ABI \
                 and no method dispatch at is02",
                decl.name.name
            ));
        };
        if args.len() != decl.params.len() {
            // Arity is the static type checker's (E0402). Declining is honest;
            // inventing a trap would put a static rejection in the run rung.
            return unsupported(format!(
                "`{}` takes {} argument(s) and was called with {}; argument arity is the type \
                 checker's (E0402), not this machine's",
                decl.name.name,
                decl.params.len(),
                args.len()
            ));
        }
        if self.frames.len() > 512 {
            return unsupported("call depth exceeded 512 frames".to_owned());
        }

        self.fire(Rule::Call, span, &format!("call `{}`", decl.name.name));
        self.frames.push(Frame {
            module: module.to_owned(),
            scopes: vec![Scope::default()],
        });
        for (param, value) in decl.params.iter().zip(args) {
            let name = match &param.kind {
                ParamKind::Named { name, .. } => name.name.clone(),
                ParamKind::SelfParam { .. } => "self".to_owned(),
            };
            let value = match &param.kind {
                ParamKind::Named { ty, .. } => coerce(value, Some(ty)),
                ParamKind::SelfParam { .. } => value,
            };
            self.declare(&name, Slot::live(value));
        }

        let result = self.eval_block(body);
        let frame = self.frame();
        self.access.release_frame(frame);
        // The parameters' final values, read out before the frame dies — this
        // is the "result" half of call-by-value-result.
        let params = decl
            .params
            .iter()
            .map(|param| {
                let name = match &param.kind {
                    ParamKind::Named { name, .. } => name.name.as_str(),
                    ParamKind::SelfParam { .. } => "self",
                };
                self.frames
                    .last()
                    .and_then(|frame| {
                        frame.scopes.iter().rev().find_map(|scope| {
                            scope
                                .locals
                                .iter()
                                .rev()
                                .find(|(n, _)| n == name)
                                .map(|(_, slot)| slot.value.clone())
                        })
                    })
                    .unwrap_or(Value::Unit)
            })
            .collect();
        self.frames.pop();

        let value = match result {
            Ok(value) | Err(Signal::Return(value)) => value,
            Err(other) => return Err(other),
        };
        Ok(Applied { value, params })
    }

    /// Evaluates the arguments of a call, applying call-site modes (X1).
    ///
    /// Exclusivity is checked as the arguments accumulate, which is what makes
    /// `f(mut a.x, mut a.y)` legal and `f(mut a, mut a.x)` a trap.
    fn eval_args(&mut self, args: &[Arg]) -> EResult<Args> {
        let mut values = Vec::with_capacity(args.len());
        let mut writebacks = Vec::new();
        let mut held = 0usize;

        for (index, arg) in args.iter().enumerate() {
            match arg.mode {
                Some(ParamMode::Mut) => {
                    let path = self.place_of(&arg.expr)?;
                    self.check_access(&path, Access::Exclusive, arg.span)?;
                    let value = self.read_path(&path, arg.span)?;
                    self.fire(
                        Rule::ModeMut,
                        arg.span,
                        &format!("`mut {path}` held for the call"),
                    );
                    self.access.push(Held {
                        path: path.clone(),
                        access: Access::Exclusive,
                        span: arg.span,
                    });
                    held += 1;
                    writebacks.push((index, path));
                    values.push(value);
                }
                Some(ParamMode::Take) => {
                    // `take` consumes: the argument moves at the call site
                    // (`[mem.tier0.mode.take]` → `[mem.tier0.move.1]`).
                    let value = match self.live_place(&arg.expr)? {
                        Some(path) => {
                            self.fire(Rule::ModeTake, arg.span, &format!("`take {path}`"));
                            self.move_path(&path, arg.span)?
                        }
                        None => self.eval(&arg.expr)?,
                    };
                    values.push(value);
                }
                None => {
                    // Default mode: immutable for the whole call, caller retains
                    // (`[mem.tier0.mode.read]`).
                    let value = match self.live_place(&arg.expr)? {
                        Some(path) => {
                            self.check_access(&path, Access::Shared, arg.span)?;
                            let value = self.read_path(&path, arg.span)?;
                            self.fire(Rule::ModeRead, arg.span, &format!("`read {path}`"));
                            self.access.push(Held {
                                path,
                                access: Access::Shared,
                                span: arg.span,
                            });
                            held += 1;
                            value
                        }
                        None => self.eval(&arg.expr)?,
                    };
                    values.push(value);
                }
            }
        }
        Ok(Args {
            values,
            writebacks,
            held,
        })
    }

    fn finish_args(
        &mut self,
        writebacks: &[(usize, Path)],
        final_values: &[Value],
        held: usize,
        span: Span,
    ) {
        self.access.release(held);
        for (index, path) in writebacks {
            let Some(value) = final_values.get(*index) else {
                continue;
            };
            if let Some(slot) = self.slot_mut(path) {
                slot.state = SlotState::Live;
                slot.value = value.clone();
            }
        }
        if !writebacks.is_empty() {
            self.fire(Rule::ModeMut, span, "`mut` arguments written back");
        }
    }

    // -- statements & blocks -----------------------------------------------

    fn eval_block(&mut self, block: &Block) -> EResult<Value> {
        self.push_scope();
        let result = self.eval_block_body(block);
        // `errdefer` runs when the scope is left on the error path, and only
        // then (`[err.errdefer]`); `defer` runs on both.
        let errored = match &result {
            Ok(value) => value.is_error(),
            Err(Signal::Return(value)) => value.is_error(),
            Err(Signal::Trap(_) | Signal::Unsupported(_)) => true,
            Err(Signal::Break(_) | Signal::Continue) => false,
        };
        let defers = self.run_defers(errored);
        self.pop_scope();
        match (result, defers) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(signal), _) => Err(signal),
            (Ok(_), Err(signal)) => Err(signal),
        }
    }

    fn eval_block_body(&mut self, block: &Block) -> EResult<Value> {
        for stmt in &block.stmts {
            self.exec(stmt)?;
        }
        match &block.tail {
            // `[gram.expr.block]`: the block's value is its tail expression.
            Some(tail) => {
                let value = self.eval(tail)?;
                self.fire(Rule::Block, block.span, "block yields its tail");
                Ok(value)
            }
            None => Ok(Value::Unit),
        }
    }

    fn run_defers(&mut self, errored: bool) -> EResult<()> {
        let Some(frame) = self.frames.last_mut() else {
            return Ok(());
        };
        let Some(scope) = frame.scopes.last_mut() else {
            return Ok(());
        };
        let defers = std::mem::take(&mut scope.defers);
        // LIFO (`[mem.shared.drop.1]`): scope-exit effects run in reverse
        // registration order.
        for (on_error, expr) in defers.into_iter().rev() {
            if on_error && !errored {
                continue;
            }
            let rule = if on_error {
                Rule::ErrDefer
            } else {
                Rule::DeferLifo
            };
            self.fire(rule, expr.span, "scope-exit effect");
            self.eval(&expr)?;
        }
        Ok(())
    }

    fn exec(&mut self, stmt: &Stmt) -> EResult<()> {
        self.step()?;
        match &stmt.kind {
            StmtKind::Binding(binding) => self.exec_binding(binding),
            StmtKind::Assign { place, op, value } => self.exec_assign(place, *op, value, stmt.span),
            StmtKind::Defer { on_error, expr } => {
                if let Some(frame) = self.frames.last_mut()
                    && let Some(scope) = frame.scopes.last_mut()
                {
                    scope.defers.push((*on_error, expr.clone()));
                }
                Ok(())
            }
            StmtKind::AssumeNoalias(_) => unsupported(
                "`assume noalias` is Tier 3; the provenance machine that gives it meaning is is04"
                    .to_owned(),
            ),
            StmtKind::Expr(expr) => {
                self.eval(expr)?;
                Ok(())
            }
            StmtKind::Item(_) => Ok(()),
        }
    }

    fn exec_binding(&mut self, binding: &Binding) -> EResult<()> {
        // Initialization moves (`[mem.tier0.move.1]`) unless the source is a
        // `Copy`-shaped value or is not a place at all.
        let value = self.eval_for_init(&binding.value)?;
        let value = coerce(value, binding.ty.as_ref());
        self.bind_pattern(&binding.pattern, value)
    }

    /// The path an expression denotes, but only when that path denotes storage
    /// that exists right now.
    ///
    /// `xs[3]` on a one-element list *is* a well-formed place expression and is
    /// *not* a place: the difference is the `bounds` trap, and it belongs to the
    /// value path (`builtin::index`), not to a "this is not a place" gap.
    fn live_place(&mut self, expr: &Expr) -> EResult<Option<Path>> {
        match self.place_of(expr) {
            Ok(path) if self.slot_mut(&path).is_some() => Ok(Some(path)),
            Ok(_) | Err(Signal::Unsupported(_)) => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// Evaluates an initializer: a bare place expression *moves*.
    fn eval_for_init(&mut self, expr: &Expr) -> EResult<Value> {
        match self.live_place(expr)? {
            Some(path) => {
                let value = self.read_path(&path, expr.span)?;
                if is_copy(&value) {
                    self.fire(Rule::ValueSemantics, expr.span, "copy (Copy-shaped value)");
                    Ok(value)
                } else {
                    self.move_path(&path, expr.span)
                }
            }
            None => self.eval(expr),
        }
    }

    fn bind_pattern(&mut self, pattern: &Pattern, value: Value) -> EResult<()> {
        match &*pattern.kind {
            PatKind::Wildcard => Ok(()),
            PatKind::Binding(ident) => {
                self.declare(&ident.name, Slot::live(value));
                Ok(())
            }
            PatKind::Tuple(items) => {
                let Value::Tuple(slots) = value else {
                    return unsupported(format!(
                        "a tuple pattern was given {}; shape mismatches are the type checker's",
                        value.kind()
                    ));
                };
                if slots.len() != items.len() {
                    return unsupported(
                        "tuple pattern arity does not match the value; the type checker owns this"
                            .to_owned(),
                    );
                }
                for (sub, slot) in items.iter().zip(slots) {
                    self.bind_pattern(sub, slot.value)?;
                }
                Ok(())
            }
            PatKind::At { name, pattern } => {
                self.declare(&name.name, Slot::live(value.clone()));
                self.bind_pattern(pattern, value)
            }
            _ => unsupported(
                "only irrefutable patterns bind here; a refutable one needs `match`".to_owned(),
            ),
        }
    }

    fn exec_assign(&mut self, place: &Expr, op: AssignOp, value: &Expr, span: Span) -> EResult<()> {
        let path = self.place_of(place)?;
        if op == AssignOp::Assign {
            let value = self.eval_for_init(value)?;
            // Keep the place's integer type: `x += 1` and `x = x + 1` agree.
            let value = match (self.slot_mut(&path).map(|s| s.value.clone()), value) {
                (Some(Value::Int(_, ty)), Value::Int(v, lit)) if lit.literal => Value::Int(v, ty),
                (_, value) => value,
            };
            return self.write_path(&path, value, span);
        }

        let current = self.read_path(&path, place.span)?;
        let rhs = self.eval(value)?;
        let binop = match op {
            AssignOp::Assign => unreachable!("handled above"),
            AssignOp::Add => BinOp::Add,
            AssignOp::Sub => BinOp::Sub,
            AssignOp::Mul => BinOp::Mul,
            AssignOp::Div => BinOp::Div,
            AssignOp::Rem => BinOp::Rem,
            AssignOp::BitAnd => BinOp::BitAnd,
            AssignOp::BitOr => BinOp::BitOr,
            AssignOp::BitXor => BinOp::BitXor,
            AssignOp::Shl => BinOp::Shl,
            AssignOp::Shr => BinOp::Shr,
        };
        // A map's absent key defaults to its value type's zero, which is what
        // makes `tally[w] += 1` the idiom the corpus writes.
        let current = if current == Value::Unit {
            Value::Int(0, IntTy::INT)
        } else {
            current
        };
        let result = self.binary(binop, current, rhs, span)?;
        self.write_path(&path, result, span)
    }

    // -- places ------------------------------------------------------------

    /// The path an expression denotes, or `Unsupported` if it denotes no place.
    ///
    /// `Unsupported` here is a *control* answer, not a verdict: callers that
    /// have a value-expression fallback catch it. Only an unresolvable name
    /// escapes to the record.
    fn place_of(&mut self, expr: &Expr) -> EResult<Path> {
        match &*expr.kind {
            // `[gram.item.use]` folds `a.b.c` into one `path` production, so a
            // dotted expression is a *place* whenever its head names a local:
            // field projection and module qualification share a syntax and are
            // told apart here, by what the head turned out to be.
            ExprKind::Path(path) => {
                let name = &path.segments[0].name;
                let mut base = if self.local_exists(name) {
                    Path::local(self.frame(), name.clone())
                } else if self.globals.contains_key(name) {
                    Path {
                        frame: usize::MAX,
                        base: name.clone(),
                        projections: Vec::new(),
                    }
                } else {
                    return unsupported(format!("`{name}` is not a local place"));
                };
                for segment in &path.segments[1..] {
                    base = base.project(Proj::Field(segment.name.clone()));
                }
                Ok(base)
            }
            ExprKind::Group(inner) => self.place_of(inner),
            ExprKind::Member { base, member } => {
                let path = self.place_of(base)?;
                Ok(match member {
                    Member::Named(ident) => path.project(Proj::Field(ident.name.clone())),
                    Member::Index(index, _) => path.project(Proj::Index(i128::from(*index))),
                })
            }
            ExprKind::BracketApply { base, args } => {
                let path = self.place_of(base)?;
                let [IndexArg::Value(arg)] = args.as_slice() else {
                    return unsupported("only a single-argument index denotes a place".to_owned());
                };
                let key = self.eval(&arg.expr)?;
                Ok(match key {
                    Value::Int(i, _) => path.project(Proj::Index(i)),
                    Value::Str(s) => path.project(Proj::Key(s)),
                    other => path.project(Proj::Key(other.to_string())),
                })
            }
            _ => unsupported("this expression denotes no place".to_owned()),
        }
    }

    // -- expressions -------------------------------------------------------

    #[allow(clippy::too_many_lines)]
    fn eval(&mut self, expr: &Expr) -> EResult<Value> {
        self.step()?;
        match &*expr.kind {
            ExprKind::Int(text) => Ok(Value::Int(parse_int(text)?, IntTy::LITERAL)),
            ExprKind::Float(text) => Ok(Value::Float(parse_float(text)?)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Str(literal) => {
                let mut out = String::new();
                for part in &literal.parts {
                    match part {
                        StrPart::Text(text) => out.push_str(text),
                        StrPart::Interp(interp) => out.push_str(&self.eval_interp(interp)?),
                    }
                }
                self.fire(Rule::StrInterp, expr.span, "f-string");
                Ok(Value::Str(out))
            }
            ExprKind::Wildcard => {
                unsupported("`_` is never a value you can read (`[gram.lex.ident]`)".to_owned())
            }
            ExprKind::Path(_) => self.eval_path_expr(expr),
            ExprKind::Group(inner) => self.eval(inner),
            ExprKind::Block(block) => self.eval_block(block),
            ExprKind::Tuple(items) => {
                let mut slots = Vec::with_capacity(items.len());
                for item in items {
                    slots.push(Slot::live(self.eval_for_init(item)?));
                }
                Ok(Value::Tuple(slots))
            }
            ExprKind::StructLit { path, fields } => {
                let name = path
                    .segments
                    .last()
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                let mut built = Vec::with_capacity(fields.len());
                for field in fields {
                    let value = match &field.value {
                        Some(value) => self.eval_for_init(value)?,
                        // `Point { x }` binds the field from the identifier.
                        None => {
                            let path = Path::local(self.frame(), field.name.name.clone());
                            self.read_path(&path, field.span)?
                        }
                    };
                    built.push((field.name.name.clone(), Slot::live(value)));
                }
                self.fire(Rule::Alloc, expr.span, &format!("struct literal `{name}`"));
                Ok(Value::Struct {
                    name,
                    fields: built,
                })
            }
            ExprKind::Unary { op, operand } => self.eval_unary(*op, operand, expr.span),
            ExprKind::Binary { op, lhs, rhs } => {
                // `&&`/`||` short-circuit; everything else evaluates both
                // operands left to right (`[gram.expr.prec]`).
                if matches!(op, BinOp::And | BinOp::Or) {
                    let left = self.eval(lhs)?;
                    let Some(left) = left.as_bool() else {
                        return unsupported(format!(
                            "`{}` needs a bool, got {}",
                            if *op == BinOp::And { "&&" } else { "||" },
                            left.kind()
                        ));
                    };
                    self.fire(Rule::EvalOrder, expr.span, "short-circuit");
                    if (*op == BinOp::And && !left) || (*op == BinOp::Or && left) {
                        return Ok(Value::Bool(left));
                    }
                    let right = self.eval(rhs)?;
                    return match right.as_bool() {
                        Some(right) => Ok(Value::Bool(right)),
                        None => unsupported(format!(
                            "a bool operand was expected, got {}",
                            right.kind()
                        )),
                    };
                }
                let left = self.eval(lhs)?;
                let right = self.eval(rhs)?;
                self.binary(*op, left, right, expr.span)
            }
            ExprKind::Cast { expr: inner, ty } => {
                let value = self.eval(inner)?;
                Ok(coerce(value, Some(ty)))
            }
            ExprKind::Call { callee, args } => self.eval_call(callee, args, expr.span),
            ExprKind::BracketApply { base, args } => self.eval_bracket(base, args, expr.span),
            ExprKind::Member { base, member } => self.eval_member(base, member, expr.span),
            ExprKind::Try(inner) => {
                let value = self.eval(inner)?;
                if value.is_error() {
                    // `?` returns the error to the caller, widening the row by
                    // union (`[err.propagate]`). A return, not an unwind.
                    self.fire(Rule::ErrPropagate, expr.span, "`?` propagates");
                    return Err(Signal::Return(value));
                }
                self.fire(Rule::ErrUnion, expr.span, "`?` unwraps ok");
                Ok(value)
            }
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                let (Some(start), Some(end)) = (start, end) else {
                    return unsupported(
                        "an open-ended range is only meaningful as a slice endpoint".to_owned(),
                    );
                };
                let start = self.eval(start)?;
                let end = self.eval(end)?;
                match (start, end) {
                    (Value::Int(a, ty), Value::Int(b, _)) => Ok(Value::Range {
                        start: a,
                        end: b,
                        inclusive: *inclusive,
                        ty: if ty.literal { IntTy::INT } else { ty },
                    }),
                    (a, b) => unsupported(format!(
                        "a range needs integer endpoints, got {} and {}",
                        a.kind(),
                        b.kind()
                    )),
                }
            }
            ExprKind::FromEnd(_) => unsupported(
                "`^n` from-end indexing needs the string/collection surface s-tier owns".to_owned(),
            ),
            ExprKind::ElseDefault {
                expr: inner,
                handler,
            } => self.eval_else(inner, handler, expr.span),
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                let value = self.eval(cond)?;
                let Some(taken) = value.as_bool() else {
                    return unsupported(format!(
                        "an `if` condition must be a bool, got {}",
                        value.kind()
                    ));
                };
                self.fire(Rule::Flow, expr.span, "if");
                if taken {
                    self.eval_block(then)
                } else {
                    match otherwise {
                        Some(other) => self.eval(other),
                        None => Ok(Value::Unit),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                let value = self.eval(scrutinee)?;
                for arm in arms {
                    self.push_scope();
                    let matched = self.match_pattern(&arm.pattern, &value)?;
                    let guard = if matched {
                        match &arm.guard {
                            Some(guard) => self.eval(guard)?.as_bool().unwrap_or(false),
                            None => true,
                        }
                    } else {
                        false
                    };
                    if guard {
                        self.fire(Rule::Flow, arm.span, "match arm taken");
                        let result = self.eval(&arm.body);
                        self.pop_scope();
                        return result;
                    }
                    self.pop_scope();
                }
                unsupported(
                    "no `match` arm applied; exhaustiveness is the type checker's".to_owned(),
                )
            }
            ExprKind::For {
                pattern,
                iter,
                body,
            } => self.eval_for(pattern, iter, body, expr.span),
            ExprKind::While { cond, body } => {
                loop {
                    self.step()?;
                    let value = self.eval(cond)?;
                    let Some(taken) = value.as_bool() else {
                        return unsupported(format!(
                            "a `while` condition must be a bool, got {}",
                            value.kind()
                        ));
                    };
                    if !taken {
                        break;
                    }
                    match self.eval_block(body) {
                        Ok(_) | Err(Signal::Continue) => {}
                        Err(Signal::Break(value)) => return Ok(value),
                        Err(other) => return Err(other),
                    }
                }
                Ok(Value::Unit)
            }
            ExprKind::Loop { body } => loop {
                self.step()?;
                match self.eval_block(body) {
                    Ok(_) | Err(Signal::Continue) => {}
                    Err(Signal::Break(value)) => return Ok(value),
                    Err(other) => return Err(other),
                }
            },
            ExprKind::Return(value) => {
                let value = match value {
                    Some(value) => self.eval_for_init(value)?,
                    None => Value::Unit,
                };
                self.fire(Rule::Flow, expr.span, "return");
                Err(Signal::Return(value))
            }
            ExprKind::Break(value) => {
                let value = match value {
                    Some(value) => self.eval(value)?,
                    None => Value::Unit,
                };
                Err(Signal::Break(value))
            }
            ExprKind::Continue => Err(Signal::Continue),
            ExprKind::Closure {
                params,
                body,
                block_bodied: _,
            } => self.eval_closure(params, body, expr.span),

            // -- outside is02's coverage, by name ---------------------------
            ExprKind::RegionSugar { .. } | ExprKind::RegionValue { .. } | ExprKind::In { .. } => {
                unsupported("regions are Tier 1 (`[mem.region]`); is03 extends the value model")
            }
            ExprKind::Freeze(_) => unsupported(
                "`freeze` promotes a region to deep-immutable; Tier 1 arrives with is03",
            ),
            ExprKind::Scope { .. }
            | ExprKind::SpawnProc { .. }
            | ExprKind::Select { .. }
            | ExprKind::When { .. } => {
                unsupported("concurrency is spec/03 and campaign ic03; nothing here schedules")
            }
            ExprKind::Unsafe { .. } | ExprKind::UnsafeC { .. } | ExprKind::Asm { .. } => {
                unsupported("the unsafe tier and its provenance machine are is04")
            }
            ExprKind::Borrow { .. } => {
                unsupported("`borrow r from ptr` is the Tier-3 re-entry door (`[mem.unsafe.door]`)")
            }
        }
    }

    fn eval_interp(&mut self, interp: &Interpolation) -> EResult<String> {
        let value = self.eval(&interp.expr)?;
        let rendered = value.to_string();
        let Some(parts) = &interp.format else {
            return Ok(rendered);
        };
        let mut spec = String::new();
        for part in parts {
            match part {
                crate::ast::FmtPart::Text(text) => spec.push_str(text),
                crate::ast::FmtPart::Interp(expr) => {
                    let value = self.eval(expr)?;
                    spec.push_str(&value.to_string());
                }
            }
        }
        match value::apply_format(&rendered, &spec) {
            Ok(text) => Ok(text),
            Err(reason) => unsupported(reason),
        }
    }

    fn eval_path_expr(&mut self, expr: &Expr) -> EResult<Value> {
        let ExprKind::Path(path) = &*expr.kind else {
            unreachable!("caller checked")
        };
        let head = &path.segments[0].name;
        if self.local_exists(head) || self.globals.contains_key(head) {
            let place = self.place_of(expr)?;
            if self.slot_mut(&place).is_some() {
                return self.read_path(&place, expr.span);
            }
            // A dotted tail that is not a stored field: `xs.len`, `s.len`.
            let mut parent = place.clone();
            let Some(Proj::Field(member)) = parent.projections.pop() else {
                return self.read_path(&place, expr.span);
            };
            let value = self.read_path(&parent, expr.span)?;
            return builtin::property(self, &value, &member, expr.span);
        }

        if path.is_single() {
            let name = head;
            let module = self
                .frames
                .last()
                .map(|f| f.module.clone())
                .unwrap_or_default();
            if let Some(def) = self.program.lookup(&module, name, false) {
                return self.value_of_def(def, &module, name, expr.span);
            }
            if self.program.modules.contains_key(name) {
                return Ok(Value::Module(name.clone()));
            }
            if let Some(builtin) = builtin::ambient(name) {
                return Ok(builtin);
            }
            // An unresolved capitalized name is a structural error tag: D30's
            // rows need no declaration (`[err.rows]`).
            if name.starts_with(char::is_uppercase) {
                self.fire(Rule::ErrRows, expr.span, &format!("error tag `{name}`"));
                return Ok(Value::Error(Box::new(ErrorValue {
                    tag: name.clone(),
                    payload: Vec::new(),
                })));
            }
            return unsupported(format!("`{name}` does not resolve"));
        }

        // A dotted path: module member, or a dotted error tag (`io.Error`).
        let head = path.segments[0].name.clone();
        let tail = path.segments[1].name.clone();
        if self.program.modules.contains_key(&head) {
            return match self.program.lookup(&head, &tail, true) {
                Some(def) => {
                    let def = def.clone();
                    self.value_of_def(&def, &head, &tail, expr.span)
                }
                None => unsupported(format!(
                    "`{head}.{tail}` does not resolve; it is either absent or not `pub` \
                     (`[mod.vis.private]` is the compiler's E0304)"
                )),
            };
        }
        if let Some(builtin) = builtin::ambient_dotted(&head, &tail) {
            return Ok(builtin);
        }
        if tail.starts_with(char::is_uppercase) {
            let tag = format!("{head}.{tail}");
            self.fire(Rule::ErrRows, expr.span, &format!("error tag `{tag}`"));
            return Ok(Value::Error(Box::new(ErrorValue {
                tag,
                payload: Vec::new(),
            })));
        }
        unsupported(format!("`{head}.{tail}` does not resolve"))
    }

    fn value_of_def(&mut self, def: &Def, module: &str, name: &str, span: Span) -> EResult<Value> {
        match def {
            Def::Fn(_) => Ok(Value::Fn(qualify(module, name))),
            Def::Struct(_) => Ok(Value::Fn(qualify(module, name))),
            Def::Binding(binding) => {
                if let Some(slot) = self.globals.get(name) {
                    return Ok(slot.value.clone());
                }
                let binding = binding.clone();
                let value = self.eval(&binding.value)?;
                self.globals
                    .insert(name.to_owned(), Slot::live(value.clone()));
                Ok(value)
            }
            Def::Opaque(what) => unsupported(format!(
                "`{name}` is a {what}; traits, enums and type-level items have no is02 semantics"
            )),
            Def::Ambiguous => {
                let _ = span;
                unsupported(format!(
                    "`{name}` is defined more than once in module `{module}`; dispatch is \
                     ambiguous (the compiler's E0302)"
                ))
            }
        }
    }

    fn eval_unary(&mut self, op: UnOp, operand: &Expr, span: Span) -> EResult<Value> {
        match op {
            UnOp::Copy => {
                // `copy x` produces an independent value from any type
                // (`[mem.tier0.move.3]`) — and does NOT move the source.
                let value = match self.live_place(operand)? {
                    Some(path) => self.read_path(&path, span)?,
                    None => self.eval(operand)?,
                };
                self.fire(Rule::Copy, span, "explicit copy");
                Ok(value)
            }
            UnOp::Move => {
                let path = self.place_of(operand)?;
                self.move_path(&path, span)
            }
            UnOp::Not => match self.eval(operand)? {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                other => unsupported(format!("`!` needs a bool, got {}", other.kind())),
            },
            UnOp::Neg => match self.eval(operand)? {
                Value::Int(v, ty) => self.checked(ty, v.checked_neg(), span, "negation"),
                Value::Float(v) => Ok(Value::Float(-v)),
                other => unsupported(format!("`-` needs a number, got {}", other.kind())),
            },
            UnOp::Borrow | UnOp::BorrowMut => {
                // A local borrow gets the same slot states as a `mut` argument:
                // it holds its path for the extent of its binding
                // (`[mem.tier0.borrow.1]`, `[mem.tier0.borrow.2]`).
                let path = self.place_of(operand)?;
                let access = if op == UnOp::BorrowMut {
                    Access::Exclusive
                } else {
                    Access::Shared
                };
                self.check_access(&path, access, span)?;
                let value = self.read_path(&path, span)?;
                self.fire(
                    if op == UnOp::BorrowMut {
                        Rule::BorrowExtent
                    } else {
                        Rule::Borrow
                    },
                    span,
                    &format!("borrow `{path}` as `{access}`"),
                );
                self.access.push(Held { path, access, span });
                Ok(value)
            }
            UnOp::Deref => unsupported("raw-pointer deref is Tier 3 (`[mem.unsafe.raw]`)"),
            UnOp::Shared => unsupported("`shared` is Tier 2 (`[mem.shared.rc]`); is03 owns it"),
        }
    }

    fn checked(
        &mut self,
        ty: IntTy,
        computed: Option<i128>,
        span: Span,
        what: &str,
    ) -> EResult<Value> {
        let Some(raw) = computed else {
            return self.trap(
                TrapKind::Overflow,
                Rule::ArithChecked,
                span,
                format!("{what} overflowed the machine's widest integer"),
                None,
            );
        };
        match ty.reduce(raw) {
            Some(value) => {
                if ty.mode != ArithMode::Checked {
                    self.fire(
                        Rule::ArithWrapping,
                        span,
                        &format!("{what} in {}", ty.name()),
                    );
                }
                Ok(Value::Int(value, ty))
            }
            None => self.trap(
                TrapKind::Overflow,
                Rule::ArithChecked,
                span,
                format!(
                    "{what} produced {raw}, outside `{}` — checked arithmetic traps in every \
                     profile (X3); spell intended overflow `wrapping[{}]`",
                    ty.name(),
                    ty.name()
                ),
                None,
            ),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn binary(&mut self, op: BinOp, left: Value, right: Value, span: Span) -> EResult<Value> {
        use BinOp::{
            Add, BitAnd, BitOr, BitXor, Cmp, Div, Eq, Ge, Gt, Le, Lt, Mul, Ne, Rem, Shl, Shr, Sub,
        };

        // Equality is over *values*, never over the type a value carries: an
        // `i32` 3 and an untyped literal 3 are the same value
        // (`[mem.model.value]` — "values have no identity beyond their current
        // place", and certainly none in their width).
        match op {
            Eq => return Ok(Value::Bool(value_eq(&left, &right))),
            Ne => return Ok(Value::Bool(!value_eq(&left, &right))),
            _ => {}
        }

        // Strings compare and concatenate.
        if let (Value::Str(a), Value::Str(b)) = (&left, &right) {
            return match op {
                Add => Ok(Value::Str(format!("{a}{b}"))),
                Lt => Ok(Value::Bool(a < b)),
                Le => Ok(Value::Bool(a <= b)),
                Gt => Ok(Value::Bool(a > b)),
                Ge => Ok(Value::Bool(a >= b)),
                Cmp => Ok(Value::int(i128::from(a.cmp(b) as i8))),
                _ => unsupported(format!("`{}` is not defined on two strings", spelling(op))),
            };
        }

        if let (Value::Float(a), Value::Float(b)) = (&left, &right) {
            let (a, b) = (*a, *b);
            return match op {
                Add => Ok(Value::Float(a + b)),
                Sub => Ok(Value::Float(a - b)),
                Mul => Ok(Value::Float(a * b)),
                Div => Ok(Value::Float(a / b)),
                Rem => Ok(Value::Float(a % b)),
                Lt => Ok(Value::Bool(a < b)),
                Le => Ok(Value::Bool(a <= b)),
                Gt => Ok(Value::Bool(a > b)),
                Ge => Ok(Value::Bool(a >= b)),
                _ => unsupported(format!("`{}` is not defined on floats", spelling(op))),
            };
        }

        let (Value::Int(a, ta), Value::Int(b, tb)) = (&left, &right) else {
            return unsupported(format!(
                "`{}` is not defined on {} and {}; operand types are the checker's",
                spelling(op),
                left.kind(),
                right.kind()
            ));
        };
        let (a, b) = (*a, *b);
        // Literal defaulting and checking-context propagation
        // (`[arith.literal.default]`): an unconstrained literal adopts the other
        // operand's type; two literals default to i32.
        let ty = match (ta.literal, tb.literal) {
            (true, false) => *tb,
            (false, true) | (false, false) => *ta,
            (true, true) => IntTy::I32,
        };
        if ta.literal && tb.literal {
            self.fire(Rule::LiteralDefault, span, "literals default to i32");
        }

        match op {
            Lt => return Ok(Value::Bool(a < b)),
            Le => return Ok(Value::Bool(a <= b)),
            Gt => return Ok(Value::Bool(a > b)),
            Ge => return Ok(Value::Bool(a >= b)),
            Cmp => return Ok(Value::int(i128::from(a.cmp(&b) as i8))),
            _ => {}
        }

        if matches!(op, Div | Rem) && b == 0 {
            // Defined behavior, not UB (`[mem.ub.defined]`).
            return self.trap(
                TrapKind::DivZero,
                Rule::DivZero,
                span,
                "division by zero is defined behavior in wolf: it traps",
                None,
            );
        }

        let computed = match op {
            Add => a.checked_add(b),
            Sub => a.checked_sub(b),
            Mul => a.checked_mul(b),
            Div => a.checked_div(b),
            Rem => a.checked_rem(b),
            BitAnd => Some(a & b),
            BitOr => Some(a | b),
            BitXor => Some(a ^ b),
            Shl => u32::try_from(b).ok().and_then(|s| a.checked_shl(s)),
            Shr => u32::try_from(b).ok().and_then(|s| a.checked_shr(s)),
            _ => unreachable!("handled above"),
        };
        self.checked(ty, computed, span, &format!("`{}`", spelling(op)))
    }

    fn eval_else(&mut self, inner: &Expr, handler: &ElseHandler, span: Span) -> EResult<Value> {
        let value = self.eval(inner)?;
        if !value.is_error() {
            return Ok(value);
        }
        self.fire(Rule::ErrElse, span, "`else` defaults the error away");
        match handler {
            ElseHandler::Expr(expr) => self.eval(expr),
            ElseHandler::Block(block) => self.eval_block(block),
            ElseHandler::Handler { pattern, body } => {
                self.push_scope();
                let matched = self.match_pattern(pattern, &value);
                let result = match matched {
                    Ok(true) => self.eval(body),
                    Ok(false) => unsupported(
                        "the `else |pat|` pattern did not match the error value".to_owned(),
                    ),
                    Err(signal) => Err(signal),
                };
                self.pop_scope();
                result
            }
        }
    }

    fn eval_for(
        &mut self,
        pattern: &Pattern,
        iter: &Expr,
        body: &Block,
        span: Span,
    ) -> EResult<Value> {
        let iterable = self.eval(iter)?;
        let items: Vec<Value> = match iterable {
            Value::Range {
                start,
                end,
                inclusive,
                ty,
            } => {
                let last = if inclusive { end } else { end - 1 };
                let mut out = Vec::new();
                let mut at = start;
                while at <= last {
                    out.push(Value::Int(at, ty));
                    at += 1;
                }
                out
            }
            Value::List(slots) => slots.into_iter().map(|s| s.value).collect(),
            Value::Map(pairs) => pairs
                .into_iter()
                .map(|(key, slot)| Value::Tuple(vec![Slot::live(key), slot]))
                .collect(),
            other => {
                return unsupported(format!("`for` cannot iterate {}", other.kind()));
            }
        };

        self.fire(Rule::Flow, span, "for");
        for item in items {
            self.step()?;
            self.push_scope();
            let bound = self.bind_pattern(pattern, item);
            let result = match bound {
                Ok(()) => self.eval_block(body),
                Err(signal) => Err(signal),
            };
            self.pop_scope();
            match result {
                Ok(_) | Err(Signal::Continue) => {}
                Err(Signal::Break(value)) => return Ok(value),
                Err(other) => return Err(other),
            }
        }
        Ok(Value::Unit)
    }

    fn eval_closure(&mut self, params: &[ClosureParam], body: &Expr, span: Span) -> EResult<Value> {
        // Captured by value at construction (`[gram.expr.closure]`, D10 Tier 0:
        // there is nothing to capture by reference).
        let mut captures = Vec::new();
        if let Some(frame) = self.frames.last() {
            for scope in &frame.scopes {
                for (name, slot) in &scope.locals {
                    if slot.is_live() {
                        captures.push((name.clone(), slot.value.clone()));
                    }
                }
            }
        }
        self.fire(Rule::Closure, span, "closure captures by value");
        Ok(Value::Closure(Box::new(ClosureValue {
            params: params.iter().map(|p| p.name.name.clone()).collect(),
            body: body.clone(),
            captures,
        })))
    }

    fn match_pattern(&mut self, pattern: &Pattern, value: &Value) -> EResult<bool> {
        match &*pattern.kind {
            PatKind::Wildcard => Ok(true),
            PatKind::Binding(ident) => {
                self.declare(&ident.name, Slot::live(value.clone()));
                Ok(true)
            }
            PatKind::Literal(expr) => {
                let literal = self.eval(expr)?;
                Ok(match (&literal, value) {
                    (Value::Int(a, _), Value::Int(b, _)) => a == b,
                    _ => literal == *value,
                })
            }
            PatKind::Path(path) => {
                let tag = path
                    .segments
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                Ok(matches!(value, Value::Error(e) if e.tag == tag && e.payload.is_empty()))
            }
            PatKind::Variant { path, fields } => {
                let tag = path
                    .segments
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let Value::Error(err) = value else {
                    return Ok(false);
                };
                if err.tag != tag || err.payload.len() != fields.len() {
                    return Ok(false);
                }
                let payload = err.payload.clone();
                for (sub, item) in fields.iter().zip(payload) {
                    if !self.match_pattern(sub, &item)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            PatKind::Tuple(items) => {
                let Value::Tuple(slots) = value else {
                    return Ok(false);
                };
                if slots.len() != items.len() {
                    return Ok(false);
                }
                let values: Vec<Value> = slots.iter().map(|s| s.value.clone()).collect();
                for (sub, item) in items.iter().zip(values) {
                    if !self.match_pattern(sub, &item)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            PatKind::At { name, pattern } => {
                if self.match_pattern(pattern, value)? {
                    self.declare(&name.name, Slot::live(value.clone()));
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            PatKind::Or(alternatives) => {
                for alternative in alternatives {
                    if self.match_pattern(alternative, value)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    // -- calls, members, indexing -----------------------------------------

    fn eval_call(&mut self, callee: &Expr, args: &[Arg], span: Span) -> EResult<Value> {
        // A method call is a call whose callee projects a member out of a
        // *value*. `[gram.item.use]`'s `path` production swallows the dots, so
        // `xs.push` and `geometry.area` arrive in the same shape and are told
        // apart by whether the head names a local or a module.
        if let Some((receiver, method)) = self.method_split(callee) {
            return self.eval_method(&receiver, &method, args, span);
        }

        let target = self.eval(callee)?;
        let evaluated = self.eval_args(args)?;
        let result = self.apply(target, evaluated.values.clone(), span);
        let finals = match &result {
            Ok(applied) => applied.params.clone(),
            Err(_) => evaluated.values,
        };
        self.finish_args(&evaluated.writebacks, &finals, evaluated.held, span);
        result.map(|applied| applied.value)
    }

    /// Splits a callee into `(receiver, method)` when it is a method call.
    fn method_split(&mut self, callee: &Expr) -> Option<(Receiver, String)> {
        match &*callee.kind {
            ExprKind::Member {
                base,
                member: Member::Named(name),
            } if !self.is_module_expr(base) => Some((
                match self.place_of(base) {
                    Ok(path) => Receiver::Place(path),
                    Err(_) => Receiver::Expr(base.clone()),
                },
                name.name.clone(),
            )),
            ExprKind::Path(path) if path.segments.len() >= 2 => {
                let head = &path.segments[0].name;
                if !self.local_exists(head) && !self.globals.contains_key(head) {
                    return None;
                }
                let mut place = self.place_of(callee).ok()?;
                let Some(Proj::Field(method)) = place.projections.pop() else {
                    return None;
                };
                Some((Receiver::Place(place), method))
            }
            _ => None,
        }
    }

    fn is_module_expr(&self, expr: &Expr) -> bool {
        match &*expr.kind {
            ExprKind::Path(path) if path.is_single() => {
                let name = &path.segments[0].name;
                !self.local_exists(name) && self.program.modules.contains_key(name)
            }
            _ => false,
        }
    }

    fn apply(&mut self, target: Value, args: Vec<Value>, span: Span) -> EResult<Applied> {
        let unchanged = args.clone();
        let plain = move |value: Value| Applied {
            value,
            params: unchanged,
        };
        match target {
            Value::Fn(qualified) => {
                let (module, name) = split_qualified(&qualified);
                match self.program.lookup(&module, &name, false).cloned() {
                    Some(Def::Fn(decl)) => self.call_fn(&decl, &module, args, span),
                    Some(Def::Struct(def)) => {
                        // A tuple-shaped constructor call. The corpus builds
                        // structs with literals, so this is only reached by a
                        // program the type checker would have caught.
                        let _ = def;
                        unsupported(format!(
                            "`{name}` is a type; build it with a struct literal"
                        ))
                    }
                    _ => unsupported(format!("`{qualified}` is not callable")),
                }
            }
            Value::Closure(closure) => {
                if closure.params.len() != args.len() {
                    return unsupported(format!(
                        "a closure of {} parameter(s) was called with {}",
                        closure.params.len(),
                        args.len()
                    ));
                }
                self.frames.push(Frame {
                    module: self
                        .frames
                        .last()
                        .map(|f| f.module.clone())
                        .unwrap_or_default(),
                    scopes: vec![Scope::default()],
                });
                for (name, value) in &closure.captures {
                    self.declare(name, Slot::live(value.clone()));
                }
                for (name, value) in closure.params.iter().zip(args) {
                    self.declare(name, Slot::live(value));
                }
                let result = self.eval(&closure.body);
                let frame = self.frame();
                self.access.release_frame(frame);
                self.frames.pop();
                match result {
                    Ok(value) | Err(Signal::Return(value)) => Ok(plain(value)),
                    Err(other) => Err(other),
                }
            }
            Value::Error(tag) if tag.payload.is_empty() => {
                // `BadDigit(payload)` — a structural row entry applied to its
                // payload (`[err.rows]`).
                self.fire(
                    Rule::ErrRows,
                    span,
                    &format!("error `{}` with payload", tag.tag),
                );
                Ok(plain(Value::Error(Box::new(ErrorValue {
                    tag: tag.tag,
                    payload: args,
                }))))
            }
            Value::Builtin(name) => builtin::call(self, name, args, span).map(plain),
            other => unsupported(format!("{} is not callable", other.kind())),
        }
    }

    fn eval_method(
        &mut self,
        receiver: &Receiver,
        method: &str,
        args: &[Arg],
        span: Span,
    ) -> EResult<Value> {
        let (path, value) = match receiver {
            Receiver::Place(path) if self.slot_mut(path).is_some() => {
                let value = self.read_path(path, span)?;
                (Some(path.clone()), value)
            }
            Receiver::Place(path) => {
                let display = path.to_string();
                return unsupported(format!("`{display}` does not denote a place at run time"));
            }
            Receiver::Expr(expr) => (None, self.eval(expr)?),
        };
        let evaluated = self.eval_args(args)?;
        let mut receiver_value = value;
        let result = builtin::method(
            self,
            &mut receiver_value,
            method,
            evaluated.values.clone(),
            span,
        );
        self.finish_args(
            &evaluated.writebacks,
            &evaluated.values,
            evaluated.held,
            span,
        );
        // A mutating method writes its receiver back — value semantics, so the
        // observable effect is a whole-value store into the receiver's place.
        if let (Some(path), Ok(_)) = (&path, &result) {
            self.write_path(path, receiver_value, span)?;
        }
        result
    }

    fn eval_member(&mut self, base: &Expr, member: &Member, span: Span) -> EResult<Value> {
        if self.is_module_expr(base)
            && let (ExprKind::Path(path), Member::Named(name)) = (&*base.kind, member)
        {
            let module = path.segments[0].name.clone();
            return match self.program.lookup(&module, &name.name, true).cloned() {
                Some(def) => self.value_of_def(&def, &module, &name.name, span),
                None => unsupported(format!(
                    "`{module}.{}` does not resolve; it is either absent or not `pub`",
                    name.name
                )),
            };
        }

        if let Ok(path) = self.place_of(base) {
            let projected = match member {
                Member::Named(ident) => path.clone().project(Proj::Field(ident.name.clone())),
                Member::Index(index, _) => path.clone().project(Proj::Index(i128::from(*index))),
            };
            if self.slot_mut(&projected).is_some() {
                return self.read_path(&projected, span);
            }
        }

        let value = self.eval(base)?;
        match member {
            Member::Named(ident) => builtin::property(self, &value, &ident.name, span),
            Member::Index(index, _) => match &value {
                Value::Tuple(slots) => {
                    match slots.get(usize::try_from(*index).unwrap_or(usize::MAX)) {
                        Some(slot) => Ok(slot.value.clone()),
                        None => self.trap(
                            TrapKind::Bounds,
                            Rule::Bounds,
                            span,
                            format!("tuple has no element {index}"),
                            None,
                        ),
                    }
                }
                other => unsupported(format!("{} has no numbered members", other.kind())),
            },
        }
    }

    fn eval_bracket(&mut self, base: &Expr, args: &[IndexArg], span: Span) -> EResult<Value> {
        let target = self.eval(base)?;

        // `e[…]` is one production (`[gram.amb.brackets]`): generic application
        // or indexing, told apart here by what the base turned out to be.
        if matches!(target, Value::Builtin(_) | Value::Fn(_))
            || args.iter().any(|a| matches!(a, IndexArg::Type(_)))
        {
            return Ok(target);
        }

        let [IndexArg::Value(arg)] = args else {
            return unsupported("indexing takes exactly one argument".to_owned());
        };
        // A range in index position is a *slice*, and its endpoints are
        // optional (`s[..8]`, `s[4..]`) — which no `Value` shape carries, so
        // the range is read off the syntax here rather than evaluated to one.
        if let ExprKind::Range {
            start,
            end,
            inclusive,
        } = &*arg.expr.kind
        {
            let start = match start {
                Some(expr) => Some(self.eval_index_endpoint(expr)?),
                None => None,
            };
            let end = match end {
                Some(expr) => Some(self.eval_index_endpoint(expr)?),
                None => None,
            };
            return builtin::slice(self, &target, start, end, *inclusive, span);
        }
        let index = self.eval(&arg.expr)?;
        builtin::index(self, &target, &index, span)
    }

    fn eval_index_endpoint(&mut self, expr: &Expr) -> EResult<i128> {
        match self.eval(expr)? {
            Value::Int(v, _) => Ok(v),
            other => Err(Signal::Unsupported(format!(
                "a slice endpoint must be an integer, got {}",
                other.kind()
            ))),
        }
    }

    // -- the machine's own doors, used by `builtin` ------------------------

    pub(crate) fn out(&mut self, text: &str) {
        self.write_out(text);
    }

    pub(crate) fn fault<T>(
        &mut self,
        kind: TrapKind,
        rule: Rule,
        span: Span,
        message: impl Into<String>,
    ) -> EResult<T> {
        self.trap(kind, rule, span, message, None)
    }

    pub(crate) fn note(&mut self, rule: Rule, span: Span, detail: &str) {
        self.fire(rule, span, detail);
    }

    pub(crate) fn invoke(&mut self, target: Value, args: Vec<Value>, span: Span) -> EResult<Value> {
        self.apply(target, args, span).map(|applied| applied.value)
    }
}

// -- helpers ---------------------------------------------------------------

/// The first `depth` projections of a path, rendered — the place that actually
/// moved when a read of a deeper place traps.
fn prefix(path: &Path, depth: usize) -> Path {
    let mut prefix = path.clone();
    prefix.projections.truncate(depth);
    prefix
}

/// A binary operator as the programmer wrote it. Fault messages quote source,
/// never a Rust variant name.
fn spelling(op: BinOp) -> &'static str {
    match op {
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::BitAnd => "&",
        BinOp::BitXor => "^",
        BinOp::BitOr => "|",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::Cmp => "<=>",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

/// Structural value equality, blind to the width an integer is carrying.
pub(crate) fn value_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Int(a, _), Value::Int(b, _)) => a == b,
        (Value::Tuple(a) | Value::List(a), Value::Tuple(b) | Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| value_eq(&x.value, &y.value))
        }
        (
            Value::Struct {
                name: an,
                fields: af,
            },
            Value::Struct {
                name: bn,
                fields: bf,
            },
        ) => {
            an == bn
                && af.len() == bf.len()
                && af
                    .iter()
                    .zip(bf)
                    .all(|((n, x), (m, y))| n == m && value_eq(&x.value, &y.value))
        }
        (Value::Error(a), Value::Error(b)) => {
            a.tag == b.tag
                && a.payload.len() == b.payload.len()
                && a.payload
                    .iter()
                    .zip(&b.payload)
                    .all(|(x, y)| value_eq(x, y))
        }
        (a, b) => a == b,
    }
}

fn qualify(module: &str, name: &str) -> String {
    if module.is_empty() {
        name.to_owned()
    } else {
        format!("{module}::{name}")
    }
}

fn split_qualified(qualified: &str) -> (String, String) {
    match qualified.split_once("::") {
        Some((module, name)) => (module.to_owned(), name.to_owned()),
        None => (String::new(), qualified.to_owned()),
    }
}

/// Are values of this shape `Copy` (`[mem.tier0.move.3]`: "POD-shaped types
/// only")?
///
/// Scalars and `str` are; aggregates are not. `str` is the judgement call: it
/// owns bytes, but wolf strings are immutable views (D25) and treating them as
/// non-`Copy` would make the dynamic machine *stricter* than the compiler,
/// which is the one direction the sprint forbids.
fn is_copy(value: &Value) -> bool {
    matches!(
        value,
        Value::Unit
            | Value::Bool(_)
            | Value::Int(..)
            | Value::Float(_)
            | Value::Str(_)
            | Value::Range { .. }
            | Value::Fn(_)
            | Value::Module(_)
            | Value::Builtin(_)
    )
}

/// Applies a declared type to a value: the checking context sema-lite provides.
fn coerce(value: Value, ty: Option<&Type>) -> Value {
    let Some(ty) = ty else { return value };
    match (&*ty.kind, value) {
        (TypeKind::Path { path, args }, Value::Int(v, current)) => {
            let name = path.segments.last().map(|s| s.name.as_str()).unwrap_or("");
            let mode = match name {
                "wrapping" => Some(ArithMode::Wrapping),
                "saturating" => Some(ArithMode::Saturating),
                _ => None,
            };
            if let Some(mode) = mode {
                let inner = args.iter().find_map(|arg| match arg {
                    TypeArg::Type(inner) => match &*inner.kind {
                        TypeKind::Path { path, .. } => {
                            IntTy::named(path.segments.last().map_or("", |s| s.name.as_str()))
                        }
                        _ => None,
                    },
                    TypeArg::Expr(_) => None,
                });
                let base = inner.unwrap_or(IntTy::INT);
                return Value::Int(v, base.with_mode(mode));
            }
            match IntTy::named(name) {
                Some(ty) => Value::Int(v, ty),
                None => Value::Int(v, current),
            }
        }
        (TypeKind::Path { path, .. }, Value::Float(v)) => {
            let _ = path;
            Value::Float(v)
        }
        (TypeKind::ErrorUnion(inner), value) => coerce(value, Some(inner)),
        (_, value) => value,
    }
}

fn parse_int(text: &str) -> EResult<i128> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    let (radix, digits) = if let Some(rest) = cleaned.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = cleaned.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = cleaned.strip_prefix("0b") {
        (2, rest)
    } else {
        (10, cleaned.as_str())
    };
    i128::from_str_radix(digits, radix)
        .map_err(|_| Signal::Unsupported(format!("integer literal `{text}` does not fit in i128")))
}

fn parse_float(text: &str) -> EResult<f64> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned
        .parse()
        .map_err(|_| Signal::Unsupported(format!("float literal `{text}` does not parse")))
}

/// Runs one program from source, with no filesystem module graph.
///
/// # Errors
///
/// The load error, when the source does not lex or parse.
pub fn run_source(name: &str, source: &str) -> Result<Run, crate::sema::LoadError> {
    let program = crate::sema::load_source(name, source)?;
    Ok(Machine::new(&program).run())
}

#[cfg(test)]
mod tests;
