"""Construction sheet: the kiln arch, the mouth, the flame, the glyph safe area, clear space."""
import sys
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from icons import TILE, MOUTH, TOP, BASE, SLAB, W, H, MW, MH, MTOP, MBASE, SAFE, icon_full, mono_mark, P, Q, glyph
from geo import kiln_arch_open

O = '/home/muth/kiln-brand/final'
G = '#3159C6'
E = '#C2481C'


def sheet():
    s = 1.2
    ox, oy = 70, 110
    lines = []
    def L(x1, y1, x2, y2, c=G, dash='4 5', w=1.2):
        lines.append(f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{c}" stroke-width="{w / s:.2f}" stroke-dasharray="{dash}"/>')
    for y in (TOP, BASE, BASE + SLAB, MTOP, MBASE):
        L(-40, y, 560, y)
    for x in (256 - W / 2, 256 + W / 2, 256, 256 - MW / 2, 256 + MW / 2):
        L(x, 0, x, 512)
    g, _ = glyph('write', 'full')
    tile = (f'<path d="{TILE}" fill="#FFF6E8" stroke="#2B1C14" stroke-width="{2 / s:.2f}"/>'
            f'<rect x="{256 - W / 2 + 30}" y="{BASE}" width="{W - 60}" height="{SLAB}" rx="6" fill="#2B1C14" fill-opacity=".12"/>'
            f'<path d="{MOUTH}" fill="none" stroke="{E}" stroke-width="{2 / s:.2f}"/>')
    safe = f'<rect x="{SAFE[0]}" y="{SAFE[1]}" width="{SAFE[2]}" height="{SAFE[3]}" fill="{G}" fill-opacity=".06" stroke="{G}" stroke-width="{1.2 / s:.2f}"/>'
    left = f'<g transform="translate({ox} {oy}) scale({s})">{"".join(lines)}{tile}{safe}</g>'
    labels = f'''
    <div class="lb" style="left:{ox + 566 * s}px;top:{oy + TOP * s - 9}px">crown</div>
    <div class="lb" style="left:{ox + 566 * s}px;top:{oy + MTOP * s - 9}px">mouth · 0.54 W × 0.64 H</div>
    <div class="lb" style="left:{ox + 566 * s}px;top:{oy + MBASE * s - 18}px">sill · 20</div>
    <div class="lb" style="left:{ox + 566 * s}px;top:{oy + BASE * s - 9}px">hearth</div>
    <div class="lb" style="left:{ox + 566 * s}px;top:{oy + (BASE + SLAB) * s - 4}px">slab · 20</div>
    <div class="lb b" style="left:{ox + SAFE[0] * s + 6}px;top:{oy + SAFE[1] * s + 4}px">glyph safe area 232 × 216</div>'''
    return left, labels


def build():
    left, labels = sheet()
    clear = (f'<svg width="430" height="300" viewBox="0 0 430 300"><rect x="0" y="0" width="430" height="300" fill="none"/>'
             f'<rect x="20" y="20" width="390" height="260" fill="{E}" fill-opacity=".07" stroke="{E}" stroke-dasharray="5 5"/>'
             f'<rect x="85" y="68" width="260" height="164" fill="#FFFCF7"/>'
             f'<image href="../../lockup-light.svg" x="85" y="68" width="260" height="164" preserveAspectRatio="xMidYMid meet"/></svg>')
    html = f'''<!doctype html><html><head><meta charset="utf-8"><style>
    @font-face {{ font-family: Figtree; src: url(../../fonts/Figtree-VF.ttf); font-weight: 300 900; }}
    @font-face {{ font-family: Fraunces; src: url(../../fonts/Fraunces-VF.ttf); font-weight: 100 900; }}
    body {{ margin: 0; width: 1600px; height: 900px; background: #FFFCF7; font: 15px/1.45 Figtree; color: #2B1C14; position: relative; overflow: hidden; }}
    .lb {{ position: absolute; font: 600 13px/1 Figtree; color: {G}; }} .lb.b {{ color: {G}; }}
    h2 {{ font: 800 30px/1.1 Fraunces; font-variation-settings: 'SOFT' 40; margin: 0 0 10px; }}
    .r {{ position: absolute; left: 970px; top: 60px; width: 580px; }}
    .r p {{ margin: 0 0 12px; color: #6B5547; }}
    code {{ font: 700 14px/1 Figtree; color: {E}; background: #FBE8DE; padding: 2px 6px; border-radius: 5px; }}
    .k {{ display: flex; gap: 22px; margin-top: 18px; align-items: flex-end; }}
    .k div {{ text-align: center; font: 600 12px Figtree; color: #8F7A6A; }}
    </style></head><body>
    <svg width="900" height="900" style="position:absolute;left:0;top:0">{left}</svg>{labels}
    <div class="r">
      <h2>One arch, fixed for every tile</h2>
      <p>The tile is a superelliptic kiln arch <code>|x/L|<sup>2.6</sup> + |y/H|<sup>1.6</sup> = 1</code>. The 1.6 crown pulls it toward a catenary, the curve real kilns are built on, so it reads as an arch and not a gumdrop. The 2.6 walls meet the hearth almost vertically. Width 440, height 412 on a 512 grid, hearth corners r 40, a 20 px slab underneath.</p>
      <p>The suite mark cuts a mouth from the same arch at 0.54 W × 0.64 H, standing on a 20 px sill. The flame is a flicked body with a back tongue; the white-hot core is the body at 0.42 scale.</p>
      <p>Product glyphs live in the 232 × 216 safe area centred at (256, 300). They are bone <code>#FFF6E8</code>, never white, and never touch the rim.</p>
      <h2 style="margin-top:26px">Clear space and minimum sizes</h2>
      <p>Keep clear space equal to the height of the <b>n</b> on every side of the lockup. Smallest lockup: 24 px tall on screen, 12 mm in print. Below that, use the mark alone. Under 24 px, use the flat tile; at 16 px, the pixel-fitted tile.</p>
      {clear}
    </div>
    </body></html>'''
    open(f'{O}/work/html/construction.html', 'w').write(html)


if __name__ == '__main__':
    build()
