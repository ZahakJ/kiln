#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Writes the template scripts and the templates index.

    python3 tools/templates/make.py

writes tools/templates/<app>-<name>.jsonl (one `kiln-cli control` script per template) and
assets/templates/index.json. `tools/templates/build.sh` then runs the scripts to make the native
bundles and their thumbnails. The content of each template is in write_templates.py,
grid_templates.py and deck_templates.py; the order below is the gallery's order.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.dont_write_bytecode = True
sys.path.insert(0, str(HERE))

import deck_templates as deck  # noqa: E402
import grid_templates as grid  # noqa: E402
import write_templates as write  # noqa: E402

# (function, name, description, featured)
CATALOGUE = {
    "Write": [
        (write.letter, "Business letter", "A formal one-page letter on a clean letterhead.", True),
        (write.resume, "Résumé", "A one-page CV with experience, education and skills.", True),
        (write.report, "Annual report", "A report with a cover page, tables and page numbers.", True),
        (write.invoice, "Invoice", "An itemised invoice with VAT, totals and payment details.", False),
        (write.newsletter, "Newsletter", "A two-column community newsletter with a masthead.", False),
        (write.meeting_notes, "Meeting notes", "Agenda, discussion, decisions and actions in one page.", False),
        (write.proposal, "Project proposal", "A funding proposal with a timeline and a budget.", False),
        (write.cover_letter, "Cover letter", "A job application letter that matches the résumé.", False),
        (write.memo, "Memo", "An internal memo with clear headings and actions.", False),
        (write.press_release, "Press release", "A news release with a quote and media contacts.", False),
        (write.research_paper, "Research paper", "An academic paper with an abstract and references.", False),
        (write.recipe, "Recipe", "A recipe card with ingredients, method and notes.", False),
    ],
    "Grid": [
        (grid.budget, "Monthly budget", "Income and spending against plan, with a chart.", True),
        (grid.tracker, "Project tracker", "Tasks, owners, dates and progress at a glance.", True),
        (grid.loan, "Loan calculator", "Monthly payments and a full repayment schedule.", True),
        (grid.invoice, "Invoice", "An invoice that totals itself, VAT included.", False),
        (grid.schedule, "Weekly schedule", "A colour-coded week planner in hourly slots.", False),
        (grid.gradebook, "Gradebook", "Weighted marks, averages and grades for a class.", False),
        (grid.expenses, "Expense report", "Expenses by date and category, ready to approve.", False),
        (grid.inventory, "Stock inventory", "Stock levels, values and what to reorder.", False),
        (grid.sales, "Sales report", "Monthly sales by region with totals and a chart.", False),
        (grid.timesheet, "Timesheet", "Hours worked in a week, with overtime and pay.", False),
        (grid.savings, "Savings goals", "Progress towards savings goals, month by month.", False),
        (grid.travel, "Travel planner", "An itinerary with costs set against a budget.", False),
    ],
    "Deck": [
        (deck.pitch, "Pitch deck", "A startup pitch: problem, solution, traction and the ask.", True),
        (deck.quarterly, "Quarterly review", "Results, highlights, risks and the next quarter.", True),
        (deck.report, "Business review", "A year in numbers, lessons and priorities.", False),
        (deck.lesson, "Lesson", "A classroom lesson with objectives and an activity.", False),
        (deck.portfolio, "Portfolio", "Selected projects, illustrated, with how you work.", False),
        (deck.timeline_deck, "Product roadmap", "A year of milestones on a timeline.", False),
        (deck.kickoff, "Project kickoff", "Goals, scope, team, milestones and next steps.", False),
        (deck.workshop, "Workshop", "A half-day workshop with a plan and exercises.", False),
        (deck.launch, "Product launch", "A launch briefing with audience, plan and prices.", False),
        (deck.all_hands, "All-hands", "A company meeting: news, people and celebrations.", False),
        (deck.talk, "Conference talk", "A talk with findings, a chart and lessons.", False),
        (deck.case_study, "Case study", "A client story: challenge, approach and results.", False),
    ],
}


def main() -> None:
    index = []
    for kind, entries in CATALOGUE.items():
        for make, name, description, featured in entries:
            script = make()
            app, stem = script.app, script.name
            ext = Path(script.path).suffix
            text = script.jsonl(title=name)
            if app == "write":
                # Kiln Write paints spelling marks under unknown names; its thumbnail is rendered
                # in the same session, with spell checking off, rather than from the saved file.
                text += json.dumps({"render": {"doc": 1, "png": f"assets/templates/{app}/{stem}.png",
                                               "page": 1, "dpi": 48}}) + "\n"
            (HERE / f"{app}-{stem}.jsonl").write_text(text, encoding="utf-8")
            index.append({
                "id": f"{app}-{stem}",
                "kind": kind,
                "name": name,
                "file": f"{app}/{stem}{ext}",
                "thumbnail": f"{app}/{stem}.png",
                "description": description,
                "featured": featured,
            })
    out = ROOT / "assets" / "templates" / "index.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"version": 1, "templates": index}, ensure_ascii=False, indent=2) + "\n",
                   encoding="utf-8")
    print(f"wrote {len(index)} scripts and {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
