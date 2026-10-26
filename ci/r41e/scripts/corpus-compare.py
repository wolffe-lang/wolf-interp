# r41e: compare two `lupin corpus --json` outputs, ignoring the version and
# commit fields; print the shape, the counts and every differing entry.
import json, sys
def load(p):
    with open(p) as f:
        text = f.read()
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return [json.loads(l) for l in text.splitlines() if l.strip()]
a, b = load(sys.argv[1]), load(sys.argv[2])
def scrub(x):
    if isinstance(x, dict):
        return {k: scrub(v) for k, v in x.items() if k not in ("impl_version", "commit", "version")}
    if isinstance(x, list):
        return [scrub(v) for v in x]
    return x
sa, sb = scrub(a), scrub(b)
def describe(x):
    if isinstance(x, dict):
        return "object keys=" + ",".join(sorted(x)[:20])
    if isinstance(x, list):
        return "list len=%d" % len(x)
    return type(x).__name__
print("old:", describe(a)); print("new:", describe(b))
if isinstance(sa, dict):
    for k in sorted(set(sa) | set(sb)):
        va, vb = sa.get(k), sb.get(k)
        if not isinstance(va, (list, dict)):
            print("  %s: %r -> %r" % (k, va, vb))
if sa == sb:
    print("CORPUS_IDENTICAL (version and commit fields aside)")
    sys.exit(0)
def walk(x, y, path, out):
    if type(x) != type(y):
        out.append((path, x, y)); return
    if isinstance(x, dict):
        for k in sorted(set(x) | set(y)):
            walk(x.get(k), y.get(k), path + "." + str(k), out)
    elif isinstance(x, list) and len(x) == len(y):
        for i, (u, v) in enumerate(zip(x, y)):
            walk(u, v, "%s[%d]" % (path, i), out)
    elif x != y:
        out.append((path, x, y))
out = []
walk(sa, sb, "$", out)
print("CORPUS_DIFFERS: %d paths" % len(out))
for p, x, y in out[:60]:
    print("  %s: %s -> %s" % (p, json.dumps(x)[:200], json.dumps(y)[:200]))
sys.exit(1)
