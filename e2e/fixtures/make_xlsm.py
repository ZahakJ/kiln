# SPDX-License-Identifier: AGPL-3.0-or-later
"""Builds e2e/fixtures/macros.xlsm: fixtures/xlsx/manual/minimal.xlsx made macro-enabled with the
VBA project (an OLE compound file) of e2e/fixtures/macros.docm."""
import io, sys, zipfile
root = sys.argv[1]
docm = zipfile.ZipFile(f"{root}/e2e/fixtures/macros.docm")
vba = docm.read("word/vbaProject.bin")
src = zipfile.ZipFile(f"{root}/fixtures/xlsx/manual/minimal.xlsx")
out = io.BytesIO()
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for info in src.infolist():
        body = src.read(info.filename)
        if info.filename == "[Content_Types].xml":
            t = body.decode()
            t = t.replace("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
                          "application/vnd.ms-excel.sheet.macroEnabled.main+xml")
            t = t.replace("</Types>", '<Default Extension="bin" ContentType="application/vnd.ms-office.vbaProject"/></Types>')
            body = t.encode()
        if info.filename == "xl/_rels/workbook.xml.rels":
            t = body.decode()
            t = t.replace("</Relationships>", '<Relationship Id="rIdVba1" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/></Relationships>')
            body = t.encode()
        zi = zipfile.ZipInfo(info.filename, date_time=(1980, 1, 1, 0, 0, 0))
        zi.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(zi, body)
    zi = zipfile.ZipInfo("xl/vbaProject.bin", date_time=(1980, 1, 1, 0, 0, 0))
    zi.compress_type = zipfile.ZIP_DEFLATED
    z.writestr(zi, vba)
open(f"{root}/e2e/fixtures/macros.xlsm", "wb").write(out.getvalue())
