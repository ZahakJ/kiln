## What this changes

<!-- One or two sentences, from the user's side. Which app: Kiln Write, Kiln Grid,
     Kiln Deck, Kiln Notes, Kiln Home, or the shells? -->

## Why

<!-- The problem it solves, or the issue it closes ("Closes #123"). -->

## How you verified it

<!-- Commands you ran and what you did in the app. For example: "`pnpm verify`
     and `pnpm e2e` pass; opened fixtures/docx/tables.docx, merged two cells,
     saved, and the file opens in Word/LibreOffice with the merge intact." -->

## Screenshots

<!-- For anything visible: light and dark theme, and a phone width if the
     layout changes there. -->

## Checklist

<!-- Tick what applies; say "n/a" rather than leaving a box unexplained. -->

- [ ] One concern. No unrelated refactor riding along.
- [ ] `pnpm verify` passes; `pnpm e2e` passes if the UI changed.
- [ ] Tests cover the change (unit, end-to-end, or a fixture for a reader or writer).
- [ ] Reader or writer changed? `pnpm fidelity` scores did not drop.
- [ ] User-visible strings go through i18n and use the product names.
- [ ] A line in `CHANGELOG.md` under `Unreleased` for anything a user would notice.
- [ ] New dependency? Pinned in the catalog and `pnpm licenses:check` passes.
- [ ] Commits signed off (`git commit -s`).
