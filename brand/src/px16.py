"""Hand-fitted 16 px icons, drawn pixel by pixel on an integer grid.
Every pixel is (colour, alpha). Output is an SVG of merged rect runs with crispEdges."""
import math

# silhouette of the kiln arch at 16 px: row -> (first col, last col, edge alpha)
SIL = {0: (6, 9), 1: (4, 11), 2: (3, 12), 3: (2, 13), 4: (2, 13), 5: (1, 14)}
for r in range(6, 16):
    SIL[r] = (1, 14)
# anti-aliased edge pixels (col,row): alpha
AA = {(6, 0): .55, (9, 0): .55, (4, 1): .6, (11, 1): .6, (3, 2): .75, (12, 2): .75,
      (2, 3): .9, (13, 3): .9, (1, 5): .7, (14, 5): .7, (1, 15): .45, (14, 15): .45}
# extra soft pixels just outside the silhouette to round the crown
OUT = {(5, 0): .18, (10, 0): .18, (3, 1): .15, (12, 1): .15, (2, 2): .2, (13, 2): .2,
       (1, 4): .25, (14, 4): .25}

FLAT16 = {  # fill, rim (lighter glaze break), glyph, detail (darker glaze)
    'write': ('#3159C6', '#7393EA', '#FFF6E8', '#1E3C93'),
    'grid':  ('#1B875D', '#5DC197', '#FFF6E8', '#0F5A3D'),
    'deck':  ('#E05A27', '#FF9C72', '#FFF6E8', '#9C3510'),
    'notes': ('#DF9A1C', '#FFD16E', '#FFF6E8', '#8E5C06'),
    'suite': ('#3A241A', '#8A5232', '#FFF1D2', '#1A0D08'),
}


def canvas():
    return [[None] * 16 for _ in range(16)]


def _rgb(h):
    h = h.lstrip('#'); return [int(h[i:i + 2], 16) for i in (0, 2, 4)]


def put(c, x, y, col, a=1.0, replace=False):
    """source-over compositing onto the canvas"""
    if not (0 <= x < 16 and 0 <= y < 16):
        return
    d = c[y][x]
    if replace or d is None or a >= 1:
        c[y][x] = (col, a)
        return
    sr, dr = _rgb(col), _rgb(d[0])
    da = d[1]
    oa = a + da * (1 - a)
    rgb = [round((sr[i] * a + dr[i] * da * (1 - a)) / oa) for i in range(3)]
    c[y][x] = ('#%02X%02X%02X' % tuple(rgb), round(oa, 3))


def tile(c, fill, rim, wall_rim=False):
    for r, (a, b) in SIL.items():
        for x in range(a, b + 1):
            put(c, x, r, fill, AA.get((x, r), 1.0))
    for (x, y), a in OUT.items():
        put(c, x, y, fill, a)
    # 1px glaze-break rim along crown and walls (not the hearth)
    for r, (a, b) in SIL.items():
        if r == 15:
            continue
        for x in range(a, b + 1):
            above = SIL.get(r - 1)
            edge = above is None or not (above[0] <= x <= above[1])
            if (edge and r < 6) or (wall_rim and x in (a, b)):
                cur = c[r][x]
                put(c, x, r, rim, cur[1] if cur else 1)


def rect(c, x0, y0, x1, y1, col, a=1.0):
    for y in range(y0, y1 + 1):
        for x in range(x0, x1 + 1):
            put(c, x, y, col, a)


def glyph16(kind, c):
    fill, rim, g, det = FLAT16[kind]
    if kind == 'write':          # pilcrow
        rect(c, 5, 5, 11, 5, g); put(c, 4, 5, g, .55)
        rect(c, 4, 6, 8, 7, g)
        rect(c, 5, 8, 8, 8, g); put(c, 4, 8, g, .55)
        rect(c, 7, 9, 8, 13, g)
        rect(c, 10, 6, 11, 13, g)
    elif kind == 'grid':         # headed table
        rect(c, 4, 5, 11, 6, g)
        for y in (8, 11):
            for x in (4, 7, 10):
                rect(c, x, y, x + 1, y + 1, g)
        # selected cell: brighter frame hint
    elif kind == 'deck':         # split pie (supersampled, snapped)
        cx, cy, r, k = 7.5, 10.0, 4.6, 1.6
        for y in range(16):
            for x in range(16):
                cov = 0
                for sy in range(4):
                    for sx in range(4):
                        px, py = x + (sx + .5) / 4, y + (sy + .5) / 4
                        dx, dy = px - cx, py - cy
                        inq = dx >= 0 and dy <= 0
                        if not inq and dx * dx + dy * dy <= r * r:
                            cov += 1
                        qx, qy = px - (cx + k), py - (cy - k)
                        if qx >= 0 and qy <= 0 and qx * qx + qy * qy <= r * r:
                            cov += 1
                a = cov / 16
                if a > .6:
                    put(c, x, y, g, 1)
                elif a > .3:
                    put(c, x, y, g, .5)
    elif kind == 'notes':        # page, solid folded corner, two ruled lines
        rect(c, 4, 4, 11, 13, g)
        for (x, y) in ((11, 12), (11, 13), (10, 13)):
            put(c, x, y, fill, 1, replace=True)
        for (x, y) in ((11, 11), (10, 12), (9, 13)):
            put(c, x, y, fill, 1, replace=True); put(c, x, y, det, .7)
        for (x, y) in ((10, 11), (9, 12), (9, 11)):
            put(c, x, y, det, 1)
        rect(c, 6, 6, 9, 6, fill)
        rect(c, 6, 9, 8, 9, fill)
    elif kind == 'suite':        # mouth + flame
        # mouth (dark), spans cols 5-10, rows 6-15 with a rounded crown
        rect(c, 6, 6, 9, 6, '#5A1C0A'); put(c, 5, 6, '#5A1C0A', .5); put(c, 10, 6, '#5A1C0A', .5)
        rect(c, 5, 7, 10, 15, '#5A1C0A')
        # glow toward the hearth
        rect(c, 5, 12, 10, 13, '#B83A10'); rect(c, 5, 14, 10, 15, '#E2541C')
        # flame: amber body, cream core, leaning right
        put(c, 8, 7, '#FF8A2E'); put(c, 8, 8, '#FF8A2E'); put(c, 9, 8, '#FF8A2E', .6)
        rect(c, 7, 9, 9, 9, '#FF8A2E'); rect(c, 6, 10, 9, 13, '#FF9A35')
        put(c, 6, 9, '#E2541C', .9)
        rect(c, 7, 11, 8, 13, '#FFE2A6'); put(c, 8, 10, '#FFE2A6', .7)
        rect(c, 6, 14, 9, 14, '#FFB24A')
        rect(c, 5, 15, 10, 15, '#FF9A35')


def build(kind):
    c = canvas()
    fill, rim, g, det = FLAT16[kind]
    tile(c, fill, rim, wall_rim=(kind == 'suite'))
    glyph16(kind, c)
    return c


def to_svg(c, size=16):
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="{size}" height="{size}" shape-rendering="crispEdges">']
    for y in range(16):
        x = 0
        while x < 16:
            p = c[y][x]
            if not p or p[1] <= 0:
                x += 1
                continue
            x1 = x
            while x1 + 1 < 16 and c[y][x1 + 1] == p:
                x1 += 1
            op = '' if p[1] >= 1 else f' fill-opacity="{p[1]:g}"'
            out.append(f'<rect x="{x}" y="{y}" width="{x1 - x + 1}" height="1" fill="{p[0]}"{op}/>')
            x = x1 + 1
    out.append('</svg>')
    return ''.join(out)


def to_png(c, path, scale=1):
    from PIL import Image
    img = Image.new('RGBA', (16, 16), (0, 0, 0, 0))
    for y in range(16):
        for x in range(16):
            p = c[y][x]
            if p and p[1] > 0:
                h = p[0].lstrip('#')
                img.putpixel((x, y), (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), round(255 * p[1])))
    if scale != 1:
        img = img.resize((16 * scale, 16 * scale), Image.NEAREST)
    img.save(path)
    return img
