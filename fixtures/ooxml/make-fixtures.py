#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Build the hand-made OPC edge-case packages in fixtures/ooxml/.

    python3 -I fixtures/ooxml/make-fixtures.py

Each package isolates one thing real-world files do that the Office writers never do (or do
rarely); crates/kiln-ooxml/tests/ooxml_fixtures.rs says what each one must do. Output is
deterministic (fixed timestamps, fixed entry order); the committed files are the reference.
Standard library only.
"""

from __future__ import annotations

import sys
import zipfile
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent
EPOCH = (1980, 1, 1, 0, 0, 0)

CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
OD = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\r\n'
DOCX_MAIN = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
DOCX_STYLES = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"
DOCX_SETTINGS = "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"


def content_types(defaults: list[tuple[str, str]], overrides: list[tuple[str, str]]) -> bytes:
    body = "".join(f'<Default Extension="{e}" ContentType="{t}"/>' for e, t in defaults)
    body += "".join(f'<Override PartName="{n}" ContentType="{t}"/>' for n, t in overrides)
    return f'{DECL}<Types xmlns="{CT_NS}">{body}</Types>'.encode()


def rels(entries: list[tuple[str, str, str] | tuple[str, str, str, str]]) -> bytes:
    body = ""
    for e in entries:
        rid, rtype, target = e[0], e[1], e[2]
        mode = f' TargetMode="{e[3]}"' if len(e) > 3 else ""
        body += f'<Relationship Id="{rid}" Type="{rtype}" Target="{target}"{mode}/>'
    return f'{DECL}<Relationships xmlns="{REL_NS}">{body}</Relationships>'.encode()


def document(text: str) -> bytes:
    return (
        f'{DECL}<w:document xmlns:w="{W_NS}"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p>'
        f"</w:body></w:document>"
    ).encode()


DEFAULTS = [
    ("rels", "application/vnd.openxmlformats-package.relationships+xml"),
    ("xml", "application/xml"),
]
PNG = bytes.fromhex(
    "89504e470d0a1a0a0000000d4948445200000001000000010806000000"
    "1f15c4890000000d49444154789c6360f8cf00000301010018dd8db00000000049454e44ae426082"
)


def write_zip(name: str, entries: list[tuple[str, bytes] | tuple[str, bytes, int]]) -> None:
    path = OUT / name
    with zipfile.ZipFile(path, "w") as z:
        for e in entries:
            arcname, data = e[0], e[1]
            method = e[2] if len(e) > 2 else zipfile.ZIP_DEFLATED
            info = zipfile.ZipInfo(arcname, date_time=EPOCH)
            info.compress_type = method
            info.create_system = 0
            info.external_attr = 0o644 << 16 if not arcname.endswith("/") else 0o755 << 16
            if method == zipfile.ZIP_DEFLATED:
                z.writestr(info, data, compresslevel=6)
            else:
                z.writestr(info, data)
    print(f"wrote {path.relative_to(OUT.parent.parent)}")


def minimal_docx_entries(text: str) -> list[tuple[str, bytes]]:
    return [
        ("[Content_Types].xml", content_types(DEFAULTS, [("/word/document.xml", DOCX_MAIN)])),
        ("_rels/.rels", rels([("rId1", f"{OD}/officeDocument", "word/document.xml")])),
        ("word/document.xml", document(text)),
    ]


def missing_content_types() -> None:
    entries = minimal_docx_entries("no content types")[1:]
    write_zip("missing-content-types.docx", entries)


def absolute_rels() -> None:
    write_zip(
        "absolute-rels.docx",
        [
            (
                "[Content_Types].xml",
                content_types(
                    DEFAULTS + [("png", "image/png")],
                    [("/word/document.xml", DOCX_MAIN), ("/word/styles.xml", DOCX_STYLES)],
                ),
            ),
            # An absolute package relationship target.
            ("_rels/.rels", rels([("rId1", f"{OD}/officeDocument", "/word/document.xml")])),
            ("word/document.xml", document("absolute and odd relationship targets")),
            (
                "word/_rels/document.xml.rels",
                rels(
                    [
                        ("rId1", f"{OD}/styles", "/word/styles.xml"),
                        ("rId2", f"{OD}/image", "./media/../media/image1.png"),
                        ("rId3", f"{OD}/image", "media/my%20image.png"),
                        ("rId4", f"{OD}/image", "media\\image3.png"),
                        ("rId5", f"{OD}/hyperlink", "https://example.org/a?b=c#d", "External"),
                        ("rId6", f"{OD}/image", "media/missing.png"),
                    ]
                ),
            ),
            ("word/styles.xml", f'{DECL}<w:styles xmlns:w="{W_NS}"/>'.encode()),
            ("word/media/image1.png", PNG, zipfile.ZIP_STORED),
            ("word/media/my image.png", PNG, zipfile.ZIP_STORED),
            ("word/media/image3.png", PNG, zipfile.ZIP_STORED),
        ],
    )


def encodings() -> None:
    utf16 = f'<?xml version="1.0" encoding="UTF-16" standalone="yes"?>\r\n<w:document xmlns:w="{W_NS}"><w:body><w:p><w:r><w:t>UTF-16 «ünïcödé» ✓</w:t></w:r></w:p></w:body></w:document>'
    cp1252 = (
        f'<?xml version="1.0" encoding="windows-1252"?>\r\n<w:styles xmlns:w="{W_NS}">'
        f'<w:style w:styleId="Caf\xe9"><w:name w:val="Caf\xe9 \x80uro"/></w:style></w:styles>'
    ).encode("latin-1")
    bom8 = b"\xef\xbb\xbf" + f'{DECL}<w:settings xmlns:w="{W_NS}"><w:zoom w:percent="100"/></w:settings>'.encode()
    utf16be = f'<w:footnotes xmlns:w="{W_NS}"><w:footnote w:id="1"/></w:footnotes>'.encode("utf-16-be")
    write_zip(
        "encodings.docx",
        [
            (
                "[Content_Types].xml",
                content_types(
                    DEFAULTS,
                    [
                        ("/word/document.xml", DOCX_MAIN),
                        ("/word/styles.xml", DOCX_STYLES),
                        ("/word/settings.xml", DOCX_SETTINGS),
                        (
                            "/word/footnotes.xml",
                            "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
                        ),
                    ],
                ),
            ),
            ("_rels/.rels", rels([("rId1", f"{OD}/officeDocument", "word/document.xml")])),
            ("word/document.xml", b"\xff\xfe" + utf16.encode("utf-16-le")),
            (
                "word/_rels/document.xml.rels",
                rels(
                    [
                        ("rId1", f"{OD}/styles", "styles.xml"),
                        ("rId2", f"{OD}/settings", "settings.xml"),
                        ("rId3", f"{OD}/footnotes", "footnotes.xml"),
                    ]
                ),
            ),
            ("word/styles.xml", cp1252),
            ("word/settings.xml", bom8),
            ("word/footnotes.xml", utf16be),
        ],
    )


def case_and_order() -> None:
    xlsx_main = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
    sheet = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"
    x_ns = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    r_ns = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    write_zip(
        "case-and-order.xlsx",
        [
            ("xl/", b"", zipfile.ZIP_STORED),
            (
                "xl/Workbook.xml",
                f'{DECL}<workbook xmlns="{x_ns}" xmlns:r="{r_ns}"><sheets><sheet name="One" sheetId="1" r:id="rId1"/></sheets></workbook>'.encode(),
            ),
            # The content types sit in the middle and use other casings than the entries.
            (
                "[Content_Types].xml",
                content_types(
                    [("RELS", DEFAULTS[0][1]), ("XML", "application/xml")],
                    [("/XL/workbook.XML", xlsx_main), ("/xl/worksheets/SHEET1.xml", sheet)],
                ),
            ),
            ("xl/worksheets/", b"", zipfile.ZIP_STORED),
            (
                "xl/worksheets/sheet1.xml",
                f'{DECL}<worksheet xmlns="{x_ns}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>case</t></is></c></row></sheetData></worksheet>'.encode(),
                zipfile.ZIP_STORED,
            ),
            ("XL/_rels/workbook.xml.rels", rels([("rId1", f"{OD}/worksheet", "worksheets/sheet1.xml")])),
            # The package relationships come last.
            ("_rels/.rels", rels([("rId1", f"{OD}/officeDocument", "xl/workbook.xml")])),
        ],
    )


def strict() -> None:
    s_od = "http://purl.oclc.org/ooxml/officeDocument/relationships"
    x_ns = "http://purl.oclc.org/ooxml/spreadsheetml/main"
    r_ns = "http://purl.oclc.org/ooxml/officeDocument/relationships"
    xlsx_main = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
    sheet = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"
    write_zip(
        "strict.xlsx",
        [
            (
                "[Content_Types].xml",
                content_types(
                    DEFAULTS,
                    [("/xl/workbook.xml", xlsx_main), ("/xl/worksheets/sheet1.xml", sheet)],
                ),
            ),
            ("_rels/.rels", rels([("rId1", f"{s_od}/officeDocument", "xl/workbook.xml")])),
            (
                "xl/workbook.xml",
                f'{DECL}<workbook xmlns="{x_ns}" xmlns:r="{r_ns}" conformance="strict"><sheets><sheet name="Strict" sheetId="1" r:id="rId1"/></sheets></workbook>'.encode(),
            ),
            ("xl/_rels/workbook.xml.rels", rels([("rId1", f"{s_od}/worksheet", "worksheets/sheet1.xml")])),
            (
                "xl/worksheets/sheet1.xml",
                f'{DECL}<worksheet xmlns="{x_ns}"><sheetData/></worksheet>'.encode(),
            ),
        ],
    )


def deep_nesting() -> None:
    depth = 5000
    body = "<w:tbl><w:tr><w:tc>" * (depth // 3) + "</w:tc></w:tr></w:tbl>" * (depth // 3)
    doc = f'{DECL}<w:document xmlns:w="{W_NS}"><w:body>{body}</w:body></w:document>'.encode()
    entries = minimal_docx_entries("")
    entries[2] = ("word/document.xml", doc)
    write_zip("deep-nesting.docx", entries)


def billion_laughs() -> None:
    doc = (
        '<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol "lol">'
        '<!ENTITY lol1 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">'
        '<!ENTITY lol2 "&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;">'
        '<!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;">]>'
        f'<w:document xmlns:w="{W_NS}"><w:body><w:p><w:r><w:t>&lol3;</w:t></w:r></w:p></w:body></w:document>'
    ).encode()
    entries = minimal_docx_entries("")
    entries[2] = ("word/document.xml", doc)
    write_zip("billion-laughs.docx", entries)


def corrupt_entry() -> None:
    name = "corrupt-entry.docx"
    entries = minimal_docx_entries("this entry's compressed bytes are damaged " * 8)
    write_zip(name, entries)
    path = OUT / name
    data = bytearray(path.read_bytes())
    # Damage the compressed data of word/document.xml (just after its local header).
    marker = b"word/document.xml"
    at = data.index(marker) + len(marker)
    data[at + 4] ^= 0xFF
    data[at + 5] ^= 0xFF
    path.write_bytes(bytes(data))
    # The damage must not be in the central directory.
    with zipfile.ZipFile(path) as z:
        assert z.namelist()[2] == "word/document.xml"


def encrypted() -> None:
    # The first sector of an OLE compound file, as a password-protected Office file starts.
    header = bytearray(512)
    header[0:8] = bytes.fromhex("d0cf11e0a1b11ae1")
    header[0x18:0x1A] = (0x3E).to_bytes(2, "little")  # minor version
    header[0x1A:0x1C] = (0x03).to_bytes(2, "little")  # major version 3
    header[0x1C:0x1E] = (0xFFFE).to_bytes(2, "little")  # byte order
    header[0x1E:0x20] = (9).to_bytes(2, "little")  # sector shift
    header[0x20:0x22] = (6).to_bytes(2, "little")  # mini sector shift
    path = OUT / "encrypted.docx"
    path.write_bytes(bytes(header) + b"EncryptionInfo" + bytes(64))
    print(f"wrote {path.relative_to(OUT.parent.parent)}")


def main() -> int:
    if zlib.ZLIB_VERSION is None:  # pragma: no cover - zlib is always present in CPython
        return 1
    missing_content_types()
    absolute_rels()
    encodings()
    case_and_order()
    strict()
    deep_nesting()
    billion_laughs()
    corrupt_entry()
    encrypted()
    return 0


if __name__ == "__main__":
    sys.exit(main())
