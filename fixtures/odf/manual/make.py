#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Writes the hand-made OpenDocument fixtures of this directory.

Each file exercises what LibreOffice's own output does not: a package without styles.xml or a
manifest, foreign namespace prefixes, legacy `oooc:` formulas, matrix formulas, repeated rows
and cells, covered cells, outline groups, rotated shapes and groups, and a chart object in a text
document (the chart of `../features/charts.odp`, slide 1).

    python3 fixtures/odf/manual/make.py
"""
import os
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))

NS = {
    "office": "urn:oasis:names:tc:opendocument:xmlns:office:1.0",
    "style": "urn:oasis:names:tc:opendocument:xmlns:style:1.0",
    "text": "urn:oasis:names:tc:opendocument:xmlns:text:1.0",
    "table": "urn:oasis:names:tc:opendocument:xmlns:table:1.0",
    "draw": "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0",
    "fo": "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0",
    "svg": "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0",
    "presentation": "urn:oasis:names:tc:opendocument:xmlns:presentation:1.0",
    "number": "urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0",
    "xlink": "http://www.w3.org/1999/xlink",
    "of": "urn:oasis:names:tc:opendocument:xmlns:of:1.2",
}


def decls(prefixes):
    return " ".join(f'xmlns:{p}="{NS[p]}"' for p in prefixes)


def package(name, mime, files, manifest=True, entries_extra=()):
    path = os.path.join(HERE, name)
    with zipfile.ZipFile(path, "w") as z:
        info = zipfile.ZipInfo("mimetype", date_time=(1980, 1, 1, 0, 0, 0))
        z.writestr(info, mime, compress_type=zipfile.ZIP_STORED)
        entries = []
        for fname, data in files:
            info = zipfile.ZipInfo(fname, date_time=(1980, 1, 1, 0, 0, 0))
            z.writestr(info, data, compress_type=zipfile.ZIP_DEFLATED)
            entries.append(fname)
        if manifest:
            m = ['<?xml version="1.0" encoding="UTF-8"?>',
                 f'<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">',
                 f'<manifest:file-entry manifest:full-path="/" manifest:media-type="{mime}"/>']
            for e, media in entries_extra:
                m.append(f'<manifest:file-entry manifest:full-path="{e}" manifest:media-type="{media}"/>')
            for e in entries:
                m.append(f'<manifest:file-entry manifest:full-path="{e}" manifest:media-type="text/xml"/>')
            m.append("</manifest:manifest>")
            info = zipfile.ZipInfo("META-INF/manifest.xml", date_time=(1980, 1, 1, 0, 0, 0))
            z.writestr(info, "".join(m), compress_type=zipfile.ZIP_DEFLATED)
    print("wrote", path)


# A text document with nothing but content.xml: default styles everywhere.
TEXT = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {decls(["office", "style", "text", "table", "fo"])} office:version="1.3">
<office:automatic-styles>
<style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold" fo:color="#c00000"/></style:style>
<style:style style:name="P1" style:family="paragraph"><style:paragraph-properties fo:text-align="center" fo:margin-bottom="0.2in"/></style:style>
</office:automatic-styles>
<office:body><office:text>
<text:h text:outline-level="1">A minimal document</text:h>
<text:p text:style-name="P1">Centred, with <text:span text:style-name="T1">bold red</text:span> text,<text:s text:c="3"/>three spaces,<text:tab/>a tab and a<text:line-break/>line break.</text:p>
<text:list><text:list-item><text:p>First item</text:p><text:list><text:list-item><text:p>Nested item</text:p></text:list-item></text:list></text:list-item><text:list-item><text:p>Second item</text:p></text:list-item></text:list>
<table:table table:name="T"><table:table-column table:number-columns-repeated="3"/>
<table:table-row><table:table-cell table:number-columns-spanned="2"><text:p>Spanned</text:p></table:table-cell><table:covered-table-cell/><table:table-cell><text:p>C</text:p></table:table-cell></table:table-row>
<table:table-row><table:table-cell><text:p>1</text:p></table:table-cell><table:table-cell><text:p>2</text:p></table:table-cell><table:table-cell><text:p>3</text:p></table:table-cell></table:table-row>
</table:table>
<text:p>A footnote<text:note text:id="n1" text:note-class="footnote"><text:note-citation>1</text:note-citation><text:note-body><text:p>The note.</text:p></text:note-body></text:note> and a <text:a xlink:type="simple" xlink:href="https://example.org/" xmlns:xlink="http://www.w3.org/1999/xlink">link</text:a>.</text:p>
</office:text></office:body></office:document-content>"""

# The same kind of content with foreign prefixes and a default namespace.
FOREIGN = """<?xml version="1.0" encoding="UTF-8"?>
<document-content xmlns="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:t="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:s="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:f="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:x="urn:example:unknown" xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" version="1.3">
<automatic-styles><s:style s:name="T1" s:family="text"><s:text-properties f:font-style="italic"/></s:style></automatic-styles>
<body><text><t:h t:outline-level="2">Foreign prefixes</t:h><t:p>Plain and <t:span t:style-name="T1">italic</t:span> text<x:kept> kept</x:kept><x:hidden office:process-content="false"> hidden</x:hidden>.</t:p></text></body>
</document-content>"""

SHEET = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {decls(["office", "style", "text", "table", "fo", "number", "of"])} office:version="1.3">
<office:automatic-styles>
<number:date-style style:name="N1"><number:year number:style="long"/><number:text>-</number:text><number:month number:style="long"/><number:text>-</number:text><number:day number:style="long"/></number:date-style>
<number:time-style style:name="N2" number:truncate-on-overflow="false"><number:hours/><number:text>:</number:text><number:minutes number:style="long"/></number:time-style>
<number:number-style style:name="N3P0" style:volatile="true"><number:number number:decimal-places="2" number:min-integer-digits="1" number:grouping="true"/></number:number-style>
<number:number-style style:name="N3"><style:text-properties fo:color="#ff0000"/><number:text>-</number:text><number:number number:decimal-places="2" number:min-integer-digits="1" number:grouping="true"/><style:map style:condition="value()&gt;=0" style:apply-style-name="N3P0"/></number:number-style>
<style:style style:name="ce1" style:family="table-cell" style:data-style-name="N1"/>
<style:style style:name="ce2" style:family="table-cell" style:data-style-name="N2"/>
<style:style style:name="ce3" style:family="table-cell" style:data-style-name="N3"><style:table-cell-properties fo:background-color="#ffff00"/></style:style>
<style:style style:name="co1" style:family="table-column"><style:table-column-properties style:column-width="1.5in"/></style:style>
<style:style style:name="ro1" style:family="table-row"><style:table-row-properties style:row-height="0.4in" style:use-optimal-row-height="false"/></style:style>
</office:automatic-styles>
<office:body><office:spreadsheet>
<table:table table:name="Data">
<table:table-column table:style-name="co1"/><table:table-column table:number-columns-repeated="3"/>
<table:table-row table:style-name="ro1"><table:table-cell office:value-type="string"><text:p>Item</text:p></table:table-cell><table:table-cell office:value-type="string"><text:p>Qty</text:p></table:table-cell><table:table-cell office:value-type="string"><text:p>Price</text:p></table:table-cell><table:table-cell office:value-type="string"><text:p>Total</text:p></table:table-cell></table:table-row>
<table:table-row table:number-rows-repeated="3"><table:table-cell office:value-type="string"><text:p>Same</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="2"/><table:table-cell table:style-name="ce3" office:value-type="float" office:value="-1.5"/><table:table-cell/></table:table-row>
<table:table-row><table:table-cell table:number-columns-spanned="2" office:value-type="string"><text:p>Merged</text:p></table:table-cell><table:covered-table-cell/><table:table-cell table:formula="of:=SUM([.C2:.C4])" office:value-type="float" office:value="-4.5"/><table:table-cell table:formula="oooc:=[.B2]*[.C2]" office:value-type="float" office:value="-3"/></table:table-row>
<table:table-row-group><table:table-row><table:table-cell table:style-name="ce1" office:value-type="date" office:date-value="2026-10-17"/><table:table-cell table:style-name="ce2" office:value-type="time" office:time-value="PT36H30M00S"/><table:table-cell table:formula="of:=SUMPRODUCT([.B2:.B4]*[.C2:.C4])" table:number-matrix-columns-spanned="1" table:number-matrix-rows-spanned="1" office:value-type="float" office:value="-9"/><table:table-cell office:value-type="boolean" office:boolean-value="true"/></table:table-row></table:table-row-group>
<table:table-row><table:table-cell table:formula="of:=IF([.D6];&quot;yes&quot;;&quot;no&quot;)" office:value-type="string" office:string-value="yes"><text:p>yes</text:p></table:table-cell><table:table-cell table:formula="of:=COM.MICROSOFT.TEXTJOIN(&quot;, &quot;;TRUE();[.A2:.A4])" office:value-type="string" office:string-value="Same, Same, Same"/><table:table-cell office:value-type="string"><text:p>two<text:s text:c="2"/>spaces</text:p></table:table-cell></table:table-row>
<table:table-row table:number-rows-repeated="1048569"><table:table-cell table:number-columns-repeated="1024"/></table:table-row>
</table:table>
<table:named-expressions><table:named-range table:name="Prices" table:base-cell-address="$Data.$A$1" table:cell-range-address="$Data.$C$2:.$C$4"/></table:named-expressions>
</office:spreadsheet></office:body></office:document-content>"""

DECK_STYLES = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {decls(["office", "style", "text", "draw", "fo", "svg", "presentation"])} office:version="1.3">
<office:styles>
<style:style style:name="Plain-title" style:family="presentation"><style:graphic-properties draw:fill="none" draw:stroke="none"/><style:text-properties fo:font-size="36pt" fo:color="#1f3864"/></style:style>
<style:style style:name="Plain-outline1" style:family="presentation"><style:graphic-properties draw:fill="none" draw:stroke="none"/><style:text-properties fo:font-size="24pt"/></style:style>
</office:styles>
<office:automatic-styles>
<style:page-layout style:name="PM1"><style:page-layout-properties fo:page-width="25.4cm" fo:page-height="19.05cm" fo:margin-top="0cm" fo:margin-bottom="0cm" fo:margin-left="0cm" fo:margin-right="0cm"/></style:page-layout>
<style:style style:name="Mdp1" style:family="drawing-page"><style:drawing-page-properties draw:fill="solid" draw:fill-color="#f2f2f2"/></style:style>
</office:automatic-styles>
<office:master-styles>
<style:master-page style:name="Plain" style:page-layout-name="PM1" draw:style-name="Mdp1">
<draw:frame presentation:style-name="Plain-title" svg:x="1cm" svg:y="1cm" svg:width="23.4cm" svg:height="3cm" presentation:class="title"><draw:text-box/></draw:frame>
<draw:frame presentation:style-name="Plain-outline1" svg:x="1cm" svg:y="5cm" svg:width="23.4cm" svg:height="12cm" presentation:class="outline"><draw:text-box/></draw:frame>
</style:master-page>
</office:master-styles></office:document-styles>"""

DECK = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {decls(["office", "style", "text", "draw", "fo", "svg", "presentation"])} office:version="1.3">
<office:automatic-styles>
<style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="solid" draw:fill-color="#4472c4" draw:stroke="solid" svg:stroke-color="#2f528f" svg:stroke-width="0.05cm"/></style:style>
<style:style style:name="gr2" style:family="graphic"><style:graphic-properties draw:stroke="solid" svg:stroke-color="#c00000" svg:stroke-width="0.08cm" draw:marker-end="Arrow"/></style:style>
<style:style style:name="dp1" style:family="drawing-page"><style:drawing-page-properties smil:type="pushWipe" smil:subtype="fromBottom" xmlns:smil="urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0"/></style:style>
</office:automatic-styles>
<office:body><office:presentation>
<draw:page draw:name="One" draw:master-page-name="Plain" draw:style-name="dp1">
<draw:frame presentation:style-name="Plain-title" presentation:class="title" svg:x="1cm" svg:y="1cm" svg:width="23.4cm" svg:height="3cm"><draw:text-box><text:p>Hand-made slide</text:p></draw:text-box></draw:frame>
<draw:frame presentation:style-name="Plain-outline1" presentation:class="outline" presentation:user-transformed="true" svg:x="1cm" svg:y="5cm" svg:width="11cm" svg:height="12cm"><draw:text-box><text:list><text:list-item><text:p>Point one</text:p></text:list-item><text:list-item><text:list><text:list-item><text:p>Sub point</text:p></text:list-item></text:list></text:list-item></text:list></draw:text-box></draw:frame>
<draw:custom-shape draw:style-name="gr1" draw:name="Rotated" svg:width="5cm" svg:height="3cm" draw:transform="rotate (0.5235987756) translate (13cm 8cm)"><text:p>Turned</text:p><draw:enhanced-geometry svg:viewBox="0 0 21600 21600" draw:type="ooxml-roundRect" draw:modifiers="5000" draw:enhanced-path="M 0 3600 C 0 1600 1600 0 3600 0 L 18000 0 C 20000 0 21600 1600 21600 3600 L 21600 18000 C 21600 20000 20000 21600 18000 21600 L 3600 21600 C 1600 21600 0 20000 0 18000 Z N"/></draw:custom-shape>
<draw:line draw:style-name="gr2" draw:name="Arrow" svg:x1="13cm" svg:y1="15cm" svg:x2="23cm" svg:y2="16cm"/>
<draw:g draw:name="Pair"><draw:rect draw:style-name="gr1" draw:name="Box" svg:x="14cm" svg:y="2cm" svg:width="2cm" svg:height="1cm"/><draw:ellipse draw:style-name="gr1" svg:x="17cm" svg:y="2cm" svg:width="2cm" svg:height="1cm"/></draw:g>
<presentation:notes><draw:frame presentation:class="notes" svg:x="2cm" svg:y="14cm" svg:width="17cm" svg:height="10cm"><draw:text-box><text:p>Speaker notes for the hand-made slide.</text:p></draw:text-box></draw:frame></presentation:notes>
</draw:page>
</office:presentation></office:body></office:document-content>"""

package("minimal.odt", "application/vnd.oasis.opendocument.text", [("content.xml", TEXT)], manifest=False)
package("foreign-prefixes.odt", "application/vnd.oasis.opendocument.text", [("content.xml", FOREIGN)])
package("structures.ods", "application/vnd.oasis.opendocument.spreadsheet", [("content.xml", SHEET)])
package("shapes.odp", "application/vnd.oasis.opendocument.presentation", [("content.xml", DECK), ("styles.xml", DECK_STYLES)])

# A text document with a chart object (LibreOffice's own chart of charts.odp, with its local table).
with zipfile.ZipFile(os.path.join(HERE, "..", "features", "charts.odp")) as src:
    CHART = src.read("Object 1/content.xml").decode("utf-8")
    CHART_STYLES = src.read("Object 1/styles.xml").decode("utf-8")
CHART_TEXT = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {decls(["office", "style", "text", "draw", "svg", "xlink"])} office:version="1.3">
<office:body><office:text>
<text:h text:outline-level="1">Quarterly sales</text:h>
<text:p>The chart below comes from the presentation fixture.</text:p>
<text:p><draw:frame draw:name="Sales" text:anchor-type="as-char" svg:width="14cm" svg:height="7cm"><draw:object xlink:href="./Object 1" xlink:type="simple" xlink:show="embed" xlink:actuate="onLoad"/></draw:frame></text:p>
</office:text></office:body></office:document-content>"""
package("chart.odt", "application/vnd.oasis.opendocument.text",
        [("content.xml", CHART_TEXT), ("Object 1/content.xml", CHART), ("Object 1/styles.xml", CHART_STYLES)],
        entries_extra=[("Object 1/", "application/vnd.oasis.opendocument.chart")])
