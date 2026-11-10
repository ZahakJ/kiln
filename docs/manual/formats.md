# File formats and compatibility

Kiln reads and writes Office files directly, with no import step.

| App | Opens and saves | Also opens or exports |
|---|---|---|
| Kiln Write | `.docx`, `.dotx`, `.docm`, `.dotm` | `.odt` and `.ott` in the Linux build; `.txt` and `.md`; PDF export |
| Kiln Grid | `.xlsx`, `.xltx`, `.xlsm` | `.ods` and `.ots` in the Linux build; `.csv`, `.tsv` and `.json`; PDF export |
| Kiln Deck | `.pptx`, `.potx`, `.pptm`, `.ppsx` | `.odp` and `.otp` in the Linux build; PDF export |

Each app also has a native file, `.kwrite`, `.kgrid` or `.kdeck`, which carries the document with its version history: see [Files, AutoSave and recovery](files.md).

## What survives a round trip

An Office file is a package of parts. Kiln builds its document from the parts it models and keeps every other part as it found it. When you save, it writes the parts it models again, which can change their bytes even without an edit, and copies the others back byte for byte. So these stay in the file even though Kiln does not edit them:

- macros (`vbaProject.bin`) in `.docm`, `.xlsm` and `.pptm` files; Kiln never runs them
- embedded objects, custom XML, SmartArt and printer settings

A workbook's stored results stay as the file has them until something they depend on changes, also for a function Kiln cannot evaluate; a file saved with `fullCalcOnLoad` is calculated on opening.

Saving a macro-enabled file as `.docx`, `.xlsx` or `.pptx` drops its macros, as Office does, and the Compatibility Checker says so. A file saved under the extension it came with keeps them, also when it holds macros behind a `.xlsx`, `.docx` or `.pptx` name.

## Fonts

Documents keep the font names they ask for. Kiln bundles metric-compatible faces for Calibri, Cambria, Arial, Times New Roman and Courier New: Carlito, Caladea and Liberation. Noto covers other scripts, and is fetched when a document first needs it.

## Before you send a file

**File ▸ Info ▸ Check for Issues ▸ Compatibility Checker** lists anything in the document that the target format cannot hold. **Inspect Document** beside it finds comments, hidden text and properties you may not want to send. See [Options, properties and checking before you share](options.md).

## How fidelity is measured

The test suite runs a corpus of Word, Excel, PowerPoint and OpenDocument files through Kiln: real-world files from the test data of Apache POI and LibreOffice, and generated ones. Each file is read, written back without edits and read again, opened in LibreOffice, and scored on structure, text and pages rendered by LibreOffice at 60 dpi. Nothing here is rendered in Microsoft Office. The scores are on the [landing page](../index.html) and in [the report](../qa/fidelity.md).
