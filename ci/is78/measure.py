#!/usr/bin/env python3
"""ci/is78/measure.py RUN_ID... — job-minutes and wall per run (is78).

is75's measure.py with the tier and a per-OS split. For each run: the
tier (the verdict job's name), the jobs that ran (skipped excluded), the
job-minutes (the sum of startedAt -> completedAt over those jobs), the
wall (the first job's start to the last job's end; queueing before the
first job excluded), the widest job, and the job-minutes by OS (a job's
OS is the runner named in its name: windows, macos, ubuntu; the small
ubuntu jobs count as ubuntu). Needs `gh`, read-only.
"""
import json
import subprocess
import sys
from datetime import datetime

REPO = "wolffe-lang/wolf-interp"


def t(s):
    return datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ")


def os_of(name):
    for o in ("windows", "macos"):
        if o in name:
            return o
    return "ubuntu"


print("run\tevent\tsha\ttier\tjobs\tjob_min\twall_min\twidest_min\tubuntu_min\tmacos_min\twindows_min\twindows_jobs")
for r in sys.argv[1:]:
    d = json.loads(subprocess.check_output(["gh", "run", "view", r, "-R", REPO, "--json", "jobs,event,headSha"]))
    tot = wid = 0.0
    n = 0
    st, en = [], []
    by = {"ubuntu": 0.0, "macos": 0.0, "windows": 0.0}
    win = 0
    tier = "-"
    for j in d["jobs"]:
        if j["name"].startswith("tier "):
            tier = j["name"][5:]
        if not j.get("startedAt") or not j.get("completedAt") or j["completedAt"].startswith("0001") or j["conclusion"] == "skipped":
            continue
        m = (t(j["completedAt"]) - t(j["startedAt"])).total_seconds() / 60
        tot += m
        n += 1
        wid = max(wid, m)
        st.append(t(j["startedAt"]))
        en.append(t(j["completedAt"]))
        by[os_of(j["name"])] += m
        win += os_of(j["name"]) == "windows"
    wall = (max(en) - min(st)).total_seconds() / 60 if st else 0
    print("%s\t%s\t%s\t%s\t%d\t%.0f\t%.0f\t%.1f\t%.0f\t%.0f\t%.0f\t%d" % (
        r, d["event"], d["headSha"][:7], tier, n, tot, wall, wid, by["ubuntu"], by["macos"], by["windows"], win))
