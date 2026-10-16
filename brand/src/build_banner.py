"""README banners (2000x600, dark + light) and the social preview (1280x640). All text outlined."""
import sys, random
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from icons import icon_full, kiln_arch, P, Q
from wordmark import wordmark
from text import text
from geo import spark, kiln_arch_open

O = '/home/muth/kiln-brand/final'
KERN = (-0.026, -0.016, -0.016, 0)
TAG1 = 'Where work gets made.'
TAG2 = 'An office suite with a little fire in it.'
SPEC = ['AGPL-3.0', 'Linux · Windows · macOS · Android', 'Opens .docx .xlsx .pptx as they are']
FR = {'wght': 700, 'SOFT': 40, 'WONK': 0, 'opsz': 48}

THEMES = {
    'dark': dict(card='#17110E', card2='#21160F', stroke='#FFFFFF', strokeop=0.07, ink='#FFF1DF', tag1='#FFF1DF',
                 tag2='#B8A392', spec='#F7976B', spark='#FF8A4C', heat='#FF7A2A', heatop=0.16, glow='#FF6A26',
                 glowop=0.30, ember_core='#FFD08A'),
    'light': dict(card='#F6EFE6', card2='#FBE9DA', stroke='#2B1C14', strokeop=0.08, ink='#2B1C14', tag1='#2B1C14',
                  tag2='#6B5547', spec='#B2431A', spark='#D9541E', heat='#C2481C', heatop=0.13, glow='#FF8A4C',
                  glowop=0.22, ember_core='#FF9A3C'),
}


def embers(t, area, n, seed, rmin=2, rmax=6, rise_from=None):
    rnd = random.Random(seed)
    x0, y0, x1, y1 = area
    out = ''
    for i in range(n):
        x = rnd.uniform(x0, x1); y = rnd.uniform(y0, y1)
        r = rnd.uniform(rmin, rmax)
        op = rnd.uniform(0.35, 0.9)
        if rnd.random() < 0.45:
            out += f'<path d="{spark(x, y, r * 1.9)}" fill="{t["spark"]}" opacity="{op:.2f}"/>'
        else:
            out += (f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r * 1.6:.1f}" fill="{t["glow"]}" opacity="{op * 0.35:.2f}" filter="url(#eb)"/>'
                    f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r * 0.55:.1f}" fill="{t["ember_core"]}" opacity="{op:.2f}"/>')
    return out


def heat_lines(t, cx, base, w0, h0, n=6, step=1.0, uid='h'):
    """concentric kiln arches radiating from the mark; they fade out toward the hearth"""
    out = (f'<defs><linearGradient id="{uid}-f" x1="0" y1="{base - h0 * (1 + n * 0.32 * step)}" x2="0" y2="{base}" gradientUnits="userSpaceOnUse">'
           f'<stop offset="0" stop-color="#fff"/><stop offset="0.55" stop-color="#fff" stop-opacity="0.8"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>'
           f'<mask id="{uid}-m" maskUnits="userSpaceOnUse" x="-2000" y="-2000" width="6000" height="6000"><rect x="-2000" y="-2000" width="6000" height="6000" fill="url(#{uid}-f)"/></mask></defs>'
           f'<g mask="url(#{uid}-m)">')
    for i in range(1, n + 1):
        k = 1 + i * 0.32 * step
        d = kiln_arch_open(cx, base - h0 * k, w0 * k, h0 * k, P, Q)
        op = t['heatop'] * (1 - (i - 1) / n) ** 1.3 * 1.4
        out += f'<path d="{d}" fill="none" stroke="{t["heat"]}" stroke-width="2" opacity="{op:.3f}"/>'
    return out + '</g>'


def spec_line(t, x, y, size=28):
    out = ''
    for i, s in enumerate(SPEC):
        if i:
            out += f'<path d="{spark(x + 20, y - size * 0.34, size * 0.3)}" fill="{t["spark"]}"/>'
            x += 40
        p, w = text(s, 'figtree', size, x, y, t['spec'], {'wght': 600})
        out += p
        x += w
    return out, x


def banner(theme, W=2000, H=600):
    t = THEMES[theme]
    uid = 'b' + theme[0]
    s = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">',
         '<title>Kiln: where work gets made</title><defs>',
         f'<clipPath id="card"><rect x="1" y="1" width="{W - 2}" height="{H - 2}" rx="44"/></clipPath>',
         f'<linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{t["card2"]}"/>'
         f'<stop offset="0.45" stop-color="{t["card"]}"/><stop offset="1" stop-color="{t["card"]}"/></linearGradient>',
         f'<radialGradient id="glow" cx="300" cy="420" r="420" gradientUnits="userSpaceOnUse">'
         f'<stop offset="0" stop-color="{t["glow"]}" stop-opacity="{t["glowop"]}"/>'
         f'<stop offset="0.5" stop-color="{t["glow"]}" stop-opacity="{t["glowop"] * 0.3:.3f}"/>'
         f'<stop offset="1" stop-color="{t["glow"]}" stop-opacity="0"/></radialGradient>',
         f'<radialGradient id="glow2" cx="1640" cy="300" r="380" gradientUnits="userSpaceOnUse">'
         f'<stop offset="0" stop-color="{t["glow"]}" stop-opacity="{t["glowop"] * 0.28:.3f}"/>'
         f'<stop offset="1" stop-color="{t["glow"]}" stop-opacity="0"/></radialGradient>',
         f'<linearGradient id="rule" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#FF6A26"/>'
         f'<stop offset="1" stop-color="#FFB547"/></linearGradient>',
         '<filter id="eb" x="-2" y="-2" width="5" height="5"><feGaussianBlur stdDeviation="3"/></filter>',
         '</defs>',
         '<g clip-path="url(#card)">',
         f'<rect width="{W}" height="{H}" fill="url(#bg)"/>',
         f'<rect width="{W}" height="{H}" fill="url(#glow)"/>',
         f'<rect width="{W}" height="{H}" fill="url(#glow2)"/>']
    # kiln mark with heat lines
    sc = 0.74
    mx, my = 300 - 256 * sc, 300 - 262 * sc
    tile_base = my + 472 * sc
    s.append(heat_lines(t, 300, tile_base, 440 * sc, 432 * sc, n=7, uid=uid + 'h'))
    s.append(embers(t, (170, 40, 470, 150), 9, 3))
    s.append(f'<g transform="translate({mx:.1f} {my:.1f}) scale({sc})">{icon_full("suite", uid + "k", hero=True)}</g>')
    # wordmark
    x0 = 560
    ws, ww, bb = wordmark(240, x0, 296, t['ink'], uid + 'w', kern=KERN)
    s.append(ws)
    # rule: an ember stroke and the four fires
    s.append(f'<rect x="{x0 + 4}" y="338" width="150" height="6" rx="3" fill="url(#rule)"/>')
    for i, c in enumerate(['#4C72E0', '#30A577', '#F47B45', '#F5B73F']):
        s.append(f'<circle cx="{x0 + 176 + i * 22}" cy="341" r="6" fill="{c}"/>')
    p, _ = text(TAG1, 'fraunces', 50, x0, 420, t['tag1'], FR); s.append(p)
    p, _ = text(TAG2, 'figtree', 36, x0, 478, t['tag2'], {'wght': 400}); s.append(p)
    p, _ = spec_line(t, x0, 548); s.append(p)
    # the four product tiles on the right, sitting on the wordmark baseline
    isc = 0.27
    gap = 148
    right = W - 110
    for i, k in enumerate(['write', 'grid', 'deck', 'notes']):
        cx = right - (3 - i) * gap - 256 * isc
        s.append(f'<g transform="translate({cx:.1f} {296 - 472 * isc:.1f}) scale({isc})">{icon_full(k, uid + k, hero=True)}</g>')
        p, _ = text(k.capitalize(), 'figtree', 22, cx + 256 * isc, 340, t['tag2'], {'wght': 600}, anchor='middle')
        s.append(p)
    s.append(embers(t, (1380, 40, 1900, 140), 10, 11, 1.6, 4.5))
    s.append('</g>')
    s.append(f'<rect x="1" y="1" width="{W - 2}" height="{H - 2}" rx="44" fill="none" stroke="{t["stroke"]}" stroke-opacity="{t["strokeop"]}" stroke-width="2"/>')
    s.append('</svg>')
    return ''.join(s)


def social(theme='dark', W=1280, H=640):
    t = THEMES[theme]
    uid = 's' + theme[0]
    s = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">',
         '<defs>',
         f'<linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{t["card2"]}"/>'
         f'<stop offset="0.6" stop-color="{t["card"]}"/></linearGradient>',
         f'<radialGradient id="glow" cx="640" cy="330" r="520" gradientUnits="userSpaceOnUse">'
         f'<stop offset="0" stop-color="{t["glow"]}" stop-opacity="{t["glowop"] * 0.9:.3f}"/>'
         f'<stop offset="0.55" stop-color="{t["glow"]}" stop-opacity="{t["glowop"] * 0.2:.3f}"/>'
         f'<stop offset="1" stop-color="{t["glow"]}" stop-opacity="0"/></radialGradient>',
         '<filter id="eb" x="-2" y="-2" width="5" height="5"><feGaussianBlur stdDeviation="3"/></filter>',
         '</defs>',
         f'<rect width="{W}" height="{H}" fill="url(#bg)"/><rect width="{W}" height="{H}" fill="url(#glow)"/>']
    # centred: kiln mark, heat lines, wordmark, tagline, four tiles
    sc = 0.48
    base = 246
    s.append(heat_lines(t, 640, base, 440 * sc, 432 * sc, n=7, step=1.2, uid=uid + 'h'))
    s.append(embers(t, (420, 30, 860, 110), 10, 5))
    s.append(f'<g transform="translate({640 - 256 * sc} {base - 472 * sc}) scale({sc})">{icon_full("suite", uid + "k", hero=True)}</g>')
    ws, ww, bb = wordmark(150, 0, 0, t['ink'], uid + 'w', kern=KERN)
    s.append(f'<g transform="translate({640 - ww / 2:.1f} 396)">{ws}</g>')
    p, _ = text(TAG1, 'fraunces', 38, 640, 462, t['tag1'], FR, anchor='middle'); s.append(p)
    p, _ = text(TAG2, 'figtree', 26, 640, 502, t['tag2'], {'wght': 400}, anchor='middle'); s.append(p)
    isc = 0.13
    for i, k in enumerate(['write', 'grid', 'deck', 'notes']):
        cx = 640 + (i - 1.5) * 84 - 256 * isc
        s.append(f'<g transform="translate({cx:.1f} {604 - 472 * isc:.1f}) scale({isc})">{icon_full(k, uid + k, hero=False)}</g>')
    s.append('</svg>')
    return ''.join(s)


if __name__ == '__main__':
    import json
    open(f'{O}/banner.svg', 'w').write(banner('dark'))
    open(f'{O}/banner-light.svg', 'w').write(banner('light'))
    open(f'{O}/social-preview.svg', 'w').write(social('dark'))
    jobs = []
    for n, w_, h_ in (('banner', 2000, 600), ('banner-light', 2000, 600), ('social-preview', 1280, 640)):
        hp = f'{O}/work/html/{n}.html'
        open(hp, 'w').write(f'<!doctype html><style>html,body{{margin:0;background:transparent}}svg{{display:block}}</style>'
                            + open(f'{O}/{n}.svg').read())
        jobs.append({'in': hp, 'out': f'{O}/{n}.png', 'w': w_, 'h': h_, 'transparent': n != 'social-preview'})
    json.dump(jobs, open(f'{O}/work/banner-jobs.json', 'w'))
