# s288: every wolf program embedded in this repository's Rust sources (tests/
# and src/), written out one per directory, so two lupin builds can judge each
# at the resolve rung. A program is a raw string (r"…", r#"…"#, r##"…"##) or a
# plain string literal, unescaped, that contains `fn `. A format string's
# doubled braces stay doubled: such a program fails to parse on both builds
# alike, so it never shows as a move.
#   python3 -I sweep.py OUTDIR ROOT
import os, re, sys
out, root = sys.argv[1], sys.argv[2]
raw = re.compile(r'r(#*)"(.*?)"\1', re.S)
plain = re.compile(r'(?<![A-Za-z0-9_#])"((?:[^"\\]|\\.)*)"', re.S)
def unescape(s):
    return re.sub(r'\\(n|t|r|"|\\|0|\n\s*)', lambda m: {'n': '\n', 't': '\t', 'r': '\r', '"': '"', '\\': '\\', '0': '\0'}.get(m.group(1), ''), s)
os.makedirs(out, exist_ok=True)
index = open(os.path.join(out, 'index.tsv'), 'w')
n = 0
for base in ('tests', 'src'):
    for dirpath, dirs, files in os.walk(os.path.join(root, base)):
        dirs.sort()
        for name in sorted(files):
            if not name.endswith('.rs'):
                continue
            path = os.path.join(dirpath, name)
            text = open(path, encoding='utf-8', errors='replace').read()
            found = []
            spans = []
            for m in raw.finditer(text):
                found.append((m.start(), m.group(2)))
                spans.append((m.start(), m.end()))
            for m in plain.finditer(text):
                if any(a <= m.start() < b for a, b in spans):
                    continue
                found.append((m.start(), unescape(m.group(1))))
            for at, body in sorted(found):
                if 'fn ' not in body or '{' not in body:
                    continue
                n += 1
                d = os.path.join(out, '%05d' % n)
                os.makedirs(d, exist_ok=True)
                open(os.path.join(d, 'main.lu'), 'w').write(body)
                line = text.count('\n', 0, at) + 1
                index.write('%05d\t%s:%d\n' % (n, os.path.relpath(path, root), line))
print('programs', n)
