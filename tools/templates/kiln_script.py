# SPDX-License-Identifier: AGPL-3.0-or-later
"""Helpers that write `kiln-cli control` scripts (JSON lines) for the bundled templates.

Each template is a list of control requests: a new document, the commands that fill it, and a save
to its native bundle under assets/templates/. The helpers only build JSON; nothing here talks to the
core. `make.py` collects the templates and writes tools/templates/<app>-<name>.jsonl.
"""

from __future__ import annotations

import json
from typing import Any

FIND_DEFAULTS = {"match_case": True, "whole_word": False, "wildcards": False, "regex": False}

# Stands in for a tab character while Markdown is pasted.
TAB_MARK = "\u00a6"

# A4 in points.
A4_W, A4_H = 595.3, 841.9


def font(**kw: Any) -> dict:
    """A FontFormat: only the given fields change."""
    d: dict = {"underline_none": False}
    d.update(kw)
    return d


class Script:
    """A control script for one document (always document 1 of a fresh channel)."""

    def __init__(self, kind: str, app: str, name: str, ext: str, fmt: str) -> None:
        self.app = app
        self.name = name
        self.path = f"assets/templates/{app}/{name}.{ext}"
        self.save_format = fmt
        self.lines: list[dict] = [{"new": {"kind": kind}}]

    def cmd(self, command: dict) -> "Script":
        self.lines.append({"execute": {"doc": 1, "cmd": command}})
        return self

    def session(self, command: dict) -> "Script":
        self.lines.append({"execute": {"cmd": command}})
        return self

    def query(self, q: Any) -> "Script":
        self.lines.append({"query": {"doc": 1, "q": q}})
        return self

    def jsonl(self, title: str | None = None) -> str:
        """The script: the commands, then the save. A bundle records the document's title, which
        is its file name once saved: with `title`, the document is first saved under that name
        (in `target/`), so the bundle carries it."""
        saves = []
        if title:
            ext = self.path.rsplit(".", 1)[1]
            saves.append({"save": {"doc": 1, "path": f"target/templates-tmp/{title}.{ext}",
                                   "format": self.save_format}})
        saves.append({"save": {"doc": 1, "path": self.path, "format": self.save_format}})
        lines = self.lines + saves
        return "".join(json.dumps(line, ensure_ascii=False, separators=(",", ":")) + "\n" for line in lines)


# ── Kiln Write ────────────────────────────────────────────────────────────────────────────────


class Write(Script):
    def __init__(self, name: str) -> None:
        super().__init__("write", "write", name, "kwrite", "KWrite")

    def w(self, name: str, args: Any = None) -> "Write":
        self.cmd({"Write": name if args is None else {name: args}})
        return self

    def e(self, name: str, args: Any = None) -> "Write":
        self.cmd({"Suite": {"Edit": name if args is None else {name: args}}})
        return self

    # Document setup
    def a4(self, top: float = 72, bottom: float = 72, left: float = 72, right: float = 72,
           header: float = 36, footer: float = 36) -> "Write":
        return self.w("PageSetup", {"setup": {"width": A4_W, "height": A4_H, "top": top, "bottom": bottom,
                                               "left": left, "right": right, "header": header,
                                               "footer": footer}})

    def style(self, style_id: str, *, f: dict | None = None, p: dict | None = None, **spec: Any) -> "Write":
        body = {"font": font(**(f or {})), "paragraph": p or {}}
        body.update(spec)
        return self.w("ModifyStyle", {"style_id": style_id, "spec": body})

    def props(self, **kw: Any) -> "Write":
        return self.w("SetProperties", {"props": kw})

    # Content
    def md(self, text: str) -> "Write":
        """Pastes Markdown. Tab characters (which Markdown turns into spaces) are kept: they go in
        as a placeholder that is replaced with real tabs afterwards."""
        if "\t" not in text:
            return self.w("PasteMarkdown", {"text": text})
        self.w("PasteMarkdown", {"text": text.replace("\t", TAB_MARK)})
        q = dict(FIND_DEFAULTS)
        q["text"] = TAB_MARK
        return self.w("ReplaceAll", {"query": q, "replacement": "^t"})

    def text(self, text: str) -> "Write":
        return self.w("InsertText", {"text": text})

    def replace_all(self, text: str, replacement: str, regex: bool = False) -> "Write":
        q = dict(FIND_DEFAULTS)
        q["text"] = text
        q["regex"] = regex
        return self.w("ReplaceAll", {"query": q, "replacement": replacement})

    # Selection
    def start(self) -> "Write":
        return self.w("MoveCaret", {"motion": "DocumentStart", "extend": False})

    def end(self) -> "Write":
        return self.w("MoveCaret", {"motion": "DocumentEnd", "extend": False})

    def find(self, text: str, *, from_start: bool = True) -> "Write":
        if from_start:
            self.start()
        q = dict(FIND_DEFAULTS)
        q["text"] = text
        return self.w("FindNext", {"query": q, "backward": False})

    def sel_para(self, text: str, count: int = 1) -> "Write":
        """Selects the paragraph holding `text` (and the `count - 1` after it), not the next one."""
        self.find(text)
        self.w("MoveCaret", {"motion": "LineStart", "extend": False})
        for _ in range(count):
            self.w("MoveCaret", {"motion": "ParagraphForward", "extend": True})
        return self.w("MoveCaret", {"motion": "Backward", "extend": True})

    def sel_to_end(self, text: str) -> "Write":
        """Selects from the start of the paragraph holding `text` to the end of the document."""
        self.find(text)
        self.w("MoveCaret", {"motion": "LineStart", "extend": False})
        return self.w("MoveCaret", {"motion": "DocumentEnd", "extend": True})

    # Formatting of the selection
    def f(self, **kw: Any) -> "Write":
        return self.w("SetFont", {"format": font(**kw)})

    def p(self, **kw: Any) -> "Write":
        return self.w("SetParagraph", {"format": kw})

    def apply(self, style_id: str) -> "Write":
        return self.w("ApplyStyle", {"style_id": style_id})

    def shade(self, color: str | None) -> "Write":
        return self.w("ParagraphShading", {"color": color})

    def borders(self, preset: str) -> "Write":
        return self.w("ParagraphBorders", {"preset": preset})

    def tabs(self, *stops: tuple) -> "Write":
        return self.w("SetTabs", {"tabs": [{"pos": pos, "align": align, "leader": leader}
                                           for pos, align, leader in stops]})

    # Tables (caret inside the table)
    def table_style(self, style: str) -> "Write":
        return self.e("TableStyle", {"style": style})

    def table_select(self, what: str) -> "Write":
        return self.w("TableSelect", {"what": what})


# ── Kiln Grid ─────────────────────────────────────────────────────────────────────────────────


def col_index(letters: str) -> int:
    n = 0
    for ch in letters:
        n = n * 26 + (ord(ch) - 64)
    return n - 1


def cell_ref(ref: str) -> tuple[int, int]:
    letters = "".join(c for c in ref if c.isalpha())
    digits = "".join(c for c in ref if c.isdigit())
    return int(digits) - 1, col_index(letters)


def rng(ref: str) -> dict:
    a, _, b = ref.partition(":")
    r0, c0 = cell_ref(a)
    r1, c1 = cell_ref(b or a)
    return {"r0": min(r0, r1), "c0": min(c0, c1), "r1": max(r0, r1), "c1": max(c0, c1)}


class Grid(Script):
    def __init__(self, name: str) -> None:
        super().__init__("grid", "grid", name, "kgrid", "KGrid")

    def g(self, name: str, args: Any = None) -> "Grid":
        self.cmd({"Grid": name if args is None else {name: args}})
        return self

    def select(self, ref: str) -> "Grid":
        if ":" in ref:
            return self.g("SelectRange", {"range": rng(ref), "add": False})
        r, c = cell_ref(ref)
        return self.g("SelectCell", {"row": r, "col": c, "extend": False, "add": False})

    def put(self, ref: str, value: Any) -> "Grid":
        """Types a value into a cell (a formula when it starts with `=`)."""
        # Typed in the formula bar (Edit mode), where AutoComplete does not finish a word from the
        # column's earlier entries ("VAT" would become "VAT rate" when typed into the cell).
        text = str(value)
        n = len(text.encode("utf-16-le")) // 2
        self.select(ref)
        self.g("EditCell")
        self.g("SetEditText", {"text": text, "start": n, "end": n, "formula_bar": True})
        return self.g("CommitEdit", {"advance": None})

    def row(self, ref: str, values: list) -> "Grid":
        """Types values across a row from `ref` (None skips a cell)."""
        r, c = cell_ref(ref)
        for i, v in enumerate(values):
            if v is None:
                continue
            self.put(f"{col_letters(c + i)}{r + 1}", v)
        return self

    def rows(self, ref: str, table: list[list]) -> "Grid":
        r, c = cell_ref(ref)
        for i, values in enumerate(table):
            self.row(f"{col_letters(c)}{r + 1 + i}", values)
        return self

    def fmt(self, ref: str, **kw: Any) -> "Grid":
        self.select(ref)
        return self.g("ApplyFormat", {"format": kw})

    def width(self, cols: str, chars: float) -> "Grid":
        a, _, b = cols.partition(":")
        self.g("SelectColumns", {"from": col_index(a), "to": col_index(b or a), "extend": False,
                                 "add": False})
        return self.g("ColumnWidth", {"chars": chars})

    def height(self, rows: str, points: float) -> "Grid":
        a, _, b = rows.partition(":")
        self.g("SelectRows", {"from": int(a) - 1, "to": int(b or a) - 1, "extend": False, "add": False})
        return self.g("RowHeight", {"points": points})

    def number(self, ref: str, code: str) -> "Grid":
        return self.fmt(ref, number_format=code)


def col_letters(i: int) -> str:
    s = ""
    i += 1
    while i:
        i, rem = divmod(i - 1, 26)
        s = chr(65 + rem) + s
    return s


def side(style: str = "Thin", color: str | None = None) -> dict:
    d: dict = {"style": style}
    if color is not None:
        d["color"] = color
    return d


# ── Kiln Deck ─────────────────────────────────────────────────────────────────────────────────


class Deck(Script):
    def __init__(self, name: str) -> None:
        super().__init__("deck", "deck", name, "kdeck", "KDeck")

    def d(self, name: str, args: Any = None) -> "Deck":
        self.cmd({"Deck": name if args is None else {name: args}})
        return self
