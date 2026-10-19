#!/usr/bin/env python3
"""ci/tiers-test.py — the planner's own proof (tl21). python3 and git only.

    ci/tiers-test.py

  T1  the tiers by event: the nightly, `-f tier=release` and the
      `release-matrix` label are release (is78); a trunk push, a dispatch
      and the `full-matrix` label are full; another label's `labeled`
      event is a noop, and so is `full-matrix` added beside
      `release-matrix`; a dispatch that names jobs is partial, keeps only
      the test entries `-f entries` names, and refuses a job that is none
      of the workflow's or whose needs it leaves out, an entry that is
      none, entries without the test job, a tier beside jobs, and a tier
      a dispatch cannot ask for.
  T2  the pull-request tiers by path: code is pr; prose alone is noop;
      prose beside code is pr. What tests read is never prose: README.md,
      docs/manual/, docs/repl.md, and every tracked path under the vendored
      spec and corpus.
  T3  full wins: a path no rule names, alone or beside anything; changed
      paths that cannot be read; a change that names no path.
  T4  the wiring of .github/workflows/ci.yml: every job needs the plan and
      runs only when the plan names it; every plan output is read with a
      default; the `tier` job needs every job, runs on `!cancelled()` and
      checks with `ci/tiers.py verdict`; cancel-in-progress is for pull
      requests only and every other run has a concurrency group of its own
      run id; the trunk push, the nightly, the label and the dispatch's
      `jobs`, `tier` and `entries` inputs are there; the `test` matrix is,
      entry for entry, is75's twelve and the windows smoke on a release
      run, ubuntu's and macOS's eight and the windows smoke on a full run
      (no windows heavy entry at the merge), is75's six on a PR run; the
      bundle is compared across the OSes the plan says ran a ladder.
  T5  the tripwire: no tracked path the planner calls prose is named by a
      line of src/, tests/, build.rs or ci/*.sh that reads a file. The
      search is shown to fire on the three it must find. (A WARN, not a
      failure: a tracked path no rule names; its pull request runs full.)
  T6  the verdict: green for a release, a full, a pr, a noop and a
      partial run; red for a chosen job failed, skipped or cancelled, a
      job that ran unasked, a failed plan, a plan with no tier, a tier
      whose jobs are not that tier's (a `full` or a `release` that chose
      less), and green when GitHub drops an output that would have been
      empty.
  T7  the change read from git: a real merge commit against its first
      parent, with the base moved on meanwhile; red-to-full when HEAD is no
      merge or its second parent is not the pull request's head.

Exit 0 when every check passes, 1 otherwise.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PLAN = os.path.join(ROOT, "ci", "tiers.py")
WORKFLOW = os.path.join(ROOT, ".github", "workflows", "ci.yml")
ENV_KEYS = ("EVENT", "ACTION", "LABEL", "LABELS", "HEAD_SHA", "IN_JOBS", "IN_TIER", "IN_ENTRIES", "NEEDS")
fails = 0


def say(ok, what):
    global fails
    print("tiers-test: %s %s" % ("PASS" if ok else "FAIL", what))
    fails += not ok


def clean_env(**more):
    env = {k: v for k, v in os.environ.items() if k not in ENV_KEYS}
    env.update(more)
    return env


def plan(paths=None, **kw):
    args = [sys.executable, PLAN, "plan", "--json"]
    flags = kw.pop("flags", [])
    for k, v in kw.items():
        args += ["--" + k.replace("_", "-"), v]
    args += flags
    if paths is not None:
        args += ["--paths-from", "-"]
    r = subprocess.run(args, input="\n".join(paths or []) + "\n", capture_output=True, text=True, env=clean_env())
    if r.returncode != 0:
        return {"tier": "ERROR rc=%d %s" % (r.returncode, r.stderr.strip()[-200:]), "jobs": [], "matrix": [], "shard_oses": "", "ladder_oses": ""}
    return json.loads(r.stdout)


wf = open(WORKFLOW, encoding="utf-8").read()
JOBS = re.findall(r"^  ([A-Za-z0-9_-]+):\s*$", wf[wf.index("\njobs:\n"):], re.M)
EVERY = [j for j in JOBS if j not in ("plan", "tier")]
PR_JOBS = [j for j in EVERY if j != "bundle-identical"]


def names(r):
    return [e["osid"] + "/" + e["shard"] for e in r["matrix"]]


HEAVY = [o + "/" + s for o in ("ubuntu", "macos", "windows") for s in ("1", "2", "3", "ladder")]
RELEASE_M = HEAVY + ["windows/smoke"]
FULL_M = HEAVY[:8] + ["windows/smoke"]
PR_M = HEAVY[:4] + ["macos/smoke", "windows/smoke"]


def expect(name, got, tier, jobs):
    why = []
    if got["tier"] != tier:
        why.append("tier %s, wanted %s" % (got["tier"], tier))
    if got["jobs"] != jobs:
        why.append("jobs %s, wanted %s" % (" ".join(got["jobs"]) or "none", " ".join(jobs) or "none"))
    shown = "every job (%d)" % len(EVERY) if jobs == EVERY else (" ".join(jobs) or "no job")
    say(not why, "%s: %s%s" % (name, tier, (" (" + "; ".join(why) + ")") if why else ", " + shown))


# --- T1 the tiers by event ---------------------------------------------------
say(EVERY == ["plan-test", "test", "shard-coverage", "bundle-identical", "independence", "dist-floor"],
    "T1 the workflow's jobs between plan and tier: %s" % " ".join(EVERY))
expect("T1 a push to trunk", plan(event="push"), "full", EVERY)
expect("T1 the nightly", plan(event="schedule"), "release", EVERY)
expect("T1 a dispatch, no input", plan(event="workflow_dispatch"), "full", EVERY)
expect("T1 a dispatch asking for tier full", plan(event="workflow_dispatch", tier="full"), "full", EVERY)
expect("T1 a dispatch asking for tier release", plan(event="workflow_dispatch", tier="release"), "release", EVERY)
for t in ("pr", "noop", "partial", "everything"):
    say(plan(event="workflow_dispatch", tier=t)["tier"].startswith("ERROR rc=2"), "T1 a dispatch asking for tier %s is refused" % t)
say(plan(event="workflow_dispatch", tier="release", jobs="plan-test")["tier"].startswith("ERROR rc=2"),
    "T1 a dispatch naming jobs and a tier is refused")
expect("T1 the tier input means nothing to a push", plan(event="push", tier="release"), "full", EVERY)
expect("T1 a dispatch naming the planner's test", plan(event="workflow_dispatch", jobs="plan-test"), "partial", ["plan-test"])
expect("T1 a dispatch naming two jobs, in the workflow's order",
       plan(event="workflow_dispatch", jobs="shard-coverage test"), "partial", ["test", "shard-coverage"])
expect("T1 a dispatch naming every job is still partial",
       plan(event="workflow_dispatch", jobs=" ".join(EVERY)), "partial", EVERY)
say(plan(event="workflow_dispatch", jobs="tset")["tier"].startswith("ERROR rc=2"), "T1 a dispatch naming no job of the workflow is refused")
say(plan(event="workflow_dispatch", jobs="shard-coverage")["tier"].startswith("ERROR rc=2"),
    "T1 a dispatch naming a job without the job it needs is refused")
r = plan(event="workflow_dispatch", jobs="test")
say(r["tier"] == "partial" and names(r) == RELEASE_M, "T1 a partial run of the test job, no entries: tier release's entries (%s)" % " ".join(names(r)))
r = plan(event="workflow_dispatch", jobs="test", entries="windows/2")
say(r["tier"] == "partial" and r["jobs"] == ["test"] and names(r) == ["windows/2"] and r["shard_oses"] == "" and r["ladder_oses"] == "",
    "T1 a partial run of one entry: that entry alone (%s), no OS's shards or ladder whole" % " ".join(names(r)))
r = plan(event="workflow_dispatch", jobs="test shard-coverage bundle-identical",
         entries="windows/ladder windows/3 windows/2 windows/1 ubuntu/1 ubuntu/2 ubuntu/3 ubuntu/ladder macos/smoke")
say(r["tier"] == "partial" and names(r) == HEAVY[:4] + HEAVY[8:] + ["macos/smoke"]
    and r["shard_oses"] == "ubuntu windows" and r["ladder_oses"] == "ubuntu windows",
    "T1 a partial run's entries come in the matrix's order, and name the OSes they make whole (%s; %s; %s)" % (
        " ".join(names(r)), r["shard_oses"], r["ladder_oses"]))
say(plan(event="workflow_dispatch", jobs="test", entries="windows/4")["tier"].startswith("ERROR rc=2"),
    "T1 a dispatch naming an entry that is none is refused")
say(plan(event="workflow_dispatch", jobs="test", entries="ubuntu/smoke")["tier"].startswith("ERROR rc=2"),
    "T1 a dispatch naming an entry no tier runs is refused")
say(plan(event="workflow_dispatch", jobs="plan-test", entries="windows/2")["tier"].startswith("ERROR rc=2"),
    "T1 a dispatch naming entries but not the test job is refused")
say(plan(event="workflow_dispatch", entries="windows/2")["tier"].startswith("ERROR rc=2"),
    "T1 a dispatch naming entries and no jobs is refused")
expect("T1 a pull request labelled full-matrix, prose only",
       plan(["ci/tl99.md"], event="pull_request", action="synchronize", labels="bug,full-matrix"), "full", EVERY)
expect("T1 the full-matrix label added",
       plan(["ci/tl99.md"], event="pull_request", action="labeled", label="full-matrix", labels="full-matrix"), "full", EVERY)
expect("T1 a pull request labelled release-matrix, prose only",
       plan(["ci/tl99.md"], event="pull_request", action="synchronize", labels="release-matrix"), "release", EVERY)
expect("T1 the release-matrix label added",
       plan(["src/eval/mod.rs"], event="pull_request", action="labeled", label="release-matrix", labels="release-matrix"), "release", EVERY)
expect("T1 the release-matrix label added beside full-matrix",
       plan(["src/eval/mod.rs"], event="pull_request", action="labeled", label="release-matrix", labels="full-matrix,release-matrix"),
       "release", EVERY)
expect("T1 the full-matrix label added beside release-matrix (whose run covers it)",
       plan(["src/eval/mod.rs"], event="pull_request", action="labeled", label="full-matrix", labels="release-matrix,full-matrix"),
       "noop", [])
expect("T1 a push to a pull request carrying both labels",
       plan(["src/eval/mod.rs"], event="pull_request", action="synchronize", labels="full-matrix,release-matrix"), "release", EVERY)
expect("T1 another label added", plan(["src/eval/mod.rs"], event="pull_request", action="labeled", label="bug", labels="bug"), "noop", [])
expect("T1 another label added to a labelled pull request",
       plan(["src/eval/mod.rs"], event="pull_request", action="labeled", label="bug", labels="bug,full-matrix"), "noop", [])
expect("T1 another label added to a release-labelled pull request",
       plan(["src/eval/mod.rs"], event="pull_request", action="labeled", label="bug", labels="bug,release-matrix"), "noop", [])
expect("T1 the jobs input means nothing to a pull request",
       plan(["src/eval/mod.rs"], event="pull_request", action="synchronize", jobs="plan-test"), "pr", PR_JOBS)
expect("T1 the tier input means nothing to a pull request",
       plan(["src/eval/mod.rs"], event="pull_request", action="synchronize", tier="release"), "pr", PR_JOBS)

# --- T2 the pull-request tiers by path ------------------------------------------
pr = dict(event="pull_request", action="synchronize")
for p in ["src/eval/mod.rs", "tests/doc_truth.rs", "tests/ub/p2_frozen_write.lu", "vendor/upstream/corpus/hello.lu",
          "vendor/upstream/spec/02-memory-model.md", "vendor/upstream/spec/anchors.json", "vendor/upstream/spec/prelude.json",
          "upstream", "Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", "LICENSE", "examples/pack.lu",
          ".github/workflows/ci.yml", ".github/workflows/release.yml", "ci/tiers.py", "ci/tiers-test.py",
          "ci/test-shards.sh", "ci/tl21/before.tsv", "tools/replkeys_probe.py", "assets/wolf-logo.svg"]:
    expect("T2 %s" % p, plan([p], **pr), "pr", PR_JOBS)
for p in ["README.md", "docs/repl.md", "docs/manual/01-running-programs.md", "docs/manual/README.md",
          "docs/manual/new/page.md", "docs/figure.png", "tests/faults/README.md", "vendor/README.md"]:
    expect("T2 not prose (a test reads it, or it is no document): %s" % p, plan([p], **pr), "pr", PR_JOBS)
for p in ["CHANGELOG.md", "CONTRIBUTING.md", "docs/README.md", "docs/approximation-contract.md",
          "docs/conformance-bundle.md", "docs/divergence-log.md", "ci/is75.md", "ci/tl21.md"]:
    expect("T2 prose: %s" % p, plan([p], **pr), "noop", [])
expect("T2 prose alone, several", plan(["CHANGELOG.md", "ci/tl21.md", "docs/divergence-log.md"], **pr), "noop", [])
expect("T2 prose beside code", plan(["CHANGELOG.md", "src/eval/mod.rs", "ci/tl21.md"], **pr), "pr", PR_JOBS)
expect("T2 prose beside a document a test reads", plan(["CHANGELOG.md", "README.md"], **pr), "pr", PR_JOBS)
r = plan(["src/eval/mod.rs"], **pr)
say([e["shard"] for e in r["matrix"]] == ["1", "2", "3", "ladder", "smoke", "smoke"] and r["shard_oses"] == "ubuntu",
    "T2 a pull request's matrix: ubuntu's four and two smokes; shard coverage checks ubuntu")

# --- T3 full wins ---------------------------------------------------------------
expect("T3 a path no rule names", plan(["newplace/thing.rs"], **pr), "full", EVERY)
expect("T3 a new file at the root", plan(["deny.toml"], **pr), "full", EVERY)
expect("T3 an unknown path beside prose", plan(["CHANGELOG.md", "newplace/notes.md"], **pr), "full", EVERY)
expect("T3 an unknown path beside code", plan(["src/eval/mod.rs", "newplace/thing.rs"], **pr), "full", EVERY)
expect("T3 no changed paths to read", plan(**pr), "full", EVERY)
expect("T3 a change that names no path", plan([], **pr), "full", EVERY)
expect("T3 a merge that cannot be read (no repository there)",
       plan(repo=os.path.join(ROOT, "no", "such", "dir"), flags=["--merge"], **pr), "full", EVERY)

# --- T6 the verdict -------------------------------------------------------------
def verdict(needs):
    r = subprocess.run([sys.executable, PLAN, "verdict"], capture_output=True, text=True, env=clean_env(NEEDS=json.dumps(needs)))
    return r.returncode, r.stdout + r.stderr


def run_of(tier, chosen, results=None, drop=()):
    outs = {"tier": tier, "jobs": json.dumps(chosen), "matrix": "[]", "shard_oses": "ubuntu"}
    for k in drop:  # GitHub leaves an empty output out of `needs`
        del outs[k]
    n = {"plan": {"result": "success", "outputs": outs}}
    for j in EVERY:
        n[j] = {"result": "success" if j in chosen else "skipped", "outputs": {}}
    n.update(results or {})
    return n


def vexpect(name, needs, want):
    rc, text = verdict(needs)
    say(rc == want, "T6 %s: exit %d%s" % (name, rc, "" if rc == want else " (wanted %d) %s" % (want, text.strip()[-300:])))


failed = {"result": "failure", "outputs": {}}
vexpect("a green release run", run_of("release", EVERY), 0)
vexpect("a green full run", run_of("full", EVERY), 0)
vexpect("a green pr run", run_of("pr", PR_JOBS), 0)
vexpect("a noop run", run_of("noop", []), 0)
vexpect("a noop run whose jobs output is absent", run_of("noop", [], drop=["jobs"]), 0)
vexpect("a green run whose other outputs are absent", run_of("pr", PR_JOBS, drop=["matrix", "shard_oses"]), 0)
vexpect("a partial run of the planner's test", run_of("partial", ["plan-test"]), 0)
vexpect("a chosen job failed", run_of("full", EVERY, {"test": failed}), 1)
vexpect("the planner's test failed", run_of("pr", PR_JOBS, {"plan-test": failed}), 1)
vexpect("a chosen job was skipped", run_of("pr", PR_JOBS, {"shard-coverage": {"result": "skipped", "outputs": {}}}), 1)
vexpect("a chosen job was cancelled", run_of("pr", PR_JOBS, {"test": {"result": "cancelled", "outputs": {}}}), 1)
vexpect("a job ran unasked", run_of("pr", PR_JOBS, {"bundle-identical": {"result": "success", "outputs": {}}}), 1)
vexpect("a job ran in a noop run", run_of("noop", [], {"independence": {"result": "success", "outputs": {}}}), 1)
vexpect("a full run that chose less than every job", run_of("full", PR_JOBS), 1)
vexpect("a release run that chose less than every job", run_of("release", PR_JOBS), 1)
vexpect("a release run whose test job failed", run_of("release", EVERY, {"test": failed}), 1)
vexpect("a full run whose jobs output is absent", run_of("full", EVERY, drop=["jobs"]), 1)
vexpect("a pr run that chose one job", run_of("pr", ["plan-test"]), 1)
vexpect("a partial run that chose nothing", run_of("partial", []), 1)
vexpect("the plan failed", {"plan": {"result": "failure", "outputs": {}}, "test": {"result": "skipped", "outputs": {}}}, 1)
vexpect("the plan named no tier", {"plan": {"result": "success", "outputs": {"jobs": "[]"}}, "test": {"result": "skipped", "outputs": {}}}, 1)
vexpect("the plan named a tier that is none", run_of("fast", []), 1)
vexpect("the plan's jobs are no list", {"plan": {"result": "success", "outputs": {"tier": "pr", "jobs": "plan-test"}}}, 1)
vexpect("a chosen job the tier job does not need", run_of("partial", ["plan-test", "lint"]), 1)

# --- T4 the wiring --------------------------------------------------------------
blocks = {}
for j in JOBS:
    m = re.search(r"^  %s:\s*\n(.*?)(?=^  [A-Za-z0-9_-]+:\s*$|\Z)" % re.escape(j), wf, re.M | re.S)
    blocks[j] = m.group(1) if m else ""
say(JOBS[:1] == ["plan"] and JOBS[-1:] == ["tier"], "T4 the plan is the first job and the tier is the last")
for j in EVERY:
    b = blocks[j]
    needs = re.search(r"^    needs: (.*)$", b, re.M)
    ok = needs is not None and "plan" in re.findall(r"[A-Za-z0-9_-]+", needs.group(1))
    ok = ok and ("    if: contains(fromJSON(needs.plan.outputs.jobs || '[]'), '%s')\n" % j) in b
    say(ok, "T4 %s needs the plan and runs only when the plan names it" % j)
reads = re.findall(r"needs\.plan\.outputs\.([a-z_]+)(\s*\|\|)?", wf)
say(len(reads) >= 9 and all(d for _, d in reads), "T4 every plan output is read with a default (%d reads: %s)" % (
    len(reads), " ".join(sorted(set(k for k, _ in reads)))))
say(all(("      %s: ${{ steps.plan.outputs.%s }}\n" % (k, k)) in blocks["plan"] for k in ("tier", "jobs", "matrix", "shard_oses", "ladder_oses")),
    "T4 the plan job declares tier, jobs, matrix, shard_oses and ladder_oses")
say('        run: python3 ci/tiers.py plan --merge --github-output "$GITHUB_OUTPUT"\n' in blocks["plan"]
    and "          fetch-depth: 2\n" in blocks["plan"] and "HEAD_SHA: ${{ github.event.pull_request.head.sha }}" in blocks["plan"],
    "T4 the plan reads the merge commit (depth 2) and checks its second parent")
say("      - run: python3 ci/tiers-test.py\n" in blocks["plan-test"], "T4 the plan-test job runs this file")
t = blocks["tier"]
tn = re.findall(r"^      - ([A-Za-z0-9_-]+)\s*$", t, re.M)
say(sorted(tn) == sorted(["plan"] + EVERY), "T4 the tier job needs every job (%d of %d)" % (len(tn), len(EVERY) + 1))
ifs = re.findall(r"^    if: (.*)$", t, re.M)
say(ifs == ["${{ !cancelled() }}"], "T4 the tier job runs on !cancelled(), never always() (%s)" % "; ".join(ifs))
say("    name: tier ${{ needs.plan.outputs.tier || 'unplanned' }}\n" in t, "T4 the tier job is named for the tier")
say("        run: python3 ci/tiers.py verdict\n" in t and "          NEEDS: ${{ toJSON(needs) }}\n" in t and "actions/checkout" in t,
    "T4 the tier job's check is ci/tiers.py verdict over toJSON(needs)")
say("  cancel-in-progress: ${{ github.event_name == 'pull_request' }}\n" in wf, "T4 cancel-in-progress for pull requests only")
group = re.search(r"^concurrency:\n  group: (.*)$", wf, re.M)
g = group.group(1).rstrip() if group else ""
say(g.endswith("|| format('{0}-{1}', github.event_name, github.run_id) }}") and "github.ref" not in g.replace("github.head_ref", ""),
    "T4 a run that is no pull request's has a concurrency group of its own run id")
say("&& 'release' || contains(github.event.pull_request.labels.*.name, 'full-matrix') && 'full' || 'reduced'" in g
    and "format('noop-{0}', github.run_id)" in g,
    "T4 a pull request's release, full and reduced runs are separate groups, and another label's noop cancels nothing")
say("github.event.label.name != 'full-matrix' && github.event.label.name != 'release-matrix' || github.event.label.name == 'full-matrix' && contains(github.event.pull_request.labels.*.name, 'release-matrix'))) && format('noop-{0}'" in g,
    "T4 full-matrix added beside release-matrix is a noop's group, as the plan says, and cancels the release run of nothing")
say(re.search(r"^  push:\n    branches: \[trunk, main\]", wf, re.M) is not None, "T4 a push to trunk runs")
say(re.search(r"^  schedule:\n(\s+#.*\n)*\s+- cron: ", wf, re.M) is not None, "T4 the nightly runs")
say("    types: [opened, synchronize, reopened, labeled]\n" in wf, "T4 adding a label starts a run")
say(re.search(r"^  workflow_dispatch:\n    inputs:\n      jobs:\n", wf, re.M) is not None and "IN_JOBS: ${{ inputs.jobs }}" in blocks["plan"],
    "T4 a dispatch takes the jobs input")
say(re.search(r"^      tier:\n        description: .*\n        type: string\n        default: \"\"\n", wf, re.M) is not None
    and re.search(r"^      entries:\n        description: .*\n        type: string\n        default: \"\"\n", wf, re.M) is not None
    and "IN_TIER: ${{ inputs.tier }}" in blocks["plan"] and "IN_ENTRIES: ${{ inputs.entries }}" in blocks["plan"],
    "T4 a dispatch takes the tier and entries inputs, empty by default, and the plan reads them")
want = [{"os": o + "-latest", "osid": o, "shard": s} for o in ("ubuntu", "macos", "windows") for s in ("1", "2", "3", "ladder")]
smoke = {"os": "windows-latest", "osid": "windows", "shard": "smoke"}
r = plan(event="schedule")
say(r["matrix"] == want + [smoke] and r["shard_oses"] == "ubuntu macos windows" and r["ladder_oses"] == "ubuntu macos windows",
    "T4 the release matrix is is75's twelve entries, in its order, and the windows smoke; every OS's shards and ladder")
r = plan(event="push")
say(r["matrix"] == want[:8] + [smoke] and r["shard_oses"] == "ubuntu macos" and r["ladder_oses"] == "ubuntu macos",
    "T4 the full matrix is ubuntu's and macOS's eight entries, in is75's order, and the windows smoke; their shards and ladders")
heavy_win = [e for e in want if e["osid"] == "windows"]
merge = [plan(event="push"), plan(["src/main.rs"], event="pull_request", action="labeled", label="full-matrix", labels="full-matrix"),
         plan(["src/main.rs"], event="pull_request", action="synchronize"), plan(["newplace/x.rs"], event="pull_request", action="synchronize")]
say(all(not any(e in m["matrix"] for e in heavy_win) and smoke in m["matrix"] for m in merge),
    "T4 no merge-gate run (a trunk push, the label, a PR, a PR with a path no rule names) has a windows heavy entry; each has the windows smoke")
r = plan(["src/main.rs"], **pr)
want = want[:4] + [{"os": "macos-latest", "osid": "macos", "shard": "smoke"}, smoke]
say(r["matrix"] == want and r["ladder_oses"] == "ubuntu", "T4 the pull-request matrix is is75's six entries, in its order")
b = blocks["bundle-identical"]
say("      LADDER_OSES: ${{ needs.plan.outputs.ladder_oses || 'ubuntu macos windows' }}\n" in b and 'set -- $LADDER_OSES' in b
    and '"bundle-manifest-$os-latest/MANIFEST.json"' in b and "macos-latest windows-latest" not in b,
    "T4 the bundle is compared across the OSes the plan says ran a ladder, all three when it says nothing")
say("      SHARD_OSES: ${{ needs.plan.outputs.shard_oses || 'ubuntu macos windows' }}\n" in blocks["shard-coverage"],
    "T4 shard coverage checks the OSes the plan says were sharded, all three when it says nothing")

# --- T7 the change read from git ----------------------------------------------------
def git(repo, *args):
    cmd = ["git", "-C", repo, "-c", "user.name=tiers-test", "-c", "user.email=tiers-test@invalid",
           "-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"] + list(args)
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout.strip()


def write(repo, rel, text):
    full = os.path.join(repo, rel)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, "w", encoding="utf-8") as f:
        f.write(text)


def merged(topic_paths):
    """A repository whose HEAD merges a topic branch (changing topic_paths)
    into a base that moved on meanwhile -> (repo, the topic's head)."""
    repo = tempfile.mkdtemp(prefix="tiers-test-")
    git(repo, "init", "-q", "-b", "trunk")
    for rel in ("src/lib.rs", "docs/divergence-log.md", "CHANGELOG.md"):
        write(repo, rel, "one\n")
    git(repo, "add", ".")
    git(repo, "commit", "-q", "-m", "base")
    git(repo, "checkout", "-q", "-b", "topic")
    for rel in topic_paths:
        write(repo, rel, "the topic\n")
    git(repo, "add", ".")
    git(repo, "commit", "-q", "-m", "topic")
    head = git(repo, "rev-parse", "HEAD")
    git(repo, "checkout", "-q", "trunk")
    write(repo, "src/moved.rs", "the base moved on\n")
    git(repo, "add", ".")
    git(repo, "commit", "-q", "-m", "trunk moves")
    git(repo, "merge", "-q", "--no-ff", "-m", "merge", "topic")
    return repo, head


made = []
try:
    repo, head = merged(["docs/divergence-log.md", "CHANGELOG.md", "ci/tl99 the note.md"])
    made.append(repo)
    expect("T7 a prose-only merge, the base moved on (its src/moved.rs is not the pull request's)",
           plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "noop", [])
    expect("T7 the same merge, the head not given", plan(repo=repo, flags=["--merge"], **pr), "noop", [])
    expect("T7 the merge's second parent is not the pull request's head",
           plan(repo=repo, head_sha="0" * 40, flags=["--merge"], **pr), "full", EVERY)
    git(repo, "checkout", "-q", "topic")
    expect("T7 HEAD is the branch, not a merge commit", plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "full", EVERY)
    repo, head = merged(["docs/divergence-log.md", "src/lib.rs"])
    made.append(repo)
    expect("T7 a merge that changes code and prose", plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "pr", PR_JOBS)
    repo, head = merged(["CHANGELOG.md", "somewhere/new.txt"])
    made.append(repo)
    expect("T7 a merge that adds a path no rule names", plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "full", EVERY)
except (subprocess.CalledProcessError, OSError) as e:
    say(False, "T7 could not build its repositories: %s %s" % (e, getattr(e, "stderr", "")))
finally:
    for d in made:
        shutil.rmtree(d, ignore_errors=True)

# --- T5 the tripwire: what the planner calls prose, nothing reads ---------------------
READS = re.compile(r"read_to_string|include_str!|include_bytes!|File::open|fs::read\b|read_dir|\.join\(|Path::new|PathBuf::from|\bcat\b|\bcp\b|\bgrep\b|\bsed\b")


def tracked():
    try:
        return subprocess.run(["git", "-C", ROOT, "ls-files"], check=True, capture_output=True, text=True).stdout.split("\n")
    except (subprocess.CalledProcessError, FileNotFoundError):
        return []


def classes(paths):
    r = subprocess.run([sys.executable, PLAN, "classify"] + paths, check=True, capture_output=True, text=True)
    return dict(line.split("\t")[:2] for line in r.stdout.splitlines())


def readers(paths):
    """{path: ["file:line", ...]}: the lines of code that read a file and
    name the path, or a directory above it, as a string."""
    code = [p for p in files if (p.startswith(("src/", "tests/")) and p.endswith(".rs")) or p == "build.rs"
            or (p.startswith("ci/") and p.endswith(".sh"))]
    names = {}
    for p in paths:
        cand = {p}
        d = os.path.dirname(p)
        while d:
            cand.add(d)
            d = os.path.dirname(d)
        names[p] = cand
    hits = {p: [] for p in paths}
    for c in code:
        with open(os.path.join(ROOT, c), encoding="utf-8", errors="replace") as f:
            for n, line in enumerate(f, 1):
                if line.lstrip().startswith(("//", "#")) or not READS.search(line):
                    continue
                lits = set(re.findall(r'"/?(?:\.\./)*([A-Za-z0-9_./-]+?)/?"', line))
                for p in paths:
                    if lits & names[p]:
                        hits[p].append("%s:%d" % (c, n))
    return hits


files = [f for f in tracked() if f]
if files:
    cls = classes(files)
    prose = sorted(p for p in files if cls.get(p) == "prose")
    unknown = sorted(p for p in files if cls.get(p) == "full")
    print("tiers-test: the %d tracked paths that are prose: %s" % (len(prose), " ".join(prose)))
    must = ["README.md", "docs/repl.md", "docs/manual/README.md"]
    seen = readers(must)
    say(all(seen[p] for p in must), "T5 the search fires: it finds what reads %s" % ", ".join(
        "%s (%s)" % (p, seen[p][0] if seen[p] else "NOTHING") for p in must))
    hit = {p: w for p, w in readers(prose).items() if w}
    say(not hit, "T5 no prose path is named by a line that reads a file%s" % (
        "" if not hit else ": " + "; ".join("%s at %s" % (p, " ".join(w[:3])) for p, w in sorted(hit.items()))))
    spec = [p for p in files if p.startswith(("vendor/upstream/spec/", "vendor/upstream/corpus/"))]
    say(len(spec) > 500 and all(cls.get(p) == "pr" for p in spec),
        "T5 no tracked path of the vendored spec and corpus is prose (%d paths)" % len(spec))
    odd = [p for p in prose if not p.endswith(".md")]
    say(not odd, "T5 nothing but a .md file is prose%s" % ("" if not odd else ": " + " ".join(odd[:10])))
    if unknown:
        print("tiers-test: WARN T5 %d tracked path(s) no rule names (a pull request touching one runs full): %s"
              % (len(unknown), " ".join(unknown[:20])))
    else:
        print("tiers-test: PASS T5 every one of %d tracked paths has a rule" % len(files))
else:
    say(False, "T5 not a git checkout: the tracked paths cannot be listed")

print("tiers-test: %s" % ("PASS" if fails == 0 else "FAIL (%d)" % fails))
sys.exit(1 if fails else 0)
