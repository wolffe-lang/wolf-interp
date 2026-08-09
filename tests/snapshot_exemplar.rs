//! The insta exemplar.
//!
//! One snapshot, establishing the pattern the rest of the track follows (see
//! CONTRIBUTING.md for the `INSTA_UPDATE` ritual).
//!
//! Note what this snapshot deliberately does **not** contain: corpus text.
//! Corpus bytes are canonical output of `wolf fmt` (STYLE_VERSION 1) and will
//! be rewritten wholesale when the style version bumps — a snapshot that
//! embedded them would churn for reasons that have nothing to do with this
//! repo. The header below is owned by this test, so the snapshot only ever
//! moves when the *parser* changes.

use wolf_interp::directive::{Directives, parse_header};

/// A header exercising every key and both interesting `check:` shapes.
const HEADER: &str = "\
//! check: run(exit=trap(div-zero), stdout=\"one, two\")
//! phase: resolve
//! conforms: mem.ub.defined, str.interp
//!
//! Division by zero is a deterministic trap, not UB. This prose line has a
//! colon in it: it must not be mistaken for a directive.
fn main() {}
";

fn render(directives: &Directives) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "member:   {}\n",
        if directives.member { "true" } else { "false" }
    ));
    out.push_str(&format!(
        "phase:    {}\n",
        directives
            .phase
            .map_or_else(|| "<none>".to_owned(), |p| p.to_string())
    ));
    out.push_str(&format!(
        "check:    {}\n",
        directives
            .check
            .as_ref()
            .map_or_else(|| "<none>".to_owned(), ToString::to_string)
    ));
    out.push_str("conforms:\n");
    for tag in &directives.conforms {
        out.push_str(&format!("  - {tag}\n"));
    }
    out.push_str(&format!("prose:    {} line(s)\n", directives.prose.len()));
    out
}

#[test]
fn directive_header_rendering() {
    let directives = parse_header(HEADER).expect("the exemplar header parses");
    insta::assert_snapshot!(render(&directives));
}
