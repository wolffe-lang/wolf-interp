//! Fuzzer-driven generation for the differential runner (is05 §3).
//!
//! A grammar-aware program generator seeded from `spec/grammar.ebnf`'s
//! surface, with a semantic-plausibility layer — declared-before-use,
//! mode-correct call sites, arity-correct calls — so generated programs
//! mostly clear sema on *both* sides. Two modes (the csmith/rustlantis
//! posture):
//!
//! - [`Mode::Defined`] — **defined-by-construction**: safe-tier, int-typed
//!   throughout, no overflow reachable (small operands, literal nonzero
//!   divisors, bounded loops), deterministic output printed at the end,
//!   `main` returns 0. Any divergence — or, counterparty-less, any interp
//!   outcome other than `exit(0)` — indicts an implementation directly.
//! - [`Mode::Boundary`] — **boundary-poking**: moves (including in loops),
//!   `region`/`in`/`freeze` sequences, `mut`/`take` call-site modes,
//!   shadowing, nested blocks. Programs may legitimately trap; the
//!   interpreter's verdict defines expected behavior, and an unsafe-free
//!   program must still never produce `ub(*)`.
//!
//! Determinism is a contract: the PRNG is SplitMix64 over explicit `u64`
//! state, program text is a pure function of `(seed, index, mode)`, and no
//! platform-varying input (paths, pointer values, map iteration order) feeds
//! generation — the path-hash lesson from is01, kept.
//!
//! Reduction is AST-aware (treereduce posture): the generator keeps its tree,
//! and [`reduce`] shrinks *the tree* — drop statements, unwrap blocks,
//! simplify expressions to literals — re-checking the caller's predicate at
//! every step, so a divergent case files as a minimal reproducer rather than
//! a haystack.

use std::fmt::Write as _;

// ---------------------------------------------------------------------------
// Deterministic PRNG
// ---------------------------------------------------------------------------

/// SplitMix64 — tiny, deterministic, platform-stable. Not for cryptography;
/// for replayable fuzz campaigns.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    /// The per-program seed for one campaign index: mixed, not additive, so
    /// neighboring indices share no low-bit structure.
    #[must_use]
    pub fn for_case(campaign_seed: u64, index: u64) -> Rng {
        let mut rng = Rng(campaign_seed ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let _ = rng.next_u64();
        rng
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (n > 0).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// True with probability `num/den`.
    pub fn chance(&mut self, num: u64, den: u64) -> bool {
        self.below(den) < num
    }
}

// ---------------------------------------------------------------------------
// The generator's tree
// ---------------------------------------------------------------------------

/// Generation mode; see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Defined,
    Boundary,
}

impl Mode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Defined => "defined",
            Mode::Boundary => "boundary",
        }
    }
}

/// An int-valued expression. Small on purpose: every shape here is one both
/// implementations' frontends must take, and the *combinations* do the work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GExpr {
    Lit(i64),
    Var(String),
    /// `a <op> b` with defined-mode-safe operands.
    Bin(&'static str, Box<GExpr>, Box<GExpr>),
    /// `(cond) if-else as expression`: `if c { a } else { b }`.
    IfVal(Box<GExpr>, Box<GExpr>, Box<GExpr>),
    /// A call to a generated helper `fK(args…)`.
    Call(String, Vec<GExpr>),
    /// A comparison, usable only where a condition is wanted.
    Cmp(&'static str, Box<GExpr>, Box<GExpr>),
    /// `(e)` — grouping the differ has to parse identically.
    Group(Box<GExpr>),
}

/// One statement of a generated block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GStmt {
    Let {
        name: String,
        value: GExpr,
    },
    Var {
        name: String,
        value: GExpr,
    },
    Assign {
        name: String,
        value: GExpr,
    },
    /// `print("… {expr} …")` — f-string interpolation is in every literal.
    Print {
        prefix: String,
        expr: GExpr,
    },
    If {
        cond: GExpr,
        then: Vec<GStmt>,
        otherwise: Vec<GStmt>,
    },
    /// A bounded counting loop: `var i = 0` … `while i < n { body; i = i + 1 }`.
    While {
        counter: String,
        bound: i64,
        body: Vec<GStmt>,
    },
    /// A bare block — scoping, shadowing.
    Block(Vec<GStmt>),
    /// Boundary mode: `region r { … }`.
    Region {
        name: String,
        body: Vec<GStmt>,
    },
    /// Boundary mode: a move chain — `let b = a` then optional re-init.
    Move {
        from: String,
        to: String,
        reinit: bool,
    },
    /// Boundary mode: a mode-correct call to a `mut` helper: `bump(mut x)`.
    CallMut {
        func: String,
        target: String,
    },
}

/// A generated helper function: int parameters, int result, body is an
/// expression over its parameters (defined mode) — always total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GFunc {
    pub name: String,
    pub params: Vec<String>,
    pub body: GExpr,
    /// A `mut` first parameter (boundary mode): the body may assign to it.
    pub mutates: bool,
}

/// One generated program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GProgram {
    pub mode: Mode,
    pub funcs: Vec<GFunc>,
    pub main: Vec<GStmt>,
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// The generation context: which names are live (declared and not moved),
/// which are `var`, which helpers exist.
struct Ctx {
    live: Vec<(String, bool)>, // (name, is_var)
    moved: Vec<String>,
    funcs: Vec<(String, usize, bool)>, // (name, arity, mutates)
    next_name: usize,
    depth: usize,
}

impl Ctx {
    fn fresh(&mut self, prefix: &str) -> String {
        let name = format!("{prefix}{}", self.next_name);
        self.next_name += 1;
        name
    }

    fn live_names(&self) -> Vec<String> {
        self.live
            .iter()
            .filter(|(n, _)| !self.moved.contains(n))
            .map(|(n, _)| n.clone())
            .collect()
    }

    fn var_names(&self) -> Vec<String> {
        self.live
            .iter()
            .filter(|(n, is_var)| *is_var && !self.moved.contains(n))
            .map(|(n, _)| n.clone())
            .collect()
    }
}

/// Generates one program from one seeded rng.
#[must_use]
pub fn generate(rng: &mut Rng, mode: Mode) -> GProgram {
    let mut ctx = Ctx {
        live: Vec::new(),
        moved: Vec::new(),
        funcs: Vec::new(),
        next_name: 0,
        depth: 0,
    };

    // Helpers first: declared-before-use is the plausibility layer's rule 1.
    let mut funcs = Vec::new();
    for _ in 0..rng.below(3) {
        let arity = 1 + rng.below(2) as usize;
        let name = format!("f{}", funcs.len());
        let params: Vec<String> = (0..arity).map(|i| format!("p{i}")).collect();
        let mutates = mode == Mode::Boundary && arity == 1 && rng.chance(1, 3);
        // Function bodies obey the defined-mode arithmetic guard in *both*
        // modes: arguments can be loop-amplified values, and a trap inside a
        // helper would make "defined by construction" a lie in the mode that
        // promises it.
        let body = gen_expr(rng, &params, &ctx.funcs, 2, true);
        ctx.funcs.push((name.clone(), arity, mutates));
        funcs.push(GFunc {
            name,
            params,
            body,
            mutates,
        });
    }

    let stmt_count = 2 + rng.below(6);
    let mut main = Vec::new();
    // Seed one binding so every later production has a variable to reach for.
    let seed_name = ctx.fresh("x");
    main.push(GStmt::Let {
        name: seed_name.clone(),
        value: GExpr::Lit(rng.below(50) as i64),
    });
    ctx.live.push((seed_name, false));

    for _ in 0..stmt_count {
        main.push(gen_stmt(rng, &mut ctx, mode));
    }

    // Deterministic output at the end: print an expression over whatever is
    // still live, through an interpolated literal.
    let live = ctx.live_names();
    let expr = if live.is_empty() {
        GExpr::Lit(7)
    } else {
        GExpr::Var(live[rng.below(live.len() as u64) as usize].clone())
    };
    main.push(GStmt::Print {
        prefix: "out".to_owned(),
        expr,
    });

    GProgram { mode, funcs, main }
}

fn gen_expr(
    rng: &mut Rng,
    names: &[String],
    funcs: &[(String, usize, bool)],
    depth: usize,
    defined: bool,
) -> GExpr {
    if depth == 0 {
        return leaf(rng, names);
    }
    match rng.below(8) {
        0..=1 => leaf(rng, names),
        2..=4 => {
            // Defined-mode-safe arithmetic: `/` and `%` only by a literal
            // nonzero divisor; `*` only between literal leaves — additive
            // growth is bounded by the loop caps, multiplicative growth over
            // loop-amplified variables is how checked `int` overflows.
            let op = match rng.below(5) {
                0 => "+",
                1 => "-",
                2 => "*",
                3 => "/",
                _ => "%",
            };
            let (lhs, rhs) = if op == "/" || op == "%" {
                (
                    gen_expr(rng, names, funcs, depth - 1, defined),
                    GExpr::Lit(1 + rng.below(9) as i64),
                )
            } else if op == "*" && defined {
                (
                    GExpr::Lit(rng.below(10) as i64),
                    GExpr::Lit(rng.below(10) as i64),
                )
            } else {
                (
                    gen_expr(rng, names, funcs, depth - 1, defined),
                    gen_expr(rng, names, funcs, depth - 1, defined),
                )
            };
            GExpr::Bin(op, Box::new(lhs), Box::new(rhs))
        }
        5 => GExpr::IfVal(
            Box::new(gen_cond(rng, names, funcs, depth - 1, defined)),
            Box::new(gen_expr(rng, names, funcs, depth - 1, defined)),
            Box::new(gen_expr(rng, names, funcs, depth - 1, defined)),
        ),
        6 if !funcs.is_empty() => {
            let (name, arity, mutates) = &funcs[rng.below(funcs.len() as u64) as usize];
            if *mutates {
                // A `mut` helper is a statement's business, not an expression's.
                leaf(rng, names)
            } else {
                let args = (0..*arity)
                    .map(|_| gen_expr(rng, names, funcs, depth.saturating_sub(2), defined))
                    .collect();
                GExpr::Call(name.clone(), args)
            }
        }
        _ => GExpr::Group(Box::new(gen_expr(rng, names, funcs, depth - 1, defined))),
    }
}

fn gen_cond(
    rng: &mut Rng,
    names: &[String],
    funcs: &[(String, usize, bool)],
    depth: usize,
    defined: bool,
) -> GExpr {
    let op = match rng.below(4) {
        0 => "<",
        1 => ">",
        2 => "==",
        _ => "!=",
    };
    GExpr::Cmp(
        op,
        Box::new(gen_expr(rng, names, funcs, depth, defined)),
        Box::new(gen_expr(rng, names, funcs, depth, defined)),
    )
}

fn leaf(rng: &mut Rng, names: &[String]) -> GExpr {
    if !names.is_empty() && rng.chance(1, 2) {
        GExpr::Var(names[rng.below(names.len() as u64) as usize].clone())
    } else {
        GExpr::Lit(rng.below(50) as i64)
    }
}

fn gen_stmt(rng: &mut Rng, ctx: &mut Ctx, mode: Mode) -> GStmt {
    let names = ctx.live_names();
    let funcs = ctx.funcs.clone();
    let boundary = mode == Mode::Boundary;
    let defined = mode == Mode::Defined;
    let choice = rng.below(if boundary { 10 } else { 7 });
    match choice {
        0..=1 => {
            let name = ctx.fresh("x");
            let value = gen_expr(rng, &names, &funcs, 3, defined);
            ctx.live.push((name.clone(), false));
            GStmt::Let { name, value }
        }
        2 => {
            let name = ctx.fresh("v");
            let value = gen_expr(rng, &names, &funcs, 2, defined);
            ctx.live.push((name.clone(), true));
            GStmt::Var { name, value }
        }
        3 => {
            let vars = ctx.var_names();
            if vars.is_empty() {
                let name = ctx.fresh("x");
                let value = gen_expr(rng, &names, &funcs, 2, defined);
                ctx.live.push((name.clone(), false));
                GStmt::Let { name, value }
            } else {
                let name = vars[rng.below(vars.len() as u64) as usize].clone();
                GStmt::Assign {
                    name,
                    value: gen_expr(rng, &names, &funcs, 2, defined),
                }
            }
        }
        4 if ctx.depth < 2 => {
            ctx.depth += 1;
            let cond = gen_cond(rng, &names, &funcs, 1, defined);
            // Declarations inside a branch are scoped to it: snapshot the
            // live set, or later statements would reach for names the
            // rendered program no longer has (declared-before-use, rule 1).
            let visible = ctx.live.len();
            let then = vec![gen_stmt(rng, ctx, mode)];
            ctx.live.truncate(visible);
            let otherwise = if rng.chance(1, 2) {
                let out = vec![gen_stmt(rng, ctx, mode)];
                ctx.live.truncate(visible);
                out
            } else {
                Vec::new()
            };
            ctx.depth -= 1;
            GStmt::If {
                cond,
                then,
                otherwise,
            }
        }
        5 if ctx.depth < 2 => {
            ctx.depth += 1;
            let counter = ctx.fresh("i");
            let visible = ctx.live.len();
            let body = vec![gen_stmt(rng, ctx, mode)];
            ctx.live.truncate(visible);
            ctx.depth -= 1;
            GStmt::While {
                counter,
                bound: 1 + rng.below(6) as i64,
                body,
            }
        }
        6 if ctx.depth < 2 => {
            ctx.depth += 1;
            let visible = ctx.live.len();
            let inner = vec![gen_stmt(rng, ctx, mode), gen_stmt(rng, ctx, mode)];
            ctx.live.truncate(visible);
            ctx.depth -= 1;
            GStmt::Block(inner)
        }
        7 if boundary && ctx.depth < 2 => {
            ctx.depth += 1;
            let name = ctx.fresh("r");
            let visible = ctx.live.len();
            let body = vec![gen_stmt(rng, ctx, mode)];
            ctx.live.truncate(visible);
            ctx.depth -= 1;
            GStmt::Region { name, body }
        }
        8 if boundary && !names.is_empty() => {
            let from = names[rng.below(names.len() as u64) as usize].clone();
            let to = ctx.fresh("m");
            let reinit = rng.chance(2, 3);
            // Ints are Copy-shaped, so a "move" of one is a copy and the
            // source stays live either way; the *syntax* is the point here.
            ctx.live.push((to.clone(), false));
            GStmt::Move { from, to, reinit }
        }
        _ if boundary => {
            let mut_funcs: Vec<String> = funcs
                .iter()
                .filter(|(_, _, m)| *m)
                .map(|(n, _, _)| n.clone())
                .collect();
            let vars = ctx.var_names();
            if mut_funcs.is_empty() || vars.is_empty() {
                let name = ctx.fresh("x");
                let value = gen_expr(rng, &names, &funcs, 2, defined);
                ctx.live.push((name.clone(), false));
                GStmt::Let { name, value }
            } else {
                GStmt::CallMut {
                    func: mut_funcs[rng.below(mut_funcs.len() as u64) as usize].clone(),
                    target: vars[rng.below(vars.len() as u64) as usize].clone(),
                }
            }
        }
        _ => {
            let name = ctx.fresh("x");
            let value = gen_expr(rng, &names, &funcs, 3, defined);
            ctx.live.push((name.clone(), false));
            GStmt::Let { name, value }
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Renders a program to `.lu` source — Candidate-A braced expression syntax,
/// one statement per line, `main` returning `0` per the corpus idiom.
#[must_use]
pub fn render(program: &GProgram) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "// fuzz: is05 generated program ({} mode)",
        program.mode.as_str()
    );
    for func in &program.funcs {
        let params = func
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if func.mutates && i == 0 {
                    format!("mut {p}: int")
                } else {
                    format!("{p}: int")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "fn {}({params}) -> int {{", func.name);
        if func.mutates {
            let first = &func.params[0];
            let _ = writeln!(out, "    {first} = {first} + 1");
        }
        let _ = writeln!(out, "    {}", render_expr(&func.body));
        let _ = writeln!(out, "}}");
        out.push('\n');
    }
    let _ = writeln!(out, "fn main() -> !int {{");
    for stmt in &program.main {
        render_stmt(&mut out, stmt, 1);
    }
    let _ = writeln!(out, "    0");
    let _ = writeln!(out, "}}");
    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

fn render_stmt(out: &mut String, stmt: &GStmt, depth: usize) {
    indent(out, depth);
    match stmt {
        GStmt::Let { name, value } => {
            let _ = writeln!(out, "let {name} = {}", render_expr(value));
        }
        GStmt::Var { name, value } => {
            let _ = writeln!(out, "var {name} = {}", render_expr(value));
        }
        GStmt::Assign { name, value } => {
            let _ = writeln!(out, "{name} = {}", render_expr(value));
        }
        GStmt::Print { prefix, expr } => {
            let _ = writeln!(out, "print(\"{prefix} {{{}}}\")", render_expr(expr));
        }
        GStmt::If {
            cond,
            then,
            otherwise,
        } => {
            let _ = writeln!(out, "if {} {{", render_expr(cond));
            for s in then {
                render_stmt(out, s, depth + 1);
            }
            if otherwise.is_empty() {
                indent(out, depth);
                let _ = writeln!(out, "}}");
            } else {
                indent(out, depth);
                let _ = writeln!(out, "}} else {{");
                for s in otherwise {
                    render_stmt(out, s, depth + 1);
                }
                indent(out, depth);
                let _ = writeln!(out, "}}");
            }
        }
        GStmt::While {
            counter,
            bound,
            body,
        } => {
            let _ = writeln!(out, "var {counter} = 0");
            indent(out, depth);
            let _ = writeln!(out, "while {counter} < {bound} {{");
            for s in body {
                render_stmt(out, s, depth + 1);
            }
            indent(out, depth + 1);
            let _ = writeln!(out, "{counter} = {counter} + 1");
            indent(out, depth);
            let _ = writeln!(out, "}}");
        }
        GStmt::Block(body) => {
            let _ = writeln!(out, "{{");
            for s in body {
                render_stmt(out, s, depth + 1);
            }
            indent(out, depth);
            let _ = writeln!(out, "}}");
        }
        GStmt::Region { name, body } => {
            let _ = writeln!(out, "region {name} {{");
            for s in body {
                render_stmt(out, s, depth + 1);
            }
            indent(out, depth);
            let _ = writeln!(out, "}}");
        }
        GStmt::Move { from, to, reinit } => {
            let _ = writeln!(out, "let {to} = {from}");
            if *reinit {
                indent(out, depth);
                let _ = writeln!(out, "let _ = {to}");
            }
        }
        GStmt::CallMut { func, target } => {
            let _ = writeln!(out, "let _ = {func}(mut {target})");
        }
    }
}

fn render_expr(expr: &GExpr) -> String {
    match expr {
        GExpr::Lit(n) => n.to_string(),
        GExpr::Var(name) => name.clone(),
        GExpr::Bin(op, a, b) => format!("{} {op} {}", render_expr(a), render_expr(b)),
        GExpr::IfVal(c, a, b) => format!(
            "if {} {{ {} }} else {{ {} }}",
            render_expr(c),
            render_expr(a),
            render_expr(b)
        ),
        GExpr::Call(name, args) => format!(
            "{name}({})",
            args.iter().map(render_expr).collect::<Vec<_>>().join(", ")
        ),
        GExpr::Cmp(op, a, b) => format!("{} {op} {}", render_expr(a), render_expr(b)),
        GExpr::Group(inner) => format!("({})", render_expr(inner)),
    }
}

// ---------------------------------------------------------------------------
// Reduction
// ---------------------------------------------------------------------------

/// Shrinks a program while `still_interesting` holds — AST-aware, so the
/// minimized case is a *program*, not a truncated byte string.
///
/// Passes, to fixpoint: drop a helper function; drop a `main` statement (at
/// any block depth); unwrap a block/if/while/region to its body; simplify an
/// expression to `1`. Every candidate is validated by the predicate before it
/// is kept.
pub fn reduce(
    program: &GProgram,
    still_interesting: &mut dyn FnMut(&GProgram) -> bool,
) -> GProgram {
    let mut current = program.clone();
    loop {
        let mut progressed = false;

        // Drop helpers (deepest first, so call sites vanish with their targets
        // only when the predicate allows it).
        for index in (0..current.funcs.len()).rev() {
            let mut candidate = current.clone();
            candidate.funcs.remove(index);
            if still_interesting(&candidate) {
                current = candidate;
                progressed = true;
            }
        }

        // Drop or unwrap statements.
        loop {
            let candidates = stmt_edits(&current.main);
            let mut any = false;
            for edit in candidates {
                let mut candidate = current.clone();
                candidate.main = apply_edit(&current.main, &edit);
                if candidate.main != current.main && still_interesting(&candidate) {
                    current = candidate;
                    any = true;
                    progressed = true;
                    break;
                }
            }
            if !any {
                break;
            }
        }

        // Simplify expressions to `1`.
        loop {
            let mut simplified = false;
            let snapshot = current.clone();
            let mut target = 0usize;
            let total = count_exprs(&snapshot.main);
            while target < total {
                let mut candidate = snapshot.clone();
                let mut counter = 0usize;
                simplify_nth(&mut candidate.main, target, &mut counter);
                if candidate.main != current.main && still_interesting(&candidate) {
                    current = candidate;
                    simplified = true;
                    progressed = true;
                    break;
                }
                target += 1;
            }
            if !simplified {
                break;
            }
        }

        if !progressed {
            return current;
        }
    }
}

/// One structural edit: remove or unwrap the statement at a path.
#[derive(Debug, Clone)]
enum Edit {
    Remove(Vec<usize>),
    Unwrap(Vec<usize>),
}

fn stmt_edits(stmts: &[GStmt]) -> Vec<Edit> {
    let mut edits = Vec::new();
    collect_edits(stmts, &mut Vec::new(), &mut edits);
    edits
}

fn collect_edits(stmts: &[GStmt], path: &mut Vec<usize>, out: &mut Vec<Edit>) {
    for (index, stmt) in stmts.iter().enumerate() {
        path.push(index);
        out.push(Edit::Remove(path.clone()));
        match stmt {
            GStmt::If {
                then, otherwise, ..
            } => {
                out.push(Edit::Unwrap(path.clone()));
                collect_edits(then, path, out);
                collect_edits(otherwise, path, out);
            }
            GStmt::While { body, .. } | GStmt::Block(body) | GStmt::Region { body, .. } => {
                out.push(Edit::Unwrap(path.clone()));
                collect_edits(body, path, out);
            }
            _ => {}
        }
        path.pop();
    }
}

fn apply_edit(stmts: &[GStmt], edit: &Edit) -> Vec<GStmt> {
    let (path, unwrap) = match edit {
        Edit::Remove(path) => (path.as_slice(), false),
        Edit::Unwrap(path) => (path.as_slice(), true),
    };
    rebuild(stmts, path, unwrap)
}

fn rebuild(stmts: &[GStmt], path: &[usize], unwrap: bool) -> Vec<GStmt> {
    let Some((&head, rest)) = path.split_first() else {
        return stmts.to_vec();
    };
    let mut out = Vec::with_capacity(stmts.len());
    for (index, stmt) in stmts.iter().enumerate() {
        if index != head {
            out.push(stmt.clone());
            continue;
        }
        if !rest.is_empty() {
            // Recurse into the container at `head`.
            let mut inner = stmt.clone();
            match &mut inner {
                GStmt::If {
                    then, otherwise, ..
                } => {
                    // The path does not distinguish arms; try both.
                    let rebuilt_then = rebuild(then, rest, unwrap);
                    if rebuilt_then != *then {
                        *then = rebuilt_then;
                    } else {
                        *otherwise = rebuild(otherwise, rest, unwrap);
                    }
                }
                GStmt::While { body, .. } | GStmt::Block(body) | GStmt::Region { body, .. } => {
                    *body = rebuild(body, rest, unwrap);
                }
                _ => {}
            }
            out.push(inner);
            continue;
        }
        if unwrap {
            match stmt {
                GStmt::If {
                    then, otherwise, ..
                } => {
                    out.extend(then.iter().cloned());
                    out.extend(otherwise.iter().cloned());
                }
                GStmt::While { body, .. } | GStmt::Block(body) | GStmt::Region { body, .. } => {
                    out.extend(body.iter().cloned());
                }
                other => out.push(other.clone()),
            }
        }
        // Remove: push nothing.
    }
    out
}

fn count_exprs(stmts: &[GStmt]) -> usize {
    let mut n = 0;
    for stmt in stmts {
        match stmt {
            GStmt::Let { value, .. }
            | GStmt::Var { value, .. }
            | GStmt::Assign { value, .. }
            | GStmt::Print { expr: value, .. } => n += count_expr(value),
            GStmt::If {
                cond,
                then,
                otherwise,
            } => {
                n += count_expr(cond) + count_exprs(then) + count_exprs(otherwise);
            }
            GStmt::While { body, .. } | GStmt::Block(body) | GStmt::Region { body, .. } => {
                n += count_exprs(body)
            }
            GStmt::Move { .. } | GStmt::CallMut { .. } => {}
        }
    }
    n
}

fn count_expr(expr: &GExpr) -> usize {
    1 + match expr {
        GExpr::Lit(_) | GExpr::Var(_) => 0,
        GExpr::Bin(_, a, b) | GExpr::Cmp(_, a, b) => count_expr(a) + count_expr(b),
        GExpr::IfVal(c, a, b) => count_expr(c) + count_expr(a) + count_expr(b),
        GExpr::Call(_, args) => args.iter().map(count_expr).sum(),
        GExpr::Group(inner) => count_expr(inner),
    }
}

fn simplify_nth(stmts: &mut [GStmt], target: usize, counter: &mut usize) {
    for stmt in stmts {
        match stmt {
            GStmt::Let { value, .. }
            | GStmt::Var { value, .. }
            | GStmt::Assign { value, .. }
            | GStmt::Print { expr: value, .. } => simplify_expr(value, target, counter),
            GStmt::If {
                cond,
                then,
                otherwise,
            } => {
                simplify_expr(cond, target, counter);
                simplify_nth(then, target, counter);
                simplify_nth(otherwise, target, counter);
            }
            GStmt::While { body, .. } | GStmt::Block(body) | GStmt::Region { body, .. } => {
                simplify_nth(body, target, counter)
            }
            GStmt::Move { .. } | GStmt::CallMut { .. } => {}
        }
    }
}

fn simplify_expr(expr: &mut GExpr, target: usize, counter: &mut usize) {
    if *counter == target && !matches!(expr, GExpr::Lit(_)) {
        *counter += 1;
        *expr = GExpr::Lit(1);
        return;
    }
    *counter += 1;
    match expr {
        GExpr::Lit(_) | GExpr::Var(_) => {}
        GExpr::Bin(_, a, b) | GExpr::Cmp(_, a, b) => {
            simplify_expr(a, target, counter);
            simplify_expr(b, target, counter);
        }
        GExpr::IfVal(c, a, b) => {
            simplify_expr(c, target, counter);
            simplify_expr(a, target, counter);
            simplify_expr(b, target, counter);
        }
        GExpr::Call(_, args) => {
            for arg in args {
                simplify_expr(arg, target, counter);
            }
        }
        GExpr::Group(inner) => simplify_expr(inner, target, counter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic_for_a_seed() {
        for mode in [Mode::Defined, Mode::Boundary] {
            for index in [0u64, 1, 99] {
                let a = generate(&mut Rng::for_case(42, index), mode);
                let b = generate(&mut Rng::for_case(42, index), mode);
                assert_eq!(a, b);
                assert_eq!(render(&a), render(&b));
            }
        }
    }

    #[test]
    fn different_seeds_generate_different_programs() {
        let a = render(&generate(&mut Rng::for_case(1, 0), Mode::Defined));
        let b = render(&generate(&mut Rng::for_case(2, 0), Mode::Defined));
        // Not a law of nature, but with this generator's entropy a collision
        // here means the seed is not reaching the generator.
        assert_ne!(a, b);
    }

    #[test]
    fn defined_mode_programs_run_clean_in_this_machine() {
        // Defined-by-construction, checked by construction's own referee: two
        // hundred programs, every one must lex, parse, resolve and exit 0.
        for index in 0..200u64 {
            let program = generate(&mut Rng::for_case(0xD1FF, index), Mode::Defined);
            let source = render(&program);
            let observation = crate::frontend::observe(source.as_bytes(), None);
            assert_eq!(
                observation.verdict,
                crate::protocol::Verdict::Exit(0),
                "case {index} must run clean:\n{source}\ngot {} ({:?})",
                observation.verdict,
                observation.detail,
            );
        }
    }

    #[test]
    fn boundary_mode_programs_never_produce_ub() {
        // Every generated program is unsafe-free, so `[mem.ub]`'s "safe-tier
        // programs cannot reach any row" applies: traps are legitimate
        // outcomes here, `ub(*)` never is.
        for index in 0..200u64 {
            let program = generate(&mut Rng::for_case(0xB0DA, index), Mode::Boundary);
            let source = render(&program);
            let observation = crate::frontend::observe(source.as_bytes(), None);
            assert!(
                !matches!(observation.verdict, crate::protocol::Verdict::Ub(_)),
                "case {index}: ub in safe code:\n{source}"
            );
            assert!(
                !matches!(observation.verdict, crate::protocol::Verdict::Fail(_)),
                "case {index}: the generator emitted an ill-formed program:\n{source}\n{:?}",
                observation.detail
            );
        }
    }

    #[test]
    fn the_reducer_shrinks_while_the_predicate_holds() {
        let program = generate(&mut Rng::for_case(7, 3), Mode::Defined);
        let source = render(&program);
        // Toy predicate: "still contains a print". The reducer must keep the
        // print and strip essentially everything else.
        let mut predicate = |candidate: &GProgram| render(candidate).contains("print(");
        let reduced = reduce(&program, &mut predicate);
        let reduced_source = render(&reduced);
        assert!(reduced_source.contains("print("));
        assert!(
            reduced_source.len() <= source.len(),
            "reduction must never grow the case"
        );
        // The reduced program still renders and observes without panicking.
        let _ = crate::frontend::observe(reduced_source.as_bytes(), None);
    }

    #[test]
    fn reduction_is_deterministic() {
        let program = generate(&mut Rng::for_case(11, 5), Mode::Boundary);
        let mut p1 = |c: &GProgram| render(c).contains("region");
        let mut p2 = |c: &GProgram| render(c).contains("region");
        let a = reduce(&program, &mut p1);
        let b = reduce(&program, &mut p2);
        assert_eq!(a, b);
    }
}
