#!/usr/bin/env python3
"""tl29: one repo's runs created since a date, with jobs, into CACHE (jobs shared)."""
import json, os, subprocess, sys, time
ORG = "wolffe-lang"
def gh(path):
    for attempt in range(4):
        r = subprocess.run(["gh", "api", path], capture_output=True, text=True)
        if r.returncode == 0:
            return json.loads(r.stdout)
        sys.stderr.write("gh api %s failed (%s), retry\n" % (path, r.stderr.strip()[:200])); time.sleep(5 * (attempt + 1))
    raise SystemExit("gh api failed: " + path)
cache, repo, since = sys.argv[1], sys.argv[2], sys.argv[3]
jobsdir = sys.argv[4]
runs = []
d = gh("repos/%s/%s/actions/runs?per_page=100&created=%%3E%%3D%s" % (ORG, repo, since))
total = d["total_count"]; runs += d["workflow_runs"]; page = 1
while len(runs) < total and page < 20:
    page += 1
    d = gh("repos/%s/%s/actions/runs?per_page=100&page=%d&created=%%3E%%3D%s" % (ORG, repo, page, since)); runs += d["workflow_runs"]
    if not d["workflow_runs"]: break
def jobs_of(r):
    jf = os.path.join(jobsdir, "%s-%d.json" % (repo, r["id"]))
    if os.path.exists(jf):
        try:
            c = json.load(open(jf))
            if c.get("status") == "completed" and c.get("run_attempt") == r["run_attempt"]: return c["jobs"]
        except ValueError: pass
    d = gh("repos/%s/%s/actions/runs/%d/jobs?per_page=100&filter=latest" % (ORG, repo, r["id"]))
    jobs = [{k: j.get(k) for k in ("id", "name", "status", "conclusion", "created_at", "started_at", "completed_at", "labels", "runner_name", "run_attempt")} for j in d["jobs"]]
    json.dump({"status": r["status"], "run_attempt": r["run_attempt"], "jobs": jobs}, open(jf + ".tmp", "w")); os.replace(jf + ".tmp", jf)
    return jobs
from concurrent.futures import ThreadPoolExecutor
with ThreadPoolExecutor(8) as ex:
    got = list(ex.map(jobs_of, runs))
out = [{k: r.get(k) for k in ("id", "name", "path", "event", "status", "conclusion", "created_at", "run_started_at", "updated_at", "run_attempt", "head_branch", "head_sha")} | {"repo": repo, "jobs": j} for r, j in zip(runs, got)]
os.makedirs(cache, exist_ok=True)
json.dump(out, open(os.path.join(cache, "runs-%s.json" % repo), "w"))
print(repo, len(out), "runs since", since)
