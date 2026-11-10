# SPDX-License-Identifier: AGPL-3.0-or-later
"""Builds e2e/fixtures/macros.pptm and e2e/fixtures/show.ppsx from fixtures/pptx/features/transitions.pptx
(Kiln's own): the presentation's main content type becomes the macro-enabled one, with the VBA
project (an OLE compound file) of e2e/fixtures/macros.docm as ppt/vbaProject.bin, or the slide
show one."""
import io, sys, zipfile
root = sys.argv[1]
MAIN = "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"
src = zipfile.ZipFile(f"{root}/fixtures/pptx/features/transitions.pptx")
vba = zipfile.ZipFile(f"{root}/e2e/fixtures/macros.docm").read("word/vbaProject.bin")


def build(main, extra):
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for info in src.infolist():
            body = src.read(info.filename)
            if info.filename == "[Content_Types].xml":
                t = body.decode().replace(MAIN, main)
                if extra:
                    t = t.replace("</Types>", '<Default Extension="bin" ContentType="application/vnd.ms-office.vbaProject"/></Types>')
                body = t.encode()
            if extra and info.filename == "ppt/_rels/presentation.xml.rels":
                t = body.decode().replace("</Relationships>", '<Relationship Id="rIdVba1" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/></Relationships>')
                body = t.encode()
            zi = zipfile.ZipInfo(info.filename, date_time=(1980, 1, 1, 0, 0, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, body)
        if extra:
            zi = zipfile.ZipInfo("ppt/vbaProject.bin", date_time=(1980, 1, 1, 0, 0, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, vba)
    return out.getvalue()


open(f"{root}/e2e/fixtures/macros.pptm", "wb").write(build("application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml", True))
open(f"{root}/e2e/fixtures/show.ppsx", "wb").write(build("application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml", False))
