# The command line

`kiln-cli` drives the same core as the Kiln apps from a terminal, a script or an AI agent: it looks inside
Office files, checks that they survive Kiln untouched, renders pages to images, evaluates formulas and offers a
JSON control channel for everything else. The Linux packages install it as `kiln`; from a source checkout run
it with `cargo run -p kiln-cli --`.

```sh
kiln-cli inspect report.docx                      # parts, content types, sizes, relationships
kiln-cli inspect report.docx --xml word/document.xml
kiln-cli repack book.xlsx /tmp/book.xlsx          # open and save without edits
kiln-cli render notes.txt --png out/page-%03d.png --dpi 150
kiln-cli eval '=SUM(1,2,3)/4'                     # 1.5
kiln-cli control < requests.jsonl                 # JSON requests in, JSON responses out
kiln-cli fidelity --ci                            # round-trip fidelity of the test corpus
kiln-cli version
```

Every command exits with **0** when it succeeds, **1** when it ran and failed (a missing file, a part that did
not survive, a page that does not exist) and **2** when the command line itself is wrong (running `kiln-cli` with no
command is one); `kiln-cli help` prints the summary and exits 0. Output piped into a command that stops reading
early (`kiln-cli inspect a.docx --xml /word/document.xml | head`) ends quietly.

What works today: `inspect`, `repack` and `fidelity` handle any `.docx`, `.xlsx` or `.pptx` package;
`render` and `control` open the document kinds whose app is in the build, which is currently the plain-text
scratch document (`.txt`, `.md`). Word, Excel and PowerPoint files join `render` and `control` as their
readers land, with no change to the commands. `eval` knows the engine's core functions (`SUM`, `IF`, `AND`,
`OR`, `NOT`, `TRUE`, `FALSE` and every operator) until the full function library arrives.

## inspect

```text
kiln-cli inspect <file> [--xml <part>] [--json]
```

Lists every part of an Office package (the zip inside a `.docx`, `.xlsx` or `.pptx`) with its uncompressed
size and content type, then the relationships of each part (`rId`, type, resolved target, `(external)` for
links outside the package). `--xml <part>` prints one part's XML indented, with the prefixes exactly as the file
uses them (`word/document.xml` and `/word/document.xml` both work, and so does `[Content_Types].xml`).
`--json` prints the listing as JSON (`main`, `parts`, `relationships`) for scripts.

Password-protected files and old binary `.doc`/`.xls`/`.ppt` files are recognised and refused with a message
saying so; so are archives that would inflate beyond Kiln's safety limits.

## repack

```text
kiln-cli repack <in> <out>
```

Opens the package and saves it again without changing anything, then compares the two part by part and prints
`identical parts: N/N`. Parts Kiln does not understand are copied byte for byte, so N/N is the expected
result for every file; anything else is listed (`changed:`, `missing:`, `added:`) and the command exits 1.
`[Content_Types].xml` is read into a table of content types when the package opens, so it is checked separately:
the line `[Content_Types].xml: the same declared content types` says the copy declares the same types, and
`the output file is byte-identical to the input` (when printed) says the whole file, that part included, came
out byte for byte.

## render

```text
kiln-cli render <file> --png <out.png | out/page-%03d.png | dir/> [--page N[,M…]] [--dpi 96] [--dark]
```

Opens a document and writes its pages as PNG images, painted by the same renderer as the apps' canvas and PDF
export. `--png` takes a file name for a single page, a pattern with `%d` or `%03d` for the page number, or a
directory (pages are then `page-001.png`, `page-002.png`, …). `--page` picks pages (numbered from 1),
`--dpi` sets the resolution (24–600; 96 is one pixel per CSS pixel, so a Letter page is 816 × 1056) and
`--dark` uses the dark theme's page colours.

```text
kiln-cli render --scratch <file.txt> --png <out.png> [--width 1000] [--height 900] [--zoom 1] [--dpr 1]
                [--scroll-x <px>] [--scroll-y 0] [--theme light|dark]
```

The second form renders what the editor window would show for a viewport — its size, zoom, device pixel ratio
and scroll position, caret included — and is what the golden-image tests use.

## eval

```text
kiln-cli eval '=formula'
```

Evaluates one formula on an empty workbook and prints the value as a cell shows it: numbers in Excel's
general format, text as is, `TRUE`/`FALSE`, error codes such as `#DIV/0!`, arrays as `{1,2;3,4}`. The leading
`=` is optional; quote the formula for the shell. A formula error is a result (exit 0); a formula that does
not parse is a failure (exit 1).

## control

```text
kiln-cli control
```

A JSON control channel over standard input and output, for scripts and agents. Each input line is one request:
a JSON object with one verb and an optional `"id"`, which the response repeats. Each request gets exactly one
response line, `{"id":…,"ok":true,"result":…}` or `{"id":…,"ok":false,"error":{"kind":…,"message":…}}`,
followed by the events of documents you subscribed to as `{"event":…}` lines. A malformed line gets an error
response and the channel carries on; it ends at the end of the input. Arguments a verb does not take, and
viewport fields that do not exist, are errors rather than being ignored, so a typo such as `"width"` for
`"width_px"` is reported instead of silently doing nothing.

| Request | Result |
|---|---|
| `{"new":{"kind":"scratch"}}` — kinds `scratch`, `write`, `grid`, `deck`, `notes` | the document summary (its `doc` id, title, revision, undo state) |
| `{"open":{"path":"notes.txt","read_only":false}}` | the document summary |
| `{"execute":{"doc":1,"cmd":{"Write":{"InsertText":{"text":"hi"}}}}}` — leave out `doc` for session commands such as `{"Suite":"Ping"}` | the command result (`Applied`, `Unchanged` or `Pending`) and the summary |
| `{"query":{"doc":1,"q":"Paragraphs"}}` — also `Summary`, `Selection`, `Status`, `A11y`, `SelectedText`, `Documents` (no `doc`), and `Text` for the whole plain text | the answer, e.g. `{"Text":"hi"}` |
| `{"viewport":{"doc":1,"viewport":{"width_px":1000,"height_px":800,"zoom":1.5}}}` — fields left out keep their defaults | `null` |
| `{"frame":{"doc":1}}` — `"full":true` returns the whole display-list delta | a summary of the next frame (revision, extent, tiles, operations) |
| `{"render":{"doc":1,"png":"out/p-%d.png","page":1,"dpi":96,"dark":false}}` — `page` may be a list; omitted renders every page | `{"written":[…]}` |
| `{"save":{"doc":1,"path":"out.txt"}}` — `format` (e.g. `"Txt"`) is optional; without `path` the document is saved where it came from | the command result |
| `{"subscribe":{"doc":1}}`, or `{"subscribe":{}}` for every document; `unsubscribe` the same way | `null` |
| `{"close":{"doc":1}}` | the command result |
| `{"ping":{}}` | the core's version, protocol version and platform |

Commands, queries, viewports and events use the same JSON form as the apps' own protocol (`kiln-protocol`),
so every command the ribbon can run is available here. For example:

```sh
printf '%s\n' \
  '{"new":{"kind":"scratch"}}' \
  '{"execute":{"doc":1,"cmd":{"Write":{"InsertText":{"text":"Hello from a script"}}}}}' \
  '{"save":{"doc":1,"path":"hello.txt"}}' \
  '{"query":{"doc":1,"q":"Text"}}' | kiln-cli control
```

## fidelity

```text
kiln-cli fidelity [--ci] [--format docx|xlsx|pptx]… [--report <file.md>] [--no-oracle] [--large]
```

For developers: reads every file of the test corpus (`fixtures/`), writes it back without edits, reads it again
and scores what survived — structure, text, cell values, formulas, the parts Kiln did not touch, whether
LibreOffice opens the result and how its pages compare with the original's — against the thresholds in
`parity/corpus.json`. Every package also goes through the `repack` check. `--report` writes the Markdown report
(`cargo xtask fidelity` writes `docs/qa/fidelity.md`). `--ci` uses only the committed files; without it the
downloaded real-world files in `fixtures/external/` are scored too. LibreOffice is optional: without it the
visual comparison is skipped and reported as not measured.
