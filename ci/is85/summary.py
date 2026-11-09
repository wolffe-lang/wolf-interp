#!/usr/bin/env python3
"""ci/is85/summary.py — the note's tables, from the TSVs beside it (is85).

Reads nightly-jobs.tsv, nightly-targets.tsv, nightly-tests.tsv,
windows-ladder-steps.tsv and corpus-by-head.tsv and prints, per corpus
era (the vendored pin: the eras are the heads whose corpus is the same),
the min / median / max over the nightlies of that era:

  1. each OS's test jobs, in minutes;
  2. each heavy test binary, in seconds;
  3. run_corpus's tests, in seconds (a test's own start to its end);
  4. the windows ladder's steps, in minutes.

A cancelled job's or binary's ">N" is shown as such and kept out of the
median. Nothing is read from GitHub; `measure.py fetch` and `parse` make
the TSVs.
"""
import csv
import os
import statistics
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))


def rows(name):
    with open(os.path.join(HERE, name)) as fh:
        return list(csv.DictReader(fh, delimiter="\t"))


heads = {r["head"]: r for r in rows("corpus-by-head.tsv")}


def era(head):
    h = heads[head]
    return "%s (%s files)" % (h["pin"], h["lu_files"])


ERAS = []
for r in rows("nightly-jobs.tsv"):
    if era(r["head"]) not in ERAS:
        ERAS.append(era(r["head"]))


def stat(values):
    done = [float(v) for v in values if not str(v).startswith(">")]
    cut = [v for v in values if str(v).startswith(">")]
    if not done:
        return "%s" % " ".join(cut)
    s = "%.0f / %.0f / %.0f" % (min(done), statistics.median(done), max(done))
    return s + ("  (+ %s)" % " ".join(cut) if cut else "")


def nights(rs):
    return len({r["run"] for r in rs})


print("## 1. test jobs, minutes (min / median / max; '!' a job that did not succeed)\n")
by = defaultdict(list)
for r in rows("nightly-jobs.tsv"):
    v = r["minutes"] if r["conclusion"] == "success" else ">" + r["minutes"]
    by[(era(r["head"]), r["os"], r["entry"])].append(v)
for e in ERAS:
    print("era %s" % e)
    for o in ("ubuntu", "macos", "windows"):
        for entry in ("1", "2", "3", "ladder", "smoke"):
            if (e, o, entry) in by:
                print("  %-8s %-7s %s" % (o, entry, stat(by[(e, o, entry)])))
    print()

print("## 2. heavy test binaries, seconds (cargo's own 'finished in')\n")
by = defaultdict(list)
for r in rows("nightly-targets.tsv"):
    by[(era(r["head"]), r["os"], r["entry"], r["target"])].append(r["seconds"])
HEAVY = ("run_corpus", "divergence", "doc_truth", "region_machine", "rule_registry", "export", "cli", "fuzz_smoke")
for e in ERAS:
    print("era %s" % e)
    for o in ("ubuntu", "macos", "windows"):
        for t in HEAVY:
            for entry in ("1", "2", "3"):
                if (e, o, entry, t) in by:
                    print("  %-8s shard %s %-15s %s" % (o, entry, t, stat(by[(e, o, entry, t)])))
    print()

print("## 3. run_corpus's tests, seconds from the test's start to its end\n")
by = defaultdict(list)
for r in rows("nightly-tests.tsv"):
    if r["target"] != "run_corpus" or r["entry"] not in ("1", "2", "3"):
        continue
    end = r["end"]
    dur = end if end.startswith(">") else str(float(end) - float(r["start"]))
    if end.startswith(">"):
        dur = ">%.0f" % (float(end[1:]) - float(r["start"]))
    by[(era(r["head"]), r["os"], r["test"])].append(dur)
for e in ERAS:
    print("era %s" % e)
    for o in ("ubuntu", "macos", "windows"):
        for (ee, oo, t), v in sorted(by.items()):
            if ee == e and oo == o:
                print("  %-8s %-72s %s" % (o, t, stat(v)))
    print()

print("## 4. the windows ladder's steps, minutes\n")
by = defaultdict(list)
for r in rows("windows-ladder-steps.tsv"):
    by[(era(r["head"]), r["step"])].append(r["minutes"])
for e in ERAS:
    print("era %s" % e)
    for (ee, s), v in sorted(by.items()):
        if ee == e:
            print("  %-48s %s" % (s, stat(v)))
    print()
