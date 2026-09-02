//! Tests generated from the pinned spec document itself.
//!
//! The sprint asks for grammar-conformance tests *extracted from* `spec/01`,
//! regenerated on every pin bump, so that a spec change that moves the grammar
//! shows up as a failing generated test rather than as a silent divergence.
//! This file is that extractor: it re-reads the pinned markdown at test time
//! and diffs it against the transcriptions in `lex.rs` and `parse.rs`.
//!
//! What is mechanically extractable, and therefore checked here:
//!
//! | spec | shape | checked against |
//! |---|---|---|
//! | §6.1 `reserved_kw` | an EBNF alternation of quoted terminals | [`lex::KEYWORDS`] |
//! | §6.1 "the count is a checksum: 50" | prose with a number | the list's length |
//! | §6.2 contextual keywords | inline-code run in prose | [`lex::CONTEXTUAL`] |
//! | §3.2 precedence | a markdown table | [`parse::PRECEDENCE`] |
//! | §1.6 counter-example | a fenced ```text block | must fail to parse |
//! | §2.3 counter-example | prose + inline code | must fail to parse |
//! | §1.4 counter-example | prose + inline code | must lex as member access |
//! | §9 code reservations | prose list of `E000x (…)` | the `diag` constants |
//!
//! What is *not*: the ambiguity annex's paired readings, because §8 already
//! ships them as corpus files — `tests/conformance.rs` runs those, which is
//! what the sprint says to do rather than re-deriving them from prose.
//!
//! A local mutation to any of these spec blocks makes a test here fail. That
//! is the alarm, and it is the point.

use std::path::{Path, PathBuf};

use wolf_interp::diag;
use wolf_interp::lex::{self, Tok};
use wolf_interp::parse::{self, Assoc};

fn spec(file: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(wolf_interp::upstream_root())
        .join("spec")
        .join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the pinned spec must be readable: {} ({e})", path.display()))
}

fn grammar() -> String {
    spec("01-grammar.md")
}

/// Every ` ```<tag> ` fenced block in a markdown document.
fn fenced_blocks(document: &str, tag: &str) -> Vec<String> {
    let fence = format!("```{tag}");
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in document.lines() {
        match &mut current {
            Some(buffer) => {
                if line.trim_end() == "```" {
                    blocks.push(std::mem::take(buffer));
                    current = None;
                } else {
                    buffer.push_str(line);
                    buffer.push('\n');
                }
            }
            None => {
                if line.trim_end() == fence {
                    current = Some(String::new());
                }
            }
        }
    }
    blocks
}

/// The body of an EBNF production, joined across its continuation lines.
fn production(document: &str, name: &str) -> String {
    let needle = format!("{name} ::=");
    for block in fenced_blocks(document, "ebnf") {
        let mut lines = block.lines().peekable();
        while let Some(line) = lines.next() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with(&needle) && !trimmed.starts_with(&format!("{name}  ::=")) {
                continue;
            }
            let mut body = trimmed
                .split_once("::=")
                .expect("just matched")
                .1
                .to_owned();
            // Continuation lines of an alternation begin with `|` — and, as
            // of s132's `region_cap` amendment, a single alternative may also
            // WRAP onto an indented line that begins with neither (the sugar
            // arm carries its cap parenthesis on one line and its strategy on
            // the next). Absorb both: anything indented that is not itself a
            // new `name ::=` production and not blank belongs to this body.
            while let Some(next) = lines.peek() {
                let trimmed = next.trim_start();
                let wrapped = trimmed.starts_with('|')
                    || (next.starts_with(char::is_whitespace)
                        && !trimmed.is_empty()
                        && !trimmed.contains("::="));
                if wrapped {
                    body.push(' ');
                    body.push_str(next.trim());
                    lines.next();
                } else {
                    break;
                }
            }
            return body;
        }
    }
    panic!("no `{name} ::=` production in the pinned grammar");
}

/// The quoted terminals of an alternation, in the order the spec lists them.
fn quoted_terminals(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(open) = rest.find('\'') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('\'') else { break };
        out.push(after[..close].to_owned());
        rest = &after[close + 1..];
    }
    out
}

/// Every `` `word` `` in a slice of prose.
fn inline_codes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        out.push(after[..close].to_owned());
        rest = &after[close + 1..];
    }
    out
}

/// The prose of one `## …` / `### …` section, by heading substring.
fn section(document: &str, heading_contains: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    let mut level = 0usize;
    for line in document.lines() {
        if let Some(hashes) = line.split_whitespace().next()
            && !hashes.is_empty()
            && hashes.chars().all(|c| c == '#')
        {
            if inside && hashes.len() <= level {
                break;
            }
            if !inside && line.contains(heading_contains) {
                inside = true;
                level = hashes.len();
                continue;
            }
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    assert!(inside, "no section matching `{heading_contains}`");
    out
}

// ---------------------------------------------------------------------------

#[test]
fn the_reserved_keyword_list_matches_the_spec_production() {
    let body = production(&grammar(), "reserved_kw");
    let mut from_spec = quoted_terminals(&body);
    assert!(!from_spec.is_empty());

    // `[gram.inv.kw]`: "The list is normative, the count is a checksum: 50."
    // Both halves are checked — a spec edit that adds a keyword and forgets the
    // checksum should fail here too.
    let checksum = section(&grammar(), "6.1 Reserved keywords")
        .split("checksum:")
        .nth(1)
        .and_then(|tail| {
            tail.trim_start()
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse::<usize>().ok())
        })
        .expect("§6.1 states its own checksum");
    assert_eq!(from_spec.len(), checksum, "spec list vs spec checksum");

    from_spec.sort();
    let mut ours: Vec<String> = lex::KEYWORDS.iter().map(|k| (*k).to_owned()).collect();
    ours.sort();
    assert_eq!(
        ours, from_spec,
        "our keyword set has drifted from the spec's"
    );
    assert_eq!(lex::KEYWORDS.len(), checksum);

    // Sortedness is load-bearing: `is_keyword` binary-searches the array.
    let mut sorted = lex::KEYWORDS;
    sorted.sort_unstable();
    assert_eq!(lex::KEYWORDS, sorted);
}

#[test]
fn the_contextual_keyword_list_matches_the_spec_prose() {
    // §6.2 lists them as inline code in one paragraph, with `/` separating
    // alternatives inside a single backtick run (`rc` / `pool`).
    let prose = section(&grammar(), "6.2 Contextual keywords");
    let mentioned: Vec<String> = inline_codes(&prose)
        .into_iter()
        .flat_map(|code| {
            code.split('/')
                .map(|part| part.trim().to_owned())
                .collect::<Vec<_>>()
        })
        .filter(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .collect();

    for contextual in lex::CONTEXTUAL {
        assert!(
            mentioned.iter().any(|m| m == contextual),
            "`{contextual}` is treated as contextual but §6.2 does not name it"
        );
        // The defining property: a contextual keyword is an identifier
        // everywhere else, so it must lex as one.
        let lexed = lex::lex(contextual);
        assert!(
            matches!(lexed.tokens.first().map(|t| &t.tok), Some(Tok::Ident(name)) if name == contextual),
            "`{contextual}` must lex as an identifier"
        );
        assert!(!lex::is_keyword(contextual));
    }
}

#[test]
fn the_precedence_table_matches_the_spec_table() {
    // §3.2 is a markdown table: `| # | Operators | Assoc |`. Operators are
    // inline code, escaped where markdown would eat them (`\|`).
    let prose = section(&grammar(), "3.2 Precedence");
    let mut rows: Vec<(u8, Vec<String>, Assoc)> = Vec::new();
    for line in prose.lines() {
        let line = line.trim();
        if !line.starts_with('|') || line.starts_with("|---") {
            continue;
        }
        // Markdown escapes a literal pipe as `\|`; tiers 10, 13 and 15 all
        // carry one, so the cell split has to respect the escape.
        let guarded = line.replace("\\|", "\u{0}");
        let cells: Vec<&str> = guarded
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() != 3 {
            continue;
        }
        let Ok(tier) = cells[0].parse::<u8>() else {
            continue;
        };
        let operators: Vec<String> = inline_codes(cells[1])
            .into_iter()
            .map(|code| code.replace('\u{0}', "|"))
            .collect();
        let assoc = match cells[2] {
            "left" => Assoc::Left,
            "right" => Assoc::Right,
            "none" => Assoc::None,
            // Tiers 1 and 3 are marked `—`: not binary, no associativity.
            _ => continue,
        };
        rows.push((tier, operators, assoc));
    }

    assert_eq!(
        rows.len(),
        parse::PRECEDENCE.len(),
        "the spec's table and our transcription disagree on how many binary tiers exist:\n\
         spec {rows:?}\nours {:?}",
        parse::PRECEDENCE
    );

    for ((tier, operators, assoc), (our_tier, our_operators, our_assoc)) in
        rows.iter().zip(parse::PRECEDENCE)
    {
        assert_eq!(tier, our_tier, "tier order differs");
        assert_eq!(assoc, our_assoc, "tier {tier}: associativity differs");
        // The spec's operator cell carries prose for the two tiers whose
        // operators are not plain punctuation (`as` type cast, ranges, `else`
        // defaulting), so containment is the honest comparison there.
        for ours in *our_operators {
            assert!(
                operators.iter().any(|spec_op| spec_op.contains(ours)),
                "tier {tier}: we parse `{ours}` but §3.2 does not list it: {operators:?}"
            );
        }
    }
}

#[test]
fn the_newline_counter_example_does_not_parse() {
    // §1.6 ships its counter-example as a fenced ```text block:
    //     let x = a
    //           + b        // ERROR: …
    let blocks = fenced_blocks(&grammar(), "text");
    assert!(!blocks.is_empty(), "§1.6's counter-example block is gone");
    let leading_operator = blocks
        .iter()
        .find(|b| b.contains("+ b"))
        .expect("the leading-operator counter-example");

    // Wrapped in the smallest function that makes it a compilation unit; the
    // block itself is a statement sequence, not an item.
    let program = format!("fn main() -> int {{\n{leading_operator}    0\n}}\n");
    let error = parse::parse_source(&program).expect_err("the counter-example must not parse");
    assert_eq!(error.code, diag::E_LEADING_OPERATOR);
}

#[test]
fn the_parameter_mode_counter_example_does_not_parse() {
    // §2.3: "Counter-example: `fn f(x: mut int)` does not parse — modes precede
    // the *name*, not the type: `fn f(mut x: int)`." Both spellings are pulled
    // out of the prose and run.
    let prose = section(&grammar(), "2.3 Functions");
    // The counter-example is a paragraph, not a line: the rejected spelling
    // opens it and the suggested fix closes it one line down.
    let start = prose
        .lines()
        .position(|line| line.contains("Counter-example"))
        .expect("§2.3 states a counter-example");
    let paragraph: String = prose
        .lines()
        .skip(start)
        .take_while(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let codes = inline_codes(&paragraph);
    let bad = codes.first().expect("the rejected spelling");
    let good = codes.last().expect("the suggested fix");
    assert!(
        bad.contains("mut int"),
        "extracted the wrong snippet: {bad}"
    );

    // A bare signature is bodyless; give both spellings the same trivial body
    // so the only difference under test is the mode's position.
    assert!(
        parse::parse_source(&format!("{bad} {{ 0 }}\n")).is_err(),
        "`{bad}` must not parse"
    );
    parse::parse_source(&format!("{good} {{ 0 }}\n"))
        .unwrap_or_else(|e| panic!("`{good}` is the spec's fix and must parse: {e}"));
}

#[test]
fn the_float_counter_example_lexes_as_member_access() {
    // §1.4: "`1.e5` does not parse as a float (it is member access on `1` with
    // member `e5`)". This is a *token-shape* claim, not a rejection — and it is
    // the same claim `[gram.amb.intdot]` makes for `1.s`, which the corpus
    // requires to work. So the assertion is on the tokens, not on a verdict.
    let prose = section(&grammar(), "1.4 Integer and float");
    assert!(
        prose.contains("1.e5"),
        "§1.4's counter-example moved; this test is stale"
    );

    let tokens: Vec<Tok> = lex::lex("1.e5").tokens.into_iter().map(|t| t.tok).collect();
    assert_eq!(
        tokens,
        vec![
            Tok::Int("1".to_owned()),
            Tok::Dot,
            Tok::Ident("e5".to_owned()),
            Tok::Term { explicit: false },
        ],
        "`1.e5` must be int-dot-member, never a float"
    );

    // The three readings §1.4 and `[gram.amb.intdot]` distinguish, side by side.
    assert!(matches!(lex::lex("1.0").tokens[0].tok, Tok::Float(_)));
    assert!(matches!(lex::lex("1.0e5").tokens[0].tok, Tok::Float(_)));
    assert_eq!(lex::lex("1..10").tokens[1].tok, Tok::DotDot);
    assert_eq!(lex::lex("5.s").tokens[1].tok, Tok::Dot);
}

#[test]
fn the_section_nine_code_reservations_are_the_ones_we_emit() {
    // §9 lists them as `E0001 (leading-operator continuation), E0002 (…), …`.
    // At pin f0da6e6 the heading grew the `[diag]` anchor and s67's
    // subsections (§9.1 severity, §9.2 families, §9.3 levels) — the
    // reservations stay in the section head, so the scan stops at §9.1
    // (the warning families cite codes, W0301/E0802, that are not E000x
    // reservations and are not this machine's to emit).
    let full = section(&grammar(), "9. Diagnostics");
    let prose = full
        .split("### 9.1")
        .next()
        .expect("the section head precedes its subsections")
        .to_owned();
    let mut reserved: Vec<String> = Vec::new();
    let mut rest = prose.as_str();
    while let Some(at) = rest.find('E') {
        let candidate = &rest[at..];
        let code: String = candidate.chars().take(5).collect();
        if code.len() == 5 && code[1..].chars().all(|c| c.is_ascii_digit()) {
            reserved.push(code);
        }
        rest = &rest[at + 1..];
    }
    reserved.sort();
    reserved.dedup();

    let ours = [
        diag::E_LEADING_OPERATOR,
        diag::E_EMPTY_STATEMENT,
        diag::E_COMPARISON_CHAIN,
        diag::E_FLOAT_DOT_EXPONENT,
        diag::E_ELSE_NEW_LINE,
        diag::E_STRUCT_LIT_IN_COND,
        diag::E_INTERP_DEPTH,
        diag::E_KEYWORD_AS_IDENT,
    ];
    assert_eq!(
        reserved,
        ours.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>(),
        "§9's reservations and our constants disagree"
    );
}

#[test]
fn the_interpolation_depth_rails_are_the_ones_the_spec_names() {
    // `[gram.lex.str]`: "legal to depth 8; deeper is an error … (E0007, enforced
    // at the parse tier). The lexer's hard safety rail is depth 32 (E0108)."
    let prose = section(&grammar(), "1.5 String literals");
    assert!(prose.contains("depth 8"), "the friendly limit moved");
    assert!(prose.contains("E0007"), "the friendly code moved");
    assert!(prose.contains("depth 32"), "the hard rail moved");
    assert!(prose.contains("E0108"), "the rail's code moved");

    // Depth 9: still tokenizes (so the parser can produce the friendly error),
    // and E0007 is what comes out.
    let nested = nest_strings(9);
    let lexed = lex::lex(&nested);
    assert!(
        lexed.first_error().is_none(),
        "between 8 and 32 the input must still tokenize: {:?}",
        lexed.errors
    );
    assert_eq!(lexed.deferred.len(), 1);
    assert_eq!(lexed.deferred[0].code, diag::E_INTERP_DEPTH);

    // Depth 8: legal, silent.
    let legal = lex::lex(&nest_strings(8));
    assert!(legal.first_error().is_none());
    assert!(legal.deferred.is_empty(), "depth 8 is legal");

    // Depth 33: the lexer's own rail.
    let railed = lex::lex(&nest_strings(33));
    assert_eq!(
        railed.first_error().map(|d| d.code),
        Some(diag::E_INTERP_RAIL)
    );
}

#[test]
fn the_syntactic_nesting_rail_is_the_number_the_spec_makes_normative() {
    // `[gram.lex.rails]`, landed at pin 28ab5c9 in answer to is01's fuzz
    // finding: "expression/statement recursion depth **256** — deeper input is
    // rejected with a diagnostic at the point the rail is hit. Both
    // implementations enforce identical rail values (differential-tested)."
    //
    // "Identical rail values" makes the number a comparison surface, so it is
    // read out of the pinned document rather than trusted to a constant.
    let prose = section(&grammar(), "1.7 Nesting rails");
    let spelled: Vec<usize> = prose
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().expect("digits"))
        .collect();
    assert!(
        spelled.contains(&parse::MAX_NESTING),
        "[gram.lex.rails] names {spelled:?}; MAX_NESTING is {}",
        parse::MAX_NESTING
    );
    assert_eq!(parse::MAX_NESTING, 256, "the normative rail is 256");

    // At the rail: answered with a diagnostic, not a stack overflow.
    let deep = format!(
        "fn main() -> int {{ {}0{} }}\n",
        "(".repeat(parse::MAX_NESTING + 4),
        ")".repeat(parse::MAX_NESTING + 4)
    );
    let diag = parse::parse_source(&deep).expect_err("the rail answers");
    assert_eq!(diag.code, diag::E_NESTING_RAIL);
}

#[test]
fn underscore_identifiers_are_legal_and_a_bare_underscore_is_still_the_wildcard() {
    // Amended `[gram.lex.ident]`: `IDENT ::= ('_' XID_Continue+) |
    // (XID_Start XID_Continue*)`, and "A bare `_` is the wildcard identifier
    // (never a binding you can read); `_foo`-style identifiers are ordinary
    // names". is01 accepted `_foo` as a CHOICE; it is now the production.
    let ebnf = production(&grammar(), "IDENT");
    assert!(ebnf.contains("'_'"), "the amended IDENT production moved");

    parse::parse_source("fn main() -> int {\n    let _unused = 1\n    0\n}\n")
        .expect("`_unused` is an ordinary identifier");
    // A bare `_` binds nothing but is legal in pattern position.
    parse::parse_source("fn main() -> int {\n    for _ in 0..3 { }\n    0\n}\n")
        .expect("`_` is the wildcard");
}

#[test]
fn freeze_takes_a_prefix_tier_operand() {
    // Amended `[gram.expr.region]`: `region_expr ::= … | 'freeze'
    // prefix_operand`, with the prose spelling out "`freeze r == x` means
    // `(freeze r) == x`". is01 filed this as a CHOICE; it is now the grammar.
    let ebnf = production(&grammar(), "region_expr");
    assert!(
        ebnf.contains("'freeze' prefix_operand"),
        "the amended freeze production moved: {ebnf}"
    );

    // `(freeze r) == x` parses; the whole-expr reading would swallow the `==`
    // and leave the `{` of the block dangling.
    parse::parse_source("fn main() -> int {\n    if freeze r == x { 0 } else { 1 }\n}\n")
        .expect("freeze binds tighter than `==`");
}

#[test]
fn the_in_header_is_a_no_struct_literal_position() {
    // Amended `[gram.amb.structlit]`: "No struct-literal expressions in
    // condition/`in`-header/scrutinee position". is01 inferred it; now it is
    // written down.
    let annex = section(&grammar(), "8. Ambiguity annex");
    assert!(
        annex.contains("`in`-header"),
        "the amended structlit annex entry moved"
    );
    parse::parse_source("fn main() -> int {\n    in r { let a = 1 }\n    0\n}\n")
        .expect("`in r { … }` keeps its block");
}

#[test]
fn e0006_spans_the_opening_brace() {
    // Amended §9: "E0006 (struct literal in condition; primary span = the
    // opening `{`)".
    let reservations = section(&grammar(), "9. Diagnostics");
    assert!(
        reservations.contains("primary span = the opening `{`"),
        "the amended E0006 reservation moved"
    );

    let source = "fn main() -> int {\n    if p == Point { x: 0 } { return 0 }\n    1\n}\n";
    let diag = parse::parse_source(source).expect_err("E0006");
    assert_eq!(diag.code, diag::E_STRUCT_LIT_IN_COND);
    assert_eq!(&source[diag.span.start..diag.span.end], "{");
}

#[test]
fn the_emitted_operator_climb_matches_our_transcription() {
    // The pin `cbde620` closes the gap both editor sprints flagged: `cargo
    // xtask spec-extract` now renders the §3.2 climb table into
    // `spec/grammar.ebnf` as explicit tier productions (`[gram.expr.prec]`),
    // tightest first, each carrying its tier number and associativity in a
    // trailing comment. That gives the spec's "one authoritative table, two
    // independent transcriptions" a *third* rendering — and this test diffs
    // it against is01's transcription mechanically, the same way the
    // markdown-table test above does. A difference is a finding, never a
    // local patch.
    let ebnf = spec("grammar.ebnf");
    let line = |name: &str| -> String {
        let needle = format!("{name} ::=");
        ebnf.lines()
            .find(|l| l.trim_start().starts_with(&needle))
            .unwrap_or_else(|| panic!("no `{needle}` production in the emitted grammar"))
            .split_once("::=")
            .expect("just matched")
            .1
            .to_owned()
    };

    // Tier 3: the prefix operators, exactly and in order.
    assert_eq!(
        quoted_terminals(&line("prefix_op")),
        parse::PREFIX_OPERATORS
            .iter()
            .map(|op| (*op).to_owned())
            .collect::<Vec<_>>(),
        "tier 3: the emitted prefix set differs from our transcription"
    );
    assert!(
        line("prefix_operand").contains("tier 3, prefix"),
        "tier 3's comment moved"
    );

    // Tiers 4–13: production name, operator spellings (in order), tier
    // number and associativity — all four read out of the emitted grammar
    // and compared against `parse::PRECEDENCE`.
    let tiers: &[(u8, &str)] = &[
        (4, "cast_expr"),
        (5, "mul_expr"),
        (6, "add_expr"),
        (7, "shift_expr"),
        (8, "bitand_expr"),
        (9, "bitxor_expr"),
        (10, "bitor_expr"),
        (11, "cmp_op"),
        (12, "and_expr"),
        (13, "or_expr"),
    ];
    for (tier, production) in tiers {
        let body = line(production);
        let ours = parse::PRECEDENCE
            .iter()
            .find(|(t, _, _)| t == tier)
            .unwrap_or_else(|| panic!("tier {tier} missing from our transcription"));
        assert_eq!(
            quoted_terminals(&body),
            ours.1.iter().map(|op| (*op).to_owned()).collect::<Vec<_>>(),
            "tier {tier} ({production}): operator sets differ"
        );
        // Tier 11's comment rides `cmp_expr`; every other tier annotates its
        // own production.
        let commented = if *tier == 11 { line("cmp_expr") } else { body };
        let assoc = match ours.2 {
            Assoc::Left => "left",
            Assoc::Right => "right",
            Assoc::None => "none",
        };
        assert!(
            commented.contains(&format!("tier {tier}, {assoc}")),
            "tier {tier} ({production}): the emitted grammar disagrees on tier \
             number or associativity: {commented}"
        );
    }

    // Tier 11's shape claim — comparisons do not chain — is structural in the
    // emitted grammar: `(cmp_op bitor_expr)?`, one optional rung, never `*`.
    let cmp = line("cmp_expr");
    assert!(
        cmp.contains(")?") && !cmp.contains(")*"),
        "tier 11 must be non-chaining in the emitted grammar: {cmp}"
    );
}

/// `"{"{"…"}"}"` — `depth` string literals, each inside the previous one's
/// interpolation.
fn nest_strings(depth: usize) -> String {
    let mut out = String::from("x");
    for _ in 0..depth {
        out = format!("\"{{{out}}}\"");
    }
    out
}
