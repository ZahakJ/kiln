# SPDX-License-Identifier: AGPL-3.0-or-later
"""The Kiln Grid templates: working workbooks with formulas, number formats, tables and charts."""

from kiln_script import Grid, side

GBP = '"£"#,##0.00'
GBP0 = '"£"#,##0'
PCT = "0%"
PCT1 = "0.0%"
DATE = "d mmm yyyy"


def sheet_title(g: Grid, title: str, subtitle: str, accent: str, *, width: str = "A1") -> None:
    """A large title and a quiet subtitle in A1:A2, gridlines hidden."""
    g.g("Gridlines")
    g.put("A1", title)
    g.fmt("A1", font_family="Caladea", font_size=22.0, font_color=accent)
    g.put("A2", subtitle)
    g.fmt("A2", font_size=10.0, font_color="6B6B6B")
    g.height("1", 34)
    g.height("2", 18)


def header(g: Grid, ref: str, fill: str, color: str = "FFFFFF") -> None:
    g.fmt(ref, bold=True, font_color=color, fill_color=fill, v_align="Center",
          border_bottom=side("Thin", fill))


def band(g: Grid, ref: str, color: str = "E5E5E5") -> None:
    """Thin rules between the rows of a range."""
    g.fmt(ref, border_inside_h=side("Thin", color), border_bottom=side("Thin", color))


def total_row(g: Grid, ref: str, accent: str, fill: str | None = None) -> None:
    g.fmt(ref, bold=True, border_top=side("Thin", accent), border_bottom=side("Double", accent),
          **({"fill_color": fill} if fill else {}))


def label(g: Grid, ref: str, text: str, accent: str) -> None:
    """A small capitalised section label."""
    g.put(ref, text)
    g.fmt(ref, bold=True, font_size=9.0, font_color=accent)


def chart(g: Grid, data: str, kind: str, title: str, at: tuple[float, float, float, float],
          colors: int | None = None, legend: str | None = "Bottom") -> None:
    g.select(data)
    g.cmd({"Suite": {"ChartInsert": {"kind": kind}}})
    g.cmd({"Suite": {"ChartSetTitle": {"text": title}}})
    if colors is not None:
        g.cmd({"Suite": {"ChartSetColors": {"palette": colors}}})
    g.cmd({"Suite": {"ChartSetLegend": {"position": legend}}})
    x, y, w, h = at
    g.g("PlaceDrawing", {"index": 0, "x": x, "y": y, "width": w, "height": h})


def finish(g: Grid, sheet: str, cell: str = "B2") -> Grid:
    """Names the sheet and gives the content a margin: a narrow first column and a short first
    row are inserted before it (formulas and charts move with their cells)."""
    g.select("A1")
    g.g("InsertColumns")
    g.g("InsertRows")
    g.width("A", 2.5)
    g.height("1", 10)
    g.g("RenameSheet", {"index": 0, "name": sheet})
    g.select(cell)
    return g


# ── Budget ────────────────────────────────────────────────────────────────────────────────────


def budget() -> Grid:
    a = "1D6B4F"
    g = Grid("budget")
    sheet_title(g, "Monthly budget", "Household budget · January 2026 · amounts in pounds", a)
    g.width("A", 30)
    g.width("B:D", 13)
    g.width("E", 3)
    g.width("F:H", 13)

    # Summary cards.
    label(g, "A4", "SUMMARY", a)
    g.rows("A5", [["Total income", "=C15"],
                  ["Total spending", "=C27"],
                  ["Left over", "=B5-B6"],
                  ["Saved this month", "=B7/B5"]])
    g.number("B5:B7", GBP0)
    g.number("B8", PCT1)
    g.fmt("A5:B8", font_size=11.0)
    g.fmt("B5:B8", bold=True, h_align="Right")
    g.fmt("B7", font_color=a)
    band(g, "A5:B8")

    label(g, "A10", "INCOME", a)
    g.rows("A11", [["Income", "Planned", "Actual", "Difference"]])
    header(g, "A11:D11", a)
    g.rows("A12", [["Salary (after tax)", 3150, 3150, "=C12-B12"],
                   ["Freelance work", 400, 520, "=C13-B13"],
                   ["Other", 50, 35, "=C14-B14"],
                   ["Total income", "=SUM(B12:B14)", "=SUM(C12:C14)", "=C15-B15"]])
    total_row(g, "A15:D15", a, "E3F0EA")

    label(g, "A17", "SPENDING", a)
    g.rows("A18", [["Spending", "Planned", "Actual", "Difference"]])
    header(g, "A18:D18", a)
    spending = [
        ("Rent", 1150, 1150),
        ("Council tax", 165, 165),
        ("Energy and water", 190, 214),
        ("Groceries", 420, 447),
        ("Transport", 140, 126),
        ("Phone and internet", 55, 55),
        ("Eating out", 120, 158),
        ("Subscriptions", 38, 38),
    ]
    for i, (name, plan, actual) in enumerate(spending):
        r = 19 + i
        g.row(f"A{r}", [name, plan, actual, f"=B{r}-C{r}"])
    g.rows("A27", [["Total spending", "=SUM(B19:B26)", "=SUM(C19:C26)", "=B27-C27"]])
    total_row(g, "A27:D27", a, "E3F0EA")
    band(g, "A12:D14")
    band(g, "A19:D26")
    g.number("B12:D15", GBP0)
    g.number("B19:D27", GBP0)
    # Overspending shows in red.
    g.select("D19:D27")
    g.g("AddConditionalFormat", {"rule": {
        "id": 0, "applies_to": "", "stop_if_true": False,
        "kind": {"CellValue": {"op": "LessThan", "values": ["0"]}},
        "style": {"fill": "FBE3E1", "font_color": "B42318", "bold": False, "italic": False,
                  "underline": False, "strike": False, "border": None, "number_format": None}}})
    g.fmt("B11:D11", h_align="Right")
    g.fmt("B18:D18", h_align="Right")

    chart(g, "A18:C26", "BarClustered", "Planned and actual spending", (540, 84, 400, 400))
    return finish(g, "January")


def cf(g: Grid, ref: str, kind: dict, style: dict | None = None) -> None:
    """A conditional-formatting rule on `ref`."""
    full = None
    if style is not None:
        full = {"fill": None, "font_color": None, "bold": False, "italic": False, "underline": False,
                "strike": False, "border": None, "number_format": None}
        full.update(style)
    g.select(ref)
    g.g("AddConditionalFormat", {"rule": {"id": 0, "applies_to": "", "stop_if_true": False,
                                          "kind": kind, "style": full}})


def data_bar(g: Grid, ref: str, color: str, top: str | None = None) -> None:
    mx = {"kind": "Number", "value": top, "gte": False} if top else {"kind": "Max", "value": "", "gte": False}
    cf(g, ref, {"DataBar": {"min": {"kind": "Number", "value": "0", "gte": False}, "max": mx,
                            "color": color, "gradient": False, "show_value": True,
                            "negative_color": None, "border_color": None}})


def text_is(g: Grid, ref: str, text: str, fill: str, color: str) -> None:
    cf(g, ref, {"CellValue": {"op": "Equal", "values": [f'"{text}"']}},
       {"fill": fill, "font_color": color, "bold": True})


def table(g: Grid, ref: str, style: str) -> None:
    from kiln_script import rng
    g.g("CreateTable", {"range": rng(ref), "header": True, "style": style})


# ── Invoice ───────────────────────────────────────────────────────────────────────────────────


def invoice() -> Grid:
    a = "B4532A"
    g = Grid("invoice")
    g.g("Gridlines")
    g.width("A", 34)
    g.width("B", 10)
    g.width("C:D", 14)
    g.put("A1", "Wren & Pike Studio")
    g.fmt("A1", font_family="Caladea", font_size=20.0, font_color="222222")
    g.put("D1", "INVOICE")
    g.fmt("D1", font_family="Caladea", font_size=22.0, font_color=a, h_align="Right")
    g.height("1", 34)
    g.rows("A2", [["Unit 3, Mill Yard, Sheffield S3 8PB", None, "Invoice no.", "INV-2026-0142"],
                  ["hello@wrenpike.example · +44 114 496 0782", None, "Issued", "2026-03-03"],
                  ["VAT reg. GB 284 1937 06", None, "Terms (days)", 30],
                  [None, None, "Due", "=D3+D4"]])
    g.fmt("A2:A4", font_size=9.0, font_color="6B6B6B")
    g.fmt("C2:C5", font_size=9.0, font_color="6B6B6B", h_align="Right")
    g.fmt("D2:D5", bold=True, h_align="Right")
    g.number("D3", DATE)
    g.number("D5", DATE)
    label(g, "A7", "BILL TO", a)
    g.rows("A8", [["Moorland Coffee Roasters Ltd"], ["Attn: Sam Achebe, Head of Marketing"],
                  ["27 Kelham Island Road, Sheffield S3 8RY"]])
    g.fmt("A8", bold=True)
    g.rows("A12", [["Description", "Qty", "Rate", "Amount"]])
    header(g, "A12:D12", a)
    g.fmt("B12:D12", h_align="Right")
    items = [("Brand identity refresh", 1, 3200), ("Packaging design, single-origin labels", 3, 650),
             ("Photography direction, one-day shoot", 1, 720), ("Print management (hours)", 6, 55),
             ("Delivery of print-ready files", 1, 0)]
    for i, (desc, q, rate) in enumerate(items):
        r = 13 + i
        g.row(f"A{r}", [desc, q, rate, f"=B{r}*C{r}"])
    band(g, "A13:D17")
    g.rows("C19", [["Subtotal", "=SUM(D13:D17)"], ["VAT rate", 0.2], ["VAT", "=D19*D20"],
                   ["Total due", "=D19+D21"]])
    g.number("C13:D19", GBP)
    g.number("D21:D22", GBP)
    g.number("D20", PCT)
    g.fmt("C19:C22", h_align="Right", font_color="6B6B6B")
    g.fmt("C22:D22", bold=True, font_color=a, font_size=13.0, border_top=side("Medium", a))
    label(g, "A25", "PAYMENT DETAILS", a)
    g.rows("A26", [["Please pay by bank transfer, quoting the invoice number."],
                   ["Northern Mutual Bank · Sort code 40-12-76 · Account 81457362"]])
    g.fmt("A27", font_size=9.0, font_color="6B6B6B")
    g.put("A29", "Thank you for your business.")
    g.fmt("A29", font_family="Caladea", italic=True, font_size=12.0, font_color=a)
    return finish(g, "Invoice")


# ── Schedule ──────────────────────────────────────────────────────────────────────────────────


def schedule() -> Grid:
    a = "3A4E9C"
    g = Grid("schedule")
    sheet_title(g, "Weekly schedule", "Studio rota · week of 9 March 2026", a)
    g.width("A", 9)
    g.width("B:F", 19)
    days = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"]
    g.rows("A4", [["Time", *days]])
    header(g, "A4:F4", a)
    g.fmt("B4:F4", h_align="Center")
    times = ["08:00", "09:00", "10:00", "11:00", "12:00", "13:00", "14:00", "15:00", "16:00", "17:00"]
    for i, t in enumerate(times):
        g.put(f"A{5 + i}", t)
    g.number("A5:A14", "hh:mm")
    g.fmt("A5:A14", font_color="6B6B6B", v_align="Top")
    g.height("5:14", 30)
    blocks = {
        "B": {5: "Team stand-up", 6: "Client workshop", 7: "Client workshop", 9: "Lunch", 10: "Design review",
              12: "Focus time", 13: "Focus time"},
        "C": {5: "Team stand-up", 6: "Focus time", 7: "Focus time", 9: "Lunch", 10: "Supplier call",
              11: "Studio tidy", 13: "Reading group"},
        "D": {5: "Team stand-up", 7: "Planning", 8: "Planning", 9: "Lunch", 11: "Portfolio shoot",
              12: "Portfolio shoot"},
        "E": {5: "Team stand-up", 6: "Focus time", 7: "Focus time", 9: "Lunch", 10: "Client call",
              12: "Mentoring", 13: "Mentoring"},
        "F": {5: "Team stand-up", 6: "Show and tell", 9: "Team lunch", 11: "Admin", 12: "Week review"},
    }
    colors = {"Team stand-up": ("E8ECF8", a), "Client workshop": ("FDE8DC", "9A3B12"),
              "Focus time": ("E4F3EC", "1D6B4F"), "Lunch": ("F2F2F2", "6B6B6B"),
              "Team lunch": ("F2F2F2", "6B6B6B"), "Design review": ("F3E8F8", "6B2E8A"),
              "Supplier call": ("FFF4D6", "8A6100"), "Client call": ("FFF4D6", "8A6100"),
              "Studio tidy": ("F2F2F2", "6B6B6B"), "Reading group": ("F3E8F8", "6B2E8A"),
              "Planning": ("E8ECF8", a), "Portfolio shoot": ("FDE8DC", "9A3B12"),
              "Mentoring": ("E4F3EC", "1D6B4F"), "Show and tell": ("F3E8F8", "6B2E8A"),
              "Admin": ("F2F2F2", "6B6B6B"), "Week review": ("E8ECF8", a)}
    for col, cells in blocks.items():
        for row, what in cells.items():
            g.put(f"{col}{row}", what)
            fill, ink = colors[what]
            g.fmt(f"{col}{row}", fill_color=fill, font_color=ink, bold=True, font_size=10.0,
                  v_align="Center", border_left=side("Thick", ink))
    g.fmt("A5:F14", border_inside_h=side("Thin", "E5E5E5"), border_bottom=side("Thin", "E5E5E5"))
    g.put("A16", "Booked")
    for col in "BCDEF":
        g.put(f"{col}16", f'=COUNTA({col}5:{col}14)&" of 10 hours"')
    g.fmt("A16:F16", font_color="6B6B6B", font_size=9.0, h_align="Center")
    g.fmt("A16", h_align="Left", bold=True)
    g.select("B5")
    g.g("FreezePanes")
    return finish(g, "Week 11")


# ── Project tracker ───────────────────────────────────────────────────────────────────────────


def tracker() -> Grid:
    a = "2F5597"
    g = Grid("tracker")
    sheet_title(g, "Project tracker", "Website relaunch · project lead Ines Duarte", a)
    g.width("A", 36)
    g.width("B", 12)
    g.width("C:D", 13)
    g.width("E", 12)
    g.width("F", 12)
    g.width("G", 11)
    label(g, "A4", "AT A GLANCE", a)
    g.rows("A5", [["Status date", "2026-03-16"], ["Tasks done", '=COUNTIF(E11:E22,"Done")&" of "&COUNTA(A11:A22)'],
                  ["At risk", '=COUNTIF(E11:E22,"At risk")'], ["Overall progress", "=AVERAGE(F11:F22)"]])
    g.number("B5", DATE)
    g.number("B8", PCT)
    g.fmt("B5:B8", bold=True, h_align="Right")
    g.fmt("B5", font_color="1F4FB4", fill_color="EEF3FB")
    band(g, "A5:B8")
    g.rows("A10", [["Task", "Owner", "Start", "Due", "Status", "Progress", "Days left"]])
    tasks = [
        ("Agree goals and success measures", "Ines", "2026-02-02", "2026-02-06", "Done", 1),
        ("Audit current content", "Theo", "2026-02-04", "2026-02-20", "Done", 1),
        ("Interview eight customers", "Maya", "2026-02-09", "2026-02-27", "Done", 1),
        ("Site map and navigation", "Theo", "2026-02-23", "2026-03-06", "Done", 1),
        ("Visual design: home and product pages", "Femi", "2026-03-02", "2026-03-20", "In progress", 0.6),
        ("Write new product copy", "Clara", "2026-03-05", "2026-03-27", "In progress", 0.4),
        ("Build page templates", "Rohan", "2026-03-16", "2026-04-10", "In progress", 0.15),
        ("Migrate blog archive", "Rohan", "2026-03-23", "2026-04-03", "At risk", 0),
        ("Accessibility review", "Maya", "2026-04-06", "2026-04-14", "Not started", 0),
        ("Load and browser testing", "Josh", "2026-04-08", "2026-04-17", "Not started", 0),
        ("Launch plan and redirects", "Ines", "2026-04-13", "2026-04-22", "Not started", 0),
        ("Go live", "Ines", "2026-04-27", "2026-04-27", "Not started", 0),
    ]
    for i, (task, owner, start, due, status, prog) in enumerate(tasks):
        r = 11 + i
        g.row(f"A{r}", [task, owner, start, due, status, prog,
                        f'=IF(E{r}="Done",0,MAX(0,D{r}-$B$5))'])
    table(g, "A10:G22", "TableStyleMedium2")
    g.number("C11:D22", "d mmm")
    g.number("F11:F22", PCT)
    g.fmt("C10:D22", h_align="Right")
    g.fmt("F10:G22", h_align="Right")
    data_bar(g, "F11:F22", "8FAADC", top="1")
    text_is(g, "E11:E22", "Done", "E4F3EC", "1D6B4F")
    text_is(g, "E11:E22", "At risk", "FBE3E1", "B42318")
    text_is(g, "E11:E22", "In progress", "FFF4D6", "8A6100")
    g.select("A11")
    g.g("FreezePanes")
    return finish(g, "Tasks")


# ── Loan ──────────────────────────────────────────────────────────────────────────────────────


def loan() -> Grid:
    a = "0F5C8C"
    g = Grid("loan")
    sheet_title(g, "Loan calculator", "Repayments and amortisation schedule · change the blue cells", a)
    g.width("A", 22)
    g.width("B", 14)
    g.width("C", 3)
    g.width("D", 8)
    g.width("E:J", 13)
    label(g, "A4", "LOAN DETAILS", a)
    g.rows("A5", [["Amount borrowed", 18500], ["Annual interest rate", 0.069], ["Term (years)", 5],
                  ["Payments per year", 12], ["First payment", "2026-04-01"]])
    g.number("B5", GBP)
    g.number("B6", "0.00%")
    g.number("B9", DATE)
    g.fmt("B5:B9", font_color="1F4FB4", fill_color="EEF3FB", h_align="Right")
    band(g, "A5:B9")
    label(g, "A11", "SUMMARY", a)
    g.rows("A12", [["Monthly payment", "=-PMT(B6/B8,B7*B8,B5)"], ["Number of payments", "=B7*B8"],
                   ["Total repaid", "=B12*B13"], ["Total interest", "=B14-B5"],
                   ["Last payment", "=EDATE(B9,B13-1)"]])
    g.number("B12", GBP)
    g.number("B14:B15", GBP)
    g.number("B16", DATE)
    g.fmt("B12:B16", bold=True, h_align="Right")
    g.fmt("A12:B12", font_size=13.0, font_color=a)
    band(g, "A12:B16")
    g.rows("D4", [["No.", "Date", "Opening", "Payment", "Interest", "Principal", "Closing"]])
    header(g, "D4:J4", a)
    g.fmt("E4:J4", h_align="Right")
    n = 60
    for k in range(1, n + 1):
        r = 4 + k
        opening = "=$B$5" if k == 1 else f"=J{r - 1}"
        g.row(f"D{r}", [k, f"=EDATE($B$9,D{r}-1)", opening, "=$B$12", f"=F{r}*$B$6/$B$8",
                        f"=G{r}-H{r}", f"=MAX(0,ROUND(F{r}-I{r},2))"])
    g.number(f"E5:E{4 + n}", "mmm yyyy")
    g.number(f"F5:J{4 + n}", '#,##0.00')
    g.fmt(f"D5:D{4 + n}", h_align="Center", font_color="6B6B6B")
    band(g, f"D5:J{4 + n}", "EDEDED")
    g.select("D5")
    g.g("FreezePanes")
    return finish(g, "Loan")


# ── Gradebook ─────────────────────────────────────────────────────────────────────────────────


def gradebook() -> Grid:
    a = "8A3B6B"
    g = Grid("gradebook")
    sheet_title(g, "Gradebook", "Year 10 Geography · autumn term 2025 · teacher: Ms R. Adeyemi", a)
    g.width("A", 24)
    g.width("B:F", 11)
    g.width("G:H", 11)
    tasks = ["Map skills", "Rivers essay", "Fieldwork", "Mid-term test", "Final exam"]
    g.rows("A4", [["Weight", 0.1, 0.2, 0.2, 0.2, 0.3, "=SUM(B4:F4)"]])
    g.number("B4:G4", PCT)
    g.fmt("A4:G4", italic=True, font_color="6B6B6B", h_align="Right")
    g.fmt("A4", h_align="Left")
    g.rows("A5", [["Student", *tasks, "Average", "Grade"]])
    header(g, "A5:H5", a)
    g.fmt("B5:H5", h_align="Center", wrap=True)
    g.height("5", 30)
    students = [
        ("Aisha Bello", [86, 78, 91, 74, 82]), ("Ben Carter", [64, 58, 70, 61, 66]),
        ("Chloe Dubois", [92, 88, 95, 90, 93]), ("Dylan Evans", [71, 69, 75, 68, 72]),
        ("Elif Kaya", [55, 62, 60, 49, 57]), ("Finn O'Neill", [80, 84, 77, 79, 85]),
        ("Grace Huang", [97, 91, 94, 96, 98]), ("Harvey Mills", [68, 74, 66, 70, 63]),
        ("Isla Novak", [74, 81, 88, 77, 80]), ("Jamal Reid", [59, 66, 72, 64, 69]),
        ("Kiran Shah", [88, 79, 85, 83, 87]), ("Lena Vogel", [77, 72, 70, 75, 74]),
    ]
    for i, (name, marks) in enumerate(students):
        r = 6 + i
        g.row(f"A{r}", [name, *marks, f"=SUMPRODUCT(B{r}:F{r},$B$4:$F$4)/$G$4",
                        f'=IF(G{r}>=85,"A*",IF(G{r}>=75,"A",IF(G{r}>=65,"B",IF(G{r}>=55,"C","D"))))'])
    last = 5 + len(students)
    g.rows(f"A{last + 1}", [["Class average", *[f"=AVERAGE({c}6:{c}{last})" for c in "BCDEFG"]]])
    total_row(g, f"A{last + 1}:H{last + 1}", a, "F6EAF1")
    g.number(f"B6:G{last + 1}", "0")
    g.fmt(f"B6:H{last + 1}", h_align="Center")
    g.fmt(f"G6:H{last}", bold=True)
    band(g, f"A6:H{last}")
    cf(g, f"B6:F{last}", {"ColorScale": {"points": [
        {"kind": "Min", "value": "", "gte": False},
        {"kind": "Percentile", "value": "50", "gte": False},
        {"kind": "Max", "value": "", "gte": False}],
        "colors": ["F8D7D3", "FFF6DA", "D6EEDF"]}})
    g.rows("J5", [["Grade", "Students"]])
    header(g, "J5:K5", a)
    for i, gr in enumerate(["A*", "A", "B", "C", "D"]):
        g.row(f"J{6 + i}", [gr, f'=COUNTIF($H$6:$H${last},J{6 + i})'])
    g.fmt("J6:K10", h_align="Center")
    band(g, "J6:K10")
    g.width("I", 3)
    g.width("J:K", 10)
    g.select("B6")
    g.g("FreezePanes")
    return finish(g, "Autumn term")


# ── Expenses ──────────────────────────────────────────────────────────────────────────────────


def expenses() -> Grid:
    a = "4E7A2C"
    g = Grid("expenses")
    sheet_title(g, "Expense report", "Claimant: Rowan Ellis · Client visits, February 2026", a)
    g.width("A", 12)
    g.width("B", 14)
    g.width("C", 36)
    g.width("D:E", 12)
    g.width("F", 3)
    g.width("G", 16)
    g.width("H", 12)
    g.rows("A4", [["Date", "Category", "Description", "Amount", "VAT"]])
    rows = [
        ("2026-02-03", "Travel", "Train, Leeds to London return", 142.6, 0),
        ("2026-02-03", "Meals", "Lunch with client team (4)", 86.4, 14.4),
        ("2026-02-04", "Lodging", "Hotel, one night", 129, 21.5),
        ("2026-02-04", "Travel", "Underground and buses", 18.2, 0),
        ("2026-02-11", "Mileage", "Client site, 84 miles at 45p", 37.8, 0),
        ("2026-02-12", "Meals", "Working dinner", 41.25, 6.88),
        ("2026-02-18", "Supplies", "Presentation boards and printing", 54.99, 9.17),
        ("2026-02-24", "Travel", "Taxi to the airport", 32, 0),
        ("2026-02-24", "Lodging", "Hotel, two nights", 236, 39.33),
        ("2026-02-25", "Meals", "Breakfast meetings", 23.7, 3.95),
    ]
    for i, row in enumerate(rows):
        g.row(f"A{5 + i}", list(row))
    last = 4 + len(rows)
    table(g, f"A4:E{last}", "TableStyleMedium7")
    g.fmt("D4:E4", h_align="Right")
    g.number(f"A5:A{last}", "d mmm")
    g.number(f"D5:E{last + 1}", GBP)
    g.rows(f"C{last + 1}", [["Total claimed", f"=SUM(D5:D{last})", f"=SUM(E5:E{last})"]])
    g.fmt(f"C{last + 1}", h_align="Right")
    total_row(g, f"C{last + 1}:E{last + 1}", a)
    g.rows("G4", [["By category", "Amount"]])
    header(g, "G4:H4", "70AD47")
    g.fmt("H4", h_align="Right")
    for i, cat in enumerate(["Travel", "Lodging", "Meals", "Mileage", "Supplies"]):
        g.row(f"G{5 + i}", [cat, f'=SUMIF($B$5:$B${last},G{5 + i},$D$5:$D${last})'])
    g.number("H5:H9", GBP)
    band(g, "G5:H9")
    chart(g, "G4:H9", "Doughnut", "Spending by category", (690, 210, 330, 270), legend="Right")
    r = last + 4
    label(g, f"A{r}", "APPROVAL", a)
    g.rows(f"A{r + 1}", [["Submitted", None, "Rowan Ellis, 27 February 2026"],
                         ["Approved", None, "Signature and date"]])
    g.fmt(f"C{r + 2}", font_color="9A9A9A", italic=True, border_bottom=side("Thin", "9A9A9A"))
    g.fmt(f"A{r + 1}:A{r + 2}", bold=True)
    return finish(g, "February")


# ── Inventory ─────────────────────────────────────────────────────────────────────────────────


def inventory() -> Grid:
    a = "7A5A16"
    g = Grid("inventory")
    sheet_title(g, "Stock inventory", "Ember Lane Ceramics · stock count 1 March 2026", a)
    g.width("A", 10)
    g.width("B", 30)
    g.width("C", 14)
    g.width("D:G", 12)
    g.width("H", 13)
    g.rows("A4", [["SKU", "Item", "Category", "In stock", "Reorder at", "Unit cost", "Stock value", "Status"]])
    items = [
        ("EL-101", "Speckled mug, 350 ml", "Mugs", 64, 30, 6.2),
        ("EL-102", "Tall mug, 450 ml", "Mugs", 18, 30, 7.1),
        ("EL-201", "Side plate, 20 cm", "Plates", 120, 40, 5.4),
        ("EL-202", "Dinner plate, 27 cm", "Plates", 46, 40, 8.9),
        ("EL-301", "Pasta bowl", "Bowls", 32, 25, 7.6),
        ("EL-302", "Nesting bowls, set of 3", "Bowls", 9, 12, 21.5),
        ("EL-401", "Bud vase", "Vases", 27, 10, 9.8),
        ("EL-402", "Tall vase, ash glaze", "Vases", 6, 8, 24),
        ("EL-501", "Teapot, 1 litre", "Tea", 14, 10, 28.4),
        ("EL-502", "Tea bowl", "Tea", 52, 20, 6.75),
        ("EL-601", "Planter, 15 cm", "Garden", 23, 15, 11.2),
        ("EL-602", "Bird bath", "Garden", 3, 4, 46),
    ]
    for i, (sku, item, cat, stock, reorder, cost) in enumerate(items):
        r = 5 + i
        g.row(f"A{r}", [sku, item, cat, stock, reorder, cost, f"=D{r}*F{r}",
                        f'=IF(D{r}<=E{r},"Reorder","In stock")'])
    last = 4 + len(items)
    table(g, f"A4:H{last}", "TableStyleMedium5")
    header(g, "A4:H4", a)
    g.number(f"F5:G{last + 1}", GBP)
    g.fmt(f"D4:G{last}", h_align="Right")
    text_is(g, f"H5:H{last}", "Reorder", "FBE3E1", "B42318")
    text_is(g, f"H5:H{last}", "In stock", "E4F3EC", "1D6B4F")
    data_bar(g, f"D5:D{last}", "E0B65B")
    g.rows(f"F{last + 1}", [["Total", f"=SUM(G5:G{last})"]])
    g.rows(f"A{last + 1}", [[None, f'=COUNTIF(H5:H{last},"Reorder")&" items to reorder"']])
    g.fmt(f"B{last + 1}", font_color="B42318", bold=True)
    g.fmt(f"F{last + 1}", h_align="Right")
    total_row(g, f"F{last + 1}:G{last + 1}", a)
    g.select("A5")
    g.g("FreezePanes")
    return finish(g, "Stock")


# ── Sales ─────────────────────────────────────────────────────────────────────────────────────


def sales() -> Grid:
    a = "C2410C"
    g = Grid("sales")
    sheet_title(g, "Sales report", "Monthly sales by region, 2025 · thousands of pounds", a)
    g.width("A", 12)
    g.width("B:F", 11)
    g.width("G", 3)
    regions = ["North", "Midlands", "South", "Online"]
    g.rows("A4", [["Month", *regions, "Total"]])
    header(g, "A4:F4", a)
    g.fmt("B4:F4", h_align="Right")
    months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
    data = [(42, 38, 55, 61), (39, 36, 51, 64), (47, 41, 58, 70), (51, 44, 62, 73), (55, 47, 66, 79),
            (58, 50, 71, 84), (54, 48, 69, 88), (52, 46, 67, 91), (57, 51, 72, 95), (61, 54, 76, 102),
            (66, 58, 83, 118), (74, 63, 92, 136)]
    for i, (m, vals) in enumerate(zip(months, data)):
        r = 5 + i
        g.row(f"A{r}", [m, *vals, f"=SUM(B{r}:E{r})"])
    g.rows("A17", [["Year", *[f"=SUM({c}5:{c}16)" for c in "BCDEF"]],
                   ["Share", *[f"={c}17/$F$17" for c in "BCDEF"]],
                   ["Growth", *[f"={c}16/{c}5-1" for c in "BCDEF"]]])
    g.number("B5:F17", "#,##0")
    g.number("B18:F19", PCT)
    g.fmt("F5:F17", bold=True)
    band(g, "A5:F16")
    total_row(g, "A17:F17", a, "FDEDE3")
    g.fmt("A18:F19", italic=True, font_color="6B6B6B")
    cf(g, "F5:F16", {"DataBar": {"min": {"kind": "Number", "value": "0", "gte": False},
                                 "max": {"kind": "Max", "value": "", "gte": False},
                                 "color": "F4A77D", "gradient": False, "show_value": True,
                                 "negative_color": None, "border_color": None}})
    chart(g, "A4:E16", "ColumnStacked", "Sales by region", (530, 84, 470, 340))
    return finish(g, "2025")


# ── Timesheet ─────────────────────────────────────────────────────────────────────────────────


def timesheet() -> Grid:
    a = "1F6F8B"
    g = Grid("timesheet")
    sheet_title(g, "Weekly timesheet", "Northgate Community Library", a)
    g.width("A", 14)
    g.width("B", 17)
    g.width("C", 10)
    g.width("D", 14)
    g.width("E", 11)
    g.width("F", 10)
    g.width("G", 18)
    g.rows("A4", [["Employee", "Joanna Pryce", None, "Week starting", "2026-03-09"],
                  ["Role", "Library assistant", None, "Hourly rate", 13.4],
                  ["Contract hours", 30, None, "Overtime rate", "=E5*1.5"]])
    g.fmt("A4:A6", bold=True, font_color=a)
    g.fmt("D4:D6", bold=True, font_color=a)
    g.number("E4", DATE)
    g.number("E5:E6", GBP)
    g.fmt("B4:B6", h_align="Left")
    g.rows("A8", [["Day", "Date", "Start", "Finish", "Break (min)", "Hours", "Notes"]])
    header(g, "A8:G8", a)
    days = [("Monday", "09:00", "17:00", 45, ""), ("Tuesday", "09:00", "17:30", 45, "Story time"),
            ("Wednesday", "12:00", "20:00", 30, "Late opening"), ("Thursday", "09:00", "15:00", 30, ""),
            ("Friday", "09:00", "17:00", 45, "Stock delivery"), ("Saturday", "10:00", "14:00", 0, "Code club"),
            ("Sunday", "", "", "", "")]
    for i, (day, start, finish_, brk, note) in enumerate(days):
        r = 9 + i
        g.row(f"A{r}", [day, f"=$E$4+{i}", start or None, finish_ or None, brk if brk != "" else None,
                        f'=IF(C{r}="",0,(D{r}-C{r})*24-E{r}/60)', note or None])
    g.number("B9:B15", "d mmm")
    g.number("C9:D15", "hh:mm")
    g.number("F9:F15", "0.00")
    g.fmt("B8:F15", h_align="Right")
    band(g, "A9:G15")
    g.rows("E17", [["Total hours", "=SUM(F9:F15)"], ["Overtime", "=MAX(0,F17-B6)"],
                   ["Standard pay", "=(F17-F18)*E5"], ["Overtime pay", "=F18*E6"],
                   ["Gross pay", "=F19+F20"]])
    g.number("F17:F18", "0.00")
    g.number("F19:F21", GBP)
    g.fmt("E17:E21", h_align="Right", font_color="6B6B6B")
    g.fmt("F17:F21", bold=True)
    total_row(g, "E21:F21", a, "E3EFF3")
    g.rows("A23", [["Employee signature"], [None], ["Manager signature"]])
    g.fmt("B23:D23", border_bottom=side("Thin", "9A9A9A"))
    g.fmt("B25:D25", border_bottom=side("Thin", "9A9A9A"))
    g.fmt("A23:A25", font_color="6B6B6B")
    return finish(g, "Week 11")


# ── Savings ───────────────────────────────────────────────────────────────────────────────────


def savings() -> Grid:
    a = "0E7C66"
    g = Grid("savings")
    sheet_title(g, "Savings goals", "Personal savings plan · Jordan and Sam Achterberg", a)
    g.width("A", 24)
    g.width("B:D", 12)
    g.width("E", 12)
    g.width("F", 12)
    g.width("G", 14)
    g.put("F2", "Progress as of")
    g.put("G2", "2026-04-01")
    g.number("G2", DATE)
    g.fmt("F2", font_size=10.0, font_color="6B6B6B", h_align="Right")
    g.fmt("G2", font_size=10.0, bold=True, font_color="1F4FB4", fill_color="EEF3FB", h_align="Right")
    g.rows("A4", [["Goal", "Target", "Saved", "Progress", "Still to save", "Target date", "Per month"]])
    header(g, "A4:G4", a)
    g.fmt("B4:G4", h_align="Right")
    goals = [("Emergency fund", 6000, 4350, "2026-09-30"), ("Summer holiday", 2400, 1580, "2026-07-15"),
             ("New bicycle", 950, 720, "2026-05-31"), ("House deposit", 25000, 8200, "2028-12-31"),
             ("Christmas", 800, 150, "2026-12-01"), ("Laptop", 1300, 1300, "2026-03-31")]
    for i, (goal, target, saved, date) in enumerate(goals):
        r = 5 + i
        g.row(f"A{r}", [goal, target, saved, f"=C{r}/B{r}", f"=MAX(0,B{r}-C{r})", date,
                        f'=IF(E{r}=0,0,E{r}/MAX(1,DATEDIF($G$2,F{r},"m")))'])
    last = 4 + len(goals)
    g.rows(f"A{last + 1}", [["All goals", f"=SUM(B5:B{last})", f"=SUM(C5:C{last})",
                             f"=C{last + 1}/B{last + 1}", f"=SUM(E5:E{last})", None,
                             f"=SUM(G5:G{last})"]])
    g.number(f"B5:C{last + 1}", GBP0)
    g.number(f"E5:E{last + 1}", GBP0)
    g.number(f"G5:G{last + 1}", GBP0)
    g.number(f"D5:D{last + 1}", PCT)
    g.number(f"F5:F{last}", DATE)
    g.fmt(f"B5:G{last + 1}", h_align="Right")
    band(g, f"A5:G{last}")
    total_row(g, f"A{last + 1}:G{last + 1}", a, "E0F2EE")
    data_bar(g, f"D5:D{last}", "5FBFA9", top="1")
    label(g, "A14", "DEPOSITS THIS YEAR", a)
    g.rows("A15", [["Month", "Deposited", "Running total"]])
    header(g, "A15:C15", a)
    deposits = [("January", 640), ("February", 580), ("March", 710)]
    for i, (m, amt) in enumerate(deposits):
        r = 16 + i
        g.row(f"A{r}", [m, amt, f"=SUM($B$16:B{r})"])
    g.number("B16:C18", GBP0)
    band(g, "A16:C18")
    return finish(g, "Goals")


# ── Travel ────────────────────────────────────────────────────────────────────────────────────


def travel() -> Grid:
    a = "1B5E8C"
    g = Grid("travel")
    sheet_title(g, "Travel planner", "Lisbon and Porto · 12–19 May 2026 · two travellers", a)
    g.width("A", 12)
    g.width("B", 11)
    g.width("C", 34)
    g.width("D", 13)
    g.width("E", 11)
    g.width("F", 3)
    g.width("G", 16)
    g.width("H:I", 11)
    g.rows("A4", [["Date", "Place", "Plan", "Category", "Cost"]])
    plan = [
        ("2026-05-12", "Lisbon", "Flight from Bristol, arrive 14:20", "Transport", 248),
        ("2026-05-12", "Lisbon", "Apartment in Alfama, 4 nights", "Stay", 520),
        ("2026-05-13", "Lisbon", "Belém: monastery and tower", "Activities", 36),
        ("2026-05-13", "Lisbon", "Dinner in Bairro Alto", "Food", 64),
        ("2026-05-14", "Sintra", "Day trip by train, palace tickets", "Activities", 58),
        ("2026-05-15", "Lisbon", "Tile museum and food market", "Food", 45),
        ("2026-05-16", "Porto", "Train to Porto, first class", "Transport", 62),
        ("2026-05-16", "Porto", "Riverside hotel, 3 nights", "Stay", 435),
        ("2026-05-17", "Porto", "Port cellar tour and tasting", "Activities", 44),
        ("2026-05-18", "Douro", "Valley boat trip with lunch", "Activities", 170),
        ("2026-05-19", "Porto", "Flight home", "Transport", 0),
    ]
    for i, row in enumerate(plan):
        g.row(f"A{5 + i}", list(row))
    last = 4 + len(plan)
    table(g, f"A4:E{last}", "TableStyleMedium2")
    g.fmt("E4", h_align="Right")
    g.number(f"A5:A{last}", "ddd d mmm")
    g.number(f"E5:E{last + 1}", GBP0)
    g.rows(f"D{last + 1}", [["Total", f"=SUM(E5:E{last})"]])
    g.fmt(f"D{last + 1}", h_align="Right")
    total_row(g, f"D{last + 1}:E{last + 1}", a)
    g.rows("G4", [["Budget", "Planned", "Left"]])
    header(g, "G4:I4", a)
    g.fmt("H4:I4", h_align="Right")
    budget = [("Transport", 350), ("Stay", 1000), ("Food", 400), ("Activities", 350)]
    for i, (cat, amt) in enumerate(budget):
        r = 5 + i
        g.row(f"G{r}", [cat, f'=SUMIF($D$5:$D${last},G{r},$E$5:$E${last})', f"={amt}-H{r}"])
    g.rows("G9", [["Total", "=SUM(H5:H8)", "=SUM(I5:I8)"]])
    g.number("H5:I9", GBP0)
    band(g, "G5:I8")
    total_row(g, "G9:I9", a)
    cf(g, "I5:I9", {"CellValue": {"op": "LessThan", "values": ["0"]}}, {"font_color": "B42318", "bold": True})
    label(g, "G11", "PACKING", a)
    g.rows("G12", [["Passports and travel insurance"], ["Train tickets printed"], ["Adapter (type F)"],
                   ["Walking shoes"], ["Light rain jacket"]])
    g.fmt("G12:G16", font_color="444444")
    return finish(g, "Itinerary")
