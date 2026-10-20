#!/usr/bin/env python3
"""tl29: how long the org's slots were full, from fetch.py's cache.
    saturation.py CACHE FROM TO
Time with every job slot in use (20 on the free plan), with all 5 macOS
slots in use, and the runner waits of the jobs that queued meanwhile."""
import glob, json, sys
from datetime import datetime, timezone
def ts(s): return datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
cache, frm, to = sys.argv[1], ts(sys.argv[2]), ts(sys.argv[3])
pts, seen, waits = [], set(), {"linux": [], "macos": [], "windows": []}
for f in glob.glob(cache + "/runs-*.json"):
    for r in json.load(open(f)):
        for j in r["jobs"]:
            if j["id"] in seen or not (j.get("started_at") and j.get("completed_at") and j.get("runner_name")):
                continue
            seen.add(j["id"])
            lab = " ".join(j["labels"] or []).lower()
            o = "macos" if "macos" in lab else "windows" if "windows" in lab else "linux"
            pts += [(ts(j["started_at"]), 1, o), (ts(j["completed_at"]), -1, o)]
            if frm <= ts(j["created_at"]) < to:
                waits[o].append((ts(j["started_at"]) - ts(j["created_at"])).total_seconds() / 60)
pts.sort(key=lambda x: (x[0], x[1]))
n = {"linux": 0, "macos": 0, "windows": 0}
full = mac5 = 0.0
prev = None
for t, d, o in pts:
    if prev is not None and t > frm and prev < to:
        a, b = max(prev, frm), min(t, to)
        if b > a:
            dt = (b - a).total_seconds()
            if sum(n.values()) >= 20: full += dt
            if n["macos"] >= 5: mac5 += dt
    n[o] += d
    prev = t
span = (to - frm).total_seconds()
print("window %s .. %s, %.1f h" % (frm.isoformat(), to.isoformat(), span / 3600))
print("all 20 slots in use: %.1f h (%.0f%%)" % (full / 3600, 100 * full / span))
print("all 5 macOS slots in use: %.1f h (%.0f%%)" % (mac5 / 3600, 100 * mac5 / span))
for o, w in waits.items():
    w.sort()
    if w:
        print("%-7s jobs %4d  runner wait median %.1f  p90 %.1f  max %.1f min  sum %.0f job-min waiting" % (
            o, len(w), w[len(w) // 2], w[int(0.9 * (len(w) - 1))], w[-1], sum(w)))
