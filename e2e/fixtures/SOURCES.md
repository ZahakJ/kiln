# End-to-end fixtures

| File | Source | Licence |
|---|---|---|
| `macros.docm` | `test-data/document/SimpleMacro.docm` of Apache POI at commit `ebf602f1bc748f9ec9b29553070ff62170e3243b` (the commit `tools/fetch-corpus.sh` pins), unchanged: a Word document with a real VBA project (`word/vbaProject.bin`, an OLE compound file) | Apache-2.0 |
| `macros.xlsm` | Made by `python3 -I e2e/fixtures/make_xlsm.py .` from `fixtures/xlsx/manual/minimal.xlsx` (Kiln's own) and the VBA project of `macros.docm`: the workbook's main content type becomes `macroEnabled.main+xml` and `xl/vbaProject.bin` is that OLE compound file | Apache-2.0 (the VBA project), AGPL-3.0-or-later (the rest) |
| `macros.pptm` | Made by `python3 -I e2e/fixtures/make_deck_fixtures.py .` from `fixtures/pptx/features/transitions.pptx` (Kiln's own) and the VBA project of `macros.docm`: the presentation's main content type becomes `macroEnabled.main+xml` and `ppt/vbaProject.bin` is that OLE compound file | Apache-2.0 (the VBA project), AGPL-3.0-or-later (the rest) |
| `show.ppsx` | Made by the same script from the same presentation, its main content type the slide show one (`slideshow.main+xml`) | AGPL-3.0-or-later |
| `kiln-clip.webm` | Made with FFmpeg: `ffmpeg -f lavfi -i testsrc=size=320x180:rate=10 -t 1.5 -c:v libvpx -b:v 80k -pix_fmt yuv420p kiln-clip.webm` (FFmpeg's test pattern, 1.5 s, VP8) | AGPL-3.0-or-later |
| `bell.ogg` | Made with FFmpeg: `ffmpeg -f lavfi -i sine=frequency=440:duration=1 -c:a libvorbis -q:a 2 bell.ogg` (a 440 Hz tone, 1 s, Vorbis) | AGPL-3.0-or-later |
| `media/*.docx`, `*.xlsx`, `*.pptx` | The site's sample documents: the bundled templates (`assets/templates/`, `tools/templates/`), saved with `kiln-cli convert` under the names the screenshots show | AGPL-3.0-or-later |

sha256 of the files: see `sha256sum -c` below.

```
fd591958fcf5322f72c0a740e9606309c949254bda4c3d9bd966481ddf220563  macros.docm
1104f4117f8afe8b168de39f582d3c6d277dd4befb8fc78c69ca20aec2ae7a82  macros.xlsm
6b8cb01c24ea052b975b9784c754ab15b92d4755392fd951d4e676f2bb2ab1e2  macros.pptm
7520d8f24ff8eb98fb61668200db08a83992e8fc1fa8a744601b8f9615934dc0  show.ppsx
79d442056adc1b27b5fc7e84c57b6225b6abafb70c2886fa2deb014a6a4c6278  kiln-clip.webm
4ab18881d389750cfc8773606f92bb194c9609a486663661f3567daa127d3ba0  bell.ogg
```
