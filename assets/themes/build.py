#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Builds the twelve bundled Kiln Deck themes (assets/themes/*.kdeck-theme).

The design of every theme lives in this file: palettes, type, grids, text styles, layout
treatments and variants. The .kdeck-theme files it writes are committed and compiled into
kiln-deck (crates/kiln-deck/src/themes/bundled.rs); a kiln-deck test fails when a file is not
what this script produces, so edit here and run:

    python3 assets/themes/build.py

The format is documented in crates/kiln-deck/src/themes/mod.rs. Colours, fills and text
properties are DrawingML, so each theme's a:theme is written unchanged into a .pptx.

The brand kit (kiln-brand/final, BRAND.md) is the inspiration, not a source of UI tokens: the
themes borrow its glazes (ember, cobalt, jade, saffron, tenmoku), its warm paper and its kiln
arch, and turn them into twelve distinct personalities.
"""

from __future__ import annotations

import pathlib
from xml.sax.saxutils import escape, quoteattr

HERE = pathlib.Path(__file__).resolve().parent
NS_A = "http://schemas.openxmlformats.org/drawingml/2006/main"
NS_KD = "urn:kiln:deck-theme:1"


# ---------------------------------------------------------------- DrawingML helpers


def attrs(**kw) -> str:
    out = []
    for k, v in kw.items():
        if v is None:
            continue
        k = k.rstrip("_")
        if isinstance(v, bool):
            v = "1" if v else "0"
        out.append(f" {k}={quoteattr(str(v))}")
    return "".join(out)


def srgb(hex_: str, alpha: int | None = None) -> str:
    if alpha is None:
        return f'<a:srgbClr val="{hex_}"/>'
    return f'<a:srgbClr val="{hex_}"><a:alpha val="{alpha}"/></a:srgbClr>'


def scheme(val: str, lum_mod: int | None = None, lum_off: int | None = None, alpha: int | None = None) -> str:
    inner = ""
    if lum_mod is not None:
        inner += f'<a:lumMod val="{lum_mod}"/>'
    if lum_off is not None:
        inner += f'<a:lumOff val="{lum_off}"/>'
    if alpha is not None:
        inner += f'<a:alpha val="{alpha}"/>'
    if not inner:
        return f'<a:schemeClr val="{val}"/>'
    return f'<a:schemeClr val="{val}">{inner}</a:schemeClr>'


def solid(color: str) -> str:
    return f"<a:solidFill>{color}</a:solidFill>"


def no_fill() -> str:
    return "<a:noFill/>"


def gradient(stops: list[tuple[int, str]], angle: int | None = None, path: str | None = None,
             focus: tuple[int, int, int, int] | None = None) -> str:
    gs = "".join(f'<a:gs pos="{p}">{c}</a:gs>' for p, c in stops)
    if path:
        l, t, r, b = focus or (50000, 50000, 50000, 50000)
        shade = f'<a:path path="{path}"><a:fillToRect l="{l}" t="{t}" r="{r}" b="{b}"/></a:path>'
    else:
        shade = f'<a:lin ang="{angle or 0}" scaled="0"/>'
    return f'<a:gradFill rotWithShape="1"><a:gsLst>{gs}</a:gsLst>{shade}</a:gradFill>'


def line(width_emu: int, color: str) -> str:
    return f'<a:ln w="{width_emu}">{solid(color)}</a:ln>'


def clr_scheme(name: str, c: dict) -> str:
    order = ["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5",
             "accent6", "hlink", "folHlink"]
    body = "".join(f"<a:{k}>{srgb(c[k])}</a:{k}>" for k in order)
    return f'<a:clrScheme name={quoteattr(name)}>{body}</a:clrScheme>'


def font_scheme(name: str, major: str, minor: str) -> str:
    def coll(tag: str, face: str) -> str:
        return (f'<a:{tag}><a:latin typeface={quoteattr(face)}/><a:ea typeface=""/>'
                f'<a:cs typeface=""/></a:{tag}>')
    return f'<a:fontScheme name={quoteattr(name)}>{coll("majorFont", major)}{coll("minorFont", minor)}</a:fontScheme>'


def def_rpr(size: float | None = None, bold: bool | None = None, italic: bool | None = None,
            color: str | None = None, font: str | None = None, caps: str | None = None,
            spc: int | None = None) -> str:
    a = attrs(sz=None if size is None else int(round(size * 100)), b=bold, i=italic, cap=caps, spc=spc)
    inner = ""
    if color:
        inner += solid(color)
    if font:
        inner += f'<a:latin typeface={quoteattr(font)}/>'
    return f"<a:defRPr{a}>{inner}</a:defRPr>" if inner else f"<a:defRPr{a}/>"


def lvl(n: int, *, algn: str | None = None, line_pct: int | None = None, before_pts: int | None = None,
        after_pts: int | None = None, bullet: str | None = None, bullet_color: str | None = None,
        bullet_none: bool = False, rpr: str = "") -> str:
    inner = ""
    if line_pct is not None:
        inner += f'<a:lnSpc><a:spcPct val="{line_pct * 1000}"/></a:lnSpc>'
    if before_pts is not None:
        inner += f'<a:spcBef><a:spcPts val="{before_pts * 100}"/></a:spcBef>'
    if after_pts is not None:
        inner += f'<a:spcAft><a:spcPts val="{after_pts * 100}"/></a:spcAft>'
    if bullet_color:
        inner += f"<a:buClr>{bullet_color}</a:buClr>"
    if bullet_none:
        inner += "<a:buNone/>"
    elif bullet:
        inner += f'<a:buFont typeface="Arial"/><a:buChar char={quoteattr(bullet)}/>'
    inner += rpr
    return f"<a:lvl{n}pPr{attrs(algn=algn)}>{inner}</a:lvl{n}pPr>"


def body_pr(**kw) -> str:
    autofit = kw.pop("autofit", None)
    inner = {"shrink": "<a:normAutofit/>", "none": "<a:noAutofit/>", "grow": "<a:spAutoFit/>"}.get(autofit, "")
    return f"<a:bodyPr{attrs(**kw)}>{inner}</a:bodyPr>" if inner else f"<a:bodyPr{attrs(**kw)}/>"


def ph(type_: str, *, n: int | None = None, rect: tuple | None = None, body: str = "", lst: str = "",
       prompt: str | None = None) -> str:
    a = attrs(type=type_, n=n)
    if rect:
        x, y, w, h = rect
        a += attrs(x=fmt(x), y=fmt(y), w=fmt(w), h=fmt(h))
    inner = body
    if lst:
        inner += f"<a:lstStyle>{lst}</a:lstStyle>"
    if prompt:
        inner += f"<kd:prompt>{escape(prompt)}</kd:prompt>"
    return f"<kd:ph{a}>{inner}</kd:ph>" if inner else f"<kd:ph{a}/>"


def shape(name: str, rect: tuple, fill: str, *, prst: str = "rect", adj: str | None = None,
          ln: str = "", rot: int | None = None, flip_h: bool | None = None, cust: str = "") -> str:
    x, y, w, h = rect
    a = attrs(name=name, prst=None if cust else prst, adj=adj, x=fmt(x), y=fmt(y), w=fmt(w), h=fmt(h),
              rot=rot, flipH=flip_h)
    return f"<kd:shape{a}>{cust}{fill}{ln}</kd:shape>"


def fmt(v: float) -> str:
    s = f"{v:.4f}".rstrip("0").rstrip(".")
    return s if s else "0"


def bg(fill: str) -> str:
    return f"<kd:background>{fill}</kd:background>"


# ---------------------------------------------------------------- text style builders


def title_style(*, size: float, color: str, bold: bool = True, caps: str | None = None,
                spc: int | None = None, algn: str = "l", line_pct: int = 90) -> str:
    return lvl(1, algn=algn, line_pct=line_pct, before_pts=0, bullet_none=True,
               rpr=def_rpr(size, bold=bold, color=color, caps=caps, spc=spc))


def body_style(*, sizes: list[float], color: str, bullets: list[str], bullet_color: str | None,
               line_pct: int = 100, before: tuple[int, int] = (10, 5)) -> str:
    out = ""
    for i in range(9):
        size = sizes[min(i, len(sizes) - 1)]
        bullet = bullets[min(i, len(bullets) - 1)]
        out += lvl(i + 1, line_pct=line_pct, before_pts=before[0] if i == 0 else before[1],
                   bullet=bullet, bullet_color=bullet_color, rpr=def_rpr(size, color=color))
    return out


def footer_style(*, size: float, color: str) -> str:
    return lvl(1, rpr=def_rpr(size, color=color))


# ---------------------------------------------------------------- the themes

# Shared geometry (fractions of the slide).
GRID = dict(marginX=0.0625, marginTop=0.075, marginBottom=0.03, titleHeight=0.16, gap=0.035,
            footerHeight=0.05, gutter=0.03)


def theme_doc(t: dict) -> str:
    grid = {**GRID, **t.get("grid", {})}
    grid_xml = "<kd:grid" + "".join(f' {k}="{fmt(v)}"' for k, v in grid.items()) + "/>"
    fmt_scheme = t.get("fmt", f'<a:fmtScheme name={quoteattr(t["name"])}/>')
    theme = (f'<a:theme name={quoteattr(t["name"])}><a:themeElements>'
             f'{clr_scheme(t["name"], t["colors"])}{font_scheme(t["name"], *t["fonts"])}{fmt_scheme}'
             f"</a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>")
    m = t["master"]
    master = (f'<kd:master{attrs(dark=t.get("dark", False))}>'
              f'{bg(m["background"])}{grid_xml}'
              f'<kd:titleStyle>{m["title"]}</kd:titleStyle>'
              f'<kd:bodyStyle>{m["body"]}</kd:bodyStyle>'
              f'<kd:otherStyle>{m.get("other", lvl(1, rpr=def_rpr(color=scheme("tx1"))))}</kd:otherStyle>'
              f'<kd:footerStyle>{m["footer"]}</kd:footerStyle>'
              + "".join(m.get("ph", [])) + "".join(m.get("shapes", [])) + "</kd:master>")
    layouts = ""
    for kind, l in t.get("layouts", {}).items():
        a = attrs(type=kind, dark=l.get("dark"), hideMaster=l.get("hide"))
        inner = (bg(l["background"]) if "background" in l else "") + "".join(l.get("shapes", [])) \
            + "".join(l.get("ph", []))
        layouts += f"<kd:layout{a}>{inner}</kd:layout>"
    variants = ""
    for v in t.get("variants", []):
        inner = clr_scheme(f'{t["name"]} {v["name"]}', v["colors"])
        if "fonts" in v:
            inner += font_scheme(f'{t["name"]} {v["name"]}', *v["fonts"])
        if "background" in v:
            inner += bg(v["background"])
        variants += f'<kd:variant{attrs(name=v["name"], dark=v.get("dark"))}>{inner}</kd:variant>'
    return ('<?xml version="1.0" encoding="UTF-8"?>\n'
            f'<kd:deckTheme xmlns:kd="{NS_KD}" xmlns:a="{NS_A}" version="1"'
            f' id={quoteattr(t["id"])} name={quoteattr(t["name"])}>\n'
            f'<kd:description>{escape(t["description"])}</kd:description>\n'
            f"{theme}\n{master}\n{layouts}\n{variants}\n</kd:deckTheme>\n")


def colors(dk1, lt1, dk2, lt2, a1, a2, a3, a4, a5, a6, hlink, fol) -> dict:
    return dict(dk1=dk1, lt1=lt1, dk2=dk2, lt2=lt2, accent1=a1, accent2=a2, accent3=a3, accent4=a4,
                accent5=a5, accent6=a6, hlink=hlink, folHlink=fol)


def with_accents(base: dict, *accents: str, **extra) -> dict:
    c = dict(base)
    for i, a in enumerate(accents):
        c[f"accent{i + 1}"] = a
    c.update(extra)
    return c


TX = scheme("tx1")
TX_SOFT = scheme("tx1", 75000, 25000)
TX_FAINT = scheme("tx1", 50000, 50000)


def kiln() -> dict:
    c = colors("2B1C14", "FFFAF3", "5A3A2A", "F1E6D6", "E05A27", "3159C6", "1D8A60", "E3A126",
               "B4462A", "6B5CA5", "2D4FB5", "7A4A33")
    glow = gradient([(0, srgb("F07A3A")), (30000, srgb("A53D18")), (70000, srgb("3A2016")),
                     (100000, srgb("221510"))], path="circle", focus=(88000, 62000, 12000, 38000))
    return dict(
        id="kiln", name="Kiln",
        description="Warm paper, tenmoku ink and an ember accent; Fraunces headlines over Figtree. The house style.",
        colors=c, fonts=("Fraunces", "Figtree"),
        master=dict(
            background=solid(scheme("bg1")),
            title=title_style(size=40, color=TX, spc=-60),
            body=body_style(sizes=[24, 20, 18, 16], color=TX_SOFT, bullets=["•", "–", "•", "–"],
                            bullet_color=scheme("accent1"), line_pct=105),
            footer=footer_style(size=10.5, color=TX_FAINT),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[shape("Ember rule", (0.0625, 0.062, 0.032, 0.0075), solid(scheme("accent1")),
                          prst="roundRect", adj="adj=50000")],
        ),
        layouts={
            "title": dict(dark=True, hide=True, background=glow, ph=[
                ph("ctrTitle", rect=(0.0625, 0.26, 0.62, 0.36), body=body_pr(anchor="b"),
                   lst=title_style(size=60, color=scheme("lt1"), spc=-120, line_pct=88)),
                ph("subTitle", rect=(0.0625, 0.65, 0.55, 0.14), body=body_pr(anchor="t"),
                   lst=lvl(1, algn="l", bullet_none=True,
                           rpr=def_rpr(20, color=scheme("lt2", 85000)))),
            ]),
            "secHead": dict(dark=True, hide=True, background=solid(scheme("accent1")), ph=[
                ph("title", rect=(0.0625, 0.30, 0.80, 0.33), body=body_pr(anchor="b"),
                   lst=title_style(size=48, color=scheme("lt1"), spc=-80)),
                ph("body", rect=(0.0625, 0.66, 0.70, 0.14),
                   lst=lvl(1, bullet_none=True, rpr=def_rpr(20, color=scheme("lt1", 90000)))),
            ]),
        },
        variants=[
            dict(name="Ember", colors=c),
            dict(name="Cobalt", colors=with_accents(c, "3159C6", "E05A27", "1D8A60", "E3A126", "6B5CA5", "B4462A")),
            dict(name="Jade", colors=with_accents(c, "1D8A60", "E3A126", "3159C6", "E05A27", "6B5CA5", "B4462A")),
            dict(name="Night", dark=True, colors=with_accents(c, "F2804E", "97AEF4", "6FD0A6", "F2C35C",
                                                              "F7976B", "B9A7E8", dk1="1E1510", lt1="F5EBDD",
                                                              dk2="2E2119", lt2="D9C6AE")),
        ],
    )


def tenmoku() -> dict:
    c = colors("1E140F", "F5EBDD", "3A2619", "D9C6AE", "FF8A3D", "F2C35C", "6FD0A6", "97AEF4",
               "F7976B", "C9A27E", "F7976B", "C9A27E")
    falloff = gradient([(0, srgb("3D271B")), (55000, srgb("24170F")), (100000, srgb("1A110C"))],
                       path="circle", focus=(0, 0, 100000, 100000))
    flame = gradient([(0, srgb("FFB547")), (45000, srgb("FF6A26")), (100000, srgb("FF6A26", 0))], angle=0)
    return dict(
        id="tenmoku", name="Tenmoku",
        description="Brown-black glaze with a warm light from the top left and a flame hairline. For evening talks and big screens.",
        colors=c, fonts=("Fraunces", "Figtree"), dark=True,
        master=dict(
            background=falloff,
            title=title_style(size=40, color=TX, spc=-60),
            body=body_style(sizes=[24, 20, 18, 16], color=scheme("tx1", 90000), bullets=["•", "–", "•", "–"],
                            bullet_color=scheme("accent1"), line_pct=105),
            footer=footer_style(size=10.5, color=scheme("tx1", 60000)),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[shape("Flame line", (0.0625, 0.245, 0.30, 0.004), flame)],
        ),
        layouts={
            "title": dict(hide=True, background=gradient(
                [(0, srgb("5A2E17")), (35000, srgb("2C1A11")), (100000, srgb("170F0B"))],
                path="circle", focus=(50000, 100000, 50000, 0)), shapes=[
                shape("Flame line", (0.35, 0.655, 0.30, 0.005), gradient(
                    [(0, srgb("FF6A26", 0)), (50000, srgb("FFB547")), (100000, srgb("FF6A26", 0))], angle=0)),
            ], ph=[
                ph("ctrTitle", rect=(0.10, 0.22, 0.80, 0.40), body=body_pr(anchor="b"),
                   lst=title_style(size=60, color=TX, spc=-120, algn="ctr", line_pct=88)),
                ph("subTitle", rect=(0.15, 0.69, 0.70, 0.12),
                   lst=lvl(1, algn="ctr", bullet_none=True, rpr=def_rpr(20, color=scheme("accent2")))),
            ]),
            "secHead": dict(hide=True, ph=[
                ph("title", rect=(0.0625, 0.30, 0.80, 0.33), body=body_pr(anchor="b"),
                   lst=title_style(size=48, color=scheme("accent2"), spc=-80)),
            ]),
        },
        variants=[
            dict(name="Ember", colors=c),
            dict(name="Jade", colors=with_accents(c, "6FD0A6", "F2C35C", "97AEF4", "FF8A3D", "F7976B", "C9A27E")),
            dict(name="Cobalt", colors=with_accents(c, "97AEF4", "F2C35C", "6FD0A6", "FF8A3D", "F7976B", "C9A27E")),
            dict(name="Smoke", colors=with_accents(c, "FF8A3D", "E8DCC8", "8FB8AA", "A6B4D8", "D9A48A", "B8A898",
                                                   dk1="17171A", dk2="2A2A30"),
                 background=gradient([(0, srgb("2E2E35")), (100000, srgb("141417"))], path="circle",
                                     focus=(0, 0, 100000, 100000))),
        ],
    )


def cobalt() -> dict:
    c = colors("1A2238", "FFFFFF", "2D4FB5", "E7ECFA", "2D4FB5", "F28C28", "1D8A60", "E3A126",
               "7B5EA7", "2AA3B8", "2D4FB5", "7B5EA7")
    return dict(
        id="cobalt", name="Cobalt",
        description="Clean white with a cobalt spine and Georgia headlines. Confident and clear for reports and pitches.",
        colors=c, fonts=("Georgia", "Calibri"),
        master=dict(
            background=solid(scheme("bg1")),
            title=title_style(size=36, color=scheme("tx2")),
            body=body_style(sizes=[22, 20, 18, 16], color=TX, bullets=["■", "–", "■", "–"],
                            bullet_color=scheme("accent1")),
            footer=footer_style(size=10, color=TX_FAINT),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[shape("Spine", (0, 0, 0.012, 1), solid(scheme("accent1")))],
        ),
        layouts={
            "title": dict(dark=True, hide=True, background=solid(scheme("accent1")), shapes=[
                shape("Rule", (0.0625, 0.64, 0.10, 0.006), solid(scheme("lt1", alpha=None)))], ph=[
                ph("ctrTitle", rect=(0.0625, 0.24, 0.78, 0.38), body=body_pr(anchor="b"),
                   lst=title_style(size=54, color=scheme("lt1"))),
                ph("subTitle", rect=(0.0625, 0.67, 0.70, 0.13),
                   lst=lvl(1, algn="l", bullet_none=True, rpr=def_rpr(20, color=scheme("lt2")))),
            ]),
            "secHead": dict(background=solid(scheme("bg2")), ph=[
                ph("title", body=body_pr(anchor="b"), lst=title_style(size=44, color=scheme("tx2"))),
            ]),
        },
        variants=[
            dict(name="Cobalt", colors=c),
            dict(name="Indigo", colors=with_accents(c, "4B3FB8", "F28C28", "1D8A60", "E3A126", "C2416B", "2AA3B8",
                                                    dk2="4B3FB8", lt2="ECEAFA")),
            dict(name="Teal", colors=with_accents(c, "117A8B", "F07E4B", "6A8F2E", "E3A126", "7B5EA7", "2D4FB5",
                                                  dk2="117A8B", lt2="E3F2F4")),
            dict(name="Midnight", dark=True, colors=with_accents(c, "7D9BF2", "F7A65A", "4FC08D", "F2C35C", "B39DE0",
                                                                 "5CC8DA", dk1="121A2E", lt1="EEF2FB", dk2="1E2946",
                                                                 lt2="A9B8E0")),
        ],
    )


def jade() -> dict:
    c = colors("17302A", "F6FAF7", "1D8A60", "E2F2EA", "1D8A60", "0F6E8C", "E3A126", "D9622B",
               "6E8B3D", "8D6CAB", "0F6E8C", "8D6CAB")
    return dict(
        id="jade", name="Jade",
        description="Calm celadon paper, a jade panel on the title slide and light Calibri headings. Quiet and modern.",
        colors=c, fonts=("Calibri Light", "Calibri"),
        master=dict(
            background=solid(scheme("bg1")),
            title=title_style(size=40, color=scheme("tx2"), bold=False),
            body=body_style(sizes=[22, 20, 18, 16], color=TX, bullets=["•", "◦", "•", "◦"],
                            bullet_color=scheme("accent1")),
            footer=footer_style(size=10, color=TX_FAINT),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[shape("Top band", (0, 0, 1, 0.014), solid(scheme("accent1")))],
        ),
        layouts={
            "title": dict(hide=True, shapes=[shape("Panel", (0, 0, 0.42, 1), solid(scheme("accent1")))], ph=[
                ph("ctrTitle", rect=(0.05, 0.20, 0.33, 0.46), body=body_pr(anchor="b"),
                   lst=title_style(size=44, color=scheme("lt1"), bold=False)),
                ph("subTitle", rect=(0.48, 0.42, 0.44, 0.20), body=body_pr(anchor="ctr"),
                   lst=lvl(1, algn="l", bullet_none=True, rpr=def_rpr(24, color=scheme("tx2")))),
            ]),
            "secHead": dict(hide=True, shapes=[shape("Band", (0, 0.60, 1, 0.40), solid(scheme("bg2")))], ph=[
                ph("title", rect=(0.0625, 0.24, 0.85, 0.34), body=body_pr(anchor="b"),
                   lst=title_style(size=44, color=scheme("tx2"), bold=False)),
            ]),
        },
        variants=[
            dict(name="Jade", colors=c),
            dict(name="Moss", colors=with_accents(c, "5E7F3A", "B5832A", "0F6E8C", "D9622B", "1D8A60", "8D6CAB",
                                                  dk2="4E6B30", lt2="ECF1E2")),
            dict(name="Lagoon", colors=with_accents(c, "0F6E8C", "1D8A60", "E3A126", "D9622B", "6E8B3D", "8D6CAB",
                                                    dk2="0F6E8C", lt2="E0EFF4")),
            dict(name="Forest", dark=True, colors=with_accents(c, "6FD0A6", "7CC4D9", "F2C35C", "F59A6B", "A9C27A",
                                                               "C3A8E0", dk1="10241E", lt1="EAF5EF", dk2="183A30",
                                                               lt2="A9D8C2")),
        ],
    )


def saffron() -> dict:
    c = colors("2E2312", "FFF8EA", "87590A", "FAF0D9", "E8A82C", "C2481C", "2D4FB5", "1D8A60",
               "7A4A33", "9C4F96", "C2481C", "7A4A33")
    return dict(
        id="saffron", name="Saffron",
        description="Sunny and editorial: a saffron title slide, bold Trebuchet headings and a Georgia body. For workshops and stories.",
        colors=c, fonts=("Trebuchet MS", "Georgia"),
        master=dict(
            background=solid(scheme("bg1")),
            title=title_style(size=36, color=TX),
            body=body_style(sizes=[22, 19, 17, 15], color=TX_SOFT, bullets=["•", "–", "•", "–"],
                            bullet_color=scheme("accent2"), line_pct=110),
            footer=footer_style(size=10, color=scheme("tx2")),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[shape("Underline", (0.0625, 0.255, 0.09, 0.009), solid(scheme("accent1")))],
        ),
        layouts={
            "title": dict(hide=True, background=solid(scheme("accent1")), ph=[
                ph("ctrTitle", rect=(0.0625, 0.18, 0.80, 0.44), body=body_pr(anchor="b"),
                   lst=title_style(size=64, color=TX, line_pct=85)),
                ph("subTitle", rect=(0.0625, 0.66, 0.60, 0.14),
                   lst=lvl(1, algn="l", bullet_none=True, rpr=def_rpr(22, color=scheme("tx1", 80000)))),
            ]),
            "secHead": dict(dark=True, hide=True, background=solid(scheme("dk1")), ph=[
                ph("title", rect=(0.0625, 0.28, 0.85, 0.34), body=body_pr(anchor="b"),
                   lst=title_style(size=48, color=scheme("accent1"))),
                ph("body", rect=(0.0625, 0.64, 0.70, 0.14),
                   lst=lvl(1, bullet_none=True, rpr=def_rpr(20, color=scheme("lt2")))),
            ]),
        },
        variants=[
            dict(name="Saffron", colors=c),
            dict(name="Marigold", colors=with_accents(c, "F2B705", "C2481C", "2D4FB5", "1D8A60", "7A4A33", "9C4F96")),
            dict(name="Amber", colors=with_accents(c, "D98E04", "8C3B1A", "2D4FB5", "1D8A60", "7A4A33", "9C4F96",
                                                   dk2="7A4A07")),
            dict(name="Dusk", dark=True, colors=with_accents(c, "F2C35C", "F7976B", "97AEF4", "6FD0A6", "D9B38C",
                                                             "D7A2D2", dk1="241B10", lt1="FBF1DD", dk2="3A2C18",
                                                             lt2="E8D4A8")),
        ],
    )


def celadon() -> dict:
    c = colors("1F3534", "EEF5F3", "3E6E68", "DCEAE6", "5E9E95", "2F6690", "C7804A", "9A6FB0",
               "6B8E4E", "D1A54A", "2F6690", "9A6FB0")
    return dict(
        id="celadon", name="Celadon",
        description="Pale blue-green glaze with a soft kiln arch on the title slide and Cambria headings. Gentle and considered.",
        colors=c, fonts=("Cambria", "Calibri"),
        master=dict(
            background=solid(scheme("bg1")),
            title=title_style(size=38, color=scheme("tx2"), bold=False),
            body=body_style(sizes=[22, 20, 18, 16], color=TX, bullets=["•", "–", "•", "–"],
                            bullet_color=scheme("accent1")),
            footer=footer_style(size=10, color=scheme("tx2")),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[shape("Hairline", (0.0625, 0.268, 0.875, 0.002), solid(scheme("accent1", alpha=60000)))],
        ),
        layouts={
            "title": dict(hide=True, shapes=[
                shape("Arch", (0.27, 0.10, 0.46, 0.90), solid(scheme("bg2")), prst="round2SameRect",
                      adj="adj1=50000;adj2=0"),
                shape("Sill", (0.27, 0.985, 0.46, 0.015), solid(scheme("accent1"))),
            ], ph=[
                ph("ctrTitle", rect=(0.30, 0.30, 0.40, 0.30), body=body_pr(anchor="b"),
                   lst=title_style(size=44, color=scheme("tx2"), bold=False, algn="ctr")),
                ph("subTitle", rect=(0.31, 0.63, 0.38, 0.14),
                   lst=lvl(1, algn="ctr", bullet_none=True, rpr=def_rpr(18, color=TX_SOFT))),
            ]),
            "secHead": dict(hide=True, background=solid(scheme("bg2")), shapes=[
                shape("Sill", (0.0625, 0.70, 0.08, 0.006), solid(scheme("accent1")))], ph=[
                ph("title", body=body_pr(anchor="b"), lst=title_style(size=44, color=scheme("tx2"), bold=False)),
            ]),
        },
        variants=[
            dict(name="Celadon", colors=c),
            dict(name="Mist", colors=with_accents(c, "6F8FA3", "5E9E95", "C7804A", "9A6FB0", "6B8E4E", "D1A54A",
                                                  dk2="4A6475", lt2="E1E8EE")),
            dict(name="Sage", colors=with_accents(c, "7E9C74", "C7804A", "2F6690", "9A6FB0", "5E9E95", "D1A54A",
                                                  dk2="566E4E", lt2="E6EDE2")),
            dict(name="Deep Sea", dark=True, colors=with_accents(c, "7FC8BD", "8DB6E0", "E8A877", "C3A3D9", "A3C487",
                                                                 "E8C77F", dk1="12282A", lt1="E8F3F0", dk2="1D3B3C",
                                                                 lt2="A8CFC8")),
        ],
    )


def shino() -> dict:
    c = colors("3A322C", "FBF7F0", "8C5A3C", "F0E6D8", "C8553D", "E4A06F", "5D7A8C", "A3884D",
               "8C8279", "6E4B6B", "5D7A8C", "6E4B6B")
    scorch = gradient([(0, scheme("accent1", alpha=50000)), (30000, scheme("accent2", alpha=28000)),
                       (62000, scheme("accent2", alpha=0)), (100000, scheme("accent2", alpha=0))], path="circle")
    return dict(
        id="shino", name="Shino",
        description="Creamy white with a scorched orange blush, Garamond headings and Gill Sans text. Crafted and personal.",
        colors=c, fonts=("Garamond", "Gill Sans MT"),
        master=dict(
            background=solid(scheme("bg1")),
            title=title_style(size=40, color=TX, bold=False),
            body=body_style(sizes=[22, 20, 18, 16], color=TX_SOFT, bullets=["•", "–", "•", "–"],
                            bullet_color=scheme("accent1")),
            footer=footer_style(size=10, color=TX_FAINT),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[shape("Blush", (0.80, -0.20, 0.36, 0.64), scorch, prst="ellipse")],
        ),
        layouts={
            "title": dict(hide=True, shapes=[
                shape("Blush", (0.52, 0.30, 0.70, 1.20), scorch, prst="ellipse"),
            ], ph=[
                ph("ctrTitle", rect=(0.0625, 0.22, 0.62, 0.38), body=body_pr(anchor="b"),
                   lst=title_style(size=58, color=TX, bold=False, line_pct=88)),
                ph("subTitle", rect=(0.0625, 0.63, 0.50, 0.14),
                   lst=lvl(1, algn="l", bullet_none=True, rpr=def_rpr(20, italic=True, color=scheme("tx2")))),
            ]),
        },
        variants=[
            dict(name="Shino", colors=c),
            dict(name="Stone", colors=with_accents(c, "8C8279", "C8553D", "5D7A8C", "A3884D", "E4A06F", "6E4B6B")),
            dict(name="Clay", colors=with_accents(c, "A8432E", "D98E5F", "5D7A8C", "A3884D", "8C8279", "6E4B6B",
                                                  dk2="7A3A26")),
            dict(name="Ink", dark=True, colors=with_accents(c, "E7836D", "F0BC93", "93AFC2", "CDB37A", "BDB3A9",
                                                            "B691B3", dk1="221D19", lt1="F6EFE4", dk2="342C26",
                                                            lt2="D8C9B6")),
        ],
    )


def raku() -> dict:
    c = colors("121212", "EDE6DD", "2A2826", "B9AFA3", "C87533", "2FA4A4", "D4A537", "B5483A",
               "7FA35B", "8E7CC3", "2FA4A4", "8E7CC3")
    return dict(
        id="raku", name="Raku",
        description="Smoke black with copper and turquoise lustre and wide-tracked capitals. Bold, for launches and keynotes.",
        colors=c, fonts=("Arial", "Arial"), dark=True,
        master=dict(
            background=solid(scheme("bg1")),
            title=title_style(size=32, color=TX, caps="all", spc=300),
            body=body_style(sizes=[22, 20, 18, 16], color=scheme("tx1", 85000), bullets=["■", "–", "■", "–"],
                            bullet_color=scheme("accent1")),
            footer=footer_style(size=9, color=scheme("tx1", 55000)),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[shape("Lustre", (0, 0.985, 1, 0.015), gradient(
                [(0, scheme("accent1")), (60000, scheme("accent3")), (100000, scheme("accent2"))], angle=0))],
        ),
        layouts={
            "title": dict(hide=True, background=gradient(
                [(0, srgb("4A2A16")), (45000, srgb("1C1612")), (100000, srgb("0D0D0D"))], angle=2700000), shapes=[
                shape("Lustre", (0.0625, 0.66, 0.16, 0.006), solid(scheme("accent1")))], ph=[
                ph("ctrTitle", rect=(0.0625, 0.24, 0.86, 0.40), body=body_pr(anchor="b"),
                   lst=title_style(size=54, color=TX, caps="all", spc=400, line_pct=95)),
                ph("subTitle", rect=(0.0625, 0.69, 0.70, 0.12),
                   lst=lvl(1, algn="l", bullet_none=True, rpr=def_rpr(18, color=scheme("accent2"), caps="all",
                                                                        spc=200))),
            ]),
            "secHead": dict(hide=True, background=solid(scheme("accent1")), dark=False, ph=[
                ph("title", rect=(0.0625, 0.30, 0.86, 0.33), body=body_pr(anchor="b"),
                   lst=title_style(size=44, color=scheme("dk1"), caps="all", spc=300)),
                ph("body", rect=(0.0625, 0.66, 0.70, 0.14),
                   lst=lvl(1, bullet_none=True, rpr=def_rpr(18, color=scheme("dk1")))),
            ]),
        },
        variants=[
            dict(name="Copper", colors=c),
            dict(name="Turquoise", colors=with_accents(c, "2FA4A4", "C87533", "D4A537", "B5483A", "7FA35B", "8E7CC3")),
            dict(name="Gold", colors=with_accents(c, "D4A537", "2FA4A4", "C87533", "B5483A", "7FA35B", "8E7CC3")),
            dict(name="Bone", dark=False, colors=with_accents(c, "A85A22", "1F7F80", "A8821E", "9A3428", "5E7D3E",
                                                              "6C5BA3", dk1="1A1816", lt1="EDE6DD", dk2="2A2826",
                                                              lt2="D8CFC2")),
        ],
    )


def porcelain() -> dict:
    c = colors("1F2A37", "FFFFFF", "1F3A5F", "EEF2F6", "2F6DB5", "7F8C9B", "3FA7A0", "E07B53",
               "9C7AC0", "E3B341", "2F6DB5", "9C7AC0")
    return dict(
        id="porcelain", name="Porcelain",
        description="Crisp white, ink blue and a hairline rule in Calibri. The familiar, professional choice.",
        colors=c, fonts=("Calibri Light", "Calibri"),
        master=dict(
            background=solid(scheme("bg1")),
            title=title_style(size=36, color=scheme("tx2"), bold=False),
            body=body_style(sizes=[24, 20, 18, 16], color=TX, bullets=["•", "–", "•", "–"], bullet_color=None,
                            line_pct=95),
            footer=footer_style(size=10, color=TX_FAINT),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[shape("Rule", (0.0625, 0.268, 0.875, 0.0022), solid(scheme("tx2", alpha=35000)))],
        ),
        layouts={
            "title": dict(hide=True, shapes=[shape("Band", (0, 0.36, 0.022, 0.30), solid(scheme("accent1")))], ph=[
                ph("ctrTitle", rect=(0.0625, 0.26, 0.82, 0.32), body=body_pr(anchor="b"),
                   lst=title_style(size=52, color=scheme("tx2"), bold=False)),
                ph("subTitle", rect=(0.0625, 0.61, 0.70, 0.13),
                   lst=lvl(1, algn="l", bullet_none=True, rpr=def_rpr(20, color=TX_SOFT))),
            ]),
            "secHead": dict(hide=True, shapes=[shape("Band", (0, 0.44, 0.022, 0.22), solid(scheme("accent1")))], ph=[
                ph("title", body=body_pr(anchor="b"), lst=title_style(size=48, color=scheme("tx2"), bold=False)),
            ]),
        },
        variants=[
            dict(name="Porcelain", colors=c),
            dict(name="Graphite", colors=with_accents(c, "4A5563", "2F6DB5", "3FA7A0", "E07B53", "9C7AC0", "E3B341",
                                                      dk2="2E3440")),
            dict(name="Delft", colors=with_accents(c, "1B4D9B", "6C8FC7", "3FA7A0", "E07B53", "9C7AC0", "E3B341",
                                                   dk2="1B3F7A", lt2="E8EEF8")),
            dict(name="Night", dark=True, colors=with_accents(c, "7FAAE3", "AEB8C4", "6CCBC3", "F0A27F", "C4AEE0",
                                                              "F0CE73", dk1="161D27", lt1="EEF2F7", dk2="22303F",
                                                              lt2="A9BBD3")),
        ],
    )


def slate() -> dict:
    c = colors("1C232B", "E8EDF2", "2B3540", "AAB6C3", "4FB3BF", "F07E5E", "F2B84B", "7FC28B",
               "A78BDA", "5B8DEF", "7FC8D2", "A78BDA")
    return dict(
        id="slate", name="Slate",
        description="Blue-grey dark mode with teal and coral and Aptos type. Precise, for product and engineering reviews.",
        colors=c, fonts=("Aptos Display", "Aptos"), dark=True,
        master=dict(
            background=gradient([(0, srgb("222B35")), (100000, srgb("181E25"))], angle=5400000),
            title=title_style(size=34, color=TX),
            body=body_style(sizes=[22, 19, 17, 15], color=scheme("tx1", 85000), bullets=["•", "–", "•", "–"],
                            bullet_color=scheme("accent1")),
            footer=footer_style(size=9.5, color=scheme("tx1", 55000)),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[shape("Marker", (0.0405, 0.152, 0.0055, 0.10), solid(scheme("accent1")))],
        ),
        layouts={
            "title": dict(hide=True, background=gradient(
                [(0, srgb("2E3B49")), (60000, srgb("1C232B")), (100000, srgb("141A20"))], angle=2700000), shapes=[
                shape("Marker", (0.0625, 0.30, 0.006, 0.30), solid(scheme("accent1")))], ph=[
                ph("ctrTitle", rect=(0.085, 0.26, 0.80, 0.34), body=body_pr(anchor="b"),
                   lst=title_style(size=54, color=TX)),
                ph("subTitle", rect=(0.085, 0.62, 0.70, 0.12),
                   lst=lvl(1, algn="l", bullet_none=True, rpr=def_rpr(20, color=scheme("accent1")))),
            ]),
            "secHead": dict(hide=True, shapes=[shape("Marker", (0.0405, 0.50, 0.0055, 0.16), solid(scheme("accent1")))], ph=[
                ph("title", body=body_pr(anchor="b"), lst=title_style(size=48, color=TX)),
            ]),
        },
        variants=[
            dict(name="Slate", colors=c),
            dict(name="Coral", colors=with_accents(c, "F07E5E", "4FB3BF", "F2B84B", "7FC28B", "A78BDA", "5B8DEF")),
            dict(name="Ocean", colors=with_accents(c, "5B8DEF", "4FB3BF", "F2B84B", "F07E5E", "7FC28B", "A78BDA",
                                                   dk1="13233A", dk2="1E3350"),
                 background=gradient([(0, srgb("1B3150")), (100000, srgb("101C2E"))], angle=5400000)),
            dict(name="Daylight", dark=False, colors=with_accents(c, "1F8A96", "D9583A", "C98A12", "3E8E50", "7A5CC0",
                                                                  "2F63D0", dk1="1C232B", lt1="F7F9FB", dk2="2B3540",
                                                                  lt2="E3E9EF"),
                 background=solid(scheme("bg1"))),
        ],
    )


def terracotta() -> dict:
    c = colors("3B2418", "F8EEE5", "B4462A", "EED9C8", "B4462A", "7D8B4A", "C98A2B", "2F6F6A",
               "8A5A44", "5E5A8C", "2F6F6A", "8A5A44")
    return dict(
        id="terracotta", name="Terracotta",
        description="Earthy clay, olive and ochre with an arched window on the title slide. Warm, for culture and community.",
        colors=c, fonts=("Constantia", "Corbel"),
        master=dict(
            background=solid(scheme("bg1")),
            title=title_style(size=38, color=scheme("tx2")),
            body=body_style(sizes=[22, 20, 18, 16], color=TX, bullets=["•", "–", "•", "–"],
                            bullet_color=scheme("accent1")),
            footer=footer_style(size=10, color=scheme("tx2")),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
        ),
        layouts={
            "title": dict(dark=True, hide=True, background=solid(scheme("accent1")), shapes=[
                shape("Window", (0.66, 0.16, 0.25, 0.84), solid(scheme("lt2", alpha=None)), prst="round2SameRect",
                      adj="adj1=50000;adj2=0"),
                shape("Window glow", (0.70, 0.30, 0.17, 0.70), solid(scheme("accent3")), prst="round2SameRect",
                      adj="adj1=50000;adj2=0"),
            ], ph=[
                ph("ctrTitle", rect=(0.0625, 0.22, 0.55, 0.42), body=body_pr(anchor="b"),
                   lst=title_style(size=54, color=scheme("lt1"), line_pct=88)),
                ph("subTitle", rect=(0.0625, 0.67, 0.52, 0.13),
                   lst=lvl(1, algn="l", bullet_none=True, rpr=def_rpr(20, color=scheme("lt2")))),
            ]),
            "secHead": dict(background=solid(scheme("bg2")), ph=[
                ph("title", body=body_pr(anchor="b"), lst=title_style(size=44, color=scheme("tx2"))),
            ]),
        },
        variants=[
            dict(name="Terracotta", colors=c),
            dict(name="Olive", colors=with_accents(c, "6C7A3A", "B4462A", "C98A2B", "2F6F6A", "8A5A44", "5E5A8C",
                                                   dk2="5B6830")),
            dict(name="Ochre", colors=with_accents(c, "B87A1E", "B4462A", "7D8B4A", "2F6F6A", "8A5A44", "5E5A8C",
                                                   dk2="8C5C14")),
            dict(name="Umber", dark=True, colors=with_accents(c, "E07856", "B4C27A", "E8B864", "6FB3AD", "C79A84",
                                                              "A8A4D6", dk1="2A1A12", lt1="F6E9DD", dk2="3E2619",
                                                              lt2="E3C4AE")),
        ],
    )


def ash() -> dict:
    c = colors("2E2F2A", "F5F4EF", "5C6650", "E4E3DA", "6B7F4E", "A65A3A", "4E6170", "C2A15A",
               "8C8C7E", "7A5C7E", "4E6170", "7A5C7E")
    return dict(
        id="ash", name="Ash",
        description="Natural ash glaze: moss, rust and slate on stone paper, Book Antiqua over Gill Sans. Thoughtful and grounded.",
        colors=c, fonts=("Book Antiqua", "Gill Sans MT"),
        master=dict(
            background=solid(scheme("bg1")),
            title=title_style(size=38, color=TX, bold=False),
            body=body_style(sizes=[22, 20, 18, 16], color=TX_SOFT, bullets=["•", "–", "•", "–"],
                            bullet_color=scheme("accent1")),
            footer=footer_style(size=10, color=TX_FAINT),
            ph=[ph("title", body=body_pr(anchor="b", autofit="shrink"))],
            shapes=[
                shape("Rule top", (0.0625, 0.905, 0.875, 0.0018), solid(scheme("tx1", alpha=30000))),
            ],
        ),
        layouts={
            "title": dict(dark=True, hide=True, shapes=[
                shape("Glaze", (0, 0, 1, 0.64), solid(scheme("accent1"))),
                shape("Foot", (0, 0.64, 1, 0.36), solid(scheme("lt1"))),
            ], background=solid(scheme("dk1")), ph=[
                ph("ctrTitle", rect=(0.0625, 0.16, 0.80, 0.42), body=body_pr(anchor="b"),
                   lst=title_style(size=54, color=scheme("lt1"), bold=False, line_pct=88)),
                ph("subTitle", rect=(0.0625, 0.70, 0.70, 0.14),
                   lst=lvl(1, algn="l", bullet_none=True, rpr=def_rpr(20, color=scheme("dk2")))),
            ]),
        },
        variants=[
            dict(name="Ash", colors=c),
            dict(name="Moss", colors=with_accents(c, "4F6B3A", "C2A15A", "4E6170", "A65A3A", "8C8C7E", "7A5C7E",
                                                  dk2="4F6B3A")),
            dict(name="Rust", colors=with_accents(c, "A65A3A", "6B7F4E", "4E6170", "C2A15A", "8C8C7E", "7A5C7E",
                                                  dk2="7E4229")),
            dict(name="Charcoal", dark=True, colors=with_accents(c, "A3B87F", "D9906E", "8FA6B8", "E0C27E", "B8B8AA",
                                                                 "B596B8", dk1="23241F", lt1="F0EFE8", dk2="34362F",
                                                                 lt2="CFCDC0")),
        ],
    )


THEMES = [kiln, tenmoku, cobalt, jade, saffron, celadon, shino, raku, porcelain, slate, terracotta, ash]


def main() -> None:
    for make in THEMES:
        t = make()
        path = HERE / f'{t["id"]}.kdeck-theme'
        path.write_text(theme_doc(t), encoding="utf-8")
        print(f"wrote {path.relative_to(HERE.parent.parent)}")


if __name__ == "__main__":
    main()
