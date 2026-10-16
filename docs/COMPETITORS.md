# Competitors

The field Kiln competes in, what each product does better than Kiln does today, where each one is
weak, and the concrete bar Kiln has to beat. This is the dossier behind PLAN.md §2A; it is
re-scanned at the start of every parity round (PLAN.md §7.3) and the parity tracker
([PARITY.md](../PARITY.md)) carries the resulting rows.

Last reviewed: 2026-10-16 (web search for each product; GitHub stars, latest releases and push
dates from the GitHub API on that day). The first scan was on 2026-10-15; changes found by this
re-scan are marked **New on 2026-10-16**.

## Summary

| Product | Licence | Latest (2026-10-16) | GitHub stars | Kiln's opening |
|---|---|---|---:|---|
| Microsoft 365 | proprietary, subscription | Copilot prompt box in Word/Excel/PowerPoint rolling out Sep–Dec 2026 | — | no account, no telemetry, Linux, AI off unless configured |
| Google Workspace | proprietary, web | Gemini in Docs/Sheets/Slides (beta since 2026-03-10) | — | native Office files, large sheets, real offline |
| LibreOffice | MPL-2.0 / LGPL | 26.8 (2026-08-26), 26.8.1 | 4,523 (`LibreOffice/core` mirror) | measured docx fidelity, a real ribbon, collaboration |
| Collabora Online | MPL-2.0, commercial support | 26.04 (2026-07-23) and a desktop Collabora Office | 3,376 (`CollaboraOnline/online`) | one install, offline first, no server needed |
| ONLYOFFICE Docs | AGPL-3.0 | **10.0 (2026-10-10)** | 7,027 (`ONLYOFFICE/DocumentServer`) | native desktop core, startup, large sheets |
| Euro-Office | AGPL-3.0 | first stable 2026-06-09 (DocumentServer 9.3.1 base) | 54 (`Euro-Office/documentation`) | same as ONLYOFFICE; works without a server |
| WPS Office | proprietary freemium | Mac update 2026-07-24 | — | no upsell, no ads, open source |
| Apple iWork | proprietary | Pages/Numbers/Keynote 15.x | — | Office fidelity, every platform |
| Univer | Apache-2.0 (Pro tiers paid) | **1.0.4 (2026-10-10)** | 22,635 (`dream-num/univer`) | a finished product with OOXML in the open core |
| GenOffice | Apache-2.0 | v0.11.829 (2026-10-15) | 9,793 (`genspark-ai/genoffice`) | Office-familiar UI, Android, collaboration, AI not routed through a vendor |
| ArtCraft "Craft" apps | MIT / Apache-2.0 | **v0.7.0 (2026-10-14)** | WordCraft 13,353 · GridCraft 5,253 · DeckCraft 3,493 | measured fidelity, Android, accessibility |

## Microsoft 365

- **What it is:** Word, Excel and PowerPoint on Windows, macOS, the web, iOS and Android; the
  reference for every behaviour Kiln copies. Proprietary, subscription with a Microsoft account.
- **Latest:** Copilot Agent Mode became the default across Word, Excel and PowerPoint on
  2026-04-22 (first scan). **New on 2026-10-16:** a prompt box attached to the Copilot button in
  Word, Excel and PowerPoint (type or dictate without opening the pane; worldwide rollout
  September–December 2026, message centre MC1470885); PowerPoint "Skills" (saved, repeatable
  Copilot workflows); `/` and `@` to call agents and skills; Excel Copilot's Plan mode (June 2026).
- **Better than Kiln v0.1:** fidelity by definition, 500+ Excel functions, Power Query and Power
  Pivot, Designer, Morph, co-authoring at scale, accessibility tooling, mobile apps.
- **Weak spots:** subscription and account, telemetry, no Linux desktop, AI pushed on by default,
  heavy (Startup Boost exists because cold start is slow; 8 GB RAM recommended).
- **Bar:** open the same `.docx/.xlsx/.pptx` with the §3.5 fidelity scores; cold start to Kiln
  Home < 2.5 s and open-document < 3 s on the 7 GB development laptop; every §1.7 shortcut
  identical; no account, no telemetry, AI off unless configured.
- **Sources:** [MC1470885 — Copilot button prompt box](https://mc.merill.net/message/MC1470885);
  [Office Watch, Copilot September 2026 updates](https://office-watch.com/?p=77878);
  [ICAEW, new tech features Mar–May 2026](https://www.icaew.com/technical/technology/excel-community/excel-community-articles/2026/new-tech-features-mar-may-26).

## Google Workspace

- **What it is:** Docs, Sheets and Slides in the browser; proprietary; Google account required.
- **Latest:** Gemini in Docs, Sheets and Slides (beta, 2026-03-10): "Help me create" drafting
  from Drive/Gmail/Chat, "Fill with Gemini" for tables, theme-matched slide generation. The
  re-scan found no newer Workspace editor release.
- **Better than Kiln v0.1:** effortless real-time collaboration and version history, zero install,
  comments and @mentions.
- **Weak spots:** Office files are converted on import and export (fidelity loss); slow on large
  sheets (50k+ rows); 10 million cell cap; offline is partial; account required.
- **Bar:** collaboration with cursors and offline merge (WP-82) on a server you host; 1M-cell
  sheets scroll at 55+ fps; `.xlsx` opened natively without conversion.
- **Sources:** [TechCrunch, 2026-03-10](https://www.techcrunch.com/2026/03/10/google-rolls-out-new-gemini-capabilities-to-docs-sheets-slides-and-drive/);
  [AlternativeTo](https://alternativeto.net/news/2026/3/google-gives-gemini-more-capabilities-for-collaborating-in-docs-sheets-slides-and-drive).

## LibreOffice

- **What it is:** the C++ suite (Writer, Calc, Impress, Draw, Math, Base) of The Document
  Foundation; MPL-2.0/LGPL; the ODF reference implementation. `LibreOffice/core` GitHub mirror:
  4,523 stars, pushed daily.
- **Latest:** **New on 2026-10-16:** LibreOffice 26.8 (2026-08-26) and 26.8.1: Writer's
  Paragraph Composer balances word spacing across consecutive justified lines; native OpenType
  variable fonts; Calc shuffle; OOXML round-trip preservation of waterfall, treemap and sunburst
  charts; faster opening of image-heavy documents; per-module Notebookbar colours; deliberately no
  generative AI. 26.2 (Markdown import/export, faster Calc with hidden columns, BIFF12 clipboard
  paste) reaches end of life on 2026-11-30.
- **Better than Kiln v0.1:** 30 years of format handling, typography (now including paragraph
  composition), Base, macros (Basic/Python), print engine.
- **Weak spots:** the switchers' top complaints — `.docx` style and font shifts, text-wrap pain,
  "looks different in Word"; dated UI (the ribbon-like Notebookbar is optional); no real-time
  collaboration without Collabora; large-sheet performance.
- **Bar:** beat LibreOffice's own docx round trip on our corpus (WP-93 scores LibreOffice's
  `docx → odt → docx` beside ours; ours must be ≥ theirs on ≥ 80 % of files); native ribbon;
  built-in collaboration. **Bar moved:** LibreOffice now preserves waterfall, treemap and
  sunburst charts through OOXML, so the XLSX and PPTX loop targets in §2A.3 below add "every
  chart type preserved on round trip".
- **Sources:** [Linuxiac, LibreOffice 26.8](https://linuxiac.com/libreoffice-26-8-released-with-new-paragraph-composer-variable-font-support/);
  [Phoronix](https://phoronix.com/news/LibreOffice-26.8);
  [DebugPoint, 26.8 features](https://www.debugpoint.com/libreoffice-26-8-features/?amp=1);
  [Deskmodder, 26.8.1](https://www.deskmodder.de/blog/2026/10/03/libreoffice-26-8-1-mit-verbesserter-kompatibilitaet-und-mehr/).

## Collabora Online

- **What it is:** LibreOffice's core served as a web editor with collaboration (C++/JS); MPL with
  commercial support; CODE is the community edition. `CollaboraOnline/online`: 3,376 stars.
- **Latest:** 26.04 (press release 2026-07-23): optional AI assistant, off by default, with
  commercial or self-hosted models; colour-coded document comparison with author and time;
  Markdown import/export; Calc per-user sheet views and calculated pivot fields; Impress
  follow-me collaborative presenting; Writer passed a BITV 2.0 accessibility audit. **New on
  2026-10-16:** Collabora has announced a desktop Collabora Office for Windows, macOS and Linux
  running the same code locally (reported by a Collabora developer; no release notes found).
- **Better than Kiln v0.1:** server-grade collaboration at Nextcloud scale, accessibility
  certification, document comparison.
- **Weak spots:** the web product needs a deployment to edit at all; tile-rendering latency;
  LibreOffice's UI underneath. The new desktop app narrows the "no offline story" gap but is a
  separate product from the collaborative server.
- **Bar:** accessibility — axe 0 serious/critical on every screen **and** a WCAG 2.2 AA
  self-audit (`docs/qa/a11y.md`) with 100 % keyboard operability of ribbon commands; AI off by
  default with a bring-your-own provider, like theirs; colour-coded comparison (parity row
  `write.review.compare.compare`).
- **Sources:** [Collabora press release](https://www.generation-nt.com/communiques-presse/collabora-online-26-04-celebrates-10th-year-with-release-adding-2079049);
  [AlternativeTo](https://alternativeto.net/news/2026/7/collabora-online-26-04-adds-optional-ai-assistant-and-smarter-document-comparison-and-review/);
  [Borncity](https://borncity.com/news/collabora-online-26-04-zehn-jahre-mit-ki-assistent-und-barrierefreiheit/);
  [Collabora forum, COOL 26.04](https://forum.collaboraonline.com/t/cool-26-04-is-here/4893).

## ONLYOFFICE Docs

- **What it is:** JavaScript editors with C++ converters whose native model is OOXML; AGPL-3.0;
  server (Docs/DocSpace) plus thin Desktop Editors and mobile apps.
  `ONLYOFFICE/DocumentServer`: 7,027 stars; `ONLYOFFICE/DesktopEditors`: 5,568.
- **Latest:** **New on 2026-10-16:** Docs 10.0 (2026-10-10; DocumentServer and Desktop Editors
  v10.0.0 tagged 2026-10-12/13): about 50 new features, 1,000+ fixes and 35 fixed
  vulnerabilities; an AI tab in every editor (summarise, rewrite, generate formulas, analyse
  data, presentation outlines, PDF questions) with quick actions and reusable prompts — removed
  from the open-source build after community feedback and installed separately, with any
  provider including local models; four optional AGPL apps (Files, Rooms, Forms, AI Agents);
  long tables flowing across pages; better RTL for inline objects and equations. Before that, 9.4
  (2026-05-19): dark mode for spreadsheets, 25 new themes and 20 transitions, a Chart Design tab.
- **Better than Kiln v0.1:** the best-regarded OOXML fidelity among open suites, a tabbed
  ribbon-like UI, forms, rooms, desktop and mobile editors, and now an agent/AI layer.
- **Weak spots:** licence-key friction; the Community edition is a server; the desktop app is a
  thin client; big-sheet performance; slow-ish startup.
- **Bar:** fidelity ≥ ONLYOFFICE's on the corpus is the long-term bar (WP-93 measures its `x2t`
  conversion from the Desktop Editors AppImage); 25+ themes and 20+ transitions in Kiln Deck by
  round 2 (parity row `deck.design.more-themes`). The AI bar is unchanged in substance — off by
  default, bring your own provider, local models allowed — and now matches both Collabora and
  ONLYOFFICE.
- **Sources:** [ONLYOFFICE blog, Docs 10.0](https://www.onlyoffice.com/blog/2026/10/onlyoffice-docs-10);
  [Linuxiac](https://linuxiac.com/onlyoffice-docs-10-0-released-with-50-new-features-and-ai-integration/);
  [It's FOSS](https://itsfoss.com/news/onlyoffice-docs-10-release/);
  [AlternativeTo](https://alternativeto.net/news/2026/10/onlyoffice-docs-10-0-launches-with-context-aware-ai-across-pretty-much-all-its-editors/).

## Euro-Office

- **What it is:** a fork of ONLYOFFICE backed by IONOS, Nextcloud, XWiki, OpenProject, Eurostack
  and others; AGPL-3.0; Windows, macOS, Linux, iOS and Android clients.
- **Latest:** first stable release 2026-06-09 with Nextcloud Hub 26 Spring (based on
  DocumentServer 9.3.1 according to secondary coverage); IONOS rollout in its Nextcloud Workspace
  and XWiki integration planned for Q4 2026. With ONLYOFFICE at 10.0 the fork now trails its
  upstream by a major version.
- **Better than Kiln v0.1:** co-editing, comments, version history, track changes, comparison,
  chat and forms; public-sector backing in the EU.
- **Weak spots:** server-centric like ONLYOFFICE; a young fork still converging; licence dispute
  history.
- **Bar:** the ONLYOFFICE bars, plus: Kiln works fully offline as a single install.
- **Sources:** [Nextcloud press release](https://nextcloud.com/blog/press_releases/sovereign-office-suite-euro-office-to-release-june-9/);
  [IT Brief UK](https://itbrief.co.uk/story/euro-office-launches-as-europe-s-open-source-office-rival);
  [Pillitteri](https://pasqualepillitteri.it/en/news/4773/euro-office-european-open-source-office-suite).

## WPS Office

- **What it is:** Kingsoft's proprietary freemium suite with an Office-like UI on every platform.
- **Latest:** **New on 2026-10-16:** Mac update of 2026-07-24 — AI Contract Review, AI Resume
  Assistant, AI citation linking for PDFs, five new spreadsheet functions, one-click
  presentations from templates; June 2026: AI proofing, an AI image editor, APA formatting and
  14 new languages.
- **Better than Kiln v0.1:** a very Office-like UI, strong PDF tools, mobile apps.
- **Weak spots:** ads and upsell, telemetry concerns, closed source, ~400 MB RAM reported.
- **Bar:** an Office-like UI with none of the upsell; PDF export quality; < 600 MB for a
  300-page document.
- **Sources:** [WPS what's new, Mac 2026-07-24](https://www.wps.com/whatsnew/mac/20260724/);
  [WPS what's new, 2026-06-03](https://br.wps.com/whatsnew/mac/20260603/);
  [futr.sg](https://futr.sg/?p=13598).

## Apple iWork

- **What it is:** Pages, Numbers and Keynote on Apple platforms; proprietary.
- **Latest:** the 15.x series. 15.1 (January 2026) moved premium templates, a Content Hub and
  generative image tools into the paid Apple Creator Studio bundle; 15.4 adds a long list of
  trigonometry, statistics and logic functions to Numbers and Screen View to Pages, and requires
  macOS 26. The first scan's "Siri questions about documents, request-access sharing" (September
  2026) could not be re-confirmed by this scan's searches and is kept as unverified.
- **Better than Kiln v0.1:** beautiful templates, Keynote's animation quality (Magic Move), free
  on Apple devices.
- **Weak spots:** Apple-only, weak Office fidelity, no Windows/Linux/Android; premium content now
  behind a subscription.
- **Bar:** template quality — 36 templates reviewed to "looks finished" (WP-85); a Morph-lite
  transition in the loop (round 2; parity row `deck.transitions.morph`).
- **Sources:** [TidBITS, 15.4](https://tidbits.com/watchlist/keynote-15-4-numbers-15-4-and-pages-15-4/);
  [9to5Mac](https://9to5mac.com/?p=1059040).

## Univer

- **What it is:** a TypeScript office SDK ("the Office harness for AI agents"); Apache-2.0 core
  with paid Pro and Server tiers. `dream-num/univer`: 22,635 stars, pushed daily.
- **Latest:** **New on 2026-10-16:** Univer 1.0 (September 2026) brings six editors — Sheets,
  Docs, Slides, Boards, Bases and PDFs — into one SDK, with a reworked formula engine,
  PivotCharts, a Server SDK and an AI SDK (agents load, edit and convert Office files and check
  results with screenshots and layout diagnostics); 1.0.4 (2026-10-10) adds header/footer fields
  in Docs and Paste Special for slides (Pro).
- **Better than Kiln v0.1:** an excellent spreadsheet engine and canvas renderer; an agent and MCP
  story.
- **Weak spots:** still an SDK, not an app — no ribbon or Backstage out of the box, OOXML
  import/export, pivots, charts and collaboration largely in the paid tiers, no desktop or mobile
  apps.
- **Bar:** beat it on *product*: ribbon UX, file fidelity in the open core, offline desktop and
  Android. Our grid is benchmarked against Univer's demo (scroll fps, 1M cells) in WP-71/WP-94
  and must beat it by ≥ 10 % in frame time. Its `@univerjs/engine-formula` (Apache-2.0) stays the
  documented formula-engine fallback.
- **Sources:** [GitHub releases (v1.0.4)](https://github.com/dream-num/univer/releases);
  [newreleases.io, v1.0.0](https://newreleases.io/project/github/dream-num/univer/release/v1.0.0);
  [ai-tldr, Univer 1.0](https://ai-tldr.dev/releases/dream-num-univer-1-0/).

## GenOffice

- **What it is:** Genspark's AI-first desktop suite (Docs, Sheets, Slides, PDF, Markdown, HTML)
  in TypeScript; Apache-2.0; public since 2026-07-31/08-03. `genspark-ai/genoffice`: 9,793 stars.
- **Latest:** **New on 2026-10-16:** v0.11.829 (2026-10-15): a mail-merge engine with MERGEFIELD
  kept as a field, Word-like word counts in the status bar, keep-with-next chains and
  page-anchored floating tables behaving like Word, OpenDocument files opened as editable OOXML
  copies, an AI mind map; official Linux builds. AI features run on a credit system through
  Genspark unless you bring a key.
- **Better than Kiln v0.1:** a clear AI-first UX, byte-preserving saves (only edited parts
  rewritten), an agent skill and CLI, AI edits as tracked changes with rollback, mail merge.
- **Weak spots:** AI routed through Genspark by default; no Android; no collaboration; not
  Office-familiar (no ribbon); young codebase with no published fidelity benchmark.
- **Bar:** byte preservation — every part we did not edit is byte-identical after save (tests in
  WP-65/WP-70/WP-79; parity row `suite.files.byte-preserving`); AI edits as tracked changes when a
  provider is configured (loop round 1); `kiln` CLI and MCP server (loop round 1); mail merge
  (parity row `write.files.mail-merge`, loop round 1).
- **Sources:** [GitHub release v0.11.829](https://github.com/genspark-ai/genoffice/releases);
  [OpenSourceForU](https://www.opensourceforu.com/2026/08/genspark-open-sources-genoffice-the-ai-native-desktop-alternative-to-microsoft-office/);
  [AI Weekly](https://aiweekly.co/alerts/genspark-open-sources-genoffice-ai-suite-for-mac-and-windows).

## ArtCraft "Craft" apps (WordCraft, GridCraft, DeckCraft)

- **What they are:** three pure-Rust apps (`storytold/*`), MIT or Apache-2.0, created 2026-10-06/07,
  largely AI-built; native on macOS, Windows, Linux and BSD plus WebAssembly; agent-drivable over
  MCP, a CLI and a JSON control channel. Stars on 2026-10-16: WordCraft 13,353, GridCraft 5,253,
  DeckCraft 3,493 (all three released v0.7.0 on 2026-10-14).
- **Latest:** **New on 2026-10-16:** v0.7.0 — WordCraft opens RTF in its code page, inserts
  WMF/EMF pictures, maps Wingdings/Symbol to Unicode and prints through a native print service;
  GridCraft fixes spill, LAMBDA, SUBTOTAL and histogram edge cases; DeckCraft matches
  PowerPoint's gradients, line spacing and CJK wrapping. Their READMEs now rate themselves:
  WordCraft "88 % of Word's ribbon features … about 60 % of the way to replacing Word";
  GridCraft "89 % of the Excel ribbon commands in our catalog", 500+ functions, 17 chart types,
  pivots saved to XLSX, Spanish formula names with decimal commas; DeckCraft "84 % of
  PowerPoint's features by breadth … about 55 % of the way".
- **Better than Kiln v0.1:** breadth already shipped across all three apps, localised formula
  entry, agent control.
- **Weak spots:** alpha by their own rating; no Android; Rust GUI toolkit accessibility is weak;
  fidelity is claimed per fix, not measured on a corpus.
- **Bar:** function count ≥ 513 (loop target; v0.1 ships ≥ 484); ≥ 17 chart types (v0.1 ships
  13); localised formula names (loop); every command drivable over a JSON control channel and MCP
  (loop round 1); and a *measured* fidelity corpus they do not have. Unchanged by this scan:
  their published numbers moved only within the bars already set.
- **Sources:** [WordCraft releases](https://github.com/storytold/wordcraft/releases);
  [GridCraft releases](https://github.com/storytold/gridcraft/releases);
  [DeckCraft releases](https://github.com/storytold/deckcraft/releases);
  [Office Watch](https://office-watch.com/?p=77930);
  [Pillitteri](https://pasqualepillitteri.it/en/news/22194/artcraft-wordcraft-gridcraft-deckcraft-office-clones-rust).

## Also noted

CryptPad (own formats, end-to-end encryption; `cryptpad/cryptpad` 8,008 stars), openDesk (the
German public-sector bundle, over 100,000 workplaces), Workbench (GitLab, unclear status),
Luckysheet (superseded by Univer), the Collabora and ONLYOFFICE mobile apps. A search for new
open-source office suites launched in October 2026 found none.

## Quality bars (PLAN.md §2A.3)

The bars each parity row's tests enforce. Changes from this re-scan are marked; PLAN.md keeps
the v0.1 column.

| Bar | v0.1 | Loop target ("beats competitors") | Verified by |
|---|---|---|---|
| DOCX round trip (corpus) | structural ≥ 0.95, text 1.00, visual ≥ 0.90, untouched parts byte-identical | structural ≥ 0.98, visual ≥ 0.95, ≥ LibreOffice's own score on ≥ 80 % of files, ≥ ONLYOFFICE on ≥ 50 % | `cargo xtask fidelity --format docx` |
| XLSX round trip | structural ≥ 0.97, values 1.00, formulas 1.00, oracle opens 100 % | structural ≥ 0.99; pivots/sparklines/slicers preserved as models; **every chart type, including waterfall/treemap/sunburst, preserved (moved: LibreOffice 26.8)** | `cargo xtask fidelity --format xlsx` |
| PPTX round trip | structural ≥ 0.95, text 1.00, visual ≥ 0.85 | visual ≥ 0.92; animations preserved 100 %; **every chart type preserved (moved: LibreOffice 26.8)** | `cargo xtask fidelity --format pptx` |
| Formula functions | ≥ 484 (ported), conformance ≥ 4,500 cases, 0 failures | ≥ 520 (every worksheet function except cube, RTD and web services; [parity/functions.json](../parity/functions.json) lists 523, 18 out) | `kiln functions --count`, conformance suite |
| Chart types | 13 | ≥ 18 | chart story screenshot count |
| Performance | §3.9 budgets | open time ≤ 0.25 × `soffice --convert-to` for the same file; grid frame time ≤ 0.9 × Univer demo | `cargo xtask budgets`, `perf.spec.ts` |
| Startup | Tauri cold start < 0.7 s to Kiln Home (Linux, warm cache) | < 0.4 s | `kiln-desktop --startup-probe`, `perf.spec.ts` |
| UX polish | no overlapping or clipped UI at 1280×800, 1440×900, 412×915 | same at 1024×768 and 1920×1080 at 125 %/150 % zoom; 4 px grid lint | `e2e/specs/layout-scan.spec.ts` |
| Accessibility | axe 0 serious/critical on all screens; 100 % of ribbon commands keyboard-reachable | WCAG 2.2 AA self-audit complete; screen-reader walkthrough per app | `a11y.spec.ts`, `docs/qa/a11y.md` |
| Shortcuts | 100 % of the §1.7 table | plus Excel's full Alt sequences for Home/Insert/Data | `shortcuts.spec.ts` |
| Collaboration | 2 users, cursors, offline merge, version history | 10 concurrent editors in a test, comments with @mentions | `collab.spec.ts` |
| Mobile | APK with full editing; touch e2e on Pixel 7 emulation | plus an iOS build in CI | `mobile.spec.ts`, `verify-apk.sh` |
| Agent/automation | command registry introspectable | JSON control channel, MCP server, `kiln` CLI | `control.spec.ts` (loop) |
| AI | hooks only, no provider, no network | bring-your-own provider incl. local models, off by default, edits as tracked changes (Collabora 26.04, ONLYOFFICE 10.0, GenOffice) | parity rows `suite.ai.*` |

## What changed in this scan, for the next gap analysis

1. ONLYOFFICE 10.0 and Collabora 26.04 both ship optional AI with local models; GenOffice ships
   mail merge. Kiln's rows `suite.ai.provider-tracked-changes` and `write.files.mail-merge` stay
   commonly-missed, P1, round 1.
2. Collabora now has a desktop app; "works offline" alone is no longer unique — the combination
   of offline desktop, Android and self-hosted collaboration from one codebase still is.
3. LibreOffice 26.8 preserves the newer OOXML chart types; the round-trip loop targets above now
   require every chart type to survive a save.
4. Univer 1.0 is a six-editor SDK; the product bar (ribbon, Backstage, open-core OOXML, offline
   apps) is unchanged, and the grid frame-time benchmark against its demo stays.
5. The Craft apps self-report 84–89 % ribbon breadth; Kiln's tracker measures the same thing in
   [PARITY.md](../PARITY.md), where a row counts only when its tests pass.
