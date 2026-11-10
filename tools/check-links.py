#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Checks the internal links of the site: every link and image in docs/ and README.md that stays
in the repository must lead to a file, and every #fragment to a heading or an id in it.

  python3 tools/check-links.py          exit 1 and a list when anything is broken

External links (a scheme, or //) are left alone: the check needs no network. Links on the landing
page are read with html.parser as a browser reads them; `manual/?p=name` means docs/manual/name.md.
docs/manual/*.md are read as the viewer reads them, with headings slugged the same way. The
engineering notes in docs/qa and docs/adr are not published, so only the site's own pages and the
README are read.
"""
from __future__ import annotations

import html
import re
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import parse_qs, unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
# A link on github.com to this repository's main branch is a link to a file here.
REPO = re.compile(r"^https://github\.com/ZahakJ/kiln/(?:blob|tree)/main/([^#?]*)(#.*)?$")
# What tools/pages-build.sh adds to docs/ in the published site.
BUILT = ("app/", "../app/")


class Page(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.links: list[str] = []
        self.ids: set[str] = set()

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        a = {k: v or "" for k, v in attrs}
        if a.get("id"):
            self.ids.add(a["id"])
        if tag == "a" and a.get("href"):
            self.links.append(a["href"])
        elif tag in ("img", "source", "script") and a.get("src"):
            self.links.append(a["src"])
        elif tag == "link" and a.get("rel") in ("icon", "preload", "stylesheet"):
            self.links.append(a.get("href", ""))
        if tag in ("img", "source") and a.get("srcset"):
            self.links += [part.split()[0] for part in a["srcset"].split(",") if part.strip()]


def parse_html(path: Path) -> Page:
    page = Page()
    page.feed(path.read_text(encoding="utf-8"))
    return page


def slug(text: str) -> str:
    """The viewer's `slug`, on a heading's plain text."""
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"[*`_]", "", html.unescape(text))
    text = re.sub(r"[^\w\s-]", "", text.lower().strip(), flags=re.UNICODE)
    return re.sub(r"\s+", "-", text)


def github_slug(text: str) -> str:
    """GitHub's anchor: as `slug`, but every space is a hyphen of its own."""
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"[*`_]", "", html.unescape(text))
    return re.sub(r"[^\w\s-]", "", text.lower().strip(), flags=re.UNICODE).replace(" ", "-")


def md_anchors(path: Path) -> set[str]:
    text = re.sub(r"```.*?```", "", path.read_text(encoding="utf-8"), flags=re.S)
    headings = [m.group(1) for m in re.finditer(r"^#{1,6}\s+(.+?)\s*#*$", text, flags=re.M)]
    return {slug(h) for h in headings} | {github_slug(h) for h in headings}


def md_links(path: Path) -> list[str]:
    text = re.sub(r"```.*?```", "", path.read_text(encoding="utf-8"), flags=re.S)
    text = re.sub(r"`[^`\n]*`", "", text)
    links = re.findall(r"!?\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)", text)
    links += re.findall(r"""(?:src|srcset|href)="([^"]+)\"""", text)
    return links


def resolve(base: Path, href: str) -> tuple[Path | None, str]:
    """The file and the fragment an href leads to, or (None, "") when it leaves the repository."""
    repo = REPO.match(href)
    if repo:
        return ROOT / unquote(repo.group(1)), (repo.group(2) or "").lstrip("#")
    parts = urlsplit(href)
    if parts.scheme or href.startswith("//"):
        return None, ""
    target = base / unquote(parts.path) if parts.path else None
    if target is None:
        return (base, parts.fragment)
    if target.is_dir():
        name = parse_qs(parts.query).get("p", [None])[0]
        if name:
            return target / f"{name}.md", parts.fragment
        # A folder of the repository, as GitHub shows it.
        target = target / "index.html" if (target / "index.html").exists() else target
    return target, parts.fragment


def check(source: Path, base: Path, links: list[str], own: set[str], broken: list[str]) -> None:
    for href in links:
        if not href or href in BUILT or href.startswith(("mailto:", "tel:", "data:")):
            continue
        if href.startswith("#"):
            if unquote(href[1:]) and unquote(href[1:]) not in own:
                broken.append(f"{source.relative_to(ROOT)}: {href}: no such anchor")
            continue
        target, fragment = resolve(base, href)
        if target is None:
            continue
        if not target.exists():
            broken.append(f"{source.relative_to(ROOT)}: {href}: no such file")
            continue
        if not fragment or not target.is_file():
            continue
        if target.suffix == ".md":
            anchors = md_anchors(target)
        elif target.suffix == ".html":
            anchors = parse_html(target).ids
        else:
            continue
        if unquote(fragment) not in anchors:
            broken.append(f"{source.relative_to(ROOT)}: {href}: no such anchor")


def main() -> int:
    broken: list[str] = []
    pages = 0
    for path in sorted(DOCS.rglob("*.html")):
        if {"qa", "adr"} & set(path.relative_to(DOCS).parts):
            continue
        page = parse_html(path)
        check(path, path.parent, page.links, page.ids, broken)
        pages += 1
    public = [ROOT / "README.md", *sorted((DOCS / "manual").glob("*.md"))]
    for path in public:
        check(path, path.parent, md_links(path), md_anchors(path), broken)
        pages += 1
    for line in broken:
        print(line)
    print(f"check-links: {pages} pages, {len(broken)} broken")
    return 1 if broken else 0


if __name__ == "__main__":
    sys.exit(main())
