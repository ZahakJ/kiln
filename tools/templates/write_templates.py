# SPDX-License-Identifier: AGPL-3.0-or-later
"""The Kiln Write templates: twelve A4 documents in the bundled document fonts (Caladea, Carlito,
Liberation Serif and Sans), each with its own type scale and accent colour."""

from kiln_script import A4_W, Write

MARGIN = 72
TEXT_W = A4_W - 2 * MARGIN  # text width with 2.54 cm margins


def house(d: Write, *, accent: str, head: str = "Caladea", body: str = "Carlito", ink: str = "262626",
          size: float = 11, line: float = 1.15, h1: float = 16, title: float = 30) -> None:
    """A consistent type scale: Normal, Title, Subtitle and Heading 1–3."""
    d.style("Normal", f={"family": body, "size": size, "color": ink},
            p={"space_after": 6, "line_multiple": line})
    d.style("Title", f={"family": head, "size": title, "color": accent, "bold": False},
            p={"space_after": 4, "line_multiple": 1.0})
    d.style("Subtitle", f={"family": body, "size": 12.5, "color": "6B6B6B", "italic": False},
            p={"space_after": 18})
    d.style("Heading1", f={"family": head, "size": h1, "color": accent, "bold": False},
            p={"space_before": 18, "space_after": 6, "keep_next": True})
    d.style("Heading2", f={"family": body, "size": size + 1.5, "color": ink, "bold": True},
            p={"space_before": 12, "space_after": 4, "keep_next": True})
    d.style("Heading3", f={"family": body, "size": size, "color": accent, "bold": True},
            p={"space_before": 10, "space_after": 2, "keep_next": True})


def finish(d: Write, **props: str) -> Write:
    d.w("SpellCheck", {"on": False})
    d.props(**props)
    d.start()
    return d


def table(d: Write, first_cell: str, *, fill: str, color: str = "FFFFFF", style: str = "PlainTable1",
          widths: dict[str, float] | None = None, right: tuple[str, ...] = (), band: str | None = None,
          rows_after: tuple[str, ...] = (), first_col: tuple[str, ...] = ()) -> None:
    """Styles the table whose header row holds `first_cell`: a table style, a coloured header,
    column widths and right-aligned columns (keyed by their header text)."""
    d.find(first_cell)
    d.table_style(style)
    d.find(first_cell)
    d.table_select("Row")
    d.e("TableShading", {"color": fill})
    d.f(color=color, bold=True)
    for header, width in (widths or {}).items():
        d.find(header)
        d.w("TableCellSize", {"width": width, "height": None})
    for header in right:
        d.find(header)
        d.table_select("Column")
        d.e("AlignRight")
    # Table Layout ▸ Select ▸ Column selects the text from the column's first cell to its last, so
    # the cells of earlier columns in between are aligned too: put the first column back.
    if right:
        for text in (first_cell, *first_col):
            d.find(text)
            d.e("AlignLeft")
    for text in rows_after:
        d.find(text)
        d.table_select("Row")
        if band:
            d.e("TableShading", {"color": band})
        d.f(bold=True)


def rule_under(d: Write, text: str, space_after: float | None = None) -> None:
    d.sel_para(text)
    d.borders("Bottom")
    if space_after is not None:
        d.p(space_after=space_after)


# ── Letters ───────────────────────────────────────────────────────────────────────────────────


def letter() -> Write:
    d = Write("letter")
    d.a4(top=60)
    house(d, accent="1F3A5F")
    d.md(
        "Harbour & Finch Architects\n\n"
        "14 Quay Street · Bristol BS1 4DJ · +44 117 496 0321 · studio@harbourfinch.example\n\n"
        "12 March 2026\n\n"
        "Ms Eleanor Whitcombe\n\nDirector of Estates\n\nRiverside Arts Trust\n\n"
        "2 Canal Walk\n\nBath BA2 3QZ\n\n"
        "Dear Ms Whitcombe,\n\n"
        "**Proposal for the Riverside Gallery extension**\n\n"
        "Thank you for meeting us at the gallery last week and for showing us the old boat store. "
        "We left with a clear sense of what the Trust needs: a light, flexible exhibition space that "
        "opens onto the towpath without losing the character of the existing building.\n\n"
        "Enclosed is our proposal. It sets out a phased approach that keeps the gallery open "
        "throughout construction, a timber-frame structure that meets your carbon target, and a "
        "fixed fee for the design stages through to planning.\n\n"
        "We would welcome the chance to present the scheme to the trustees on 2 April. In the "
        "meantime, please call me directly on the number above if anything needs clarifying.\n\n"
        "Yours sincerely,\n\n"
        "Daniel Okafor\n\nPartner, Harbour & Finch Architects\n\n"
        "Enclosure: Riverside Gallery extension — design proposal (12 pages)\n"
    )
    d.sel_para("Harbour & Finch Architects")
    d.f(family="Caladea", size=26, color="1F3A5F")
    d.p(space_after=2, line_multiple=1.0)
    d.sel_para("14 Quay Street")
    d.f(size=9, color="6B7280", spacing=0.3)
    d.p(space_after=36)
    d.borders("Bottom")
    d.sel_para("12 March 2026")
    d.p(space_after=18)
    d.sel_para("Ms Eleanor Whitcombe", 5)
    d.p(space_after=0)
    d.sel_para("Bath BA2")
    d.p(space_after=24)
    d.sel_para("Proposal for the Riverside")
    d.f(color="1F3A5F", size=12)
    d.sel_para("Daniel Okafor")
    d.p(space_before=30, space_after=0)
    d.find("Daniel Okafor")
    d.f(bold=True)
    d.sel_to_end("Enclosure:")
    d.f(size=9, color="6B7280", italic=True)
    d.p(space_before=24)
    return finish(d, title="Business letter", author="Daniel Okafor",
                  company="Harbour & Finch Architects")


def cover_letter() -> Write:
    d = Write("cover-letter")
    d.a4(top=64)
    house(d, accent="7A3E2E", head="Caladea")
    d.md(
        "Amara Lindqvist\n\n"
        "Senior Product Designer\n\n"
        "Leeds · +44 7700 900 418 · amara.lindqvist@example.com · amaralindqvist.example\n\n"
        "4 May 2026\n\n"
        "Hiring Team\n\nNorthfield Health\n\n21 Wellington Place\n\nLeeds LS1 4AP\n\n"
        "Dear Hiring Team,\n\n"
        "I am writing to apply for the Lead Product Designer role on your patient app. For the past "
        "six years I have designed health and booking products used by more than two million people, "
        "and the problem you describe — making repeat prescriptions effortless for people managing "
        "long-term conditions — is exactly the kind of work I want to lead.\n\n"
        "At Brightwell Clinics I led the redesign of appointment booking across web and mobile. By "
        "testing every step with patients over seventy, we cut the average booking time from four "
        "minutes to under ninety seconds and halved calls to the help line. I also built the design "
        "system that four product teams now share.\n\n"
        "What I would bring to Northfield:\n\n"
        "- Research habits that put real patients, carers and pharmacists in the room early\n"
        "- Accessible, tested design: every release met WCAG 2.2 AA\n"
        "- Calm leadership of designers, researchers and engineers across time zones\n\n"
        "I would be glad to walk you through two recent case studies. Thank you for your time and "
        "consideration.\n\n"
        "Kind regards,\n\n"
        "Amara Lindqvist\n"
    )
    d.sel_para("Amara Lindqvist")
    d.f(family="Caladea", size=28, color="7A3E2E")
    d.p(space_after=0, line_multiple=1.0)
    d.sel_para("Senior Product Designer")
    d.f(size=12, color="444444", small_caps=True, spacing=1.0)
    d.p(space_after=4)
    d.sel_para("Leeds · +44")
    d.f(size=9, color="6B6B6B")
    d.p(space_after=30)
    d.borders("Bottom")
    d.sel_para("4 May 2026")
    d.p(space_after=18)
    d.sel_para("Hiring Team", 4)
    d.p(space_after=0)
    d.sel_para("Leeds LS1 4AP")
    d.p(space_after=24)
    d.find("What I would bring to Northfield:")
    d.f(bold=True, color="7A3E2E")
    d.w("MoveCaret", {"motion": "DocumentEnd", "extend": False})
    d.w("MoveCaret", {"motion": "LineStart", "extend": True})
    d.f(family="Caladea", size=16, color="7A3E2E")
    d.p(space_before=24)
    return finish(d, title="Cover letter", author="Amara Lindqvist")


def resume() -> Write:
    d = Write("resume")
    d.a4(top=54, bottom=54, left=62, right=62)
    house(d, accent="1E5B5B", head="Caladea", size=10.5, h1=13)
    d.md(
        "Amara Lindqvist\n\n"
        "Senior Product Designer\n\n"
        "Leeds, UK · +44 7700 900 418 · amara.lindqvist@example.com · amaralindqvist.example\n\n"
        "# Profile\n\n"
        "Product designer with eight years in health and booking products. I turn messy services into "
        "calm, accessible journeys, back decisions with research, and build the systems teams need to "
        "ship consistently.\n\n"
        "# Experience\n\n"
        "## Senior Product Designer, Brightwell Clinics\n\n"
        "Leeds · 2022 – present\n\n"
        "- Led the redesign of appointment booking across web and mobile apps (1.4 million users)\n"
        "- Cut median booking time from 4 minutes to 85 seconds; help-line calls fell by 48 %\n"
        "- Built and maintain the design system shared by four product teams\n\n"
        "## Product Designer, Tessellate Travel\n\n"
        "Manchester · 2019 – 2022\n\n"
        "- Designed the group booking flow that now accounts for 22 % of revenue\n"
        "- Introduced fortnightly usability testing and a research repository\n\n"
        "## UX Designer, Copperline Agency\n\n"
        "Manchester · 2017 – 2019\n\n"
        "- Delivered service design and prototypes for twelve public and charity clients\n\n"
        "# Education\n\n"
        "## MA Interaction Design, University of Northbridge\n\n"
        "2016 – 2017 · Distinction\n\n"
        "## BA Graphic Communication, Pennine College of Art\n\n"
        "2013 – 2016 · First-class honours\n\n"
        "# Selected projects\n\n"
        "**Booking for everyone** — a booking journey tested with 60 patients aged 70 and over; "
        "shortlisted for the Northern Design Awards 2024.\n\n"
        "**Quiet mode** — a low-stimulation version of the clinic app for neurodivergent patients, "
        "now used by 38,000 people a month.\n\n"
        "# Skills\n\n"
        "**Design** — interaction and visual design, prototyping, design systems, accessibility (WCAG 2.2)\n\n"
        "**Research** — interviews, usability testing, journey mapping, service blueprints\n\n"
        "**Tools** — prototyping and design-system tools, HTML and CSS, research repositories\n\n"
        "**Languages** — English (native), Swedish (fluent), French (conversational)\n"
    )
    d.sel_para("Amara Lindqvist")
    d.f(family="Caladea", size=30, color="1E5B5B")
    d.p(space_after=0, line_multiple=1.0)
    d.sel_para("Senior Product Designer")
    d.f(size=12, color="333333", small_caps=True, spacing=1.2)
    d.p(space_after=2)
    d.sel_para("Leeds, UK")
    d.f(size=9, color="6B6B6B")
    d.p(space_after=6)
    for h in ("Profile", "Experience", "Education", "Selected projects", "Skills"):
        d.find(h)
        d.f(small_caps=True, spacing=1.5)
        d.sel_para(h)
        d.borders("Bottom")
        d.p(space_before=14, space_after=6)
    for dates in ("Leeds · 2022", "Manchester · 2019", "Manchester · 2017", "2016 – 2017", "2013 – 2016"):
        d.sel_para(dates)
        d.f(size=9.5, color="1E5B5B", italic=True)
        d.p(space_after=3)
    for h2 in ("Senior Product Designer, Brightwell", "Product Designer, Tessellate", "UX Designer, Copperline",
               "MA Interaction Design", "BA Graphic Communication"):
        d.sel_para(h2)
        d.p(space_before=8, space_after=0)
        d.f(size=11)
    return finish(d, title="Résumé", author="Amara Lindqvist")


def memo() -> Write:
    d = Write("memo")
    d.a4(top=64)
    house(d, accent="2E4057", head="Liberation Sans", body="Liberation Sans", size=10.5, h1=13)
    d.md(
        "Memorandum\n\n"
        "To:\tAll studio staff\n\n"
        "From:\tPriya Raman, Operations Director\n\n"
        "Date:\t9 February 2026\n\n"
        "Subject:\tMove to the Canal Street office\n\n"
        "Our move to the new office at 48 Canal Street is confirmed for the weekend of 7–8 March. "
        "This memo sets out what will happen and what we need from each team.\n\n"
        "# Before the move\n\n"
        "- Pack your desk into the crates provided by 5 pm on Friday 6 March and label each crate "
        "with your name and team\n"
        "- Back up anything stored only on your laptop; IT will reimage machines on arrival\n"
        "- Return library books and shared equipment to the front desk\n\n"
        "# Moving weekend\n\n"
        "The removals team will work on Saturday. IT will connect desks, printers and the meeting "
        "rooms on Sunday. The building will be closed to staff on both days.\n\n"
        "# First week at Canal Street\n\n"
        "Desks are unassigned for the first fortnight so that teams can find the layout that works. "
        "Building passes will be on your desk on Monday morning, with a short welcome guide.\n\n"
        "Questions can go to the move channel or to me directly. Thank you for your patience while "
        "we settle in.\n"
    )
    d.sel_para("Memorandum")
    d.f(family="Liberation Sans", size=28, bold=True, color="2E4057", spacing=0.5)
    d.p(space_after=18)
    d.sel_para("To:", 4)
    d.tabs((72, "Left", "None"))
    d.p(space_after=4)
    for label in ("To:", "From:", "Date:", "Subject:"):
        d.find(label)
        d.f(bold=True, color="2E4057")
    d.sel_para("Subject:")
    d.borders("Bottom")
    d.p(space_after=18)
    return finish(d, title="Memo", author="Priya Raman")


# ── Reports and proposals ─────────────────────────────────────────────────────────────────────


def report() -> Write:
    d = Write("report")
    d.a4()
    house(d, accent="2A4A9E", head="Caladea", h1=18)
    d.md(
        "# Executive summary\n\n"
        "In 2025 the Coastal Paths Network welcomed 1.86 million visits, 12 % more than the year "
        "before, while maintenance costs per kilometre fell for the second year running. This report "
        "reviews the year, sets out where the network is under pressure, and recommends three "
        "priorities for 2026.\n\n"
        "- Visits grew fastest on the restored northern section (+31 %)\n"
        "- Volunteer hours reached 41,200, the highest on record\n"
        "- Erosion now threatens 6.4 km of path, up from 4.1 km\n\n"
        "# Visitor numbers\n\n"
        "Counters at fourteen access points recorded visits by month. Summer peaks were higher than in "
        "any previous year, but the strongest growth came in spring and autumn, which spreads the "
        "load on car parks and facilities.\n\n"
        "| Section | 2024 visits | 2025 visits | Change |\n"
        "|---|---|---|---|\n"
        "| Northern | 312,000 | 409,000 | +31 % |\n"
        "| Estuary | 488,000 | 521,000 | +7 % |\n"
        "| Cliffs | 540,000 | 583,000 | +8 % |\n"
        "| Southern | 322,000 | 347,000 | +8 % |\n"
        "| Total | 1,662,000 | 1,860,000 | +12 % |\n\n"
        "# Maintenance and erosion\n\n"
        "Routine maintenance cost £1,140 per kilometre, down from £1,220, thanks largely to volunteer "
        "work parties on drainage and vegetation. Erosion is the main risk: two winter storms moved "
        "the cliff edge inland by up to four metres at Gull Point.\n\n"
        "## Sections at risk\n\n"
        "Survey work identified eleven sites where the path is within five metres of an active cliff "
        "edge. Four need a permanent realignment within two years.\n\n"
        "# Recommendations\n\n"
        "1. Fund the realignment of the four highest-risk sites in 2026 (£186,000)\n"
        "2. Extend the volunteer ranger scheme to the southern section\n"
        "3. Install three more visitor counters to cover the new estuary links\n\n"
        "# Volunteers and partners\n\n"
        "Volunteers gave 41,200 hours in 2025, the equivalent of twenty-three full-time staff. "
        "Work parties cleared 38 km of vegetation, rebuilt 212 drainage channels and repaired 64 "
        "stiles and gates. The ranger scheme on the northern section now has 46 trained volunteers "
        "who walk their stretch at least once a month and report problems through the app.\n\n"
        "| Partner | Contribution in 2025 |\n"
        "|---|---|\n"
        "| Harbourside Rotary Club | Funded two new access gates at Mill Cove |\n"
        "| Saltmarsh Wildlife Group | Surveyed nesting birds before path works |\n"
        "| Eastport College | Students built 120 m of boardwalk as course work |\n"
        "| Seaview Holiday Parks | Sponsored the summer litter-pick weekends |\n\n"
        "## Finances\n\n"
        "Total spending was £2.31 million against a budget of £2.38 million. Grants and donations "
        "covered 64 % of costs, the local authorities 29 % and car park income the remaining 7 %. "
        "The underspend will be carried forward to the erosion programme.\n\n"
        "# Next steps\n\n"
        "The board is asked to approve the 2026 capital plan at its April meeting. Detailed costings "
        "and the full erosion survey are in the appendices.\n"
    )
    table(d, "Section", fill="3159C6",
          widths={"Section": 151, "2024 visits": 100, "2025 visits": 100, "Change": TEXT_W - 351}, right=("2024 visits", "2025 visits", "Change"),
          band="DCE6F0", rows_after=("Total",),
          first_col=("Northern", "Estuary", "Cliffs", "Southern", "Total"))
    table(d, "Partner", fill="3159C6", widths={"Partner": 170, "Contribution in 2025": TEXT_W - 170})
    d.w("EditHeader")
    d.text("Coastal Paths Network · Annual report 2025")
    d.w("MoveCaret", {"motion": "LineStart", "extend": True})
    d.f(size=8.5, color="7A7A7A", spacing=0.5)
    d.w("CloseHeaderFooter")
    d.w("EditFooter")
    d.w("PageNumber", {"position": "Caret"})
    d.w("MoveCaret", {"motion": "LineStart", "extend": True})
    d.f(size=9, color="7A7A7A")
    d.e("AlignRight")
    d.w("CloseHeaderFooter")
    d.start()
    d.w("InsertCoverPage", {"design": "Banded"})
    for placeholder, text in (("Document title", "Annual report 2025"),
                              ("Document subtitle", "Coastal Paths Network · A year on the coast path"),
                              ("Author", "Prepared by Imogen Hart, Network Manager")):
        d.replace_all(placeholder, text)
    # The cover page is dated the day it is made: the template's date is fixed.
    d.replace_all(r"\d{1,2} [A-Z][a-z]+ \d{4}", "March 2026", regex=True)
    d.w("DifferentFirstPage")
    return finish(d, title="Annual report 2025", author="Imogen Hart", company="Coastal Paths Network")


def proposal() -> Write:
    d = Write("proposal")
    d.a4()
    house(d, accent="2F6B3A", head="Caladea", h1=17)
    d.md(
        "Project proposal\n\n"
        "Community repair café for Eastbrook\n\n"
        "Prepared for the Eastbrook Neighbourhood Fund · Submitted by Tilly Okonjo · 18 January 2026\n\n"
        "# Summary\n\n"
        "We propose a monthly repair café in the Eastbrook Library hall, where volunteers help "
        "residents fix clothes, small electricals, bicycles and furniture instead of throwing them "
        "away. We ask the Fund for £8,400 to run twelve sessions, train volunteers and buy tools.\n\n"
        "# The need\n\n"
        "Eastbrook households sent an estimated 310 tonnes of bulky and electrical waste to landfill "
        "last year. In our survey of 214 residents, 71 % said they had thrown away something that "
        "could have been repaired, most often because they did not know how or could not afford a "
        "professional repair.\n\n"
        "# What we will do\n\n"
        "- Run a repair café on the second Saturday of every month, 10 am to 2 pm\n"
        "- Recruit and train twenty volunteer fixers, with safety checks for electrical repairs\n"
        "- Lend tools between sessions through the library's catalogue\n"
        "- Record every repair so that we can report what was saved\n\n"
        "# Timeline\n\n"
        "| Phase | Months | Milestone |\n"
        "|---|---|---|\n"
        "| Set-up | February – March | Volunteers recruited, tools bought, insurance in place |\n"
        "| Launch | April | First repair café and open day |\n"
        "| Running | May – January | Monthly sessions, tool library open |\n"
        "| Review | February 2027 | Report to the Fund and the community |\n\n"
        "# Budget\n\n"
        "| Item | Cost |\n"
        "|---|---|\n"
        "| Tools and consumables | £3,200 |\n"
        "| Volunteer training and safety testing | £1,900 |\n"
        "| Hall hire (12 sessions) | £1,440 |\n"
        "| Publicity and printing | £860 |\n"
        "| Insurance | £1,000 |\n"
        "| Total requested | £8,400 |\n\n"
        "# Measuring success\n\n"
        "By the end of the first year we aim to repair 600 items, divert at least four tonnes from "
        "landfill and have 150 residents attend more than one session.\n"
    )
    d.sel_para("Project proposal")
    d.f(size=10, color="2F6B3A", bold=True, all_caps=True, spacing=2)
    d.p(space_after=4)
    d.sel_para("Community repair café")
    d.apply("Title")
    d.sel_para("Prepared for the Eastbrook")
    d.f(size=9.5, color="6B6B6B")
    d.borders("Bottom")
    d.p(space_after=12)
    table(d, "Phase", fill="2F6B3A", widths={"Phase": 80, "Months": 120, "Milestone": TEXT_W - 200})
    table(d, "Item", fill="2F6B3A", widths={"Item": 340, "Cost": TEXT_W - 340}, right=("Cost",),
          band="E2EFE3", rows_after=("Total requested",),
          first_col=("Tools and consumables", "Volunteer training", "Hall hire", "Publicity and",
                     "Insurance", "Total requested"))
    return finish(d, title="Project proposal", author="Tilly Okonjo")


def research_paper() -> Write:
    d = Write("research-paper")
    d.a4()
    house(d, accent="1A1A1A", head="Liberation Serif", body="Liberation Serif", size=11, h1=13,
          title=20, line=1.2)
    d.style("Heading1", f={"family": "Liberation Serif", "size": 12, "color": "1A1A1A", "bold": True,
                           "small_caps": True, "spacing": 0.5},
            p={"space_before": 16, "space_after": 6, "keep_next": True})
    d.md(
        "Shade and soil moisture in urban street trees: evidence from twelve streets\n\n"
        "Hannah Ostrowski¹ · Kwame Mensah² · Lucía Ferrer¹\n\n"
        "¹ Department of Geography, University of Northbridge · ² City Trees Trust, Eastport\n\n"
        "Abstract. Street trees cool cities, but many die young because the soil around them dries "
        "out in summer. We measured soil moisture at 144 trees on twelve streets over two summers and "
        "compared trees whose pits were shaded by buildings for more than half the day with trees in "
        "full sun. Pits in shade held on average 23 % more water in July and August, and trees in them "
        "grew 18 % more new wood. We discuss what this means for planting plans.\n\n"
        "Keywords: urban forestry, soil moisture, shade, street design\n\n"
        "# 1. Introduction\n\n"
        "Cities plant street trees for shade, cooler air and wellbeing, yet a large share do not "
        "survive their first decade. Drought stress in summer is a leading cause. Where a tree is "
        "planted along a street affects how much sun its pit receives, but planting plans rarely "
        "take this into account.\n\n"
        "# 2. Methods\n\n"
        "We chose twelve streets in Eastport with similar soils, tree species and planting dates. On "
        "each street we fitted moisture probes at 30 cm depth beside twelve trees and logged readings "
        "every hour from June to September in 2024 and 2025. We classed each pit as shaded or sunny "
        "from building shadows modelled for the summer solstice.\n\n"
        "# 3. Results\n\n"
        "| Measure | Shaded pits | Sunny pits | Difference |\n"
        "|---|---|---|---|\n"
        "| Mean soil moisture, July–August | 24.1 % | 19.6 % | +23 % |\n"
        "| Days below wilting point | 6 | 19 | −68 % |\n"
        "| New wood growth (mm) | 7.4 | 6.3 | +18 % |\n\n"
        "Shaded pits stayed wetter through both summers, and the gap widened in the dry August of "
        "2025. Trees in sunny pits spent almost three times as many days below the wilting point.\n\n"
        "# 4. Discussion\n\n"
        "Our results suggest that the sunniest positions along a street need deeper pits, more "
        "organic matter or watering in their first five summers. Shade from buildings is not "
        "always available, but planting plans can use it where it is.\n\n"
        "Two limits apply. All twelve streets share one soil type, so the effect may be smaller on "
        "free-draining sands, and two summers cannot show whether the extra growth lasts. We have "
        "kept the probes in place to follow the same trees for a further five years.\n\n"
        "# 5. Conclusions\n\n"
        "Shade from buildings makes a measurable difference to the water available to young street "
        "trees. Councils can use simple shadow models, already common in planning, to decide where "
        "extra soil volume and summer watering will do the most good.\n\n"
        "# Acknowledgements\n\n"
        "We thank the Eastport street tree team for access to the planting records, and the residents "
        "who watched over the probes on their doorsteps. The work was funded by the Northbridge "
        "Urban Futures Fund (grant UF-2023-117).\n\n"
        "# References\n\n"
        "Fenwick, R. and Adeyemi, T. (2021) Why young street trees die. *Urban Ecology Review*, 14(2), 55–71.\n\n"
        "Morales, P. (2019) *Soils for City Trees*. Northbridge: Riverside Academic Press.\n\n"
        "Osei, K., Larsen, M. and Byrne, J. (2023) Pit design and tree survival in compacted soils. "
        "*Journal of Street Ecology*, 8(1), 12–29.\n\n"
        "Price, D. and Whitfield, A. (2024) Shadow modelling for street design. *Planning Methods*, "
        "31(4), 201–219.\n\n"
        "Varga, E. (2020) Water use of young lime and plane trees in paved sites. *Arboricultural "
        "Notes*, 46, 88–97.\n"
    )
    d.sel_para("Shade and soil moisture")
    d.apply("Title")
    d.e("AlignCenter")
    d.f(bold=True)
    d.p(space_after=12)
    d.sel_para("Hannah Ostrowski")
    d.e("AlignCenter")
    d.p(space_after=2)
    d.sel_para("¹ Department of Geography")
    d.f(size=9, italic=True, color="555555")
    d.e("AlignCenter")
    d.p(space_after=18)
    d.sel_para("Abstract. Street trees")
    d.f(size=10)
    d.p(indent_left=28, indent_right=28, alignment="Justify")
    d.find("Abstract.")
    d.f(bold=True)
    d.sel_para("Keywords:")
    d.f(size=10, italic=True)
    d.p(indent_left=28, indent_right=28, space_after=12)
    d.borders("Bottom")
    for text in ("Cities plant street", "We chose twelve", "Shaded pits stayed", "Our results suggest",
                 "Two limits apply", "Shade from buildings makes", "We thank the Eastport"):
        d.sel_para(text)
        d.p(alignment="Justify")
    table(d, "Measure", fill="FFFFFF", color="1A1A1A", style="TableGrid",
          widths={"Measure": 187, "Shaded pits": 88, "Sunny pits": 88, "Difference": 88},
          right=("Shaded pits", "Sunny pits", "Difference"),
          first_col=("Mean soil moisture", "Days below wilting", "New wood growth"))
    d.sel_para("Shaded pits stayed")
    d.p(space_before=10)
    for ref in ("Fenwick, R.", "Morales, P.", "Osei, K.", "Price, D.", "Varga, E."):
        if ref == "Varga, E.":
            d.sel_to_end(ref)
        else:
            d.sel_para(ref)
        d.p(indent_left=24, first_line=-24, space_after=4)
        d.f(size=10)
    d.w("EditFooter")
    d.w("PageNumber", {"position": "Caret"})
    d.w("MoveCaret", {"motion": "LineStart", "extend": True})
    d.e("AlignCenter")
    d.f(size=9, family="Liberation Serif")
    d.w("CloseHeaderFooter")
    return finish(d, title="Research paper", author="Hannah Ostrowski")


# ── Business documents ────────────────────────────────────────────────────────────────────────


def invoice() -> Write:
    d = Write("invoice")
    d.a4(top=60)
    house(d, accent="B4532A", head="Caladea", size=10.5)
    r = TEXT_W
    d.md(
        "Wren & Pike Studio\tINVOICE\n\n"
        "Unit 3, Mill Yard, Sheffield S3 8PB\tInvoice no. INV-2026-0142\n\n"
        "hello@wrenpike.example · +44 114 496 0782\tIssued 3 March 2026\n\n"
        "VAT reg. GB 284 1937 06\tDue 2 April 2026\n\n"
        "Bill to\n\n"
        "Moorland Coffee Roasters Ltd\n\nAttn: Sam Achebe, Head of Marketing\n\n"
        "27 Kelham Island Road, Sheffield S3 8RY\n\n"
        "| Description | Qty | Rate | Amount |\n"
        "|---|---|---|---|\n"
        "| Brand identity refresh: logo, palette and type system | 1 | £3,200.00 | £3,200.00 |\n"
        "| Packaging design, three single-origin labels | 3 | £650.00 | £1,950.00 |\n"
        "| Photography direction, one-day shoot | 1 | £720.00 | £720.00 |\n"
        "| Print management and proof checks (hours) | 6 | £55.00 | £330.00 |\n"
        "| Subtotal |  |  | £6,200.00 |\n"
        "| VAT at 20 % |  |  | £1,240.00 |\n"
        "| Total due |  |  | £7,440.00 |\n\n"
        "Payment details\n\n"
        "Please pay by bank transfer within 30 days, quoting the invoice number.\n\n"
        "Bank: Northern Mutual Bank · Account name: Wren & Pike Studio Ltd · "
        "Sort code: 40-12-76 · Account: 81457362\n\n"
        "Thank you for your business.\n\n"
        "Notes\n\n"
        "Late payments may be charged interest at 8 % a year above the base rate under the Late Payment "
        "of Commercial Debts (Interest) Act 1998. Artwork files are released when this invoice is paid "
        "in full.\n\n"
        "Wren & Pike Studio Ltd · Registered in England and Wales no. 11834502 · Unit 3, Mill Yard, "
        "Sheffield S3 8PB\n"
    )
    d.sel_para("Wren & Pike Studio", 4)
    d.tabs((r, "Right", "None"))
    d.p(space_after=0)
    d.f(size=9.5, color="555555")
    d.find("Wren & Pike Studio")
    d.f(family="Caladea", size=22, color="222222")
    d.find("INVOICE")
    d.f(family="Caladea", size=26, color="B4532A", spacing=3)
    d.find("Invoice no. INV-2026-0142")
    d.f(bold=True, color="222222")
    d.sel_para("VAT reg.")
    d.p(space_after=24)
    d.borders("Bottom")
    d.sel_para("Bill to")
    d.f(size=9, bold=True, color="B4532A", all_caps=True, spacing=1.5)
    d.p(space_before=12, space_after=4)
    d.sel_para("Moorland Coffee Roasters", 3)
    d.p(space_after=0)
    d.find("Moorland Coffee Roasters Ltd")
    d.f(bold=True)
    d.sel_para("27 Kelham Island Road")
    d.p(space_after=24)
    table(d, "Description", fill="B4532A", widths={"Description": 255, "Qty": 40, "Rate": 78, "Amount": TEXT_W - 373},
          right=("Qty", "Rate", "Amount"), rows_after=("Total due",), band="F6E3D9",
          first_col=("Brand identity refresh", "Packaging design,", "Photography direction",
                     "Print management", "Subtotal", "VAT at 20 %", "Total due"))
    d.find("Total due")
    d.table_select("Row")
    d.f(size=12, color="B4532A")
    d.sel_para("Payment details")
    d.f(size=9, bold=True, color="B4532A", all_caps=True, spacing=1.5)
    d.p(space_before=24, space_after=4)
    d.sel_para("Bank: Northern Mutual")
    d.f(size=9.5, color="555555")
    d.sel_para("Thank you for your business.")
    d.f(family="Caladea", size=13, italic=True, color="B4532A")
    d.p(space_before=18)
    d.sel_para("Notes")
    d.f(size=9, bold=True, color="B4532A", all_caps=True, spacing=1.5)
    d.p(space_before=36, space_after=4)
    d.sel_para("Late payments may")
    d.f(size=9, color="555555")
    d.sel_to_end("Wren & Pike Studio Ltd · Registered")
    d.f(size=8, color="8A8A8A")
    d.e("AlignCenter")
    d.borders("Top")
    d.p(space_before=48)
    return finish(d, title="Invoice INV-2026-0142", author="Wren & Pike Studio",
                  company="Wren & Pike Studio Ltd")


def meeting_notes() -> Write:
    d = Write("meeting-notes")
    d.a4(top=60)
    house(d, accent="4B3F8C", head="Carlito", body="Carlito", h1=14, title=26)
    d.style("Heading1", f={"family": "Carlito", "size": 13, "color": "4B3F8C", "bold": True},
            p={"space_before": 16, "space_after": 6, "keep_next": True})
    d.md(
        "Product team weekly\n\n"
        "Meeting notes · Tuesday 10 February 2026 · 10:00–10:45 · Room 4 and online\n\n"
        "| Chair | Notes | Attendees | Apologies |\n"
        "|---|---|---|---|\n"
        "| Ines Duarte | Theo Park | Ines, Theo, Maya, Rohan, Femi, Clara | Josh |\n\n"
        "# Agenda\n\n"
        "1. Release 4.2 status\n"
        "2. Customer feedback from the January survey\n"
        "3. Planning for the spring offsite\n\n"
        "# Discussion\n\n"
        "**Release 4.2.** Testing found two issues in the export flow; both are fixed and waiting "
        "for review. Release stays on track for Thursday.\n\n"
        "**Customer feedback.** 412 responses. Search and offline access are the most requested "
        "improvements; satisfaction is up four points on last quarter.\n\n"
        "**Spring offsite.** Two venues shortlisted. Budget agreed at £4,500 for two days.\n\n"
        "# Decisions\n\n"
        "- Ship release 4.2 on Thursday 12 February after the final review\n"
        "- Make offline access the headline goal for the next quarter\n\n"
        "# Actions\n\n"
        "| Action item | Owner | Due |\n"
        "|---|---|---|\n"
        "| Review the export fixes and approve the release | Maya | 11 Feb |\n"
        "| Share the survey summary with the wider team | Rohan | 13 Feb |\n"
        "| Draft an outline for offline access | Femi | 20 Feb |\n"
        "| Book the offsite venue | Clara | 17 Feb |\n\n"
        "Next meeting: Tuesday 17 February, 10:00, Room 4.\n"
    )
    d.sel_para("Product team weekly")
    d.apply("Title")
    d.f(bold=True)
    d.sel_para("Meeting notes · Tuesday")
    d.f(size=10, color="6B6B6B")
    d.p(space_after=14)
    table(d, "Chair", fill="ECE9F7", color="4B3F8C", style="PlainTable1",
          widths={"Chair": 90, "Notes": 80, "Attendees": 220})
    table(d, "Action item", fill="4B3F8C", widths={"Action item": 290, "Owner": 80})
    for lead in ("Release 4.2.", "Customer feedback.", "Spring offsite."):
        d.find(lead)
        d.f(color="4B3F8C")
    d.sel_to_end("Next meeting:")
    d.f(italic=True, color="6B6B6B")
    d.p(space_before=18)
    return finish(d, title="Meeting notes", author="Theo Park")


def press_release() -> Write:
    d = Write("press-release")
    d.a4(top=64)
    house(d, accent="0F5C8C", head="Liberation Sans", body="Liberation Serif", size=11, h1=13, line=1.25)
    d.md(
        "Lanternfield Robotics\n\n"
        "For immediate release\n\n"
        "Lanternfield Robotics opens its first school robotics lab in Cardiff\n\n"
        "Free after-school programme will reach 600 pupils across eight schools in its first year\n\n"
        "CARDIFF, 24 February 2026 — Lanternfield Robotics today opened a robotics lab at Bay View "
        "High School, the first of eight it will run with schools across Cardiff this year. The lab "
        "offers free after-school sessions where pupils aged 11 to 16 design, build and program their "
        "own robots, guided by Lanternfield engineers.\n\n"
        "The programme was shaped with teachers, who asked for hands-on projects that link maths and "
        "science lessons to real engineering. Each school receives a full kit of parts, laptops and "
        "a trained lab leader for the year.\n\n"
        "“Every pupil should get the chance to build something that moves and thinks,” said Gwen "
        "Morgan, chief executive of Lanternfield Robotics. “We want these labs to be the place where "
        "the next generation of Welsh engineers discovers what they love.”\n\n"
        "Bay View head teacher Owen Price added: “Within a week, pupils who had never written a line of "
        "code were racing their robots down the corridor. The energy has been remarkable.”\n\n"
        "The remaining seven labs will open between April and September 2026. Schools in Cardiff can "
        "register their interest at lanternfield.example/schools.\n\n"
        "About Lanternfield Robotics\n\n"
        "Lanternfield Robotics designs autonomous machines for warehouses and farms. Founded in 2019, "
        "the company employs 140 people at its headquarters in Cardiff Bay.\n\n"
        "Media contact\n\n"
        "Rhys Llewellyn, Communications Manager · press@lanternfield.example · +44 29 2018 0456\n\n"
        "###\n"
    )
    d.sel_para("Lanternfield Robotics")
    d.f(family="Liberation Sans", size=11, bold=True, color="0F5C8C", all_caps=True, spacing=2)
    d.p(space_after=2)
    d.sel_para("For immediate release")
    d.f(family="Liberation Sans", size=9, color="6B6B6B", all_caps=True, spacing=1.5)
    d.borders("Bottom")
    d.p(space_after=24)
    d.sel_para("Lanternfield Robotics opens")
    d.f(family="Liberation Sans", size=22, bold=True, color="1A1A1A")
    d.p(space_after=8, line_multiple=1.05)
    d.sel_para("Free after-school programme")
    d.f(family="Liberation Serif", size=13, italic=True, color="444444")
    d.p(space_after=18)
    d.find("CARDIFF, 24 February 2026 —")
    d.f(bold=True, family="Liberation Sans", size=10)
    d.sel_para("“Every pupil should")
    d.p(indent_left=28, indent_right=28, space_before=6, space_after=12)
    d.f(family="Liberation Sans", size=12.5, italic=True, color="0F5C8C")
    for h in ("About Lanternfield Robotics", "Media contact"):
        d.sel_para(h)
        d.f(family="Liberation Sans", size=10, bold=True, color="0F5C8C", all_caps=True, spacing=1)
        d.p(space_before=14, space_after=4)
    d.sel_to_end("###")
    d.e("AlignCenter")
    d.p(space_before=18)
    return finish(d, title="Press release", author="Rhys Llewellyn", company="Lanternfield Robotics")


# ── Newsletter and recipe ─────────────────────────────────────────────────────────────────────


def newsletter() -> Write:
    d = Write("newsletter")
    d.a4(top=48, bottom=56, left=56, right=56)
    house(d, accent="1F6F5C", head="Caladea", h1=17, line=1.15, size=10.5)
    d.md(
        "THE ALLOTMENT\n\n"
        "News from Hollins Lane Community Garden · Spring 2026 · Issue 18\n\n"
        "# Seed swap returns on 21 March\n\n"
        "Our spring seed swap is back in the polytunnel from 10 am to 1 pm. Bring spare seeds in "
        "labelled envelopes and take home something new. Last year more than ninety varieties changed "
        "hands, from purple carrots to heritage beans. Tea and Margaret's famous flapjacks will be "
        "served all morning.\n\n"
        "# New raised beds for wheelchair users\n\n"
        "Thanks to a grant from the Riverside Community Fund, six new waist-height beds now line the "
        "path by the shed. They are open to every member; to book one for the season, add your name "
        "to the list on the noticeboard.\n\n"
        "| This season's work days |  |\n"
        "|---|---|\n"
        "| 7 March | Clearing the orchard and mulching fruit trees |\n"
        "| 4 April | Building the new compost bays |\n"
        "| 2 May | Planting out the shared herb garden |\n"
        "| 6 June | Pond clean and wildflower survey |\n\n"
        "# From the plot: early potatoes\n\n"
        "Chit your seed potatoes now in egg boxes on a cool, bright windowsill. Plant them out in late "
        "March, 30 cm apart, and earth them up as the shoots appear. Charlotte and Rocket both do well "
        "on our heavy soil and should be ready to lift by late June.\n\n"
        "# Welcome to our new members\n\n"
        "A warm welcome to the Asante family, who have taken on plot 14, and to the pupils of Hollins "
        "Primary, whose gardening club now meets on Wednesday afternoons.\n\n"
        "# Bees on the east boundary\n\n"
        "Two hives arrived in October and came through the winter strong. Our beekeeper, Joe Nwosu, "
        "asks members to keep the gate by the hives closed and to leave the clover along the "
        "boundary uncut until July. Honey from the first harvest will be sold at the summer fair, "
        "with every penny going back into the garden.\n\n"
        "# Committee news\n\n"
        "At the AGM in January, members elected Ruth Kaplan as chair and Dev Malhotra as treasurer. "
        "The committee meets on the first Monday of each month in the shed; minutes are posted on the "
        "noticeboard and every member is welcome to attend.\n\n"
        "> “The garden gave me somewhere to be when I needed it most. Now I come for the people.” "
        "— Lorna, plot 9\n\n"
        "Membership is £24 a year · hollinslanegarden.example · Find us behind the library on Hollins Lane\n"
    )
    d.sel_para("THE ALLOTMENT")
    d.f(family="Caladea", size=40, color="FFFFFF", spacing=4)
    d.shade("1F6F5C")
    d.e("AlignCenter")
    d.p(space_after=0, space_before=0, line_multiple=1.0)
    d.sel_para("News from Hollins Lane")
    d.f(size=9.5, color="FFFFFF", spacing=0.5)
    d.shade("1F6F5C")
    d.e("AlignCenter")
    d.p(space_after=12)
    table(d, "This season's work days", fill="1F6F5C", style="PlainTable1")
    # The articles run in two columns under the masthead.
    d.find("Seed swap returns")
    d.w("MoveCaret", {"motion": "LineStart", "extend": False})
    d.w("SectionBreak", {"kind": "Continuous"})
    d.w("Columns", {"count": 2})
    d.sel_para("Seed swap returns")
    d.p(space_before=0)
    d.sel_para("“The garden gave me")
    d.f(family="Caladea", size=13, italic=True, color="1F6F5C")
    d.p(space_before=10, space_after=10, indent_left=0)
    d.sel_to_end("Membership is £24")
    d.f(size=9, color="1F6F5C", bold=True)
    d.e("AlignCenter")
    d.borders("Top")
    d.p(space_before=24)
    return finish(d, title="Community newsletter", author="Hollins Lane Community Garden")


def recipe() -> Write:
    d = Write("recipe")
    d.a4(top=60)
    house(d, accent="9C3D1C", head="Caladea", h1=16, title=32)
    d.md(
        "Roast squash and sage risotto\n\n"
        "A slow, comforting autumn supper. Roasting the squash first gives it a sweet, caramelised "
        "edge that stirs through the rice.\n\n"
        "| Serves | Preparation | Cooking | Difficulty |\n"
        "|---|---|---|---|\n"
        "| 4 | 20 minutes | 45 minutes | Easy |\n\n"
        "# Ingredients\n\n"
        "- 1 small butternut squash (about 900 g), peeled and cut into 2 cm cubes\n"
        "- 3 tbsp olive oil\n"
        "- 1.2 litres hot vegetable stock\n"
        "- 30 g butter\n"
        "- 1 onion, finely chopped\n"
        "- 2 garlic cloves, crushed\n"
        "- 300 g risotto rice\n"
        "- 150 ml dry white wine\n"
        "- 12 sage leaves\n"
        "- 60 g Parmesan-style hard cheese, finely grated, plus extra to serve\n\n"
        "# Method\n\n"
        "1. Heat the oven to 200 °C (180 °C fan). Toss the squash with 2 tbsp oil, season and roast "
        "for 30 minutes until golden at the edges.\n"
        "2. Meanwhile, melt half the butter in a wide pan and soften the onion for 8 minutes. Add the "
        "garlic and rice and stir for 2 minutes until the grains turn glossy.\n"
        "3. Pour in the wine and let it bubble away. Add the stock a ladle at a time, stirring often, "
        "until the rice is creamy but still has a little bite, about 20 minutes.\n"
        "4. Fry the sage leaves in the remaining oil until crisp, then drain on kitchen paper.\n"
        "5. Stir half the squash, the rest of the butter and the cheese into the risotto. Cover and rest "
        "for 2 minutes.\n"
        "6. Serve topped with the remaining squash, the crisp sage and extra cheese.\n\n"
        "# Cook's notes\n\n"
        "Keep the stock at a gentle simmer beside the risotto so that each ladle goes in hot. For a "
        "vegan version, use olive oil instead of butter and a plant-based hard cheese.\n"
    )
    d.sel_para("Roast squash and sage")
    d.apply("Title")
    d.p(space_after=8)
    d.sel_para("A slow, comforting")
    d.f(family="Caladea", size=13, italic=True, color="5A5A5A")
    d.p(space_after=16)
    table(d, "Serves", fill="F5E6DD", color="9C3D1C", style="PlainTable1")
    d.find("Easy")
    d.table_select("Row")
    d.e("AlignCenter")
    d.find("Serves")
    d.table_select("Row")
    d.e("AlignCenter")
    d.sel_to_end("Keep the stock")
    d.shade("FBF3EE")
    d.p(indent_left=8, indent_right=8)
    return finish(d, title="Recipe card", author="Nell Brannigan")
