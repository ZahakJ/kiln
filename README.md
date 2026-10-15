<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/media/banner.png">
    <source media="(prefers-color-scheme: light)" srcset="docs/media/banner-light.png">
    <img src="docs/media/banner.png" alt="Officina" width="820">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/ZahakJ/officina/actions/workflows/ci.yml"><img src="https://github.com/ZahakJ/officina/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/ZahakJ/officina/releases/latest"><img src="https://img.shields.io/github/v/release/ZahakJ/officina?color=3f6fe0&label=release" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-AGPL--3.0--or--later-3f6fe0" alt="AGPL-3.0-or-later"></a>
</p>

Officina is an open-source office suite: a word processor (**Scriba**), a
spreadsheet (**Tabula**), a presentation editor (**Scena**) and a notebook
(**Notula**), started from one hall, **Atrium**. It keeps the layout Microsoft
Office users already know (the ribbon, the File menu, the status bar and the
same keyboard shortcuts) and adds a command palette, version history and live
collaboration.

It opens and saves Word, Excel and PowerPoint files (`.docx`, `.xlsx`,
`.pptx`), imports and exports the OpenDocument formats, reads CSV, and exports
PDF. Parts of a file it does not understand, such as macros or embedded
objects, are carried through a save unchanged rather than dropped, and the
Compatibility Checker lists anything that will not survive before you save.

The same application runs as a desktop app on Linux, macOS and Windows, as an
Android app, and in the browser as an installable web app that works offline.
There is no account and nothing is sent anywhere unless you start a shared
session on a collaboration server you choose.

```sh
curl -fsSL https://github.com/ZahakJ/officina/releases/download/v0.1.0/officina-0.1.0-linux-x86_64.AppImage -o officina && chmod +x officina && ./officina
```

Windows, macOS and Android builds are on the
[releases page](https://github.com/ZahakJ/officina/releases/latest), with
`SHA256SUMS.txt`.

![Officina: opening a template from Atrium, editing with the ribbon, finding a command in the palette and saving as .docx](docs/media/demo.gif)

<p align="center"><img src="docs/media/screenshots/write-light.png" width="400" align="top" alt="Scriba, the word processor, in the light theme"><img src="docs/media/screenshots/grid-dark.png" width="400" align="top" alt="Tabula, the spreadsheet, with a chart, in the dark theme"><br><img src="docs/media/screenshots/deck-light.png" width="400" align="top" alt="Scena, the presentation editor, in the light theme"><img src="docs/media/screenshots/phone-write.png" width="400" align="top" alt="Scriba on an Android phone"></p>

## Features

**Scriba (documents)**

- **Pages as Word lays them out.** Headers and footers, sections, columns, footnotes, tables, floating pictures with text wrap, and Office's default styles, so a `.docx` paginates the way it does in Word.
- **Review.** Track changes with accept and reject, threaded comments, word count and spelling.
- **References.** Table of contents, captions, cross-references and bookmarks.
- **Markdown shortcuts** as you type (`#` for a heading, `-` for a list, `**bold**`), a focus mode, and the Format Painter.

**Tabula (spreadsheets)**

- **A formula engine with Excel's semantics:** more than 350 functions, dynamic arrays that spill, `LET` and `LAMBDA`, structured table references, and autocomplete that shows each function's arguments as you type.
- **The tools of the Excel ribbon:** sort and filter, conditional formatting, data validation, Format as Table, pivot tables, sparklines, Goal Seek, freeze panes and Page Layout.
- **Charts** that follow the data: column, bar, line, area, pie, scatter and combinations, read from and written to `.xlsx`.
- **Formula Lab,** a pane that shows how a formula is evaluated, step by step.

**Scena (presentations)**

- **Masters, layouts and themes** as PowerPoint defines them, with smart guides when you move shapes.
- **Slide show and presenter view,** with notes, a timer and the next slide, plus transitions and animations.
- **Handouts and notes pages** to print or export as PDF.

**Everywhere**

- **The Office layout:** ribbon, File menu (Backstage), status bar, Key Tips on Alt, and the Office shortcuts listed in [SHORTCUTS.md](docs/SHORTCUTS.md).
- **Command palette** (Ctrl+Shift+P) that also answers to Office's names for things.
- **Version history** for every document, with named versions, preview and restore.
- **Live collaboration** on documents and slides through a server you run, with presence and shared comments.
- **AutoSave and document recovery** after a crash.
- **Light, dark and high-contrast themes,** and English, German and Arabic with right-to-left layout.
- **Notula,** notebooks with sections, pages and tags, written with the same editor as Scriba.

## Compared with Microsoft Office

- **Opens and saves your files:** `.docx`, `.xlsx` and `.pptx` are the formats Officina reads and writes, not an import step. What it cannot edit it keeps.
- **No subscription, no account:** install it and use it. Nothing is tied to a sign-in.
- **Version history built in:** every document keeps its own history on your device, without OneDrive or SharePoint.
- **Collaboration you host:** live co-editing runs through a small server you control, on your own machine or network.
- **Find any command:** the palette searches every command by its Officina and its Office name.
- **Open source** under the GNU Affero General Public License.

## Building

Node 24 and pnpm (the version is pinned in `package.json`).

```sh
git clone https://github.com/ZahakJ/officina && cd officina && pnpm install
pnpm dev        # the web app on http://localhost:5173
pnpm verify     # lint, typecheck and unit tests across the workspace
pnpm e2e        # Playwright end-to-end tests in headless Chromium
```

`pnpm release:desktop --linux` builds the AppImage and `.deb`;
`pnpm release:android` builds the APK. [RELEASING.md](docs/RELEASING.md) covers
signing and publishing.

## Documentation

The manual is at **[zahakj.github.io/officina/manual](https://zahakj.github.io/officina/manual/)**:
installing on each platform, each application, collaboration, file formats and
troubleshooting. Its source is the Markdown in [docs/manual](docs/manual).

| | |
|---|---|
| [DESIGN.md](docs/DESIGN.md) | How it is built: packages, document models, the formula engine, file formats |
| [FORMATS.md](docs/FORMATS.md) | Fidelity notes for each file format |
| [SHORTCUTS.md](docs/SHORTCUTS.md) | Keyboard shortcuts in Scriba, Tabula and Scena |
| [SECURITY.md](SECURITY.md) | Reporting a vulnerability; known gaps |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Development loop, tests, code style |
| [RELEASING.md](docs/RELEASING.md) | Maintainer runbook |
| [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) | Bundled dependencies, fonts and icons, and their licences |

## Licence

GNU Affero General Public License, version 3 or later. See [LICENSE](LICENSE).
