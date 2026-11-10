# SPDX-License-Identifier: AGPL-3.0-or-later
"""Checks the workbooks Kiln saves against the OOXML schemas and LibreOffice.

For every given .xlsx/.xlsm fixture this writes two files with the Kiln CLI: the workbook saved
back unchanged (`kiln-cli convert`), and an edited save made through the control channel (cells,
a formula, a bold number format, a chart of the data moved to a chart sheet). Each XML part of
both is then validated against the ISO/IEC 29500-4:2016 transitional schemas after Markup
Compatibility processing (ISO/IEC 29500-3: attributes and elements of ignorable namespaces are
dropped, `mc:AlternateContent` keeps its `mc:Fallback`), as Excel reads them; and LibreOffice
converts every file, which fails on a package it cannot open.

The schemas are not part of Kiln: pass the folder that holds `sml.xsd` and its companions.

    python3 -I tools/xlsx-schema-check.py --kiln target/debug/kiln-cli \\
        --schemas /path/to/ISO-IEC29500-4_2016 --out target/scratch/xsd fixtures/xlsx/**/*.xlsx

Exits 0 when every part validates and LibreOffice opens every file.
"""
import argparse
import json
import os
import subprocess
import sys
import zipfile

from lxml import etree

MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
SML = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
SCHEMAS = {
    SML: "sml.xsd",
    "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing": "dml-spreadsheetDrawing.xsd",
    "http://schemas.openxmlformats.org/drawingml/2006/chart": "dml-chart.xsd",
    "http://schemas.openxmlformats.org/drawingml/2006/main": "dml-main.xsd",
    "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties": "shared-documentPropertiesExtended.xsd",
}


def mce(root):
    """Markup Compatibility processing with no extension namespace understood."""
    def ignorable(el):
        names = set()
        node = el
        while node is not None:
            v = node.get(f"{{{MC}}}Ignorable")
            if v:
                for p in v.split():
                    ns = node.nsmap.get(p)
                    if ns:
                        names.add(ns)
            node = node.getparent()
        return names

    for alt in list(root.iter(f"{{{MC}}}AlternateContent")):
        parent = alt.getparent()
        fallback = alt.find(f"{{{MC}}}Fallback")
        index = parent.index(alt)
        parent.remove(alt)
        if fallback is not None:
            for i, child in enumerate(list(fallback)):
                parent.insert(index + i, child)
    known = set(SCHEMAS) | {
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        "http://www.w3.org/XML/1998/namespace",
    }
    for el in list(root.iter()):
        if not isinstance(el.tag, str):
            continue
        # `xml:space` is XML's own attribute (Excel writes it on every `t`); the schemas leave it
        # to XML and do not list it.
        el.attrib.pop("{http://www.w3.org/XML/1998/namespace}space", None)
        drop = ignorable(el)
        for name in list(el.attrib):
            if name.startswith("{"):
                ns = name[1:].split("}")[0]
                if ns == MC or (ns in drop and ns not in known):
                    del el.attrib[name]
        ns = etree.QName(el).namespace
        if ns in drop and ns not in known and el.getparent() is not None:
            el.getparent().remove(el)
    return root


def validators(folder):
    cache = {}

    def get(ns):
        if ns not in SCHEMAS:
            return None
        if ns not in cache:
            cache[ns] = etree.XMLSchema(etree.parse(os.path.join(folder, SCHEMAS[ns])))
        return cache[ns]

    return get


def part_errors(data, schema):
    """The schema errors of one part (`None` for a part no schema covers)."""
    root = etree.fromstring(data)
    xsd = schema(etree.QName(root).namespace)
    if xsd is None:
        return None
    mce(root)
    return [] if xsd.validate(root) else [e.message for e in xsd.error_log]


def check_package(path, schema, source=None):
    """(parts validated, [problems], [inherited]) of one package. A part whose errors the source
    file's part of the same name has as well is reported as inherited, not as a problem."""
    problems, inherited, validated = [], [], 0
    original = zipfile.ZipFile(source) if source else None
    with zipfile.ZipFile(path) as z:
        names = z.namelist()
        if "[Content_Types].xml" not in names:
            problems.append("no [Content_Types].xml")
        for name in names:
            if not (name.endswith(".xml") or name.endswith(".rels")):
                continue
            try:
                root = etree.fromstring(z.read(name))
            except etree.XMLSyntaxError as e:
                problems.append(f"{name}: not well-formed: {e}")
                continue
            ns = etree.QName(root).namespace
            xsd = schema(ns)
            if xsd is None:
                continue
            errors = part_errors(z.read(name), schema) or []
            validated += 1
            if not errors:
                continue
            before = None
            if original is not None and name in original.namelist():
                try:
                    before = part_errors(original.read(name), schema)
                except etree.XMLSyntaxError:
                    before = None
            if before and set(errors) <= set(before):
                inherited.append(f"{name}: {errors[0]}")
            else:
                problems.extend(f"{name}: {e}" for e in errors[:5])
    return validated, problems, inherited


def control(kiln, requests):
    lines = "\n".join(json.dumps(r) for r in requests) + "\n"
    out = subprocess.run([kiln, "control"], input=lines, capture_output=True, text=True, timeout=300)
    replies = [json.loads(line) for line in out.stdout.splitlines() if line.strip()]
    bad = [r for r in replies if isinstance(r, dict) and r.get("ok") is False]
    return replies, bad


def edited_save(kiln, src, dst):
    """Opens `src`, edits it and saves it as `dst` through the control channel."""
    grid = lambda doc, c: {"execute": {"doc": doc, "cmd": {"Grid": c}}}
    suite = lambda doc, c: {"execute": {"doc": doc, "cmd": {"Suite": c}}}
    # A fresh control session numbers its first document 1.
    doc = 1
    requests = [{"id": 1, "open": {"path": os.path.abspath(src)}}]
    # New rows far below the data: text, numbers, a formula, bold, then a chart of them.
    cells = [("Kiln", "12.5", "7", "=B9001*C9001")]
    for r, row in enumerate(cells):
        for c, text in enumerate(row):
            requests.append(grid(doc, {"SelectCell": {"row": 9000 + r, "col": c, "extend": False, "add": False}}))
            requests.append(grid(doc, {"BeginEdit": {"text": text}}))
            requests.append(grid(doc, {"CommitEdit": {"advance": None}}))
    requests.append(grid(doc, {"SelectRange": {"range": {"r0": 9000, "c0": 0, "r1": 9000, "c1": 3}, "add": False}}))
    requests.append(suite(doc, {"Edit": "Bold"}))
    requests.append(suite(doc, {"ChartInsert": {"kind": "ColumnClustered"}}))
    requests.append(suite(doc, {"ChartSetElement": {"element": "StandardErrorBars", "on": True}}))
    requests.append(suite(doc, {"ChartFormatArea": {"area": "ChartArea", "fill": None, "line": None,
                                                    "line_width": None, "shadow": True, "rounded": True}}))
    requests.append(suite(doc, {"ChartMove": {"to": {"NewSheet": {"name": "Kiln chart"}}}}))
    requests.append({"save": {"doc": doc, "path": os.path.abspath(dst), "format": "Xlsm" if dst.endswith(".xlsm") else "Xlsx"}})
    replies, bad = control(kiln, requests)
    return [f"edit: {b}" for b in bad]


def libreoffice_opens(path, outdir):
    os.makedirs(outdir, exist_ok=True)
    r = subprocess.run(["soffice", "--headless", "--calc", "--convert-to", "csv", "--outdir", outdir, path],
                       capture_output=True, text=True, timeout=180)
    out = os.path.join(outdir, os.path.splitext(os.path.basename(path))[0] + ".csv")
    return r.returncode == 0 and os.path.exists(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--kiln", required=True)
    ap.add_argument("--schemas", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("files", nargs="+")
    a = ap.parse_args()
    schema = validators(a.schemas)
    os.makedirs(a.out, exist_ok=True)
    total, failed, parts = 0, 0, 0
    for src in a.files:
        stem = os.path.splitext(os.path.basename(src))[0]
        ext = os.path.splitext(src)[1]
        for kind in ("resaved", "edited"):
            dst = os.path.join(a.out, f"{stem}.{kind}{ext}")
            if kind == "resaved":
                r = subprocess.run([a.kiln, "convert", src, dst], capture_output=True, text=True)
                problems = [] if r.returncode == 0 else [f"convert: {r.stderr.strip()}"]
            else:
                problems = edited_save(a.kiln, src, dst)
            refused = [p for p in problems if "grid.protected" in p]
            if refused:
                # A protected workbook refuses the edits, as Excel does: nothing to check.
                print(f"skip {dst}: the workbook is protected")
                continue
            inherited = []
            if not problems and os.path.exists(dst):
                n, problems, inherited = check_package(dst, schema, src)
                parts += n
                if not libreoffice_opens(dst, os.path.join(a.out, "lo")):
                    problems.append("LibreOffice did not open it")
            elif not os.path.exists(dst):
                problems.append("not written")
            total += 1
            if problems:
                failed += 1
                print(f"FAIL {dst}")
                for p in problems:
                    print(f"  {p}")
            else:
                print(f"ok   {dst}")
            for i in inherited:
                print(f"  kept as read (the source has it too): {i}")
    print(f"{total - failed}/{total} files valid ({parts} XML parts schema-validated); LibreOffice opened every valid file")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
