#!/usr/bin/env python3
"""Writes the right-to-left Word fixtures of this folder (see README.md).

    python3 -I fixtures/docx/rtl/make.py

The packages are written part by part (WordprocessingML as Word writes it: `w:bidi` paragraphs,
`w:rtl` runs, `w:bidiVisual` tables, numbering in right-to-left paragraphs) with fixed zip
timestamps, so running the script again gives byte-identical files.
"""

from __future__ import annotations

import io
import zipfile
from pathlib import Path
from xml.sax.saxutils import escape

HERE = Path(__file__).resolve().parent
ZIP_DATE = (1980, 1, 1, 0, 0, 0)
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>
{numbering}<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>
</Types>"""

NUMBERING_CT = (
    '<Override PartName="/word/numbering.xml" '
    'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>\n'
)

PACKAGE_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>
</Relationships>"""

DOCUMENT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>
{numbering}</Relationships>"""

NUMBERING_REL = (
    '<Relationship Id="rId3" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" '
    'Target="numbering.xml"/>\n'
)

CORE = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<dc:title>{title}</dc:title>
<dc:creator>Kiln Fixtures</dc:creator>
<dc:language>ar-SA</dc:language>
<dcterms:created xsi:type="dcterms:W3CDTF">2026-01-15T09:30:00Z</dcterms:created>
<dcterms:modified xsi:type="dcterms:W3CDTF">2026-01-15T09:30:00Z</dcterms:modified>
</cp:coreProperties>"""

APP = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties">
<Application>Kiln Fixtures</Application>
</Properties>"""

STYLES = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="{W}">
<w:docDefaults>
<w:rPrDefault><w:rPr>
<w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:eastAsia="Times New Roman" w:cs="Noto Naskh Arabic"/>
<w:sz w:val="24"/><w:szCs w:val="28"/>
<w:lang w:val="en-US" w:eastAsia="en-US" w:bidi="ar-SA"/>
</w:rPr></w:rPrDefault>
<w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault>
</w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/>
<w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/><w:outlineLvl w:val="0"/></w:pPr>
<w:rPr><w:b/><w:bCs/><w:color w:val="1F3864"/><w:sz w:val="32"/><w:szCs w:val="36"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/><w:basedOn w:val="Normal"/><w:qFormat/>
<w:pPr><w:ind w:left="720"/><w:contextualSpacing/></w:pPr></w:style>
<w:style w:type="character" w:default="1" w:styleId="DefaultParagraphFont"><w:name w:val="Default Paragraph Font"/><w:uiPriority w:val="1"/><w:semiHidden/></w:style>
<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:semiHidden/>
<w:tblPr><w:tblInd w:w="0" w:type="dxa"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>
<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:basedOn w:val="TableNormal"/>
<w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>
<w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders></w:tblPr></w:style>
</w:styles>"""

SETTINGS = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="{W}">
<w:defaultTabStop w:val="720"/>
<w:characterSpacingControl w:val="doNotCompress"/>
<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>
<w:themeFontLang w:val="en-US" w:bidi="ar-SA"/>
</w:settings>"""

SECT = (
    '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>'
    '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/>'
    '<w:cols w:space="708"/><w:bidi/><w:docGrid w:linePitch="360"/></w:sectPr>'
)


def run(text: str, *, rtl: bool = False, bold: bool = False, italic: bool = False, lang: str | None = None) -> str:
    """A run; `rtl` marks right-to-left text (`w:rtl`, complex-script formatting)."""
    props = []
    if bold:
        props += ["<w:b/>", "<w:bCs/>"]
    if italic:
        props += ["<w:i/>", "<w:iCs/>"]
    if rtl:
        props.append("<w:rtl/>")
    if lang:
        props.append(f'<w:lang w:val="{lang}"/>')
    rpr = f"<w:rPr>{''.join(props)}</w:rPr>" if props else ""
    return f'<w:r>{rpr}<w:t xml:space="preserve">{escape(text)}</w:t></w:r>'


def para(*runs: str, bidi: bool = True, style: str | None = None, jc: str | None = None, extra: str = "") -> str:
    """A paragraph. In a `w:bidi` paragraph `jc` start/end follow Word: `left` is the start (the
    right side) and `right` the end."""
    props = []
    if style:
        props.append(f'<w:pStyle w:val="{style}"/>')
    props.append(extra)
    if bidi:
        props.append("<w:bidi/>")
    if jc:
        props.append(f'<w:jc w:val="{jc}"/>')
    return f"<w:p><w:pPr>{''.join(props)}</w:pPr>{''.join(runs)}</w:p>"


def ar(text: str, **kw) -> str:
    return run(text, rtl=True, **kw)


def document(body: list[str]) -> str:
    return (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{"".join(body)}{SECT}</w:body></w:document>'
    )


def save(name: str, title: str, body: list[str], numbering: str | None = None) -> None:
    parts = [
        ("[Content_Types].xml", CONTENT_TYPES.format(numbering=NUMBERING_CT if numbering else "")),
        ("_rels/.rels", PACKAGE_RELS),
        ("word/document.xml", document(body)),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS.format(numbering=NUMBERING_REL if numbering else "")),
        ("word/styles.xml", STYLES),
        ("word/settings.xml", SETTINGS),
    ]
    if numbering:
        parts.append(("word/numbering.xml", numbering))
    parts += [("docProps/core.xml", CORE.format(title=escape(title))), ("docProps/app.xml", APP)]
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for part, text in parts:
            info = zipfile.ZipInfo(part, date_time=ZIP_DATE)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o600 << 16
            z.writestr(info, text.encode("utf-8"), compresslevel=6)
    (HERE / f"{name}.docx").write_bytes(buf.getvalue())


def arabic_digits() -> None:
    save(
        "arabic-digits",
        "Arabic text with Latin words and two kinds of digits",
        [
            para(ar("الأرقام والنصوص المختلطة"), style="Heading1"),
            para(ar("بلغت المبيعات ١٢٣٬٤٥٦ ريالاً في الربع الأول من عام ٢٠٢٦، بزيادة قدرها ١٥٪ عن العام الماضي.")),
            para(
                ar("يدعم برنامج "),
                run("Kiln Write"),
                ar(" ملفات "),
                run("DOCX"),
                ar(" و"),
                run("ODT"),
                ar("، ورقم الإصدار "),
                run("0.1.0"),
                ar(" متاح منذ 15 يناير 2026."),
            ),
            para(ar("اَلْعِلْمُ نُورٌ وَالْجَهْلُ ظَلَامٌ، وَمَنْ طَلَبَ الْعُلَا سَهِرَ اللَّيَالِي.")),
            para(ar("قال المدير (في الاجتماع): «لا بد من الالتزام بالمواعيد»، ثم أضاف: [الساعة ٩:٣٠ صباحاً]."),),
            para(ar("اتصل على الرقم +966 11 234 5678 بين الساعة 9:00 و17:30، أو راسلنا على "), run("info@example.com"), ar(".")),
            para(ar("النسبة 45% من 1,250 مستخدماً، والمتوسط ٣٫٧٥ من ٥.")),
        ],
    )


JUSTIFIED_1 = (
    "صناعة الفخار من أقدم الحرف التي عرفها الإنسان، إذ كان الخزّافون يشكّلون الطين بأيديهم ثم يجففونه "
    "في الشمس قبل أن يحرقوه في الأفران. ومع مرور القرون تطورت الأفران وصارت درجات الحرارة أعلى وأكثر "
    "انتظاماً، فظهر الخزف الصيني والبورسلين الذي انتشر في أسواق العالم عبر طرق التجارة القديمة. وما زالت "
    "هذه الحرفة حيّة في مدن كثيرة، يتوارثها الأبناء عن الآباء ويضيفون إليها من خبرتهم وذوقهم."
)
JUSTIFIED_2 = (
    "تعتمد جودة الطلاء الزجاجي على تركيب المواد ودرجة الحرارة ومدة الحرق، ولذلك يحتفظ كل خزّاف بدفتر "
    "يسجّل فيه ما جرّبه من خلطات ونتائجها، حتى يستطيع أن يكرر ما نجح منها ويتجنب ما فشل، ويشارك "
    "تلاميذه ما تعلّمه في سنوات العمل الطويلة."
)


def arabic_justified() -> None:
    save(
        "arabic-justified",
        "Justified Arabic paragraphs",
        [
            para(ar("الفخار والأفران"), style="Heading1"),
            para(ar(JUSTIFIED_1), jc="both"),
            para(ar(JUSTIFIED_2), jc="both"),
            para(ar("فقرة قصيرة مضبوطة من الجهتين."), jc="both"),
            para(ar("فقرة في وسط الصفحة."), jc="center"),
            para(ar("فقرة محاذاة إلى نهاية السطر (اليسار)."), jc="right"),
        ],
    )


def cell(text: str, width: int, *, rtl: bool = True, shade: str | None = None, span: int = 1, jc: str | None = None,
         bold: bool = False) -> str:
    props = [f'<w:tcW w:w="{width}" w:type="dxa"/>']
    if span > 1:
        props.append(f'<w:gridSpan w:val="{span}"/>')
    if shade:
        props.append(f'<w:shd w:val="clear" w:color="auto" w:fill="{shade}"/>')
    content = ar(text, bold=bold) if rtl else run(text, bold=bold)
    return f"<w:tc><w:tcPr>{''.join(props)}</w:tcPr>{para(content, jc=jc)}</w:tc>"


def bidi_table() -> None:
    widths = [3200, 1500, 1800, 2400]
    grid = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)

    def row(values: list[str], header: bool = False) -> str:
        cells = []
        for i, (v, w) in enumerate(zip(values, widths)):
            numeric = i > 0 and not header
            cells.append(cell(v, w, shade="D9E2F3" if header else None, jc="right" if numeric else None,
                              bold=header))
        trpr = "<w:trPr><w:tblHeader/></w:trPr>" if header else ""
        return f"<w:tr>{trpr}{''.join(cells)}</w:tr>"

    rows = [
        row(["المنتج", "الكمية", "سعر الوحدة", "المجموع"], header=True),
        row(["أكواب فخارية", "12", "4.50", "54.00"]),
        row(["صحون مزخرفة", "8", "7.25", "58.00"]),
        row(["مزهرية كبيرة", "2", "19.90", "39.80"]),
    ]
    total = (
        "<w:tr>"
        + cell("المجموع الكلي", sum(widths[:3]), span=3, shade="F2F2F2", bold=True)
        + cell("151.80", widths[3], shade="F2F2F2", jc="right", bold=True)
        + "</w:tr>"
    )
    table = (
        '<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:bidiVisual/>'
        f'<w:tblW w:w="{sum(widths)}" w:type="dxa"/><w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0" '
        'w:firstColumn="1" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/></w:tblPr>'
        f"<w:tblGrid>{grid}</w:tblGrid>{''.join(rows)}{total}</w:tbl>"
    )
    save(
        "bidi-table",
        "A right-to-left table",
        [
            para(ar("فاتورة المشغل"), style="Heading1"),
            para(ar("الأعمدة من اليمين إلى اليسار: المنتج أولاً ثم الكمية والسعر والمجموع.")),
            table,
            para(ar("تُدفع الفاتورة خلال ٣٠ يوماً.")),
        ],
    )


def mixed_runs() -> None:
    save(
        "mixed-runs",
        "Left-to-right and right-to-left runs on one line",
        [
            para(run("Mixed directions"), style="Heading1", bidi=False),
            para(run("The word "), ar("كيلن"), run(" means kiln, and "), ar("٢٠٢٦"), run(" is the year 2026."),
                 bidi=False),
            para(ar("يستخدم الفريق "), run("Rust"), ar(" و"), run("TypeScript"), ar(" لبناء الواجهة، مع مكتبة "),
                 run("parley"), ar(" للتخطيط.")),
            para(ar("النص "), ar("العريض", bold=True), ar(" والكلمة "), run("italic", italic=True),
                 ar(" في سطر واحد.")),
            para(run("Quote: "), ar("«مرحباً بالعالم»"), run(" — from the release notes (v0.1)."), bidi=False),
            para(ar("الإصدار "), run("Kiln 0.1 (beta)"), ar(" يعمل على "), run("Linux, Windows, macOS"),
                 ar(" وأندرويد.")),
        ],
    )


NUMBERING = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="{W}">
<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="hybridMultilevel"/>
<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/>
<w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl>
<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="arabicAlpha"/><w:lvlText w:val="%2)"/><w:lvlJc w:val="left"/>
<w:pPr><w:ind w:left="1440" w:hanging="360"/></w:pPr></w:lvl>
</w:abstractNum>
<w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="hybridMultilevel"/>
<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/><w:lvlJc w:val="left"/>
<w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr><w:rPr><w:rFonts w:ascii="Liberation Serif" w:hAnsi="Liberation Serif" w:cs="Liberation Serif"/></w:rPr></w:lvl>
</w:abstractNum>
<w:abstractNum w:abstractNumId="2"><w:multiLevelType w:val="hybridMultilevel"/>
<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="arabicAbjad"/><w:lvlText w:val="%1-"/><w:lvlJc w:val="left"/>
<w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl>
</w:abstractNum>
<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
<w:num w:numId="3"><w:abstractNumId w:val="2"/></w:num>
</w:numbering>"""


def item(text: str, num: int, level: int = 0) -> str:
    return para(ar(text), style="ListParagraph", extra=f'<w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{num}"/></w:numPr>')


def rtl_lists() -> None:
    save(
        "rtl-lists",
        "Lists in right-to-left paragraphs",
        [
            para(ar("خطوات العمل"), style="Heading1"),
            item("تحضير الطين وعجنه جيداً.", 1),
            item("تشكيل القطعة على الدولاب.", 1),
            item("الشكل الأسطواني", 1, 1),
            item("الشكل الكروي", 1, 1),
            item("التجفيف ثم الحرق الأول في الفرن.", 1),
            para(ar("الأدوات اللازمة:")),
            item("دولاب الخزّاف", 2),
            item("أدوات التشذيب", 2),
            item("فرن كهربائي أو فرن غاز", 2),
            para(ar("ترتيب الأبجدية:")),
            item("أبجد", 3),
            item("هوز", 3),
            item("حطي", 3),
        ],
        numbering=NUMBERING,
    )


def main() -> None:
    arabic_digits()
    arabic_justified()
    bidi_table()
    mixed_runs()
    rtl_lists()


if __name__ == "__main__":
    main()
