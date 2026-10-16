#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Single-paragraph .docx fixtures for the kiln-text LibreOffice line-count test (WP-53).

    ~/.venvs/kiln/bin/python fixtures/docx/text/make_fixtures.py            # write specs + .docx
    ~/.venvs/kiln/bin/python fixtures/docx/text/make_fixtures.py --oracle   # also render with LibreOffice

Each fixture is described once in FIXTURES; the script writes `<name>.json` (read by the Rust test
`crates/kiln-text/tests/libreoffice_oracle.rs`) and `<name>.docx` (built with python-docx). With --oracle it
converts every .docx to PDF through tools/oracle-soffice.sh, extracts the text lines with
`pdftotext -layout` and records their count and (whitespace-normalised) text in `oracle.json`, which the Rust test compares against.
"""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

from docx import Document
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.oxml.ns import qn
from docx.shared import Pt

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]

LOREM = (
    "Kiln is an open-source office suite that opens, edits and saves real Microsoft Office files and "
    "feels immediately familiar to Office users, while looking and feeling like its own product: warmer, "
    "calmer and faster, with features Office lacks such as a command palette with Office synonyms, "
    "built-in version history, self-hosted live collaboration and presenter view everywhere."
)
PAGINATION = (
    "Pagination and line breaking are deterministic code using bundled font metrics, so a long document "
    "paginates identically on every platform and identically in the exported PDF. The layout engine owns "
    "the hot loop of layout, invalidation, culling and glyph caching, and the painter only blits tiles "
    "and glyph quads onto the canvas of each page."
)
FIDELITY = (
    "Fidelity is measured, not claimed: every fixture in the corpus is opened, laid out and compared "
    "with a reference rendering, and a regression in the number of lines of any paragraph fails the "
    "build before it reaches anyone who depends on their documents looking the way they wrote them."
)

# width_pt is the text width (page width minus the left and right margins).
FIXTURES = [
    {"name": "carlito_left_11pt", "font": "Calibri", "size_pt": 11, "width_pt": 468,
     "alignment": "left", "runs": [{"text": LOREM}]},
    {"name": "caladea_justify_12pt", "font": "Cambria", "size_pt": 12, "width_pt": 453.5,
     "alignment": "justify", "runs": [{"text": PAGINATION}]},
    {"name": "times_left_12pt_narrow", "font": "Times New Roman", "size_pt": 12, "width_pt": 216,
     "alignment": "left", "runs": [{"text": FIDELITY}]},
    {"name": "arial_center_10pt", "font": "Arial", "size_pt": 10, "width_pt": 360,
     "alignment": "center", "runs": [{"text": LOREM}]},
    {"name": "courier_right_10pt", "font": "Courier New", "size_pt": 10, "width_pt": 400,
     "alignment": "right", "runs": [{"text": PAGINATION}]},
    {"name": "carlito_first_line_indent", "font": "Calibri", "size_pt": 12, "width_pt": 400,
     "alignment": "left", "first_line_pt": 36, "runs": [{"text": FIDELITY}]},
    {"name": "caladea_hanging_indent", "font": "Cambria", "size_pt": 11, "width_pt": 420,
     "alignment": "left", "left_pt": 36, "first_line_pt": -36, "runs": [{"text": "1.\t" + LOREM}]},
    {"name": "liberation_sans_both_indents", "font": "Liberation Sans", "size_pt": 11, "width_pt": 468,
     "alignment": "justify", "left_pt": 72, "right_pt": 54, "runs": [{"text": PAGINATION}]},
    {"name": "carlito_mixed_runs", "font": "Calibri", "size_pt": 11, "width_pt": 340,
     "alignment": "left", "runs": [
         {"text": "Mixed formatting: "},
         {"text": "bold words", "bold": True},
         {"text": " sit next to "},
         {"text": "italic Cambria at fourteen points", "italic": True, "font": "Cambria", "size_pt": 14},
         {"text": " and a run of "},
         {"text": "Courier New code", "font": "Courier New"},
         {"text": ". " + FIDELITY},
     ]},
    {"name": "times_16pt_justify", "font": "Times New Roman", "size_pt": 16, "width_pt": 432,
     "alignment": "justify", "runs": [{"text": LOREM}]},
]


def set_run_font(run, family):
    run.font.name = family
    rpr = run._element.get_or_add_rPr()
    fonts = rpr.find(qn("w:rFonts"))
    if fonts is None:
        fonts = rpr.makeelement(qn("w:rFonts"), {})
        rpr.insert(0, fonts)
    for attr in ("w:ascii", "w:hAnsi", "w:cs", "w:eastAsia"):
        fonts.set(qn(attr), family)


def build(fx):
    doc = Document()
    section = doc.sections[0]
    # US Letter, margins chosen so the text width is width_pt.
    page_w = 612.0
    side = (page_w - fx["width_pt"]) / 2
    section.page_width = Pt(page_w)
    section.page_height = Pt(792)
    section.left_margin = Pt(side)
    section.right_margin = Pt(side)
    section.top_margin = Pt(72)
    section.bottom_margin = Pt(72)
    # Make the default paragraph neutral (python-docx's template adds spacing to Normal).
    normal = doc.styles["Normal"]
    normal.paragraph_format.space_before = Pt(0)
    normal.paragraph_format.space_after = Pt(0)
    normal.paragraph_format.line_spacing = 1.0
    p = doc.paragraphs[0] if doc.paragraphs else doc.add_paragraph()
    pf = p.paragraph_format
    pf.alignment = {
        "left": WD_ALIGN_PARAGRAPH.LEFT,
        "center": WD_ALIGN_PARAGRAPH.CENTER,
        "right": WD_ALIGN_PARAGRAPH.RIGHT,
        "justify": WD_ALIGN_PARAGRAPH.JUSTIFY,
    }[fx["alignment"]]
    pf.left_indent = Pt(fx.get("left_pt", 0))
    pf.right_indent = Pt(fx.get("right_pt", 0))
    pf.first_line_indent = Pt(fx.get("first_line_pt", 0))
    pf.space_before = Pt(0)
    pf.space_after = Pt(0)
    for r in fx["runs"]:
        run = p.add_run(r["text"])
        set_run_font(run, r.get("font", fx["font"]))
        run.font.size = Pt(r.get("size_pt", fx["size_pt"]))
        run.font.bold = r.get("bold", False)
        run.font.italic = r.get("italic", False)
    doc.save(HERE / f"{fx['name']}.docx")


def oracle_lines(docx_path):
    with tempfile.TemporaryDirectory() as out:
        subprocess.run([str(REPO / "tools/oracle-soffice.sh"), str(docx_path), "pdf", out],
                       check=True, capture_output=True)
        pdf = Path(out) / (docx_path.stem + ".pdf")
        text = subprocess.run(["pdftotext", "-layout", str(pdf), "-"], check=True,
                              capture_output=True, text=True).stdout
    return [" ".join(line.split()) for line in text.splitlines() if line.strip()]


def main():
    for fx in FIXTURES:
        (HERE / f"{fx['name']}.json").write_text(json.dumps(fx, indent=2, ensure_ascii=False) + "\n")
        build(fx)
    if "--oracle" in sys.argv:
        oracle = {}
        for fx in FIXTURES:
            lines = oracle_lines(HERE / f"{fx['name']}.docx")
            oracle[fx["name"]] = {"line_count": len(lines), "lines": lines}
        (HERE / "oracle.json").write_text(json.dumps(oracle, indent=2) + "\n")
        print(json.dumps(oracle, indent=2))


if __name__ == "__main__":
    main()
