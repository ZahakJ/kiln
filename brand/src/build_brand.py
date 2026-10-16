"""Mark, wordmark, lockups (SVG, all text outlined)."""
import sys, os, json
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from icons import icon_full, mono_mark, svg, TOP, BASE, SLAB
from wordmark import wordmark, glyph_runs, AXES

O = '/home/muth/kiln-brand/final'
INK = '#2B1C14'
BONE = '#FFF1DF'
KERN = (-0.026, -0.016, -0.016, 0)


def w(p, s):
    open(p, 'w').write(s)


def wm_metrics(size=200):
    gls, width = glyph_runs(['k', 'dotlessi', 'l', 'n'], size, 0, 0, list(KERN), AXES)
    asc = -min(g['bbox'][1] for g in gls)       # k / l ascender height
    xh = -gls[3]['bbox'][1]                      # n height
    return width, asc, xh


def wordmark_svg(ink, uid, size=200, pad=6):
    s, width, (x0, top, x1, base) = wordmark(size, pad, 0, ink, uid, kern=KERN)
    # bbox: flame top .. baseline (+ serif overshoot)
    t = top - 2
    h = -t + size * 0.02
    vb = f'{-0:.0f} {t:.2f} {width + 2 * pad:.2f} {h:.2f}'
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" width="{width + 2*pad:.0f}" height="{h:.0f}">'
            f'<title>Kiln</title>{s}</svg>'), width, h, t


def lockup(ink, uid, mono=False):
    size = 200
    width, asc, xh = wm_metrics(size)
    # mark: 512 grid tile spans y=TOP..BASE+SLAB; scale so the tile equals 1.32 x ascender
    th = (BASE + SLAB) - TOP
    sc = asc * 1.32 / th
    mark_w = 512 * sc
    gap = asc * 0.34
    # tile visible width is 440/512 of the box; tighten the left side
    left = -(512 - 440) / 2 * sc
    mx = left
    my = -(BASE + SLAB) * sc + size * 0.012  # sit the slab on the baseline
    if mono:
        ms = sc * th / 464
        mk = f'<g transform="translate({mx + (sc - ms) * 256:.2f} {-488 * ms + size * 0.012:.2f}) scale({ms:.4f})">{mono_mark(ink, uid + "mm")}</g>'
    else:
        mk = f'<g transform="translate({mx:.2f} {my:.2f}) scale({sc:.4f})">{icon_full("suite", uid + "m", hero=True)}</g>'
    wx = mx + (512 - (512 - 440) / 2) * sc + gap
    ws, wwid, _ = wordmark(size, wx, 0, ink, uid, kern=KERN, mode='mono' if mono else 'color')
    top = TOP * sc + my - 6
    right = wx + wwid
    bottom = size * 0.05
    vb = f'0 {top:.2f} {right:.2f} {bottom - top:.2f}'
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" width="{right:.0f}" height="{bottom - top:.0f}">'
            f'<title>Kiln</title>{mk}{ws}</svg>')


if __name__ == '__main__':
    w(f'{O}/logo.svg', svg(icon_full('suite', 'logo', hero=True), 512).replace('<svg ', '<svg role="img" aria-label="Kiln" ', 1))
    w(f'{O}/logo-mono.svg', svg(mono_mark(INK), 512))
    w(f'{O}/logo-mono-bone.svg', svg(mono_mark(BONE), 512))
    w(f'{O}/logo-mono-currentColor.svg', svg(mono_mark('currentColor'), 512))
    s, *_ = wordmark_svg(BONE, 'wd'); w(f'{O}/wordmark.svg', s)
    s, *_ = wordmark_svg(INK, 'wl'); w(f'{O}/wordmark-light.svg', s)
    w(f'{O}/lockup.svg', lockup(BONE, 'ld'))
    w(f'{O}/lockup-light.svg', lockup(INK, 'll'))
    w(f'{O}/lockup-mono.svg', lockup(INK, 'lm', mono=True))
    w(f'{O}/lockup-mono-bone.svg', lockup(BONE, 'lmb', mono=True))
    print('ok')
