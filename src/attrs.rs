//! The closed attribute set (`[gram.item.attr.set]`), conditional
//! compilation (`[gram.item.attr.cfg]`) and the one ABI string
//! (`[abi.c.seams]`) — lupin's half of KWC K13 and K7 (STATUS #31,
//! wolf-lang#519 and #524; wolf-interp#174, is70).
//!
//! Until is70 this machine read no attribute but the origin marker and
//! `allow`: an unknown or unimplemented attribute ran as if absent (looser
//! than every wolfgang lane, which refuses it E0817), `extern "S"` for any
//! `S` ran (E0818 there), and `#[cfg(target = "…")]` kept every gated node,
//! so two arch-gated definitions collided (E0302) and freestanding-only
//! code was typed on a hosted run.
//!
//! Two halves, one rule each:
//!
//! - [`cfg_keeps`] decides whether a parsed node survives. The parser asks
//!   it for every item, impl member, field and statement, after the block's
//!   tail is decided, so a dropped node is removed after parsing and before
//!   name resolution: its names are never defined and its body is never
//!   resolved or run. A `cfg` this machine cannot read keeps the node, so
//!   [`check`] can refuse it — a typo never silently drops code.
//! - [`check`] refuses, at the resolve rung, every attribute on a surviving
//!   node that nothing reads in its position, and every `extern "S"` whose
//!   `S` is not `"c"`. The codes and spans are the compiler's on all three
//!   lanes, measured at wolf-lang `50830027` (`tests/rulings_is70/attr_*`,
//!   `cfg_*`, `extern_abi_*`): E0817 at the offending attr (the whole attr,
//!   or each bad item of a `repr(…)` list, or the bad predicate inside a
//!   `cfg(…)`), E0818 at the ABI string. Every one is reported, in source
//!   order, as the compiler reports them.

use crate::ast::{
    Attr, AttrArg, AttrInput, Attribute, Block, Expr, ExprKind, FnDecl, FnQual, Item, ItemKind,
    Stmt, StmtKind,
};
use crate::diag::Diag;
use crate::rowmatch::{Child, each_child_with_blocks};
use crate::sema::Program;

/// Every target a `cfg(target = "…")` may name: the hosted triples, the
/// freestanding one, and the two architectures — the set the compiler's own
/// E0817 note names (wolf-lang `50830027`).
pub const KNOWN_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-freebsd",
    crate::FREESTANDING_TRIPLE,
    "x86_64",
    "aarch64",
];

/// What one `#[cfg(…)]` attr says.
enum Cfg<'a> {
    /// `cfg(target = "S")` with a known `S`.
    Target(&'a str),
    /// Anything else: refused E0817 at this span, and the node is kept.
    Bad(crate::diag::Span, String),
}

fn is_named(attr: &Attr, name: &str) -> bool {
    attr.path.is_single() && attr.path.segments[0].name == name
}

fn plain_string(expr: &Expr) -> Option<String> {
    match &*expr.kind {
        ExprKind::Str(lit) => lit.as_plain_text(),
        _ => None,
    }
}

fn read_cfg(attr: &Attr) -> Cfg<'_> {
    let whole = |why: &str| {
        Cfg::Bad(
            attr.span,
            format!(
                "`cfg` takes exactly one predicate, `target = \"…\"` — {why} \
                 ([gram.item.attr.cfg]); a `cfg` this machine cannot read keeps its node, so \
                 a typo never silently drops code"
            ),
        )
    };
    let Some(AttrInput::Args(args)) = &attr.input else {
        return whole("this one has none");
    };
    let [arg] = args.as_slice() else {
        return whole("this one has several");
    };
    let AttrArg::Nested(predicate) = arg else {
        return whole("a bare literal is not a predicate");
    };
    if !is_named(predicate, "target") {
        return Cfg::Bad(
            predicate.span,
            "`cfg` decides one predicate, `target = \"…\"`; no other key exists \
             ([gram.item.attr.cfg])"
                .to_owned(),
        );
    }
    let Some(AttrInput::Literal(value)) = &predicate.input else {
        return Cfg::Bad(
            predicate.span,
            "a `target` predicate names its target as a string: `target = \"x86_64\"` \
             ([gram.item.attr.cfg])"
                .to_owned(),
        );
    };
    let Some(text) = plain_string(value) else {
        return Cfg::Bad(
            predicate.span,
            "a `target` predicate names its target as a string: `target = \"x86_64\"` \
             ([gram.item.attr.cfg])"
                .to_owned(),
        );
    };
    match KNOWN_TARGETS.iter().find(|known| **known == text) {
        Some(known) => Cfg::Target(known),
        None => Cfg::Bad(
            predicate.span,
            format!(
                "`{text}` names no target wolf knows: a `target` predicate names a triple ({}) \
                 or an architecture (`x86_64`, `aarch64`) ([gram.item.attr.cfg])",
                KNOWN_TARGETS[..6]
                    .iter()
                    .map(|t| format!("`{t}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
    }
}

/// Whether `target` (a known triple or architecture) names the build's
/// target: the whole triple, or its architecture, the first component.
fn holds(target: &str) -> bool {
    let host = crate::HOST_TRIPLE;
    target == host || host.split('-').next() == Some(target)
}

/// Whether a node carrying `attrs` survives conditional compilation: false
/// exactly when one well-formed `cfg(target = "S")` names a known target
/// that is not this build's. Several `cfg`s must all hold; a malformed one
/// keeps the node (and [`check`] refuses it).
#[must_use]
pub fn cfg_keeps(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .flat_map(|attribute| &attribute.attrs)
        .filter(|attr| is_named(attr, "cfg"))
        .all(|attr| match read_cfg(attr) {
            Cfg::Target(target) => holds(target),
            Cfg::Bad(..) => true,
        })
}

/// Where an attribute is written, for `[gram.item.attr.set]`'s position
/// column.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    Fn,
    Struct,
    OtherItem,
    Stmt,
    Field,
}

impl Position {
    fn of(item: &Item) -> Position {
        match &item.kind {
            ItemKind::Fn(_) => Position::Fn,
            ItemKind::Struct(_) => Position::Struct,
            _ => Position::OtherItem,
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Position::Fn => "a function",
            Position::Struct => "a struct",
            Position::OtherItem => "this item",
            Position::Stmt => "a statement",
            Position::Field => "a field",
        }
    }
}

fn attr_name(attr: &Attr) -> String {
    attr.path
        .segments
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn e0817(span: crate::diag::Span, message: String) -> Diag {
    Diag::new("E0817", span, "gram.item.attr.set", message)
}

/// The E0817s one node's attributes earn.
fn judge(attrs: &[Attribute], at: Position, out: &mut Vec<Diag>) {
    for attr in attrs.iter().flat_map(|attribute| &attribute.attrs) {
        let name = attr_name(attr);
        let misplaced = |what: &str| {
            e0817(
                attr.span,
                format!(
                    "`#[{name}]` means something on {what}, not on {}: an implemented \
                     attribute where nothing reads it is refused ([gram.item.attr.set])",
                    at.noun()
                ),
            )
        };
        match name.as_str() {
            // Any attributed node: the lint owns their argument shapes
            // (W0302/W0303, E0813).
            "allow" | "index" => {}
            "trusted" | "consttime" => {
                if at != Position::Fn {
                    out.push(misplaced("a function"));
                }
            }
            "budget" => {
                if at == Position::Field {
                    out.push(misplaced("a statement or an item"));
                }
            }
            "cfg" => {
                if let Cfg::Bad(span, message) = read_cfg(attr) {
                    out.push(e0817(span, message));
                }
            }
            "repr" => {
                if at != Position::Struct {
                    out.push(misplaced("a struct"));
                    continue;
                }
                let Some(AttrInput::Args(args)) = &attr.input else {
                    out.push(e0817(
                        attr.span,
                        "`repr` names a layout: `#[repr(c)]` is the one implemented \
                         ([gram.item.attr.set])"
                            .to_owned(),
                    ));
                    continue;
                };
                for arg in args {
                    let (span, item) = match arg {
                        AttrArg::Nested(inner) => (inner.span, attr_name(inner)),
                        AttrArg::Literal(lit) => (lit.span, "a literal".to_owned()),
                    };
                    let fine = matches!(arg, AttrArg::Nested(inner)
                        if is_named(inner, "c") && inner.input.is_none());
                    if !fine {
                        out.push(e0817(
                            span,
                            format!(
                                "`repr({item})` is not implemented: `#[repr(c)]` is the one \
                                 layout attribute in force; `packed`, `align(N)` and \
                                 `transparent` are refused by name until the lane that \
                                 implements them ([gram.item.attr.set])"
                            ),
                        ));
                    }
                }
            }
            _ => out.push(e0817(
                attr.span,
                format!(
                    "`#[{name}]` is not an attribute wolf implements: the set is closed \
                     (`trusted`, `consttime`, `allow`, `index`, `budget`, `repr(c)`, \
                     `cfg(target = \"…\")`), and an attribute nothing reads is refused, \
                     never ignored ([gram.item.attr.set])"
                ),
            )),
        }
    }
}

/// E0818: `extern "S"` with any `S` but `"c"` (`[abi.c.seams]`, K7).
fn judge_abi(decl: &FnDecl, out: &mut Vec<Diag>) {
    for qual in &decl.quals {
        if let FnQual::Extern { abi, .. } = qual {
            let text = abi.as_plain_text().unwrap_or_default();
            if text != "c" {
                out.push(Diag::new(
                    "E0818",
                    abi.span,
                    "abi.c.seams",
                    format!(
                        "`extern \"{text}\"` names no ABI wolf has: the only ABI string is \
                         `\"c\"` ([abi.c.seams]). An interrupt or exception enters through an \
                         assembly trampoline that calls an `export fn` with the C convention"
                    ),
                ));
            }
        }
    }
}

struct Walk {
    out: Vec<Diag>,
}

impl Walk {
    fn item(&mut self, item: &Item) {
        judge(&item.attrs, Position::of(item), &mut self.out);
        match &item.kind {
            ItemKind::Fn(decl) => self.decl(decl),
            ItemKind::Struct(def) => {
                for field in &def.fields {
                    judge(&field.attrs, Position::Field, &mut self.out);
                }
            }
            ItemKind::Impl(def) => {
                for member in &def.members {
                    self.item(member);
                }
            }
            ItemKind::Trait(def) => {
                for member in &def.members {
                    self.item(member);
                }
            }
            ItemKind::Binding(binding) => self.expr(&binding.value),
            _ => {}
        }
    }

    fn decl(&mut self, decl: &FnDecl) {
        judge_abi(decl, &mut self.out);
        if let Some(body) = &decl.body {
            self.block(body);
        }
    }

    fn block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.stmt(stmt);
        }
        if let Some(tail) = &block.tail {
            self.expr(tail);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        if let StmtKind::Item(item) = &stmt.kind {
            // The attributes are the item's, whichever side parsed them.
            let mut attrs = stmt.attrs.clone();
            attrs.extend(item.attrs.iter().cloned());
            judge(&attrs, Position::of(item), &mut self.out);
            let bare = Item {
                attrs: Vec::new(),
                ..(**item).clone()
            };
            self.item(&bare);
            return;
        }
        judge(&stmt.attrs, Position::Stmt, &mut self.out);
        match &stmt.kind {
            StmtKind::Binding(binding) => self.expr(&binding.value),
            StmtKind::Assign { place, value, .. } => {
                self.expr(place);
                self.expr(value);
            }
            StmtKind::Defer { expr, .. } | StmtKind::Expr(expr) => self.expr(expr),
            StmtKind::AssumeNoalias(operands) => {
                for operand in operands {
                    self.expr(operand);
                }
            }
            StmtKind::Item(_) => {}
        }
    }

    fn expr(&mut self, expr: &Expr) {
        each_child_with_blocks(expr, &mut |child| match child {
            Child::Expr(child) => self.expr(child),
            Child::Block(block) => self.block(block),
        });
    }
}

/// Every E0817 and E0818 the program earns, in source order within the
/// first file that earns any (the record names one file for its
/// rejection). The root module is read first.
#[must_use]
pub fn check(program: &Program) -> Vec<Diag> {
    let root = program.root();
    let modules = std::iter::once(root).chain(
        program
            .modules
            .values()
            .filter(|module| !std::ptr::eq(*module, root)),
    );
    for module in modules {
        for unit in &module.units {
            let mut walk = Walk { out: Vec::new() };
            for item in &unit.unit.items {
                walk.item(item);
            }
            if !walk.out.is_empty() {
                let mut out = walk.out;
                out.sort_by_key(|diag| (diag.span.start, diag.span.end));
                return out
                    .into_iter()
                    .map(|diag| diag.in_file(unit.file.clone()))
                    .collect();
            }
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs_of(line: &str) -> Vec<Attribute> {
        let source = format!("{line}\nfn g() -> int {{\n    1\n}}\n");
        let parsed = crate::parse::parse_source(&source).expect("parses");
        // The parser already dropped a node `cfg` gates away; read the
        // attributes off a parse that keeps it.
        parsed
            .unit
            .items
            .first()
            .map(|item| item.attrs.clone())
            .unwrap_or_default()
    }

    fn arch() -> &'static str {
        crate::HOST_TRIPLE.split('-').next().expect("a triple")
    }

    fn other_arch() -> &'static str {
        if arch() == "x86_64" {
            "aarch64"
        } else {
            "x86_64"
        }
    }

    #[test]
    fn the_host_and_its_architecture_hold_and_another_does_not() {
        assert!(holds(crate::HOST_TRIPLE));
        assert!(holds(arch()));
        assert!(!holds(other_arch()));
        assert!(!holds(crate::FREESTANDING_TRIPLE));
    }

    #[test]
    fn a_node_another_target_gates_is_dropped_at_parse() {
        let kept = format!("#[cfg(target = \"{}\")]", arch());
        assert_eq!(attrs_of(&kept).len(), 1, "kept on its own target");
        let gone = format!("#[cfg(target = \"{}\")]", other_arch());
        assert!(attrs_of(&gone).is_empty(), "the item is gone");
    }

    #[test]
    fn a_cfg_this_machine_cannot_read_keeps_its_node() {
        for line in [
            "#[cfg(unix)]",
            "#[cfg(target = \"no-such-target\")]",
            "#[cfg]",
            "#[cfg(target = 3)]",
            "#[cfg(target = \"x86_64\", target = \"aarch64\")]",
        ] {
            let attrs = attrs_of(line);
            assert_eq!(attrs.len(), 1, "{line} keeps the node");
            assert!(cfg_keeps(&attrs), "{line}");
        }
    }

    #[test]
    fn every_refusal_is_reported_in_source_order() {
        let source = "#[repr(bogus, packed(3)), align(7)]\nstruct R {\n    a: u8,\n}\n\n\
                      extern \"x86-interrupt\" fn isr() {\n}\n\n#[allow(w0301), trusted]\n\
                      fn main() -> int {\n    0\n}\n";
        let program = crate::sema::load_source("main.lu", source).expect("loads");
        let found: Vec<(&str, &str)> = check(&program)
            .iter()
            .map(|d| (d.code, &source[d.span.start..d.span.end]))
            .collect();
        assert_eq!(
            found,
            vec![
                ("E0817", "bogus"),
                ("E0817", "packed(3)"),
                ("E0817", "align(7)"),
                ("E0818", "\"x86-interrupt\""),
            ]
        );
    }
}
