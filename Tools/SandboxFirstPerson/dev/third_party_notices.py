#!/usr/bin/env python3
"""Write THIRD_PARTY_NOTICES.txt for a Rust app from its Cargo dependency list.

Usage: third_party_notices.py <cargo project dir> <app name> <output file>

Covers the crates compiled into the shipped builds (macOS arm64 and Windows
x86_64-gnu): normal dependencies reachable from the app, not dev- or
build-only ones. Licence texts come from each crate's own files in the local
Cargo registry (cargo metadata --offline). Where a crate offers a choice
("MIT OR Apache-2.0"), Apache-2.0 is used, then MIT, so most crates share one
text. Anything unresolved is printed to stderr and the script exits 1.
"""
import json, os, re, subprocess, sys

TARGETS = ["aarch64-apple-darwin", "x86_64-pc-windows-gnu"]
PREFER = ["Apache-2.0", "MIT", "Zlib", "BSD-3-Clause", "BSD-2-Clause", "ISC",
          "Unicode-3.0", "Unicode-DFS-2016", "BSL-1.0", "MPL-2.0", "CC0-1.0", "Unlicense"]
LICENCE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE|COPYRIGHT|OFL|UFL)", re.I)

def metadata(proj, target):
    out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--offline",
                          "--filter-platform", target], cwd=proj, check=True,
                         capture_output=True, text=True).stdout
    return json.loads(out)

def shipped(m):
    nodes = {n["id"]: n for n in m["resolve"]["nodes"]}
    root = m["resolve"]["root"]
    seen, todo = set(), [root]
    while todo:
        nid = todo.pop()
        for d in nodes[nid]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]) and d["pkg"] not in seen:
                seen.add(d["pkg"]); todo.append(d["pkg"])
    return seen, root

def tokens(expr):
    return re.findall(r"[A-Za-z0-9.\-+]+|\(|\)", expr.replace("/", " OR "))

def choose(expr):
    """Licence ids to honour for an SPDX expression: AND keeps all, OR picks one."""
    def parse_or(t):
        alts = [parse_and(t)]
        while t and t[0] == "OR":
            t.pop(0); alts.append(parse_and(t))
        if len(alts) == 1:
            return alts[0]
        for p in PREFER:
            for a in alts:
                if a == [p]:
                    return a
        return min(alts, key=len)
    def parse_and(t):
        parts = parse_atom(t)
        while t and t[0] == "AND":
            t.pop(0); parts = parts + parse_atom(t)
        return parts
    def parse_atom(t):
        tok = t.pop(0)
        if tok == "(":
            r = parse_or(t); t.pop(0); return r
        if t and t[0] == "WITH":
            t.pop(0); exc = t.pop(0); return [tok + " WITH " + exc]
        return [tok]
    return parse_or(tokens(expr))

def licence_files(pkg_dir):
    found = []
    for dirpath, dirs, files in os.walk(pkg_dir):
        depth = dirpath[len(pkg_dir):].count(os.sep)
        if depth >= 2:
            dirs[:] = []
        dirs[:] = [d for d in dirs if d not in ("target", ".git", "tests", "benches", "examples")]
        for f in files:
            if LICENCE_FILE.match(f):
                found.append(os.path.join(dirpath, f))
    return sorted(found)

def matches(path, lid):
    n = os.path.basename(path).upper()
    key = {"Apache-2.0": "APACHE", "MIT": "MIT", "Zlib": "ZLIB", "BSL-1.0": "BOOST",
           "Unicode-3.0": "UNICODE", "Unicode-DFS-2016": "UNICODE", "OFL-1.1": "OFL",
           "LicenseRef-UFL-1.0": "UFL", "Ubuntu-font-1.0": "UFL", "ISC": "ISC", "MPL-2.0": "MPL", "CC0-1.0": "CC0",
           "BSD-3-Clause": "BSD", "BSD-2-Clause": "BSD", "Unlicense": "UNLICENSE"}.get(lid.split(" ")[0])
    return key is not None and key in n

MIT_TEXT = """Copyright (c) <HOLDERS>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""

BSL_TEXT = """Copyright (c) <HOLDERS>

Boost Software License - Version 1.0 - August 17th, 2003

Permission is hereby granted, free of charge, to any person or organization
obtaining a copy of the software and accompanying documentation covered by
this license (the "Software") to use, reproduce, display, distribute,
execute, and transmit the Software, and to prepare derivative works of the
Software, and to permit third-parties to whom the Software is furnished to
do so, all subject to the following:

The copyright notices in the Software and this entire statement, including
the above license grant, this restriction and the following disclaimer,
must be included in all copies of the Software, in whole or in part, and
all derivative works of the Software, unless such copies or derivative
works are solely in the form of machine-executable object code generated by
a source language processor.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE, TITLE AND NON-INFRINGEMENT. IN NO EVENT
SHALL THE COPYRIGHT HOLDERS OR ANYONE DISTRIBUTING THE SOFTWARE BE LIABLE
FOR ANY DAMAGES OR OTHER LIABILITY, WHETHER IN CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
"""

TEMPLATES = {"MIT": MIT_TEXT, "BSL-1.0": BSL_TEXT}

def holders(p):
    names = [re.sub(r"\s*<[^>]*>", "", a).strip() for a in p.get("authors") or []]
    names = [n for n in names if n]
    if names:
        return ", ".join(names)
    repo = p.get("repository") or ""
    return "the %s authors%s" % (p["name"], " (" + repo + ")" if repo else "")

def read(p):
    with open(p, encoding="utf-8", errors="replace") as fh:
        return fh.read().strip("\n") + "\n"

def norm(text):
    return re.sub(r"\s+", " ", text).strip().lower()

def main():
    proj, app, out = sys.argv[1], sys.argv[2], sys.argv[3]
    pkgs, problems = {}, []
    root_name = None
    for t in TARGETS:
        m = metadata(proj, t)
        ids, root = shipped(m)
        byid = {p["id"]: p for p in m["packages"]}
        root_name = byid[root]["name"]
        for i in ids:
            p = byid[i]
            pkgs[(p["name"], p["version"])] = p

    apache_text = None
    groups = {}          # normalised text -> {"text", "ids", "crates"}
    notices = []         # (crate, NOTICE text)
    rows = []
    for (name, ver), p in sorted(pkgs.items()):
        d = os.path.dirname(p["manifest_path"])
        expr = p.get("license") or ""
        files = licence_files(d)
        if p.get("license_file"):
            lf = os.path.join(d, p["license_file"])
            if lf not in files:
                files.append(lf)
        ids = choose(expr) if expr else ["(see licence file)"]
        rows.append((name, ver, expr or "see licence file", " AND ".join(ids)))
        for n in files:
            if os.path.basename(n).upper().startswith("NOTICE"):
                notices.append(("%s %s" % (name, ver), read(n)))
        for lid in ids:
            if lid == "Apache-2.0":
                f = [x for x in files if matches(x, lid)]
                if apache_text is None and f:
                    apache_text = read(f[0])
                groups.setdefault("APACHE", {"text": None, "ids": "Apache-2.0", "crates": []})["crates"].append("%s %s" % (name, ver))
                continue
            f = [x for x in files if matches(x, lid)]
            if not f:
                # one licence and one plain LICENSE/COPYING file: that's the text
                plain = [x for x in files if not os.path.basename(x).upper().startswith("NOTICE")]
                if len(ids) == 1 and plain:
                    f = plain
            if not f and lid in TEMPLATES:
                # The crate names the licence but ships no text: use the
                # standard text, with the copyright from its listed authors.
                text = TEMPLATES[lid].replace("<HOLDERS>", holders(p))
                g = groups.setdefault(norm(text), {"text": text, "ids": lid + " (standard text; the crate ships none)", "crates": []})
                g["crates"].append("%s %s" % (name, ver))
                continue
            if not f:
                problems.append("%s %s: no file for %s (licence %r; files: %s)"
                                % (name, ver, lid, expr, [os.path.relpath(x, d) for x in files]))
                continue
            for x in f:
                text = read(x)
                g = groups.setdefault(norm(text), {"text": text, "ids": lid, "crates": []})
                g["crates"].append("%s %s" % (name, ver))
    if "APACHE" in groups:
        if apache_text is None:
            problems.append("no Apache-2.0 licence text found in any crate")
        groups["APACHE"]["text"] = apache_text

    lines = []
    w = lines.append
    w("THIRD-PARTY SOFTWARE NOTICES")
    w("")
    w("%s (%s) is built with the open-source Rust crates listed below." % (app, root_name))
    w("Their licences ask that these notices go with the program. The program's")
    w("own licence is in LICENSE, at the top of the scripts-and-tools folder.")
    w("")
    w("Where a crate is offered under a choice of licences, it is used here under")
    w("the one named in the \"Used under\" column.")
    w("")
    w("Generated from Cargo.lock for the macOS (arm64) and Windows (x64) builds.")
    w("")
    w("-" * 78)
    w("CRATES (%d)" % len(rows))
    w("-" * 78)
    nw = max(len("%s %s" % (r[0], r[1])) for r in rows)
    w("%-*s  %s" % (nw, "Crate", "Used under (offered as)"))
    for name, ver, expr, used in rows:
        w("%-*s  %s%s" % (nw, "%s %s" % (name, ver), used, "" if used == expr else "  (" + expr + ")"))
    w("")
    order = sorted(groups.values(), key=lambda g: (g["ids"] != "Apache-2.0", g["ids"], g["crates"][0]))
    for g in order:
        w("=" * 78)
        w("%s, used by:" % g["ids"])
        crates = g["crates"]
        line = " "
        for c in crates:
            if len(line) + len(c) + 2 > 78:
                w(line.rstrip()); line = " "
            line += " " + c + ","
        w(line.rstrip(","))
        w("=" * 78)
        w("")
        w(g["text"] or "")
    if notices:
        w("=" * 78)
        w("NOTICE FILES")
        w("=" * 78)
        for c, t in notices:
            w("")
            w("--- " + c + " ---")
            w(t)
    with open(out, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines).rstrip("\n") + "\n")
    print("%s: %d crates, %d licence texts, %d NOTICE files -> %s"
          % (app, len(rows), len(groups), len(notices), out))
    for p in problems:
        print("UNRESOLVED " + p, file=sys.stderr)
    sys.exit(1 if problems else 0)

if __name__ == "__main__":
    main()
