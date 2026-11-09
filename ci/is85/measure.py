#!/usr/bin/env python3
"""ci/is85/measure.py [fetch|parse] CACHE — where the nightly's test time goes (is85).

  fetch CACHE   every scheduled run of CI (the nightly, tier release) into
                CACHE: the run list, each run's jobs (pages of 30: the
                one-request listing of a big run's jobs fails), each test
                job's log, and each windows ladder job's step times. Needs
                `gh`, read-only. Files already in CACHE are kept.
  parse CACHE   the tables below, from CACHE alone, into ci/is85/:

    nightly-jobs.tsv     run date head os entry minutes conclusion: every
                         `test (…)` job of every nightly
    nightly-targets.tsv  run date head os entry target seconds: cargo's own
                         "finished in" for every test binary a shard ran
                         for 10 s or more (">N" for the binary a cancelled
                         job was still in, N its seconds so far), and one
                         `(others)` row per job: the sum over the rest
    nightly-tests.tsv    run date head os entry target test start end: every
                         test that ran 60 s or more, in seconds from its
                         binary's `Running` line. Its start is its "has been
                         running for over 60 seconds" line less 60; its end
                         is its `... ok` line (">N" when the job was
                         cancelled before it).
    windows-ladder-steps.tsv  run date head step minutes: each windows
                         ladder step of a minute or more

Every number is read from GitHub's own timestamps (the job's, the step's,
or the log line's) or from cargo's own summary line; nothing is estimated.
"""
import datetime
import glob
import json
import os
import re
import subprocess
import sys
import time

REPO = "wolffe-lang/wolf-interp"
HERE = os.path.dirname(os.path.abspath(__file__))


def gh_api(path, raw=False):
    args = ["gh", "api"] + (["--allow-escape-sequences"] if raw else []) + [path]
    for attempt in range(6):
        p = subprocess.run(args, capture_output=True)
        if p.returncode == 0:
            return p.stdout
        time.sleep(5 * (attempt + 1))
    sys.exit("gh api %s: %s" % (path, p.stderr.decode(errors="replace")[:300]))


def fetch(cache):
    os.makedirs(os.path.join(cache, "jobs"), exist_ok=True)
    os.makedirs(os.path.join(cache, "logs"), exist_ok=True)
    os.makedirs(os.path.join(cache, "steps"), exist_ok=True)
    runs, page = [], 1
    while True:
        d = json.loads(gh_api("repos/%s/actions/runs?event=schedule&per_page=30&page=%d" % (REPO, page)))
        runs += d["workflow_runs"]
        if len(runs) >= d["total_count"] or not d["workflow_runs"]:
            break
        page += 1
    with open(os.path.join(cache, "runs.tsv"), "w") as fh:
        for r in runs:
            fh.write("%s\t%s\t%s\t%s\t%s\n" % (r["id"], r["head_sha"][:8], r["created_at"], r["status"], r["conclusion"]))
    for r in runs:
        if r["status"] != "completed":
            continue
        out = os.path.join(cache, "jobs", "%s.json" % r["id"])
        if not os.path.exists(out):
            jobs, page = [], 1
            while True:
                d = json.loads(gh_api("repos/%s/actions/runs/%s/jobs?per_page=30&page=%d&filter=latest" % (REPO, r["id"], page)))
                jobs += d["jobs"]
                if len(jobs) >= d["total_count"] or not d["jobs"]:
                    break
                page += 1
            with open(out, "w") as fh:
                json.dump(jobs, fh)
        for j in json.load(open(out)):
            if not j["name"].startswith("test (") or j["status"] != "completed":
                continue
            log = os.path.join(cache, "logs", "%s-%s.log" % (r["id"], j["id"]))
            if not os.path.exists(log):
                with open(log, "wb") as fh:
                    fh.write(gh_api("repos/%s/actions/jobs/%s/logs" % (REPO, j["id"]), raw=True))
                print("fetched", log, flush=True)


ESC = re.compile(r"\x1b\[[0-9;]*m")
STAMP = re.compile(r"^\ufeff?(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d+)Z (.*)$")


def t(s):
    return datetime.datetime.fromisoformat(s.replace("Z", "+00:00")[:26])


def entry_of(name):
    m = re.match(r"test \((\w+)-latest, (.*)\)$", name)
    rest = {"the ladder and the bundle": "ladder", "smoke": "smoke"}.get(m.group(2))
    return m.group(1), rest or m.group(2).replace("test shard ", "").replace(" of 3", "")


def parse(cache):
    runs = {}
    for line in open(os.path.join(cache, "runs.tsv")):
        rid, head, created, _, _ = line.rstrip("\n").split("\t")
        runs[rid] = (created[:10], head)
    jobs_tsv = open(os.path.join(HERE, "nightly-jobs.tsv"), "w")
    targets_tsv = open(os.path.join(HERE, "nightly-targets.tsv"), "w")
    tests_tsv = open(os.path.join(HERE, "nightly-tests.tsv"), "w")
    steps_tsv = open(os.path.join(HERE, "windows-ladder-steps.tsv"), "w")
    jobs_tsv.write("run\tdate\thead\tos\tentry\tminutes\tconclusion\n")
    targets_tsv.write("run\tdate\thead\tos\tentry\ttarget\tseconds\n")
    tests_tsv.write("run\tdate\thead\tos\tentry\ttarget\ttest\tstart\tend\n")
    steps_tsv.write("run\tdate\thead\tstep\tminutes\n")
    rows = []
    for f in glob.glob(os.path.join(cache, "jobs", "*.json")):
        rid = os.path.basename(f)[:-5]
        if rid not in runs:
            continue
        for j in json.load(open(f)):
            if not j["name"].startswith("test (") or j["status"] != "completed":
                continue
            rows.append((runs[rid][0], rid, j))
    rows.sort(key=lambda r: (r[0], r[2]["name"]))
    for date, rid, j in rows:
        head = runs[rid][1]
        osn, entry = entry_of(j["name"])
        minutes = (t(j["completed_at"]) - t(j["started_at"])).total_seconds() / 60
        jobs_tsv.write("%s\t%s\t%s\t%s\t%s\t%.1f\t%s\n" % (rid, date, head, osn, entry, minutes, j["conclusion"]))
        if osn == "windows" and entry == "ladder":
            for s in j.get("steps") or []:
                if s.get("started_at") and s.get("completed_at"):
                    m = (t(s["completed_at"]) - t(s["started_at"])).total_seconds() / 60
                    if m >= 1:
                        steps_tsv.write("%s\t%s\t%s\t%s\t%.1f\n" % (rid, date, head, s["name"], m))
        log = os.path.join(cache, "logs", "%s-%s.log" % (rid, j["id"]))
        if not os.path.exists(log):
            continue
        target = start = last = None
        started, others = {}, 0.0
        for raw in open(log, encoding="utf-8", errors="replace"):
            m = STAMP.match(ESC.sub("", raw.rstrip("\r\n")))
            if not m:
                continue
            last, line = t(m.group(1)), m.group(2)
            mm = re.match(r"\s*Running (?:unittests src[\\/](lib|main)\.rs|tests[\\/](\w+)\.rs) ", line)
            if mm or re.match(r"\s*Doc-tests \w+\s*$", line):
                target = ({"lib": "lib", "main": "bins"}.get(mm.group(1)) or mm.group(2)) if mm else "doc"
                start, started = last, {}
                continue
            if target is None:
                continue
            mm = re.match(r"test (\S+) has been running for over 60 seconds", line)
            if mm:
                started[mm.group(1)] = (last - start).total_seconds() - 60
                continue
            mm = re.match(r"test (\S+) \.\.\. (ok|FAILED|ignored)", line)
            if mm:
                end = (last - start).total_seconds()
                st = started.pop(mm.group(1), -1)
                if st >= 0:
                    tests_tsv.write("%s\t%s\t%s\t%s\t%s\t%s\t%s\t%.0f\t%.0f\n" % (rid, date, head, osn, entry, target, mm.group(1), st, end))
                continue
            mm = re.match(r"test result: \w+\. .* finished in ([\d.]+)s", line)
            if mm:
                if float(mm.group(1)) >= 10:
                    targets_tsv.write("%s\t%s\t%s\t%s\t%s\t%s\t%.0f\n" % (rid, date, head, osn, entry, target, float(mm.group(1))))
                else:
                    others += float(mm.group(1))
                target = None
        if last is not None:
            targets_tsv.write("%s\t%s\t%s\t%s\t%s\t(others)\t%.0f\n" % (rid, date, head, osn, entry, others))
        if target is not None:
            so_far = (last - start).total_seconds()
            targets_tsv.write("%s\t%s\t%s\t%s\t%s\t%s\t>%.0f\n" % (rid, date, head, osn, entry, target, so_far))
            for name, st in sorted(started.items()):
                tests_tsv.write("%s\t%s\t%s\t%s\t%s\t%s\t%s\t%.0f\t>%.0f\n" % (rid, date, head, osn, entry, target, name, st, so_far))
    for fh in (jobs_tsv, targets_tsv, tests_tsv, steps_tsv):
        fh.close()


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in ("fetch", "parse"):
        sys.exit(__doc__)
    (fetch if sys.argv[1] == "fetch" else parse)(sys.argv[2])
