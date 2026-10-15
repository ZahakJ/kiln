# Changelog

All notable changes to Officina are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Monorepo scaffold: pnpm workspace with Turborepo, TypeScript, Biome, Vitest and Playwright; the four apps (web, desktop, Android, collaboration server) and the library packages; a licence gate for every shipped dependency.
- Repository hygiene: AGPL-3.0-or-later licence, README, contributing guide, security policy, Code of Conduct, issue and pull request templates, CODEOWNERS and Dependabot.
- Continuous integration (lint, typecheck, unit tests with coverage, licence check, end-to-end tests) and a release workflow that builds Linux, Windows, macOS and Android packages from a tag and attaches them with `SHA256SUMS.txt`.
- The Officina mark, the README banner in dark and light, the social preview image and the application icons, all rendered from SVG by `pnpm media:render`.
- Placeholder landing page and Markdown manual viewer on GitHub Pages.
- Reference documents: design overview, file-format fidelity notes, keyboard shortcuts and the release runbook.
- "Horizon" design tokens: light, dark and high-contrast themes (following the OS by default), per-app accent colours, spacing, radii, shadows, motion and type scale as CSS variables, a typed TypeScript object and a Tailwind 4 theme; the theme choice is remembered.
- Bundled fonts: Inter and JetBrains Mono for the interface, and the metric-compatible document fonts Liberation Sans/Serif/Mono, Carlito and Caladea so Office documents lay out like they do in Word.
- UI building blocks shared by every app: buttons (including Classic-ribbon large and split buttons), toggles, menus with submenus and shortcuts, tooltips with Office-style descriptions, dialogs, tabs, drop-down lists, an editable font/size box, unit-aware number fields, an Office-style colour picker with theme tints and recent colours, a zoom slider, a styles gallery, toasts, keyboard keycaps and scroll areas, all keyboard accessible.
- Officina Glyphs: 275 icons for Office commands (Format Painter, Merge & Center, Freeze Panes, Track Changes, slide layouts…), with an optional colourful style.

[Unreleased]: https://github.com/ZahakJ/officina/commits/main
