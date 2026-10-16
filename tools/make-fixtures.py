#!/usr/bin/env python3
"""Generate Kiln's synthetic .docx/.xlsx/.pptx fixture corpus (PLAN.md §5).

    python3 -I tools/make-fixtures.py --out fixtures [--large] [--only docx,xlsx,pptx]
    python3 -I tools/make-fixtures.py --synthetic-large [--only docx,xlsx,pptx]

Files go to <out>/<format>/features/<name>.<ext> (``--out`` defaults to the repository's fixtures/). The
``--large`` flag also writes the 100 000-row workbook to <out>/xlsx/large/ (gitignored; too big for the
repository and for PDF conversion tests).

``--synthetic-large`` writes only the performance-budget inputs of PLAN.md §3.9 to <out>/large/
(gitignored, regenerated on demand): ``doc-300p.docx`` (300 pages of headings, body text, tables and
pictures, one page per explicit page break), ``sheet-1m.xlsx`` (1 000 000 populated cells, 200 000 of them
formulas) and ``deck-200.pptx`` (200 slides: bullets, two columns, pictures, tables, shapes, charts).

Output is deterministic: fixed random seeds, a fixed document date, fixed zip timestamps and a fixed
entry order, so regenerating produces byte-identical files and the committed corpus only changes when
this script does.

The script needs python-docx, openpyxl and python-pptx. When they are not importable it re-executes
itself with the project virtualenv (~/.venvs/kiln, PLAN.md §4.5), creating it first if necessary.
"""

from __future__ import annotations

import argparse
import datetime as dt
import io
import os
import random
import re
import subprocess
import sys
import zipfile
from pathlib import Path

VENV = Path.home() / ".venvs" / "kiln"
REQUIREMENTS = ["openpyxl==3.1.*", "python-docx==1.1.*", "python-pptx==1.0.*"]
FIXED_DATE = dt.datetime(2026, 1, 15, 9, 30, 0)
FIXED_W3CDTF = "2026-01-15T09:30:00Z"
ZIP_DATE = (1980, 1, 1, 0, 0, 0)
AUTHOR = "Kiln Fixtures"


def ensure_dependencies() -> None:
    try:
        import docx  # noqa: F401
        import openpyxl  # noqa: F401
        import pptx  # noqa: F401
        return
    except ImportError:
        pass
    py = VENV / "bin" / "python"
    if Path(sys.prefix).resolve() == VENV.resolve():
        sys.exit("make-fixtures: the virtualenv is missing python-docx/openpyxl/python-pptx")
    if not py.exists():
        print(f"make-fixtures: creating {VENV}", file=sys.stderr)
        subprocess.run([sys.executable, "-m", "venv", str(VENV)], check=True)
    probe = subprocess.run([str(py), "-c", "import docx, openpyxl, pptx"], capture_output=True)
    if probe.returncode != 0:
        subprocess.run([str(py), "-m", "pip", "install", "--quiet", *REQUIREMENTS], check=True)
    os.execv(str(py), [str(py), "-I", os.path.abspath(__file__), *sys.argv[1:]])


# --------------------------------------------------------------------------- common


def normalize_package(data: bytes, rewrite=None) -> bytes:
    """Rewrite a zip with fixed timestamps and fixed core-property dates.

    ``rewrite(name, bytes) -> bytes | None`` may change a part, drop it (``None``), and the special
    name ``"+"`` is called once with ``b""`` and may return a list of extra ``(name, bytes)`` entries.
    """
    src = zipfile.ZipFile(io.BytesIO(data))
    out = io.BytesIO()
    names = src.namelist()
    # Office order: content types first, package relationships second, then the rest as written.
    order = [n for n in ("[Content_Types].xml", "_rels/.rels") if n in names]
    order += [n for n in names if n not in order]
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as dst:
        entries = []
        for name in order:
            body = src.read(name)
            if name == "docProps/core.xml":
                text = body.decode("utf-8")
                text = re.sub(r"(<dcterms:created[^>]*>)[^<]*(<)", rf"\g<1>{FIXED_W3CDTF}\2", text)
                text = re.sub(r"(<dcterms:modified[^>]*>)[^<]*(<)", rf"\g<1>{FIXED_W3CDTF}\2", text)
                text = re.sub(r"(<cp:lastPrinted[^>]*>)[^<]*(<)", rf"\g<1>{FIXED_W3CDTF}\2", text)
                body = text.encode("utf-8")
            if name.endswith((".xlsx", ".docx", ".pptx")) and body[:2] == b"PK":
                body = normalize_package(body)  # embedded chart workbooks carry their own timestamps
            if rewrite is not None:
                body = rewrite(name, body)
                if body is None:
                    continue
            entries.append((name, body))
        if rewrite is not None:
            extra = rewrite("+", b"")
            if extra:
                entries.extend(extra)
        for name, body in entries:
            info = zipfile.ZipInfo(name, date_time=ZIP_DATE)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o600 << 16
            dst.writestr(info, body, compresslevel=6)
    return out.getvalue()


def png_bytes(width: int, height: int, seed: int, kind: str = "gradient") -> bytes:
    from PIL import Image, ImageDraw

    rng = random.Random(seed)
    img = Image.new("RGB", (width, height), (255, 255, 255))
    draw = ImageDraw.Draw(img)
    c1 = (rng.randrange(40, 200), rng.randrange(40, 200), rng.randrange(40, 200))
    c2 = (rng.randrange(120, 255), rng.randrange(120, 255), rng.randrange(120, 255))
    if kind == "gradient":
        for x in range(width):
            t = x / max(width - 1, 1)
            draw.line([(x, 0), (x, height)], fill=tuple(int(a + (b - a) * t) for a, b in zip(c1, c2)))
    else:
        draw.rectangle([0, 0, width, height], fill=c2)
    # A grid and a circle make crops and rotations visible in renders.
    for x in range(0, width, max(width // 8, 1)):
        draw.line([(x, 0), (x, height)], fill=(255, 255, 255), width=1)
    for y in range(0, height, max(height // 8, 1)):
        draw.line([(0, y), (width, y)], fill=(255, 255, 255), width=1)
    r = min(width, height) // 4
    draw.ellipse([width // 2 - r, height // 2 - r, width // 2 + r, height // 2 + r], outline=(20, 20, 20), width=3)
    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=False)
    return buf.getvalue()


def jpeg_bytes(width: int, height: int, seed: int) -> bytes:
    from PIL import Image

    img = Image.open(io.BytesIO(png_bytes(width, height, seed)))
    buf = io.BytesIO()
    img.save(buf, format="JPEG", quality=85)
    return buf.getvalue()


LOREM = (
    "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et "
    "dolore magna aliqua ut enim ad minim veniam quis nostrud exercitation ullamco laboris nisi ut aliquip ex "
    "ea commodo consequat duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu "
    "fugiat nulla pariatur excepteur sint occaecat cupidatat non proident sunt in culpa qui officia deserunt "
    "mollit anim id est laborum"
).split()


def lorem(rng: random.Random, words: int) -> str:
    out = [rng.choice(LOREM) for _ in range(words)]
    out[0] = out[0].capitalize()
    return " ".join(out) + "."


class Writer:
    def __init__(self, out: Path, fmt: str):
        self.dir = out / fmt / "features"
        self.dir.mkdir(parents=True, exist_ok=True)
        self.fmt = fmt
        self.count = 0

    def save(self, name: str, data: bytes, rewrite=None) -> None:
        path = self.dir / f"{name}.{self.fmt}"
        path.write_bytes(normalize_package(data, rewrite))
        self.count += 1


# ----------------------------------------------------------------------------- docx

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
WPS_NS = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"


def new_document():
    import docx

    d = docx.Document()
    cp = d.core_properties
    cp.author = AUTHOR
    cp.last_modified_by = AUTHOR
    cp.created = FIXED_DATE
    cp.modified = FIXED_DATE
    cp.revision = 1
    return d


def save_docx(w: Writer, name: str, d, title: str, rewrite=None) -> None:
    d.core_properties.title = title
    buf = io.BytesIO()
    d.save(buf)
    w.save(name, buf.getvalue(), rewrite)


def docx_styles(w: Writer) -> None:
    from docx.enum.text import WD_COLOR_INDEX, WD_UNDERLINE
    from docx.shared import Pt, RGBColor

    d = new_document()
    d.add_heading("Text styles matrix", 0)
    for level in range(1, 7):
        d.add_heading(f"Heading {level}", level)
    for style in ["Title", "Subtitle", "Quote", "Intense Quote", "List Paragraph", "No Spacing", "Caption"]:
        d.add_paragraph(f"Paragraph in the {style} style.", style=style)
    d.add_heading("Character formatting", 1)
    toggles = [
        ("bold", {"bold": True}),
        ("italic", {"italic": True}),
        ("bold italic", {"bold": True, "italic": True}),
        ("strike", {"strike": True}),
        ("double strike", {"double_strike": True}),
        ("small caps", {"small_caps": True}),
        ("all caps", {"all_caps": True}),
        ("superscript", {"superscript": True}),
        ("subscript", {"subscript": True}),
        ("hidden", {"hidden": True}),
    ]
    p = d.add_paragraph()
    for label, props in toggles:
        r = p.add_run(label)
        for k, v in props.items():
            setattr(r.font, k, v)
        p.add_run(" · ")
    d.add_heading("Underlines", 2)
    p = d.add_paragraph()
    for u in [WD_UNDERLINE.SINGLE, WD_UNDERLINE.DOUBLE, WD_UNDERLINE.DOTTED, WD_UNDERLINE.DASH, WD_UNDERLINE.WAVY, WD_UNDERLINE.THICK, WD_UNDERLINE.WORDS]:
        r = p.add_run(f"underline {u.name.lower()}")
        r.font.underline = u
        p.add_run("  ")
    d.add_heading("Sizes and fonts", 2)
    p = d.add_paragraph()
    for size in [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36]:
        r = p.add_run(f"{size}pt ")
        r.font.size = Pt(size)
    for font in ["Calibri", "Cambria", "Arial", "Times New Roman", "Courier New", "Georgia", "Verdana"]:
        r = d.add_paragraph().add_run(f"The quick brown fox in {font}")
        r.font.name = font
    d.add_heading("Colours and highlights", 2)
    p = d.add_paragraph()
    for hexv in ["C00000", "FF0000", "FFC000", "FFFF00", "92D050", "00B050", "00B0F0", "0070C0", "002060", "7030A0"]:
        r = p.add_run(f"#{hexv} ")
        r.font.color.rgb = RGBColor.from_string(hexv)
    p = d.add_paragraph()
    for hl in [WD_COLOR_INDEX.YELLOW, WD_COLOR_INDEX.BRIGHT_GREEN, WD_COLOR_INDEX.TURQUOISE, WD_COLOR_INDEX.PINK, WD_COLOR_INDEX.GRAY_25]:
        r = p.add_run(f"{hl.name.lower()} ")
        r.font.highlight_color = hl
    p = d.add_paragraph()
    from docx.enum.dml import MSO_THEME_COLOR

    for tc in [MSO_THEME_COLOR.ACCENT_1, MSO_THEME_COLOR.ACCENT_2, MSO_THEME_COLOR.ACCENT_6, MSO_THEME_COLOR.TEXT_2]:
        r = p.add_run(f"theme {tc.name.lower()} ")
        r.font.color.theme_color = tc
    d.add_heading("Character spacing", 2)
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls

    p = d.add_paragraph()
    for spacing in [-20, 0, 20, 60]:
        r = p.add_run(f"spacing {spacing} ")
        r._r.get_or_add_rPr().append(parse_xml(f'<w:spacing {nsdecls("w")} w:val="{spacing}"/>'))
    save_docx(w, "styles", d, "Text styles matrix")


def docx_paragraphs(w: Writer) -> None:
    from docx.enum.text import WD_ALIGN_PARAGRAPH, WD_LINE_SPACING, WD_TAB_ALIGNMENT, WD_TAB_LEADER
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls
    from docx.shared import Cm, Pt

    rng = random.Random(2)
    d = new_document()
    d.add_heading("Paragraph formatting", 1)
    for align in [WD_ALIGN_PARAGRAPH.LEFT, WD_ALIGN_PARAGRAPH.CENTER, WD_ALIGN_PARAGRAPH.RIGHT, WD_ALIGN_PARAGRAPH.JUSTIFY]:
        p = d.add_paragraph(f"[{align.name}] " + lorem(rng, 40))
        p.alignment = align
    for label, kw in [
        ("left indent 2 cm", {"left_indent": Cm(2)}),
        ("right indent 2 cm", {"right_indent": Cm(2)}),
        ("first line 1.27 cm", {"first_line_indent": Cm(1.27)}),
        ("hanging 1.27 cm", {"left_indent": Cm(1.27), "first_line_indent": Cm(-1.27)}),
        ("space before 18 pt / after 6 pt", {"space_before": Pt(18), "space_after": Pt(6)}),
    ]:
        p = d.add_paragraph(f"[{label}] " + lorem(rng, 30))
        for k, v in kw.items():
            setattr(p.paragraph_format, k, v)
    for label, rule, value in [
        ("single", WD_LINE_SPACING.SINGLE, None),
        ("1.5 lines", WD_LINE_SPACING.ONE_POINT_FIVE, None),
        ("double", WD_LINE_SPACING.DOUBLE, None),
        ("exactly 20 pt", WD_LINE_SPACING.EXACTLY, Pt(20)),
        ("at least 24 pt", WD_LINE_SPACING.AT_LEAST, Pt(24)),
        ("multiple 1.15", WD_LINE_SPACING.MULTIPLE, 1.15),
    ]:
        p = d.add_paragraph(f"[line spacing {label}] " + lorem(rng, 35))
        pf = p.paragraph_format
        if value is None:
            pf.line_spacing_rule = rule
        else:
            pf.line_spacing = value
            if rule in (WD_LINE_SPACING.EXACTLY, WD_LINE_SPACING.AT_LEAST):
                pf.line_spacing_rule = rule
    p = d.add_paragraph("Keep with next: this paragraph stays with the next one.")
    p.paragraph_format.keep_with_next = True
    p = d.add_paragraph("Keep lines together. " + lorem(rng, 50))
    p.paragraph_format.keep_together = True
    p = d.add_paragraph("Widow control off. " + lorem(rng, 20))
    p.paragraph_format.widow_control = False
    d.add_heading("Borders and shading", 2)
    p = d.add_paragraph("Boxed paragraph with shading. " + lorem(rng, 20))
    p._p.get_or_add_pPr().append(
        parse_xml(
            f'<w:pBdr {nsdecls("w")}><w:top w:val="single" w:sz="8" w:space="1" w:color="2F5496"/>'
            '<w:left w:val="single" w:sz="8" w:space="4" w:color="2F5496"/>'
            '<w:bottom w:val="double" w:sz="6" w:space="1" w:color="2F5496"/>'
            '<w:right w:val="single" w:sz="8" w:space="4" w:color="2F5496"/></w:pBdr>'
        )
    )
    p._p.get_or_add_pPr().append(parse_xml(f'<w:shd {nsdecls("w")} w:val="clear" w:color="auto" w:fill="DEEAF6"/>'))
    d.add_heading("Tab stops", 2)
    p = d.add_paragraph("Left\tCentre\tRight\t1234.56")
    ts = p.paragraph_format.tab_stops
    ts.add_tab_stop(Cm(4), WD_TAB_ALIGNMENT.CENTER, WD_TAB_LEADER.DOTS)
    ts.add_tab_stop(Cm(9), WD_TAB_ALIGNMENT.RIGHT, WD_TAB_LEADER.DASHES)
    ts.add_tab_stop(Cm(14), WD_TAB_ALIGNMENT.DECIMAL, WD_TAB_LEADER.SPACES)
    p = d.add_paragraph("Page break before this paragraph.")
    p.paragraph_format.page_break_before = True
    save_docx(w, "paragraphs", d, "Paragraph formatting")


def docx_numbering(w: Writer) -> None:
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls, qn

    d = new_document()
    numbering = d.part.numbering_part.element
    base_abs = 40
    base_num = 40

    def lvl(ilvl: int, fmt: str, text: str, indent: int, font: str | None = None) -> str:
        rpr = f'<w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:hint="default"/></w:rPr>' if font else ""
        return (
            f'<w:lvl w:ilvl="{ilvl}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/>'
            f'<w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{indent}" w:hanging="360"/></w:pPr>{rpr}</w:lvl>'
        )

    kinds = {
        "bullet": [lvl(i, "bullet", ["•", "o", "▪"][i % 3], 720 * (i + 1), ["Symbol", "Courier New", "Wingdings"][i % 3]) for i in range(9)],
        "decimal": [lvl(i, "decimal", f"%{i + 1}.", 720 * (i + 1)) for i in range(9)],
        "lowerLetter": [lvl(i, "lowerLetter", f"%{i + 1})", 720 * (i + 1)) for i in range(9)],
        "upperRoman": [lvl(i, "upperRoman", f"%{i + 1}.", 720 * (i + 1)) for i in range(9)],
        "legal": [lvl(i, "decimal", ".".join(f"%{j + 1}" for j in range(i + 1)), 432 + 360 * i) for i in range(9)],
    }
    first_num = numbering.find(qn("w:num"))
    ids = {}
    for k, (kind, levels) in enumerate(kinds.items()):
        abs_id = base_abs + k
        multi = "hybridMultilevel" if kind != "legal" else "multilevel"
        node = parse_xml(
            f'<w:abstractNum {nsdecls("w")} w:abstractNumId="{abs_id}"><w:multiLevelType w:val="{multi}"/>'
            + "".join(levels)
            + "</w:abstractNum>"
        )
        if first_num is not None:
            first_num.addprevious(node)
        else:
            numbering.append(node)
        num_id = base_num + k
        numbering.append(parse_xml(f'<w:num {nsdecls("w")} w:numId="{num_id}"><w:abstractNumId w:val="{abs_id}"/></w:num>'))
        ids[kind] = (abs_id, num_id)
    # A second instance of the decimal list that restarts at 1.
    restart_id = base_num + 20
    numbering.append(
        parse_xml(
            f'<w:num {nsdecls("w")} w:numId="{restart_id}"><w:abstractNumId w:val="{ids["decimal"][0]}"/>'
            '<w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride></w:num>'
        )
    )

    def item(text: str, num_id: int, level: int = 0) -> None:
        p = d.add_paragraph(text, style="List Paragraph")
        p._p.get_or_add_pPr().append(
            parse_xml(f'<w:numPr {nsdecls("w")}><w:ilvl w:val="{level}"/><w:numId w:val="{num_id}"/></w:numPr>')
        )

    d.add_heading("Numbering and bullets", 1)
    for kind, (_, num_id) in ids.items():
        d.add_heading(f"{kind} list", 2)
        for i in range(3):
            item(f"{kind} item {i + 1}", num_id, 0)
            item(f"{kind} item {i + 1}.1", num_id, 1)
        item(f"{kind} level 3", num_id, 2)
    d.add_heading("Restarted numbering", 2)
    item("Continues the decimal list", ids["decimal"][1])
    item("Restarts at one", restart_id)
    item("Then two", restart_id)
    d.add_heading("Built-in list styles", 2)
    for s in ["List Bullet", "List Bullet 2", "List Number", "List Number 2"]:
        d.add_paragraph(f"Paragraph in {s}", style=s)
    save_docx(w, "numbering", d, "Numbering and bullets")


def docx_tables(w: Writer) -> None:
    from docx.enum.table import WD_TABLE_ALIGNMENT
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls
    from docx.shared import Cm

    d = new_document()
    d.add_heading("Tables", 1)
    d.add_paragraph("Grid table with header row:")
    t = d.add_table(rows=4, cols=4)
    t.style = "Table Grid"
    for r in range(4):
        for c in range(4):
            t.cell(r, c).text = f"R{r + 1}C{c + 1}" if r else f"Header {c + 1}"
    d.add_paragraph("Styled table (Light Grid Accent 1) with horizontal and vertical merges:")
    t = d.add_table(rows=5, cols=4)
    t.style = "Light Grid Accent 1"
    t.alignment = WD_TABLE_ALIGNMENT.CENTER
    for r in range(5):
        for c in range(4):
            t.cell(r, c).text = f"{r},{c}"
    a = t.cell(1, 0).merge(t.cell(1, 2))
    a.text = "Merged across three columns"
    b = t.cell(2, 3).merge(t.cell(4, 3))
    b.text = "Merged down three rows"
    d.add_paragraph("Shaded cells, widths and a nested table:")
    t = d.add_table(rows=2, cols=3)
    t.style = "Table Grid"
    for i, width in enumerate([Cm(3), Cm(6), Cm(4)]):
        for cell in t.columns[i].cells:
            cell.width = width
    for c, fill in enumerate(["FFF2CC", "E2EFDA", "DDEBF7"]):
        cell = t.cell(0, c)
        cell.text = f"fill {fill}"
        cell._tc.get_or_add_tcPr().append(parse_xml(f'<w:shd {nsdecls("w")} w:val="clear" w:color="auto" w:fill="{fill}"/>'))
    inner = t.cell(1, 1).add_table(rows=2, cols=2)
    inner.style = "Table Grid"
    for r in range(2):
        for c in range(2):
            inner.cell(r, c).text = f"inner {r}{c}"
    t.cell(1, 0).text = "Outer cell"
    t.cell(1, 2).text = "Vertical\nlines"
    save_docx(w, "tables", d, "Tables")


def docx_sections(w: Writer) -> None:
    from docx.enum.section import WD_ORIENT, WD_SECTION
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls
    from docx.shared import Cm, Mm

    rng = random.Random(5)
    d = new_document()
    s0 = d.sections[0]
    s0.page_width, s0.page_height = Mm(210), Mm(297)
    s0.different_first_page_header_footer = True
    s0.header.paragraphs[0].text = "Default header (section 1)"
    s0.first_page_header.paragraphs[0].text = "First-page header"
    s0.footer.paragraphs[0].text = "Default footer"
    d.settings.odd_and_even_pages_header_footer = True
    s0.even_page_header.paragraphs[0].text = "Even-page header"
    d.add_heading("Section 1: A4 portrait, first-page and even headers", 1)
    for _ in range(12):
        d.add_paragraph(lorem(rng, 60))
    s1 = d.add_section(WD_SECTION.NEW_PAGE)
    s1.orientation = WD_ORIENT.LANDSCAPE
    s1.page_width, s1.page_height = Mm(297), Mm(210)
    s1.left_margin = s1.right_margin = Cm(1.5)
    s1.header.is_linked_to_previous = False
    s1.header.paragraphs[0].text = "Landscape section header"
    d.add_heading("Section 2: landscape, new page", 1)
    for _ in range(4):
        d.add_paragraph(lorem(rng, 50))
    s2 = d.add_section(WD_SECTION.CONTINUOUS)
    s2.orientation = WD_ORIENT.LANDSCAPE
    s2.page_width, s2.page_height = Mm(297), Mm(210)
    s2._sectPr.append(parse_xml(f'<w:cols {nsdecls("w")} w:num="2" w:space="720" w:sep="1"/>'))
    d.add_heading("Section 3: continuous, two columns", 1)
    for _ in range(6):
        d.add_paragraph(lorem(rng, 45))
    s3 = d.add_section(WD_SECTION.ODD_PAGE)
    s3.orientation = WD_ORIENT.PORTRAIT
    s3.page_width, s3.page_height = Mm(215.9), Mm(279.4)
    s3.top_margin = s3.bottom_margin = Cm(3)
    s3.gutter = Cm(1)
    d.add_heading("Section 4: odd page, US Letter, gutter", 1)
    d.add_paragraph(lorem(rng, 80))
    save_docx(w, "sections", d, "Sections")


def docx_headers_footers(w: Writer) -> None:
    from docx.enum.text import WD_ALIGN_PARAGRAPH

    rng = random.Random(6)
    d = new_document()
    s = d.sections[0]
    hp = s.header.paragraphs[0]
    hp.text = "Kiln fixtures\tHeaders and footers\tConfidential"
    footer = s.footer.paragraphs[0]
    footer.alignment = WD_ALIGN_PARAGRAPH.CENTER
    add_field_runs(footer, "Page ", "PAGE", "1")
    add_field_runs(footer, " of ", "NUMPAGES", "3")
    for i in range(30):
        d.add_paragraph(lorem(rng, 70))
    save_docx(w, "headers-footers", d, "Headers and footers")


def add_field_runs(paragraph, prefix: str, instr: str, result: str) -> None:
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls

    if prefix:
        paragraph.add_run(prefix)
    p = paragraph._p
    p.append(parse_xml(f'<w:r {nsdecls("w")}><w:fldChar w:fldCharType="begin"/></w:r>'))
    p.append(parse_xml(f'<w:r {nsdecls("w")}><w:instrText xml:space="preserve"> {instr} </w:instrText></w:r>'))
    p.append(parse_xml(f'<w:r {nsdecls("w")}><w:fldChar w:fldCharType="separate"/></w:r>'))
    p.append(parse_xml(f'<w:r {nsdecls("w")}><w:t>{result}</w:t></w:r>'))
    p.append(parse_xml(f'<w:r {nsdecls("w")}><w:fldChar w:fldCharType="end"/></w:r>'))


def inline_picture(d, data: bytes, width_cm: float):
    from docx.shared import Cm

    run = d.add_paragraph().add_run()
    run.add_picture(io.BytesIO(data), width=Cm(width_cm))
    return run


def docx_images_inline(w: Writer) -> None:
    d = new_document()
    d.add_heading("Inline images", 1)
    d.add_paragraph("A PNG at 8 cm wide:")
    inline_picture(d, png_bytes(320, 200, 11), 8)
    d.add_paragraph("A JPEG at 5 cm wide:")
    inline_picture(d, jpeg_bytes(300, 300, 12), 5)
    p = d.add_paragraph("Images inline with text: ")
    from docx.shared import Cm

    p.add_run().add_picture(io.BytesIO(png_bytes(64, 64, 13, "flat")), width=Cm(1))
    p.add_run(" between words ")
    p.add_run().add_picture(io.BytesIO(png_bytes(64, 64, 14, "flat")), width=Cm(1))
    p.add_run(" end.")
    save_docx(w, "images-inline", d, "Inline images")


ANCHOR_WRAPS = {
    "square": '<wp:wrapSquare wrapText="bothSides"/>',
    "tight": '<wp:wrapTight wrapText="bothSides"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="21600" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon></wp:wrapTight>',
    "through": '<wp:wrapThrough wrapText="bothSides"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="21600" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon></wp:wrapThrough>',
    "topAndBottom": "<wp:wrapTopAndBottom/>",
    "behind": "<wp:wrapNone/>",
    "inFront": "<wp:wrapNone/>",
}


def anchor_xml(rid: str, doc_pr_id: int, cx: int, cy: int, wrap: str, x: int, graphic: str | None = None) -> str:
    behind = "1" if wrap == "behind" else "0"
    if graphic is None:
        graphic = (
            '<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
            f'<pic:pic><pic:nvPicPr><pic:cNvPr id="{doc_pr_id}" name="image{doc_pr_id}.png"/><pic:cNvPicPr/></pic:nvPicPr>'
            f'<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
            f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
            '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData>'
        )
    return (
        '<w:drawing xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
        'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
        'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
        'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" '
        f'xmlns:wps="{WPS_NS}" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        f'<wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="{251659264 + doc_pr_id}" '
        f'behindDoc="{behind}" locked="0" layoutInCell="1" allowOverlap="1">'
        '<wp:simplePos x="0" y="0"/>'
        f'<wp:positionH relativeFrom="column"><wp:posOffset>{x}</wp:posOffset></wp:positionH>'
        '<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>'
        f'<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>'
        f"{ANCHOR_WRAPS[wrap]}"
        f'<wp:docPr id="{doc_pr_id}" name="Object {doc_pr_id}"/>'
        '<wp:cNvGraphicFramePr/>'
        f"<a:graphic>{graphic}</a:graphic></wp:anchor></w:drawing>"
    )


def docx_images_anchored(w: Writer) -> None:
    from docx.oxml import parse_xml

    rng = random.Random(7)
    d = new_document()
    d.add_heading("Anchored images: every wrap mode", 1)
    for i, wrap in enumerate(ANCHOR_WRAPS):
        d.add_heading(f"Wrap: {wrap}", 2)
        rid, _ = d.part.get_or_add_image(io.BytesIO(png_bytes(240, 160, 20 + i)))
        p = d.add_paragraph()
        r = p.add_run()
        r._r.append(parse_xml(anchor_xml(rid, 100 + i, 1828800, 1219200, wrap, 914400 * (i % 3))))
        p.add_run(lorem(rng, 90))
    save_docx(w, "images-anchored", d, "Anchored images")


def docx_text_boxes(w: Writer) -> None:
    from docx.oxml import parse_xml

    rng = random.Random(8)
    d = new_document()
    d.add_heading("Text boxes and shapes", 1)
    for i, (prst, fill) in enumerate([("rect", "DEEBF7"), ("roundRect", "FBE5D6"), ("ellipse", "E2F0D9")]):
        graphic = (
            f'<a:graphicData uri="{WPS_NS}"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr>'
            '<a:xfrm><a:off x="0" y="0"/><a:ext cx="2286000" cy="914400"/></a:xfrm>'
            f'<a:prstGeom prst="{prst}"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{fill}"/></a:solidFill>'
            '<a:ln w="12700"><a:solidFill><a:srgbClr val="2F5597"/></a:solidFill></a:ln></wps:spPr>'
            f'<wps:txbx><w:txbxContent><w:p><w:r><w:t>Text box {i + 1}: {prst}</w:t></w:r></w:p></w:txbxContent></wps:txbx>'
            '<wps:bodyPr rot="0" vert="horz" wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720" anchor="ctr" anchorCtr="0"><a:noAutofit/></wps:bodyPr>'
            "</wps:wsp></a:graphicData>"
        )
        p = d.add_paragraph()
        p.add_run()._r.append(parse_xml(anchor_xml("", 200 + i, 2286000, 914400, "square", 914400 * i, graphic)))
        p.add_run(lorem(rng, 70))
    save_docx(w, "text-boxes", d, "Text boxes")


def add_part(d, partname: str, content_type: str, reltype: str, xml: str):
    from docx.opc.packuri import PackURI
    from docx.opc.part import Part

    part = Part(PackURI(partname), content_type, xml.encode("utf-8"), d.part.package)
    rid = d.part.relate_to(part, reltype)
    return part, rid


def docx_footnotes(w: Writer) -> None:
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls

    rng = random.Random(9)
    d = new_document()
    d.add_heading("Footnotes and endnotes", 1)
    sep = (
        '<w:footnote w:type="separator" w:id="-1"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:separator/></w:r></w:p></w:footnote>'
        '<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>'
    )
    notes = "".join(
        f'<w:footnote w:id="{i}"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteRef/></w:r>'
        f'<w:r><w:t xml:space="preserve"> Footnote {i}: {lorem(rng, 8)}</w:t></w:r></w:p></w:footnote>'
        for i in range(1, 4)
    )
    add_part(
        d,
        "/word/footnotes.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes",
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:footnotes {nsdecls("w")}>{sep}{notes}</w:footnotes>',
    )
    esep = sep.replace("footnote", "endnote")
    enotes = "".join(
        f'<w:endnote w:id="{i}"><w:p><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:endnoteRef/></w:r>'
        f'<w:r><w:t xml:space="preserve"> Endnote {i}: {lorem(rng, 8)}</w:t></w:r></w:p></w:endnote>'
        for i in range(1, 3)
    )
    add_part(
        d,
        "/word/endnotes.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes",
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:endnotes {nsdecls("w")}>{esep}{enotes}</w:endnotes>',
    )
    for i in range(1, 4):
        p = d.add_paragraph(lorem(rng, 40))
        p._p.append(
            parse_xml(f'<w:r {nsdecls("w")}><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="{i}"/></w:r>')
        )
    for i in range(1, 3):
        p = d.add_paragraph(lorem(rng, 30))
        p._p.append(
            parse_xml(f'<w:r {nsdecls("w")}><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:endnoteReference w:id="{i}"/></w:r>')
        )
    save_docx(w, "footnotes", d, "Footnotes and endnotes")


def docx_fields(w: Writer) -> None:
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls

    rng = random.Random(10)
    d = new_document()
    d.add_heading("Fields", 1)
    # A TOC field with a cached result, as Word stores it.
    toc = d.add_paragraph()
    tp = toc._p
    tp.append(parse_xml(f'<w:r {nsdecls("w")}><w:fldChar w:fldCharType="begin" w:dirty="false"/></w:r>'))
    tp.append(parse_xml(f'<w:r {nsdecls("w")}><w:instrText xml:space="preserve"> TOC \\o "1-3" \\h \\z \\u </w:instrText></w:r>'))
    tp.append(parse_xml(f'<w:r {nsdecls("w")}><w:fldChar w:fldCharType="separate"/></w:r>'))
    tp.append(parse_xml(f'<w:r {nsdecls("w")}><w:t>Chapter One\t2</w:t></w:r>'))
    d.add_paragraph("Chapter Two\t2")
    end = d.add_paragraph()
    end._p.append(parse_xml(f'<w:r {nsdecls("w")}><w:fldChar w:fldCharType="end"/></w:r>'))
    for title in ["Chapter One", "Chapter Two"]:
        h = d.add_heading(title, 1)
        if title == "Chapter One":
            h._p.insert(1, parse_xml(f'<w:bookmarkStart {nsdecls("w")} w:id="0" w:name="chapter_one"/>'))
            h._p.append(parse_xml(f'<w:bookmarkEnd {nsdecls("w")} w:id="0"/>'))
        d.add_paragraph(lorem(rng, 40))
    p = d.add_paragraph()
    add_field_runs(p, "Page: ", "PAGE", "1")
    add_field_runs(p, " of ", "NUMPAGES", "1")
    p = d.add_paragraph()
    add_field_runs(p, "Date: ", 'DATE \\@ "yyyy-MM-dd"', "2026-01-15")
    p = d.add_paragraph()
    add_field_runs(p, "Cross-reference: ", "REF chapter_one \\h", "Chapter One")
    p = d.add_paragraph("Simple field: ")
    p._p.append(parse_xml(f'<w:fldSimple {nsdecls("w")} w:instr=" AUTHOR "><w:r><w:t>{AUTHOR}</w:t></w:r></w:fldSimple>'))
    save_docx(w, "fields", d, "Fields")


def docx_comments(w: Writer) -> None:
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls

    rng = random.Random(11)
    d = new_document()
    d.add_heading("Comments", 1)
    comments = []
    for i in range(3):
        p = d.add_paragraph(lorem(rng, 12) + " ")
        p._p.append(parse_xml(f'<w:commentRangeStart {nsdecls("w")} w:id="{i}"/>'))
        p.add_run(f"commented text {i + 1}")
        p._p.append(parse_xml(f'<w:commentRangeEnd {nsdecls("w")} w:id="{i}"/>'))
        p._p.append(
            parse_xml(f'<w:r {nsdecls("w")}><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="{i}"/></w:r>')
        )
        p.add_run(" " + lorem(rng, 10))
        comments.append(
            f'<w:comment w:id="{i}" w:author="Reviewer {i % 2 + 1}" w:date="2026-01-1{i + 1}T10:00:00Z" w:initials="R{i % 2 + 1}">'
            f'<w:p><w:r><w:annotationRef/></w:r><w:r><w:t>Comment number {i + 1}: {lorem(rng, 6)}</w:t></w:r></w:p></w:comment>'
        )
    add_part(
        d,
        "/word/comments.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:comments {nsdecls("w")}>{"".join(comments)}</w:comments>',
    )
    save_docx(w, "comments", d, "Comments")


def docx_tracked_changes(w: Writer) -> None:
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls

    d = new_document()
    d.add_heading("Tracked changes", 1)
    attrs = 'w:author="Editor" w:date="2026-01-15T09:30:00Z"'
    p = d.add_paragraph("Unchanged start. ")
    p._p.append(parse_xml(f'<w:ins {nsdecls("w")} w:id="1" {attrs}><w:r><w:t xml:space="preserve">Inserted words. </w:t></w:r></w:ins>'))
    p._p.append(parse_xml(f'<w:del {nsdecls("w")} w:id="2" {attrs}><w:r><w:delText xml:space="preserve">Deleted words. </w:delText></w:r></w:del>'))
    p.add_run("Unchanged end.")
    p = d.add_paragraph()
    p._p.append(
        parse_xml(
            f'<w:r {nsdecls("w")}><w:rPr><w:b/><w:rPrChange w:id="3" {attrs}><w:rPr/></w:rPrChange></w:rPr><w:t>Formatting change: made bold.</w:t></w:r>'
        )
    )
    p = d.add_paragraph("Paragraph property change: centred.")
    p._p.get_or_add_pPr().append(
        parse_xml(f'<w:pPrChange {nsdecls("w")} w:id="4" {attrs}><w:pPr/></w:pPrChange>')
    )
    p._p.get_or_add_pPr().insert(0, parse_xml(f'<w:jc {nsdecls("w")} w:val="center"/>'))
    d.settings.element.append(parse_xml(f'<w:trackRevisions {nsdecls("w")}/>'))
    save_docx(w, "tracked-changes", d, "Tracked changes")


def docx_rtl(w: Writer) -> None:
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls

    d = new_document()
    d.add_heading("Right-to-left text", 1)
    texts = [
        "هذه فقرة باللغة العربية لاختبار اتجاه النص من اليمين إلى اليسار.",
        "זוהי פסקה בעברית לבדיקת כיוון הטקסט מימין לשמאל.",
        "مختلط: Kiln 2026 مع أرقام 12345 ونص لاتيني.",
    ]
    for t in texts:
        p = d.add_paragraph()
        p._p.get_or_add_pPr().append(parse_xml(f'<w:bidi {nsdecls("w")}/>'))
        p._p.get_or_add_pPr().append(parse_xml(f'<w:jc {nsdecls("w")} w:val="right"/>'))
        r = p.add_run(t)
        rpr = r._r.get_or_add_rPr()
        rpr.append(parse_xml(f'<w:rFonts {nsdecls("w")} w:cs="Arial"/>'))
        rpr.append(parse_xml(f'<w:rtl {nsdecls("w")}/>'))
    d.add_paragraph("A left-to-right paragraph for contrast.")
    save_docx(w, "rtl", d, "Right-to-left text")


def docx_hyperlinks(w: Writer) -> None:
    from docx.opc.constants import RELATIONSHIP_TYPE as RT
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls

    d = new_document()
    d.add_heading("Hyperlinks and bookmarks", 1)
    for i, url in enumerate(["https://example.org/", "https://www.ecma-international.org/publications-and-standards/standards/ecma-376/", "mailto:someone@example.org"]):
        rid = d.part.relate_to(url, RT.HYPERLINK, is_external=True)
        p = d.add_paragraph(f"Link {i + 1}: ")
        p._p.append(
            parse_xml(
                f'<w:hyperlink {nsdecls("w", "r")} r:id="{rid}" w:history="1"><w:r><w:rPr><w:rStyle w:val="Hyperlink"/>'
                f'<w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr><w:t>{url}</w:t></w:r></w:hyperlink>'
            )
        )
    p = d.add_paragraph("Internal link: ")
    p._p.append(
        parse_xml(
            f'<w:hyperlink {nsdecls("w")} w:anchor="target"><w:r><w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr><w:t>jump to target</w:t></w:r></w:hyperlink>'
        )
    )
    for _ in range(20):
        d.add_paragraph("Filler paragraph.")
    p = d.add_paragraph()
    p._p.append(parse_xml(f'<w:bookmarkStart {nsdecls("w")} w:id="1" w:name="target"/>'))
    p.add_run("Bookmark target")
    p._p.append(parse_xml(f'<w:bookmarkEnd {nsdecls("w")} w:id="1"/>'))
    save_docx(w, "hyperlinks", d, "Hyperlinks")


def docx_page_setup(w: Writer) -> None:
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls
    from docx.shared import Cm, Mm

    rng = random.Random(12)
    d = new_document()
    s = d.sections[0]
    s.page_width, s.page_height = Mm(215.9), Mm(355.6)  # US Legal
    s.left_margin, s.right_margin = Cm(3.18), Cm(3.18)
    s.top_margin, s.bottom_margin = Cm(2.54), Cm(2.54)
    s.header_distance = s.footer_distance = Cm(1.25)
    s._sectPr.append(parse_xml(f'<w:lnNumType {nsdecls("w")} w:countBy="5" w:restart="newPage"/>'))
    s._sectPr.append(parse_xml(f'<w:pgNumType {nsdecls("w")} w:fmt="lowerRoman" w:start="3"/>'))
    s._sectPr.append(
        parse_xml(
            f'<w:pgBorders {nsdecls("w")} w:offsetFrom="page"><w:top w:val="single" w:sz="12" w:space="24" w:color="4472C4"/>'
            '<w:left w:val="single" w:sz="12" w:space="24" w:color="4472C4"/><w:bottom w:val="single" w:sz="12" w:space="24" w:color="4472C4"/>'
            '<w:right w:val="single" w:sz="12" w:space="24" w:color="4472C4"/></w:pgBorders>'
        )
    )
    d.add_heading("Page setup: US Legal, line numbers, page borders", 1)
    for _ in range(8):
        d.add_paragraph(lorem(rng, 60))
    save_docx(w, "page-setup", d, "Page setup")


def docx_breaks(w: Writer) -> None:
    from docx.enum.text import WD_BREAK

    rng = random.Random(13)
    d = new_document()
    d.add_heading("Breaks and special characters", 1)
    p = d.add_paragraph("Line one")
    p.add_run().add_break(WD_BREAK.LINE)
    p.add_run("Line two after a manual line break")
    d.add_paragraph("Before a page break.")
    d.add_paragraph().add_run().add_break(WD_BREAK.PAGE)
    d.add_paragraph("After the page break.")
    d.add_paragraph("Special characters: non breaking space, soft­hyphen, en – dash, em — dash, “quotes”, ‘single’, …, €, ©, ®, ™, ½, α β γ, 漢字, 😀.")
    p = d.add_paragraph("Tabs:\tone\ttwo\tthree")
    from docx.oxml import parse_xml
    from docx.oxml.ns import nsdecls

    p = d.add_paragraph("Symbol characters: ")
    p._p.append(parse_xml(f'<w:r {nsdecls("w")}><w:sym w:font="Wingdings" w:char="F04A"/></w:r>'))
    p._p.append(parse_xml(f'<w:r {nsdecls("w")}><w:sym w:font="Symbol" w:char="F061"/></w:r>'))
    for _ in range(3):
        d.add_paragraph(lorem(rng, 40))
    save_docx(w, "breaks", d, "Breaks")


def docx_long_text(w: Writer) -> None:
    rng = random.Random(14)
    d = new_document()
    d.add_heading("Long document", 0)
    for ch in range(1, 6):
        d.add_heading(f"Chapter {ch}", 1)
        for sec in range(1, 4):
            d.add_heading(f"Section {ch}.{sec}", 2)
            for _ in range(rng.randrange(3, 6)):
                d.add_paragraph(lorem(rng, rng.randrange(40, 120)))
    save_docx(w, "long-text", d, "Long document")


def docx_properties(w: Writer) -> None:
    d = new_document()
    cp = d.core_properties
    cp.subject = "Fixture subject"
    cp.keywords = "kiln; fixture; properties"
    cp.comments = "Description of the document"
    cp.category = "Testing"
    cp.content_status = "Draft"
    cp.language = "en-GB"
    cp.version = "1.0"
    cp.identifier = "urn:kiln:fixture:properties"
    d.add_heading("Document properties", 1)
    d.add_paragraph("Core properties are set; custom properties are added below.")

    custom = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" '
        'xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">'
        '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="Project"><vt:lpwstr>Kiln</vt:lpwstr></property>'
        '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="3" name="Build"><vt:i4>42</vt:i4></property>'
        '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="4" name="Ratio"><vt:r8>0.75</vt:r8></property>'
        '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="5" name="Reviewed"><vt:bool>true</vt:bool></property>'
        '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="6" name="Due"><vt:filetime>2026-02-01T00:00:00Z</vt:filetime></property>'
        "</Properties>"
    )

    def rewrite(name: str, body: bytes):
        if name == "+":
            return [("docProps/custom.xml", custom.encode("utf-8"))]
        if name == "[Content_Types].xml":
            return body.replace(
                b"</Types>",
                b'<Override PartName="/docProps/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/></Types>',
            )
        if name == "_rels/.rels":
            return body.replace(
                b"</Relationships>",
                b'<Relationship Id="rIdCustom" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties" Target="docProps/custom.xml"/></Relationships>',
            )
        return body

    save_docx(w, "properties", d, "Document properties", rewrite)


def make_docx(out: Path) -> int:
    w = Writer(out, "docx")
    for fn in [
        docx_styles,
        docx_paragraphs,
        docx_numbering,
        docx_tables,
        docx_sections,
        docx_headers_footers,
        docx_images_inline,
        docx_images_anchored,
        docx_text_boxes,
        docx_footnotes,
        docx_fields,
        docx_comments,
        docx_tracked_changes,
        docx_rtl,
        docx_hyperlinks,
        docx_page_setup,
        docx_breaks,
        docx_long_text,
        docx_properties,
    ]:
        fn(w)
    return w.count


# ----------------------------------------------------------------------------- xlsx


def new_workbook():
    import openpyxl

    wb = openpyxl.Workbook()
    wb.properties.creator = AUTHOR
    wb.properties.lastModifiedBy = AUTHOR
    wb.properties.created = FIXED_DATE
    wb.properties.modified = FIXED_DATE
    return wb


def save_xlsx(w: Writer, name: str, wb, title: str, rewrite=None) -> None:
    wb.properties.title = title
    buf = io.BytesIO()
    wb.save(buf)
    w.save(name, buf.getvalue(), rewrite)


NUMBER_FORMATS = [
    ("General", "General", 1234.5678),
    ("Number", "0", 1234.5678),
    ("Number", "0.00", 1234.5678),
    ("Number", "#,##0", 1234567.891),
    ("Number", "#,##0.00", -1234567.891),
    ("Number", "#,##0.00;[Red]-#,##0.00", -42.5),
    ("Currency", '"$"#,##0.00', 1234.5),
    ("Currency", '[$€-x-euro2] #,##0.00', 1234.5),
    ("Currency", '"£"#,##0.00;[Red]-"£"#,##0.00', -99.95),
    ("Accounting", '_-* #,##0.00_-;-* #,##0.00_-;_-* "-"??_-;_-@_-', 1234.5),
    ("Accounting", '_("$"* #,##0.00_);_("$"* (#,##0.00);_("$"* "-"??_);_(@_)', 0),
    ("Date", "m/d/yyyy", 46037),
    ("Date", "d-mmm-yy", 46037),
    ("Date", "yyyy-mm-dd", 46037),
    ("Date", "dddd, mmmm d, yyyy", 46037),
    ("Date", "mmm-yy", 46037),
    ("Time", "h:mm AM/PM", 0.396),
    ("Time", "h:mm:ss", 0.396),
    ("Time", "[h]:mm:ss", 1.75),
    ("Time", "m/d/yyyy h:mm", 46037.396),
    ("Percentage", "0%", 0.256),
    ("Percentage", "0.00%", 0.25678),
    ("Fraction", "# ?/?", 1.25),
    ("Fraction", "# ??/??", 3.14159),
    ("Fraction", "# ?/8", 0.625),
    ("Scientific", "0.00E+00", 123456789),
    ("Scientific", "##0.0E+0", 0.000123),
    ("Text", "@", "00123"),
    ("Special", "00000", 2134),
    ("Special", "[<=9999999]###-####;(###) ###-####", 5551234567),
    ("Custom", '[>=1000000]#,##0.0,,"M";[>=1000]#,##0.0,"K";0', 2500000),
    ("Custom", '[Blue]#,##0;[Red]-#,##0;[Green]"zero"', 0),
    ("Custom", '"Total: "#,##0', 512),
    ("Custom", "0.000", 3.14159),
    ("Custom", "#,##0_);(#,##0)", -1500),
    ("Custom", ";;;", "hidden"),
]


def xlsx_number_formats(w: Writer) -> None:
    from openpyxl.styles import Font

    wb = new_workbook()
    ws = wb.active
    ws.title = "Formats"
    ws.append(["Category", "Format code", "Value", "Formatted"])
    for c in ws[1]:
        c.font = Font(bold=True)
    for i, (cat, code, value) in enumerate(NUMBER_FORMATS, start=2):
        ws.cell(i, 1, cat)
        ws.cell(i, 2, code)
        ws.cell(i, 3, value)
        cell = ws.cell(i, 4, value)
        cell.number_format = code
    ws.column_dimensions["A"].width = 14
    ws.column_dimensions["B"].width = 48
    ws.column_dimensions["D"].width = 26
    save_xlsx(w, "number-formats", wb, "Number formats")


def xlsx_styles(w: Writer) -> None:
    from openpyxl.styles import Alignment, Border, Font, GradientFill, PatternFill, Protection, Side

    wb = new_workbook()
    ws = wb.active
    ws.title = "Styles"
    fonts = [
        Font(bold=True),
        Font(italic=True),
        Font(underline="single"),
        Font(underline="double"),
        Font(strike=True),
        Font(size=16),
        Font(size=8),
        Font(color="FFC00000"),
        Font(name="Cambria"),
        Font(name="Courier New", bold=True, italic=True),
        Font(vertAlign="superscript"),
        Font(vertAlign="subscript"),
    ]
    fills = [
        PatternFill("solid", fgColor="FFFFFF00"),
        PatternFill("solid", fgColor="FFDDEBF7"),
        PatternFill("darkGrid", fgColor="FF4472C4", bgColor="FFFFFFFF"),
        PatternFill("lightTrellis", fgColor="FFED7D31"),
        PatternFill("gray125"),
        GradientFill(stop=("FF4472C4", "FFFFFFFF"), degree=90),
        GradientFill(type="path", stop=("FFFFFFFF", "FF70AD47"), left=0.5, right=0.5, top=0.5, bottom=0.5),
    ]
    sides = ["thin", "medium", "thick", "dashed", "dotted", "double", "hair", "mediumDashDot", "slantDashDot"]
    borders = [Border(left=Side(s), right=Side(s), top=Side(s), bottom=Side(s)) for s in sides]
    borders.append(Border(diagonal=Side("thin", color="FFFF0000"), diagonalUp=True, diagonalDown=True))
    aligns = [
        Alignment(horizontal="left"),
        Alignment(horizontal="center"),
        Alignment(horizontal="right"),
        Alignment(horizontal="justify", wrap_text=True),
        Alignment(vertical="top"),
        Alignment(vertical="center"),
        Alignment(vertical="bottom"),
        Alignment(wrap_text=True),
        Alignment(text_rotation=45),
        Alignment(text_rotation=90),
        Alignment(indent=2),
        Alignment(shrink_to_fit=True),
        Alignment(horizontal="centerContinuous"),
        Alignment(horizontal="distributed"),
    ]
    combos = []
    for f in fonts:
        combos.append(("font", f, None, None, None))
    for f in fills:
        combos.append(("fill", None, f, None, None))
    for b in borders:
        combos.append(("border", None, None, b, None))
    for a in aligns:
        combos.append(("align", None, None, None, a))
    ws.append(["#", "Kind", "Sample"])
    for i, (kind, f, fl, b, a) in enumerate(combos, start=1):
        row = i + 1
        ws.cell(row, 1, i)
        ws.cell(row, 2, kind)
        c = ws.cell(row, 3, f"Style {i}: the quick brown fox")
        if f:
            c.font = f
        if fl:
            c.fill = fl
        if b:
            c.border = b
        if a:
            c.alignment = a
        ws.row_dimensions[row].height = 30
    unlocked = ws.cell(len(combos) + 3, 3, "Unlocked and hidden-formula cell")
    unlocked.protection = Protection(locked=False, hidden=True)
    ws.column_dimensions["C"].width = 40
    save_xlsx(w, "styles", wb, "Cell styles")


def xlsx_merges(w: Writer) -> None:
    from openpyxl.styles import Alignment

    wb = new_workbook()
    ws = wb.active
    ws.title = "Merges"
    for r in range(1, 21):
        for c in range(1, 11):
            ws.cell(r, c, r * 100 + c)
    for rng, label in [("A1:D1", "Merged across"), ("A3:A8", "Down"), ("C3:F6", "Block"), ("H10:J12", "Another block"), ("B15:B16", "Two")]:
        ws.merge_cells(rng)
        cell = ws[rng.split(":")[0]]
        cell.value = label
        cell.alignment = Alignment(horizontal="center", vertical="center")
    save_xlsx(w, "merges", wb, "Merged cells")


def xlsx_freeze_panes(w: Writer) -> None:
    wb = new_workbook()
    specs = [("Rows", "A2"), ("Columns", "B1"), ("Both", "C3"), ("None", None)]
    for i, (title, freeze) in enumerate(specs):
        ws = wb.active if i == 0 else wb.create_sheet()
        ws.title = title
        for r in range(1, 61):
            for c in range(1, 16):
                ws.cell(r, c, f"{title[0]}{r}:{c}" if r == 1 or c == 1 else r * c)
        ws.freeze_panes = freeze
    ws = wb.create_sheet("Split")
    for r in range(1, 41):
        ws.append([r * c for c in range(1, 12)])
    ws.sheet_view.pane = None
    from openpyxl.worksheet.views import Pane

    ws.sheet_view.pane = Pane(xSplit=2400, ySplit=1800, topLeftCell="D6", activePane="bottomRight", state="split")
    ws2 = wb.create_sheet("Zoomed")
    ws2.sheet_view.zoomScale = 150
    ws2.sheet_view.showGridLines = False
    ws2["B2"] = "Zoom 150 %, gridlines hidden"
    save_xlsx(w, "freeze-panes", wb, "Freeze panes")


FORMULAS = {
    "Math": [
        "SUM(Data!A1:A20)", 'SUMIF(Data!A1:A20,">10")', 'SUMIFS(Data!A1:A20,Data!A1:A20,">5",Data!A1:A20,"<15")',
        "SUMPRODUCT(Data!A1:A5,Data!A6:A10)", "ROUND(PI(),2)", "ROUNDUP(2.341,1)", "ROUNDDOWN(2.349,2)", "MOD(17,5)",
        "INT(-3.7)", "TRUNC(-3.7)", "ABS(-5)", "SQRT(16)", "POWER(2,10)", "EXP(1)", "LN(10)", "LOG(1000)", "LOG10(100)",
        "LOG(8,2)", "PI()", "CEILING(4.3,1)", "FLOOR(4.7,1)", "MROUND(10,3)", "SIGN(-2)", "FACT(5)", "COMBIN(5,2)",
        "GCD(12,18)", "LCM(4,6)", "SUBTOTAL(9,Data!A1:A20)", "SUBTOTAL(1,Data!A1:A20)", "AGGREGATE(9,6,Data!A1:A20)",
        "PRODUCT(1,2,3,4)", "QUOTIENT(17,5)", "EVEN(3)", "ODD(4)", "RADIANS(180)", "DEGREES(PI())", "SIN(PI()/2)",
        "COS(0)", "TAN(PI()/4)", "ASIN(1)", "ACOS(0)", "ATAN(1)", "ATAN2(1,1)", "SINH(1)", "COSH(1)", "TANH(1)",
        "SUMSQ(1,2,3)", "ROMAN(1999)", '_xlfn.ARABIC("MCMXCIX")', "MULTINOMIAL(2,3)", "_xlfn.CEILING.MATH(4.3)",
        "_xlfn.FLOOR.MATH(-4.3)", "SQRTPI(1)", "FACTDOUBLE(6)", "PERMUT(5,2)",
    ],
    "Statistical": [
        "AVERAGE(Data!A1:A20)", 'AVERAGEIF(Data!A1:A20,">10")', 'AVERAGEIFS(Data!A1:A20,Data!A1:A20,">2",Data!A1:A20,"<18")',
        "COUNT(Data!A1:C20)", "COUNTA(Data!A1:C20)", "COUNTBLANK(Data!D1:D20)", 'COUNTIF(Data!B1:B20,"a*")',
        'COUNTIFS(Data!A1:A20,">5",Data!B1:B20,"<>")', "MIN(Data!A1:A20)", "MAX(Data!A1:A20)", "MEDIAN(Data!A1:A20)",
        "MODE(1,2,2,3)", "_xlfn.MODE.SNGL(1,2,2,3)", "STDEV(Data!A1:A20)", "_xlfn.STDEV.S(Data!A1:A20)",
        "_xlfn.STDEV.P(Data!A1:A20)", "VAR(Data!A1:A20)", "_xlfn.VAR.S(Data!A1:A20)", "_xlfn.VAR.P(Data!A1:A20)",
        "LARGE(Data!A1:A20,2)", "SMALL(Data!A1:A20,2)", "RANK(5,Data!A1:A20)", "_xlfn.RANK.EQ(5,Data!A1:A20)",
        "PERCENTILE(Data!A1:A20,0.9)", "_xlfn.PERCENTILE.INC(Data!A1:A20,0.9)", "QUARTILE(Data!A1:A20,1)",
        "_xlfn.QUARTILE.INC(Data!A1:A20,3)", "CORREL(Data!A1:A10,Data!A11:A20)", "FORECAST(5,Data!A1:A10,Data!A11:A20)",
        "_xlfn.NORM.DIST(1,0,1,TRUE)", "NORMSDIST(1)", "_xlfn.NORM.INV(0.9,0,1)", "_xlfn.T.DIST(1,10,TRUE)",
        "MAXA(Data!A1:A20)", "MINA(Data!A1:A20)", "AVERAGEA(Data!A1:A20)", "GEOMEAN(1,2,4)", "HARMEAN(1,2,4)",
        "DEVSQ(Data!A1:A20)", "SLOPE(Data!A1:A10,Data!A11:A20)", "INTERCEPT(Data!A1:A10,Data!A11:A20)",
        "RSQ(Data!A1:A10,Data!A11:A20)", '_xlfn.MAXIFS(Data!A1:A20,Data!A1:A20,"<10")',
        '_xlfn.MINIFS(Data!A1:A20,Data!A1:A20,">10")', "STANDARDIZE(5,10,2)", "_xlfn.PERCENTRANK.INC(Data!A1:A20,7)",
        "KURT(Data!A1:A20)", "SKEW(Data!A1:A20)", "AVEDEV(Data!A1:A20)", "COVAR(Data!A1:A10,Data!A11:A20)",
    ],
    "Logical": [
        'IF(Data!A1>5,"big","small")', "AND(TRUE,Data!A2>0)", "OR(FALSE,Data!A3>100)", "NOT(Data!A4>3)",
        "_xlfn.XOR(TRUE,FALSE)", "IFERROR(1/0,\"div0\")", '_xlfn.IFNA(NA(),"na")',
        '_xlfn.IFS(Data!A1>15,"high",Data!A1>5,"mid",TRUE,"low")', '_xlfn.SWITCH(Data!A2,1,"one",2,"two","other")',
        "TRUE()", "FALSE()", 'IF(AND(Data!A1>0,Data!A2>0),"both","no")', 'IF(Data!A1>10,IF(Data!A1>15,"A","B"),"C")',
        "IFERROR(VLOOKUP(999,Data!A1:B20,2,FALSE),-1)", 'IF(ISBLANK(Data!D1),"empty","full")',
        "NOT(OR(Data!A1>100,Data!A2>100))", 'IF(Data!A3="","blank",Data!A3*2)', "AND(Data!A1:A5>0)",
    ],
    "Text": [
        'CONCATENATE("Offi","cina")', '_xlfn.CONCAT("A","B","C")', 'Data!B1&" "&Data!B2', 'TEXT(1234.5,"#,##0.00")',
        'TEXT(46037,"yyyy-mm-dd")', '_xlfn.TEXTJOIN(", ",TRUE,Data!B1:B5)', 'LEFT("Kiln",4)', 'RIGHT("Kiln",3)',
        'MID("Kiln",3,4)', 'LEN("Kiln")', 'FIND("c","Kiln")', 'SEARCH("I","Kiln")',
        'SUBSTITUTE("a-b-c","-","+")', 'REPLACE("Kiln",1,1,"o")', 'TRIM("  spaced   out  ")', "CLEAN(CHAR(7)&\"x\")",
        'UPPER("kiln")', 'LOWER("KILN")', 'PROPER("hello world")', 'VALUE("42")',
        '_xlfn.NUMBERVALUE("1.234,5",",",".")', 'REPT("ab",3)', "CHAR(65)", 'CODE("A")', "_xlfn.UNICHAR(9731)",
        '_xlfn.UNICODE("A")', 'EXACT("a","A")', "DOLLAR(1234.567,2)", "FIXED(1234.567,1)", 'T("text")',
        "LEN(Data!B3)", 'LEFT(Data!B4,2)&"…"', 'UPPER(Data!B5)', 'SUBSTITUTE(Data!B6,"a","A")', 'TEXT(0.256,"0.0%")',
        'TEXT(Data!C1,"mmmm")',
    ],
    "Date and time": [
        "DATE(2026,1,15)", 'DATEVALUE("2026-01-15")', "DAY(Data!C1)", "MONTH(Data!C1)", "YEAR(Data!C1)",
        "HOUR(0.396)", "MINUTE(0.396)", "SECOND(0.3961)", "TIME(9,30,0)", "WEEKDAY(Data!C1)", "WEEKNUM(Data!C1)",
        "_xlfn.ISOWEEKNUM(Data!C1)", "EOMONTH(Data!C1,1)", "EDATE(Data!C1,3)", "NETWORKDAYS(Data!C1,Data!C10)",
        "_xlfn.NETWORKDAYS.INTL(Data!C1,Data!C10,11)", "WORKDAY(Data!C1,10)", "_xlfn.WORKDAY.INTL(Data!C1,10,1)",
        'DATEDIF(Data!C1,Data!C10,"d")', "_xlfn.DAYS(Data!C10,Data!C1)", "DAYS360(Data!C1,Data!C10)",
        "YEARFRAC(Data!C1,Data!C10)", 'TIMEVALUE("9:30 AM")', "DATE(2024,2,29)+365", "Data!C2-Data!C1",
        "EOMONTH(DATE(2024,1,31),1)", "WEEKDAY(DATE(2026,1,15),2)", 'DATEDIF(DATE(2000,1,1),DATE(2026,1,15),"y")',
    ],
    "Lookup and reference": [
        "VLOOKUP(5,Data!A1:B20,2,FALSE)", "VLOOKUP(5.5,Data!A1:B20,2,TRUE)", "HLOOKUP(\"Qty\",Data!F1:H5,3,FALSE)",
        "_xlfn.XLOOKUP(7,Data!A1:A20,Data!B1:B20)", '_xlfn.XLOOKUP(99,Data!A1:A20,Data!B1:B20,"none")',
        "_xlfn.XMATCH(7,Data!A1:A20)", "LOOKUP(7,Data!A1:A20,Data!B1:B20)", "INDEX(Data!A1:C20,3,2)",
        "MATCH(7,Data!A1:A20,0)", "INDEX(Data!B1:B20,MATCH(9,Data!A1:A20,0))", "OFFSET(Data!A1,2,1)",
        'INDIRECT("Data!A5")', 'CHOOSE(2,"a","b","c")', "ROW()", "ROWS(Data!A1:A20)", "COLUMN()",
        "COLUMNS(Data!A1:C1)", "ADDRESS(2,3)", 'ADDRESS(2,3,4,TRUE,"Data")', 'HYPERLINK("https://example.org","link")',
        "SUM(OFFSET(Data!A1,0,0,5,1))", "INDEX(Data!A1:A20,ROWS(Data!A1:A20))", "MATCH(\"c*\",Data!B1:B20,0)",
        "SUM(INDIRECT(\"Data!A1:A\"&5))", "ROW(Data!A10)",
    ],
    "Information": [
        "ISBLANK(Data!D1)", "ISNUMBER(Data!A1)", "ISTEXT(Data!B1)", "ISERROR(1/0)", "ISERR(NA())", "ISNA(NA())",
        "ISLOGICAL(TRUE)", "_xlfn.ISFORMULA(A2)", "ISEVEN(4)", "ISODD(4)", "ISREF(Data!A1)", "N(TRUE)", "NA()",
        "TYPE(\"x\")", "ERROR.TYPE(1/0)", "_xlfn.SHEET()", "_xlfn.SHEETS()", 'CELL("row",Data!A5)',
        "ISNONTEXT(Data!A1)", "TYPE(Data!A1:A2)",
    ],
    "Financial": [
        "PMT(0.05/12,360,250000)", "IPMT(0.05/12,1,360,250000)", "PPMT(0.05/12,1,360,250000)", "PV(0.05/12,360,-1342)",
        "FV(0.05/12,120,-200)", "NPV(0.1,-10000,3000,4200,6800)", "XNPV(0.09,Data!J1:J5,Data!K1:K5)",
        "IRR(Data!J1:J5)", "XIRR(Data!J1:J5,Data!K1:K5)", "RATE(48,-200,8000)", "NPER(0.01,-100,1000)",
        "SLN(30000,7500,10)", "DB(1000000,100000,6,1,7)", "DDB(2400,300,10,1)", "SYD(30000,7500,10,1)",
        "EFFECT(0.0525,4)", "NOMINAL(0.053543,4)", "CUMIPMT(0.09/12,360,125000,13,24,0)",
        "CUMPRINC(0.09/12,360,125000,13,24,0)", "FV(0.06/12,10,-200,-500,1)", "PMT(0.08/12,10,10000,0,1)",
        "MIRR(Data!J1:J5,0.1,0.12)", "ISPMT(0.1/12,1,36,8000000)", "PDURATION(0.025,2000,2200)", "RRI(96,10000,11000)",
    ],
    "Engineering": [
        'CONVERT(1,"lbm","kg")', 'CONVERT(68,"F","C")', "DEC2BIN(10)", 'BIN2DEC("1010")', "DEC2HEX(255)",
        'HEX2DEC("FF")', "DEC2OCT(64)", 'OCT2DEC("100")', "_xlfn.BITAND(13,25)", "_xlfn.BITOR(13,25)",
        "_xlfn.BITXOR(13,25)", "COMPLEX(3,4)", 'IMREAL("3+4i")', 'IMAGINARY("3+4i")', 'IMABS("3+4i")', "DELTA(5,5)",
        "GESTEP(5,4)", "ERF(1)", 'IMSUM("3+4i","1-2i")', "_xlfn.BITLSHIFT(4,2)",
    ],
    "Database": [
        'DSUM(Data!F1:H8,"Qty",Data!F10:F11)', 'DAVERAGE(Data!F1:H8,"Qty",Data!F10:F11)',
        'DCOUNT(Data!F1:H8,"Qty",Data!F10:F11)', 'DCOUNTA(Data!F1:H8,"Item",Data!F10:F11)',
        'DGET(Data!F1:H8,"Price",Data!F13:F14)', 'DMAX(Data!F1:H8,"Qty",Data!F10:F11)',
        'DMIN(Data!F1:H8,"Qty",Data!F10:F11)', 'DPRODUCT(Data!F1:H8,"Qty",Data!F10:F11)',
        'DSTDEV(Data!F1:H8,"Qty",Data!F10:F11)', 'DSTDEVP(Data!F1:H8,"Qty",Data!F10:F11)',
        'DVAR(Data!F1:H8,"Qty",Data!F10:F11)', 'DVARP(Data!F1:H8,"Qty",Data!F10:F11)',
    ],
}


def fill_data_sheet(ws, rng: random.Random) -> None:
    import datetime as _dt

    words = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa"]
    for i in range(1, 21):
        ws.cell(i, 1, i)
        ws.cell(i, 2, words[(i - 1) % len(words)] + ("" if i <= 10 else str(i)))
        ws.cell(i, 3, _dt.datetime(2026, 1, 15) + _dt.timedelta(days=7 * (i - 1)))
        ws.cell(i, 3).number_format = "yyyy-mm-dd"
    db = [("Item", "Qty", "Price"), ("Apple", 10, 0.5), ("Pear", 4, 0.75), ("Apple", 7, 0.55), ("Plum", 12, 0.3), ("Pear", 9, 0.8), ("Apple", 3, 0.6), ("Fig", 6, 1.2)]
    for r, row in enumerate(db, start=1):
        for c, v in enumerate(row, start=6):
            ws.cell(r, c, v)
    ws["F10"], ws["F11"] = "Item", "Apple"
    ws["F13"], ws["F14"] = "Item", "Fig"
    flows = [-10000, 2750, 4250, 3250, 2750]
    for i, v in enumerate(flows, start=1):
        ws.cell(i, 10, v)
        ws.cell(i, 11, _dt.datetime(2026, 1, 1) + _dt.timedelta(days=int(365 * (i - 1) * 0.6)))
        ws.cell(i, 11).number_format = "yyyy-mm-dd"


def xlsx_formulas(w: Writer) -> int:
    from openpyxl.styles import Font

    wb = new_workbook()
    ws = wb.active
    ws.title = "Formulas"
    data = wb.create_sheet("Data")
    fill_data_sheet(data, random.Random(3))
    ws.append(["Family", "Formula", "Result"])
    for c in ws[1]:
        c.font = Font(bold=True)
    formulas = [(fam, f) for fam, fs in FORMULAS.items() for f in fs]
    # Operators and references make up the remainder of the 300.
    extra = []
    for i in range(1, 301):
        if len(formulas) + len(extra) >= 300:
            break
        a, b = (i % 20) + 1, ((i * 7) % 20) + 1
        templates = [
            f"Data!A{a}+Data!A{b}*2",
            f"(Data!A{a}-Data!A{b})/2",
            f"Data!A{a}^2",
            f"Data!A{a}>=Data!A{b}",
            f"Data!A{a}&\"-\"&Data!B{b}",
            f"Data!A{a}*10%",
            f"SUM(Data!$A$1:Data!A{a})",
            f"-Data!A{a}",
            f"SUM(Data!A1:A{a} Data!A{b}:A20)",
            f"SUM((Data!A{min(a, b)}:A{max(a, b)}),Data!A1)",
        ]
        extra.append(("Operators and references", templates[i % len(templates)]))
    formulas += extra
    for row, (fam, f) in enumerate(formulas, start=2):
        ws.cell(row, 1, fam)
        shown = ws.cell(row, 2)
        shown.value = "=" + f.replace("_xlfn._xlws.", "").replace("_xlfn.", "")
        shown.data_type = "s"  # the formula as text, next to the live formula
        ws.cell(row, 3, "=" + f)
    ws.column_dimensions["A"].width = 24
    ws.column_dimensions["B"].width = 60
    save_xlsx(w, "formulas", wb, "Formulas")
    return len(formulas)


def xlsx_shared_array_formulas(w: Writer) -> None:
    from openpyxl.worksheet.formula import ArrayFormula

    wb = new_workbook()
    ws = wb.active
    ws.title = "Formulas"
    for i in range(1, 11):
        ws.cell(i, 1, i * 3 % 11)
        ws.cell(i, 2, f"=A{i}*2")  # rewritten into one shared formula below
    ws["C1"] = ArrayFormula("C1:C3", "=A1:A3*10")
    ws["E1"] = ArrayFormula("E1", "=SUM(A1:A10*A1:A10)")
    ws["G1"] = "=_xlfn._xlws.SORT(A1:A10)"  # dynamic array, marked with cm="1" below
    ws["H1"] = "=_xlfn.UNIQUE(A1:A10)"
    ws["I1"] = "=_xlfn.SEQUENCE(5,1,1,2)"
    sheet = "xl/worksheets/sheet1.xml"

    def rewrite(name: str, body: bytes):
        if name == sheet:
            xml = body.decode("utf-8")
            # Shared formula: B1 is the master, B2:B10 reference it.
            xml = xml.replace("<c r=\"B1\"><f>A1*2</f>", "<c r=\"B1\"><f t=\"shared\" ref=\"B1:B10\" si=\"0\">A1*2</f>")
            for i in range(2, 11):
                xml = xml.replace(f'<c r="B{i}"><f>A{i}*2</f>', f'<c r="B{i}"><f t="shared" si="0"/>')
            for cell, fn, ref in [("G1", "_xlfn._xlws.SORT(A1:A10)", "G1:G10"), ("H1", "_xlfn.UNIQUE(A1:A10)", "H1:H10"), ("I1", "_xlfn.SEQUENCE(5,1,1,2)", "I1:I5")]:
                xml = xml.replace(f'<c r="{cell}"><f>{fn}</f>', f'<c r="{cell}" cm="1"><f t="array" ref="{ref}">{fn}</f>')
            return xml.encode("utf-8")
        if name == "[Content_Types].xml":
            return body.replace(
                b"</Types>",
                b'<Override PartName="/xl/metadata.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"/></Types>',
            )
        if name == "xl/_rels/workbook.xml.rels":
            return body.replace(
                b"</Relationships>",
                b'<Relationship Id="rIdMeta" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata" Target="metadata.xml"/></Relationships>',
            )
        if name == "+":
            meta = (
                '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
                '<metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
                'xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray">'
                '<metadataTypes count="1"><metadataType name="XLDAPR" minSupportedVersion="120000" copy="1" pasteAll="1" '
                'pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" '
                'coerce="1" cellMeta="1"/></metadataTypes><futureMetadata name="XLDAPR" count="1"><bk><extLst>'
                '<ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/>'
                '</ext></extLst></bk></futureMetadata><cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata></metadata>'
            )
            return [("xl/metadata.xml", meta.encode("utf-8"))]
        return body

    save_xlsx(w, "shared-array-formulas", wb, "Shared, array and dynamic-array formulas", rewrite)


def xlsx_conditional_formatting(w: Writer) -> None:
    from openpyxl.formatting.rule import CellIsRule, ColorScaleRule, DataBarRule, FormulaRule, IconSetRule, Rule
    from openpyxl.styles import Font, PatternFill
    from openpyxl.styles.differential import DifferentialStyle

    wb = new_workbook()
    ws = wb.active
    ws.title = "CF"
    rng = random.Random(4)
    for r in range(1, 21):
        for c in range(1, 15):
            ws.cell(r, c, rng.randrange(-50, 100))
    for r in range(1, 21):
        ws.cell(r, 15, rng.choice(["apple", "banana", "cherry", "apple pie", ""]))
        import datetime as _dt

        ws.cell(r, 16, _dt.datetime(2026, 1, 1) + _dt.timedelta(days=r * 3))
    red = DifferentialStyle(font=Font(color="FF9C0006"), fill=PatternFill(bgColor="FFFFC7CE"))
    green = DifferentialStyle(font=Font(color="FF006100"), fill=PatternFill(bgColor="FFC6EFCE"))
    ws.conditional_formatting.add("A1:A20", CellIsRule(operator="greaterThan", formula=["50"], font=Font(color="FF9C0006"), fill=PatternFill("solid", bgColor="FFFFC7CE")))
    ws.conditional_formatting.add("B1:B20", CellIsRule(operator="between", formula=["0", "25"], fill=PatternFill("solid", bgColor="FFC6EFCE")))
    ws.conditional_formatting.add("C1:C20", FormulaRule(formula=["MOD(ROW(),2)=0"], fill=PatternFill("solid", bgColor="FFDDEBF7")))
    ws.conditional_formatting.add("D1:D20", ColorScaleRule(start_type="min", start_color="FFF8696B", end_type="max", end_color="FF63BE7B"))
    ws.conditional_formatting.add("E1:E20", ColorScaleRule(start_type="min", start_color="FFF8696B", mid_type="percentile", mid_value=50, mid_color="FFFFEB84", end_type="max", end_color="FF63BE7B"))
    ws.conditional_formatting.add("F1:F20", DataBarRule(start_type="min", end_type="max", color="FF638EC6", showValue=True))
    ws.conditional_formatting.add("G1:G20", IconSetRule("3TrafficLights1", "percent", [0, 33, 67]))
    ws.conditional_formatting.add("H1:H20", IconSetRule("5Arrows", "percent", [0, 20, 40, 60, 80]))
    for col, rtype, extra in [
        ("I", "top10", {"rank": 3}),
        ("J", "top10", {"rank": 10, "percent": True, "bottom": True}),
        ("K", "aboveAverage", {}),
        ("L", "aboveAverage", {"aboveAverage": False}),
        ("M", "duplicateValues", {}),
        ("N", "uniqueValues", {}),
    ]:
        rule = Rule(type=rtype, dxf=red if rtype != "uniqueValues" else green, **extra)
        ws.conditional_formatting.add(f"{col}1:{col}20", rule)
    for rtype, op, text, formula in [
        ("containsText", "containsText", "apple", 'NOT(ISERROR(SEARCH("apple",O1)))'),
        ("beginsWith", "beginsWith", "ban", 'LEFT(O1,LEN("ban"))="ban"'),
        ("endsWith", "endsWith", "pie", 'RIGHT(O1,LEN("pie"))="pie"'),
        ("notContainsText", "notContains", "rr", 'ISERROR(SEARCH("rr",O1))'),
    ]:
        rule = Rule(type=rtype, operator=op, text=text, dxf=green, formula=[formula])
        ws.conditional_formatting.add("O1:O20", rule)
    ws.conditional_formatting.add("O1:O20", Rule(type="containsBlanks", dxf=red, formula=["LEN(TRIM(O1))=0"]))
    ws.conditional_formatting.add("A1:N20", Rule(type="containsErrors", dxf=red, formula=["ISERROR(A1)"]))
    ws.conditional_formatting.add("P1:P20", Rule(type="timePeriod", timePeriod="lastMonth", dxf=green, formula=["AND(MONTH(P1)=MONTH(EDATE(TODAY(),0-1)),YEAR(P1)=YEAR(EDATE(TODAY(),0-1)))"]))
    save_xlsx(w, "conditional-formatting", wb, "Conditional formatting")


def xlsx_data_validation(w: Writer) -> None:
    from openpyxl.worksheet.datavalidation import DataValidation

    wb = new_workbook()
    ws = wb.active
    ws.title = "Validation"
    specs = [
        ("List", DataValidation(type="list", formula1='"Red,Green,Blue"', allow_blank=True)),
        ("List from range", DataValidation(type="list", formula1="$H$1:$H$4")),
        ("Whole 1-10", DataValidation(type="whole", operator="between", formula1="1", formula2="10")),
        ("Decimal > 0.5", DataValidation(type="decimal", operator="greaterThan", formula1="0.5")),
        ("Date in 2026", DataValidation(type="date", operator="between", formula1="DATE(2026,1,1)", formula2="DATE(2026,12,31)")),
        ("Time before noon", DataValidation(type="time", operator="lessThan", formula1="0.5")),
        ("Text length <= 5", DataValidation(type="textLength", operator="lessThanOrEqual", formula1="5")),
        ("Custom even", DataValidation(type="custom", formula1="MOD(B8,2)=0")),
    ]
    for i, (label, dv) in enumerate(specs, start=1):
        ws.cell(i, 1, label)
        dv.promptTitle = label
        dv.prompt = f"Enter a value: {label}"
        dv.errorTitle = "Invalid"
        dv.error = f"Value must satisfy: {label}"
        dv.showInputMessage = True
        dv.showErrorMessage = True
        dv.errorStyle = ["stop", "warning", "information"][i % 3]
        ws.add_data_validation(dv)
        dv.add(f"B{i}")
    for i, v in enumerate(["North", "South", "East", "West"], start=1):
        ws.cell(i, 8, v)
    save_xlsx(w, "data-validation", wb, "Data validation")


def xlsx_tables(w: Writer) -> None:
    from openpyxl.worksheet.table import Table, TableStyleInfo

    wb = new_workbook()
    ws = wb.active
    ws.title = "Tables"
    rng = random.Random(5)
    ws.append(["Region", "Product", "Units", "Price", "Total"])
    for i in range(2, 22):
        ws.append([rng.choice(["North", "South", "East", "West"]), rng.choice(["A", "B", "C"]), rng.randrange(1, 100), round(rng.uniform(1, 20), 2), f"=Sales[[#This Row],[Units]]*Sales[[#This Row],[Price]]"])
    t = Table(displayName="Sales", ref="A1:E21")
    t.tableStyleInfo = TableStyleInfo(name="TableStyleMedium2", showRowStripes=True)
    ws.add_table(t)
    ws["G1"] = "Total units"
    ws["G2"] = "=SUM(Sales[Units])"
    ws2 = wb.create_sheet("Second")
    ws2.append(["Name", "Score"])
    for i in range(10):
        ws2.append([f"Person {i + 1}", rng.randrange(50, 100)])
    t2 = Table(displayName="Scores", ref="A1:B11")
    t2.tableStyleInfo = TableStyleInfo(name="TableStyleLight9", showRowStripes=False, showColumnStripes=True, showFirstColumn=True)
    ws2.add_table(t2)
    save_xlsx(w, "tables", wb, "Tables")


def xlsx_charts(w: Writer) -> None:
    from openpyxl.chart import AreaChart, BarChart, BubbleChart, DoughnutChart, LineChart, PieChart, RadarChart, Reference, ScatterChart, Series

    wb = new_workbook()
    ws = wb.active
    ws.title = "Charts"
    rows = [("Month", "North", "South", "East"), ("Jan", 30, 20, 25), ("Feb", 35, 22, 30), ("Mar", 40, 30, 28), ("Apr", 32, 35, 33), ("May", 45, 33, 40), ("Jun", 50, 38, 42)]
    for r in rows:
        ws.append(r)
    data = Reference(ws, min_col=2, min_row=1, max_col=4, max_row=7)
    cats = Reference(ws, min_col=1, min_row=2, max_row=7)

    def place(chart, title, anchor):
        chart.title = title
        chart.width, chart.height = 12, 7
        ws.add_chart(chart, anchor)

    bar = BarChart()
    bar.type = "col"
    bar.add_data(data, titles_from_data=True)
    bar.set_categories(cats)
    place(bar, "Clustered column", "F1")
    sbar = BarChart()
    sbar.type = "bar"
    sbar.grouping = "stacked"
    sbar.overlap = 100
    sbar.add_data(data, titles_from_data=True)
    sbar.set_categories(cats)
    place(sbar, "Stacked bar", "F16")
    line = LineChart()
    line.add_data(data, titles_from_data=True)
    line.set_categories(cats)
    place(line, "Line", "F31")
    area = AreaChart()
    area.add_data(data, titles_from_data=True)
    area.set_categories(cats)
    place(area, "Area", "F46")
    pie = PieChart()
    pie.add_data(Reference(ws, min_col=2, min_row=1, max_row=7), titles_from_data=True)
    pie.set_categories(cats)
    place(pie, "Pie", "N1")
    dough = DoughnutChart()
    dough.add_data(Reference(ws, min_col=3, min_row=1, max_row=7), titles_from_data=True)
    dough.set_categories(cats)
    place(dough, "Doughnut", "N16")
    sc = ScatterChart()
    xs = Reference(ws, min_col=2, min_row=2, max_row=7)
    for col in (3, 4):
        s = Series(Reference(ws, min_col=col, min_row=1, max_row=7), xs, title_from_data=True)
        s.marker.symbol = "circle"
        sc.series.append(s)
    place(sc, "Scatter", "N31")
    radar = RadarChart()
    radar.add_data(data, titles_from_data=True)
    radar.set_categories(cats)
    place(radar, "Radar", "N46")
    bub = BubbleChart()
    bub.series.append(Series(values=Reference(ws, min_col=3, min_row=2, max_row=7), xvalues=xs, zvalues=Reference(ws, min_col=4, min_row=2, max_row=7), title="Bubbles"))
    place(bub, "Bubble", "V1")
    save_xlsx(w, "charts", wb, "Charts")


def xlsx_comments(w: Writer) -> None:
    from openpyxl.comments import Comment

    wb = new_workbook()
    ws = wb.active
    ws.title = "Notes"
    for r in range(1, 11):
        ws.cell(r, 1, r * 10)
    for r in (1, 4, 7):
        c = Comment(f"Note on A{r}: check this value.", "Reviewer")
        c.width, c.height = 200, 80
        ws.cell(r, 1).comment = c
    ws["C3"] = "Annotated"
    ws["C3"].comment = Comment("A longer note\nwith two lines.", AUTHOR)
    save_xlsx(w, "comments", wb, "Comments and notes")


def xlsx_defined_names(w: Writer) -> None:
    from openpyxl.workbook.defined_name import DefinedName

    wb = new_workbook()
    ws = wb.active
    ws.title = "Names"
    for r in range(1, 11):
        ws.cell(r, 1, r)
        ws.cell(r, 2, r * r)
    wb.defined_names["Values"] = DefinedName("Values", attr_text="Names!$A$1:$A$10")
    wb.defined_names["Squares"] = DefinedName("Squares", attr_text="Names!$B$1:$B$10")
    wb.defined_names["TaxRate"] = DefinedName("TaxRate", attr_text="0.2")
    ws.defined_names["LocalTotal"] = DefinedName("LocalTotal", attr_text="Names!$D$1")
    ws["D1"] = "=SUM(Values)"
    ws["D2"] = "=SUM(Squares)*TaxRate"
    ws["D3"] = "=LocalTotal*2"
    ws.print_area = "A1:D10"
    save_xlsx(w, "defined-names", wb, "Defined names")


def xlsx_hidden(w: Writer) -> None:
    wb = new_workbook()
    ws = wb.active
    ws.title = "Visible"
    for r in range(1, 31):
        ws.append([f"r{r}c{c}" for c in range(1, 11)])
    for r in (3, 4, 10):
        ws.row_dimensions[r].hidden = True
    for col in ("C", "F"):
        ws.column_dimensions[col].hidden = True
    ws.row_dimensions.group(12, 16, outline_level=1, hidden=False)
    ws.row_dimensions.group(14, 15, outline_level=2, hidden=True)
    ws.column_dimensions.group("H", "I", outline_level=1, hidden=True)
    ws.column_dimensions["A"].width = 20
    ws.column_dimensions["B"].width = 4
    ws.row_dimensions[1].height = 40
    h = wb.create_sheet("Hidden")
    h["A1"] = "hidden sheet"
    h.sheet_state = "hidden"
    vh = wb.create_sheet("VeryHidden")
    vh["A1"] = "very hidden sheet"
    vh.sheet_state = "veryHidden"
    save_xlsx(w, "hidden-rows-cols", wb, "Hidden rows, columns and sheets")


def xlsx_date1904(w: Writer) -> None:
    import datetime as _dt

    from openpyxl.utils.datetime import CALENDAR_MAC_1904

    wb = new_workbook()
    wb.epoch = CALENDAR_MAC_1904
    ws = wb.active
    ws.title = "Dates"
    for i in range(10):
        c = ws.cell(i + 1, 1, _dt.datetime(2026, 1, 15) + _dt.timedelta(days=30 * i))
        c.number_format = "yyyy-mm-dd"
        ws.cell(i + 1, 2, f"=A{i + 1}+1").number_format = "yyyy-mm-dd"
    save_xlsx(w, "date1904", wb, "1904 date system")


def xlsx_hyperlinks(w: Writer) -> None:
    wb = new_workbook()
    ws = wb.active
    ws.title = "Links"
    other = wb.create_sheet("Target")
    other["B5"] = "Internal target"
    ws["A1"] = "External"
    ws["A1"].hyperlink = "https://example.org/"
    ws["A1"].style = "Hyperlink"
    ws["A2"] = "Internal"
    ws["A2"].hyperlink = "#Target!B5"
    ws["A2"].style = "Hyperlink"
    ws["A3"] = "Mail"
    ws["A3"].hyperlink = "mailto:someone@example.org"
    ws["A3"].style = "Hyperlink"
    ws["A4"] = '=HYPERLINK("https://example.org/formula","Formula link")'
    save_xlsx(w, "hyperlinks", wb, "Hyperlinks")


def xlsx_print_setup(w: Writer) -> None:
    from openpyxl.worksheet.pagebreak import Break

    wb = new_workbook()
    ws = wb.active
    ws.title = "Print"
    for r in range(1, 121):
        ws.append([r] + [r * c for c in range(1, 12)])
    ws.page_setup.orientation = "landscape"
    ws.page_setup.paperSize = ws.PAPERSIZE_A4
    ws.page_setup.fitToWidth = 1
    ws.page_setup.fitToHeight = 0
    ws.sheet_properties.pageSetUpPr.fitToPage = True
    ws.page_margins.left = ws.page_margins.right = 0.5
    ws.page_margins.top = ws.page_margins.bottom = 0.75
    ws.print_options.gridLines = True
    ws.print_options.horizontalCentered = True
    ws.oddHeader.center.text = "&[File] — &[Tab]"
    ws.oddFooter.right.text = "Page &[Page] of &N"
    ws.print_title_rows = "1:1"
    ws.print_area = "A1:L120"
    ws.row_breaks.append(Break(id=60))
    save_xlsx(w, "print-setup", wb, "Print setup")


def xlsx_multi_sheet(w: Writer) -> None:
    wb = new_workbook()
    colours = ["FF4472C4", "FFED7D31", "FFA5A5A5", "FFFFC000", "FF70AD47"]
    for i, name in enumerate(["Jan", "Feb", "Mar", "Apr", "Summary"]):
        ws = wb.active if i == 0 else wb.create_sheet(name)
        ws.title = name
        ws.sheet_properties.tabColor = colours[i]
        if name != "Summary":
            for r in range(1, 6):
                ws.cell(r, 1, (i + 1) * r)
    s = wb["Summary"]
    s["A1"] = "=SUM(Jan:Apr!A1:A5)"
    s["A2"] = "=Jan!A1+Feb!A1"
    s["A3"] = "=AVERAGE('Mar'!A1:A5)"
    wb.create_sheet("Name with spaces")["A1"] = "=Summary!A1*2"
    wb.active = 4
    save_xlsx(w, "multi-sheet", wb, "Multiple sheets")


def xlsx_rich_text(w: Writer) -> None:
    from openpyxl.cell.rich_text import CellRichText, TextBlock
    from openpyxl.cell.text import InlineFont

    wb = new_workbook()
    ws = wb.active
    ws.title = "Rich"
    ws["A1"] = CellRichText(["Plain, ", TextBlock(InlineFont(b=True), "bold, "), TextBlock(InlineFont(i=True, color="FFC00000"), "red italic, "), TextBlock(InlineFont(u="single", sz=16), "big underline")])
    ws["A2"] = CellRichText([TextBlock(InlineFont(rFont="Courier New"), "mono"), " and ", TextBlock(InlineFont(vertAlign="superscript"), "super")])
    ws["A3"] = "Shared string repeated"
    ws["A4"] = "Shared string repeated"
    ws["A5"] = "Line one\nLine two"
    from openpyxl.styles import Alignment

    ws["A5"].alignment = Alignment(wrap_text=True)
    ws.column_dimensions["A"].width = 50
    save_xlsx(w, "rich-text", wb, "Rich text")


def xlsx_protection(w: Writer) -> None:
    from openpyxl.styles import Protection

    wb = new_workbook()
    ws = wb.active
    ws.title = "Protected"
    for r in range(1, 11):
        ws.cell(r, 1, f"Locked {r}")
        c = ws.cell(r, 2, r)
        c.protection = Protection(locked=False)
    ws.protection.sheet = True
    ws.protection.password = "kiln"
    ws.protection.formatCells = False
    wb.security.lockStructure = True
    save_xlsx(w, "protection", wb, "Protection")


def xlsx_autofilter(w: Writer) -> None:
    wb = new_workbook()
    ws = wb.active
    ws.title = "Filter"
    rng = random.Random(6)
    ws.append(["Name", "Team", "Score"])
    for i in range(30):
        ws.append([f"Player {i + 1:02d}", rng.choice(["Red", "Blue", "Green"]), rng.randrange(0, 100)])
    ws.auto_filter.ref = "A1:C31"
    ws.auto_filter.add_filter_column(1, ["Red", "Blue"])
    ws.auto_filter.add_sort_condition("C2:C31", descending=True)
    save_xlsx(w, "autofilter", wb, "AutoFilter")


def make_large_xlsx(out: Path) -> Path:
    import openpyxl

    d = out / "xlsx" / "large"
    d.mkdir(parents=True, exist_ok=True)
    wb = openpyxl.Workbook(write_only=True)
    wb.properties.creator = AUTHOR
    wb.properties.created = FIXED_DATE
    ws = wb.create_sheet("Data")
    rng = random.Random(100)
    ws.append(["Id", "Category", "Amount", "Quantity", "Total"])
    for i in range(1, 100_001):
        ws.append([i, rng.choice(["A", "B", "C", "D"]), round(rng.uniform(0, 1000), 2), rng.randrange(1, 50), f"=C{i + 1}*D{i + 1}"])
    buf = io.BytesIO()
    wb.save(buf)
    path = d / "large-100k.xlsx"
    path.write_bytes(normalize_package(buf.getvalue()))
    return path


# ------------------------------------------------------------------ synthetic large (§3.9)

LARGE_DOC_PAGES = 300
LARGE_SHEET_ROWS = 100_000  # × 10 columns = 1 000 000 cells, columns I and J are formulas
LARGE_DECK_SLIDES = 200


def large_doc(path: Path) -> None:
    """300 pages: each page is a heading, body text and either a table or a picture, closed by an
    explicit page break, with content that fits one Letter/A4 page so the page count is exact."""
    from docx.enum.text import WD_BREAK
    from docx.shared import Cm, Pt

    rng = random.Random(300)
    d = new_document()
    normal = d.styles["Normal"]
    normal.font.name = "Calibri"
    normal.font.size = Pt(11)
    pictures = [png_bytes(480, 240, 300 + i) for i in range(6)]
    for page in range(1, LARGE_DOC_PAGES + 1):
        if page % 25 == 1:
            d.add_heading(f"Part {page // 25 + 1}: {lorem(rng, 4).rstrip('.')}", 1)
        d.add_heading(f"{page}. {lorem(rng, 5).rstrip('.')}", 2)
        kind = page % 3
        for _ in range(2 if kind else 1):
            d.add_paragraph(lorem(rng, 70))
        if kind == 0:
            t = d.add_table(rows=6, cols=4)
            t.style = "Light Grid Accent 1"
            for c, head in enumerate(["Batch", "Clay body", "Cone", "Shrinkage %"]):
                t.cell(0, c).text = head
            for r in range(1, 6):
                t.cell(r, 0).text = f"{page:03d}-{r}"
                t.cell(r, 1).text = rng.choice(["stoneware", "porcelain", "earthenware", "raku"])
                t.cell(r, 2).text = str(rng.choice([6, 8, 10, 11]))
                t.cell(r, 3).text = f"{rng.uniform(9, 14):.1f}"
            d.add_paragraph(lorem(rng, 40))
        elif kind == 1:
            run = d.add_paragraph().add_run()
            run.add_picture(io.BytesIO(pictures[page % len(pictures)]), width=Cm(12))
            d.add_paragraph(f"Figure {page}: {lorem(rng, 8)}", style="Caption")
        else:
            for _ in range(2):
                d.add_paragraph(lorem(rng, 60))
        if page < LARGE_DOC_PAGES:
            d.paragraphs[-1].add_run().add_break(WD_BREAK.PAGE)
    d.core_properties.title = "300 pages"
    buf = io.BytesIO()
    d.save(buf)
    path.write_bytes(normalize_package(buf.getvalue()))


def large_sheet(path: Path) -> None:
    """100 000 rows × 10 columns: id, category, amount, quantity, unit cost, date serial, region and a
    flag as values, then a product (I) and a row sum (J) as formulas. A second sheet explains it."""
    import openpyxl

    wb = openpyxl.Workbook(write_only=True)
    wb.properties.creator = AUTHOR
    wb.properties.created = FIXED_DATE
    wb.properties.modified = FIXED_DATE
    about = wb.create_sheet("About")
    for line in [
        ["Synthetic 1M-cell workbook for the Kiln performance budgets (PLAN.md 3.9)."],
        ["Sheet Data: 100,000 rows x 10 columns = 1,000,000 cells; columns I and J are formulas."],
        ["A id, B category, C amount, D quantity, E unit cost, F date serial, G region, H flag,"],
        ["I = C*D, J = SUM(C:E)+I."],
    ]:
        about.append(line)
    ws = wb.create_sheet("Data")
    rng = random.Random(1_000_000)
    cats = ["Bowls", "Cups", "Plates", "Vases", "Tiles", "Jars"]
    regions = ["North", "South", "East", "West"]
    for r in range(1, LARGE_SHEET_ROWS + 1):
        ws.append([
            r,
            cats[rng.randrange(len(cats))],
            round(rng.uniform(1, 1000), 2),
            rng.randrange(1, 100),
            round(rng.uniform(0.5, 50), 2),
            45_000 + rng.randrange(0, 1500),
            regions[rng.randrange(len(regions))],
            rng.random() < 0.5,
            f"=C{r}*D{r}",
            f"=SUM(C{r}:E{r})+I{r}",
        ])
    buf = io.BytesIO()
    wb.save(buf)

    def with_dimensions(name: str, body: bytes) -> bytes:
        # openpyxl's write-only mode leaves out <dimension>; Excel always writes it (it follows
        # <sheetPr> and precedes <sheetViews>), and readers use it to size the sheet up front.
        refs = {"xl/worksheets/sheet1.xml": "A1:A4", "xl/worksheets/sheet2.xml": f"A1:J{LARGE_SHEET_ROWS}"}
        if name not in refs or b"<dimension" in body:
            return body
        text = body.decode("utf-8")
        tag = f'<dimension ref="{refs[name]}"/>'
        m = re.search(r"<sheetPr\b[^>]*?(/>|>.*?</sheetPr>)", text, re.S)
        if m:
            text = text[: m.end()] + tag + text[m.end() :]
        else:
            m = re.search(r"<worksheet\b[^>]*>", text)
            assert m, name
            text = text[: m.end()] + tag + text[m.end() :]
        return text.encode("utf-8")

    path.write_bytes(normalize_package(buf.getvalue(), with_dimensions))


def large_deck(path: Path) -> None:
    """200 slides cycling through bullets, two columns, a picture, a table, preset shapes and (every
    tenth slide) a chart, with a section header every 25 slides and speaker notes on each."""
    from pptx.chart.data import CategoryChartData
    from pptx.enum.chart import XL_CHART_TYPE
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Emu, Pt

    rng = random.Random(200)
    prs = new_presentation()
    pictures = [png_bytes(640, 360, 200 + i) for i in range(6)]
    shapes = [MSO_SHAPE.ROUNDED_RECTANGLE, MSO_SHAPE.OVAL, MSO_SHAPE.CHEVRON, MSO_SHAPE.HEXAGON, MSO_SHAPE.STAR_5_POINT]
    for i in range(1, LARGE_DECK_SLIDES + 1):
        if i % 25 == 1:
            s = prs.slides.add_slide(prs.slide_layouts[2])  # Section Header
            s.shapes.title.text = f"Section {i // 25 + 1}"
            s.placeholders[1].text = lorem(rng, 8)
        elif i % 10 == 0:
            s = blank_slide(prs, f"Slide {i}: results")
            cd = CategoryChartData()
            cd.categories = ["Q1", "Q2", "Q3", "Q4"]
            for name in ["Kiln A", "Kiln B"]:
                cd.add_series(name, [round(rng.uniform(10, 40), 1) for _ in range(4)])
            s.shapes.add_chart(XL_CHART_TYPE.COLUMN_CLUSTERED, Emu(1219200), Emu(1371600), Emu(9753600), Emu(4876800), cd)
        elif i % 5 == 1:
            s = prs.slides.add_slide(prs.slide_layouts[3])  # Two Content
            s.shapes.title.text = f"Slide {i}: compare"
            s.placeholders[1].text = "\n".join(lorem(rng, 6) for _ in range(3))
            s.placeholders[2].text = "\n".join(lorem(rng, 6) for _ in range(3))
        elif i % 5 == 2:
            s = blank_slide(prs, f"Slide {i}: picture")
            s.shapes.add_picture(io.BytesIO(pictures[i % len(pictures)]), Emu(1828800), Emu(1371600), Emu(8534400), Emu(4800600))
        elif i % 5 == 3:
            s = blank_slide(prs, f"Slide {i}: table")
            t = s.shapes.add_table(5, 4, Emu(609600), Emu(1524000), Emu(10972800), Emu(3048000)).table
            for r in range(5):
                for c in range(4):
                    t.cell(r, c).text = f"Column {c + 1}" if r == 0 else f"{rng.randrange(1000)}"
        elif i % 5 == 4:
            s = blank_slide(prs, f"Slide {i}: shapes")
            for k, shape in enumerate(shapes):
                sp = s.shapes.add_shape(shape, Emu(609600 + k * 2286000), Emu(2286000), Emu(1828800), Emu(1828800))
                sp.text_frame.text = shape.name.replace("_", " ").lower()
                sp.text_frame.paragraphs[0].runs[0].font.size = Pt(12)
        else:
            s = prs.slides.add_slide(prs.slide_layouts[1])  # Title and Content
            s.shapes.title.text = f"Slide {i}: {lorem(rng, 4).rstrip('.')}"
            body = s.placeholders[1].text_frame
            body.text = lorem(rng, 10)
            for level in (1, 1, 0, 1):
                para = body.add_paragraph()
                para.text = lorem(rng, 9)
                para.level = level
        s.notes_slide.notes_text_frame.text = f"Notes for slide {i}. " + lorem(rng, 20)
    prs.core_properties.title = "200 slides"
    buf = io.BytesIO()
    prs.save(buf)
    path.write_bytes(normalize_package(buf.getvalue()))


def make_synthetic_large(out: Path, formats: list[str]) -> list[Path]:
    d = out / "large"
    d.mkdir(parents=True, exist_ok=True)
    jobs = {"docx": ("doc-300p.docx", large_doc), "xlsx": ("sheet-1m.xlsx", large_sheet), "pptx": ("deck-200.pptx", large_deck)}
    written = []
    for fmt in formats:
        name, make = jobs[fmt]
        path = d / name
        make(path)
        written.append(path)
    return written


def make_xlsx(out: Path) -> int:
    w = Writer(out, "xlsx")
    for fn in [
        xlsx_number_formats,
        xlsx_styles,
        xlsx_merges,
        xlsx_freeze_panes,
        xlsx_formulas,
        xlsx_shared_array_formulas,
        xlsx_conditional_formatting,
        xlsx_data_validation,
        xlsx_tables,
        xlsx_charts,
        xlsx_comments,
        xlsx_defined_names,
        xlsx_hidden,
        xlsx_date1904,
        xlsx_hyperlinks,
        xlsx_print_setup,
        xlsx_multi_sheet,
        xlsx_rich_text,
        xlsx_protection,
        xlsx_autofilter,
    ]:
        fn(w)
    return w.count

# ----------------------------------------------------------------------------- pptx

P_NS = "http://schemas.openxmlformats.org/presentationml/2006/main"
A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main"


def new_presentation(widescreen: bool = True):
    from pptx import Presentation
    from pptx.util import Emu

    prs = Presentation()
    if widescreen:
        prs.slide_width, prs.slide_height = Emu(12192000), Emu(6858000)
    cp = prs.core_properties
    cp.author = AUTHOR
    cp.last_modified_by = AUTHOR
    cp.created = FIXED_DATE
    cp.modified = FIXED_DATE
    cp.revision = 1
    return prs


def save_pptx(w: Writer, name: str, prs, title: str, rewrite=None) -> None:
    prs.core_properties.title = title
    buf = io.BytesIO()
    prs.save(buf)
    w.save(name, buf.getvalue(), rewrite)


def blank_slide(prs, title: str | None = None):
    layout = prs.slide_layouts[5 if title else 6]  # Title Only / Blank
    s = prs.slides.add_slide(layout)
    if title:
        s.shapes.title.text = title
    return s


def p_xml(xml: str):
    from pptx.oxml import parse_xml

    return parse_xml(xml)


def pptx_placeholders(w: Writer) -> None:
    from pptx.enum.shapes import PP_PLACEHOLDER

    prs = new_presentation()
    for i, layout in enumerate(prs.slide_layouts):
        s = prs.slides.add_slide(layout)
        for ph in s.placeholders:
            kind = ph.placeholder_format.type
            if kind == PP_PLACEHOLDER.PICTURE:
                ph.insert_picture(io.BytesIO(png_bytes(400, 300, 30 + i)))
            elif ph.has_text_frame:
                if kind in (PP_PLACEHOLDER.TITLE, PP_PLACEHOLDER.CENTER_TITLE):
                    ph.text = f"Layout {i + 1}: {layout.name}"
                else:
                    tf = ph.text_frame
                    tf.text = f"{kind.name.lower()} placeholder (idx {ph.placeholder_format.idx})"
                    for lvl in range(1, 3):
                        p = tf.add_paragraph()
                        p.text = f"Level {lvl + 1} bullet"
                        p.level = lvl
    save_pptx(w, "placeholders", prs, "Every layout and placeholder type")


def pptx_preset_shapes(w: Writer) -> None:
    from pptx.dml.color import RGBColor
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Emu, Pt

    prs = new_presentation()
    members = sorted(MSO_SHAPE, key=lambda m: m.name)
    chosen = members[::3][:60]
    per_slide = 12
    cw, ch = 2438400, 2032000
    for start in range(0, len(chosen), per_slide):
        s = blank_slide(prs)
        for k, shape_type in enumerate(chosen[start : start + per_slide]):
            col, row = k % 4, k // 4
            sh = s.shapes.add_shape(shape_type, Emu(609600 + col * (cw + 304800)), Emu(304800 + row * (ch + 101600)), Emu(cw), Emu(ch - 406400))
            sh.text_frame.text = shape_type.name.lower()
            sh.text_frame.paragraphs[0].runs[0].font.size = Pt(10)
            sh.fill.solid()
            sh.fill.fore_color.rgb = RGBColor.from_string(["4472C4", "ED7D31", "70AD47", "FFC000", "5B9BD5", "A5A5A5"][(start + k) % 6])
    save_pptx(w, "preset-shapes", prs, f"{len(chosen)} preset shapes")


def pptx_groups(w: Writer) -> None:
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Emu

    prs = new_presentation()
    s = blank_slide(prs, "Groups")
    g = s.shapes.add_group_shape()
    for i in range(3):
        g.shapes.add_shape(MSO_SHAPE.OVAL, Emu(914400 + i * 1219200), Emu(1828800), Emu(1066800), Emu(1066800))
    inner = g.shapes.add_group_shape()
    inner.shapes.add_shape(MSO_SHAPE.RECTANGLE, Emu(914400), Emu(3200400), Emu(1828800), Emu(762000))
    inner.shapes.add_shape(MSO_SHAPE.ISOSCELES_TRIANGLE, Emu(3048000), Emu(3200400), Emu(1219200), Emu(762000))
    g2 = s.shapes.add_group_shape()
    a = g2.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Emu(6705600), Emu(1828800), Emu(2438400), Emu(1219200))
    a.text_frame.text = "Rotated group"
    g2.shapes.add_shape(MSO_SHAPE.STAR_5_POINT, Emu(7315200), Emu(3352800), Emu(1219200), Emu(1219200))
    g2.rotation = 15
    save_pptx(w, "groups", prs, "Groups")


def pptx_pictures(w: Writer) -> None:
    from pptx.util import Emu

    prs = new_presentation()
    s = blank_slide(prs, "Pictures: crop, rotation, formats")
    p1 = s.shapes.add_picture(io.BytesIO(png_bytes(400, 300, 40)), Emu(609600), Emu(1524000), Emu(3048000), Emu(2286000))
    p2 = s.shapes.add_picture(io.BytesIO(png_bytes(400, 300, 41)), Emu(4267200), Emu(1524000), Emu(3048000), Emu(2286000))
    p2.crop_left, p2.crop_right, p2.crop_top, p2.crop_bottom = 0.1, 0.2, 0.15, 0.05
    p3 = s.shapes.add_picture(io.BytesIO(jpeg_bytes(300, 300, 42)), Emu(7924800), Emu(1524000), Emu(2286000), Emu(2286000))
    p3.rotation = 20
    s.shapes.add_picture(io.BytesIO(png_bytes(64, 64, 43, "flat")), Emu(609600), Emu(4419600), Emu(609600), Emu(609600))
    _ = p1
    save_pptx(w, "pictures", prs, "Pictures")


def pptx_tables(w: Writer) -> None:
    from pptx.util import Emu

    prs = new_presentation()
    s = blank_slide(prs, "Tables")
    gf = s.shapes.add_table(5, 4, Emu(609600), Emu(1524000), Emu(10972800), Emu(3048000))
    t = gf.table
    for r in range(5):
        for c in range(4):
            t.cell(r, c).text = f"Header {c + 1}" if r == 0 else f"{r * 10 + c}"
    t.cell(1, 0).merge(t.cell(1, 2))
    t.cell(1, 0).text = "Merged across three columns"
    t.cell(2, 3).merge(t.cell(4, 3))
    t.cell(2, 3).text = "Merged down"
    t.first_row = True
    t.horz_banding = True
    s2 = blank_slide(prs, "Second table, no header row")
    gf2 = s2.shapes.add_table(3, 3, Emu(1524000), Emu(1828800), Emu(6096000), Emu(1828800))
    gf2.table.first_row = False
    for r in range(3):
        for c in range(3):
            gf2.table.cell(r, c).text = f"r{r}c{c}"
    save_pptx(w, "tables", prs, "Tables")


def pptx_charts(w: Writer) -> None:
    from pptx.chart.data import BubbleChartData, CategoryChartData, XyChartData
    from pptx.enum.chart import XL_CHART_TYPE, XL_LEGEND_POSITION
    from pptx.util import Emu

    prs = new_presentation()
    cats = ["Q1", "Q2", "Q3", "Q4"]
    series = {"North": (19.2, 21.4, 16.7, 25.1), "South": (22.3, 28.6, 15.2, 18.4), "East": (12.0, 14.5, 19.8, 21.3)}
    for kind in [
        XL_CHART_TYPE.COLUMN_CLUSTERED,
        XL_CHART_TYPE.BAR_STACKED,
        XL_CHART_TYPE.LINE_MARKERS,
        XL_CHART_TYPE.PIE,
        XL_CHART_TYPE.DOUGHNUT,
        XL_CHART_TYPE.AREA,
        XL_CHART_TYPE.RADAR,
    ]:
        s = blank_slide(prs, kind.name.replace("_", " ").title())
        cd = CategoryChartData()
        cd.categories = cats
        for name, values in series.items():
            cd.add_series(name, values)
            if kind in (XL_CHART_TYPE.PIE, XL_CHART_TYPE.DOUGHNUT):
                break
        gf = s.shapes.add_chart(kind, Emu(1219200), Emu(1371600), Emu(9753600), Emu(4876800), cd)
        gf.chart.has_legend = True
        gf.chart.legend.position = XL_LEGEND_POSITION.BOTTOM
        gf.chart.legend.include_in_layout = False
    s = blank_slide(prs, "Scatter")
    xy = XyChartData()
    sr = xy.add_series("Measurements")
    rng = random.Random(9)
    for i in range(12):
        sr.add_data_point(i, round(i * 1.5 + rng.uniform(-2, 2), 2))
    s.shapes.add_chart(XL_CHART_TYPE.XY_SCATTER, Emu(1219200), Emu(1371600), Emu(9753600), Emu(4876800), xy)
    s = blank_slide(prs, "Bubble")
    bd = BubbleChartData()
    bs = bd.add_series("Bubbles")
    for i in range(6):
        bs.add_data_point(i + 1, (i * 7) % 10 + 1, (i % 3 + 1) * 5)
    s.shapes.add_chart(XL_CHART_TYPE.BUBBLE, Emu(1219200), Emu(1371600), Emu(9753600), Emu(4876800), bd)
    save_pptx(w, "charts", prs, "Charts")


TRANSITIONS = [
    ("fade", '<p:fade/>'),
    ("push", '<p:push dir="u"/>'),
    ("wipe", '<p:wipe dir="r"/>'),
    ("split", '<p:split orient="vert" dir="out"/>'),
    ("cover", '<p:cover dir="l"/>'),
    ("pull", '<p:pull dir="d"/>'),
    ("cut", '<p:cut/>'),
    ("random bars", '<p:randomBar dir="vert"/>'),
    ("circle", '<p:circle/>'),
    ("dissolve", '<p:dissolve/>'),
    ("zoom", '<p:zoom dir="in"/>'),
    ("blinds", '<p:blinds dir="horz"/>'),
]


def add_transition(slide, inner: str, speed: str = "med", advance_ms: int | None = None) -> None:
    adv = f' advTm="{advance_ms}"' if advance_ms is not None else ""
    node = p_xml(f'<p:transition xmlns:p="{P_NS}" spd="{speed}"{adv}>{inner}</p:transition>')
    el = slide._element
    clr = el.find(f"{{{P_NS}}}clrMapOvr")
    (clr if clr is not None else el.find(f"{{{P_NS}}}cSld")).addnext(node)


def pptx_transitions(w: Writer) -> None:
    prs = new_presentation()
    for i, (name, xml) in enumerate(TRANSITIONS):
        s = blank_slide(prs, f"Transition: {name}")
        add_transition(s, xml, ["slow", "med", "fast"][i % 3], 3000 if i % 4 == 0 else None)
    save_pptx(w, "transitions", prs, "Transitions")


def timing_xml(shape_ids: list[int]) -> str:
    """A main sequence with one click per shape: appear, fade, wipe in turn."""
    ctn = iter(range(3, 1000))
    clicks = []
    builds = []
    for k, spid in enumerate(shape_ids):
        a, b, c = next(ctn), next(ctn), next(ctn)
        kind = k % 3
        preset = [(1, ""), (10, "fade"), (22, "wipe(down)")][kind]
        behaviours = (
            f'<p:set><p:cBhvr><p:cTn id="{next(ctn)}" dur="1" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst></p:cTn>'
            f'<p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr>'
            '<p:to><p:strVal val="visible"/></p:to></p:set>'
        )
        if preset[1]:
            behaviours += (
                f'<p:animEffect transition="in" filter="{preset[1]}"><p:cBhvr><p:cTn id="{next(ctn)}" dur="500"/>'
                f'<p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl></p:cBhvr></p:animEffect>'
            )
        clicks.append(
            f'<p:par><p:cTn id="{a}" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst>'
            f'<p:par><p:cTn id="{b}" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>'
            f'<p:par><p:cTn id="{c}" presetID="{preset[0]}" presetClass="entr" presetSubtype="{4 if kind == 2 else 0}" fill="hold" grpId="0" nodeType="clickEffect">'
            f'<p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>{behaviours}</p:childTnLst></p:cTn></p:par>'
            "</p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>"
        )
        builds.append(f'<p:bldP spid="{spid}" grpId="0" animBg="1"/>')
    return (
        f'<p:timing xmlns:p="{P_NS}"><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst>'
        '<p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst>'
        + "".join(clicks)
        + '</p:childTnLst></p:cTn><p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst>'
        '<p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq>'
        "</p:childTnLst></p:cTn></p:par></p:tnLst><p:bldLst>" + "".join(builds) + "</p:bldLst></p:timing>"
    )


def pptx_animations(w: Writer) -> None:
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Emu

    prs = new_presentation()
    for n in range(2):
        s = blank_slide(prs, f"Animation builds {n + 1}")
        ids = []
        for i in range(3):
            sh = s.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Emu(914400 + i * 3657600), Emu(2286000), Emu(3048000), Emu(1828800))
            sh.text_frame.text = ["Appear", "Fade", "Wipe"][i] + " on click"
            ids.append(sh.shape_id)
        el = s._element
        node = p_xml(timing_xml(ids))
        trans = el.find(f"{{{P_NS}}}transition")
        anchor = trans if trans is not None else el.find(f"{{{P_NS}}}clrMapOvr")
        anchor.addnext(node)
    save_pptx(w, "animations", prs, "Animations")


def pptx_notes(w: Writer) -> None:
    rng = random.Random(15)
    prs = new_presentation()
    for i in range(3):
        s = prs.slides.add_slide(prs.slide_layouts[1])
        s.shapes.title.text = f"Slide with notes {i + 1}"
        s.placeholders[1].text = lorem(rng, 10)
        s.notes_slide.notes_text_frame.text = f"Speaker notes for slide {i + 1}. " + lorem(rng, 25)
    save_pptx(w, "notes", prs, "Speaker notes")


def pptx_sections(w: Writer) -> None:
    prs = new_presentation()
    names = ["Introduction", "Introduction", "Results", "Results", "Results", "Appendix"]
    for i, n in enumerate(names):
        blank_slide(prs, f"{n}: slide {i + 1}")
    ids = [s.slide_id for s in prs.slides]
    groups: dict[str, list[int]] = {}
    for n, sid in zip(names, ids):
        groups.setdefault(n, []).append(sid)
    p14 = "http://schemas.microsoft.com/office/powerpoint/2010/main"
    sections = "".join(
        f'<p14:section name="{n}" id="{{6E2F5B1A-0000-4000-8000-00000000000{k}}}"><p14:sldIdLst>'
        + "".join(f'<p14:sldId id="{sid}"/>' for sid in sids)
        + "</p14:sldIdLst></p14:section>"
        for k, (n, sids) in enumerate(groups.items())
    )
    ext = p_xml(
        f'<p:extLst xmlns:p="{P_NS}"><p:ext uri="{{521415D9-36F7-43E2-AB2F-B90AF26B5E84}}">'
        f'<p14:sectionLst xmlns:p14="{p14}">{sections}</p14:sectionLst></p:ext></p:extLst>'
    )
    prs.part._element.append(ext)
    save_pptx(w, "sections", prs, "Sections")


def pptx_aspect(w: Writer, widescreen: bool) -> None:
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Emu

    prs = new_presentation(widescreen)
    s = prs.slides.add_slide(prs.slide_layouts[0])
    s.shapes.title.text = "16:9 widescreen" if widescreen else "4:3 standard"
    s.placeholders[1].text = f"{prs.slide_width} × {prs.slide_height} EMU"
    s2 = blank_slide(prs, "Edge markers")
    for x, y in [(0, 0), (prs.slide_width - 914400, 0), (0, prs.slide_height - 914400), (prs.slide_width - 914400, prs.slide_height - 914400)]:
        s2.shapes.add_shape(MSO_SHAPE.RECTANGLE, Emu(x), Emu(y), Emu(914400), Emu(914400))
    save_pptx(w, "aspect-16x9" if widescreen else "aspect-4x3", prs, "Slide size")


def pptx_text_formatting(w: Writer) -> None:
    from pptx.dml.color import RGBColor
    from pptx.enum.text import MSO_ANCHOR, MSO_AUTO_SIZE, PP_ALIGN
    from pptx.util import Emu, Pt

    prs = new_presentation()
    s = blank_slide(prs, "Text formatting")
    tb = s.shapes.add_textbox(Emu(609600), Emu(1371600), Emu(5486400), Emu(4572000))
    tf = tb.text_frame
    tf.word_wrap = True
    p = tf.paragraphs[0]
    for text, props in [
        ("Bold ", {"bold": True}),
        ("italic ", {"italic": True}),
        ("underline ", {"underline": True}),
        ("red ", {"color": "C00000"}),
        ("24pt ", {"size": 24}),
        ("Georgia", {"name": "Georgia"}),
    ]:
        r = p.add_run()
        r.text = text
        f = r.font
        if "bold" in props:
            f.bold = True
        if "italic" in props:
            f.italic = True
        if "underline" in props:
            f.underline = True
        if "color" in props:
            f.color.rgb = RGBColor.from_string(props["color"])
        if "size" in props:
            f.size = Pt(props["size"])
        if "name" in props:
            f.name = props["name"]
    for align in [PP_ALIGN.LEFT, PP_ALIGN.CENTER, PP_ALIGN.RIGHT, PP_ALIGN.JUSTIFY]:
        q = tf.add_paragraph()
        q.text = f"Aligned {align.name.lower()}"
        q.alignment = align
    q = tf.add_paragraph()
    q.text = "Line spacing 1.5, space before 12pt"
    q.line_spacing = 1.5
    q.space_before = Pt(12)
    body = s.shapes.add_textbox(Emu(6400800), Emu(1371600), Emu(5181600), Emu(2286000))
    btf = body.text_frame
    btf.auto_size = MSO_AUTO_SIZE.TEXT_TO_FIT_SHAPE
    btf.vertical_anchor = MSO_ANCHOR.MIDDLE
    btf.text = "Shrink text on overflow, anchored middle. " * 4
    bp = body._element.find(f".//{{{A_NS}}}bodyPr")
    bp.set("numCol", "2")
    bp.set("spcCol", "182880")
    vert = s.shapes.add_textbox(Emu(6400800), Emu(3962400), Emu(914400), Emu(2286000))
    vert.text_frame.text = "Vertical"
    vert._element.find(f".//{{{A_NS}}}bodyPr").set("vert", "vert270")
    s2 = prs.slides.add_slide(prs.slide_layouts[1])
    s2.shapes.title.text = "Bullets and levels"
    tf2 = s2.placeholders[1].text_frame
    tf2.text = "Level one"
    for lvl in range(1, 5):
        para = tf2.add_paragraph()
        para.text = f"Level {lvl + 1}"
        para.level = lvl
    num = tf2.add_paragraph()
    num.text = "Numbered paragraph"
    ppr = num._p.get_or_add_pPr()
    ppr.append(p_xml(f'<a:buAutoNum xmlns:a="{A_NS}" type="arabicPeriod"/>'))
    save_pptx(w, "text-formatting", prs, "Text formatting")


def pptx_connectors(w: Writer) -> None:
    from pptx.enum.shapes import MSO_CONNECTOR, MSO_SHAPE
    from pptx.util import Emu

    prs = new_presentation()
    s = blank_slide(prs, "Connectors")
    a = s.shapes.add_shape(MSO_SHAPE.RECTANGLE, Emu(914400), Emu(1828800), Emu(1828800), Emu(1066800))
    b = s.shapes.add_shape(MSO_SHAPE.OVAL, Emu(5486400), Emu(1371600), Emu(1828800), Emu(1066800))
    c = s.shapes.add_shape(MSO_SHAPE.DIAMOND, Emu(8534400), Emu(3962400), Emu(1828800), Emu(1371600))
    for kind, (x, y), (u, v) in [
        (MSO_CONNECTOR.STRAIGHT, (a, 3), (b, 1)),
        (MSO_CONNECTOR.ELBOW, (b, 3), (c, 0)),
        (MSO_CONNECTOR.CURVE, (a, 2), (c, 1)),
    ]:
        conn = s.shapes.add_connector(kind, Emu(0), Emu(0), Emu(100), Emu(100))
        conn.begin_connect(x, y)
        conn.end_connect(u, v)
    line = s.shapes.add_connector(MSO_CONNECTOR.STRAIGHT, Emu(914400), Emu(5486400), Emu(4572000), Emu(5486400))
    ln = line._element.find(f".//{{{P_NS}}}spPr")
    ln.append(p_xml(f'<a:ln xmlns:a="{A_NS}" w="28575"><a:solidFill><a:srgbClr val="C00000"/></a:solidFill><a:prstDash val="dash"/><a:headEnd type="oval"/><a:tailEnd type="triangle" w="lg" len="lg"/></a:ln>'))
    save_pptx(w, "connectors", prs, "Connectors")


def pptx_hyperlinks(w: Writer) -> None:
    from pptx.enum.action import PP_ACTION
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Emu

    prs = new_presentation()
    s1 = blank_slide(prs, "Hyperlinks")
    s2 = blank_slide(prs, "Target slide")
    tb = s1.shapes.add_textbox(Emu(914400), Emu(1828800), Emu(6096000), Emu(914400))
    r = tb.text_frame.paragraphs[0].add_run()
    r.text = "External link to example.org"
    r.hyperlink.address = "https://example.org/"
    btn = s1.shapes.add_shape(MSO_SHAPE.ACTION_BUTTON_FORWARD_OR_NEXT, Emu(914400), Emu(3200400), Emu(1219200), Emu(914400))
    btn.click_action.action  # noqa: B018 - touch the property so the element exists
    sh = s1.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Emu(3048000), Emu(3200400), Emu(3048000), Emu(914400))
    sh.text_frame.text = "Jump to slide 2"
    sh.click_action.target_slide = s2
    _ = PP_ACTION
    save_pptx(w, "hyperlinks", prs, "Hyperlinks")


def pptx_backgrounds(w: Writer) -> None:
    from pptx.dml.color import RGBColor

    prs = new_presentation()
    s = blank_slide(prs, "Solid background")
    s.background.fill.solid()
    s.background.fill.fore_color.rgb = RGBColor(0xDD, 0xEB, 0xF7)
    s = blank_slide(prs, "Gradient background")
    f = s.background.fill
    f.gradient()
    f.gradient_angle = 45
    f.gradient_stops[0].color.rgb = RGBColor(0x44, 0x72, 0xC4)
    f.gradient_stops[1].color.rgb = RGBColor(0xFF, 0xFF, 0xFF)
    s = blank_slide(prs, "Pattern background")
    f = s.background.fill
    f.patterned()
    from pptx.enum.dml import MSO_PATTERN

    f.pattern = MSO_PATTERN.DIAGONAL_BRICK
    f.fore_color.rgb = RGBColor(0xED, 0x7D, 0x31)
    f.back_color.rgb = RGBColor(0xFF, 0xF2, 0xCC)
    blank_slide(prs, "Master background (inherited)")
    save_pptx(w, "backgrounds", prs, "Backgrounds")


def pptx_theme_colors(w: Writer) -> None:
    from pptx.enum.dml import MSO_THEME_COLOR
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Emu

    prs = new_presentation()
    s = blank_slide(prs, "Theme colours and tints")
    accents = [MSO_THEME_COLOR.ACCENT_1, MSO_THEME_COLOR.ACCENT_2, MSO_THEME_COLOR.ACCENT_3, MSO_THEME_COLOR.ACCENT_4, MSO_THEME_COLOR.ACCENT_5, MSO_THEME_COLOR.ACCENT_6]
    for col, accent in enumerate(accents):
        for row, brightness in enumerate([0.8, 0.6, 0.4, 0.0, -0.25, -0.5]):
            sh = s.shapes.add_shape(MSO_SHAPE.RECTANGLE, Emu(914400 + col * 1676400), Emu(1371600 + row * 838200), Emu(1524000), Emu(762000))
            sh.fill.solid()
            sh.fill.fore_color.theme_color = accent
            sh.fill.fore_color.brightness = brightness
            sh.line.fill.background()
    save_pptx(w, "theme-colors", prs, "Theme colours")


def pptx_shape_styles(w: Writer) -> None:
    from pptx.dml.color import RGBColor
    from pptx.enum.dml import MSO_LINE_DASH_STYLE, MSO_PATTERN
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Emu, Pt

    prs = new_presentation()
    s = blank_slide(prs, "Fills, lines and effects")
    boxes = [s.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Emu(609600 + (i % 4) * 2895600), Emu(1371600 + (i // 4) * 2438400), Emu(2590800), Emu(2133600)) for i in range(8)]
    boxes[0].fill.solid()
    boxes[0].fill.fore_color.rgb = RGBColor(0x70, 0xAD, 0x47)
    boxes[1].fill.gradient()
    boxes[1].fill.gradient_angle = 90
    boxes[2].fill.patterned()
    boxes[2].fill.pattern = MSO_PATTERN.WIDE_UPWARD_DIAGONAL
    boxes[2].fill.fore_color.rgb = RGBColor(0x44, 0x72, 0xC4)
    boxes[3].fill.background()
    boxes[3].line.width = Pt(4)
    boxes[3].line.dash_style = MSO_LINE_DASH_STYLE.DASH_DOT
    boxes[3].line.color.rgb = RGBColor(0xC0, 0x00, 0x00)
    effects = [
        '<a:effectLst><a:outerShdw blurRad="76200" dist="38100" dir="2700000" algn="tl" rotWithShape="0"><a:prstClr val="black"><a:alpha val="40000"/></a:prstClr></a:outerShdw></a:effectLst>',
        '<a:effectLst><a:glow rad="139700"><a:schemeClr val="accent2"><a:satMod val="175000"/><a:alpha val="40000"/></a:schemeClr></a:glow></a:effectLst>',
        '<a:effectLst><a:reflection blurRad="6350" stA="50000" endA="300" endPos="55000" dir="5400000" sy="-100000" algn="bl" rotWithShape="0"/></a:effectLst>',
        '<a:effectLst><a:softEdge rad="63500"/></a:effectLst>',
    ]
    for box, fx in zip(boxes[4:], effects):
        sp_pr = box._element.spPr
        sp_pr.append(p_xml(fx.replace("<a:effectLst>", f'<a:effectLst xmlns:a="{A_NS}">')))
    for i, b in enumerate(boxes):
        b.text_frame.text = ["solid", "gradient", "pattern", "dash-dot line", "outer shadow", "glow", "reflection", "soft edge"][i]
    save_pptx(w, "shape-styles", prs, "Shape styles")


def pptx_hidden_and_many(w: Writer) -> None:
    rng = random.Random(16)
    prs = new_presentation()
    for i in range(30):
        s = prs.slides.add_slide(prs.slide_layouts[1])
        s.shapes.title.text = f"Slide {i + 1}"
        s.placeholders[1].text = lorem(rng, 12)
        if i in (3, 17):
            s._element.set("show", "0")
    save_pptx(w, "many-slides", prs, "Thirty slides, two hidden")


def make_pptx(out: Path) -> int:
    w = Writer(out, "pptx")
    for fn in [
        pptx_placeholders,
        pptx_preset_shapes,
        pptx_groups,
        pptx_pictures,
        pptx_tables,
        pptx_charts,
        pptx_transitions,
        pptx_animations,
        pptx_notes,
        pptx_sections,
        lambda x: pptx_aspect(x, True),
        lambda x: pptx_aspect(x, False),
        pptx_text_formatting,
        pptx_connectors,
        pptx_hyperlinks,
        pptx_backgrounds,
        pptx_theme_colors,
        pptx_shape_styles,
        pptx_hidden_and_many,
    ]:
        fn(w)
    return w.count


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument(
        "--out",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "fixtures",
        help="fixtures root (default: the repository's fixtures/)",
    )
    ap.add_argument("--only", default="docx,xlsx,pptx", help="comma-separated formats to generate")
    ap.add_argument("--large", action="store_true", help="also write the 100k-row workbook to xlsx/large/")
    ap.add_argument(
        "--synthetic-large",
        action="store_true",
        help="write only the budget inputs large/doc-300p.docx, sheet-1m.xlsx, deck-200.pptx",
    )
    args = ap.parse_args()
    ensure_dependencies()
    out: Path = args.out
    formats = [f.strip() for f in args.only.split(",") if f.strip()]
    makers = {"docx": make_docx, "xlsx": make_xlsx, "pptx": make_pptx}
    for fmt in formats:
        if fmt not in makers:
            sys.exit(f"make-fixtures: unknown format {fmt}")
    if args.synthetic_large:
        for path in make_synthetic_large(out, formats):
            print(f"{path} ({path.stat().st_size:,} bytes)")
        return
    for fmt in formats:
        n = makers[fmt](out)
        print(f"{fmt}: {n} files in {out / fmt / 'features'}")
    if args.large and "xlsx" in formats:
        path = make_large_xlsx(out)
        print(f"xlsx: large workbook {path}")


if __name__ == "__main__":
    main()
