#!/usr/bin/env python3
"""tl29: fetch every org repo's Actions runs and their jobs, cached.

    fetch.py CACHE_DIR [--since ISO] [--before-count N] [--before ISO]

For each repo: every run created at or after --since, and at least N runs
created before --before (paging back). Each run's jobs (filter=latest) are
fetched once a run is completed and cached forever (a completed run's jobs
do not change); in-progress runs are re-fetched every time.
"""
import json, os, subprocess, sys, time

ORG = "wolffe-lang"


def gh(path):
    for attempt in range(4):
        r = subprocess.run(["gh", "api", path], capture_output=True, text=True)
        if r.returncode == 0:
            return json.loads(r.stdout)
        sys.stderr.write("gh api %s failed (%s), retry\n" % (path, r.stderr.strip()[:200]))
        time.sleep(5 * (attempt + 1))
    raise SystemExit("gh api %s failed four times" % path)


def main():
    cache = sys.argv[1]
    args = sys.argv[2:]
    since = "2026-10-19T00:00:00Z"
    before = "2026-10-20T00:00:00Z"
    nbefore = 30
    i = 0
    while i < len(args):
        if args[i] == "--since":
            since = args[i + 1]; i += 2
        elif args[i] == "--before":
            before = args[i + 1]; i += 2
        elif args[i] == "--before-count":
            nbefore = int(args[i + 1]); i += 2
        else:
            raise SystemExit("usage")
    os.makedirs(os.path.join(cache, "jobs"), exist_ok=True)
    repos = [r["name"] for r in gh("orgs/%s/repos?per_page=100&type=all" % ORG)]
    calls = 0
    for repo in sorted(repos):
        runs, page = [], 1
        while True:
            d = gh("repos/%s/%s/actions/runs?per_page=100&page=%d" % (ORG, repo, page)); calls += 1
            got = d.get("workflow_runs", [])
            runs += got
            nb = sum(1 for r in runs if r["created_at"] < before)
            oldest = min((r["created_at"] for r in runs), default="9")
            if not got or len(got) < 100 or (nb >= nbefore and oldest < since) or page >= 8:
                break
            page += 1
        keep = []
        nb = 0
        for r in sorted(runs, key=lambda r: r["created_at"], reverse=True):
            if r["created_at"] >= since:
                keep.append(r)
                nb += r["created_at"] < before
            elif nb < nbefore:
                keep.append(r); nb += 1
        def jobs_of(r):
            jf = os.path.join(cache, "jobs", "%s-%d.json" % (repo, r["id"]))
            if os.path.exists(jf):
                try:
                    with open(jf) as f:
                        c = json.load(f)
                    if c.get("status") == "completed" and c.get("run_attempt") == r["run_attempt"]:
                        return c["jobs"], 0
                except ValueError:
                    pass
            jobs, page, n = [], 1, 0
            while True:
                d = gh("repos/%s/%s/actions/runs/%d/jobs?per_page=100&filter=latest&page=%d" % (ORG, repo, r["id"], page)); n += 1
                jobs += d.get("jobs", [])
                if len(jobs) >= d.get("total_count", 0) or not d.get("jobs"):
                    break
                page += 1
            jobs = [{k: j.get(k) for k in ("id", "name", "status", "conclusion", "created_at", "started_at",
                                            "completed_at", "labels", "runner_name", "run_attempt")} for j in jobs]
            tmp = jf + ".tmp"
            with open(tmp, "w") as f:
                json.dump({"status": r["status"], "run_attempt": r["run_attempt"], "jobs": jobs}, f)
            os.replace(tmp, jf)
            return jobs, n
        from concurrent.futures import ThreadPoolExecutor
        with ThreadPoolExecutor(8) as ex:
            got = list(ex.map(jobs_of, keep))
        slim = []
        for r, (jobs, n) in zip(keep, got):
            calls += n
            slim.append({k: r.get(k) for k in ("id", "name", "path", "event", "status", "conclusion", "created_at",
                                                "run_started_at", "updated_at", "run_attempt", "head_branch", "head_sha")}
                        | {"repo": repo, "jobs": jobs})
        with open(os.path.join(cache, "runs-%s.json" % repo), "w") as f:
            json.dump(slim, f)
        print("%-18s %4d runs (%d before %s)" % (repo, len(slim), nb, before), flush=True)
    print("api calls: %d" % calls)


if __name__ == "__main__":
    main()
