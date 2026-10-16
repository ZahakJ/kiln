"""Kiln icon system: glazed kiln-arch tiles (full), flat tiles (24-32), pixel-fitted 16 px tiles,
and single-colour companions (monochrome / themed / favicon / toolbar)."""
import sys
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from geo import kiln_arch, f, spark

# ---------------------------------------------------------------- construction (512 grid)
CX = 256
TOP = 40            # crown of the tile
W = 440             # tile width at the hearth
H = 412             # crown -> hearth
SLAB = 20           # visible slab thickness under the glaze
P, Q = 2.6, 1.6     # superellipse exponents of the kiln arch (fixed for every tile)
RB = 40             # hearth corner radius
BASE = TOP + H      # 452
TILE = kiln_arch(CX, TOP, W, H, P, Q, RB)
# glyph safe area: 232 x 216 box centred on (256, 300)
SAFE = (140, 192, 232, 216)

# mouth of the kiln (suite) - the same arch, scaled 0.52 x 0.6, standing on the hearth
MW, MH = 236, 262
MTOP = BASE - MH - 20  # leaves a 20px sill
MOUTH = kiln_arch(CX, MTOP, MW, MH, P, Q, 18)
MBASE = MTOP + MH

BONE = '#FFF6E8'

GLAZE = {
    #          top        base       pool       rim/break  ink(pressed shadow) text-on-light text-on-dark
    'write': ('#4C72E0', '#2748AE', '#18307A', '#A9BEF6', '#132B6E'),
    'grid':  ('#30A577', '#167150', '#0D4A34', '#8FDDBA', '#0A3B29'),
    'deck':  ('#F47B45', '#CC4A1E', '#8F2F10', '#FFC09E', '#7A2508'),
    'notes': ('#F5B73F', '#CF8A12', '#8E5C06', '#FFE3A0', '#7A4C02'),
    'suite': ('#54321F', '#21140D', '#0F0805', '#D9874E', '#000000'),
}
FLAT = {  # 16-32 px: one flat glaze, a 1px lighter rim
    'write': ('#3159C6', '#7E9BEE'),
    'grid':  ('#1D8A60', '#6CCBA2'),
    'deck':  ('#E05A27', '#FFA77F'),
    'notes': ('#E3A126', '#FFD878'),
    'suite': ('#2E1C13', '#8A5333'),
}

# ---------------------------------------------------------------- flame (spark's flicked flame)
# front body (tip flicks right) and back tongue (leans left), drawn on 512, base centre (256,432), height ~376
FLAME_BODY = ("M256 432C188 432 144 386 144 324C144 262 184 222 218 186C250 152 272 118 280 66"
              "C282 56 292 52 298 60C316 98 338 132 352 178C364 216 368 262 368 316C368 384 324 432 256 432Z")
FLAME_BACK = ("M236 428C178 414 132 370 128 306C124 240 162 200 180 156C190 130 190 108 178 84"
              "C177 78 183 75 188 78C246 116 274 164 272 224Z")
# white-hot core: the body scaled 0.46 around its base
FLAME_H = 376


def flame_group(cx, base, h, body_fill, back_fill, core_fill=None, core_scale=0.5, extra=''):
    s = h / FLAME_H
    t = f'translate({f(cx)} {f(base)}) scale({s:.4f}) translate(-256 -432)'
    out = f'<g transform="{t}"{extra}>'
    out += f'<path d="{FLAME_BACK}" fill="{back_fill}"/>'
    out += f'<path d="{FLAME_BODY}" fill="{body_fill}"/>'
    if core_fill:
        out += (f'<path d="{FLAME_BODY}" fill="{core_fill}" '
                f'transform="translate(256 432) scale({core_scale}) translate(-256 -432)"/>')
    return out + '</g>'


# ---------------------------------------------------------------- glyphs (bone, 512 grid)
def rr(x, y, w, h, r):
    return (f"M{f(x + r)},{f(y)}H{f(x + w - r)}A{f(r)},{f(r)} 0 0 1 {f(x + w)},{f(y + r)}V{f(y + h - r)}"
            f"A{f(r)},{f(r)} 0 0 1 {f(x + w - r)},{f(y + h)}H{f(x + r)}A{f(r)},{f(r)} 0 0 1 {f(x)},{f(y + h - r)}"
            f"V{f(y + r)}A{f(r)},{f(r)} 0 0 1 {f(x + r)},{f(y)}Z")


def glyph(kind, level='full'):
    """returns (paths_svg, accent_svg). accent uses var(--acc) (darker glaze) for details."""
    if kind == 'write':
        # geometric pilcrow: half-disc bowl + two stems under one cap bar
        y0, y1 = 194, 414
        if level == 'small':
            return (f'<path d="M262 {y0}H220A74 74 0 0 0 220 {y0 + 148}H262Z"/>'
                    f'<path d="{rr(246, y0, 54, y1 - y0, 14)}"/><path d="{rr(318, y0, 54, y1 - y0, 14)}"/>'
                    f'<path d="{rr(220, y0, 152, 54, 14)}"/>'), ''
        return (f'<path d="M270 {y0}H222A76 76 0 0 0 222 {y0 + 152}H270Z"/>'
                f'<path d="{rr(256, y0, 46, y1 - y0, 14)}"/><path d="{rr(326, y0, 46, y1 - y0, 14)}"/>'
                f'<path d="{rr(222, y0, 150, 46, 14)}"/>'), ''
    if kind == 'grid':
        # headed table with a selected cell + fill handle
        x0, y0, w = 140, 196, 232
        if level == 'small':
            out = f'<path d="{rr(x0, y0, w, 60, 16)}"/>'
            cw, ch, g = (w - 2 * 20) / 3, 56, 20
            for r in range(2):
                for c in range(3):
                    out += f'<path d="{rr(x0 + c * (cw + g), y0 + 80 + r * (ch + g), cw, ch, 12)}"/>'
            return out, ''
        out = f'<path d="{rr(x0, y0, w, 50, 16)}"/>'
        cw, ch, g = (w - 2 * 18) / 3, 60, 18
        acc = ''
        for r in range(2):
            for c in range(3):
                x, y = x0 + c * (cw + g), y0 + 68 + r * (ch + g)
                if (r, c) == (1, 1):
                    # selected cell: a bone frame with a fill handle
                    out += (f'<path fill-rule="evenodd" d="{rr(x - 3, y - 3, cw + 6, ch + 6, 13)} '
                            f'{rr(x + 9, y + 9, cw - 18, ch - 18, 5)}"/>')
                    out += f'<path d="{rr(x + cw - 9, y + ch - 9, 24, 24, 6)}"/>'
                else:
                    out += f'<path d="{rr(x, y, cw, ch, 11)}"/>'
        return out, acc
    if kind == 'deck':
        # split pie: a disc with its top-right quarter pulled out
        cx, cy, r = 248, 312, 104
        k = 22 if level != 'small' else 26
        big = f'M{cx},{cy} V{cy - r} A{r},{r} 0 1 0 {cx + r},{cy} Z'
        q = f'M{cx + k},{cy - k} H{cx + k + r} A{r},{r} 0 0 0 {cx + k},{cy - k - r} Z'
        if level == 'small':
            r2 = 110
            big = f'M{cx},{cy} V{cy - r2} A{r2},{r2} 0 1 0 {cx + r2},{cy} Z'
            q = f'M{cx + k},{cy - k} H{cx + k + r2} A{r2},{r2} 0 0 0 {cx + k},{cy - k - r2} Z'
        return f'<path d="{big}" stroke-linejoin="round"/><path d="{q}"/>', ''
    if kind == 'notes':
        # a tilted page with a solid folded corner and a bookmark ribbon
        if level == 'small':
            page = 'M162 196H350V338L282 406H162Z'
            fold = 'M350 338L282 406V354Q282 338 298 338Z'
            line = rr(196, 238, 120, 34, 17)
            return (f'<g transform="rotate(-6 256 300)"><path fill-rule="evenodd" d="{page} {line}"/>'
                    f'<path d="{fold}" fill="var(--acc)"/></g>'), ''
        page = 'M184 196H340A20 20 0 0 1 360 216V330L282 408H184A20 20 0 0 1 164 388V216A20 20 0 0 1 184 196Z'
        fold = 'M360 330L282 408V350A20 20 0 0 1 302 330Z'
        l1 = rr(198, 250, 118, 26, 13)
        l2 = rr(198, 298, 74, 26, 13)
        rib = 'M300 180H332V246L316 232L300 246Z'
        return (f'<g transform="rotate(-6 256 300)"><path fill-rule="evenodd" d="{page} {l1} {l2}"/>'
                f'<path d="{fold}" fill="var(--acc)"/></g>'), ''
    raise ValueError(kind)


# ---------------------------------------------------------------- full glazed icon (48 px and up)
def icon_full(kind, uid=None, hero=False):
    """hero=True adds the glaze-break highlight and pooled glaze used at 256 px and up."""
    uid = uid or kind
    top, base, pool, rim, ink = GLAZE[kind]
    s = ['<defs>']
    s.append(f'<linearGradient id="{uid}-g" x1="0.2" y1="0" x2="0.45" y2="1">'
             f'<stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{base}"/></linearGradient>')
    s.append(f'<radialGradient id="{uid}-pool" cx="0.5" cy="1.08" r="0.7">'
             f'<stop offset="0" stop-color="{pool}" stop-opacity="0.7"/><stop offset="1" stop-color="{pool}" stop-opacity="0"/></radialGradient>')
    s.append(f'<radialGradient id="{uid}-sheen" cx="0.33" cy="0.12" r="0.55">'
             f'<stop offset="0" stop-color="#fff" stop-opacity="{0.22 if kind != "suite" else 0.14}"/>'
             f'<stop offset="1" stop-color="#fff" stop-opacity="0"/></radialGradient>')
    s.append(f'<linearGradient id="{uid}-rim" x1="0" y1="0" x2="0" y2="1">'
             f'<stop offset="0" stop-color="{rim}" stop-opacity="0.95"/><stop offset="0.55" stop-color="{rim}" stop-opacity="0.35"/>'
             f'<stop offset="1" stop-color="{rim}" stop-opacity="0.1"/></linearGradient>')
    s.append(f'<linearGradient id="{uid}-hl" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.45"/>'
             f'<stop offset="0.3" stop-color="#fff" stop-opacity="0"/></linearGradient>')
    s.append(f'<clipPath id="{uid}-clip"><path d="{TILE}"/></clipPath>')
    s.append(f'<filter id="{uid}-blur" x="-0.3" y="-0.3" width="1.6" height="1.6"><feGaussianBlur stdDeviation="7"/></filter>')
    s.append(f'<filter id="{uid}-blur2" x="-0.5" y="-0.5" width="2" height="2"><feGaussianBlur stdDeviation="14"/></filter>')
    s.append('</defs>')
    # contact shadow + slab
    s.append(f'<ellipse cx="256" cy="{BASE + SLAB - 2}" rx="206" ry="12" fill="#1a0c05" opacity="0.28" filter="url(#{uid}-blur)"/>')
    s.append(f'<path d="{TILE}" transform="translate(0 {SLAB})" fill="{pool}"/>')
    s.append(f'<path d="{TILE}" fill="url(#{uid}-g)"/>')
    s.append(f'<g clip-path="url(#{uid}-clip)">')
    s.append(f'<rect width="512" height="512" fill="url(#{uid}-pool)"/>')
    s.append(f'<rect width="512" height="512" fill="url(#{uid}-sheen)"/>')
    # glaze break: the glaze thins on the rim and shows lighter body
    s.append(f'<path d="{TILE}" fill="none" stroke="url(#{uid}-rim)" stroke-width="{14 if hero else 10}"/>')
    if hero:
        s.append(f'<path d="{TILE}" fill="none" stroke="url(#{uid}-hl)" stroke-width="3" transform="translate(0 6)"/>')
    s.append('</g>')
    if kind == 'suite':
        s.append(suite_interior(uid, full=True))
    else:
        g, _ = glyph(kind, 'full')
        style = f'--acc:{rim};--rib:{base}'
        # pressed shadow then bone glyph
        s.append(f'<g fill="{ink}" opacity="0.42" transform="translate(0 8)" style="--acc:{ink};--rib:{ink}" filter="url(#{uid}-soft)">{g}</g>'
                 .replace(f' filter="url(#{uid}-soft)"', ''))
        s.append(f'<g fill="{BONE}" style="{style}">{g}</g>')
    return ''.join(s)


def suite_interior(uid, full=True):
    s = []
    if full:
        s.append('<defs>')
        # warm light falloff spilling out of the mouth onto the glaze
        s.append(f'<radialGradient id="{uid}-spill" cx="256" cy="{MBASE}" r="210" gradientUnits="userSpaceOnUse">'
                 f'<stop offset="0" stop-color="#FF7A2A" stop-opacity="0.55"/><stop offset="0.45" stop-color="#E2541C" stop-opacity="0.18"/>'
                 f'<stop offset="1" stop-color="#E2541C" stop-opacity="0"/></radialGradient>')
        s.append(f'<radialGradient id="{uid}-glow" cx="256" cy="{MBASE - 10}" r="190" gradientUnits="userSpaceOnUse">'
                 f'<stop offset="0" stop-color="#FF9A3C"/><stop offset="0.35" stop-color="#D9461A"/>'
                 f'<stop offset="0.75" stop-color="#5A1A0A"/><stop offset="1" stop-color="#2A0D06"/></radialGradient>')
        s.append(f'<linearGradient id="{uid}-fb" x1="0" y1="0" x2="0" y2="1">'
                 f'<stop offset="0" stop-color="#FF6A26"/><stop offset="0.55" stop-color="#FF9533"/><stop offset="1" stop-color="#FFC24D"/></linearGradient>')
        s.append(f'<linearGradient id="{uid}-fk" x1="0" y1="0" x2="0" y2="1">'
                 f'<stop offset="0" stop-color="#C8320F"/><stop offset="1" stop-color="#F0601E"/></linearGradient>')
        s.append(f'<clipPath id="{uid}-mclip"><path d="{MOUTH}"/></clipPath>')
        s.append(f'<filter id="{uid}-hb" x="-0.5" y="-2" width="2" height="5"><feGaussianBlur stdDeviation="9"/></filter>')
        s.append('</defs>')
        s.append(f'<g clip-path="url(#{uid}-clip)"><rect width="512" height="512" fill="url(#{uid}-spill)"/></g>')
        # mouth: lip (thick wall shows lighter rust edge), then glowing interior
        s.append(f'<path d="{MOUTH}" fill="none" stroke="#E08A52" stroke-opacity="0.55" stroke-width="10" transform="translate(0 -3)"/>')
        s.append(f'<path d="{MOUTH}" fill="url(#{uid}-glow)"/>')
        s.append(f'<g clip-path="url(#{uid}-mclip)">')
        s.append(f'<path d="{MOUTH}" fill="none" stroke="#0E0503" stroke-opacity="0.55" stroke-width="22"/>')
        s.append(flame_group(256, MBASE - 4, 214, f'url(#{uid}-fb)', f'url(#{uid}-fk)', '#FFF3D6', 0.42))
        s.append(f'<ellipse cx="256" cy="{MBASE}" rx="104" ry="18" fill="#FFB24A" opacity="0.85" filter="url(#{uid}-hb)"/>')
        s.append('</g>')
        # hearth sill glint
        s.append(f'<rect x="{256 - MW / 2 + 14}" y="{MBASE}" width="{MW - 28}" height="5" rx="2.5" fill="#FFB25A" opacity="0.5"/>')
    return ''.join(s)


def svg(inner, size=512, vb='0 0 512 512'):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" width="{size}" height="{size}">{inner}</svg>'


# ---------------------------------------------------------------- flat tiles (24-32 px)
SMALL_TILE = kiln_arch(256, 12, 488, 488, P, Q, 44)


def icon_flat(kind, size=32, uid=None):
    """flat glaze, one lighter 1px rim on the crown, simplified glyph. Tuned for 20-40 px."""
    uid = uid or f'{kind}f{size}'
    fill, rim = FLAT[kind]
    px = 512 / size
    s = [f'<defs><clipPath id="{uid}-c"><path d="{SMALL_TILE}"/></clipPath>'
         f'<linearGradient id="{uid}-r" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{rim}"/>'
         f'<stop offset="0.45" stop-color="{rim}" stop-opacity="{1 if kind == "suite" else 0}"/>'
         f'<stop offset="1" stop-color="{rim}" stop-opacity="{1 if kind == "suite" else 0}"/></linearGradient></defs>']
    s.append(f'<path d="{SMALL_TILE}" fill="{fill}"/>')
    s.append(f'<path d="{SMALL_TILE}" fill="none" stroke="url(#{uid}-r)" stroke-width="{2 * px:.2f}" clip-path="url(#{uid}-c)"/>')
    if kind == 'suite':
        m = kiln_arch(256, 196, 268, 316, P, Q, 0)
        s.append(f'<path d="{m}" fill="#5A1C0A"/>')
        s.append(f'<defs><linearGradient id="{uid}-m" x1="0" y1="0" x2="0" y2="1"><stop offset="0.5" stop-color="#5A1C0A"/>'
                 f'<stop offset="1" stop-color="#E2541C"/></linearGradient></defs><path d="{m}" fill="url(#{uid}-m)"/>')
        s.append(flame_group(256, 506, 250, '#FF9A35', '#E2541C', '#FFE6AE', 0.46))
    else:
        g, _ = glyph(kind, 'small')
        top, base, pool, rimc, ink = GLAZE[kind]
        g = g.replace('var(--acc)', ink if kind == 'notes' else rimc)
        s.append(f'<g fill="{BONE}" transform="translate(256 300) scale(1.1) translate(-256 -300)">{g}</g>')
    return ''.join(s)


# ---------------------------------------------------------------- single-colour companions
def mono_glyph(kind, color='currentColor'):
    """glyph only (no tile) on the 512 grid, centred and enlarged: toolbar / themed icons."""
    if kind == 'suite':
        return mono_mark(color)
    g, _ = glyph(kind, 'small')
    # details that used a second colour become knock-outs
    g = g.replace(' fill="var(--acc)"', ' fill="none"').replace(' fill="var(--rib)"', ' fill="none"')
    return f'<g fill="{color}" transform="translate(256 256) scale(1.75) translate(-256 -300)">{g}</g>'


def mono_mark(color='currentColor', uid='mm'):
    """single-colour kiln: arch with the mouth cut out and the flame standing in it."""
    body = kiln_arch(256, 24, 464, 464, P, Q, 40)
    mouth = kiln_arch(256, 214, 272, 236, P, Q, 0)
    s = f'<path fill-rule="evenodd" fill="{color}" d="{body} {mouth}"/>'
    s += flame_mono(256, 440, 200, color, uid)
    return s


def flame_mono(cx, base, h, color, uid, gap=26):
    """single-colour flame: body + back tongue, separated by a knocked-out gap (mask) so the
    flick still reads as fire, not a droplet."""
    s = h / FLAME_H
    t = f'translate({f(cx)} {f(base)}) scale({s:.4f}) translate(-256 -432)'
    return (f'<defs><mask id="{uid}-fm" maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512">'
            f'<rect x="-100" y="-100" width="712" height="712" fill="#fff"/>'
            f'<path d="{FLAME_BODY}" fill="#000" stroke="#000" stroke-width="{gap}"/></mask></defs>'
            f'<g transform="{t}"><path d="{FLAME_BACK}" fill="{color}" mask="url(#{uid}-fm)"/>'
            f'<path d="{FLAME_BODY}" fill="{color}"/></g>')
