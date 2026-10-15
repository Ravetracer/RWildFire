#!/usr/bin/env python3
# RWildFire - a fractal flame editor written in Rust
# Copyright (C) 2026 Christian Nielebock
# SPDX-License-Identifier: LGPL-3.0-or-later
"""Builds the user manual: docs/manual/*.md -> HTML -> PDF.

Requirements: Python 3 with the `markdown` package, Google Chrome or Chromium (headless PDF
printing) and pdftotext (poppler-utils, for the page numbers of the table of contents).

    python3 tools/build_manual.py            # -> docs/manual/build/RWildFire-Manual-<version>.pdf
    python3 tools/build_manual.py --html     # only the HTML (docs/manual/build/manual.html)

Source conventions:
- One file per chapter, sorted by name; `# Title` starts a chapter (numbered 1, 2, …),
  `##` and `###` are numbered sections (1.2, 1.2.3).
- `![Caption](images/name.png)` alone in a paragraph becomes a numbered figure; add
  `{: .half}` or `{: .third}` after the image for smaller figures, `{: .inline}` for small
  pictures inside the text (no caption).
- `!!! tip` / `!!! note` / `!!! warning` blocks (Python-Markdown admonitions).
"""

import argparse
import datetime
import os
import html
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import markdown

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "docs" / "manual"
OUT = SRC / "build"


def version() -> str:
    text = (ROOT / "Cargo.toml").read_text()
    m = re.search(r'\[workspace\.package\][^\[]*?version\s*=\s*"([^"]+)"', text, re.S)
    return m.group(1) if m else "unknown"


def chrome() -> str:
    for name in ("google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "chrome"):
        p = shutil.which(name)
        if p:
            return p
    sys.exit("Chrome or Chromium is needed to print the PDF")


def slug(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def api_reference() -> str:
    """The scripting reference (crates/rwf-script/API.md) as sections of the scripting chapter:
    its title is dropped and its sections are prefixed with "Reference:"."""
    text = (ROOT / "crates" / "rwf-script" / "API.md").read_text(encoding="utf-8")
    lines = text.splitlines()
    if lines and lines[0].startswith("# "):
        lines = lines[1:]
    out = []
    for line in lines:
        if line.startswith("## "):
            line = "## Reference: " + line[3:]
        out.append(line)
    return "\n".join(out)


def convert(md_text: str) -> str:
    return markdown.markdown(
        md_text,
        extensions=["extra", "admonition", "sane_lists"],
        output_format="html5",
    )


def number_and_collect(body: str):
    """Numbers h1-h3, gives them ids and returns (html, headings)."""
    counters = [0, 0, 0]
    headings = []

    def repl(m):
        level = int(m.group(1))
        inner = m.group(3)
        counters[level - 1] += 1
        for i in range(level, 3):
            counters[i] = 0
        num = ".".join(str(c) for c in counters[:level])
        text = re.sub(r"<[^>]+>", "", inner)
        hid = f"s{num.replace('.', '-')}-{slug(text)}"
        headings.append((level, num, html.unescape(text), hid))
        cls = ' class="chapter"' if level == 1 else ""
        label = f"Chapter {num}" if level == 1 else ""
        pre = f'<div class="chapter-label">{label}</div>' if label else ""
        return f'{pre}<h{level} id="{hid}"{cls}><span class="num">{num}</span> {inner}</h{level}>'

    body = re.sub(r"<h([1-3])([^>]*)>(.*?)</h\1>", repl, body, flags=re.S)
    return body, headings


def figures(body: str) -> str:
    """Paragraphs that hold only an image become numbered figures."""
    count = [0]

    def repl(m):
        img = m.group(1)
        if 'class="inline"' in img:
            return m.group(0)
        count[0] += 1
        alt = re.search(r'alt="([^"]*)"', img)
        cap = alt.group(1) if alt else ""
        size = re.search(r'class="([^"]*)"', img)
        cls = f" {size.group(1)}" if size else ""
        return f'<figure class="fig{cls}">{img}<figcaption><b>Figure {count[0]}</b> {cap}</figcaption></figure>'

    return re.sub(r"<p>\s*(<img [^>]*>)\s*</p>", repl, body)


def toc_html(headings, pages) -> str:
    rows = []
    for level, num, text, hid in headings:
        if level > 2:
            continue
        page = pages.get(hid, "")
        rows.append(
            f'<li class="toc{level}"><a href="#{hid}"><span class="tnum">{num}</span>'
            f'<span class="ttext">{html.escape(text)}</span><span class="tdots"></span>'
            f'<span class="tpage">{page}</span></a></li>'
        )
    return '<nav class="toc"><h1 class="tochead">Contents</h1><ul>' + "\n".join(rows) + "</ul></nav>"


def page_document(body, headings, pages, ver) -> str:
    css = (SRC / "manual.css").read_text()
    date = datetime.date.today().strftime("%B %Y")
    title = f"""
<section class="cover">
  <img class="cover-image" src="images/cover.jpg" alt="">
  <div class="cover-text">
    <div class="product">RWildFire</div>
    <div class="subtitle">User Manual</div>
    <div class="version">Version {ver}</div>
    <div class="date">{date}</div>
  </div>
</section>
"""
    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<title>RWildFire {ver} – User Manual</title>
<style>{css}
@page {{ @bottom-left {{ content: "RWildFire {ver} – User Manual"; }} }}
</style></head>
<body>{title}{toc_html(headings, pages)}<main>{body}</main></body></html>"""


def print_pdf(html_path: Path, pdf_path: Path):
    with tempfile.TemporaryDirectory() as profile:
        cmd = [
            chrome(), "--headless=new", "--disable-gpu", "--no-first-run", f"--user-data-dir={profile}",
            *(["--no-sandbox"] if os.environ.get("CI") else []),
            "--no-pdf-header-footer", "--generate-pdf-document-outline", "--virtual-time-budget=20000",
            f"--print-to-pdf={pdf_path}", html_path.as_uri(),
        ]
        r = subprocess.run(cmd, capture_output=True, text=True)
        if not pdf_path.exists():
            sys.exit(f"Chrome failed:\n{r.stderr}")


def heading_pages(pdf: Path, headings) -> dict:
    """Page of every heading, found by its number and text in the PDF's text."""
    text = subprocess.run(["pdftotext", "-layout", str(pdf), "-"], capture_output=True, text=True).stdout
    pages_text = text.split("\f")
    norm = lambda s: re.sub(r"\s+", " ", s).strip().lower()
    pages_norm = [norm(p) for p in pages_text]
    result = {}
    # The contents pages list every heading too: search after them, in order.
    start = 0
    for i, p in enumerate(pages_norm):
        if p.startswith("contents") or " contents " in p[:40]:
            start = i + 1
    for level, num, text_, hid in headings:
        needle = norm(f"{num} {text_}")
        for i in range(start, len(pages_norm)):
            if needle in pages_norm[i]:
                result[hid] = i + 1
                start = i
                break
    return result


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--html", action="store_true", help="only write the HTML")
    args = ap.parse_args()
    ver = version()
    chapters = sorted(p for p in SRC.glob("*.md") if p.name[0].isdigit())
    md_text = "\n\n".join(p.read_text(encoding="utf-8") for p in chapters)
    md_text = md_text.replace("{{version}}", ver)
    md_text = md_text.replace("{{api}}", api_reference())
    body, headings = number_and_collect(convert(md_text))
    body = figures(body)
    OUT.mkdir(parents=True, exist_ok=True)
    html_path = SRC / "manual.html"  # next to images/
    placeholder = {h[3]: "000" for h in headings}
    html_path.write_text(page_document(body, headings, placeholder, ver), encoding="utf-8")
    if args.html:
        shutil.copy(html_path, OUT / "manual.html")
        print(OUT / "manual.html")
        return
    pdf = OUT / f"RWildFire-Manual-{ver}.pdf"
    tmp = OUT / "pass1.pdf"
    print_pdf(html_path, tmp)
    pages = heading_pages(tmp, headings)
    missing = [h[2] for h in headings if h[3] not in pages and h[0] <= 2]
    if missing:
        print(f"warning: no page found for {len(missing)} headings: {missing[:5]}", file=sys.stderr)
    html_path.write_text(page_document(body, headings, pages, ver), encoding="utf-8")
    print_pdf(html_path, pdf)
    tmp.unlink()
    html_path.unlink()
    info = subprocess.run(["pdfinfo", str(pdf)], capture_output=True, text=True).stdout
    n = re.search(r"Pages:\s+(\d+)", info)
    print(f"{pdf} ({n.group(1) if n else '?'} pages, {pdf.stat().st_size / 1e6:.1f} MB)")


if __name__ == "__main__":
    main()
