#!/usr/bin/env python3
"""Check the manual's internal links and anchors (mdbook does not).

Standard library only; run from the repository root:

    python3 -I tools/check_doc_links.py

Checks, in docs/ and in README.md / ROADMAP.md:
- every relative link `](page.md)` names a file that exists, and every
  `](page.md#anchor)` / `](#anchor)` an anchor of that page;
- every image `![…](…)` names a file that exists;
and in the desktop app (crates/ohl-app/src): every section a window opens
with `help::heading(ui, title, "anchor")` exists on its file's `GUIDE` page.

Anchors are made as mdbook makes them: the heading's text, lowercased,
spaces to `-`, everything but letters, digits, `-` and `_` left out; a
repeated heading gets `-1`, `-2`…; `{#id}` after a heading gives `id`.
Prints each broken link with its place; exits 1 if there is any.
"""

import os
import re
import sys

LINK = re.compile(r"(!?)\[(?:[^\]\[]|\[[^\]]*\])*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*$")
EXPLICIT = re.compile(r"\{#([A-Za-z0-9_-]+)[^}]*\}\s*$")
HTML_ID = re.compile(r"<a\s+(?:name|id)=\"([^\"]+)\"")


def slug(text):
    text = re.sub(r"`([^`]*)`", r"\1", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"<[^>]+>", "", text)
    text = re.sub(r"[*_]{1,3}([^*_]+)[*_]{1,3}", r"\1", text)
    out = []
    for ch in text.strip().lower():
        if ch.isalnum() or ch in "-_":
            out.append(ch)
        elif ch.isspace():
            out.append("-")
    return "".join(out)


def anchors(path, cache={}):
    if path in cache:
        return cache[path]
    found, seen, fence = set(), {}, False
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.lstrip().startswith(("```", "~~~")):
                fence = not fence
                continue
            if fence:
                continue
            for m in HTML_ID.finditer(line):
                found.add(m.group(1))
            m = HEADING.match(line)
            if not m:
                continue
            text = m.group(2)
            e = EXPLICIT.search(text)
            if e:
                found.add(e.group(1))
                continue
            s = slug(text)
            n = seen.get(s, 0)
            seen[s] = n + 1
            found.add(s if n == 0 else f"{s}-{n}")
    cache[path] = found
    return found


def markdown_files(root):
    for d, dirs, files in os.walk(os.path.join(root, "docs")):
        dirs.sort()
        for f in sorted(files):
            if f.endswith(".md"):
                yield os.path.join(d, f)
    for f in ("README.md", "ROADMAP.md"):
        p = os.path.join(root, f)
        if os.path.exists(p):
            yield p


def check_markdown(root, problems):
    for path in markdown_files(root):
        fence = False
        with open(path, encoding="utf-8") as fh:
            for no, line in enumerate(fh, 1):
                if line.lstrip().startswith(("```", "~~~")):
                    fence = not fence
                    continue
                if fence:
                    continue
                line = re.sub(r"`[^`]*`", "", line)
                for m in LINK.finditer(line):
                    image, target = m.group(1), m.group(2)
                    if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("/"):
                        continue
                    file_part, _, anchor = target.partition("#")
                    where = f"{os.path.relpath(path, root)}:{no}"
                    dest = (os.path.normpath(os.path.join(os.path.dirname(path), file_part))
                            if file_part else path)
                    if not os.path.exists(dest):
                        problems.append(f"{where}: no file {file_part}")
                        continue
                    if anchor and not image and dest.endswith(".md"):
                        if anchor not in anchors(dest):
                            problems.append(f"{where}: no section #{anchor} in "
                                            f"{os.path.relpath(dest, root)}")


def check_app(root, problems):
    src = os.path.join(root, "crates", "ohl-app", "src")
    guide_re = re.compile(r"pub const GUIDE: &str = \"([^\"]+)\"")
    head_re = re.compile(r"heading\(\s*ui,\s*[^;]*?,\s*\"([a-z0-9_-]+)\"", re.S)
    for f in sorted(os.listdir(src)):
        if not f.endswith(".rs"):
            continue
        text = open(os.path.join(src, f), encoding="utf-8").read()
        g = guide_re.search(text)
        if not g:
            continue
        page = os.path.join(root, "docs", g.group(1) + ".md")
        if not os.path.exists(page):
            problems.append(f"crates/ohl-app/src/{f}: no page {g.group(1)}")
            continue
        for m in head_re.finditer(text):
            if m.group(1) not in anchors(page):
                line = text[:m.start()].count("\n") + 1
                problems.append(f"crates/ohl-app/src/{f}:{line}: no section "
                                f"#{m.group(1)} in docs/{g.group(1)}.md")


def main():
    root = os.getcwd()
    problems = []
    check_markdown(root, problems)
    check_app(root, problems)
    for p in problems:
        print(p)
    print(f"{len(problems)} broken link(s)" if problems else "links and anchors: all found")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
