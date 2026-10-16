# Kiln fonts

Both families are licensed under the SIL Open Font License 1.1. You may use, embed, bundle and redistribute them with Kiln (including in the AGPL app and its installers). The licence texts sit next to the files.

| File | Family | Designer | Licence | Source |
|---|---|---|---|---|
| `Fraunces-VF.ttf` | Fraunces (variable: opsz 9–144, wght 100–900, SOFT 0–100, WONK 0–1) | Undercase Type (Phaedra Charles, Flavia Zimbardi) | SIL OFL 1.1 (`OFL-Fraunces.txt`) | github.com/google/fonts/tree/main/ofl/fraunces |
| `Figtree-VF.ttf` | Figtree (variable: wght 300–900) | Erik Kennedy | SIL OFL 1.1 (`OFL-Figtree.txt`) | github.com/google/fonts/tree/main/ofl/figtree |

How Kiln uses them:
- Wordmark: Fraunces wght 900, SOFT 40, WONK 0, opsz 144, letters k, ı, l, n converted to outlines. The flame replaces the dot on the i. The wordmark is artwork. Never retype it.
- Display and headings: Fraunces 700–800, SOFT 40, WONK 0, optical size auto.
- Interface and default document body: Figtree 400–800.
- Compatibility (not bundled here): when a .docx asks for Calibri or Cambria, substitute the metric-compatible Carlito (OFL) and Caladea (Apache 2.0) so line breaks match Word.

UI icons in the mockups are Lucide (ISC licence, `../mockups/vendor/LUCIDE-LICENSE.txt`).
