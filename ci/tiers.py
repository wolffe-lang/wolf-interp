#!/usr/bin/env python3
"""ci/tiers.py — which jobs a CI run runs, and whether it ran them (tl21).

    ci/tiers.py plan [--event E] [--action A] [--label L] [--labels L1,L2]
                     [--merge [--head-sha SHA] | --paths-from FILE]
                     [--jobs "J ..."] [--entries "OS/SHARD ..."]
                     [--tier T] [--repo DIR]
                     [--github-output FILE] [--json]
    ci/tiers.py verdict        (NEEDS: the `tier` job's `toJSON(needs)`)
    ci/tiers.py classify PATH...

`plan` options also read an environment variable of their name in capitals
(EVENT, ACTION, LABEL, LABELS, HEAD_SHA, IN_JOBS, IN_ENTRIES, IN_TIER),
which is how the workflow's `plan` job passes the event in.

THE TIERS. A job is a job of .github/workflows/ci.yml other than `plan`
and `tier`. The `test` job's entries are an OS and a shard: `1`, `2`, `3`
(the test shards), `ladder` (the ladder and the bundle) or `smoke` (is75).

  release  every job, and every entry: three test shards and the ladder on
           ubuntu, macOS and windows, and the windows smoke; shard
           coverage and the bundle compared across all three OSes. The
           nightly `schedule`, a `workflow_dispatch` with `-f tier=release`,
           and a pull request carrying the `release-matrix` label (is78,
           ruling #92 R2: the release lane's tier, before any tag).
  full     every job; the entries are ubuntu's and macOS's three shards
           and ladder, and the windows smoke: the windows heavy shards and
           ladder are the nightly's and release's, not the merge's (is78).
           A push to trunk, a `workflow_dispatch` with no `jobs` input (or
           `-f tier=full`), a pull request carrying the `full-matrix` label;
           and a pull request with a changed path no rule below names, or
           whose changed paths cannot be read: what is not understood never
           shrinks a run.
  pr       any other pull-request run: is75's reduced leg, unchanged
           (ubuntu's shards and ladder, one macOS and one windows smoke,
           shard coverage, independence, dist floor), and the planner's
           test. `bundle-identical` is the one job it leaves out (one
           manifest has nothing to be identical to).
  noop     nothing but the plan and the verdict: a pull request whose
           changed paths are ALL prose; a `labeled` event for a label that
           is neither `full-matrix` nor `release-matrix`; and the
           `full-matrix` label added beside `release-matrix`, whose run
           already covers it.
  partial  a `workflow_dispatch` that names jobs (`-f jobs="plan-test"`):
           those jobs alone, and, when they include `test`, tier release's
           entries, or only those `-f entries="windows/2 ubuntu/ladder"`
           names (is78: a plant on one shard needs one runner, not
           thirteen). It is never `full` or `release`, whatever it names:
           a green `tier full` says every job ran.

PROSE IS NARROW (RULES, below): the CHANGELOG, CONTRIBUTING, the lane
notes under ci/, and the engineering documents no test reads. README.md,
docs/manual/ and docs/repl.md are NOT prose: tests/doc_truth.rs runs
every command in the first two and byte-compares the output, and
docs/repl.md is exported into the conformance bundle and read by
tests/repl_session.rs. ci/tiers-test.py holds the classes, and trips when
a test starts reading a path this file calls prose.

`verdict` checks a finished run: exit 0 when the plan succeeded, the jobs
it chose are the tier's jobs, every chosen job succeeded and every other
job was skipped; 1 otherwise. GitHub leaves an EMPTY output out of
`needs`, so each is read with a default.

`plan` prints `tier=`, `jobs=` (a JSON list), `matrix=` (the `test` job's
entries), `shard_oses=` (the OSes whose three shards `shard-coverage`
checks) and `ladder_oses=` (the OSes whose bundles `bundle-identical`
compares), to --github-output (appended) and to standard output; with
--json, one JSON object. The reasons, one line per changed path, go to
standard error, so a run's log says why it has the jobs it has. Exit 0;
2 on a usage error.
"""
import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKFLOW = ".github/workflows/ci.yml"
FULL_LABEL = "full-matrix"
RELEASE_LABEL = "release-matrix"
NOT_SELECTABLE = ("plan", "tier")
# The jobs a pull-request run leaves to the full matrix.
PR_SKIPS = ("bundle-identical",)
TIERS = ("release", "full", "pr", "noop", "partial")
# What `-f tier=` may ask for (with no `jobs`).
DISPATCH_TIERS = ("full", "release")
# The `test` job's entries, in the matrix's order: every OS's three test
# shards and its ladder, then the smokes. `ENTRIES` is every entry any tier
# runs, which is what `--entries` may name.
OSES = ("ubuntu", "macos", "windows")
SHARDS = ("1", "2", "3", "ladder")
ENTRIES = ["%s/%s" % (o, s) for o in OSES for s in SHARDS] + ["macos/smoke", "windows/smoke"]

PROSE, PR, FULL = "prose", "pr", "full"

# A changed path's class: the first rule that matches decides. `*` stays
# inside one path segment; a trailing `/**` is everything below.
RULES = [
    # Read by a test, a gate or the bundle: never prose.
    ("README.md", PR),
    ("docs/repl.md", PR),
    ("docs/manual/**", PR),
    # Prose.
    ("CHANGELOG.md", PROSE),
    ("CONTRIBUTING.md", PROSE),
    ("docs/*.md", PROSE),
    ("ci/*.md", PROSE),
    # The crate, its suite, its pinned inputs, and CI itself.
    ("src/**", PR),
    ("tests/**", PR),
    ("vendor/**", PR),
    ("upstream", PR),
    ("examples/**", PR),
    ("tools/**", PR),
    ("ci/**", PR),
    ("docs/**", PR),
    ("assets/**", PR),
    (".github/**", PR),
    ("Cargo.toml", PR),
    ("Cargo.lock", PR),
    ("build.rs", PR),
    ("rust-toolchain.toml", PR),
    ("LICENSE", PR),
    (".gitmodules", PR),
    (".gitignore", PR),
    (".gitattributes", PR),
]


def die(msg):
    print("tiers: " + msg, file=sys.stderr)
    sys.exit(2)


def matches(glob, path):
    if glob.endswith("/**"):
        return path.startswith(glob[:-2])
    rx = "".join("[^/]*" if c == "*" else re.escape(c) for c in glob)
    return re.fullmatch(rx, path) is not None


def classify(path):
    """-> (class, the rule that decided)."""
    for glob, cls in RULES:
        if matches(glob, path):
            return cls, glob
    return FULL, "no rule names it"


def workflow_jobs():
    """The workflow's jobs, in order: {id: [the ids it needs]}."""
    jobs, cur, inside = {}, None, False
    with open(os.path.join(ROOT, WORKFLOW), encoding="utf-8") as f:
        for line in f.read().splitlines():
            if re.match(r"^jobs:\s*$", line):
                inside = True
            elif inside and re.match(r"^\S", line):
                inside = False
            elif inside:
                m = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
                if m:
                    cur = m.group(1)
                    jobs[cur] = []
                    continue
                m = re.match(r"^    needs:\s*(.*)$", line)
                if m and cur:
                    jobs[cur] = re.findall(r"[A-Za-z0-9_-]+", m.group(1))
    return jobs


def jobs_of(tier, every):
    """The jobs a tier runs, of `every` selectable job, in order."""
    if tier in ("release", "full"):
        return list(every)
    if tier == "pr":
        return [j for j in every if j not in PR_SKIPS]
    return []


def matrix_of(tier, entries=None):
    """-> (the `test` job's entries, the OSes whose three shards ran, the
    OSes whose ladder ran). By tier:

      release, partial  os x {1, 2, 3, ladder} for ubuntu, macOS and
                        windows (is75's twelve, in its order), then the
                        windows smoke;
      full              ubuntu's and macOS's four, then the windows smoke
                        (is78: windows's heavy shards and ladder are the
                        nightly's and release's);
      pr, noop          ubuntu's four, then a smoke on macOS and windows
                        (is75's six).

    `entries` (a partial run's `--entries`) keeps only those, in this
    order; `ci/tiers.py plan` has refused a name that is no entry."""
    if tier in ("release", "partial"):
        heavy, smokes = OSES, ("windows",)
    elif tier == "full":
        heavy, smokes = ("ubuntu", "macos"), ("windows",)
    else:
        heavy, smokes = ("ubuntu",), ("macos", "windows")
    names = ["%s/%s" % (o, s) for o in heavy for s in SHARDS] + ["%s/smoke" % o for o in smokes]
    if entries:
        names = [n for n in ENTRIES if n in entries]
    matrix = []
    for n in names:
        osid, shard = n.split("/")
        matrix.append({"os": osid + "-latest", "osid": osid, "shard": shard})
    shard_oses = [o for o in OSES if all("%s/%s" % (o, s) in names for s in ("1", "2", "3"))]
    ladder_oses = [o for o in OSES if "%s/ladder" % o in names]
    return matrix, " ".join(shard_oses), " ".join(ladder_oses)


def merge_paths(repo, head_sha):
    """-> (the paths a pull request changes, how they were read): the
    checked-out merge commit against its first parent, the base it was
    merged into. (None, the reason) when that cannot be read."""
    def git(*args):
        return subprocess.run(["git", "-C", repo] + list(args), check=True,
                              capture_output=True, text=True).stdout

    try:
        parents = git("rev-list", "--parents", "-n", "1", "HEAD").split()
        if len(parents) != 3:
            return None, "HEAD has %d parent(s), not a pull request's merge commit" % (len(parents) - 1)
        if head_sha and parents[2] != head_sha:
            return None, "the merge's second parent %s is not the pull request's head %s" % (parents[2][:9], head_sha[:9])
        out = git("diff", "--name-only", "--no-renames", "-z", parents[1], parents[0])
    except (subprocess.CalledProcessError, FileNotFoundError, OSError) as e:
        return None, "git could not read the change (%s)" % e
    return [p for p in out.split("\0") if p], "%s..%s" % (parents[1][:9], parents[0][:9])


def plan(a):
    needs = workflow_jobs()
    every = [j for j in needs if j not in NOT_SELECTABLE]
    for j in PR_SKIPS:
        if j not in every:
            die("PR_SKIPS names %s, which is no job of %s" % (j, WORKFLOW))
    log = []
    entries = (a.entries or "").split()

    def result(tier, chosen=None):
        chosen = jobs_of(tier, every) if chosen is None else [j for j in every if j in chosen]
        matrix, shard_oses, ladder_oses = matrix_of(tier, entries if tier == "partial" else None)
        return {"tier": tier, "jobs": chosen, "matrix": matrix, "shard_oses": shard_oses,
                "ladder_oses": ladder_oses, "log": log}

    labels = [x for x in (a.labels or "").split(",") if x]
    asked = (a.jobs or "").split()
    ev = a.event or "workflow_dispatch"
    log.append("event=%s action=%s label=%s labels=%s" % (ev, a.action or "-", a.label or "-", ",".join(labels) or "-"))

    if ev == "pull_request" and a.action == "labeled" and a.label not in (FULL_LABEL, RELEASE_LABEL):
        log.append("a label other than %s and %s was added: noop" % (FULL_LABEL, RELEASE_LABEL))
        return result("noop")
    if ev == "pull_request" and a.action == "labeled" and a.label == FULL_LABEL and RELEASE_LABEL in labels:
        log.append("%s was added beside %s, whose run covers it: noop" % (FULL_LABEL, RELEASE_LABEL))
        return result("noop")
    if ev == "workflow_dispatch":
        if asked and a.tier:
            die("--jobs and --tier: a dispatch names jobs (partial) or a tier, not both")
        if entries and "test" not in asked:
            die("--entries names the test job's entries; --jobs must name test")
        for e in entries:
            if e not in ENTRIES:
                die("--entries: %s is no entry of the test job (the entries: %s)" % (e, " ".join(ENTRIES)))
        if a.tier and a.tier not in DISPATCH_TIERS:
            die("--tier: %s is no tier a dispatch can ask for (%s)" % (a.tier, ", ".join(DISPATCH_TIERS)))
    if ev == "workflow_dispatch" and asked:
        for j in asked:
            if j not in every:
                die("--jobs: %s is no job of %s (the jobs: %s)" % (j, WORKFLOW, " ".join(every)))
        for j in asked:
            for n in needs[j]:
                if n not in NOT_SELECTABLE and n not in asked:
                    die("--jobs: %s needs %s; name both" % (j, n))
        log.append("dispatch naming %s%s: partial" % (" ".join(asked), " (entries %s)" % " ".join(entries) if entries else ""))
        return result("partial", asked)
    if ev == "workflow_dispatch" and a.tier:
        log.append("dispatch asking for tier %s: %s" % (a.tier, a.tier))
        return result(a.tier)
    if ev == "schedule":
        log.append("the nightly: release")
        return result("release")
    if ev != "pull_request":
        log.append("%s: full" % ev)
        return result("full")
    if RELEASE_LABEL in labels:
        log.append("the %s label: release" % RELEASE_LABEL)
        return result("release")
    if FULL_LABEL in labels:
        log.append("the %s label: full" % FULL_LABEL)
        return result("full")

    if a.paths_from:
        f = sys.stdin if a.paths_from == "-" else open(a.paths_from, encoding="utf-8")
        paths, how = [p.strip() for p in f.read().splitlines() if p.strip()], a.paths_from
    elif a.merge:
        paths, how = merge_paths(a.repo or ROOT, a.head_sha)
    else:
        paths, how = None, "no --merge and no --paths-from"
    if paths is None:
        log.append("the changed paths cannot be read (%s): full" % how)
        return result("full")
    if not paths:
        log.append("the change names no path (%s), which no pull request's does: full" % how)
        return result("full")

    count = {PROSE: 0, PR: 0, FULL: 0}
    for p in paths:
        cls, why = classify(p)
        count[cls] += 1
        log.append("%s -> %s  [%s]" % (p, cls, why))
    if count[FULL]:
        log.append("%d of %d path(s) no rule names: full" % (count[FULL], len(paths)))
        return result("full")
    if count[PR] == 0:
        log.append("%d path(s), every one prose: noop" % len(paths))
        return result("noop")
    log.append("%d path(s), %d prose: pr" % (len(paths), count[PROSE]))
    return result("pr")


def verdict(needs):
    """The `tier` job's check, from `toJSON(needs)`."""
    plan_job = needs.get("plan") or {}
    if plan_job.get("result") != "success":
        print("tier: the plan did not succeed (%s)" % plan_job.get("result"))
        return 1
    out = plan_job.get("outputs") or {}
    tier = out.get("tier", "")
    if tier not in TIERS:
        print("tier: the plan named no tier (%r)" % tier)
        return 1
    try:
        chosen = json.loads(out.get("jobs", "[]"))
        assert isinstance(chosen, list)
    except (ValueError, AssertionError):
        print("tier: the plan's list of jobs cannot be read (%r)" % out.get("jobs"))
        return 1
    every = [j for j in needs if j != "plan"]
    bad = 0
    if tier == "partial":
        if not chosen:
            print("tier: a partial run that chose no job")
            bad += 1
    elif sorted(chosen) != sorted(jobs_of(tier, every)):
        print("tier: the plan chose %s; tier %s is %s" % (
            " ".join(chosen) or "nothing", tier, " ".join(jobs_of(tier, every)) or "nothing"))
        bad += 1
    print("tier %s: %d of %d job(s) chosen" % (tier, len(chosen), len(every)))
    for job in every:
        want = "success" if job in chosen else "skipped"
        got = (needs[job] or {}).get("result")
        ok = got == want
        bad += not ok
        print("  %-18s %-9s %s" % (job, got, "ok" if ok else "WANTED " + want))
    missing = [j for j in chosen if j not in every]
    if missing:
        print("tier: chosen, but the tier job does not need it: %s" % " ".join(missing))
        bad += 1
    return 1 if bad else 0


def main():
    argv = sys.argv[1:]
    if argv == ["verdict"]:
        sys.exit(verdict(json.loads(os.environ["NEEDS"])))
    if argv[:1] == ["classify"]:
        for p in argv[1:]:
            print("%s\t%s\t%s" % ((p,) + classify(p)))
        return
    if argv[:1] != ["plan"]:
        die("usage: ci/tiers.py plan [options] | verdict | classify PATH...")
    env = os.environ.get
    ap = argparse.ArgumentParser(prog="ci/tiers.py plan")
    ap.add_argument("--event", default=env("EVENT"))
    ap.add_argument("--action", default=env("ACTION"))
    ap.add_argument("--label", default=env("LABEL"))
    ap.add_argument("--labels", default=env("LABELS"))
    ap.add_argument("--head-sha", default=env("HEAD_SHA"))
    ap.add_argument("--jobs", default=env("IN_JOBS"))
    ap.add_argument("--entries", default=env("IN_ENTRIES"))
    ap.add_argument("--tier", default=env("IN_TIER"))
    ap.add_argument("--merge", action="store_true")
    ap.add_argument("--paths-from")
    ap.add_argument("--repo")
    ap.add_argument("--github-output")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args(argv[1:])
    r = plan(a)
    for line in r["log"]:
        print("tiers: " + line, file=sys.stderr)
    if a.json:
        print(json.dumps({k: r[k] for k in ("tier", "jobs", "matrix", "shard_oses", "ladder_oses")}))
        return
    out = "tier=%s\njobs=%s\nmatrix=%s\nshard_oses=%s\nladder_oses=%s\n" % (
        r["tier"], json.dumps(r["jobs"]), json.dumps(r["matrix"], separators=(",", ":")), r["shard_oses"], r["ladder_oses"])
    sys.stdout.write(out)
    if a.github_output:
        with open(a.github_output, "a", encoding="utf-8") as f:
            f.write(out)


if __name__ == "__main__":
    main()
