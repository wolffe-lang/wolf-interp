//! lupin's OWN RFC 8259 reading — the `json_*` builtin tier's kernels
//! (is18).
//!
//! # Independence, stated where it lives
//!
//! The counterparty's reference is `wolf_mem::json` and the track's rule
//! forbids porting it: this module is written against RFC 8259, the s40
//! prelude signatures (`json_valid(str) -> bool`; `json_get`/`json_type
//! (str, str) -> str ! {parse, missing}`; `json_len(str, str) -> int !
//! {parse, missing, kind}`), and the two corpus witnesses
//! (`corpus/json/rows.lu`, `corpus/json/query.lu`). Where the witnesses are
//! silent, the surface was pinned EMPIRICALLY against the compiled lanes —
//! probing the counterparty's binary is the differential posture; reading
//! its source is not — and every such pin is marked "(probed)" below. A
//! divergence found later between the two independent parsers is the
//! system working: file it, don't paper it.
//!
//! The semantics:
//!
//! - **Errors are three kinds**, mapped onto D30 rows by the caller:
//!   [`Error::Parse`] (any RFC 8259 violation — one value per text,
//!   surrounding insignificant whitespace only), [`Error::Missing`] (a path
//!   that addresses nothing), [`Error::Kind`] (a node of the wrong kind for
//!   the operation — `len` of a scalar).
//! - **Paths** are dotted segments (`"pack.0.name"`; `""` is the root).
//!   Resolution is type-directed: on an ARRAY a segment of ASCII digits
//!   indexes (leading zeros read as the plain number — probed: `"01"` on
//!   `[1,2,3]` answers `2`; anything non-digit is [`Error::Missing`]); on
//!   an OBJECT any segment keys, digits included (probed: `"0"` on
//!   `{"0":5}` answers `5`), first occurrence winning over a duplicate key
//!   (probed). A segment into a scalar is [`Error::Missing`].
//! - **Rendering** (`get`): a string DECODES (escapes resolved, surrogate
//!   pairs combined); a number keeps its SOURCE spelling exactly (no
//!   i64/f64 rounding is introduced by the tier — `1e999` and
//!   `12345678901234567890123456789` round-trip); `true`/`false`/`null`
//!   literally. A container at the ROOT path renders as the whole text
//!   trimmed of surrounding whitespace (probed); a NESTED container
//!   renders compactly — no whitespace, strings re-encoded with `\\"`,
//!   `\\\\`, `\\t`, `\\n`, `\\r` and `\\u00xx` (lowercase hex) for the other
//!   control bytes, everything else literal UTF-8 (probed: `é` stays
//!   literal, U+0007 re-encodes as `\\u0007`, and the short escapes
//!   `\\b` and `\\f` re-encode long, as `\\u0008` and `\\u000c`).
//! - **`kind`** answers `object`/`array`/`str`/`num`/`bool`/`null` —
//!   `array` and `null` are `query.lu`'s pins; the rest are probed.
//! - **Depth** beyond [`MAX_DEPTH`] containers is [`Error::Parse`] — RFC
//!   8259 §9 allows implementations to set limits; this one is explicit,
//!   and its value is the empirically pinned boundary (129 nested arrays
//!   valid on the compiled lanes, 130 not).
//! - A **lone surrogate** in a `\uXXXX` escape is [`Error::Parse`]
//!   (probed: `"\ud800"` is invalid on the compiled lanes; RFC 8259
//!   tolerates either reading, the two implementations agree on strict).

/// Deepest container nesting accepted; one past it is a parse error.
pub const MAX_DEPTH: usize = 129;

/// The three honest failure kinds (the builtin arms map them onto rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Any RFC 8259 violation, depth included.
    Parse,
    /// A path that addresses nothing.
    Missing,
    /// A node of the wrong kind for the operation.
    Kind,
}

/// One parsed value, spanned into the source text.
#[derive(Debug)]
enum Node {
    /// Decoded key → value, in source order, duplicates kept.
    Object(Vec<(String, Node)>),
    Array(Vec<Node>),
    /// The decoded text.
    Str(String),
    /// Source spelling span.
    Num(std::ops::Range<usize>),
    Bool(bool),
    Null,
}

/// `json_valid`: is `text` one RFC 8259 value?
#[must_use]
pub fn valid(text: &str) -> bool {
    parse(text).is_ok()
}

/// `json_get`: the value at `path`, rendered per the module rules.
///
/// # Errors
///
/// [`Error::Parse`] or [`Error::Missing`].
pub fn get(text: &str, path: &str) -> Result<String, Error> {
    let root = parse(text)?;
    let node = resolve(&root, path)?;
    Ok(match node {
        Node::Str(s) => s.clone(),
        Node::Num(span) => text[span.clone()].to_owned(),
        Node::Bool(b) => if *b { "true" } else { "false" }.to_owned(),
        Node::Null => "null".to_owned(),
        Node::Object(_) | Node::Array(_) => {
            if path.is_empty() {
                // The root container is the whole text, trimmed (probed).
                text.trim_matches([' ', '\t', '\n', '\r']).to_owned()
            } else {
                let mut out = String::new();
                compact(node, text, &mut out);
                out
            }
        }
    })
}

/// `json_type`: the node's kind name.
///
/// # Errors
///
/// [`Error::Parse`] or [`Error::Missing`].
pub fn kind(text: &str, path: &str) -> Result<&'static str, Error> {
    let root = parse(text)?;
    Ok(match resolve(&root, path)? {
        Node::Object(_) => "object",
        Node::Array(_) => "array",
        Node::Str(_) => "str",
        Node::Num(_) => "num",
        Node::Bool(_) => "bool",
        Node::Null => "null",
    })
}

/// `json_len`: element count of an array, member count of an object.
///
/// # Errors
///
/// [`Error::Parse`], [`Error::Missing`], or [`Error::Kind`] for a scalar.
pub fn len(text: &str, path: &str) -> Result<usize, Error> {
    let root = parse(text)?;
    match resolve(&root, path)? {
        Node::Array(items) => Ok(items.len()),
        Node::Object(members) => Ok(members.len()),
        _ => Err(Error::Kind),
    }
}

// -- path resolution --------------------------------------------------------

fn resolve<'n>(root: &'n Node, path: &str) -> Result<&'n Node, Error> {
    if path.is_empty() {
        return Ok(root);
    }
    let mut node = root;
    for segment in path.split('.') {
        node = match node {
            Node::Array(items) => {
                // Digits index; anything else on an array addresses nothing.
                if segment.is_empty() || !segment.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(Error::Missing);
                }
                let index: usize = segment.parse().map_err(|_| Error::Missing)?;
                items.get(index).ok_or(Error::Missing)?
            }
            Node::Object(members) => members
                .iter()
                .find(|(key, _)| key == segment)
                .map(|(_, value)| value)
                .ok_or(Error::Missing)?,
            // A segment into a scalar addresses nothing.
            _ => return Err(Error::Missing),
        };
    }
    Ok(node)
}

// -- compact rendering ------------------------------------------------------

fn compact(node: &Node, text: &str, out: &mut String) {
    match node {
        Node::Object(members) => {
            out.push('{');
            for (index, (key, value)) in members.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                encode_str(key, out);
                out.push(':');
                compact(value, text, out);
            }
            out.push('}');
        }
        Node::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                compact(item, text, out);
            }
            out.push(']');
        }
        Node::Str(s) => encode_str(s, out),
        // Numbers keep their SOURCE spelling inside containers too — the
        // exactness rule does not stop at nesting.
        Node::Num(span) => out.push_str(&text[span.clone()]),
        Node::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Node::Null => out.push_str("null"),
    }
}

// -- parsing ----------------------------------------------------------------

struct Parser<'t> {
    text: &'t str,
    bytes: &'t [u8],
    at: usize,
}

fn parse(text: &str) -> Result<Node, Error> {
    let mut parser = Parser {
        text,
        bytes: text.as_bytes(),
        at: 0,
    };
    parser.skip_ws();
    let value = parser.value(0)?;
    parser.skip_ws();
    if parser.at != parser.bytes.len() {
        return Err(Error::Parse);
    }
    Ok(value)
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(b) = self.bytes.get(self.at) {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' => self.at += 1,
                _ => break,
            }
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.bytes.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Result<Node, Error> {
        match self.bytes.get(self.at) {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => Ok(Node::Str(self.string()?)),
            Some(b't') => self.literal("true", Node::Bool(true)),
            Some(b'f') => self.literal("false", Node::Bool(false)),
            Some(b'n') => self.literal("null", Node::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(Error::Parse),
        }
    }

    fn literal(&mut self, word: &str, node: Node) -> Result<Node, Error> {
        if self.text[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(node)
        } else {
            Err(Error::Parse)
        }
    }

    fn object(&mut self, depth: usize) -> Result<Node, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::Parse);
        }
        self.at += 1; // '{'
        let mut members = Vec::new();
        self.skip_ws();
        if self.eat(b'}') {
            return Ok(Node::Object(members));
        }
        loop {
            self.skip_ws();
            if self.bytes.get(self.at) != Some(&b'"') {
                return Err(Error::Parse);
            }
            let key = self.string()?;
            self.skip_ws();
            if !self.eat(b':') {
                return Err(Error::Parse);
            }
            self.skip_ws();
            let value = self.value(depth)?;
            members.push((key, value));
            self.skip_ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Node::Object(members));
            }
            return Err(Error::Parse);
        }
    }

    fn array(&mut self, depth: usize) -> Result<Node, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::Parse);
        }
        self.at += 1; // '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.eat(b']') {
            return Ok(Node::Array(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value(depth)?);
            self.skip_ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Node::Array(items));
            }
            return Err(Error::Parse);
        }
    }

    fn number(&mut self) -> Result<Node, Error> {
        let start = self.at;
        self.eat(b'-');
        // int = 0 | [1-9][0-9]*
        match self.bytes.get(self.at) {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.bytes.get(self.at), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return Err(Error::Parse),
        }
        if self.eat(b'.') {
            if !matches!(self.bytes.get(self.at), Some(b'0'..=b'9')) {
                return Err(Error::Parse);
            }
            while matches!(self.bytes.get(self.at), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        if matches!(self.bytes.get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.bytes.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !matches!(self.bytes.get(self.at), Some(b'0'..=b'9')) {
                return Err(Error::Parse);
            }
            while matches!(self.bytes.get(self.at), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        Ok(Node::Num(start..self.at))
    }

    fn string(&mut self) -> Result<String, Error> {
        self.at += 1; // '"'
        let mut out = String::new();
        loop {
            let rest = &self.text[self.at..];
            let mut chars = rest.char_indices();
            let Some((_, c)) = chars.next() else {
                return Err(Error::Parse);
            };
            match c {
                '"' => {
                    self.at += 1;
                    return Ok(out);
                }
                '\\' => {
                    self.at += 1;
                    out.push(self.escape()?);
                }
                // Unescaped control characters are invalid (RFC 8259 §7).
                c if (c as u32) < 0x20 => return Err(Error::Parse),
                c => {
                    out.push(c);
                    self.at += c.len_utf8();
                }
            }
        }
    }

    fn escape(&mut self) -> Result<char, Error> {
        let Some(&b) = self.bytes.get(self.at) else {
            return Err(Error::Parse);
        };
        self.at += 1;
        Ok(match b {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                let first = self.hex4()?;
                if (0xDC00..=0xDFFF).contains(&first) {
                    // A low surrogate with no high before it (probed: strict).
                    return Err(Error::Parse);
                }
                if (0xD800..=0xDBFF).contains(&first) {
                    // A high surrogate must pair with `\uDC00..\uDFFF`.
                    if !(self.eat(b'\\') && self.eat(b'u')) {
                        return Err(Error::Parse);
                    }
                    let second = self.hex4()?;
                    if !(0xDC00..=0xDFFF).contains(&second) {
                        return Err(Error::Parse);
                    }
                    let scalar = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
                    char::from_u32(scalar).ok_or(Error::Parse)?
                } else {
                    char::from_u32(first).ok_or(Error::Parse)?
                }
            }
            _ => return Err(Error::Parse),
        })
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let end = self.at.checked_add(4).ok_or(Error::Parse)?;
        let digits = self.text.get(self.at..end).ok_or(Error::Parse)?;
        let value = u32::from_str_radix(digits, 16).map_err(|_| Error::Parse)?;
        self.at = end;
        Ok(value)
    }
}

/// JSON-encode one string: `"` and `\` escape, `\t`/`\n`/`\r` short, other
/// controls as `\u00xx` lowercase, everything else literal UTF-8 (probed).
fn encode_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- the corpus witnesses' own shapes -----------------------------------

    #[test]
    fn the_rows_witness_answers_parse_missing_kind() {
        assert_eq!(get("{", "x"), Err(Error::Parse));
        assert_eq!(get("[1]", "9"), Err(Error::Missing));
        assert_eq!(len("[1]", "0"), Err(Error::Kind));
    }

    #[test]
    fn the_query_witness_decodes_strings_and_keeps_number_spellings() {
        assert!(valid("[1, 2, 3]"));
        let doc = r#"{"pack": [{"name": "lupin"}, {"name": "ainu"}], "n": 42, "a": [1, 2, 3], "b": null}"#;
        assert_eq!(get(doc, "pack.0.name").as_deref(), Ok("lupin"));
        assert_eq!(get(doc, "n").as_deref(), Ok("42"));
        assert_eq!(kind(doc, "a"), Ok("array"));
        assert_eq!(kind(doc, "b"), Ok("null"));
        assert_eq!(len(doc, "a"), Ok(3));
        assert_eq!(len(doc, "pack"), Ok(2));
    }

    // -- the probed pins (empirical agreement with the compiled lanes) ------

    #[test]
    fn kind_names_are_the_probed_vocabulary() {
        assert_eq!(kind(r#"{"a": 1}"#, ""), Ok("object"));
        assert_eq!(kind(r#""hi""#, ""), Ok("str"));
        assert_eq!(kind("3.5", ""), Ok("num"));
        assert_eq!(kind("true", ""), Ok("bool"));
        assert_eq!(kind("false", ""), Ok("bool"));
    }

    #[test]
    fn a_root_container_is_the_trimmed_source_and_a_nested_one_renders_compact() {
        assert_eq!(get("   [ 1 ]   ", "").as_deref(), Ok("[ 1 ]"));
        let doc = r#"{ "a" : { "b" : [ 1 , { "c" : "x y" } ] , "d" : "e\nf" , "e" : true } }"#;
        assert_eq!(
            get(doc, "a").as_deref(),
            Ok(r#"{"b":[1,{"c":"x y"}],"d":"e\nf","e":true}"#)
        );
        assert_eq!(get(doc, "a.b").as_deref(), Ok(r#"[1,{"c":"x y"}]"#));
    }

    #[test]
    fn compact_re_encoding_keeps_source_number_spellings_and_escapes_controls() {
        let doc = r#"{ "k" : [ "aéb" , "c\td" , "e\u0007f" , 1e999 ] }"#;
        // U+00E9 stays literal; tab keeps its short escape; U+0007 re-encodes
        // long with lowercase hex; the number keeps its spelling (probed).
        assert_eq!(
            get(doc, "k").as_deref(),
            Ok("[\"a\u{e9}b\",\"c\\td\",\"e\\u0007f\",1e999]")
        );
        // \b and \f re-encode LONG with lowercase hex (probed).
        let bf = r#"{ "w" : [ "a\bb\ff" ] }"#;
        assert_eq!(get(bf, "w").as_deref(), Ok(r#"["a\u0008b\u000cf"]"#));
    }

    #[test]
    fn string_leaves_decode_escapes_and_surrogate_pairs() {
        assert_eq!(get(r#""hAi""#, "").as_deref(), Ok("hAi"));
        // A surrogate pair combines: 😀 is U+1F600.
        assert_eq!(get(r#""a😀b""#, "").as_deref(), Ok("a\u{1f600}b"));
        // \b and \f DECODE in a string leaf.
        assert_eq!(get(r#""a\bb\ff""#, "").as_deref(), Ok("a\u{8}b\u{c}f"));
        // A lone surrogate is a parse error (probed: strict on every lane).
        assert!(!valid(r#""a\ud800b""#));
        assert!(!valid(r#""a\udc00b""#));
    }

    #[test]
    fn numbers_round_trip_their_source_spelling_without_rounding() {
        let big = "[ 1e999 , -0.0 , 12345678901234567890123456789 ]";
        assert_eq!(get("3.500e2", "").as_deref(), Ok("3.500e2"));
        assert_eq!(get(big, "0").as_deref(), Ok("1e999"));
        assert_eq!(
            get(big, "2").as_deref(),
            Ok("12345678901234567890123456789")
        );
    }

    #[test]
    fn paths_are_type_directed_digits_index_arrays_and_key_objects() {
        // A digit segment on an OBJECT keys it (probed).
        assert_eq!(get(r#"{ "0" : 5 }"#, "0").as_deref(), Ok("5"));
        // Leading zeros read as the plain number (probed: "01" answers 2).
        assert_eq!(get("[1, 2, 3]", "01").as_deref(), Ok("2"));
        // A negative index is not digits: missing.
        assert_eq!(get("[1, 2, 3]", "-1"), Err(Error::Missing));
        // Alpha on an array, and any segment into a scalar: missing.
        assert_eq!(get("[1]", "x"), Err(Error::Missing));
        assert_eq!(get("5", "0"), Err(Error::Missing));
        assert_eq!(get(r#"{ "a" : 1 }"#, "a.b"), Err(Error::Missing));
        // A duplicate key answers its FIRST occurrence (probed).
        assert_eq!(get(r#"{ "a" : 1, "a" : 2 }"#, "a").as_deref(), Ok("1"));
    }

    #[test]
    fn validity_is_one_value_with_surrounding_whitespace_only() {
        assert!(!valid("[1,]"));
        assert!(valid(" 1 "));
        assert!(!valid("1 2"));
        assert!(!valid(""));
        assert!(!valid("nul"));
        assert!(!valid("01"));
        assert!(!valid("+1"));
        assert!(valid("-0.0"));
        assert!(!valid("\"unterminated"));
        assert!(!valid("\"raw\tcontrol\""));
        assert!(valid("{}"));
        assert!(valid("[]"));
        assert!(!valid("{\"a\":}"));
        assert!(!valid("{\"a\" 1}"));
        assert!(!valid("[1 2]"));
    }

    #[test]
    fn the_depth_limit_is_the_probed_boundary() {
        let nested = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert!(valid(&nested(MAX_DEPTH)));
        assert!(!valid(&nested(MAX_DEPTH + 1)));
    }

    #[test]
    fn type_and_len_answer_missing_before_kind() {
        assert_eq!(kind(r#"{"a": 1}"#, "zz"), Err(Error::Missing));
        assert_eq!(len("[1, 2]", "5"), Err(Error::Missing));
        assert_eq!(len(r#""hi""#, ""), Err(Error::Kind));
        assert_eq!(len(r#"{"a": 1, "b": 2}"#, ""), Ok(2));
    }
}
