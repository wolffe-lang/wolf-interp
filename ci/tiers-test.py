#!/usr/bin/env python3
"""ci/tiers-test.py — the planner's own proof (tl21, is78, tl29, tl30b). python3 and git only.

    ci/tiers-test.py

  T1  the tiers by event: the nightly, `-f tier=release` and the
      `release-matrix` label are release (is78); a trunk push, a dispatch
      and every code pull request's push are full, every job but the bundle
      comparison (tl29: one ladder; tl30b, ruling #101 C: the pull request's
      own run is the merge gate); every label's `labeled` event but
      `release-matrix`'s is a noop, `full-matrix`'s included, and the
      label changes no push's tier; a dispatch that names jobs is partial, keeps only
      the test entries `-f entries` names, and refuses a job that is none
      of the workflow's or whose needs it leaves out, an entry that is
      none, entries without the test job, a tier beside jobs, and a tier
      a dispatch cannot ask for.
  T2  the pull-request tiers by path: code is full (ruling #101 C);
      prose alone is noop; prose beside code is full. What tests read is never prose: README.md,
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
      run, is75's six on a full run, a code PR's own (tl29: no macOS or
      windows heavy entry at the merge; tl30b: a code PR's plan is a trunk
      push's), and the release matrix keeps every OS's four; a pull
      request's runs are a `gate` and a `release` group, and every label
      but `release-matrix` a noop's; the
      bundle is compared across the OSes the plan says ran a ladder; a red
      nightly, and only a nightly, runs ci/nightly-red.py, from a job of
      its own after the tier job and the one whose token may write issues
      (the tier job reads); every heavy test entry (three
      shards and the ladder, every OS) builds at opt-level 2 with debug
      assertions and overflow checks on, and the smoke unoptimized
      (is85).
  T5  the tripwire: no tracked path the planner calls prose is named by a
      line of src/, tests/, build.rs or ci/*.sh that reads a file. The
      search is shown to fire on the three it must find. (A WARN, not a
      failure: a tracked path no rule names; its pull request runs full.)
  T6  the verdict: green for a release, a full, a noop and a partial
      run; red for a `pr` run (no tier since tl30b), a chosen job failed, skipped or cancelled, a
      job that ran unasked, a failed plan, a plan with no tier, a tier
      whose jobs are not that tier's (a `full` or a `release` that chose
      less, a `full` that ran the bundle comparison: is78's full), and
      green when GitHub drops an output that would have been empty.
  T7  the change read from git: a real merge commit against its first
      parent, with the base moved on meanwhile; red-to-full when HEAD is no
      merge or its second parent is not the pull request's head.
  T8  the red nightly's issue (ci/nightly-red.py, against a recording
      fake `gh`): no open issue of its title opens one, labelled, naming
      every red job and the run; an open one of the same title is
      commented on and none is opened; an open issue of another title
      does not dedupe; a run with no red job says the verdict is red; a
      `gh` that fails makes the reporter exit 1, never 0.
  T9  no job a pull_request (or pull_request_target) event can reach may
      write, by its own permissions or the workflow's: a job whose `if:`
      can be true on such an event, for any value of what the `if:` cannot
      decide (failure(), cancelled(), needs.*, a label, an input), reads
      only. A workflow with no permissions of its own counts as one that
      can write, since the repository's setting would decide. The check is
      shown to say no to a job that runs unless cancelled, and to pass one
      that runs on a red schedule alone.
  T10 every checkout whose job then runs a command keeps no git
      credentials (`persist-credentials: false`): no later step runs a git
      command that needs them. A job that ever does is named here with why
      (none today); checkouts of other repositories are counted apart.
  T11 the nightly at a commit already green in full: told by ci/covered.py
      that the commit's latest `tier release` is green, the nightly plans
      `tier skipped`, no job, and says so in its log, a warning and the
      run's summary, naming the covering run and when a full night is due
      (S1); told anything else, it plans release (S2); no other event is
      ever skipped, and no dispatch may ask for it (S3); the verdict of a
      skipped night wants every job skipped and says nothing was tested
      (S4). ci/covered.py, against a fake `gh`, covers only a green
      `tier release` check run of GitHub Actions, of this workflow, at this
      commit, the latest to complete, at most 144 hours before tonight's
      run; never another tier's, an older one, a red, cancelled or running
      one, another app's, a full page, an unanswered question or another
      event (S5). The plan job asks it before it plans, with read-only
      permissions, and only the plan step reads the answer (S6). The
      nightly's issue says a `tier skipped` night is not a green one (S7).

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
NIGHTLY_RED = os.path.join(ROOT, "ci", "nightly-red.py")
WORKFLOW = os.path.join(ROOT, ".github", "workflows", "ci.yml")
ENV_KEYS = ("EVENT", "ACTION", "LABEL", "LABELS", "HEAD_SHA", "IN_JOBS", "IN_TIER", "IN_ENTRIES", "NEEDS",
            "COVERED", "COVERED_BY", "COVERED_AT", "COVERED_RUN", "GITHUB_STEP_SUMMARY")
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
EVERY = [j for j in JOBS if j not in ("plan", "tier", "nightly-red")]
FULL_JOBS = [j for j in EVERY if j != "bundle-identical"]  # tl29: one ladder, no bundle to compare


def names(r):
    return [e["osid"] + "/" + e["shard"] for e in r["matrix"]]


HEAVY = [o + "/" + s for o in ("ubuntu", "macos", "windows") for s in ("1", "2", "3", "ladder")]
RELEASE_M = HEAVY + ["windows/smoke"]
FULL_M = HEAVY[:4] + ["macos/smoke", "windows/smoke"]  # tl29


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
expect("T1 a push to trunk", plan(event="push"), "full", FULL_JOBS)
expect("T1 the nightly", plan(event="schedule"), "release", EVERY)
expect("T1 a dispatch, no input", plan(event="workflow_dispatch"), "full", FULL_JOBS)
expect("T1 a dispatch asking for tier full", plan(event="workflow_dispatch", tier="full"), "full", FULL_JOBS)
expect("T1 a dispatch asking for tier release", plan(event="workflow_dispatch", tier="release"), "release", EVERY)
for t in ("pr", "noop", "partial", "everything"):
    say(plan(event="workflow_dispatch", tier=t)["tier"].startswith("ERROR rc=2"), "T1 a dispatch asking for tier %s is refused" % t)
say(plan(event="workflow_dispatch", tier="release", jobs="plan-test")["tier"].startswith("ERROR rc=2"),
    "T1 a dispatch naming jobs and a tier is refused")
expect("T1 the tier input means nothing to a push", plan(event="push", tier="release"), "full", FULL_JOBS)
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
expect("T1 a pull request labelled full-matrix, prose only: the diet stays, the label asks for nothing (ruling #101 C)",
       plan(["ci/tl99.md"], event="pull_request", action="synchronize", labels="bug,full-matrix"), "noop", [])
expect("T1 the full-matrix label added: a noop, the pull request's own run is the gate (ruling #101 C)",
       plan(["src/eval/mod.rs"], event="pull_request", action="labeled", label="full-matrix", labels="full-matrix"), "noop", [])
expect("T1 a code pull request's push: full, the merge gate (ruling #101 C)",
       plan(["src/eval/mod.rs"], event="pull_request", action="synchronize"), "full", FULL_JOBS)
expect("T1 a code pull request's push carrying full-matrix: full, as without it",
       plan(["src/eval/mod.rs"], event="pull_request", action="synchronize", labels="full-matrix"), "full", FULL_JOBS)
expect("T1 a code pull request opened: full",
       plan(["src/eval/mod.rs"], event="pull_request", action="opened"), "full", FULL_JOBS)
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
       plan(["src/eval/mod.rs"], event="pull_request", action="synchronize", jobs="plan-test"), "full", FULL_JOBS)
expect("T1 the tier input means nothing to a pull request",
       plan(["src/eval/mod.rs"], event="pull_request", action="synchronize", tier="release"), "full", FULL_JOBS)

# --- T2 the pull-request tiers by path ------------------------------------------
pr = dict(event="pull_request", action="synchronize")
for p in ["src/eval/mod.rs", "tests/doc_truth.rs", "tests/ub/p2_frozen_write.lu", "vendor/upstream/corpus/hello.lu",
          "vendor/upstream/spec/02-memory-model.md", "vendor/upstream/spec/anchors.json", "vendor/upstream/spec/prelude.json",
          "upstream", "Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", "LICENSE", "examples/pack.lu",
          ".github/workflows/ci.yml", ".github/workflows/release.yml", "ci/tiers.py", "ci/tiers-test.py",
          "ci/test-shards.sh", "ci/tl21/before.tsv", "tools/replkeys_probe.py", "assets/wolf-logo.svg"]:
    expect("T2 %s" % p, plan([p], **pr), "full", FULL_JOBS)
for p in ["README.md", "docs/repl.md", "docs/manual/01-running-programs.md", "docs/manual/README.md",
          "docs/manual/new/page.md", "docs/figure.png", "tests/faults/README.md", "vendor/README.md"]:
    expect("T2 not prose (a test reads it, or it is no document): %s" % p, plan([p], **pr), "full", FULL_JOBS)
for p in ["CHANGELOG.md", "CONTRIBUTING.md", "docs/README.md", "docs/approximation-contract.md",
          "docs/conformance-bundle.md", "docs/divergence-log.md", "ci/is75.md", "ci/tl21.md"]:
    expect("T2 prose: %s" % p, plan([p], **pr), "noop", [])
expect("T2 prose alone, several", plan(["CHANGELOG.md", "ci/tl21.md", "docs/divergence-log.md"], **pr), "noop", [])
expect("T2 prose beside code", plan(["CHANGELOG.md", "src/eval/mod.rs", "ci/tl21.md"], **pr), "full", FULL_JOBS)
expect("T2 prose beside a document a test reads", plan(["CHANGELOG.md", "README.md"], **pr), "full", FULL_JOBS)
r = plan(["src/eval/mod.rs"], **pr)
say([e["shard"] for e in r["matrix"]] == ["1", "2", "3", "ladder", "smoke", "smoke"] and r["shard_oses"] == "ubuntu",
    "T2 a pull request's matrix: ubuntu's four and two smokes; shard coverage checks ubuntu")

# --- T3 full wins ---------------------------------------------------------------
expect("T3 a path no rule names", plan(["newplace/thing.rs"], **pr), "full", FULL_JOBS)
expect("T3 a new file at the root", plan(["deny.toml"], **pr), "full", FULL_JOBS)
expect("T3 an unknown path beside prose", plan(["CHANGELOG.md", "newplace/notes.md"], **pr), "full", FULL_JOBS)
expect("T3 an unknown path beside code", plan(["src/eval/mod.rs", "newplace/thing.rs"], **pr), "full", FULL_JOBS)
expect("T3 no changed paths to read", plan(**pr), "full", FULL_JOBS)
expect("T3 a change that names no path", plan([], **pr), "full", FULL_JOBS)
expect("T3 a merge that cannot be read (no repository there)",
       plan(repo=os.path.join(ROOT, "no", "such", "dir"), flags=["--merge"], **pr), "full", FULL_JOBS)

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
vexpect("a green full run", run_of("full", FULL_JOBS), 0)
vexpect("a pr run: no tier since ruling #101 C", run_of("pr", FULL_JOBS), 1)
vexpect("a noop run", run_of("noop", []), 0)
vexpect("a noop run whose jobs output is absent", run_of("noop", [], drop=["jobs"]), 0)
vexpect("a green run whose other outputs are absent", run_of("full", FULL_JOBS, drop=["matrix", "shard_oses"]), 0)
vexpect("a partial run of the planner's test", run_of("partial", ["plan-test"]), 0)
vexpect("a chosen job failed", run_of("full", FULL_JOBS, {"test": failed}), 1)
vexpect("the planner's test failed", run_of("full", FULL_JOBS, {"plan-test": failed}), 1)
vexpect("a chosen job was skipped", run_of("full", FULL_JOBS, {"shard-coverage": {"result": "skipped", "outputs": {}}}), 1)
vexpect("a chosen job was cancelled", run_of("full", FULL_JOBS, {"test": {"result": "cancelled", "outputs": {}}}), 1)
vexpect("a job ran unasked", run_of("full", FULL_JOBS, {"bundle-identical": {"result": "success", "outputs": {}}}), 1)
vexpect("a job ran in a noop run", run_of("noop", [], {"independence": {"result": "success", "outputs": {}}}), 1)
vexpect("a full run that chose less than its jobs", run_of("full", ["plan-test", "test"]), 1)
vexpect("a full run that ran the bundle comparison (is78's full)", run_of("full", list(EVERY)), 1)
vexpect("a release run that chose less than every job", run_of("release", FULL_JOBS), 1)
vexpect("a release run whose test job failed", run_of("release", EVERY, {"test": failed}), 1)
vexpect("a full run whose jobs output is absent", run_of("full", FULL_JOBS, drop=["jobs"]), 1)
vexpect("a full run that chose one job", run_of("full", ["plan-test"]), 1)
vexpect("a partial run that chose nothing", run_of("partial", []), 1)
vexpect("the plan failed", {"plan": {"result": "failure", "outputs": {}}, "test": {"result": "skipped", "outputs": {}}}, 1)
vexpect("the plan named no tier", {"plan": {"result": "success", "outputs": {"jobs": "[]"}}, "test": {"result": "skipped", "outputs": {}}}, 1)
vexpect("the plan named a tier that is none", run_of("fast", []), 1)
vexpect("the plan's jobs are no list", {"plan": {"result": "success", "outputs": {"tier": "full", "jobs": "plan-test"}}}, 1)
vexpect("a chosen job the tier job does not need", run_of("partial", ["plan-test", "lint"]), 1)

# --- T4 the wiring --------------------------------------------------------------
blocks = {}
for j in JOBS:
    m = re.search(r"^  %s:\s*\n(.*?)(?=^  [A-Za-z0-9_-]+:\s*$|\Z)" % re.escape(j), wf, re.M | re.S)
    blocks[j] = m.group(1) if m else ""
say(JOBS[:1] == ["plan"] and JOBS[-2:] == ["tier", "nightly-red"], "T4 the plan is the first job, the tier the last but the nightly's issue")
for j in EVERY:
    b = blocks[j]
    needs = re.search(r"^    needs: (.*)$", b, re.M)
    ok = needs is not None and "plan" in re.findall(r"[A-Za-z0-9_-]+", needs.group(1))
    ok = ok and ("    if: ${{ (github.event_name != 'pull_request' || contains(github.event.pull_request.labels.*.name, 'full-matrix') || contains(github.event.pull_request.labels.*.name, 'ci') || contains(github.event.pull_request.labels.*.name, 'release-matrix')) && (contains(fromJSON(needs.plan.outputs.jobs || '[]'), '%s')) }}\n" % j) in b
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
say(ifs == ["${{ (github.event_name != 'pull_request' || contains(github.event.pull_request.labels.*.name, 'full-matrix') || contains(github.event.pull_request.labels.*.name, 'ci') || contains(github.event.pull_request.labels.*.name, 'release-matrix')) && (!cancelled()) }}"], "T4 the opted-in tier job runs on !cancelled(), never always() (%s)" % "; ".join(ifs))
say("    name: tier ${{ needs.plan.outputs.tier || 'unplanned' }}\n" in t, "T4 the tier job is named for the tier")
say("        run: python3 ci/tiers.py verdict\n" in t and "          NEEDS: ${{ toJSON(needs) }}\n" in t and "actions/checkout" in t,
    "T4 the tier job's check is ci/tiers.py verdict over toJSON(needs)")
say("  cancel-in-progress: ${{ github.event_name == 'pull_request' }}\n" in wf, "T4 cancel-in-progress for pull requests only")
group = re.search(r"^concurrency:\n  group: (.*)$", wf, re.M)
g = group.group(1).rstrip() if group else ""
say(g.endswith("|| format('{0}-{1}', github.event_name, github.run_id) }}") and "github.ref" not in g.replace("github.head_ref", ""),
    "T4 a run that is no pull request's has a concurrency group of its own run id")
say("contains(github.event.pull_request.labels.*.name, 'release-matrix') && 'release' || 'gate'" in g
    and "format('noop-{0}', github.run_id)" in g,
    "T4 a pull request's release and gate runs are separate groups, and a label's noop cancels nothing")
say("(github.event.action == 'labeled' && github.event.label.name != 'release-matrix') && format('noop-{0}'" in g and "full-matrix" not in g,
    "T4 every label but release-matrix, full-matrix included, is a noop's group, as the plan says; full-matrix picks no group (ruling #101 C)")
say(re.search(r"^  push:\n    branches: \[trunk, main\]", wf, re.M) is not None, "T4 a push to trunk runs")
say(re.search(r"^  schedule:\n(\s+#.*\n)*\s+- cron: ", wf, re.M) is not None, "T4 the nightly runs")
say("    types: [opened, synchronize, reopened, labeled]\n" in wf, "T4 adding a label starts a run")
say(re.search(r"^  workflow_dispatch:\n    inputs:\n      jobs:\n", wf, re.M) is not None and "IN_JOBS: ${{ inputs.jobs }}" in blocks["plan"],
    "T4 a dispatch takes the jobs input")
say(re.search(r"^      tier:\n        description: .*\n        type: string\n        default: \"\"\n", wf, re.M) is not None
    and re.search(r"^      entries:\n        description: .*\n        type: string\n        default: \"\"\n", wf, re.M) is not None
    and "IN_TIER: ${{ inputs.tier }}" in blocks["plan"] and "IN_ENTRIES: ${{ inputs.entries }}" in blocks["plan"],
    "T4 a dispatch takes the tier and entries inputs, empty by default, and the plan reads them")
want = [{"os": "ubuntu-24.04" if o == "ubuntu" else o + "-latest", "osid": o, "shard": s} for o in ("ubuntu", "macos", "windows") for s in ("1", "2", "3", "ladder")]
smoke = {"os": "windows-latest", "osid": "windows", "shard": "smoke"}
r = plan(event="schedule")
say(r["matrix"] == want + [smoke] and r["shard_oses"] == "ubuntu macos windows" and r["ladder_oses"] == "ubuntu macos windows",
    "T4 the release matrix is is75's twelve entries, in its order, and the windows smoke; every OS's shards and ladder")
macsmoke = {"os": "macos-latest", "osid": "macos", "shard": "smoke"}
r = plan(event="push")
say(r["matrix"] == want[:4] + [macsmoke, smoke] and r["shard_oses"] == "ubuntu" and r["ladder_oses"] == "ubuntu",
    "T4 the full matrix is ubuntu's four entries, in is75's order, the macOS smoke and the windows smoke; ubuntu's shards and ladder (tl29)")
heavy_off = [e for e in want if e["osid"] in ("macos", "windows")]
merge = [plan(event="push"), plan(["src/main.rs"], event="pull_request", action="synchronize", labels="full-matrix"),
         plan(["src/main.rs"], event="pull_request", action="synchronize"), plan(["newplace/x.rs"], event="pull_request", action="synchronize"),
         plan(event="workflow_dispatch"), plan(event="workflow_dispatch", tier="full")]
say(all(not any(e in m["matrix"] for e in heavy_off) and smoke in m["matrix"] and macsmoke in m["matrix"] for m in merge),
    "T4 no merge-gate run (a trunk push, a labelled PR, a PR, a PR with a path no rule names, a dispatch) has a macOS or windows heavy entry; each has both smokes (tl29)")
rel = plan(event="schedule")
say(all(e in rel["matrix"] for e in want if e["osid"] in ("macos", "windows")),
    "T4 the nightly's release matrix keeps macOS's and windows's three shards and ladder (tl29: what left the merge is here)")
f, p_ = plan(event="push"), plan(["src/main.rs"], event="pull_request", action="synchronize")
say(p_["tier"] == "full" and f["jobs"] == p_["jobs"] and f["matrix"] == p_["matrix"] and f["shard_oses"] == p_["shard_oses"] and f["ladder_oses"] == p_["ladder_oses"],
    "T4 a code pull request's run is the merge gate itself, a trunk push's plan (tl30b, ruling #101 C; ci/tl29.md R-tl29-2)")
r = plan(["src/main.rs"], **pr)
want = want[:4] + [{"os": "macos-latest", "osid": "macos", "shard": "smoke"}, smoke]
say(r["matrix"] == want and r["ladder_oses"] == "ubuntu", "T4 the pull-request matrix is is75's six entries, in its order")
b = blocks["bundle-identical"]
say("      LADDER_OSES: ${{ needs.plan.outputs.ladder_oses || 'ubuntu macos windows' }}\n" in b and 'set -- $LADDER_OSES' in b
    and '"bundle-manifest-$os-latest/MANIFEST.json"' in b and "macos-latest windows-latest" not in b,
    "T4 the bundle is compared across the OSes the plan says ran a ladder, all three when it says nothing")
say("      SHARD_OSES: ${{ needs.plan.outputs.shard_oses || 'ubuntu macos windows' }}\n" in blocks["shard-coverage"],
    "T4 shard coverage checks the OSes the plan says were sharded, all three when it says nothing")
nb = blocks.get("nightly-red", "")
steps = re.findall(r"^      - name: open the nightly's issue, or comment on the open one\n((?:        .*\n)+)", nb, re.M)
say(len(steps) == 1 and "run: python3 ci/nightly-red.py" in steps[0] and "GH_TOKEN: ${{ github.token }}" in steps[0]
    and "    needs: [plan, tier]\n" in nb and "    if: ${{ failure() && github.event_name == 'schedule' }}\n" in nb
    and "run: python3 ci/nightly-red.py" not in t,
    "T4 a red nightly, and only a nightly, runs ci/nightly-red.py, in a job of its own after the verdict's")
perms = re.search(r"^    permissions:\n((?:      .*\n)+)", nb, re.M)
tperms = re.search(r"^    permissions:\n((?:      .*\n)+)", t, re.M)
say(perms is not None and sorted(perms.group(1).split()) == sorted(["contents:", "read", "actions:", "read", "issues:", "write"])
    and tperms is not None and tperms.group(1).split() == ["contents:", "read"] and wf.count("issues: write") == 1,
    "T4 the nightly's issue job may read the code and the run and write issues; the tier job reads; no other job may write issues")

# is85: the heavy entries build optimized with every debug check on; the smoke
# does not. One recipe for every OS, so the env is the test job's own.
OPT = "2"
tenv = re.search(r"^    env:\n((?:      [A-Z_]+: .*\n)+)", blocks["test"], re.M)
tenv = dict(re.findall(r"^      ([A-Z_]+): (.*)$", tenv.group(1), re.M)) if tenv else {}
want = {"CARGO_PROFILE_%s_%s" % (p, k): v for p in ("DEV", "TEST") for k, v in (
    ("OPT_LEVEL", "${{ matrix.shard == 'smoke' && '0' || '%s' }}" % OPT),
    ("DEBUG_ASSERTIONS", '"true"'), ("OVERFLOW_CHECKS", '"true"'))}
say(all(tenv.get(k) == v for k, v in want.items()) and not re.search(r"CARGO_PROFILE_\w+_(DEBUG_ASSERTIONS|OVERFLOW_CHECKS): .*false", wf),
    "T4 every heavy test entry builds at opt-level %s with debug assertions and overflow checks on, and the smoke unoptimized (is85; %s)" % (
        OPT, " ".join("%s=%s" % (k, tenv.get(k, "unset")) for k in sorted(want))))

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
           plan(repo=repo, head_sha="0" * 40, flags=["--merge"], **pr), "full", FULL_JOBS)
    git(repo, "checkout", "-q", "topic")
    expect("T7 HEAD is the branch, not a merge commit", plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "full", FULL_JOBS)
    repo, head = merged(["docs/divergence-log.md", "src/lib.rs"])
    made.append(repo)
    expect("T7 a merge that changes code and prose", plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "full", FULL_JOBS)
    repo, head = merged(["CHANGELOG.md", "somewhere/new.txt"])
    made.append(repo)
    expect("T7 a merge that adds a path no rule names", plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "full", FULL_JOBS)
except (subprocess.CalledProcessError, OSError) as e:
    say(False, "T7 could not build its repositories: %s %s" % (e, getattr(e, "stderr", "")))
finally:
    for d in made:
        shutil.rmtree(d, ignore_errors=True)

# --- T8 the red nightly's issue (ci/nightly-red.py, a recording fake gh) ------------
FAKE_GH = """#!/usr/bin/env python3
import json, os, sys
a = sys.argv[1:]
with open(os.environ["FAKE_GH_LOG"], "a", encoding="utf-8") as f:
    f.write(json.dumps(a) + "\\n")
if os.environ.get("FAKE_GH_FAIL") and " ".join(a[:2]) == os.environ["FAKE_GH_FAIL"]:
    sys.stderr.write("HTTP 403: the fake refuses\\n")
    sys.exit(1)
if a[:1] == ["api"]:
    sys.stdout.write(os.environ.get("FAKE_GH_JOBS", ""))
elif a[:2] == ["issue", "list"]:
    sys.stdout.write(os.environ.get("FAKE_GH_ISSUES", "[]"))
elif a[:2] == ["issue", "create"]:
    print("https://github.com/o/r/issues/77")
"""
JOBS_TSV = "\n".join("\t".join(x) for x in [
    ("plan", "success", "https://x/j/1"),
    ("test (windows-latest, test shard 2 of 3)", "failure", "https://x/j/2"),
    ("test (windows-latest, the ladder and the bundle)", "cancelled", "https://x/j/3"),
    ("test (ubuntu-24.04, test shard 1 of 3)", "success", "https://x/j/4"),
    ("tier release", "", "https://x/j/5"),
]) + "\n"
RED_TITLE = "nightly red: tier release on trunk"


def nightly_red(jobs, issues, fail=None):
    d = tempfile.mkdtemp(prefix="tiers-test-gh-")
    try:
        with open(os.path.join(d, "gh"), "w", encoding="utf-8") as f:
            f.write(FAKE_GH)
        os.chmod(os.path.join(d, "gh"), 0o755)
        log = os.path.join(d, "calls.jsonl")
        env = clean_env(PATH=d + os.pathsep + os.environ.get("PATH", ""), FAKE_GH_LOG=log, FAKE_GH_JOBS=jobs,
                        FAKE_GH_ISSUES=json.dumps(issues), REPO="o/r", RUN_ID="123456789", SHA="0e536e8f00", TIER="release")
        if fail:
            env["FAKE_GH_FAIL"] = fail
        r = subprocess.run([sys.executable, NIGHTLY_RED], capture_output=True, text=True, env=env)
        calls = [json.loads(x) for x in open(log, encoding="utf-8")] if os.path.exists(log) else []
        return r.returncode, calls, r.stdout + r.stderr
    finally:
        shutil.rmtree(d, ignore_errors=True)


def call(calls, *head):
    return [c for c in calls if c[:len(head)] == list(head)]


def body_of(c):
    return c[c.index("--body") + 1] if "--body" in c else ""


if os.name == "nt":
    say(True, "T8 skipped: the fake gh is a script with a shebang (the plan-test job runs on ubuntu)")
else:
    rc, calls, out = nightly_red(JOBS_TSV, [])
    made = call(calls, "issue", "create")
    b = body_of(made[0]) if made else ""
    say(rc == 0 and len(made) == 1 and made[0][made[0].index("--title") + 1] == RED_TITLE
        and made[0][made[0].index("--label") + 1] == "nightly-red" and call(calls, "label", "create", "nightly-red")
        and "test (windows-latest, test shard 2 of 3)" in b and "the ladder and the bundle)`: cancelled" in b
        and "test (ubuntu-24.04" not in b and "`plan`" not in b and "/o/r/actions/runs/123456789" in b
        and "looked at before the next merge" in b and not call(calls, "issue", "comment"),
        "T8 no open issue: one opened, labelled, naming the red jobs (failed and cancelled) and the run, and no green job%s" % (
            "" if rc == 0 else " (rc %d: %s)" % (rc, out.strip()[-200:])))
    rc, calls, out = nightly_red(JOBS_TSV, [{"number": 41, "title": "something else"}, {"number": 42, "title": RED_TITLE}])
    com = call(calls, "issue", "comment")
    say(rc == 0 and len(com) == 1 and com[0][2] == "42" and "test shard 2 of 3" in body_of(com[0])
        and not call(calls, "issue", "create") and not call(calls, "label", "create"),
        "T8 an open issue of the title: commented on (#42), none opened")
    rc, calls, out = nightly_red(JOBS_TSV, [{"number": 41, "title": RED_TITLE + " (old)"}])
    say(rc == 0 and len(call(calls, "issue", "create")) == 1 and not call(calls, "issue", "comment"),
        "T8 an open issue of another title does not dedupe: one opened")
    rc, calls, out = nightly_red("plan\tsuccess\thttps://x/j/1\ntier release\t\thttps://x/j/5\n", [])
    made = call(calls, "issue", "create")
    say(rc == 0 and len(made) == 1 and "the `tier release` verdict is" in body_of(made[0]),
        "T8 a run with no red job: the issue says the verdict is red")
    lists = call(nightly_red(JOBS_TSV, [])[1], "issue", "list")
    say(len(lists) == 1 and lists[0][lists[0].index("--state") + 1] == "open" and lists[0][lists[0].index("--label") + 1] == "nightly-red",
        "T8 the dedupe reads the open issues labelled nightly-red")
    for f in ("issue create", "issue list", "api repos/o/r/actions/runs/123456789/jobs?per_page=100"):
        rc, calls, out = nightly_red(JOBS_TSV, [], fail=f.split(" ")[0] + " " + f.split(" ")[1] if f.startswith("issue") else "api --paginate")
        say(rc == 1 and "nightly-red: gh" in out, "T8 a gh that fails (%s) makes the reporter exit 1, loudly" % f.split("?")[0])
    env = clean_env()
    r = subprocess.run([sys.executable, NIGHTLY_RED], capture_output=True, text=True, env=env)
    say(r.returncode == 1 and "is not set" in r.stderr, "T8 the reporter refuses to run without REPO and RUN_ID")
    # What the reporter writes is public: the issue's title and body, a comment, the label's description.
    INTERNAL = re.compile(r"\b(?:lanes?|rulings?|crawls?|sprints?)\b|\b[a-z]{2}\d{2,3}[a-z]?\b", re.I)
    writes = []
    for issues in ([], [{"number": 42, "title": RED_TITLE}]):
        for c in nightly_red(JOBS_TSV, issues)[1]:
            if c[:2] in (["issue", "create"], ["issue", "comment"], ["label", "create"]):
                for flag in ("--title", "--body", "--description"):
                    if flag in c:
                        writes.append((" ".join(c[:2]) + " " + flag, c[c.index(flag) + 1]))
    said = sorted({"%s: %s" % (w, m.group(0)) for w, t in writes for m in INTERNAL.finditer(t)})
    say(bool(writes) and not said,
        "T8 nothing the reporter writes in public (the issue's title and body, a comment, the label's description) carries a "
        "lane, ruling, crawl or sprint word or an internal tag%s" % ((": " + "; ".join(said)) if said else ""))

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



# --- T9 no job a pull request reaches may write ---------------------------------------------
def reach_on(cond, event):
    """Whether a job's `if:` can be true on EVENT: False only when no value of what it cannot decide makes it so."""
    if not cond.strip():
        return True
    e = re.sub(r"^\$\{\{\s*|\s*\}\}$", "", cond.strip())
    e = re.sub(r"github\.event_name\s*(==|!=)\s*'([a-z_]+)'", lambda m: " %s " % ((m.group(2) == event) == (m.group(1) == "==")), e)
    atoms = []

    def atom(m):
        atoms.append(m.group(0))
        return " v%d " % (len(atoms) - 1)
    e = re.sub(r"\b(?:contains|startsWith|endsWith|format|toJSON|fromJSON|hashFiles)\((?:[^()]|\([^()]*\))*\)(?:\s*(?:==|!=)\s*'[^']*')?", atom, e)
    e = re.sub(r"\b(?:always|success|failure|cancelled)\(\)", atom, e)
    e = re.sub(r"[A-Za-z_][\w.*-]*\s*(?:==|!=)\s*'[^']*'", atom, e)
    e = re.sub(r"\b(?!True\b|False\b|v\d+\b)[A-Za-z_][\w.*-]*", atom, e)
    e = e.replace("&&", " and ").replace("||", " or ").replace("!", " not ")
    if len(atoms) > 12:
        return True
    for bits in range(1 << len(atoms)):
        try:
            if eval(e, {"__builtins__": {}}, {"v%d" % i: bool(bits >> i & 1) for i in range(len(atoms))}):
                return True
        except Exception:
            return True
    return False


def job_writes(block):
    """A job's write permissions, its own or (with none of its own) the workflow's."""
    own = re.search(r"^    permissions:[ \t]*(\S*)\n((?:      .*\n)*)", block, re.M)
    if own:
        text = own.group(1) + " " + own.group(2)
    else:
        top = re.search(r"^permissions:[ \t]*(\S*)\n((?:  .*\n)*)", wf, re.M)
        text = (top.group(1) + " " + top.group(2)) if top else "write-all"
    return re.findall(r"([\w-]+: write|write-all)", text)


pr_events = [e for e in ("pull_request", "pull_request_target")
             if re.search(r"^on:\n(?:(?:  .*|\s*#.*|\s*)\n)*?  %s:" % e, wf, re.M)]
reached = []
for j in JOBS:
    cond = (re.search(r"^    if: (.*)$", blocks[j], re.M) or [None, ""])[1]
    w = job_writes(blocks[j])
    hit = [e for e in pr_events if reach_on(cond, e)]
    if w and hit:
        reached.append("%s (%s, on %s)" % (j, ", ".join(w), " ".join(hit)))
writers = [j for j in JOBS if job_writes(blocks[j])]
says_no = (reach_on("${{ !cancelled() }}", "pull_request")
           and not reach_on("${{ failure() && github.event_name == 'schedule' }}", "pull_request")
           and not reach_on("github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'", "pull_request")
           and reach_on("${{ (github.event_name != 'pull_request' || contains(github.event.pull_request.labels.*.name, 'ci')) && (!cancelled()) }}", "pull_request"))
say(bool(pr_events) and not reached and says_no,
    "T9 no job a pull request can reach may write: the jobs that may (%s) run where none does%s%s"
    % (" ".join(writers) or "none", ("; reached: " + "; ".join(reached)) if reached else "",
       "" if says_no else "; the check does not say no to a job that runs unless cancelled"))

# --- T10 checkouts keep no credentials they do not need ---------------------------------------
PERSIST_OK = {}  # job -> why its checkout keeps git credentials for a later git command (none today)
own_co, other_co, kept = 0, 0, []
for j in JOBS:
    steps_ = re.split(r"(?m)^(?=      - )", blocks[j])
    for i, st in enumerate(steps_):
        if not re.search(r"^(?:      - |        )uses: actions/checkout@", st, re.M):
            continue
        if not any(re.search(r"^(?:      - |        )run: ", later, re.M) for later in steps_[i + 1:]):
            continue
        if re.search(r"^          repository: ", st, re.M):
            other_co += 1
        else:
            own_co += 1
        if not re.search(r"^          persist-credentials: false$", st, re.M) and j not in PERSIST_OK:
            kept.append(j)
say(own_co > 0 and not kept,
    "T10 every checkout whose job then runs a command keeps no git credentials (%d of this repository, %d of others; kept for a "
    "later git command by: %s)%s" % (own_co, other_co, ", ".join("%s (%s)" % kv for kv in PERSIST_OK.items()) or "none",
                                     (": still kept in %d: %s" % (len(kept), " ".join(kept))) if kept else ""))

# --- T11 the nightly at a commit already green in full (ci/covered.py, a fake gh) -------------
COVERED_PY = os.path.join(ROOT, "ci", "covered.py")
NIGHTLY_TIER = "release"
GREEN = dict(covered=NIGHTLY_TIER, covered_by="https://github.com/o/r/runs/555", covered_at="2026-11-18T12:00:00Z", covered_run="777")

# S1: the nightly, covered: skipped, no job; its line, its annotation and its summary name the covering run.
expect("T11 S1 the nightly at a commit whose tier release is green", plan(event="schedule", **GREEN), "skipped", [])
d = tempfile.mkdtemp(prefix="tiers-test-skip-")
try:
    summ, gout = os.path.join(d, "summary.md"), os.path.join(d, "out")
    args = [sys.executable, PLAN, "plan", "--event", "schedule", "--github-output", gout] + [
        x for k, v in GREEN.items() for x in ("--" + k.replace("_", "-"), v)]
    r = subprocess.run(args, capture_output=True, text=True, env=clean_env(GITHUB_STEP_SUMMARY=summ))
    outs = open(gout, encoding="utf-8").read() if os.path.exists(gout) else ""
    summary = open(summ, encoding="utf-8").read() if os.path.exists(summ) else ""
    warn = [l for l in r.stdout.splitlines() if l.startswith("::warning title=nightly skipped::")]
    said = ("nothing tested tonight", "tier release green at this commit in run 777", "https://github.com/o/r/runs/555",
            "completed 2026-11-18T12:00:00Z", "due by 2026-11-24T12:00:00Z")
    say(r.returncode == 0 and "tier=skipped\n" in outs and "jobs=[]\n" in outs and "covered_by=https://github.com/o/r/runs/555\n" in outs
        and len(warn) == 1 and all(s in warn[0] for s in said) and all(s in r.stderr for s in said) and all(s in summary for s in said),
        "T11 S1 a skipped night says so in its log, a warning and the run's summary, naming the covering run and when a full night is due%s" % (
            "" if r.returncode == 0 else " (rc %d: %s)" % (r.returncode, r.stderr.strip()[-200:])))
finally:
    shutil.rmtree(d, ignore_errors=True)

# S2: anything else the nightly is told is no skip: the nightly's tier, as today.
for c in ("none", "", "full", "partial", "skipped", "noop", "all", "Release", "release yes"):
    expect("T11 S2 the nightly told covered=%r" % c, plan(event="schedule", covered=c), "release", EVERY)
expect("T11 S2 the nightly told nothing", plan(event="schedule"), "release", EVERY)

# S3: a skip is the nightly's only.
expect("T11 S3 a push told covered=release", plan(event="push", **GREEN), "full", FULL_JOBS)
expect("T11 S3 a dispatch told covered=release", plan(event="workflow_dispatch", **GREEN), "full", FULL_JOBS)
expect("T11 S3 a release dispatch told covered=release", plan(event="workflow_dispatch", tier="release", **GREEN), "release", EVERY)
expect("T11 S3 a code pull request told covered=release",
       plan(["src/eval/mod.rs"], event="pull_request", action="synchronize", **GREEN), "full", FULL_JOBS)
say(plan(event="workflow_dispatch", tier="skipped")["tier"].startswith("ERROR rc=2"), "T11 S3 a dispatch asking for tier skipped is refused")

# S4: the verdict of a skipped night.
vexpect("T11 S4 a skipped night, every job skipped", run_of("skipped", []), 0)
vexpect("T11 S4 a skipped night in which a job ran", run_of("skipped", [], {"test": {"result": "success", "outputs": {}}}), 1)
vexpect("T11 S4 a skipped night whose plan chose a job", run_of("skipped", ["plan-test"]), 1)
n = run_of("skipped", [])
n["plan"]["outputs"]["covered_by"] = "https://github.com/o/r/runs/555"
rc, text = verdict(n)
say(rc == 0 and "nothing was tested" in text and "https://github.com/o/r/runs/555" in text,
    "T11 S4 the verdict of a skipped night says nothing was tested and names the covering run")

# S5: ci/covered.py against a fake gh.
FAKE_GH_COV = """#!/usr/bin/env python3
import json, os, sys
a = sys.argv[1:]
with open(os.environ["FAKE_GH_LOG"], "a", encoding="utf-8") as f:
    f.write(json.dumps(a) + "\\n")
if os.environ.get("FAKE_GH_FAIL"):
    sys.stderr.write("HTTP 502: the fake refuses\\n")
    sys.exit(1)
path = a[-1] if a[:1] == ["api"] else ""
if "/check-runs" in path:
    sys.stdout.write(os.environ["FAKE_CHECKS"])
elif "/actions/runs?" in path:
    sys.stdout.write(os.environ["FAKE_RUNS"])
elif "/actions/runs/" in path:
    sys.stdout.write(os.environ["FAKE_RUN"])
else:
    sys.stderr.write("the fake knows no %r\\n" % a)
    sys.exit(1)
"""
COV_SHA = "0e536e8f" + "0" * 32


def check(name="tier release", concl="success", done="2026-11-18T12:00:00Z", suite=1, app="github-actions",
          url="https://github.com/o/r/runs/555", status="completed"):
    return {"name": name, "status": status, "conclusion": concl, "completed_at": done, "check_suite": {"id": suite},
            "app": {"slug": app}, "html_url": url}


def wrun(suite=1, path=".github/workflows/ci.yml", sha=COV_SHA, rid=777):
    return {"id": rid, "check_suite_id": suite, "path": path, "head_sha": sha}


def covered(checks, runs=None, event="schedule", fail=False, total=None, drop=()):
    d = tempfile.mkdtemp(prefix="tiers-test-cov-")
    try:
        with open(os.path.join(d, "gh"), "w", encoding="utf-8") as f:
            f.write(FAKE_GH_COV)
        os.chmod(os.path.join(d, "gh"), 0o755)
        log = os.path.join(d, "calls.jsonl")
        runs = [wrun()] if runs is None else runs
        env = clean_env(PATH=d + os.pathsep + os.environ.get("PATH", ""), FAKE_GH_LOG=log, REPO="o/r", SHA=COV_SHA,
                        TIER=NIGHTLY_TIER, RUN_ID="999", EVENT=event, GITHUB_EVENT_NAME="schedule",
                        FAKE_RUN=json.dumps({"id": 999, "check_suite_id": 9, "created_at": "2026-11-20T12:00:00Z"}),
                        FAKE_CHECKS=json.dumps({"total_count": len(checks) if total is None else total, "check_runs": checks}),
                        FAKE_RUNS=json.dumps({"total_count": len(runs), "workflow_runs": runs}))
        for k in drop:
            env.pop(k, None)
        if fail:
            env["FAKE_GH_FAIL"] = "1"
        r = subprocess.run([sys.executable, COVERED_PY], capture_output=True, text=True, env=env)
        got = dict(l.split("=", 1) for l in r.stdout.splitlines() if "=" in l)
        calls = [json.loads(x) for x in open(log, encoding="utf-8")] if os.path.exists(log) else []
        return r.returncode, got, calls, r.stderr
    finally:
        shutil.rmtree(d, ignore_errors=True)


def cexpect(name, res, want):
    rc, got, calls, err = res
    ok = rc == 0 and got.get("covered") == want and bool(got.get("reason"))
    say(ok, "T11 S5 %s: covered=%s%s" % (name, got.get("covered"), "" if ok else " (wanted %s; rc %d; %s)" % (want, rc, err.strip()[-200:])))


if os.name == "nt":
    say(True, "T11 S5 skipped: the fake gh is a script with a shebang (the plan-test job runs on ubuntu)")
else:
    res = covered([check()])
    rc, got, calls, err = res
    say(rc == 0 and got.get("covered") == "release" and got.get("covered_by") == "https://github.com/o/r/runs/555"
        and got.get("covered_at") == "2026-11-18T12:00:00Z" and got.get("covered_run") == "777"
        and any("check_name=tier%20release" in c[-1] and "filter=all" in c[-1] for c in calls),
        "T11 S5 (a) a green tier release of ci.yml at the commit, 2 days old: covered, naming the check run, its time and its run%s" % (
            "" if rc == 0 and got.get("covered") == "release" else " (%s; %s)" % (got, err.strip()[-200:])))
    cexpect("(b) the same, 7 days old", covered([check(done="2026-11-13T11:00:00Z")]), "none")
    cexpect("(b) the same, 144 hours old to the second", covered([check(done="2026-11-14T12:00:00Z")]), "release")
    for nm in ("tier full", "tier partial", "tier skipped", "tier noop", "tier releases"):
        cexpect("(c) a green %s" % nm, covered([check(name=nm)]), "none")
    cexpect("(d) a green tier release of another workflow", covered([check()], runs=[wrun(path=".github/workflows/release.yml")]), "none")
    cexpect("(d) a green tier release whose run is of another commit", covered([check()], runs=[wrun(sha="1" * 40)]), "none")
    cexpect("(d) a green tier release whose run is not at this commit at all", covered([check()], runs=[]), "none")
    for c in ("failure", "cancelled", "timed_out"):
        cexpect("(e) the tier release %s" % c, covered([check(concl=c)]), "none")
    cexpect("(e) the tier release still running", covered([check(status="in_progress", concl=None, done=None)]), "none")
    res = covered([check()], fail=True)
    cexpect("(g) gh exits non-zero", res, "none")
    say("unanswered" in res[1].get("reason", ""), "T11 S5 (g) the reason says the question went unanswered")
    cexpect("(h) a green tier release of another app", covered([check(app="some-app")]), "none")
    cexpect("(i) a full page of check runs", covered([check()], total=100), "none")
    cexpect("(i) a full page of workflow runs", covered([check()], runs=[wrun(rid=700 + i, suite=1 if i == 0 else 100 + i) for i in range(100)]), "none")
    cexpect("the latest word: a red tier release after a green one at the commit",
            covered([check(), check(concl="failure", done="2026-11-19T12:00:00Z", suite=2, url="https://github.com/o/r/runs/556")],
                    runs=[wrun(), wrun(suite=2, rid=778)]), "none")
    cexpect("the latest word: a red first attempt, then a green rerun",
            covered([check(concl="failure", done="2026-11-18T10:00:00Z"), check(done="2026-11-18T12:00:00Z")]), "release")
    cexpect("an earlier attempt of tonight's own run, green (a rerun of a green night)",
            covered([check(suite=9)], runs=[wrun(suite=9, rid=999)]), "none")
    res = covered([check()], event="push")
    cexpect("a push: never covered", res, "none")
    say(res[0] == 0 and not res[2] and "not the nightly" in res[1].get("reason", ""),
        "T11 S5 a push: covered.py asks GitHub nothing (%d calls) and says why" % len(res[2]))
    res = covered([check()], event="")
    cexpect("no EVENT: GITHUB_EVENT_NAME decides (schedule)", res, "release")
    res = covered([check()], drop=("TIER",))
    say(res[0] == 2 and "must be set" in res[3] and not res[2], "T11 S5 covered.py refuses to run without TIER, and asks nothing")

# S6: the wiring of the plan job.
pb = blocks["plan"]
cov_step = re.search(r"^      - name: what this commit already has green \(ci/covered\.py\)\n((?:        .*\n)+)", pb, re.M)
cs = cov_step.group(1) if cov_step else ""
plan_step = re.search(r"^      - id: plan\n((?:        .*\n)+)", pb, re.M)
ps = plan_step.group(1) if plan_step else ""
pperms = re.search(r"^    permissions:\n((?:      .*\n)+)", pb, re.M)
say(bool(cs) and "        id: covered\n" in cs and '        run: python3 ci/covered.py --github-output "$GITHUB_OUTPUT"\n' in cs
    and all(("          %s\n" % e) in cs for e in ("GH_TOKEN: ${{ github.token }}", "REPO: ${{ github.repository }}",
                                                   "SHA: ${{ github.sha }}", "RUN_ID: ${{ github.run_id }}", "TIER: %s" % NIGHTLY_TIER))
    and bool(ps) and pb.index("ci/covered.py") < pb.index("ci/tiers.py plan"),
    "T11 S6 the plan job asks ci/covered.py, with the nightly's tier, before it plans")
say(all(("          %s: ${{ steps.covered.outputs.%s }}\n" % (k.upper(), k)) in ps for k in ("covered", "covered_by", "covered_at", "covered_run"))
    and wf.count("steps.covered.outputs.") == 4 and "      covered_by: ${{ steps.plan.outputs.covered_by }}\n" in pb,
    "T11 S6 only the plan step reads ci/covered.py's answer, and the plan passes covered_by on to the verdict")
say(pperms is not None and sorted(pperms.group(1).split()) == sorted(["contents:", "read", "checks:", "read", "actions:", "read"]),
    "T11 S6 the plan job reads the code, the checks and the runs, and writes nothing")

# S7: the nightly's issue says a skipped night is not a green one.
if os.name != "nt":
    b = body_of(call(nightly_red(JOBS_TSV, [])[1], "issue", "create")[0])
    say("close it once a nightly is green again (a `tier skipped` night is not one)" in b,
        "T11 S7 the nightly's issue says to close it once a nightly is green again, and that a tier skipped night is not one")

print("tiers-test: %s" % ("PASS" if fails == 0 else "FAIL (%d)" % fails))
sys.exit(1 if fails else 0)
