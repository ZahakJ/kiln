#!/usr/bin/env python3
"""Writes the right-to-left Excel fixtures of this folder (see README.md).

    ~/.venvs/kiln/bin/python -I fixtures/xlsx/rtl/make.py

Needs openpyxl (the project virtualenv). Timestamps are fixed and the zip is
rewritten with fixed entry dates, so running the script again gives the same files.
"""

from __future__ import annotations

import datetime as dt
import io
import zipfile
from pathlib import Path

from openpyxl import Workbook
from openpyxl.chart import BarChart, Reference
from openpyxl.comments import Comment
from openpyxl.formatting.rule import ColorScaleRule, DataBarRule, IconSetRule
from openpyxl.styles import Alignment, Border, Font, PatternFill, Side

HERE = Path(__file__).resolve().parent
ZIP_DATE = (1980, 1, 1, 0, 0, 0)
FIXED = dt.datetime(2026, 1, 15, 9, 30, 0)


def normalize(data: bytes) -> bytes:
    src = zipfile.ZipFile(io.BytesIO(data))
    out = io.BytesIO()
    names = src.namelist()
    order = [n for n in ("[Content_Types].xml", "_rels/.rels") if n in names]
    order += [n for n in names if n not in order]
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as dst:
        for name in order:
            body = src.read(name)
            info = zipfile.ZipInfo(name, date_time=ZIP_DATE)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o600 << 16
            dst.writestr(info, body, compresslevel=6)
    return out.getvalue()


def save(wb: Workbook, name: str) -> None:
    wb.properties.creator = "Kiln Fixtures"
    wb.properties.created = FIXED
    wb.properties.modified = FIXED
    buf = io.BytesIO()
    wb.save(buf)
    (HERE / f"{name}.xlsx").write_bytes(normalize(buf.getvalue()))


ARABIC = Font(name="Arial", size=11)
BOLD = Font(name="Arial", size=11, bold=True)
TITLE = Font(name="Arial", size=14, bold=True, color="1F3864")
HEAD_FILL = PatternFill("solid", fgColor="D9E2F3")
THIN = Side(style="thin", color="808080")
THICK = Side(style="medium", color="C00000")


def sales() -> None:
    wb = Workbook()
    ws = wb.active
    ws.title = "المبيعات"
    ws.sheet_view.rightToLeft = True
    # Title merged across the table.
    ws["A1"] = "تقرير المبيعات الشهري"
    ws["A1"].font = TITLE
    ws.merge_cells("A1:F1")
    ws["A1"].alignment = Alignment(horizontal="center")
    header = ["المنتج", "الفئة", "الكمية", "السعر", "المجموع", "التقييم"]
    for i, h in enumerate(header, start=1):
        c = ws.cell(row=2, column=i, value=h)
        c.font = BOLD
        c.fill = HEAD_FILL
        c.border = Border(top=THIN, bottom=THIN, left=THIN, right=THIN)
    rows = [
        ("أكواب فخارية", "أواني", 120, 4.5, 4),
        ("صحون مزخرفة", "أواني", 80, 7.25, 5),
        ("مزهرية كبيرة", "ديكور", 15, 19.9, 3),
        ("بلاط مزجج", "بناء", 640, 1.2, 2),
        ("Kiln Glaze 2026", "مواد", 45, 12.0, 4),
        ("قوالب جبس", "أدوات", 30, 8.75, 1),
        ("فرشاة طلاء", "أدوات", 200, 0.95, 3),
    ]
    for r, (name, cat, qty, price, stars) in enumerate(rows, start=3):
        ws.cell(row=r, column=1, value=name).font = ARABIC
        ws.cell(row=r, column=2, value=cat).font = ARABIC
        ws.cell(row=r, column=3, value=qty)
        p = ws.cell(row=r, column=4, value=price)
        p.number_format = "#,##0.00"
        t = ws.cell(row=r, column=5, value=f"=C{r}*D{r}")
        t.number_format = "#,##0.00"
        ws.cell(row=r, column=6, value=stars)
        for col in range(1, 7):
            ws.cell(row=r, column=col).border = Border(left=THIN, right=THIN, bottom=THIN)
    last = 2 + len(rows)
    total = last + 1
    ws.cell(row=total, column=1, value="المجموع الكلي").font = BOLD
    ws.merge_cells(start_row=total, start_column=1, end_row=total, end_column=4)
    s = ws.cell(row=total, column=5, value=f"=SUM(E3:E{last})")
    s.number_format = "#,##0.00"
    s.font = BOLD
    # A thick start (left) border: drawn on the right side of the cell in a right-to-left sheet.
    s.border = Border(left=THICK, top=THIN, bottom=THIN, right=THIN)
    # Alignment names the cell's sides, whatever the sheet's direction.
    ws["H2"] = "يسار"
    ws["H2"].alignment = Alignment(horizontal="left")
    ws["H3"] = "يمين"
    ws["H3"].alignment = Alignment(horizontal="right")
    ws["H4"] = "نص طويل يلتف داخل الخلية على عدة أسطر"
    ws["H4"].alignment = Alignment(wrap_text=True, vertical="top")
    ws["H5"] = "نص طويل يمتد إلى الخلايا المجاورة الفارغة"
    ws["H7"] = 0.155
    ws["H7"].number_format = "0.0%"
    ws["H8"] = dt.date(2026, 3, 14)
    ws["H8"].number_format = "yyyy-mm-dd"
    ws["H4"].comment = Comment("ملاحظة: يلتف النص.", "Kiln")
    ws["A4"].comment = Comment("الأكثر مبيعاً", "Kiln")
    for col, width in zip("ABCDEFGH", (18, 10, 9, 9, 12, 10, 3, 22)):
        ws.column_dimensions[col].width = width
    ws.row_dimensions[1].height = 24
    ws.row_dimensions[4].height = 45
    # Frozen header rows and the product column.
    ws.freeze_panes = "B3"
    # Conditional formats: data bars on the quantities, a colour scale on the totals, icons on
    # the rating.
    ws.conditional_formatting.add(f"C3:C{last}", DataBarRule(start_type="min", end_type="max", color="5B9BD5"))
    ws.conditional_formatting.add(
        f"E3:E{last}",
        ColorScaleRule(start_type="min", start_color="F8696B", mid_type="percentile", mid_value=50,
                       mid_color="FFEB84", end_type="max", end_color="63BE7B"),
    )
    ws.conditional_formatting.add(f"F3:F{last}", IconSetRule("3Arrows", "num", [1, 2, 4]))
    # A chart of the totals.
    chart = BarChart()
    chart.title = "المجموع حسب المنتج"
    chart.y_axis.title = "ريال"
    data = Reference(ws, min_col=5, min_row=2, max_row=last)
    cats = Reference(ws, min_col=1, min_row=3, max_row=last)
    chart.add_data(data, titles_from_data=True)
    chart.set_categories(cats)
    chart.width = 14
    chart.height = 7
    ws.add_chart(chart, "A13")
    # A second, left-to-right sheet for contrast.
    ltr = wb.create_sheet("Notes")
    ltr["A1"] = "This sheet runs left to right."
    ltr["A2"] = 2026
    save(wb, "sales")


def main() -> None:
    sales()


if __name__ == "__main__":
    main()
