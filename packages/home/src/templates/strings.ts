/**
 * The names and descriptions of the templates Kiln ships (assets/templates/index.json), marked
 * for translation: the gallery shows them in the interface language (`tx`), while the
 * templates themselves stay English. `strings.test.ts` checks this list against the index.
 */
import { msg } from '@kiln/i18n';

export const TEMPLATE_STRINGS: Readonly<Record<string, { name: string; description: string }>> = {
  'write-letter': {
    name: msg('Business letter'),
    description: msg('A formal one-page letter on a clean letterhead.'),
  },
  'write-resume': {
    name: msg('Résumé'),
    description: msg('A one-page CV with experience, education and skills.'),
  },
  'write-report': {
    name: msg('Annual report'),
    description: msg('A report with a cover page, tables and page numbers.'),
  },
  'write-invoice': {
    name: msg('Invoice'),
    description: msg('An itemised invoice with VAT, totals and payment details.'),
  },
  'write-newsletter': {
    name: msg('Newsletter'),
    description: msg('A two-column community newsletter with a masthead.'),
  },
  'write-meeting-notes': {
    name: msg('Meeting notes'),
    description: msg('Agenda, discussion, decisions and actions in one page.'),
  },
  'write-proposal': {
    name: msg('Project proposal'),
    description: msg('A funding proposal with a timeline and a budget.'),
  },
  'write-cover-letter': {
    name: msg('Cover letter'),
    description: msg('A job application letter that matches the résumé.'),
  },
  'write-memo': {
    name: msg('Memo'),
    description: msg('An internal memo with clear headings and actions.'),
  },
  'write-press-release': {
    name: msg('Press release'),
    description: msg('A news release with a quote and media contacts.'),
  },
  'write-research-paper': {
    name: msg('Research paper'),
    description: msg('An academic paper with an abstract and references.'),
  },
  'write-recipe': {
    name: msg('Recipe'),
    description: msg('A recipe card with ingredients, method and notes.'),
  },
  'grid-budget': {
    name: msg('Monthly budget'),
    description: msg('Income and spending against plan, with a chart.'),
  },
  'grid-tracker': {
    name: msg('Project tracker'),
    description: msg('Tasks, owners, dates and progress at a glance.'),
  },
  'grid-loan': {
    name: msg('Loan calculator'),
    description: msg('Monthly payments and a full repayment schedule.'),
  },
  'grid-invoice': {
    name: msg('Invoice'),
    description: msg('An invoice that totals itself, VAT included.'),
  },
  'grid-schedule': {
    name: msg('Weekly schedule'),
    description: msg('A colour-coded week planner in hourly slots.'),
  },
  'grid-gradebook': {
    name: msg('Gradebook'),
    description: msg('Weighted marks, averages and grades for a class.'),
  },
  'grid-expenses': {
    name: msg('Expense report'),
    description: msg('Expenses by date and category, ready to approve.'),
  },
  'grid-inventory': {
    name: msg('Stock inventory'),
    description: msg('Stock levels, values and what to reorder.'),
  },
  'grid-sales': {
    name: msg('Sales report'),
    description: msg('Monthly sales by region with totals and a chart.'),
  },
  'grid-timesheet': {
    name: msg('Timesheet'),
    description: msg('Hours worked in a week, with overtime and pay.'),
  },
  'grid-savings': {
    name: msg('Savings goals'),
    description: msg('Progress towards savings goals, month by month.'),
  },
  'grid-travel': {
    name: msg('Travel planner'),
    description: msg('An itinerary with costs set against a budget.'),
  },
  'deck-pitch': {
    name: msg('Pitch deck'),
    description: msg('A startup pitch: problem, solution, traction and the ask.'),
  },
  'deck-quarterly': {
    name: msg('Quarterly review'),
    description: msg('Results, highlights, risks and the next quarter.'),
  },
  'deck-report': {
    name: msg('Business review'),
    description: msg('A year in numbers, lessons and priorities.'),
  },
  'deck-lesson': {
    name: msg('Lesson'),
    description: msg('A classroom lesson with objectives and an activity.'),
  },
  'deck-portfolio': {
    name: msg('Portfolio'),
    description: msg('Selected projects, illustrated, with how you work.'),
  },
  'deck-timeline': {
    name: msg('Product roadmap'),
    description: msg('A year of milestones on a timeline.'),
  },
  'deck-kickoff': {
    name: msg('Project kickoff'),
    description: msg('Goals, scope, team, milestones and next steps.'),
  },
  'deck-workshop': {
    name: msg('Workshop'),
    description: msg('A half-day workshop with a plan and exercises.'),
  },
  'deck-launch': {
    name: msg('Product launch'),
    description: msg('A launch briefing with audience, plan and prices.'),
  },
  'deck-all-hands': {
    name: msg('All-hands'),
    description: msg('A company meeting: news, people and celebrations.'),
  },
  'deck-talk': {
    name: msg('Conference talk'),
    description: msg('A talk with findings, a chart and lessons.'),
  },
  'deck-case-study': {
    name: msg('Case study'),
    description: msg('A client story: challenge, approach and results.'),
  },
};
