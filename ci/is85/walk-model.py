#!/usr/bin/env python3
"""ci/is85/walk-model.py [HEAD...] — run_corpus's wall time, today and shared (is85).

Replays one measured walk (`local-walk-HEAD.tsv`: each entry's seconds, in
walk order) on a four-core runner, in units of T1, one walk on one thread
alone. Processor sharing: each running walker gets min(1, cores/running)
of a core, and when the cores are full every walker is also slowed by the
contention factor C (1.46 measured on windows: four concurrent walks took
5,520-5,539 s where one alone took 3,777, run 42621872032).

  today     every walking test takes its own walk: twelve walks (seven
            entries() tests, three single walks, the determinism test's
            two), one thread each, four threads
  shared    one shared walk on four workers, then the determinism test's
            fresh walk on four more, one after the other
  overlap   the same two walks at once (the determinism test starts its
            own before it reads the shared one)
"""
import csv
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CORES = 4


def load(head):
    with open(os.path.join(HERE, "local-walk-%s.tsv" % head)) as fh:
        rows = [r for r in csv.reader(fh, delimiter="\t") if r and r[0] != "path" and not r[0].startswith("#")]
    return [int(r[2]) / 1e6 for r in rows if r[1] == "true"]


def replay(queues, per_queue, c):
    queues = [list(q) for q in queues]
    nxt = [0] * len(queues)
    running = []
    for qi, q in enumerate(queues):
        for _ in range(per_queue):
            if nxt[qi] < len(q):
                running.append([q[nxt[qi]], qi])
                nxt[qi] += 1
    t = 0.0
    while running:
        n = len(running)
        rate = min(1.0, CORES / n) / (c if n >= CORES else 1 + (c - 1) * n / CORES)
        dt = min(r[0] for r in running) / rate
        t += dt
        for r in running:
            r[0] -= dt * rate
        for r in [r for r in running if r[0] <= 1e-9]:
            running.remove(r)
            qi = r[1]
            if nxt[qi] < len(queues[qi]):
                running.append([queues[qi][nxt[qi]], qi])
                nxt[qi] += 1
    return t


heads = sys.argv[1:] or ["6240efe5", "ed711840"]
print("head\tentries\tT1_s\tC\ttoday_T1\tshared_T1\toverlap_T1")
for head in heads:
    walk = load(head)
    t1 = sum(walk)
    for c in (1.0, 1.46):
        today = replay([walk] * 12, 1, c)
        shared = 2 * replay([walk], CORES, c)
        overlap = replay([walk, walk], CORES, c)
        print("%s\t%d\t%.0f\t%.2f\t%.2f\t%.2f\t%.2f" % (head, len(walk), t1, c, today / t1, shared / t1, overlap / t1))
