#!/usr/bin/env python3
"""ci/nightly-red.py — a red nightly opens an issue.

    ci/nightly-red.py        (REPO, RUN_ID, SHA, TIER; GH_TOKEN for `gh`)

The nightly's issue job (`nightly-red`, after the `tier` job) runs this when the
scheduled run is red (`failure() && github.event_name == 'schedule'`). The windows
heavy shards and ladder run only there and at the release tier, so a red nightly
is the one place a windows regression shows before a tag, and it must not wait
for someone to read the Actions tab.

It names the run's red jobs (from the run's own job list: a conclusion of
failure, cancelled, timed_out or startup_failure), then, DEDUPED ON THE TITLE,
opens one issue labelled `nightly-red` or, when an open issue already has that
title, comments on it. One title for every red nightly: a second red night adds
to the open issue rather than starting another, and the issue is to be looked
at before the next merge. Everything it writes is public (ci/tiers-test.py T8
refuses an internal word in it).

`gh` is whatever `gh` is on PATH (ci/tiers-test.py T8 puts a recording fake
there). Exit 0 when the issue was opened or commented on; 1 when `gh` failed,
loudly: a reporter that fails quietly is a red nobody hears.
"""
import json
import os
import subprocess
import sys

TITLE = "nightly red: tier release on trunk"
LABEL = "nightly-red"
RED = ("failure", "cancelled", "timed_out", "startup_failure")


def gh(*args):
    r = subprocess.run(["gh"] + list(args), capture_output=True, text=True)
    if r.returncode != 0:
        print("nightly-red: gh %s failed (%d): %s" % (" ".join(args[:2]), r.returncode, r.stderr.strip()), file=sys.stderr)
        sys.exit(1)
    return r.stdout


def main():
    env = os.environ
    try:
        repo, run_id = env["REPO"], env["RUN_ID"]
    except KeyError as e:
        print("nightly-red: %s is not set" % e, file=sys.stderr)
        return 1
    sha = env.get("SHA", "")[:9] or "?"
    tier = env.get("TIER") or "unplanned"
    server = env.get("GITHUB_SERVER_URL", "https://github.com")
    url = "%s/%s/actions/runs/%s" % (server, repo, run_id)

    jobs = []
    for line in gh("api", "--paginate", "repos/%s/actions/runs/%s/jobs?per_page=100" % (repo, run_id),
                   "-q", ".jobs[] | [.name, (.conclusion // \"\"), .html_url] | @tsv").splitlines():
        parts = line.split("\t")
        if len(parts) == 3:
            jobs.append(parts)
    red = [(n, c, u) for n, c, u in jobs if c in RED]
    if red:
        named = "\n".join("- `%s`: %s (%s)" % (n, c, u) for n, c, u in red)
    else:
        named = "- no job is red by itself: the `tier %s` verdict is (a job ran unasked, or the plan failed); read its log" % tier

    body = (
        "The nightly (`tier %s`) on trunk at `%s` is red: %s\n\n"
        "The red job(s):\n%s\n\n"
        "The windows heavy shards and the ladder run only in the nightly and at the release tier, so this may "
        "be the first sight of a windows regression. It is to be looked at before the next merge.\n\n"
        "Opened by `ci/nightly-red.py` from the nightly's issue job; a later red nightly comments here "
        "while this stays open: close it once a nightly is green again (a `tier skipped` night is not one)."
    ) % (tier, sha, url, named)

    issues = json.loads(gh("issue", "list", "-R", repo, "--state", "open", "--label", LABEL,
                           "--limit", "100", "--json", "number,title") or "[]")
    same = [i["number"] for i in issues if i.get("title") == TITLE]
    if same:
        gh("issue", "comment", str(same[0]), "-R", repo, "--body", body)
        print("nightly-red: commented on #%d (%s)" % (same[0], TITLE))
        return 0
    gh("label", "create", LABEL, "-R", repo, "--force", "--color", "B60205",
       "--description", "the nightly (tier release) on trunk is red; to be looked at before the next merge")
    out = gh("issue", "create", "-R", repo, "--title", TITLE, "--label", LABEL, "--body", body)
    print("nightly-red: opened %s" % out.strip())
    return 0


if __name__ == "__main__":
    sys.exit(main())
