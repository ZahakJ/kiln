"""The KILN wordmark: lowercase 'kiln' drawn from Fraunces Black (opsz 144), converted to outlines.
The tittle of the i is the flicked flame; the counter of the n is a lit kiln mouth."""
import sys, functools
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.boundsPen import BoundsPen
from geo import f
from icons import FLAME_BODY, FLAME_BACK, FLAME_H, flame_mono

FR = '/home/muth/kiln-brand/final/fonts/Fraunces-VF.ttf'
AXES = {"wght": 900, "SOFT": 40, "WONK": 0, "opsz": 144}


@functools.lru_cache(None)
def inst(axes_items):
    return instancer.instantiateVariableFont(TTFont(FR), dict(axes_items))


def glyph_runs(names, size, x0, base, kern, axes):
    fnt = inst(tuple(sorted(axes.items())))
    gs = fnt.getGlyphSet(); hm = fnt['hmtx']; s = size / fnt['head'].unitsPerEm
    x = x0; out = []
    for i, n in enumerate(names):
        pen = SVGPathPen(gs, ntos=lambda v: ('%.2f' % v).rstrip('0').rstrip('.'))
        gs[n].draw(TransformPen(pen, (s, 0, 0, -s, x, base)))
        b = BoundsPen(gs); gs[n].draw(b)
        bb = b.bounds
        out.append(dict(name=n, d=pen.getCommands(), x=x,
                        bbox=(x + bb[0] * s, base - bb[3] * s, x + bb[2] * s, base - bb[1] * s)))
        x += hm[n][0] * s + (kern[i] if i < len(kern) else 0) * size
    return out, x - x0


class FlatPen:
    pass


def glyph_polys(name, size, x, base, axes):
    from fontTools.pens.basePen import BasePen
    fnt = inst(tuple(sorted(axes.items())))
    gs = fnt.getGlyphSet(); s = size / fnt['head'].unitsPerEm
    polys = []

    class P(BasePen):
        def _moveTo(self, p): polys.append([p])
        def _lineTo(self, p): polys[-1].append(p)
        def _curveToOne(self, p1, p2, p3):
            p0 = self._getCurrentPoint()
            for k in range(1, 13):
                t = k / 12; m = 1 - t
                polys[-1].append((m**3*p0[0]+3*m*m*t*p1[0]+3*m*t*t*p2[0]+t**3*p3[0], m**3*p0[1]+3*m*m*t*p1[1]+3*m*t*t*p2[1]+t**3*p3[1]))
        def _qCurveToOne(self, p1, p2):
            p0 = self._getCurrentPoint()
            for k in range(1, 13):
                t = k / 12; m = 1 - t
                polys[-1].append((m*m*p0[0]+2*m*t*p1[0]+t*t*p2[0], m*m*p0[1]+2*m*t*p1[1]+t*t*p2[1]))
    gs[name].draw(TransformPen(P(gs), (s, 0, 0, -s, x, base)))
    return polys


def counter_of_n(gl, size, axes, base):
    """measure the n counter: returns (xl, xr, ytop) in svg coords"""
    from PIL import Image, ImageDraw
    x0, y0, x1, y1 = gl['bbox']
    W, Hh = int(x1 - x0) + 4, int(y1 - y0) + 4
    sc = 4
    img = Image.new('L', (W * sc, Hh * sc), 0)
    dr = ImageDraw.Draw(img)
    for poly in glyph_polys(gl['name'], size, gl['x'], base, axes):
        dr.polygon([((px - x0 + 2) * sc, (py - y0 + 2) * sc) for px, py in poly], fill=255)
    yy = int((Hh * 0.8) * sc)
    row = [img.getpixel((i, yy)) for i in range(W * sc)]
    runs = []; inside = False
    for i, v in enumerate(row):
        if v and not inside: start = i; inside = True
        if not v and inside: runs.append((start, i)); inside = False
    if inside: runs.append((start, len(row)))
    xl = runs[0][1] / sc + x0 - 2
    xr = runs[1][0] / sc + x0 - 2
    cx = int(((xl + xr) / 2 - x0 + 2) * sc)
    col = [img.getpixel((cx, j)) for j in range(Hh * sc)]
    j = 0
    while j < len(col) and not col[j]: j += 1
    while j < len(col) and col[j]: j += 1
    ytop = j / sc + y0 - 2
    return xl, xr, ytop


def wordmark(size, x, base, ink, uid, mode='color', axes=None, kern=(-0.018, -0.012, -0.012, 0),
             glow=False, flame_scale=1.0):
    """svg fragment for the wordmark. mode: color | mono (single ink, flame and mouth knocked/inked).
    returns (svg, width, bbox)"""
    axes = axes or AXES
    gls, w = glyph_runs(['k', 'dotlessi', 'l', 'n'], size, x, base, list(kern), axes)
    i_gl, n_gl = gls[1], gls[3]
    ix0, iy0, ix1, iy1 = i_gl['bbox']
    # i stem centre: dotless i has a left-side serif; take the stem from the top portion
    stem_cx = (ix0 + ix1) / 2 + 0.0 * size
    xh = iy0                       # top of the x-height stem
    fh = size * 0.34 * flame_scale  # flame height
    fbase = xh - size * 0.055
    defs = ''
    parts = ''
    if glow:
        xl, xr, yt = counter_of_n(n_gl, size, axes, base)
        pad = size * 0.01
        if mode == 'color':
            defs += (f'<radialGradient id="{uid}-ng" cx="{f((xl + xr) / 2)}" cy="{f(base)}" r="{f((base - yt) * 1.05)}" gradientUnits="userSpaceOnUse">'
                     f'<stop offset="0" stop-color="#FFB347"/><stop offset="0.4" stop-color="#F06A26"/>'
                     f'<stop offset="1" stop-color="#B8350F"/></radialGradient>')
            parts += f'<path d="M{f(xl - pad)},{f(base)} V{f(yt + size*0.02)} H{f(xr + pad)} V{f(base)} Z" fill="url(#{uid}-ng)"/>'
    # flame tittle
    s = fh / FLAME_H
    # visually centre the flame belly (x=256 in flame space) over the stem
    t = f'translate({f(stem_cx + size * 0.004)} {f(fbase)}) scale({s:.4f}) translate(-256 -432)'
    if mode == 'color':
        defs += (f'<linearGradient id="{uid}-fb" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FF6A26"/>'
                 f'<stop offset="1" stop-color="#FFB547"/></linearGradient>')
        fl = (f'<g transform="{t}"><path d="{FLAME_BACK}" fill="#D93D12"/><path d="{FLAME_BODY}" fill="url(#{uid}-fb)"/></g>')
    else:
        fl = flame_mono(stem_cx + size * 0.004, fbase, fh, ink, uid)
    letters = ''.join(g['d'] + ' ' for g in gls)
    svg = f'<defs>{defs}</defs>{parts}<path d="{letters}" fill="{ink}"/>{fl}'
    top = fbase - fh
    return svg, w, (x, top, x + w, base)
