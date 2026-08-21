//! The conformance bundle — is09, the track's publishable artifact.
//!
//! `wolf-interp conformance export` turns the discipline the whole track has
//! been paying for — every semantic rule tagged with its spec clause, every
//! test tagged with what it checks — into a **versioned, self-contained,
//! byte-deterministic** bundle: the pinned corpus, this implementation's
//! protocol-1 observation of every entry (the *reference outcomes*), the
//! closed trap and UB vocabularies, the pinned anchor registry, and the
//! coverage matrix with its debt list (the honesty document). The format is
//! documented in `docs/conformance-bundle.md` as the `[proto]` extension it
//! is; schema version 1 is frozen.
//!
//! `lupin conformance check <bundle> --impl <cmd>` is is05's differ
//! generalized: the counterparty is *any* conform-run-speaking command, and
//! the interpreter side of the comparison is replayed from the bundle's
//! recorded reference outcomes rather than recomputed — which is what makes
//! the bundle a conformance gate another machine can hold against another
//! implementation without building this one.
//!
//! # Determinism (the I10 spirit)
//!
//! Re-export at the same (interpreter, pin) commits is byte-identical, on
//! every platform. The lessons that buy this are all applied at the
//! boundary: text is CRLF-normalized **before** observation (spans are byte
//! offsets into what ships), paths are `/`-separated everywhere, walks are
//! sorted by the relative slash path, JSON maps are `BTreeMap`s, and the
//! manifest's `bundle_sha256` — the one number CI compares across the three
//! OSes — is a digest of the sorted per-file hash list.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::anchor::{self, Namespace};
use crate::differ::{self, DeepDivergence, FileComparison, Invocation, LedgerEntry};
use crate::directive::{self, Directives};
use crate::eval::prov::{Coverage, UbRow};
use crate::protocol::ObservationRecord;
use crate::schema;
use crate::sha256;
use crate::trap::TrapKind;

/// The bundle schema version this module writes and reads. **Frozen**:
/// extending the bundle beyond it is a new version, never a quiet edit.
pub const BUNDLE_SCHEMA: u64 = 1;

/// The corpus formatter's style version (compiler-track finding, s13):
/// corpus bytes are formatter-canonical, and a style bump is expected churn
/// the manifest must make visible rather than absorb.
pub const STYLE_VERSION: u64 = 1;

/// The local suites that ride along with the pinned corpus: is04's UB
/// triggers and twins, is03's fault litmuses, is07's model-check witnesses.
/// All are written in the corpus's own directive dialect (upstream-ready
/// as-is), which is what lets one walker consume both trees.
pub const SUITES: [&str; 3] = ["ub", "faults", "witness"];

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Where the exporter reads from and writes to.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// The pinned corpus root.
    pub corpus: PathBuf,
    /// The pinned spec root (for `anchors.json` and the cross-check).
    pub spec: PathBuf,
    /// The directory holding the local suites (`tests/`).
    pub suites: PathBuf,
    /// The `[repl.*]` notes that ride along as documentation (is08).
    pub repl_doc: PathBuf,
    /// The pin file naming the upstream commit the corpus and spec are at.
    pub pin: PathBuf,
    /// The bundle directory to (re)create.
    pub out: PathBuf,
}

impl ExportOptions {
    /// The conventional roots, relative to the repository checkout.
    #[must_use]
    pub fn conventional(out: &Path) -> ExportOptions {
        let upstream = Path::new(crate::upstream_root());
        ExportOptions {
            corpus: upstream.join("corpus"),
            spec: upstream.join("spec"),
            suites: PathBuf::from("tests"),
            repl_doc: PathBuf::from("docs/repl.md"),
            pin: PathBuf::from("vendor/upstream/PIN"),
            out: out.to_owned(),
        }
    }
}

/// What one export produced — the numbers the human summary and the CI
/// assertions read.
#[derive(Debug, Clone)]
pub struct ExportSummary {
    pub pin: String,
    pub files: usize,
    pub programs: usize,
    pub records: usize,
    pub anchors_total: usize,
    pub anchors_covered: usize,
    pub forward_tags: usize,
    pub bundle_sha256: String,
}

/// One program headed into the bundle: its bundle-relative path, its
/// normalized bytes, and its parsed directive header.
struct Program {
    bundle_path: String,
    source: Vec<u8>,
    directives: Directives,
}

/// Exports the bundle, deterministically.
///
/// # Errors
///
/// Anything that would make the bundle a lie fails the export loudly: an
/// unreadable input, a malformed directive header, an anchor-registry
/// cross-check mismatch (an upstream finding, never worked around), or a
/// record this implementation's own schema validator rejects.
pub fn export(options: &ExportOptions) -> Result<ExportSummary, String> {
    let pin = read_pin(&options.pin)?;
    let registry = read_registry(&options.spec)?;
    for notice in cross_check_registry(&options.spec, &registry)? {
        eprintln!("{notice}");
    }

    // -- gather every program, corpus first, suites after -------------------
    let mut programs = Vec::new();
    let mut extra_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    for (relative, bytes) in walk_files(&options.corpus)? {
        let bundle_path = format!("corpus/{relative}");
        if relative.ends_with(".lu") {
            programs.push(parse_program(bundle_path, normalize(&bytes))?);
        } else {
            extra_files.insert(bundle_path, normalize(&bytes));
        }
    }
    for suite in SUITES {
        let root = options.suites.join(suite);
        for (relative, bytes) in walk_files(&root)? {
            if !relative.ends_with(".lu") {
                continue; // suite READMEs are repo docs, not bundle content
            }
            let bundle_path = format!("suite/{suite}/{relative}");
            programs.push(parse_program(bundle_path, normalize(&bytes))?);
        }
    }
    programs.sort_by(|a, b| a.bundle_path.cmp(&b.bundle_path));

    // -- write the program tree first ---------------------------------------
    // Observation happens *from the bundle*: a multi-file module resolves its
    // members from the directory of the file being observed, so the entry
    // must be observed where its members are — which also proves the bundle
    // is genuinely self-contained.
    if options.out.exists() {
        fs::remove_dir_all(&options.out)
            .map_err(|e| format!("cannot clear `{}`: {e}", options.out.display()))?;
    }
    for program in &programs {
        write_bundle_file(&options.out, &program.bundle_path, &program.source)?;
    }

    // -- observe every entry: the reference outcomes ------------------------
    let mut records = String::new();
    let mut record_count = 0usize;
    for program in &programs {
        if program.directives.member {
            continue; // [conf.directive.member]: never conform-run directly
        }
        let on_disk = options.out.join(&program.bundle_path);
        let (mut record, _) = crate::observe_record(&on_disk, &program.source, None);
        // The wire path is the bundle-relative one: records travel between
        // machines, and an exporter-local prefix would make identical
        // observations compare unequal (and break cross-OS determinism).
        record.file.clone_from(&program.bundle_path);
        let value = serde_json::to_value(&record)
            .map_err(|e| format!("{}: could not build the record: {e}", program.bundle_path))?;
        if let Err(errors) = schema::validate(&value) {
            return Err(format!(
                "{}: refusing to export a record our own validator rejects: {errors}",
                program.bundle_path
            ));
        }
        let line = record
            .to_json_line()
            .map_err(|e| format!("{}: {e}", program.bundle_path))?;
        records.push_str(&line);
        records.push('\n');
        record_count += 1;
    }

    // -- coverage: the matrix and the debt list -----------------------------
    let coverage = coverage(&registry, &programs);

    // -- assemble the file set ----------------------------------------------
    let mut files: BTreeMap<String, Vec<u8>> = extra_files;
    for program in &programs {
        files.insert(program.bundle_path.clone(), program.source.clone());
    }
    files.insert("expected/records.jsonl".to_owned(), records.into_bytes());
    files.insert(
        "anchors/anchors.json".to_owned(),
        normalize(&fs::read(options.spec.join("anchors.json")).map_err(|e| e.to_string())?),
    );
    files.insert("vocab/traps.json".to_owned(), traps_json());
    files.insert("vocab/ub-rows.json".to_owned(), ub_rows_json());
    files.insert("coverage/matrix.jsonl".to_owned(), coverage.matrix(&pin));
    files.insert("coverage/coverage.md".to_owned(), coverage.rendered(&pin));
    files.insert(
        "docs/repl.md".to_owned(),
        normalize(&fs::read(&options.repl_doc).map_err(|e| {
            format!(
                "{}: {e} (the [repl.*] notes ride along)",
                options.repl_doc.display()
            )
        })?),
    );
    files.insert("README.md".to_owned(), bundle_readme(&pin));

    // -- the manifest and the root hash -------------------------------------
    let hashes: BTreeMap<String, String> = files
        .iter()
        .map(|(path, bytes)| (path.clone(), sha256::hex(bytes)))
        .collect();
    let bundle_sha256 = root_hash(&hashes);
    let manifest = serde_json::json!({
        "bundle_schema": BUNDLE_SCHEMA,
        "protocol": crate::protocol::PROTOCOL_VERSION,
        "impl": crate::IMPL_NAME,
        "impl_version": crate::IMPL_VERSION,
        "impl_commit": crate::COMMIT,
        "pin": pin,
        "style_version": STYLE_VERSION,
        "counts": {
            "files": files.len(),
            "programs": programs.len(),
            "records": record_count,
        },
        "coverage": {
            "anchors_total": coverage.total,
            "anchors_covered": coverage.covered.len(),
            "forward_tags": coverage.forward.len(),
        },
        "files": hashes,
        "bundle_sha256": bundle_sha256,
    });
    let mut manifest_text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    manifest_text.push('\n');

    // -- write the generated half (programs are already on disk) ------------
    for (path, bytes) in &files {
        write_bundle_file(&options.out, path, bytes)?;
    }
    fs::write(options.out.join("MANIFEST.json"), manifest_text).map_err(|e| e.to_string())?;

    Ok(ExportSummary {
        pin,
        files: files.len() + 1, // + MANIFEST.json
        programs: programs.len(),
        records: record_count,
        anchors_total: coverage.total,
        anchors_covered: coverage.covered.len(),
        forward_tags: coverage.forward.len(),
        bundle_sha256,
    })
}

fn write_bundle_file(out: &Path, bundle_path: &str, bytes: &[u8]) -> Result<(), String> {
    let full = out.join(bundle_path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&full, bytes).map_err(|e| format!("cannot write `{}`: {e}", full.display()))
}

fn parse_program(bundle_path: String, source: Vec<u8>) -> Result<Program, String> {
    let text = std::str::from_utf8(&source).map_err(|_| format!("{bundle_path}: not UTF-8"))?;
    let directives = directive::parse_header(text)
        .map_err(|e| format!("{bundle_path}: unreadable directive header: {e}"))?;
    Ok(Program {
        bundle_path,
        source,
        directives,
    })
}

/// Reads the upstream pin — the corpus/spec commit the whole bundle is *at*.
fn read_pin(path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("cannot read the pin `{}`: {e}", path.display()))?;
    let pin = text.trim().to_owned();
    if pin.len() != 40 || !pin.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "`{}` does not hold a commit sha: `{pin}`",
            path.display()
        ));
    }
    Ok(pin)
}

/// The pinned anchor registry: anchor → owning document.
fn read_registry(spec: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = fs::read_to_string(spec.join("anchors.json"))
        .map_err(|e| format!("cannot read the pinned anchors.json: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("anchors.json is not JSON: {e}"))?;
    let Some(anchors) = value.get("anchors").and_then(|a| a.as_object()) else {
        return Err("anchors.json has no `anchors` object".to_owned());
    };
    Ok(anchors
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
        .collect())
}

/// Registry disagreements that have been triaged and FILED upstream, keyed
/// by namespace. The `FILED_DIVERGENCES` pattern applied to the anchor
/// cross-check: a filed finding still appears in every export (hiding it
/// would defeat the check), it just stops failing the export it already
/// filed. The waiver dies with the filing — when the clause is amended and
/// the namespace becomes legal, the row comes out and the check resumes
/// gating.
pub const FILED_REGISTRY_FINDINGS: &[(&str, &str)] = &[(
    "pkg",
    "wolf-lang#120 — [conf.anchor.ns] never amended for 08-package.md's \
     sixteen anchors; the clause's own additive-append contract (the s39 \
     `test` precedent) is the one-line fix",
)];

/// The independent extraction (`[conf.anchor.grammar]`): every
/// `[registered.namespace.token]` in the pinned spec markdown, compared both
/// ways against `anchors.json`. A mismatch is an **upstream finding** — the
/// export fails loudly and files, never patches (independence doctrine).
/// A mismatch already in [`FILED_REGISTRY_FINDINGS`] returns as a notice
/// instead: reported on every export, no longer fatal, per the standing
/// waiver rule.
///
/// # Errors
///
/// Any UNFILED mismatch, spelled out token by token.
pub fn cross_check_registry(
    spec: &Path,
    registry: &BTreeMap<String, String>,
) -> Result<Vec<String>, String> {
    let mut extracted: BTreeMap<String, ()> = BTreeMap::new();
    for (name, bytes) in walk_files(spec)? {
        if !name.ends_with(".md") {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        for token in bracketed_tokens(&text) {
            // A bare namespace token names the namespace, not a clause —
            // spec/01 §9's heading writes `[diag]` (s67) and the registry
            // keys clauses only, so the cross-check compares dotted anchors.
            // (Whether the registry should ALSO key bare namespaces is
            // routed upstream with the #18 closure notes.)
            if anchor::classify(&token) == Ok(Namespace::Registered) && token.contains('.') {
                extracted.insert(token, ());
            }
        }
    }
    let missing_from_registry: Vec<&String> = extracted
        .keys()
        .filter(|t| !registry.contains_key(*t))
        .collect();
    let filed_ns = |t: &str| {
        let ns = t.split('.').next().unwrap_or_default();
        FILED_REGISTRY_FINDINGS.iter().find(|(n, _)| *n == ns)
    };
    // A token the spec cites but the registry lacks is never waived: the
    // registry is upstream's own generated artifact, and a hole in it is
    // a fresh finding whatever namespace it lands in.
    let (filed, missing_from_spec): (Vec<&String>, Vec<&String>) = registry
        .keys()
        .filter(|t| !extracted.contains_key(*t))
        .partition(|t| filed_ns(t).is_some());
    if missing_from_registry.is_empty() && missing_from_spec.is_empty() {
        let mut notices = Vec::new();
        if !filed.is_empty() {
            let mut by_ns: BTreeMap<&str, (usize, &str)> = BTreeMap::new();
            for t in &filed {
                let (ns, filing) = filed_ns(t).expect("partitioned as filed");
                by_ns.entry(ns).or_insert((0, *filing)).0 += 1;
            }
            for (ns, (count, filing)) in by_ns {
                notices.push(format!(
                    "notice: {count} `{ns}.*` anchor(s) registered but absent from the \
                     spec's namespace clause — known upstream finding, {filing}"
                ));
            }
        }
        return Ok(notices);
    }
    Err(format!(
        "anchors.json and an independent extraction of the spec disagree — an upstream \
         finding, filed rather than worked around.\n\
         cited in the spec, absent from the registry: {missing_from_registry:?}\n\
         registered, never appearing in the spec: {missing_from_spec:?}"
    ))
}

/// Every `[token]` in a markdown document whose body fits the anchor charset.
fn bracketed_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while let Some(open) = text[at..].find('[') {
        let start = at + open + 1;
        let Some(close) = text[start..].find(']') else {
            break;
        };
        let token = &text[start..start + close];
        if !token.is_empty()
            && token.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' || b == b'.'
            })
        {
            out.push(token.to_owned());
            at = start + close + 1;
        } else {
            // Not anchor-shaped: resume just past the `[`, so an anchor
            // opening *inside* this span (`[[mem.x]`, say) is still found.
            at = start;
        }
        if at >= bytes.len() {
            break;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Coverage — the honesty document
// ---------------------------------------------------------------------------

/// The coverage accounting: every registered anchor either cited by at least
/// one bundled program or explicitly on the debt list; forward tags counted
/// beside them, never mixed in.
pub struct CoverageReport {
    /// The denominator: every anchor in the pinned registry.
    pub total: usize,
    /// anchor → owning document, for grouping.
    pub docs: BTreeMap<String, String>,
    /// Registered anchors cited by at least one program: anchor → citing
    /// bundle paths with their `check:` expectation (`-` for members).
    pub covered: BTreeMap<String, Vec<(String, String)>>,
    /// Registered anchors no program cites — the debt list.
    pub debt: Vec<String>,
    /// Reserved-namespace tags (forward, `[conf.anchor.ns]`) → citing paths.
    pub forward: BTreeMap<String, Vec<String>>,
}

fn coverage(registry: &BTreeMap<String, String>, programs: &[Program]) -> CoverageReport {
    let mut covered: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut forward: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for program in programs {
        let check = program
            .directives
            .check
            .as_ref()
            .map_or_else(|| "-".to_owned(), ToString::to_string);
        for tag in &program.directives.conforms {
            match anchor::classify(tag) {
                Ok(Namespace::Registered) => covered
                    .entry(tag.clone())
                    .or_default()
                    .push((program.bundle_path.clone(), check.clone())),
                Ok(Namespace::Reserved) => forward
                    .entry(tag.clone())
                    .or_default()
                    .push(program.bundle_path.clone()),
                Err(_) => {} // already rejected by the harness; unreachable here
            }
        }
    }
    covered.retain(|tag, _| registry.contains_key(tag));
    let debt: Vec<String> = registry
        .keys()
        .filter(|tag| !covered.contains_key(*tag))
        .cloned()
        .collect();
    CoverageReport {
        total: registry.len(),
        docs: registry.clone(),
        covered,
        debt,
        forward,
    }
}

impl CoverageReport {
    /// `[conf.cover.format]`'s JSONL — one record per registered anchor, the
    /// D5 shape (`clause`, `tests`, `status`, `commit`) plus the citing tests
    /// on an `x-` extension key, protocol-style.
    #[must_use]
    pub fn matrix(&self, pin: &str) -> Vec<u8> {
        let mut out = String::new();
        for (tag, doc) in &self.docs {
            let tests = self.covered.get(tag);
            let line = serde_json::json!({
                "clause": tag,
                "tests": tests.map_or(0, Vec::len),
                "status": if tests.is_some() { "covered" } else { "debt" },
                "commit": pin,
                "x-doc": doc,
                "x-cited-by": tests.map(|entries| {
                    entries
                        .iter()
                        .map(|(file, check)| serde_json::json!({"file": file, "check": check}))
                        .collect::<Vec<_>>()
                }).unwrap_or_default(),
            });
            let _ = writeln!(out, "{line}");
        }
        out.into_bytes()
    }

    /// The rendered table: per-document coverage (ranked by chapter weight —
    /// anchor count), the debt list in full, the forward tags, and the UB
    /// enumeration's dedicated section (D2: an untested UB item is an
    /// unlicensed optimization).
    #[must_use]
    pub fn rendered(&self, pin: &str) -> Vec<u8> {
        let mut by_doc: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for (tag, doc) in &self.docs {
            let entry = by_doc.entry(doc.as_str()).or_default();
            entry.1 += 1;
            if self.covered.contains_key(tag) {
                entry.0 += 1;
            }
        }
        let mut ranked: Vec<(&str, usize, usize)> = by_doc
            .into_iter()
            .map(|(doc, (covered, total))| (doc, covered, total))
            .collect();
        ranked.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(b.0)));

        let mut out = String::new();
        let _ = writeln!(out, "# Conformance coverage — the honesty document\n");
        let _ = writeln!(
            out,
            "Pin `{pin}`. **{} / {} registered anchors** are cited by at least one \
             bundled program; the {} uncovered anchors are listed in full below — \
             the gap list is part of the product, not a private shame file. \
             {} forward (reserved-namespace) tags ride beside the registry.\n",
            self.covered.len(),
            self.total,
            self.debt.len(),
            self.forward.len(),
        );
        let _ = writeln!(out, "## Per-document coverage (ranked by chapter weight)\n");
        let _ = writeln!(out, "| document | covered | total | % |");
        let _ = writeln!(out, "|---|---|---|---|");
        for (doc, covered, total) in &ranked {
            let _ = writeln!(
                out,
                "| {doc} | {covered} | {total} | {}% |",
                covered * 100 / total
            );
        }

        let _ = writeln!(
            out,
            "\n## The UB enumeration (D2 — 100% or a named reason)\n"
        );
        let _ = writeln!(
            out,
            "| row | status | licensed optimization |\n|---|---|---|"
        );
        for row in UbRow::ALL {
            let status = match row.coverage() {
                Coverage::Detected => {
                    "detected + paired (trigger and near-miss twin ship in `suite/ub/`)".to_owned()
                }
                Coverage::DeferredConcurrency => {
                    "deferred(concurrency): spec/03's model — tracked, s36 cross-validation"
                        .to_owned()
                }
                Coverage::Unreachable(reason) => format!("unreachable at this tier: {reason}"),
            };
            let _ = writeln!(out, "| {} | {status} | {} |", row.id(), row.optimization());
        }

        let _ = writeln!(out, "\n## Debt list — registered anchors with zero tests\n");
        let mut current_doc = "";
        for tag in &self.debt {
            let doc = self.docs.get(tag).map_or("", String::as_str);
            if doc != current_doc {
                let _ = writeln!(out, "\n### {doc}\n");
                current_doc = doc;
            }
            let _ = writeln!(out, "- `{tag}`");
        }

        let _ = writeln!(out, "\n## Forward tags (reserved namespaces)\n");
        for (tag, files) in &self.forward {
            let _ = writeln!(out, "- `{tag}` — {}", files.join(", "));
        }
        out.into_bytes()
    }
}

// ---------------------------------------------------------------------------
// Vocabularies
// ---------------------------------------------------------------------------

/// `[conf.trap.set]` as shipped data: the closed twelve, in spec order.
fn traps_json() -> Vec<u8> {
    let value = serde_json::json!({
        "source": "spec/05-conformance.md [conf.trap.set]",
        "closed": true,
        "kinds": TrapKind::ALL.iter().map(|k| k.as_str()).collect::<Vec<_>>(),
    });
    let mut text = serde_json::to_string_pretty(&value).expect("static data serializes");
    text.push('\n');
    text.into_bytes()
}

/// `[mem.ub]`'s closed enumeration with the D2 pairing — every row carries
/// the optimization it licenses, and its coverage status here.
fn ub_rows_json() -> Vec<u8> {
    let rows: Vec<serde_json::Value> = UbRow::ALL
        .into_iter()
        .map(|row| {
            let coverage = match row.coverage() {
                Coverage::Detected => "detected".to_owned(),
                Coverage::DeferredConcurrency => "deferred(concurrency)".to_owned(),
                Coverage::Unreachable(reason) => format!("unreachable: {reason}"),
            };
            serde_json::json!({
                "id": row.id(),
                "clause": row.clause(),
                "what": row.what(),
                "licenses": row.optimization(),
                "coverage": coverage,
            })
        })
        .collect();
    let value = serde_json::json!({
        "source": "spec/02-memory-model.md [mem.ub] (closed; [mem.ub.closed])",
        "rows": rows,
    });
    let mut text = serde_json::to_string_pretty(&value).expect("static data serializes");
    text.push('\n');
    text.into_bytes()
}

fn bundle_readme(pin: &str) -> Vec<u8> {
    format!(
        "# wolf conformance bundle (schema 1)\n\
         \n\
         The wolf conformance suite, exported by lupin (the wolf-interp\n\
         reference interpreter) at upstream pin `{pin}`. Self-contained: programs under\n\
         `corpus/` and `suite/`, this implementation's protocol-1 reference\n\
         records under `expected/records.jsonl`, the closed trap and UB\n\
         vocabularies under `vocab/`, the pinned anchor registry under\n\
         `anchors/`, and the coverage matrix + debt list under `coverage/`.\n\
         `MANIFEST.json` carries the pin, the schema version, and a sha256\n\
         per file; `bundle_sha256` digests the sorted hash list.\n\
         \n\
         Check any conform-run-speaking implementation against it:\n\
         \n\
             lupin conformance check <this-dir> --impl <command>\n\
         \n\
         The format is documented in the wolf-interp repository,\n\
         `docs/conformance-bundle.md`.\n"
    )
    .into_bytes()
}

// ---------------------------------------------------------------------------
// File plumbing
// ---------------------------------------------------------------------------

/// Every file under `root`, as (relative slash path, bytes), sorted by the
/// relative path — the same key every consumer sorts by.
fn walk_files(root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for entry in entries {
            if entry.is_dir() {
                collect(&entry, out)?;
            } else {
                out.push(entry);
            }
        }
        Ok(())
    }
    if !root.is_dir() {
        return Err(format!("`{}` is not a directory", root.display()));
    }
    let mut paths = Vec::new();
    collect(root, &mut paths).map_err(|e| format!("walking `{}`: {e}", root.display()))?;
    let mut named: Vec<(String, PathBuf)> = paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            (relative, path)
        })
        .collect();
    named.sort();
    named
        .into_iter()
        .map(|(relative, path)| {
            fs::read(&path)
                .map(|bytes| (relative, bytes))
                .map_err(|e| format!("cannot read `{}`: {e}", path.display()))
        })
        .collect()
}

/// CRLF → LF. Everything this bundle ships is text, and a Windows checkout
/// (`core.autocrlf`) must not change spans, hashes, or the bundle identity —
/// the compiler-track normalization lesson, applied **before** observation
/// so the recorded byte offsets index the bytes that ship.
#[must_use]
pub fn normalize(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1; // skip the CR, keep the LF
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// The one number CI compares across platforms: a digest of the sorted
/// `hash  path` list, `sha256sum`-shaped so a human can recompute it.
fn root_hash(hashes: &BTreeMap<String, String>) -> String {
    let mut listing = String::new();
    for (path, hash) in hashes {
        let _ = writeln!(listing, "{hash}  {path}");
    }
    sha256::hex(listing.as_bytes())
}

// ---------------------------------------------------------------------------
// Check — the pluggable-counterparty differ
// ---------------------------------------------------------------------------

/// The counterparty a bundle is checked against.
#[derive(Debug, Clone)]
pub enum CheckImpl {
    /// A conform-run-speaking command, invoked per `[proto.invoke]` —
    /// `<cmd> conform-run <file> --json`. The pinned compiler is the proven
    /// case; any implementation of the protocol fits.
    Command(PathBuf),
    /// Recorded observations replayed from a JSONL file, keyed by `file` —
    /// the consumption dry-run: proves the pull → run → diff path without a
    /// second implementation in the room.
    Replay(PathBuf),
}

/// What one bundle check produced.
#[derive(Debug, Clone, Default)]
pub struct CheckOutcome {
    pub compared: usize,
    /// Ordered by `[proto.cmp.severity]`, then file.
    pub divergences: Vec<DeepDivergence>,
    pub ledger: Vec<LedgerEntry>,
}

/// The parsed, integrity-verified manifest of a bundle on disk.
#[derive(Debug, Clone)]
pub struct VerifiedBundle {
    pub root: PathBuf,
    pub pin: String,
    pub bundle_sha256: String,
    pub expected: Vec<ObservationRecord>,
}

/// Opens a bundle: schema version, per-file hashes, the root hash, and the
/// expected records all verified before anything is compared. A tampered or
/// truncated bundle is refused loudly — a conformance gate that runs against
/// corrupted expectations would launder noise as verdicts.
///
/// # Errors
///
/// The first integrity violation, named.
pub fn open_bundle(root: &Path) -> Result<VerifiedBundle, String> {
    let manifest_text = fs::read_to_string(root.join("MANIFEST.json"))
        .map_err(|e| format!("cannot read `{}/MANIFEST.json`: {e}", root.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text)
        .map_err(|e| format!("MANIFEST.json is not JSON: {e}"))?;

    let schema_version = manifest
        .get("bundle_schema")
        .and_then(serde_json::Value::as_u64);
    if schema_version != Some(BUNDLE_SCHEMA) {
        return Err(format!(
            "bundle schema {schema_version:?} is not the version this tool speaks ({BUNDLE_SCHEMA})"
        ));
    }
    let pin = manifest
        .get("pin")
        .and_then(serde_json::Value::as_str)
        .ok_or("MANIFEST.json has no `pin`")?
        .to_owned();
    let Some(files) = manifest.get("files").and_then(|f| f.as_object()) else {
        return Err("MANIFEST.json has no `files` map".to_owned());
    };

    let mut hashes: BTreeMap<String, String> = BTreeMap::new();
    for (path, want) in files {
        let want = want.as_str().unwrap_or_default();
        let bytes = fs::read(root.join(path))
            .map_err(|e| format!("bundle integrity: cannot read `{path}`: {e}"))?;
        let got = sha256::hex(&bytes);
        if got != want {
            return Err(format!(
                "bundle integrity: `{path}` hashes {got}, manifest says {want} — refusing to \
                 compare against a tampered bundle"
            ));
        }
        hashes.insert(path.clone(), got);
    }
    let declared_root = manifest
        .get("bundle_sha256")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let computed_root = root_hash(&hashes);
    if computed_root != declared_root {
        return Err(format!(
            "bundle integrity: bundle_sha256 recomputes to {computed_root}, manifest says \
             {declared_root}"
        ));
    }

    let records_text = fs::read_to_string(root.join("expected/records.jsonl"))
        .map_err(|e| format!("cannot read expected/records.jsonl: {e}"))?;
    let mut expected = Vec::new();
    for (index, line) in records_text.lines().enumerate() {
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| format!("expected record {index}: not JSON: {e}"))?;
        if let Err(errors) = schema::validate(&value) {
            return Err(format!("expected record {index}: schema-invalid: {errors}"));
        }
        let record: ObservationRecord =
            serde_json::from_value(value).map_err(|e| format!("expected record {index}: {e}"))?;
        expected.push(record);
    }

    Ok(VerifiedBundle {
        root: root.to_owned(),
        pin,
        bundle_sha256: computed_root,
        expected,
    })
}

/// Checks an implementation against a verified bundle: every expected record
/// against the counterparty's observation of the same program, compared per
/// `[proto.cmp]` exactly as is05's differ does — the counterparty is just
/// pluggable now.
///
/// # Errors
///
/// A replay file that cannot be read, or a program the bundle's own manifest
/// names that is missing on disk.
pub fn check(
    bundle: &VerifiedBundle,
    counterparty: &CheckImpl,
    timeout: Duration,
) -> Result<CheckOutcome, String> {
    let replayed: Option<BTreeMap<String, ObservationRecord>> = match counterparty {
        CheckImpl::Replay(path) => {
            let text = fs::read_to_string(path)
                .map_err(|e| format!("cannot read replay records `{}`: {e}", path.display()))?;
            let mut map = BTreeMap::new();
            for (index, line) in text.lines().enumerate() {
                let record: ObservationRecord = serde_json::from_str(line)
                    .map_err(|e| format!("replay record {index}: {e}"))?;
                map.insert(record.file.clone(), record);
            }
            Some(map)
        }
        CheckImpl::Command(_) => None,
    };

    let mut out = CheckOutcome::default();
    for expected in &bundle.expected {
        let program = bundle.root.join(&expected.file);
        let source = fs::read(&program)
            .map_err(|e| format!("bundle program `{}` unreadable: {e}", expected.file))?;
        let theirs = match counterparty {
            CheckImpl::Command(cmd) => differ::invoke(cmd, &program, timeout),
            CheckImpl::Replay(_) => {
                let map = replayed.as_ref().expect("built above");
                match map.get(&expected.file) {
                    Some(record) => Invocation::Record(Box::new(record.clone())),
                    None => Invocation::ToolError(format!(
                        "the replay file has no record for `{}`",
                        expected.file
                    )),
                }
            }
        };
        let ours = Invocation::Record(Box::new(expected.clone()));
        let FileComparison { divergence, ledger } =
            differ::compare_invocations(&expected.file, &ours, &theirs, source_has_unsafe(&source));
        out.compared += 1;
        if let Some(divergence) = divergence {
            out.divergences.push(divergence);
        }
        out.ledger.extend(ledger);
    }
    out.divergences
        .sort_by(|x, y| (x.class, &x.file).cmp(&(y.class, &y.file)));
    Ok(out)
}

/// The safe-tier filter (is04's "zero UB in safe code", cross-implementation):
/// a program with no `unsafe`, no `asm`, and no C import is safe-tier by
/// construction.
#[must_use]
pub fn source_has_unsafe(source: &[u8]) -> bool {
    let text = String::from_utf8_lossy(source);
    text.contains("unsafe") || text.contains("import c ") || text.contains("asm ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_touches_only_crlf() {
        assert_eq!(normalize(b"a\r\nb\n"), b"a\nb\n");
        assert_eq!(
            normalize(b"a\rb"),
            b"a\rb",
            "a lone CR is content, not a line ending"
        );
        assert_eq!(normalize(b""), b"");
        assert_eq!(normalize(b"\r\n\r\n"), b"\n\n");
    }

    #[test]
    fn bracketed_tokens_extract_only_anchor_shaped_bodies() {
        let text = "see [mem.tier0.move.1] and [conf.cover]; a link [text](x.md), \
                    code `arr[i]`, and [Not.An.Anchor].";
        // `text` and `i` are charset-valid and survive extraction; the
        // *namespace* filter in `cross_check_registry` is what drops them.
        assert_eq!(
            bracketed_tokens(text),
            vec![
                "mem.tier0.move.1".to_owned(),
                "conf.cover".to_owned(),
                "text".to_owned(),
                "i".to_owned()
            ]
        );
    }

    #[test]
    fn the_root_hash_is_the_digest_of_the_sorted_listing() {
        let mut hashes = BTreeMap::new();
        hashes.insert("b.lu".to_owned(), "22".to_owned());
        hashes.insert("a.lu".to_owned(), "11".to_owned());
        assert_eq!(
            root_hash(&hashes),
            sha256::hex(b"11  a.lu\n22  b.lu\n"),
            "sha256sum-shaped, sorted by path"
        );
    }

    #[test]
    fn a_planted_registry_mismatch_fails_the_cross_check() {
        // The registry claims an anchor the spec never states: the export
        // must refuse loudly (an upstream finding, never absorbed).
        let dir = std::env::temp_dir().join("wolf-interp-crosscheck-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        fs::write(dir.join("01-x.md"), "a clause [mem.real] here\n").expect("write");
        let mut registry = BTreeMap::new();
        registry.insert("mem.real".to_owned(), "01-x.md".to_owned());
        assert_eq!(cross_check_registry(&dir, &registry), Ok(Vec::new()));

        registry.insert("mem.phantom".to_owned(), "01-x.md".to_owned());
        let err = cross_check_registry(&dir, &registry).expect_err("must fail");
        assert!(err.contains("mem.phantom"), "{err}");

        // A FILED namespace waives fatally-missing-from-spec into a notice
        // that still names the filing — and only that namespace: `mem` is
        // not filed, so `mem.phantom` above stayed fatal. `pkg` is.
        let mut registry = BTreeMap::new();
        registry.insert("mem.real".to_owned(), "01-x.md".to_owned());
        registry.insert("pkg.manifest".to_owned(), "08-p.md".to_owned());
        let notices = cross_check_registry(&dir, &registry).expect("filed ns is a notice");
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("wolf-lang#120"), "{}", notices[0]);
        assert!(notices[0].contains("1 `pkg.*`"), "{}", notices[0]);

        // The other direction cannot arise for a filed-but-unregistered
        // namespace: the extraction is namespace-gated (`classify` drops
        // `pkg.*` citations until the clause legalizes them), so a spec
        // citation the registry lacks stays a REGISTERED-namespace concern
        // — proven fatal by the `mem.uncatalogued` case below, which the
        // waiver must never touch.

        // And the converse: the spec cites what the registry does not know.
        fs::write(dir.join("02-y.md"), "cites [mem.uncatalogued]\n").expect("write");
        let mut registry = BTreeMap::new();
        registry.insert("mem.real".to_owned(), "01-x.md".to_owned());
        let err = cross_check_registry(&dir, &registry).expect_err("must fail");
        assert!(err.contains("mem.uncatalogued"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }
}
