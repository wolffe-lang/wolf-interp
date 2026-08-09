//! The corpus-consumption harness.
//!
//! Walks the pinned `upstream/corpus/**/*.lu`, parses every directive header
//! with our own parser, and reports what each file claims. Today it proves
//! only that the corpus is *legible* to this implementation — no wolf code is
//! executed. That is the whole job: a directive-grammar divergence between the
//! two tracks becomes visible here, at the cost of one walk.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::anchor::{self, Namespace};
use crate::directive::Directives;

/// What the harness made of one corpus file.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// An entry: a file conform-run directly.
    Entry(Box<Directives>),
    /// A `member: true` file, exercised through its module's entry file.
    Member(Box<Directives>),
    /// An unreadable file or an unparseable header — always a failure.
    Failed(String),
}

/// One line of the report.
#[derive(Debug, Clone)]
pub struct FileReport {
    /// Path relative to the corpus root, `/`-separated on every platform.
    pub path: String,
    pub outcome: Outcome,
}

impl FileReport {
    /// The directives, when the header parsed.
    #[must_use]
    pub fn directives(&self) -> Option<&Directives> {
        match &self.outcome {
            Outcome::Entry(d) | Outcome::Member(d) => Some(d),
            Outcome::Failed(_) => None,
        }
    }
}

/// The whole walk.
#[derive(Debug, Clone)]
pub struct CorpusReport {
    pub root: PathBuf,
    pub files: Vec<FileReport>,
    /// Anchors cited by the corpus in a *registered* namespace that are absent
    /// from `spec/anchors.json` (`[conf.tag.valid]`). Empty unless a pin bump
    /// moved the spec out from under the corpus.
    pub unknown_anchors: Vec<String>,
    /// Whether `spec/anchors.json` was available to check against.
    pub anchors_checked: bool,
}

impl CorpusReport {
    #[must_use]
    pub fn total(&self) -> usize {
        self.files.len()
    }

    #[must_use]
    pub fn entries(&self) -> usize {
        self.files
            .iter()
            .filter(|f| matches!(f.outcome, Outcome::Entry(_)))
            .count()
    }

    #[must_use]
    pub fn members(&self) -> usize {
        self.files
            .iter()
            .filter(|f| matches!(f.outcome, Outcome::Member(_)))
            .count()
    }

    /// Files whose header did not parse, with the reason.
    #[must_use]
    pub fn failures(&self) -> Vec<(&str, &str)> {
        self.files
            .iter()
            .filter_map(|f| match &f.outcome {
                Outcome::Failed(reason) => Some((f.path.as_str(), reason.as_str())),
                _ => None,
            })
            .collect()
    }

    /// True when every header parsed and every registered anchor resolved.
    #[must_use]
    pub fn is_green(&self) -> bool {
        self.failures().is_empty() && self.unknown_anchors.is_empty()
    }

    /// How many corpus files cite each anchor, sorted by anchor.
    #[must_use]
    pub fn tag_counts(&self) -> BTreeMap<&str, usize> {
        let mut counts = BTreeMap::new();
        for file in &self.files {
            if let Some(directives) = file.directives() {
                for tag in &directives.conforms {
                    *counts.entry(tag.as_str()).or_insert(0) += 1;
                }
            }
        }
        counts
    }
}

/// Walks a corpus root and parses every `.lu` header.
///
/// `spec_root` is optional: when it holds an `anchors.json`, tags in
/// registered namespaces are checked against it (`[conf.tag.valid]`).
///
/// # Errors
///
/// Returns an [`io::Error`] if the root is missing or unreadable. A file whose
/// *header* is bad is not an error here — it is a recorded failure, so one bad
/// file does not hide the other sixty-eight.
pub fn walk(root: &Path, spec_root: Option<&Path>) -> io::Result<CorpusReport> {
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("corpus root `{}` is not a directory", root.display()),
        ));
    }

    let mut paths = Vec::new();
    collect_lu_files(root, &mut paths)?;
    // `read_dir` order is platform noise (compiler-track lesson): sort before
    // anything downstream can depend on the order.
    //
    // The key is the **relative slash path**, not the `PathBuf`. `Path`'s order
    // is component-wise, so it puts `comptime/assert_static.lu` before
    // `comptime.lu` (the directory component `comptime` sorts under the file
    // `comptime.lu`), while every consumer of this report sorts the strings the
    // records carry, where `.` precedes `/`. Sorting the thing that leaves the
    // tool is what makes "the walk is sorted" a property a differ can rely on.
    let mut paths: Vec<(String, PathBuf)> = paths
        .into_iter()
        .map(|path| (relative_slash_path(root, &path), path))
        .collect();
    paths.sort();

    let mut files = Vec::with_capacity(paths.len());
    for (relative, path) in paths {
        let outcome = match fs::read_to_string(&path) {
            Ok(source) => match crate::directive::parse_header(&source) {
                Ok(directives) if directives.member => Outcome::Member(Box::new(directives)),
                Ok(directives) => Outcome::Entry(Box::new(directives)),
                Err(e) => Outcome::Failed(e.to_string()),
            },
            Err(e) => Outcome::Failed(format!("unreadable: {e}")),
        };
        files.push(FileReport {
            path: relative,
            outcome,
        });
    }

    let mut report = CorpusReport {
        root: root.to_path_buf(),
        files,
        unknown_anchors: Vec::new(),
        anchors_checked: false,
    };

    if let Some(spec_root) = spec_root {
        check_registered_anchors(&mut report, spec_root);
    }
    Ok(report)
}

fn collect_lu_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|e| e.path())
        .collect();
    entries.sort();

    for entry in entries {
        if entry.is_dir() {
            collect_lu_files(&entry, out)?;
        } else if entry.extension().is_some_and(|ext| ext == "lu") {
            out.push(entry);
        }
    }
    Ok(())
}

/// A path relative to `root` with `/` separators — never a raw platform path.
/// (Compiler-track lesson: raw separators leak into anything derived from a
/// path, seeds and report keys included.)
fn relative_slash_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn check_registered_anchors(report: &mut CorpusReport, spec_root: &Path) {
    let Ok(text) = fs::read_to_string(spec_root.join("anchors.json")) else {
        return;
    };
    let Ok(registry) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(known) = registry.get("anchors").and_then(|a| a.as_object()) else {
        return;
    };
    report.anchors_checked = true;

    let mut unknown = Vec::new();
    for tag in report.tag_counts().keys() {
        if anchor::classify(tag) == Ok(Namespace::Registered) && !known.contains_key(*tag) {
            unknown.push((*tag).to_owned());
        }
    }
    unknown.sort();
    report.unknown_anchors = unknown;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_paths_are_platform_independent() {
        let root = Path::new(crate::upstream_root()).join("corpus");
        let file = root.join("resolve").join("cycle").join("main.lu");
        assert_eq!(relative_slash_path(&root, &file), "resolve/cycle/main.lu");
    }

    #[test]
    fn a_missing_root_is_an_io_error() {
        let err = walk(Path::new("definitely/not/here"), None).expect_err("must fail");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
