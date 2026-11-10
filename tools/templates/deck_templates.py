# SPDX-License-Identifier: AGPL-3.0-or-later
"""The Kiln Deck templates: twelve 16:9 presentations on the bundled Deck themes."""

from kiln_script import Deck

# The layouts of each bundled theme, by name, as a fresh presentation has them after Design ▸
# Themes (`{"query":{"doc":1,"q":{"Deck":"State"}}}` lists them as `layouts`). Node ids are
# allocated in order, so they are the same every time the theme is applied to a new document.
LAYOUTS: dict[str, dict[str, int]] = {
    'Kiln': {'Title Slide': 89, 'Title and Content': 95, 'Section Header': 101, 'Two Content': 107, 'Comparison': 114, 'Title Only': 123, 'Blank': 128, 'Content with Caption': 132},
    'Tenmoku': {'Title Slide': 89, 'Title and Content': 96, 'Section Header': 102, 'Two Content': 108, 'Comparison': 115, 'Title Only': 124, 'Blank': 129, 'Content with Caption': 133},
    'Cobalt': {'Title Slide': 89, 'Title and Content': 96, 'Section Header': 102, 'Two Content': 108, 'Comparison': 115, 'Title Only': 124, 'Blank': 129, 'Content with Caption': 133},
    'Jade': {'Title Slide': 89, 'Title and Content': 96, 'Section Header': 102, 'Two Content': 109, 'Comparison': 116, 'Title Only': 125, 'Blank': 130, 'Content with Caption': 134},
    'Saffron': {'Title Slide': 89, 'Title and Content': 95, 'Section Header': 101, 'Two Content': 107, 'Comparison': 114, 'Title Only': 123, 'Blank': 128, 'Content with Caption': 132},
    'Celadon': {'Title Slide': 89, 'Title and Content': 97, 'Section Header': 103, 'Two Content': 110, 'Comparison': 117, 'Title Only': 126, 'Blank': 131, 'Content with Caption': 135},
    'Shino': {'Title Slide': 89, 'Title and Content': 96, 'Section Header': 102, 'Two Content': 108, 'Comparison': 115, 'Title Only': 124, 'Blank': 129, 'Content with Caption': 133},
    'Raku': {'Title Slide': 89, 'Title and Content': 96, 'Section Header': 102, 'Two Content': 108, 'Comparison': 115, 'Title Only': 124, 'Blank': 129, 'Content with Caption': 133},
    'Porcelain': {'Title Slide': 89, 'Title and Content': 96, 'Section Header': 102, 'Two Content': 109, 'Comparison': 116, 'Title Only': 125, 'Blank': 130, 'Content with Caption': 134},
    'Slate': {'Title Slide': 89, 'Title and Content': 96, 'Section Header': 102, 'Two Content': 109, 'Comparison': 116, 'Title Only': 125, 'Blank': 130, 'Content with Caption': 134},
    'Terracotta': {'Title Slide': 88, 'Title and Content': 96, 'Section Header': 102, 'Two Content': 108, 'Comparison': 115, 'Title Only': 124, 'Blank': 129, 'Content with Caption': 133},
    'Ash': {'Title Slide': 89, 'Title and Content': 97, 'Section Header': 103, 'Two Content': 109, 'Comparison': 116, 'Title Only': 125, 'Blank': 130, 'Content with Caption': 134},
}

# Placeholders in the order Tab reaches them on each layout (index into `slide(...)` texts):
# Title Slide: title, subtitle · Title and Content: title, body · Section Header: title, text ·
# Two Content: title, left, right · Comparison: title, left heading, left body, right heading,
# right body · Title Only: title · Content with Caption: title, content, caption.

W, H = 1280.0, 720.0  # slide size (16:9)

# Accent 1 and accent 2 of each theme (the shapes use the theme's slots; text colours need RGB).
ACCENTS: dict[str, tuple[str, str]] = {
    "Kiln": ("E05A27", "3159C6"), "Tenmoku": ("FF8A3D", "F2C35C"), "Cobalt": ("2D4FB5", "F28C28"),
    "Jade": ("1D8A60", "0F6E8C"), "Saffron": ("E8A82C", "C2481C"), "Celadon": ("5E9E95", "2F6690"),
    "Shino": ("C8553D", "E4A06F"), "Raku": ("C87533", "2FA4A4"), "Porcelain": ("2F6DB5", "7F8C9B"),
    "Slate": ("4FB3BF", "F07E5E"), "Terracotta": ("B4462A", "7D8B4A"), "Ash": ("6B7F4E", "A65A3A"),
}


class Slides(Deck):
    """A presentation on a bundled theme, filled slide by slide."""

    def __init__(self, name: str, theme: str, variant: int | None = None, body: float = 24) -> None:
        super().__init__(name)
        self.body = body
        self.layouts = LAYOUTS[theme]
        self.accent, self.accent2 = ACCENTS[theme]
        self.first = True
        self.d("ApplyTheme", {"name": theme})
        if variant is not None:
            self.d("ApplyVariant", {"index": variant})

    def deselect(self) -> "Slides":
        self.d("Escape")
        return self.d("Escape")

    def write(self, text: str) -> "Slides":
        """Types text into the shape being edited: a line per paragraph, a leading tab per level."""
        current = 0  # a new paragraph keeps the level of the one before it
        for i, line in enumerate(text.split("\n")):
            if i:
                self.d("NewParagraph")
            level = len(line) - len(line.lstrip("\t"))
            for _ in range(abs(level - current)):
                self.d("Tab", {"shift": level < current})
            current = level
            if line.strip():
                self.d("InsertText", {"text": line.lstrip("\t")})
        return self

    def fill(self, index: int, text: str, size: float | None = None) -> "Slides":
        self.deselect()
        for _ in range(index + 1):
            self.d("CycleSelection", {"backward": False})
        self.d("EditText")
        self.d("SelectAll")
        self.write(text)
        if size is not None:
            self.style_text(size=size)
        return self.deselect()

    def slide(self, layout: str, *texts: str | None, notes: str | None = None) -> "Slides":
        if self.first:
            self.first = False
            if layout != "Title Slide":
                self.d("SetLayout", {"layout": self.layouts[layout]})
        else:
            self.d("NewSlide", {"layout": self.layouts[layout]})
        for i, text in enumerate(texts):
            if text is not None:
                # Body text a size up from the theme's default, so content slides read from the
                # back of a room; titles, subtitles and section text keep the theme's sizes.
                body = i > 0 and layout in ("Title and Content", "Two Content", "Comparison",
                                            "Content with Caption")
                if layout == "Comparison" and i in (1, 3):
                    body = False
                self.fill(i, text, self.body if body else None)
        if notes:
            self.d("SetNotes", {"text": notes})
        return self

    # Objects (each is selected after it is inserted, so the format commands apply to it).
    def place(self, x: float, y: float, w: float, h: float) -> "Slides":
        """Moves and sizes the selection to a rectangle in slide pixels (Format ▸ Size takes
        points: 1280 × 720 px is 960 × 540 pt)."""
        pt = 0.75
        return self.d("SetTransform", {"x": x * pt, "y": y * pt, "width": w * pt, "height": h * pt,
                                        "rotation": None, "lock_aspect": False})

    def shape(self, preset: str, x: float, y: float, w: float, h: float, *, fill: str | None = None,
              line: str | None = None, text: str | None = None, size: float | None = None,
              color: str | None = None, bold: bool = False) -> "Slides":
        self.deselect()
        self.d("InsertShape", {"preset": preset, "rect": {"x": x, "y": y, "width": w, "height": h}})
        self.d("SetFill", {"fill": {"Solid": {"color": fill, "brightness": None, "transparency": None}}
                           if fill else "None"})
        self.d("SetOutline", {"outline": {"Color": {"color": line}} if line else "None"})
        if text is not None:
            self.d("EditText")
            self.write(text)
            self.style_text(size=size, color=color, bold=bold)
        return self.deselect()

    def textbox(self, x: float, y: float, w: float, h: float, text: str, *, size: float | None = None,
                color: str | None = None, bold: bool = False, align: str | None = None) -> "Slides":
        self.deselect()
        self.d("InsertTextBox", {"rect": {"x": x, "y": y, "width": w, "height": h}})
        self.write(text)
        self.style_text(size=size, color=color, bold=bold, align=align)
        return self.deselect()

    def style_text(self, *, size: float | None = None, color: str | None = None, bold: bool = False,
                   align: str | None = None) -> None:
        """Formats all the text of the shape being edited."""
        if size is None and color is None and not bold and align is None:
            return
        self.d("SelectAll")
        if align is not None:
            self.cmd({"Suite": {"Edit": f"Align{align}"}})
        if size is not None:
            self.cmd({"Suite": {"Edit": {"FontSize": {"size": size}}}})
        if color is not None:
            self.cmd({"Suite": {"Edit": {"FontColor": {"color": color}}}})
        if bold:
            self.cmd({"Suite": {"Edit": "Bold"}})

    def pick(self, index: int) -> "Slides":
        """Selects the slide's `index`-th placeholder (Tab order)."""
        self.deselect()
        for _ in range(index + 1):
            self.d("CycleSelection", {"backward": False})
        return self

    def table(self, rows: list[list[str]], placeholder: int = 1) -> "Slides":
        """A table filling a content placeholder (as wide as it, as tall as its rows)."""
        self.pick(placeholder)
        self.d("InsertTable", {"rows": len(rows), "cols": len(rows[0])})
        self.d("EditText")
        first = True
        for row in rows:
            for cell in row:
                if not first:
                    self.d("Tab", {"shift": False})
                first = False
                if cell:
                    self.d("InsertText", {"text": cell})
        return self.deselect()

    def stat(self, x: float, y: float, w: float, number: str, caption: str,
             color: str | None = None) -> "Slides":
        """A big figure in the accent colour with a caption under it."""
        self.textbox(x, y, w, 70, number, size=40, color=color or self.accent, bold=True)
        return self.textbox(x, y + 70, w, 60, caption, size=18)

    def chart(self, kind: str, cells: list[list[str]], x: float, y: float, w: float, h: float,
              title: str | None = None, legend: str | None = "Bottom") -> "Slides":
        self.deselect()
        self.cmd({"Suite": {"ChartInsert": {"kind": kind}}})
        self.d("SetChartData", {"cells": cells})
        self.cmd({"Suite": {"ChartSetTitle": {"text": title or ""}}})
        self.cmd({"Suite": {"ChartSetLegend": {"position": legend}}})
        self.place(x, y, w, h)
        return self.deselect()


# ── Pitch ─────────────────────────────────────────────────────────────────────────────────────


def pitch() -> Slides:
    s = Slides("pitch", "Tenmoku")
    s.slide("Title Slide", "Fieldwise", "Robotic harvesting for soft fruit · Seed round, March 2026",
            notes="Thank them for the time. One sentence: we build robots that pick strawberries "
                  "and raspberries gently enough for supermarket shelves.")
    s.slide("Title and Content", "The problem",
            "Up to 30 % of soft fruit is never picked\n"
            "Seasonal pickers are harder to find every year\n"
            "Labour is now 60 % of the cost of a punnet\n"
            "Growers are cutting the area they plant",
            notes="Lead with the grower's view: fruit rotting on the plant because nobody is there "
                  "to pick it.")
    s.slide("Two Content", "Our solution",
            "A picking robot that works through the night\n"
            "Soft grippers and vision trained on 4 million berries\n"
            "Leased per hectare: no capital cost for growers",
            "Picks 1,200 punnets a shift\n"
            "Bruising under 2 %, below hand picking\n"
            "Runs in existing polytunnels",
            notes="Keep the demo video ready in case they ask to see the gripper.")
    s.slide("Title Only", "Traction",
            notes="Revenue figures are signed lease contracts, not pilots.")
    s.chart("ColumnClustered",
            [["", "Hectares under lease"], ["2023", "12"], ["2024", "48"], ["2025", "140"], ["2026 (plan)", "420"]],
            70, 200, 700, 460, title="Hectares under lease")
    s.stat(840, 200, 380, "£1.9m", "annual recurring revenue")
    s.stat(840, 350, 380, "9 growers", "in Kent, Herefordshire and Angus")
    s.stat(840, 500, 380, "94 %", "of fruit picked to grade")
    s.slide("Title and Content", "Market",
            notes="UK first, then the Netherlands and Spain, where the same polytunnel systems are used.")
    s.table([["Market", "Soft fruit area", "Annual picking cost"],
             ["United Kingdom", "11,000 ha", "£410m"],
             ["Netherlands", "6,500 ha", "£260m"],
             ["Spain", "14,000 ha", "£520m"],
             ["Total", "31,500 ha", "£1.19bn"]])
    s.textbox(80, 470, 1120, 120,
              "Polytunnel soft fruit is our beachhead: the same tunnels, the same crops and the same "
              "labour shortage in all three countries.", size=20)
    s.slide("Section Header", "The ask", "£6m seed round to reach 1,000 hectares by 2027",
            notes="Pause here. The next slide breaks the round down.")
    s.slide("Comparison", "Use of funds", "Product · £3.1m",
            "Second-generation gripper\nRaspberry and blueberry models\nField service tools",
            "Growth · £2.9m",
            "Fleet of 120 robots\nSales team in the Netherlands\nGrower success programme",
            notes="Product spend ends once the second gripper is in production.")
    s.slide("Title Slide", "Thank you", "Maya Okafor, co-founder · maya@fieldwise.example · +44 20 7946 0811",
            notes="Leave this slide up during questions.")
    return s


def timeline(s: Slides, steps: list[tuple[str, str, str]], y: float = 360, x0: float = 160,
             x1: float = 1120) -> None:
    """A horizontal timeline: a rule, a numbered marker per step, its date above and text below."""
    n = len(steps)
    s.shape("rect", x0, y - 2, x1 - x0, 4, fill="accent2")
    gap = (x1 - x0) / (n - 1) if n > 1 else 0
    for i, (date, title, text) in enumerate(steps):
        cx = x0 + gap * i
        s.shape("ellipse", cx - 22, y - 22, 44, 44, fill="accent1", text=str(i + 1), size=16,
                color="FFFFFF", bold=True)
        s.textbox(cx - 115, y - 80, 230, 40, date, size=18, color=s.accent, bold=True, align="Center")
        s.textbox(cx - 115, y + 40, 230, 40, title, size=20, bold=True, align="Center")
        s.textbox(cx - 115, y + 84, 230, 140, text, size=16, align="Center")


def art(s: Slides, x: float, y: float, w: float, h: float, sky: tuple[str, str], hills: tuple[str, str],
        sun: str, flip: bool = False) -> None:
    """A flat landscape drawn with shapes, standing in for a photograph: a sky, a sun, two
    hills and the ground."""
    s.deselect()
    s.d("InsertShape", {"preset": "rect", "rect": {"x": x, "y": y, "width": w, "height": h}})
    s.d("SetFill", {"fill": {"Gradient": {"from": sky[0], "to": sky[1], "angle": 90}}})
    s.d("SetOutline", {"outline": "None"})
    sx = x + (w * 0.24 if flip else w * 0.68)
    s.shape("ellipse", sx, y + h * 0.16, h * 0.26, h * 0.26, fill=sun)
    a, b = (0.55, 0.05) if flip else (0.05, 0.42)
    s.shape("triangle", x + w * a, y + h * 0.38, w * 0.4, h * 0.47, fill=hills[1])
    s.shape("triangle", x + w * b, y + h * 0.3, w * 0.55, h * 0.55, fill=hills[0])
    s.shape("rect", x, y + h * 0.82, w, h * 0.18, fill=hills[0])


def quote(s: Slides, text: str, who: str, y: float = 250, size: float = 36) -> None:
    """A large quotation in the accent colour, with who said it underneath."""
    s.textbox(120, y, 1040, 220, f"“{text}”", size=size, color=s.accent)
    s.textbox(120, y + 240, 1040, 60, f"— {who}", size=20)


def product(s: Slides, x: float, y: float, w: float, h: float, body: str, grille: str) -> None:
    """A speaker drawn with shapes: a rounded body, two drivers and a strap."""
    s.shape("roundRect", x + w * 0.42, y + h * 0.04, w * 0.16, h * 0.14, fill=grille)
    s.shape("roundRect", x + w * 0.1, y + h * 0.12, w * 0.8, h * 0.8, fill=body)
    for cx in (0.32, 0.68):
        s.shape("ellipse", x + w * (cx - 0.15), y + h * 0.3, w * 0.3, w * 0.3, fill=grille)
        s.shape("ellipse", x + w * (cx - 0.06), y + h * 0.3 + w * 0.09, w * 0.12, w * 0.12, fill=body)


def cards(s: Slides, items: list[tuple[str, str]], y: float = 200, h: float = 300, x0: float = 80,
          x1: float = 1200, fill: str = "accent1") -> None:
    """A row of coloured cards, each with a heading and a line of text under it."""
    n = len(items)
    gap = 30.0
    w = (x1 - x0 - gap * (n - 1)) / n
    # The headings wrap at about 18 px a character at 24 pt; the text starts below the longest.
    per_line = max(1, int((w - 48) / 18))
    lines = max(-(-len(head) // per_line) for head, _ in items)
    body_y = 28 + lines * 34 + 18
    for i, (head, text) in enumerate(items):
        x = x0 + i * (w + gap)
        s.shape("roundRect", x, y, w, h, fill=fill)
        s.textbox(x + 24, y + 28, w - 48, lines * 34, head, size=24, color="FFFFFF", bold=True)
        s.textbox(x + 24, y + body_y, w - 48, h - body_y - 20, text, size=19, color="FFFFFF")


# ── Business review ───────────────────────────────────────────────────────────────────────────


def report() -> Slides:
    s = Slides("report", "Porcelain")
    s.slide("Title Slide", "Annual business review 2025", "Northwind Outdoor Co. · Board meeting, 4 February 2026",
            notes="Welcome the board. The review follows the agenda on the next slide.")
    s.slide("Title and Content", "Agenda",
            "The year in numbers\nWhat went well\nWhat we learned\nPriorities for 2026\nQuestions",
            notes="About forty minutes, with questions at the end.")
    s.slide("Title Only", "The year in numbers",
            notes="All figures are unaudited and in pounds sterling.")
    s.stat(90, 200, 340, "£14.2m", "revenue, up 18 % on 2024")
    s.stat(470, 200, 340, "41 %", "gross margin, up 3 points")
    s.stat(850, 200, 340, "62", "net promoter score")
    s.stat(90, 420, 340, "212,000", "orders shipped")
    s.stat(470, 420, 340, "3.1 days", "average delivery time")
    s.stat(850, 420, 340, "94", "people in the team")
    s.slide("Title Only", "Revenue by quarter",
            notes="Q4 includes the winter range, which sold out in early December.")
    s.chart("ColumnClustered", [["", "FY 2024", "FY 2025"], ["Q1", "2.6", "2.9"], ["Q2", "3.0", "3.4"],
                                ["Q3", "2.8", "3.5"], ["Q4", "3.6", "4.4"]],
            80, 190, 1120, 470, title="Revenue (£m)")
    s.slide("Two Content", "What went well",
            "Winter range sold out six weeks early\nRepair service launched in March\nReturns down from 11 % to 7 %",
            "New warehouse in Doncaster on time\nWebsite conversion up to 3.4 %\nStaff turnover at a five-year low",
            notes="Credit the warehouse team: the move happened without a day of lost orders.")
    s.slide("Title and Content", "What we learned",
            "Stock-outs cost more than overstock\n\tWe lost an estimated £0.6m of winter sales\n"
            "Customers value repairs\n\t38 % of repair customers bought again within six months\n"
            "Wholesale grows slowly but steadily",
            notes="These lessons shape the priorities on the next slide.")
    s.slide("Title and Content", "Priorities for 2026",
            "Plan winter stock earlier, with two reorder points\nTake repairs to every product line\n"
            "Open a second wholesale channel in Europe\nReach £17m revenue at a 42 % margin",
            notes="Each priority has an owner on the leadership team; details in the board pack.")
    s.slide("Section Header", "Questions", "Thank you",
            notes="Leave time for questions; the finance director will answer anything on the numbers.")
    return s


# ── Lesson ────────────────────────────────────────────────────────────────────────────────────


def lesson() -> Slides:
    s = Slides("lesson", "Saffron")
    s.slide("Title Slide", "The water cycle", "Year 7 Science · Lesson 4 of 6",
            notes="Settle the class; the starter question is on the next slide.")
    s.slide("Title and Content", "Learning objectives",
            "Name the four stages of the water cycle\nExplain how energy from the Sun drives it\n"
            "Describe one way people change the cycle",
            notes="Read the objectives aloud and leave them on the board during the starter.")
    s.slide("Section Header", "Starter", "Where does the water in a puddle go on a sunny day?",
            notes="Two minutes in pairs, then take three answers.")
    s.slide("Title Only", "Four stages",
            notes="Point to each stage as you name it; pupils copy the diagram into their books.")
    cards(s, [("Evaporation", "The Sun warms seas and lakes; water turns into vapour and rises."),
              ("Condensation", "The vapour cools high up and forms tiny droplets: clouds."),
              ("Precipitation", "Droplets join until they are heavy enough to fall as rain, snow or hail."),
              ("Collection", "Water gathers in rivers, lakes, the sea and underground, and starts again.")],
          y=200, h=330)
    s.slide("Two Content", "Key words",
            "Evaporation\nCondensation\nPrecipitation\nRun-off",
            "Water vapour\nTranspiration\nGroundwater\nReservoir",
            notes="Pupils write a definition for any two words from each column.")
    s.slide("Title and Content", "Activity: water in a bag",
            "Pour a little water into a zip bag and draw the cycle on it\n"
            "Tape the bag to a sunny window\n"
            "Check it at the end of the lesson and again tomorrow\n"
            "Record what you see, with the time",
            notes="Bags and markers are in the blue tray. Allow ten minutes.")
    s.slide("Title and Content", "Check your understanding",
            "Which stage forms clouds?\nWhere does the energy for evaporation come from?\n"
            "Give one way that cutting down forests changes the cycle",
            notes="Mini whiteboards: everyone answers, then reveal.")
    s.slide("Section Header", "Homework", "Find out where your tap water comes from, and draw its journey",
            notes="Due next Thursday. Remind them to bring the bag results to the next lesson.")
    return s


# ── Portfolio ─────────────────────────────────────────────────────────────────────────────────


def project(s: Slides, title: str, points: str, facts: str, palette: dict, notes: str,
            flip: bool = False) -> None:
    """A project slide: an illustration beside what was done and the facts."""
    s.slide("Title Only", title, notes=notes)
    art(s, 80, 190, 560, 420, palette["sky"], palette["hills"], palette["sun"], flip=flip)
    s.textbox(690, 190, 510, 280, "\n\n".join("•  " + p for p in points.split("\n")), size=22)
    s.shape("rect", 690, 492, 60, 4, fill="accent1")
    s.textbox(690, 512, 510, 110, facts, size=16, color=s.accent, bold=True)


def portfolio() -> Slides:
    s = Slides("portfolio", "Ash")
    green = {"sky": ("C9D3BC", "F5F4EF"), "hills": ("6B7F4E", "8C9A72"), "sun": "C2A15A"}
    dusk = {"sky": ("E7C9A9", "F5F4EF"), "hills": ("5C6650", "A65A3A"), "sun": "F5F4EF"}
    s.slide("Title Slide", "Selected work 2022–2026", "Noor Haddad · Landscape architect",
            notes="Introduce yourself in one sentence and say how long you will talk.")
    s.slide("Content with Caption", "About me",
            "Public parks and gardens\nSchool grounds\nStreets and squares\nRain gardens and drainage",
            "I design places that work for people and wildlife. Before starting my own practice I "
            "spent eight years leading parks projects across the north of England. Based in York; "
            "chartered member of the Landscape Institute.",
            notes="Keep this short; the work speaks for itself.")
    project(s, "Riverside Gardens, Selby",
            "A 2.4 ha park on a former car park\nRain gardens take water from 600 m of street\n"
            "Play space designed with local children",
            "Selby Town Council · 2024 · £1.8m", green,
            notes="The rain gardens took 40 % of the street's run-off in their first winter.")
    project(s, "Hollin Bank Primary School",
            "Outdoor classroom and forest school\nA quiet garden for pupils who need calm\n"
            "Food growing for every class",
            "Hollin Bank Primary · 2023 · £240,000", dusk, flip=True,
            notes="Mention the pupils' design workshops.")
    project(s, "Market Street, Thirsk",
            "Wider pavements and twelve street trees\nSeating at every bus stop\n"
            "Slower traffic without new signs",
            "North Yorkshire Highways · 2025 · £920,000", green,
            notes="Footfall on the street rose by a fifth in the first year.")
    s.slide("Title Only", "How I work",
            notes="This is the process clients most often ask about.")
    cards(s, [("Listen", "Walk the site with the people who use it, at different times of day."),
              ("Sketch", "Test ideas quickly on paper and on site before anything is drawn up."),
              ("Build", "Stay involved through construction, with regular site visits."),
              ("Return", "Visit a year later to see what works, and share the lessons.")],
          y=200, h=330)
    s.slide("Title Slide", "Let's talk", "noor@haddadlandscape.example · haddadlandscape.example",
            notes="Offer to send the full portfolio after the meeting.")
    return s


# ── Roadmap timeline ──────────────────────────────────────────────────────────────────────────


def timeline_deck() -> Slides:
    s = Slides("timeline", "Slate")
    s.slide("Title Slide", "Product roadmap 2026", "Ledgerly mobile app · Updated January 2026",
            notes="This roadmap is a plan, not a promise: dates may move as we learn.")
    s.slide("Title Only", "The year at a glance",
            notes="Walk left to right; each milestone has its own slide later.")
    timeline(s, [("January", "Offline mode", "Work without a signal; changes sync when back online."),
                 ("March", "Shared budgets", "Plan and track spending together."),
                 ("June", "Receipts", "Photograph a receipt to record it."),
                 ("September", "Open banking", "Import transactions from 30 banks."),
                 ("November", "Year in review", "A personal summary of the year.")])
    s.slide("Two Content", "Now: offline mode",
            "Every screen works without a connection\nChanges queue and sync in the background\n"
            "Conflicts are shown clearly, never lost",
            "Status: in beta with 2,000 users\nRelease: 27 January\nOwner: Platform team",
            notes="Beta feedback has been mostly about sync speed on older phones.")
    s.slide("Two Content", "Next: shared budgets",
            "Invite a partner or housemate to a budget\nSplit bills and see who owes what\n"
            "Private categories stay private",
            "Status: design complete\nRelease: March\nOwner: Money team",
            notes="Research showed couples are our fastest-growing group.")
    s.slide("Title and Content", "Later in the year",
            "Receipt scanning on the device, without sending photos anywhere\n"
            "Open banking import from 30 UK banks\nA year in review in November",
            notes="These depend on the outcome of the banking partner tender in April.")
    s.slide("Title and Content", "Delivery by quarter",
            notes="Team sizes are full-time equivalents.")
    s.table([["Quarter", "Main release", "Team", "Confidence"],
             ["Q1", "Offline mode, shared budgets", "14", "High"],
             ["Q2", "Receipt scanning", "12", "High"],
             ["Q3", "Open banking", "16", "Medium"],
             ["Q4", "Year in review", "9", "Medium"]])
    s.slide("Section Header", "Questions and ideas", "roadmap@ledgerly.example",
            notes="Invite people to send ideas to the roadmap inbox.")
    return s


# ── Quarterly review ──────────────────────────────────────────────────────────────────────────


def quarterly() -> Slides:
    s = Slides("quarterly", "Cobalt")
    s.slide("Title Slide", "Q3 2026 business review", "Customer Success · 14 October 2026",
            notes="Thirty minutes: results, highlights, risks and the plan for Q4.")
    s.slide("Title Only", "Q3 at a glance",
            notes="Targets were set in the July planning session.")
    s.stat(90, 200, 340, "96.2 %", "customer retention (target 95 %)")
    s.stat(470, 200, 340, "£2.4m", "expansion revenue, 112 % of plan")
    s.stat(850, 200, 340, "4.6 / 5", "support satisfaction")
    s.stat(90, 420, 340, "2 h 10 m", "median first response")
    s.stat(470, 420, 340, "38", "new enterprise accounts")
    s.stat(850, 420, 340, "3", "accounts at risk")
    s.slide("Title Only", "Expansion revenue by month",
            notes="August dipped after the price change and recovered in September.")
    s.chart("ColumnClustered", [["", "Actual (£k)", "Plan (£k)"], ["Apr", "352", "340"], ["May", "381", "360"],
                                ["Jun", "418", "380"], ["Jul", "436", "400"], ["Aug", "358", "400"],
                                ["Sep", "455", "420"]],
            80, 190, 1120, 470, title="Expansion revenue")
    s.slide("Comparison", "Highlights and risks", "What went well",
            "Onboarding time cut from 21 to 12 days\nNew health score live for every account\n"
            "Two case studies published",
            "What needs attention",
            "Three large accounts reviewing contracts\nTicket backlog up 18 % after the release\n"
            "One open senior hire",
            notes="For each risk, name the owner and the next step.")
    s.slide("Title and Content", "Team scorecard",
            notes="Green means on or above target.")
    s.table([["Team", "Accounts", "Retention", "Satisfaction", "Status"],
             ["Enterprise", "84", "97.8 %", "4.7", "On track"],
             ["Mid-market", "312", "96.1 %", "4.6", "On track"],
             ["Small business", "1,940", "93.4 %", "4.4", "Watch"],
             ["Public sector", "57", "98.2 %", "4.8", "On track"]])
    s.slide("Title and Content", "Plan for Q4",
            "Save the three at-risk accounts with executive sponsors\n"
            "Clear the backlog to under 200 tickets by November\n"
            "Launch the customer community in December\n"
            "Hire a senior success manager for the public sector",
            notes="Ask for the board's support on executive sponsors.")
    s.slide("Section Header", "Discussion", "Questions, comments and decisions needed",
            notes="Capture decisions in the notes for the minutes.")
    return s


# ── Project kickoff ───────────────────────────────────────────────────────────────────────────


def kickoff() -> Slides:
    s = Slides("kickoff", "Jade")
    s.slide("Title Slide", "Project Larch: kickoff", "New booking system for Greenhaven Leisure Centres · 2 March 2026",
            notes="Welcome everyone and go round the room for names and roles.")
    s.slide("Title and Content", "Why we are here",
            "Bookings still run on a system from 2009\nMembers cannot book or cancel on their phones\n"
            "Staff spend 14 hours a week fixing double bookings\nThe current contract ends in December",
            notes="Keep this grounded in what members and staff experience today.")
    s.slide("Two Content", "Goals and scope",
            "Online booking for every class and court\nMembership payments and renewals\n"
            "A mobile app for members",
            "Out of scope: café tills and the car park\nOut of scope: changes to prices\n"
            "Out of scope: the spa booking system",
            notes="Scope changes go through the steering group.")
    s.slide("Title Only", "Team and roles",
            notes="Everyone gets the contact list after the meeting.")
    cards(s, [("Sponsor", "Hannah Mbeki, Director of Operations"),
              ("Project lead", "Callum Reid, Digital Services"),
              ("Product owner", "Zara Whitfield, Membership"),
              ("Delivery", "Brightline Software, eight engineers")], y=200, h=260)
    s.slide("Title Only", "Milestones",
            notes="The pilot centre is Westgate, the busiest of the four.")
    timeline(s, [("March", "Discovery", "Interviews with members and staff."),
                 ("May", "Design", "Booking journey tested with members."),
                 ("July", "Build", "Core booking and payments."),
                 ("September", "Pilot", "Westgate centre goes live."),
                 ("November", "Roll-out", "All four centres and the app.")])
    s.slide("Title and Content", "Ways of working",
            "Fortnightly sprints with a demo every other Friday\nOne shared board for all work\n"
            "Steering group every month, chaired by the sponsor\nRisks reviewed every week",
            notes="Ask the team which day suits the demo best.")
    s.slide("Title and Content", "Next two weeks",
            "Book discovery interviews (Zara)\nSet up the shared board and channels (Callum)\n"
            "Review the current booking data (Brightline)\nFirst steering group on 16 March (Hannah)",
            notes="Confirm owners before closing.")
    return s


# ── Workshop ──────────────────────────────────────────────────────────────────────────────────


def workshop() -> Slides:
    s = Slides("workshop", "Celadon")
    s.slide("Title Slide", "Writing for the web", "Half-day workshop · Communications team",
            notes="Check the room: everyone needs a laptop and a page they look after.")
    s.slide("Title and Content", "Today's plan",
            "09:30  How people read online\n10:15  Plain words and short sentences\n"
            "11:00  Break\n11:15  Headings, links and lists\n12:00  Rewrite your own page\n12:45  Share and close",
            notes="Keep to time; the rewrite exercise needs the full forty-five minutes.")
    s.slide("Section Header", "1. How people read online", "Scan first, read later",
            notes="Show the eye-tracking heat map from the handout.")
    s.slide("Two Content", "What readers do",
            "Scan headings and the first words of lines\nLeave when they cannot find what they need\n"
            "Read on phones, often in a hurry",
            "Put the answer first\nUse headings that say what follows\nKeep paragraphs to two or three sentences",
            notes="Ask for examples of pages people have given up on.")
    s.slide("Title and Content", "Plain words",
            "Use “buy”, not “purchase”\nUse “help”, not “facilitate”\nUse “about”, not “approximately”\n"
            "Write “you” and “we”",
            notes="Hand out the plain words list.")
    s.slide("Title Only", "Exercise: rewrite your page",
            notes="Walk round the room while people work.")
    cards(s, [("Find", "Pick a page you look after with at least 300 words."),
              ("Cut", "Remove anything the reader does not need to act."),
              ("Rewrite", "Answer first, plain words, clear headings."),
              ("Swap", "Ask a partner to find one thing on your page in ten seconds.")], y=200, h=300,
          fill="accent2")
    s.slide("Section Header", "Thank you", "Slides and the plain words list are on the team site",
            notes="Collect feedback forms on the way out.")
    return s


# ── Product launch ────────────────────────────────────────────────────────────────────────────


def launch() -> Slides:
    s = Slides("launch", "Raku")
    s.slide("Title Slide", "Introducing Tideline", "The weatherproof speaker for every adventure · Launch briefing",
            notes="This briefing is for the sales and retail teams ahead of launch on 1 May.")
    s.slide("Title Only", "Meet Tideline", notes="Pass the sample units round now.")
    s.shape("roundRect", 80, 190, 560, 420, fill="2A2826")
    product(s, 150, 230, 420, 330, "C87533", "1E1C1A")
    s.textbox(690, 190, 510, 300,
              "•  20 hours of playback\n\n•  Waterproof to one metre\n\n•  Floats, so it never sinks\n\n"
              "•  Pairs with a second speaker for stereo", size=22)
    s.slide("Title Only", "Who it is for",
            notes="Sales should lead with the persona that fits each store.")
    cards(s, [("Campers", "Long battery life and a strap for the tent pole."),
              ("Paddlers", "Floats and survives a capsize."),
              ("Families", "Tough enough for the garden and the beach.")], y=200, h=280)
    s.slide("Title Only", "Launch plan",
            notes="Retail displays ship on 20 April.")
    timeline(s, [("14 April", "Press preview", "Samples to twenty reviewers."),
                 ("24 April", "Pre-orders", "Online store and partners."),
                 ("1 May", "Launch", "In 400 shops and online."),
                 ("June", "Summer push", "Festival partnerships.")])
    s.slide("Title and Content", "Price and availability",
            notes="Prices include VAT. Bundle stock is limited to 5,000 units.")
    s.table([["Edition", "Colours", "Price", "Available"],
             ["Tideline", "Slate, Coral, Kelp", "£89", "1 May"],
             ["Tideline Pair", "Slate", "£159", "1 May"],
             ["Tideline Camp bundle", "Kelp", "£119", "June"]])
    s.slide("Section Header", "Questions", "launch@tideline.example",
            notes="Point the team to the sales guide on the intranet.")
    return s


# ── All-hands ─────────────────────────────────────────────────────────────────────────────────


def all_hands() -> Slides:
    s = Slides("all-hands", "Kiln")
    s.slide("Title Slide", "All-hands", "Thursday 5 March 2026 · Main hall and online",
            notes="Start on time; the recording goes out this afternoon.")
    s.slide("Title and Content", "Agenda",
            "Welcome and new starters\nCompany update\nTeam spotlight: Customer Support\n"
            "Celebrations\nQuestions from the room",
            notes="Forty-five minutes in total.")
    s.slide("Title Only", "Welcome to the team",
            notes="Ask each new starter to wave.")
    cards(s, [("Amira Saleh", "Product designer, Payments"), ("Tom Fairclough", "Engineer, Platform"),
              ("Grace Mwangi", "Support specialist"), ("Leo Brandt", "Finance analyst")], y=210, h=220)
    s.slide("Title Only", "Company update",
            notes="Numbers are for the quarter to date.")
    s.stat(90, 210, 340, "£3.8m", "revenue this quarter, 104 % of plan")
    s.stat(470, 210, 340, "11,400", "active customers, up 9 %")
    s.stat(850, 210, 340, "4.7", "customer rating in the app stores")
    s.slide("Section Header", "Team spotlight", "Customer Support: from 9 hours to 40 minutes",
            notes="Hand over to Priya for five minutes.")
    s.slide("Two Content", "Celebrations",
            "Ten years: Margaret Holt, Finance\nFive years: Dev Patel, Engineering\nNew parents: Sophie and Ben",
            "Best-run project: the office move\nCharity bake sale raised £1,240\nThe five-a-side team won the league",
            notes="Invite applause after each column.")
    s.slide("Section Header", "Questions", "Ask in the room or in the all-hands channel",
            notes="Leadership answers anything left over in writing by Monday.")
    return s


# ── Conference talk ───────────────────────────────────────────────────────────────────────────


def talk() -> Slides:
    s = Slides("talk", "Shino")
    s.slide("Title Slide", "Small steps, quieter cities", "How six neighbourhoods cut traffic noise by a third · Lena Achterberg",
            notes="Twenty-minute slot, five minutes for questions.")
    s.slide("Section Header", "Noise is a health problem", "One in five people in our cities lives with harmful traffic noise",
            notes="Pause after the statistic.")
    s.slide("Title Only", "What we changed",
            notes="None of these needed new roads or large budgets.")
    cards(s, [("Slower streets", "20 mph limits with narrower lanes at junctions."),
              ("Quieter surfaces", "Low-noise asphalt when roads were resurfaced anyway."),
              ("Planting", "Hedges and street trees where pavements allowed."),
              ("Deliveries", "Night deliveries moved to electric vans.")], y=200, h=300)
    s.slide("Title Only", "Results after two years",
            notes="Measured with forty fixed monitors, before and after.")
    s.chart("BarClustered", [["", "Before (dB)", "After (dB)"], ["Ashgrove", "68", "61"],
                             ["Millbank", "66", "60"], ["St Anne's", "71", "63"], ["Hartfield", "64", "59"],
                             ["Quayside", "69", "62"], ["Elmwood", "65", "60"]],
            80, 190, 1120, 470, title="Average daytime noise")
    s.slide("Title Only", "What residents told us", notes="Read one quote aloud.")
    cards(s, [("“I open the windows again in summer.”", "Resident, Ashgrove"),
              ("“The children walk to school now.”", "Parent, Millbank"),
              ("“It feels like a village street.”", "Shop owner, Quayside")], y=200, h=260)
    s.textbox(80, 500, 1120, 80, "Surveys of 1,200 residents before and after the changes: satisfaction "
              "with their street rose from 41 % to 73 %.", size=20)
    s.slide("Title and Content", "Lessons for other cities",
            "Start where roads are being resurfaced anyway\nMeasure before you change anything\n"
            "Let residents choose where the planting goes\nShare results street by street",
            notes="This is the slide people photograph: leave it up a little longer.")
    s.slide("Title Slide", "Thank you", "lena.achterberg@quietercities.example · Slides and data: quietercities.example",
            notes="Take questions.")
    return s


# ── Case study ────────────────────────────────────────────────────────────────────────────────


def case_study() -> Slides:
    s = Slides("case-study", "Terracotta")
    warm = {"sky": ("EED9C8", "F8EEE5"), "hills": ("B4462A", "C98A2B"), "sun": "F8EEE5"}
    s.slide("Title Slide", "Case study: Hearth Bakery", "From one shop to six without losing the bread · Ferro Consulting",
            notes="A twelve-month engagement, finished in January 2026.")
    s.slide("Title Only", "The client", notes="Hearth is family-owned and bakes everything on site.")
    art(s, 80, 190, 560, 420, warm["sky"], warm["hills"], warm["sun"], flip=True)
    s.textbox(690, 190, 510, 340,
              "•  Independent bakery in Shrewsbury, founded in 1998\n\n"
              "•  Two shops and a wholesale round\n\n"
              "•  Wanted to grow without lowering quality", size=22)
    s.slide("Two Content", "The challenge",
            "Every loaf was made in one small kitchen\nNo stock system: orders lived in a notebook\n"
            "Staff worked long, unpredictable shifts",
            "Waste was 14 % of what was baked\nWholesale customers wanted earlier deliveries\n"
            "Two new sites were already leased",
            notes="The leases set the deadline for the whole project.")
    s.slide("Title Only", "What we did",
            notes="Each phase took about three months.")
    timeline(s, [("Phase 1", "Central bakery", "A new production kitchen with night shifts."),
                 ("Phase 2", "Ordering", "Simple stock and order system for every shop."),
                 ("Phase 3", "People", "Rotas planned four weeks ahead."),
                 ("Phase 4", "Opening", "Four new shops in nine months.")])
    s.slide("Title Only", "Results",
            notes="All figures compare the year before and the year after.")
    s.stat(90, 210, 340, "6 shops", "up from two, all profitable")
    s.stat(470, 210, 340, "4 %", "waste, down from 14 %")
    s.stat(850, 210, 340, "+138 %", "revenue in twelve months")
    s.chart("ColumnClustered", [["", "Revenue (£k)"], ["Q1", "310"], ["Q2", "420"], ["Q3", "560"], ["Q4", "740"]],
            470, 380, 740, 290, title="", legend=None)
    s.slide("Title Only", "In their words", notes="Ruth is happy to take reference calls.")
    quote(s, "We doubled our shops and our bread is better than ever. The night bakery changed everything.",
          "Ruth Penhaligon, owner, Hearth Bakery")
    s.slide("Title Slide", "Could this work for you?", "hello@ferroconsulting.example · 01743 555 0182",
            notes="Offer a free first conversation.")
    return s
