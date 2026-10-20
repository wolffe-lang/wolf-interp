#!/usr/bin/env python3
"""tl29: the measurement, from fetch.py's cache.

    measure.py CACHE concurrency [FROM TO]        # slots in use, per 10 min
    measure.py CACHE table T [--before-n N] [--only before|after] [--tsv FILE] [--md FILE]

A run's tier: the name of its job that starts with `tier ` (pax, wolf-lang,
wolf-interp and tl28's repos name the verdict job so); a run with no such
job is `untiered (<workflow>)`. Per run:
  queue       run created_at -> the first job's started_at
  wall        run created_at -> the last job's completed_at
  job-min     the sum of every job's completed_at - started_at
  macOS-min   the same, the jobs on a macOS runner only
Before: each repo's last N runs created before T (every tier, every
workflow, completed); after: every completed run created at or after T.
"""
import glob, json, os, statistics, sys
from datetime import datetime, timezone


def ts(s):
    return datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc) if s else None


def load(cache):
    runs = []
    for f in sorted(glob.glob(os.path.join(cache, "runs-*.json"))):
        runs += json.load(open(f))
    return runs


def os_of(j):
    lab = " ".join(j.get("labels") or []).lower()
    if "macos" in lab:
        return "macos"
    if "windows" in lab:
        return "windows"
    return "linux"


def tier_of(r):
    for j in r["jobs"]:
        if (j["name"] or "").startswith("tier "):
            # a verdict job that never ran keeps its name unevaluated
            return "(cancelled before the verdict)" if "${{" in j["name"] else j["name"]
    return "untiered (%s)" % r["name"]


def stats(r):
    # a job with no runner never ran (skipped, or cancelled while queued: GitHub
    # still stamps it started and completed)
    js = [j for j in r["jobs"] if j.get("started_at") and j.get("completed_at") and j.get("runner_name")]
    if not js:
        return None
    c = ts(r["created_at"])
    first = min(ts(j["started_at"]) for j in js)
    last = max(ts(j["completed_at"]) for j in js)
    jm = sum((ts(j["completed_at"]) - ts(j["started_at"])).total_seconds() for j in js) / 60
    mm = sum((ts(j["completed_at"]) - ts(j["started_at"])).total_seconds() for j in js if os_of(j) == "macos") / 60
    # the longest any one job waited for a runner after GitHub queued it
    waits = [(ts(j["started_at"]) - ts(j["created_at"])).total_seconds() / 60 for j in js if j.get("created_at")]
    return {"queue": (first - c).total_seconds() / 60, "wall": (last - c).total_seconds() / 60, "jm": jm, "mm": mm,
            "jobs": len([j for j in r["jobs"] if j.get("conclusion") != "skipped"]), "maxwait": max(waits) if waits else 0}


def concurrency(runs, frm, to, step=600):
    """Peak jobs in progress at once, per OS, in each window of `step` s (a sweep)."""
    from datetime import timedelta
    pts = []
    for r in runs:
        for j in r["jobs"]:
            if j.get("started_at") and j.get("completed_at") and j.get("runner_name"):
                o = os_of(j)
                pts.append((ts(j["started_at"]), 1, o))
                pts.append((ts(j["completed_at"]), -1, o))
    pts.sort(key=lambda x: (x[0], x[1]))
    t0, end = ts(frm), ts(to)
    nwin = int((end - t0).total_seconds() // step) + 1
    peak = [{"linux": 0, "macos": 0, "windows": 0} for _ in range(nwin)]
    n = {"linux": 0, "macos": 0, "windows": 0}
    last_w = -1
    for t, d, o in pts:
        if t >= end:
            break
        w = int((t - t0).total_seconds() // step) if t >= t0 else -1
        # every window from the last event's up to this one holds the count
        # in force before this event
        for k in range(max(last_w, 0), w + 1):
            for oo in n:
                peak[k][oo] = max(peak[k][oo], n[oo])
        n[o] += d
        if w >= 0:
            for oo in n:
                peak[w][oo] = max(peak[w][oo], n[oo])
        last_w = max(last_w, w)
    return [(t0 + timedelta(seconds=step * i), peak[i]) for i in range(nwin)]


def med(xs):
    return statistics.median(xs) if xs else float("nan")


def p90(xs):
    if not xs:
        return float("nan")
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(round(0.9 * (len(xs) - 1))))]


def table(runs, T, nbefore, tsv=None, md=None, only=None):
    T = ts(T)
    byrepo = {}
    for r in runs:
        if r["status"] != "completed":
            continue
        s = stats(r)
        if s is None:
            continue
        byrepo.setdefault(r["repo"], []).append((r, s))
    rows = []
    for repo in sorted(byrepo):
        rs = sorted(byrepo[repo], key=lambda x: x[0]["created_at"], reverse=True)
        before = [x for x in rs if ts(x[0]["created_at"]) < T][:nbefore]
        after = [x for x in rs if ts(x[0]["created_at"]) >= T]
        for period, sel in (("before", before), ("after", after)):
            if only and period != only:
                continue
            tiers = {}
            for r, s in sel:
                tiers.setdefault(tier_of(r), []).append((r, s))
            if sel:
                tiers["(every run)"] = list(sel)
            for t in sorted(tiers, key=lambda t: (t == "(every run)", t)):
                allx = tiers[t]
                # a cancelled run's times say when it was superseded, not how long
                # the work takes: counted, and its job-minutes summed, never in a median
                xs = [x for x in allx if x[0]["conclusion"] != "cancelled"] or []
                rows.append({
                    "repo": repo, "period": period, "tier": t, "runs": len(allx), "cancelled": len(allx) - len(xs),
                    "wall": med([s["wall"] for _, s in xs]), "wall90": p90([s["wall"] for _, s in xs]),
                    "queue": med([s["queue"] for _, s in xs]), "queue90": p90([s["queue"] for _, s in xs]),
                    "maxwait": med([s["maxwait"] for _, s in xs]),
                    "jm": med([s["jm"] for _, s in xs]), "jmsum": sum(s["jm"] for _, s in allx),
                    "mm": med([s["mm"] for _, s in xs]),
                    "jobs": med([s["jobs"] for _, s in xs]) if xs else float("nan"),
                    "first": min(r["created_at"] for r, _ in allx), "last": max(r["created_at"] for r, _ in allx),
                    "ids": [r["id"] for r, _ in allx],
                })
    hdr = "repo\tperiod\ttier\truns\tcancelled\twall_med\twall_p90\tqueue_med\tqueue_p90\tslotwait_med\tjobmin_med\tjobmin_sum\tmacosmin_med\tjobs_med\tfirst\tlast\trun_ids"
    lines = [hdr]
    for x in rows:
        lines.append("%s\t%s\t%s\t%d\t%d\t%.1f\t%.1f\t%.1f\t%.1f\t%.1f\t%.1f\t%.0f\t%.1f\t%g\t%s\t%s\t%s" % (
            x["repo"], x["period"], x["tier"], x["runs"], x["cancelled"], x["wall"], x["wall90"], x["queue"], x["queue90"],
            x["maxwait"], x["jm"], x["jmsum"], x["mm"], x["jobs"], x["first"], x["last"], ",".join(str(i) for i in x["ids"])))
    if tsv:
        open(tsv, "w").write("\n".join(lines) + "\n")
    if md:
        out = ["| repo | period | tier | runs (cancelled) | wall med / p90 | queue med / p90 | slot wait med | job-min med (sum) | macOS-min med |",
               "|---|---|---|---:|---:|---:|---:|---:|---:|"]
        def f(v):
            return "—" if v != v else ("%.0f" % v if v >= 10 else "%.1f" % v)
        for x in rows:
            out.append("| %s | %s | %s | %d (%d) | %s / %s | %s / %s | %s | %s (%s) | %s |" % (
                x["repo"], x["period"], x["tier"].replace("|", "/"), x["runs"], x["cancelled"], f(x["wall"]), f(x["wall90"]),
                f(x["queue"]), f(x["queue90"]), f(x["maxwait"]), f(x["jm"]), f("%.0f" % x["jmsum"] and x["jmsum"]), f(x["mm"])))
        open(md, "w").write("\n".join(out) + "\n")
    for l in lines:
        print("\t".join(l.split("\t")[:14]))


def main():
    cache, cmd = sys.argv[1], sys.argv[2]
    runs = load(cache)
    if cmd == "concurrency":
        frm, to = sys.argv[3], sys.argv[4]
        for t, p in concurrency(runs, frm, to):
            print("%s linux %3d macos %2d windows %2d" % (t.strftime("%m-%d %H:%M"), p["linux"], p["macos"], p["windows"]))
    elif cmd == "table":
        T = sys.argv[3]
        n, tsv, md, only = 30, None, None, None
        a = sys.argv[4:]
        while a:
            if a[0] == "--before-n":
                n = int(a[1]); a = a[2:]
            elif a[0] == "--tsv":
                tsv = a[1]; a = a[2:]
            elif a[0] == "--only":
                only = a[1]; a = a[2:]
            elif a[0] == "--md":
                md = a[1]; a = a[2:]
            else:
                raise SystemExit("usage")
        table(runs, T, n, tsv, md, only)


if __name__ == "__main__":
    main()
