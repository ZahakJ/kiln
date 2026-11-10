#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Writes the PivotTable test data and the aggregates Kiln must reproduce.

The records (data.json) are generated with a fixed seed. For each case of CASES the expected
values (expected.json) are computed here independently of Kiln, with Python's statistics module:
every group of the row fields crossed with every group of the column fields, at every level
(the items, the subtotals and the grand totals), for each value field. Keys are the items'
labels joined with " / " ("" for a grand total).

    python3 -I crates/kiln-sheet/src/pivot/testdata/expected.py
"""

import json
import math
import random
import statistics
from itertools import product
from pathlib import Path

HERE = Path(__file__).resolve().parent

HEADERS = ["Region", "Product", "Quarter", "Rep", "Units", "Price", "Note"]


def records():
    rnd = random.Random(74)
    regions = ["East", "West", "North", "South"]
    products = ["Apple", "Banana", "Cherry", "Date", "Elderberry"]
    reps = ["Ana", "Ben", "Caro", "Dev", "Eli", "Fay"]
    rows = []
    for i in range(400):
        region = rnd.choice(regions)
        prod = rnd.choice(products)
        quarter = rnd.randint(1, 4)
        rep = rnd.choice(reps)
        units = rnd.randint(-5, 60)
        price = round(rnd.uniform(0.5, 9.5), 2)
        note = rnd.choice(["", "", "rush", "gift", "n/a"])
        # A few odd cells: blank units, text units, a blank region.
        if i % 37 == 0:
            units = None
        if i % 53 == 0:
            units = "none"
        if i % 61 == 0:
            region = None
        rows.append([region, prod, quarter, rep, units, price, note or None])
    return rows


def label(v):
    if v is None or v == "":
        return "(blank)"
    if isinstance(v, int):
        return str(v)
    return str(v)


def summarise(values, function):
    nums = [v for v in values if isinstance(v, (int, float)) and not isinstance(v, bool)]
    count = sum(1 for v in values if v is not None and v != "")
    if function == "sum":
        return float(sum(nums))
    if function == "count":
        return float(count)
    if function == "countNums":
        return float(len(nums))
    if function == "average":
        return statistics.fmean(nums) if nums else "#DIV/0!"
    if function == "max":
        return float(max(nums)) if nums else 0.0
    if function == "min":
        return float(min(nums)) if nums else 0.0
    if function == "product":
        return float(math.prod(nums)) if nums else 0.0
    if function == "stdDev":
        return statistics.stdev(nums) if len(nums) > 1 else "#DIV/0!"
    if function == "stdDevp":
        return statistics.pstdev(nums) if nums else "#DIV/0!"
    if function == "var":
        return statistics.variance(nums) if len(nums) > 1 else "#DIV/0!"
    if function == "varp":
        return statistics.pvariance(nums) if nums else "#DIV/0!"
    raise ValueError(function)


CASES = [
    {"name": "region-sum", "rows": ["Region"], "cols": [], "values": [["Units", "sum"]]},
    {"name": "region-product", "rows": ["Region"], "cols": ["Product"], "values": [["Units", "sum"]]},
    {
        "name": "two-levels-every-function",
        "rows": ["Region", "Rep"],
        "cols": [],
        "values": [["Units", f] for f in
                   ["sum", "count", "average", "max", "min", "countNums", "stdDev", "stdDevp", "var", "varp"]],
    },
    {"name": "quarter-by-product-price", "rows": ["Quarter"], "cols": ["Product", "Region"],
     "values": [["Price", "average"], ["Price", "max"]]},
    {"name": "text-field-count", "rows": ["Note"], "cols": ["Quarter"], "values": [["Rep", "count"]]},
    {"name": "product", "rows": ["Product"], "cols": [], "values": [["Price", "product"]],
     "where": {"Region": "East", "Quarter": 1}},
]


def expected(rows, case):
    idx = {h: i for i, h in enumerate(HEADERS)}
    where = case.get("where", {})
    data = [r for r in rows if all(r[idx[k]] == v for k, v in where.items())]
    out = {}
    rf, cf = case["rows"], case["cols"]
    for vi, (field, function) in enumerate(case["values"]):
        for rd, cd in product(range(len(rf) + 1), range(len(cf) + 1)):
            groups = {}
            for r in data:
                rk = " / ".join(label(r[idx[f]]) for f in rf[:rd])
                ck = " / ".join(label(r[idx[f]]) for f in cf[:cd])
                groups.setdefault((rk, ck), []).append(r[idx[field]])
            for (rk, ck), vals in groups.items():
                out[f"{vi}|{rk}|{ck}"] = summarise(vals, function)
    return out


def main():
    rows = records()
    (HERE / "data.json").write_text(json.dumps({"headers": HEADERS, "rows": rows}) + "\n")
    cases = []
    for c in CASES:
        cases.append({**c, "expected": expected(rows, c)})
    (HERE / "expected.json").write_text(json.dumps(cases, indent=0, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
