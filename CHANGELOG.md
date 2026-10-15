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

[Unreleased]: https://github.com/ZahakJ/officina/commits/main
