"""Shaped text -> SVG outlines (HarfBuzz shaping, fontTools outlines), so renders are exact anywhere."""
import io, functools
import uharfbuzz as hb
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

FONTS = {
    'fraunces': '/home/muth/kiln-brand/final/fonts/Fraunces-VF.ttf',
    'figtree': '/home/muth/kiln-brand/final/fonts/Figtree-VF.ttf',
}


@functools.lru_cache(None)
def _inst(font, axes):
    f = instancer.instantiateVariableFont(TTFont(FONTS[font]), dict(axes))
    buf = io.BytesIO(); f.save(buf)
    data = buf.getvalue()
    return TTFont(io.BytesIO(data)), data


def text_path(text, font='figtree', size=16, x=0, y=0, axes=None, track=0.0, features=None, anchor='start'):
    """returns (d, advance_width). track in em."""
    axes = tuple(sorted((axes or {'wght': 400}).items()))
    tt, data = _inst(font, axes)
    face = hb.Face(data); hfont = hb.Font(face)
    upm = face.upem
    buf = hb.Buffer(); buf.add_str(text); buf.guess_segment_properties()
    hb.shape(hfont, buf, features or {'kern': True, 'liga': True})
    s = size / upm
    gs = tt.getGlyphSet(); order = tt.getGlyphOrder()
    total = sum(p.x_advance for p in buf.glyph_positions) * s + track * size * (len(buf.glyph_infos) - 1)
    if anchor == 'middle':
        x -= total / 2
    elif anchor == 'end':
        x -= total
    cx = x; ds = []
    for info, pos in zip(buf.glyph_infos, buf.glyph_positions):
        name = order[info.codepoint]
        pen = SVGPathPen(gs, ntos=lambda v: ('%.2f' % v).rstrip('0').rstrip('.'))
        gs[name].draw(TransformPen(pen, (s, 0, 0, -s, cx + pos.x_offset * s, y - pos.y_offset * s)))
        ds.append(pen.getCommands())
        cx += pos.x_advance * s + track * size
    return ' '.join(d for d in ds if d), total


def text(t, font='figtree', size=16, x=0, y=0, fill='#000', axes=None, track=0.0, anchor='start', opacity=None, features=None):
    d, w = text_path(t, font, size, x, y, axes, track, features, anchor)
    op = f' fill-opacity="{opacity}"' if opacity is not None else ''
    return f'<path d="{d}" fill="{fill}"{op}/>', w


FR_DISPLAY = {'wght': 800, 'SOFT': 40, 'WONK': 0, 'opsz': 72}
