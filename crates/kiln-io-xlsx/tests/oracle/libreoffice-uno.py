#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""LibreOffice as an oracle for what Kiln writes: opens an .xlsx through UNO and prints JSON.

    python3 libreoffice-uno.py <book.xlsx> [password]

For every sheet: its name, whether it is protected, and with a password whether that password
unprotects it; for every DataPilot (PivotTable): its name, its output after LibreOffice refreshes it
from the source, as text. Sparkline groups are not exposed through UNO. Needs LibreOffice's Python
(`import uno`); uses a private profile under $TMPDIR.
"""

import json
import os
import subprocess
import sys
import tempfile
import time

import uno
from com.sun.star.beans import PropertyValue


def main() -> None:
    path = os.path.abspath(sys.argv[1])
    password = sys.argv[2] if len(sys.argv) > 2 else None
    port = 2100 + os.getpid() % 500
    profile = tempfile.mkdtemp(prefix="kiln-lo-uno-")
    proc = subprocess.Popen(
        ["soffice", "--headless", "--invisible", "--norestore", "--nologo",
         f"-env:UserInstallation=file://{profile}", f"--accept=socket,host=localhost,port={port};urp;"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
    try:
        local = uno.getComponentContext()
        resolver = local.ServiceManager.createInstanceWithContext("com.sun.star.bridge.UnoUrlResolver", local)
        ctx = None
        for _ in range(120):
            try:
                ctx = resolver.resolve(f"uno:socket,host=localhost,port={port};urp;StarOffice.ComponentContext")
                break
            except Exception:
                time.sleep(0.5)
        if ctx is None:
            raise SystemExit("LibreOffice did not start")
        desktop = ctx.ServiceManager.createInstanceWithContext("com.sun.star.frame.Desktop", ctx)
        doc = desktop.loadComponentFromURL(uno.systemPathToFileUrl(path), "_blank", 0,
                                           (PropertyValue("Hidden", 0, True, 0),))
        out = {"sheets": []}
        for i in range(doc.Sheets.Count):
            sh = doc.Sheets.getByIndex(i)
            entry = {"name": sh.Name, "protected": bool(sh.isProtected()), "pivots": []}
            if password is not None and sh.isProtected():
                try:
                    sh.unprotect(password)
                except Exception:
                    pass
                entry["unprotected"] = not sh.isProtected()
            tables = sh.getDataPilotTables()
            for name in tables.getElementNames():
                t = tables.getByName(name)
                t.refresh()
                r = t.getOutputRange()
                cells = []
                for row in range(r.StartRow, r.EndRow + 1):
                    cells.append([sh.getCellByPosition(c, row).getString() for c in range(r.StartColumn, r.EndColumn + 1)])
                entry["pivots"].append({"name": name, "cells": cells})
            out["sheets"].append(entry)
        doc.close(True)
        print(json.dumps(out))
    finally:
        try:
            desktop.terminate()
        except Exception:
            pass
        try:
            proc.wait(timeout=20)
        except Exception:
            proc.kill()


if __name__ == "__main__":
    main()
