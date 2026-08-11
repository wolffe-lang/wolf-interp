//! The format-spec mini-language of `{x:spec}` interpolation holes.
//!
//! **Independently reimplemented** from the §7.4 amendment candidate
//! (wolf-lang#28) and the pinned corpus witnesses — see CONTRIBUTING.md.
//! The grammar, exactly the candidate text:
//!
//! ```text
//! FORMAT_SPEC ::= [[fill] align] [sign] ['0'] [width] ['.' precision] [type]
//! fill        ::= <any single byte at v1>
//! align       ::= '<' | '^' | '>'
//! sign        ::= '+'
//! width       ::= DIGIT+
//! precision   ::= DIGIT+
//! type        ::= 'b' | 'o' | 'x' | 'X' | 'e' | 'E' | 'f'
//! ```
//!
//! The corpus pins the semantics member by member:
//!
//! - `corpus/strings/format_spec_width.lu` — width is a minimum in BYTES
//!   (D25's currency) and never truncates; fill defaults to a space;
//!   numbers default right, `str`/`bool` default left.
//! - `corpus/strings/format_spec_full.lu` — `+` marks non-negative
//!   numbers and zero takes it; `'0'` zero-pads AFTER the sign, and
//!   `{n:08}` is the zero FLAG plus width 8, **never** width 8 with
//!   space fill (#28 item 3 — this machine's 0.1.4 reading, retired);
//!   the bases are sign-magnitude with no prefix; `.N` on a `str` is a
//!   byte cap that never splits a code point; `^` centers with the odd
//!   byte on the RIGHT.
//! - `corpus/strings/float_format.lu` — bare `.N` on a float means
//!   fixed-point; `e`/`E` carry a signed, at-least-two-digit exponent;
//!   the default float rendering is the SHORTEST decimal that reads
//!   back as the same bits; `+` never hides `-0.0` and marks `+0.0`.
//! - `corpus/strings/format_spec_malformed.lu` /
//!   `format_spec_mismatch.lu` — a malformed spec is E0412 and a
//!   well-formed spec off its hole's type is E0413, both COMPILE errors
//!   at the literal (sema-lite's rung); the runtime half here refuses
//!   rather than guesses when a spec reaches it anyway (a hole type the
//!   checker could not classify), because a spec that silently does
//!   nothing is a wrong answer (wolf-lang#10).
//!
//! v1 bounds, diagnosed rather than guessed: `fill` is a single BYTE (a
//! multi-byte fill cannot hit an exact byte width), and
//! `width`/`precision` cap at 65535.

use std::fmt;

/// Alignment inside a padded field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// The `type` selector: an integer base or a float notation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Bin,
    Oct,
    Hex,
    HexUpper,
    Exp,
    ExpUpper,
    Fixed,
}

impl Kind {
    /// The spec character that selects this kind.
    #[must_use]
    pub fn ch(self) -> char {
        match self {
            Kind::Bin => 'b',
            Kind::Oct => 'o',
            Kind::Hex => 'x',
            Kind::HexUpper => 'X',
            Kind::Exp => 'e',
            Kind::ExpUpper => 'E',
            Kind::Fixed => 'f',
        }
    }
}

/// A parsed, shape-valid format spec.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FormatSpec {
    /// Fill byte; `None` means space. Present only with an alignment.
    pub fill: Option<u8>,
    /// Explicit alignment; `None` = the hole class's default.
    pub align: Option<Align>,
    /// `'+'` — print the sign on non-negative numbers.
    pub sign: bool,
    /// `'0'` — zero-pad after the sign.
    pub zero: bool,
    pub width: Option<u16>,
    pub precision: Option<u16>,
    pub kind: Option<Kind>,
}

impl FormatSpec {
    /// Is this the empty spec (`{x:}` — everything default)?
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == FormatSpec::default()
    }
}

/// Why a spec is malformed (E0412's payload), precise enough to teach
/// the grammar.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SpecError {
    /// A character the grammar has no place for, at byte offset `at`
    /// into the spec text (after the `:`).
    Unexpected { at: usize, ch: char },
    /// `'0'` combined with an explicit fill/align — the spec must pick
    /// one spelling; the machine never picks silently.
    ZeroWithAlign,
    /// `'.'` with no digits after it.
    MissingPrecision,
    /// A multi-byte fill cannot hit an exact byte width (D25).
    MultiByteFill(char),
    /// An alignment character used as the fill (`{x:>>8}`) — it reads
    /// as a typo, never as intent.
    AlignAsFill(char),
    /// width/precision beyond the v1 cap of 65535.
    TooWide { what: &'static str },
}

impl SpecError {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            SpecError::Unexpected { ch, .. } => format!(
                "`{ch}` has no place in a format spec — the grammar is \
                 `[[fill]align][+][0][width][.precision][type]` with type one of `b o x X e E f`"
            ),
            SpecError::ZeroWithAlign => "`0` zero-pads after the sign and cannot combine with \
                                         an explicit fill or alignment — spell one or the other"
                .to_owned(),
            SpecError::MissingPrecision => {
                "`.` must be followed by the precision digits (like `.2`)".to_owned()
            }
            SpecError::MultiByteFill(c) => format!(
                "`{c}` is a multi-byte fill — width counts bytes (D25), so the fill must be \
                 a single byte"
            ),
            SpecError::AlignAsFill(c) => format!(
                "`{c}{c}` reads as a doubled alignment, not as \"fill with `{c}`\" — \
                 alignment characters cannot be the fill"
            ),
            SpecError::TooWide { what } => format!("this {what} is larger than the 65535 cap"),
        }
    }
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

/// What a spec can apply to — the hole's value, classified.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HoleClass {
    Str,
    Bool,
    Int,
    Float,
}

impl HoleClass {
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            HoleClass::Str => "`str`",
            HoleClass::Bool => "`bool`",
            HoleClass::Int => "an integer",
            HoleClass::Float => "a float",
        }
    }
}

/// A well-formed spec that does not fit the hole's class (E0413's
/// payload).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mismatch {
    /// `+` on a non-number.
    SignOn(HoleClass),
    /// `0` on a non-number.
    ZeroOn(HoleClass),
    /// `.N` on an integer or bool.
    PrecisionOn(HoleClass),
    /// A base kind (`b o x X`) off integers, or a float kind (`e E f`)
    /// off floats.
    KindOn { kind: Kind, class: HoleClass },
}

impl Mismatch {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Mismatch::SignOn(c) => format!(
                "`+` prints a sign on numbers, and this value is {}",
                c.describe()
            ),
            Mismatch::ZeroOn(c) => format!(
                "`0` zero-pads a number's digits, and this value is {}",
                c.describe()
            ),
            Mismatch::PrecisionOn(c) => format!(
                "`.precision` means digits-after-the-point on a float and a byte cap on a \
                 `str`; this value is {}",
                c.describe()
            ),
            Mismatch::KindOn { kind, class } => match kind {
                Kind::Bin | Kind::Oct | Kind::Hex | Kind::HexUpper => format!(
                    "`{}` renders an integer in another base, and this value is {}",
                    kind.ch(),
                    class.describe()
                ),
                Kind::Exp | Kind::ExpUpper | Kind::Fixed => format!(
                    "`{}` is a float notation, and this value is {}",
                    kind.ch(),
                    class.describe()
                ),
            },
        }
    }
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

/// Parses a spec's text (WITHOUT the leading `:`). The empty string is
/// the default spec.
///
/// # Errors
///
/// [`SpecError`] — the malformed-spec taxonomy E0412 reports.
pub fn parse(s: &str) -> Result<FormatSpec, SpecError> {
    let chars: Vec<char> = s.chars().collect();
    let mut spec = FormatSpec::default();
    let mut i = 0usize;
    let is_align = |c: char| matches!(c, '<' | '^' | '>');
    let to_align = |c: char| match c {
        '<' => Align::Left,
        '^' => Align::Center,
        _ => Align::Right,
    };
    // [[fill] align] — the two-character form reads as fill + align, so
    // `0>4` is a zero FILL, distinct from the zero FLAG below.
    if chars.len() >= 2 && is_align(chars[1]) {
        let fill = chars[0];
        if is_align(fill) {
            return Err(SpecError::AlignAsFill(fill));
        }
        if fill.len_utf8() != 1 {
            return Err(SpecError::MultiByteFill(fill));
        }
        spec.fill = Some(fill as u8);
        spec.align = Some(to_align(chars[1]));
        i = 2;
    } else if !chars.is_empty() && is_align(chars[0]) {
        spec.align = Some(to_align(chars[0]));
        i = 1;
    }
    // [sign]
    if i < chars.len() && chars[i] == '+' {
        spec.sign = true;
        i += 1;
    }
    // ['0'] — the flag. A lone `0` is the flag with no width; `08` is
    // the flag plus width 8 (never width 8 with space fill).
    if i < chars.len() && chars[i] == '0' {
        if spec.align.is_some() {
            return Err(SpecError::ZeroWithAlign);
        }
        spec.zero = true;
        i += 1;
    }
    // [width]
    let width_start = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i > width_start {
        spec.width = Some(parse_bounded(&chars[width_start..i], "width")?);
    }
    // ['.' precision]
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let p_start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i == p_start {
            return Err(SpecError::MissingPrecision);
        }
        spec.precision = Some(parse_bounded(&chars[p_start..i], "precision")?);
    }
    // [type]
    if i < chars.len() {
        spec.kind = Some(match chars[i] {
            'b' => Kind::Bin,
            'o' => Kind::Oct,
            'x' => Kind::Hex,
            'X' => Kind::HexUpper,
            'e' => Kind::Exp,
            'E' => Kind::ExpUpper,
            'f' => Kind::Fixed,
            ch => {
                return Err(SpecError::Unexpected {
                    at: byte_offset(&chars, i),
                    ch,
                });
            }
        });
        i += 1;
    }
    if i < chars.len() {
        return Err(SpecError::Unexpected {
            at: byte_offset(&chars, i),
            ch: chars[i],
        });
    }
    Ok(spec)
}

fn byte_offset(chars: &[char], upto: usize) -> usize {
    chars[..upto].iter().map(|c| c.len_utf8()).sum()
}

fn parse_bounded(digits: &[char], what: &'static str) -> Result<u16, SpecError> {
    let text: String = digits.iter().collect();
    let n: u32 = text.parse().map_err(|_| SpecError::TooWide { what })?;
    u16::try_from(n).map_err(|_| SpecError::TooWide { what })
}

/// Does a well-formed spec fit a hole of this class?
///
/// # Errors
///
/// [`Mismatch`] — the type-mismatch taxonomy E0413 reports.
pub fn validate(spec: &FormatSpec, class: HoleClass) -> Result<(), Mismatch> {
    let numeric = matches!(class, HoleClass::Int | HoleClass::Float);
    if spec.sign && !numeric {
        return Err(Mismatch::SignOn(class));
    }
    if spec.zero && !numeric {
        return Err(Mismatch::ZeroOn(class));
    }
    if spec.precision.is_some() && !matches!(class, HoleClass::Str | HoleClass::Float) {
        return Err(Mismatch::PrecisionOn(class));
    }
    if let Some(kind) = spec.kind {
        let fits = match kind {
            Kind::Bin | Kind::Oct | Kind::Hex | Kind::HexUpper => class == HoleClass::Int,
            Kind::Exp | Kind::ExpUpper | Kind::Fixed => class == HoleClass::Float,
        };
        if !fits {
            return Err(Mismatch::KindOn { kind, class });
        }
    }
    Ok(())
}

/// A hole value at rendering time. This machine's integers are `i128`
/// all the way down ([`crate::eval::value::Value::Int`]); the unsigned
/// types' values are non-negative there, so sign-magnitude rendering
/// needs no unsigned flag.
#[derive(Clone, Copy, Debug)]
pub enum FmtValue<'a> {
    Str(&'a str),
    Bool(bool),
    Int(i128),
    F64(f64),
}

impl FmtValue<'_> {
    #[must_use]
    pub fn class(&self) -> HoleClass {
        match self {
            FmtValue::Str(_) => HoleClass::Str,
            FmtValue::Bool(_) => HoleClass::Bool,
            FmtValue::Int(_) => HoleClass::Int,
            FmtValue::F64(_) => HoleClass::Float,
        }
    }
}

/// Renders a value under a spec.
///
/// # Errors
///
/// [`Mismatch`] when the spec does not fit the value's class — the
/// checker rules those out statically (E0413); a caller that can still
/// see one refuses honestly rather than ignoring the spec.
pub fn apply(spec: &FormatSpec, v: FmtValue<'_>) -> Result<String, Mismatch> {
    validate(spec, v.class())?;
    let core = match v {
        FmtValue::Str(s) => match spec.precision {
            // A byte cap that never splits a code point: truncate to the
            // largest boundary at or below it.
            Some(p) => {
                let mut end = (p as usize).min(s.len());
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                s[..end].to_owned()
            }
            None => s.to_owned(),
        },
        FmtValue::Bool(b) => b.to_string(),
        FmtValue::Int(n) => render_int(n, spec),
        FmtValue::F64(x) => render_f64(x, spec),
    };
    Ok(pad(core, v, spec))
}

/// Integer core: optional `+`, sign-magnitude base rendering with no
/// prefix (`-255` in `x` is `-ff`; `i128::MIN` renders).
fn render_int(n: i128, spec: &FormatSpec) -> String {
    let neg = n < 0;
    let mag = n.unsigned_abs();
    let digits = match spec.kind {
        None => mag.to_string(),
        Some(Kind::Bin) => format!("{mag:b}"),
        Some(Kind::Oct) => format!("{mag:o}"),
        Some(Kind::Hex) => format!("{mag:x}"),
        Some(Kind::HexUpper) => format!("{mag:X}"),
        // validate() rules float kinds off integers.
        Some(Kind::Exp | Kind::ExpUpper | Kind::Fixed) => mag.to_string(),
    };
    let sign = if neg {
        "-"
    } else if spec.sign {
        "+"
    } else {
        ""
    };
    format!("{sign}{digits}")
}

/// Float core: shortest / fixed / exponent per the kind; the `+` flag
/// marks `+0.0` and `+inf`, never hides `-0.0`, and never marks `nan`.
fn render_f64(x: f64, spec: &FormatSpec) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return match (x < 0.0, spec.sign) {
            (true, _) => "-inf".to_owned(),
            (false, true) => "+inf".to_owned(),
            (false, false) => "inf".to_owned(),
        };
    }
    let plus = spec.sign && !x.is_sign_negative();
    let body = match spec.kind {
        // Bare `.N` means fixed-point (the wolf-lang#10 headline
        // `{x:>8.2}` reads as fixed with 2 digits).
        None => match spec.precision {
            Some(p) => {
                let prec = p as usize;
                format!("{x:.prec$}")
            }
            None => f64_shortest(x),
        },
        Some(Kind::Fixed) => {
            // Missing precision under `f`/`e`/`E` defaults to 6 (the C
            // default).
            let prec = spec.precision.unwrap_or(6) as usize;
            format!("{x:.prec$}")
        }
        Some(Kind::Exp | Kind::ExpUpper) => {
            let prec = spec.precision.unwrap_or(6) as usize;
            let rendered = format!("{x:.prec$e}");
            let (mantissa, exp) = rendered
                .split_once('e')
                .expect("`{:e}` carries an exponent");
            let exponent: i64 = exp.parse().expect("exponent digits");
            let (es, mag) = if exponent < 0 {
                ('-', -exponent)
            } else {
                ('+', exponent)
            };
            let letter = if spec.kind == Some(Kind::ExpUpper) {
                'E'
            } else {
                'e'
            };
            format!("{mantissa}{letter}{es}{mag:02}")
        }
        // validate() rules base kinds off floats.
        Some(_) => f64_shortest(x),
    };
    if plus { format!("+{body}") } else { body }
}

/// The SHORTEST decimal that reads back as the same bits: positional
/// while the decimal exponent of the leading digit is in `-4..=16`,
/// scientific outside it (`1e+23`-shaped, exponent signed and at least
/// two digits). `corpus/strings/float_format.lu` pins the layout.
#[must_use]
pub fn f64_shortest(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf" } else { "inf" }.to_owned();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }
    // Rust's `{:e}` is the shortest round-trip mantissa in `d[.ddd]e±E`
    // form; only the layout around the point is this machine's.
    let rendered = format!("{x:e}");
    let (mantissa, exp) = rendered
        .split_once('e')
        .expect("`{:e}` carries an exponent");
    let exponent: i64 = exp.parse().expect("exponent digits");
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|&c| c != '.').collect();
    if !(-4..=16).contains(&exponent) {
        let head = &digits[..1];
        let tail = &digits[1..];
        let mantissa = if tail.is_empty() {
            head.to_owned()
        } else {
            format!("{head}.{tail}")
        };
        let (es, mag) = if exponent < 0 {
            ('-', -exponent)
        } else {
            ('+', exponent)
        };
        return format!("{sign}{mantissa}e{es}{mag:02}");
    }
    let count = digits.len() as i64;
    if exponent < 0 {
        let zeros = "0".repeat(usize::try_from(-exponent - 1).unwrap_or_default());
        return format!("{sign}0.{zeros}{digits}");
    }
    if exponent >= count - 1 {
        let zeros = "0".repeat(usize::try_from(exponent - count + 1).unwrap_or_default());
        return format!("{sign}{digits}{zeros}");
    }
    let (head, tail) = digits.split_at(usize::try_from(exponent + 1).unwrap_or_default());
    format!("{sign}{head}.{tail}")
}

/// Width/fill/alignment (and the zero-pad path) around a rendered core.
/// Width counts BYTES (D25) and never truncates.
fn pad(core: String, v: FmtValue<'_>, spec: &FormatSpec) -> String {
    let width = spec.width.unwrap_or(0) as usize;
    if width <= core.len() {
        return core;
    }
    let missing = width - core.len();
    if spec.zero {
        // Zeros go between the sign and the digits. A non-finite float
        // has no digit run to extend — space-pad right-aligned instead.
        let finite = match v {
            FmtValue::F64(x) => x.is_finite(),
            _ => true,
        };
        if finite {
            let (sign, rest) = match core.strip_prefix(['-', '+']) {
                Some(rest) => (&core[..1], rest),
                None => ("", core.as_str()),
            };
            return format!("{sign}{}{rest}", "0".repeat(missing));
        }
        return format!("{}{core}", " ".repeat(missing));
    }
    let fill = char::from(spec.fill.unwrap_or(b' '));
    let align = spec.align.unwrap_or(match v.class() {
        HoleClass::Int | HoleClass::Float => Align::Right,
        HoleClass::Str | HoleClass::Bool => Align::Left,
    });
    let fills = |n: usize| fill.to_string().repeat(n);
    match align {
        Align::Left => format!("{core}{}", fills(missing)),
        Align::Right => format!("{}{core}", fills(missing)),
        Align::Center => {
            // The odd fill byte goes RIGHT (`{a:^5}` of "a" pins it).
            let left = missing / 2;
            format!("{}{core}{}", fills(left), fills(missing - left))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> FormatSpec {
        parse(s).expect("spec parses")
    }

    fn fmt(s: &str, v: FmtValue<'_>) -> String {
        apply(&p(s), v).expect("spec fits")
    }

    #[test]
    fn the_grammar_parses_every_field() {
        assert_eq!(p(""), FormatSpec::default());
        assert!(p("").is_default());
        assert_eq!(
            p("*>8"),
            FormatSpec {
                fill: Some(b'*'),
                align: Some(Align::Right),
                width: Some(8),
                ..FormatSpec::default()
            }
        );
        assert_eq!(
            p("+08.2f"),
            FormatSpec {
                sign: true,
                zero: true,
                width: Some(8),
                precision: Some(2),
                kind: Some(Kind::Fixed),
                ..FormatSpec::default()
            }
        );
        // A digit fill is legal when an align follows it: fill + align,
        // not the zero flag.
        assert_eq!(p("0>4").fill, Some(b'0'));
        assert!(!p("0>4").zero);
        // `{n:08}` is the zero FLAG plus width 8 — never width 8 with
        // space fill (#28 item 3, this machine's 0.1.4 bug).
        let z = p("08");
        assert!(z.zero);
        assert_eq!(z.width, Some(8));
    }

    #[test]
    fn the_grammar_refuses_what_it_cannot_mean() {
        assert!(matches!(parse("q"), Err(SpecError::Unexpected { .. })));
        assert!(matches!(parse(">>8"), Err(SpecError::AlignAsFill('>'))));
        assert!(matches!(parse("8."), Err(SpecError::MissingPrecision)));
        assert!(matches!(parse("0>08"), Err(SpecError::ZeroWithAlign)));
        assert!(matches!(parse(">08"), Err(SpecError::ZeroWithAlign)));
        assert!(matches!(parse("é>4"), Err(SpecError::MultiByteFill('é'))));
        assert!(matches!(parse("99999"), Err(SpecError::TooWide { .. })));
        assert!(matches!(parse("8.99999"), Err(SpecError::TooWide { .. })));
        assert!(matches!(parse("4x2"), Err(SpecError::Unexpected { .. })));
    }

    #[test]
    fn mismatches_are_ruled_out_class_by_class() {
        assert!(validate(&p(".2"), HoleClass::Int).is_err());
        assert!(validate(&p("x"), HoleClass::Str).is_err());
        assert!(validate(&p("+"), HoleClass::Bool).is_err());
        assert!(validate(&p("08"), HoleClass::Str).is_err());
        assert!(validate(&p("e"), HoleClass::Int).is_err());
        assert!(validate(&p("x"), HoleClass::Float).is_err());
        assert!(validate(&p(".8"), HoleClass::Str).is_ok());
        assert!(validate(&p("+08.2f"), HoleClass::Float).is_ok());
    }

    #[test]
    fn width_pads_in_bytes_and_never_truncates() {
        assert_eq!(fmt(">8", FmtValue::Str("hi")), "      hi");
        assert_eq!(fmt("<8", FmtValue::Str("hi")), "hi      ");
        assert_eq!(fmt("*>8", FmtValue::Int(42)), "******42");
        assert_eq!(fmt(">3", FmtValue::Str("wolves")), "wolves");
        // Width is BYTES: "é" is 2 of them.
        assert_eq!(fmt(">3", FmtValue::Str("é")), " é");
        // Defaults: numbers right, str/bool left.
        assert_eq!(fmt("8", FmtValue::Str("hi")), "hi      ");
        assert_eq!(fmt("8", FmtValue::Bool(true)), "true    ");
        assert_eq!(fmt("6", FmtValue::Int(42)), "    42");
        // `^` centers with the odd byte on the RIGHT.
        assert_eq!(fmt("^4", FmtValue::Str("a")), " a  ");
        assert_eq!(fmt("^5", FmtValue::Str("a")), "  a  ");
    }

    #[test]
    fn sign_zero_flag_and_bases_render_sign_magnitude() {
        assert_eq!(fmt("+", FmtValue::Int(42)), "+42");
        assert_eq!(fmt("+", FmtValue::Int(0)), "+0");
        assert_eq!(fmt("+", FmtValue::Int(-42)), "-42");
        // Zero-pads AFTER the sign.
        assert_eq!(fmt("06", FmtValue::Int(-42)), "-00042");
        assert_eq!(fmt("08", FmtValue::Int(42)), "00000042");
        assert_eq!(fmt("+06", FmtValue::Int(42)), "+00042");
        // Bases: sign-magnitude, no prefix.
        assert_eq!(fmt("x", FmtValue::Int(-255)), "-ff");
        assert_eq!(fmt("X", FmtValue::Int(255)), "FF");
        assert_eq!(fmt("b", FmtValue::Int(5)), "101");
        assert_eq!(fmt("o", FmtValue::Int(8)), "10");
        assert_eq!(fmt(">8x", FmtValue::Int(42)), "      2a");
        // The extreme magnitude survives sign-magnitude.
        assert_eq!(
            fmt("x", FmtValue::Int(i128::from(i64::MIN))),
            "-8000000000000000"
        );
    }

    #[test]
    fn str_precision_is_a_byte_cap_on_code_point_boundaries() {
        assert_eq!(fmt(".2", FmtValue::Str("wolf")), "wo");
        assert_eq!(fmt(".1", FmtValue::Str("é")), "");
        assert_eq!(fmt(".3", FmtValue::Str("aé")), "aé");
        assert_eq!(fmt(".2", FmtValue::Str("aé")), "a");
        assert_eq!(fmt(">4.2", FmtValue::Str("wolf")), "  wo");
    }

    #[test]
    #[allow(clippy::approx_constant)] // 3.14159 is the corpus witness value, not π
    fn float_fixed_and_exp_notations() {
        // Fixed: exact value, ties to even.
        assert_eq!(fmt(".0f", FmtValue::F64(0.5)), "0");
        assert_eq!(fmt(".0f", FmtValue::F64(1.5)), "2");
        assert_eq!(fmt(".0f", FmtValue::F64(2.5)), "2");
        assert_eq!(fmt(".2f", FmtValue::F64(-0.0)), "-0.00");
        assert_eq!(fmt(".2f", FmtValue::F64(3.14159)), "3.14");
        // Exponent: signed, at least two digits.
        assert_eq!(fmt(".2e", FmtValue::F64(1.5)), "1.50e+00");
        assert_eq!(fmt(".0e", FmtValue::F64(1.5)), "2e+00");
        assert_eq!(fmt(".2e", FmtValue::F64(0.0)), "0.00e+00");
        assert_eq!(fmt(".2E", FmtValue::F64(1.5)), "1.50E+00");
        // Bare precision means fixed (`{x:>8.2}`).
        assert_eq!(fmt(">8.2", FmtValue::F64(3.14159)), "    3.14");
        // Sign + zero-pad on floats.
        assert_eq!(fmt("+.2f", FmtValue::F64(0.0)), "+0.00");
        assert_eq!(fmt("08.2f", FmtValue::F64(-1.5)), "-0001.50");
        // Missing precision under a float kind defaults to 6.
        assert_eq!(fmt("f", FmtValue::F64(1.5)), "1.500000");
    }

    #[test]
    #[allow(clippy::excessive_precision)] // the 17-digit pins ARE the point
    fn float_shortest_reads_back_as_the_same_bits() {
        assert_eq!(f64_shortest(3.0), "3");
        assert_eq!(f64_shortest(0.1), "0.1");
        assert_eq!(f64_shortest(1.5), "1.5");
        assert_eq!(f64_shortest(-0.0), "-0");
        assert_eq!(f64_shortest(0.0), "0");
        assert_eq!(f64_shortest(9.999999999999999e22), "1e+23");
        assert_eq!(f64_shortest(1e16), "10000000000000000");
        assert_eq!(f64_shortest(1e17), "1e+17");
        assert_eq!(f64_shortest(1e-4), "0.0001");
        assert_eq!(f64_shortest(1e-5), "1e-05");
        assert_eq!(f64_shortest(f64::NAN), "nan");
        assert_eq!(f64_shortest(f64::INFINITY), "inf");
        assert_eq!(f64_shortest(f64::NEG_INFINITY), "-inf");
        for x in [0.3, 2.5e-12, 123_456.789, 1.797_693_134_862_315_7e308] {
            assert_eq!(
                f64_shortest(x)
                    .parse::<f64>()
                    .expect("reads back")
                    .to_bits(),
                x.to_bits()
            );
        }
    }

    #[test]
    fn nonfinite_floats_stay_honest_under_flags() {
        assert_eq!(fmt("+.2f", FmtValue::F64(f64::INFINITY)), "+inf");
        assert_eq!(fmt(".2f", FmtValue::F64(f64::NAN)), "nan");
        // `+` never marks nan.
        assert_eq!(fmt("+", FmtValue::F64(f64::NAN)), "nan");
        // Zero-padding a non-finite falls back to right-aligned spaces.
        assert_eq!(fmt("06.2f", FmtValue::F64(f64::NAN)), "   nan");
    }
}
