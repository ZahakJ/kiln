#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Port the 187 DrawingML preset geometries to Rust.

Reads the reference transcription of ECMA-376 Part 1 Annex D (presetShapeDefinitions.xml) in
reference/ts-core/ooxml/src/drawingml/presets/*.ts and writes
crates/kiln-ooxml/src/drawingml/presets/data.rs: a sorted static table that the geometry engine
compiles on first use.

    python3 -I tools/port-presets.py           # regenerate data.rs
    python3 -I tools/port-presets.py --check   # exit 1 if data.rs is not what this script writes

Standard library only; run it with -I so nothing on the current path is imported.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SOURCE_DIR = REPO / "reference" / "ts-core" / "ooxml" / "src" / "drawingml" / "presets"
TARGET = REPO / "crates" / "kiln-ooxml" / "src" / "drawingml" / "presets" / "data.rs"
EXPECTED_COUNT = 187

# File stem → Rust category variant, in gallery order.
CATEGORIES = {
    "basic": "Basic",
    "lines-connectors": "LinesConnectors",
    "arrows": "Arrows",
    "flowchart": "Flowchart",
    "stars-banners": "StarsBanners",
    "callouts": "Callouts",
    "math": "Math",
    "action-buttons": "ActionButtons",
}

FILL_MODES = {
    "none": "None",
    "norm": "Norm",
    "lighten": "Lighten",
    "lightenLess": "LightenLess",
    "darken": "Darken",
    "darkenLess": "DarkenLess",
}


class PortError(Exception):
    """The reference data is not in the shape this script understands."""


# --------------------------------------------------------------------------- TS literal parser

TOKEN = re.compile(
    r"""
    (?P<ws>\s+|//[^\n]*)
  | (?P<str>'(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*")
  | (?P<num>-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)
  | (?P<ident>[A-Za-z_$][\w$]*)
  | (?P<punct>[{}\[\]:,;])
    """,
    re.VERBOSE,
)


def tokenize(text: str) -> list[tuple[str, str]]:
    tokens: list[tuple[str, str]] = []
    pos = 0
    while pos < len(text):
        m = TOKEN.match(text, pos)
        if not m:
            raise PortError(f"unexpected character {text[pos]!r} at offset {pos}")
        pos = m.end()
        kind = m.lastgroup
        if kind == "ws":
            continue
        tokens.append((kind, m.group(kind)))
    return tokens


def unquote(s: str) -> str:
    body = s[1:-1]
    return re.sub(r"\\(.)", lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), body)


class Parser:
    def __init__(self, tokens: list[tuple[str, str]]) -> None:
        self.tokens = tokens
        self.i = 0

    def peek(self) -> tuple[str, str]:
        if self.i >= len(self.tokens):
            raise PortError("unexpected end of input")
        return self.tokens[self.i]

    def take(self, value: str | None = None) -> tuple[str, str]:
        tok = self.peek()
        if value is not None and tok[1] != value:
            raise PortError(f"expected {value!r}, found {tok[1]!r}")
        self.i += 1
        return tok

    def value(self):
        kind, text = self.peek()
        if text == "{":
            return self.obj()
        if text == "[":
            return self.arr()
        self.i += 1
        if kind == "str":
            return unquote(text)
        if kind == "num":
            return float(text) if any(c in text for c in ".eE") else int(text)
        if kind == "ident" and text in ("true", "false"):
            return text == "true"
        raise PortError(f"unexpected token {text!r}")

    def obj(self) -> dict:
        self.take("{")
        out: dict = {}
        while self.peek()[1] != "}":
            kind, key = self.take()
            if kind == "str":
                key = unquote(key)
            elif kind not in ("ident", "num"):
                raise PortError(f"bad object key {key!r}")
            self.take(":")
            if key in out:
                raise PortError(f"duplicate key {key!r}")
            out[key] = self.value()
            if self.peek()[1] == ",":
                self.take(",")
        self.take("}")
        return out

    def arr(self) -> list:
        self.take("[")
        out = []
        while self.peek()[1] != "]":
            out.append(self.value())
            if self.peek()[1] == ",":
                self.take(",")
        self.take("]")
        return out


def load_category(path: Path) -> dict:
    text = path.read_text(encoding="utf-8")
    m = re.search(r"export const \w+: Record<string, GeometryDef> = ", text)
    if not m:
        raise PortError(f"{path.name}: no preset table found")
    parser = Parser(tokenize(text[m.end():]))
    table = parser.obj()
    rest = parser.tokens[parser.i:]
    if rest not in ([], [("punct", ";")]):
        raise PortError(f"{path.name}: unexpected content after the table: {rest[:3]}")
    return table


# --------------------------------------------------------------------------- Rust emitter


def rs_str(s: str) -> str:
    if not isinstance(s, str):
        raise PortError(f"expected a string, found {s!r}")
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def rs_opt_str(s) -> str:
    return "None" if s is None else f"Some({rs_str(s)})"


def rs_opt_f64(n) -> str:
    if n is None:
        return "None"
    if not isinstance(n, (int, float)) or isinstance(n, bool):
        raise PortError(f"expected a number, found {n!r}")
    return f"Some({float(n)!r})"


def rs_opt_bool(b) -> str:
    if b is None:
        return "None"
    if not isinstance(b, bool):
        raise PortError(f"expected a boolean, found {b!r}")
    return f"Some({'true' if b else 'false'})"


def emit_cmd(cmd: list) -> str:
    op, *args = cmd
    arity = {"M": 2, "L": 2, "A": 4, "Q": 4, "C": 6, "Z": 0}
    if op not in arity or len(args) != arity[op]:
        raise PortError(f"bad path command {cmd!r}")
    if op == "Z":
        return "PresetCmd::Z"
    return f"PresetCmd::{op}({', '.join(rs_str(a) for a in args)})"


def emit_handle(h: dict) -> str:
    known = {"kind", "pos"}
    if h["kind"] == "xy":
        fields = ["gdRefX", "minX", "maxX", "gdRefY", "minY", "maxY"]
        rust = ["gd_ref_x", "min_x", "max_x", "gd_ref_y", "min_y", "max_y"]
        variant = "Xy"
    elif h["kind"] == "polar":
        fields = ["gdRefR", "minR", "maxR", "gdRefAng", "minAng", "maxAng"]
        rust = ["gd_ref_r", "min_r", "max_r", "gd_ref_ang", "min_ang", "max_ang"]
        variant = "Polar"
    else:
        raise PortError(f"bad handle kind {h['kind']!r}")
    extra = set(h) - known - set(fields)
    if extra:
        raise PortError(f"unknown handle fields {sorted(extra)}")
    pos = h["pos"]
    if len(pos) != 2:
        raise PortError(f"bad handle position {pos!r}")
    parts = [f"{r}: {rs_opt_str(h.get(f))}" for f, r in zip(fields, rust)]
    parts.append(f"pos: ({rs_str(pos[0])}, {rs_str(pos[1])})")
    return f"PresetHandle::{variant} {{ {', '.join(parts)} }}"


def emit_path(p: dict) -> str:
    extra = set(p) - {"w", "h", "fill", "stroke", "extrusionOk", "cmds"}
    if extra:
        raise PortError(f"unknown path fields {sorted(extra)}")
    fill = p.get("fill")
    if fill is not None and fill not in FILL_MODES:
        raise PortError(f"bad fill mode {fill!r}")
    fill_rs = "None" if fill is None else f"Some(PathFillMode::{FILL_MODES[fill]})"
    cmds = ", ".join(emit_cmd(c) for c in p["cmds"])
    return (
        f"PresetPath {{ w: {rs_opt_f64(p.get('w'))}, h: {rs_opt_f64(p.get('h'))}, "
        f"fill: {fill_rs}, stroke: {rs_opt_bool(p.get('stroke'))}, "
        f"extrusion_ok: {rs_opt_bool(p.get('extrusionOk'))}, cmds: &[{cmds}] }}"
    )


def emit_pairs(pairs: list) -> str:
    for p in pairs:
        if len(p) != 2:
            raise PortError(f"bad guide {p!r}")
    return ", ".join(f"({rs_str(n)}, {rs_str(f)})" for n, f in pairs)


def emit_preset(name: str, category: str, d: dict) -> list[str]:
    extra = set(d) - {"av", "gd", "ah", "cxn", "rect", "path"}
    if extra:
        raise PortError(f"{name}: unknown fields {sorted(extra)}")
    lines = [
        "    PresetDef {",
        f"        name: {rs_str(name)},",
        f"        category: PresetCategory::{category},",
        f"        av: &[{emit_pairs(d.get('av', []))}],",
    ]
    gd = d.get("gd", [])
    if gd:
        lines.append("        gd: &[")
        for n, f in gd:
            lines.append(f"            ({rs_str(n)}, {rs_str(f)}),")
        lines.append("        ],")
    else:
        lines.append("        gd: &[],")
    ah = d.get("ah", [])
    if ah:
        lines.append("        ah: &[")
        lines.extend(f"            {emit_handle(h)}," for h in ah)
        lines.append("        ],")
    else:
        lines.append("        ah: &[],")
    cxn = d.get("cxn", [])
    for c in cxn:
        if len(c) != 3:
            raise PortError(f"{name}: bad connection site {c!r}")
    lines.append(f"        cxn: &[{', '.join('(' + ', '.join(rs_str(v) for v in c) + ')' for c in cxn)}],")
    rect = d.get("rect")
    if rect is None:
        lines.append("        rect: None,")
    else:
        if len(rect) != 4:
            raise PortError(f"{name}: bad text rectangle {rect!r}")
        lines.append(f"        rect: Some(({', '.join(rs_str(v) for v in rect)})),")
    lines.append("        path: &[")
    if not d.get("path"):
        raise PortError(f"{name}: no paths")
    lines.extend(f"            {emit_path(p)}," for p in d["path"])
    lines.append("        ],")
    lines.append("    },")
    return lines


def generate() -> str:
    presets: dict[str, tuple[str, dict]] = {}
    for stem, category in CATEGORIES.items():
        path = SOURCE_DIR / f"{stem}.ts"
        if not path.is_file():
            raise PortError(f"missing {path.relative_to(REPO)}")
        for name, d in load_category(path).items():
            if name in presets:
                raise PortError(f"{name} defined twice")
            presets[name] = (category, d)
    stray = sorted(p.stem for p in SOURCE_DIR.glob("*.ts") if p.stem not in CATEGORIES and p.stem != "index")
    if stray:
        raise PortError(f"unexpected preset files: {stray}")
    if len(presets) != EXPECTED_COUNT:
        raise PortError(f"expected {EXPECTED_COUNT} presets, found {len(presets)}")

    out = [
        "// SPDX-License-Identifier: AGPL-3.0-or-later",
        "// @generated by tools/port-presets.py from reference/ts-core/ooxml/src/drawingml/presets/*.ts",
        "// (ECMA-376 Part 1 Annex D, presetShapeDefinitions.xml). Do not edit by hand.",
        "//! The preset geometry table, sorted by name.",
        "",
        "use super::{PresetCategory, PresetCmd, PresetDef, PresetHandle, PresetPath};",
        "use crate::st::PathFillMode;",
        "",
        f"pub(super) const PRESET_COUNT: usize = {len(presets)};",
        "",
        "pub(super) static PRESETS: [PresetDef; PRESET_COUNT] = [",
    ]
    for name in sorted(presets):
        category, d = presets[name]
        out.extend(emit_preset(name, category, d))
    out.append("];")
    return "\n".join(out) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--check", action="store_true", help="verify data.rs is up to date")
    args = ap.parse_args()
    try:
        text = generate()
    except PortError as e:
        print(f"port-presets: {e}", file=sys.stderr)
        return 2
    if args.check:
        current = TARGET.read_text(encoding="utf-8") if TARGET.is_file() else ""
        if current != text:
            print(
                f"port-presets: {TARGET.relative_to(REPO)} is out of date; "
                "run python3 -I tools/port-presets.py",
                file=sys.stderr,
            )
            return 1
        print(f"port-presets: {TARGET.relative_to(REPO)} is current ({EXPECTED_COUNT} presets)")
        return 0
    TARGET.write_text(text, encoding="utf-8")
    print(f"port-presets: wrote {TARGET.relative_to(REPO)} ({EXPECTED_COUNT} presets)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
