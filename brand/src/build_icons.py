"""Writes every icon SVG and the render job list for PNG sets, Android, macOS, Windows, favicon."""
import sys, json, os
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from icons import (icon_full, icon_flat, mono_glyph, mono_mark, svg, TILE, SMALL_TILE, GLAZE, FLAT, BONE,
                   glyph, kiln_arch, P, Q, suite_interior, flame_group)
import px16

OUT = '/home/muth/kiln-brand/final'
KINDS = ['suite', 'write', 'grid', 'deck', 'notes']
SIZES = [16, 24, 32, 48, 64, 128, 256, 512, 1024]
os.makedirs(f'{OUT}/icons/svg', exist_ok=True)
os.makedirs(f'{OUT}/icons/png', exist_ok=True)
os.makedirs(f'{OUT}/icons/mono', exist_ok=True)
os.makedirs(f'{OUT}/android', exist_ok=True)
os.makedirs(f'{OUT}/work/html', exist_ok=True)

jobs = []


def w(path, s):
    with open(path, 'w') as fh:
        fh.write(s)


def page(inner, wpx, hpx, bg='transparent'):
    return (f'<!doctype html><meta charset="utf-8"><style>html,body{{margin:0;background:{bg}}}'
            f'svg{{display:block}}</style>{inner}')


for k in KINDS:
    full = svg(icon_full(k, k, hero=True), 512)
    w(f'{OUT}/icons/svg/{k}.svg', full)                                   # hero, 128 px and up
    w(f'{OUT}/icons/svg/{k}-48.svg', svg(icon_full(k, k + 'm', hero=False), 512))  # 40-96 px
    w(f'{OUT}/icons/svg/{k}-32.svg', svg(icon_flat(k, 32), 512))         # 20-40 px
    w(f'{OUT}/icons/svg/{k}-16.svg', px16.to_svg(px16.build(k), 16))      # 16 px, pixel-fitted
    w(f'{OUT}/icons/mono/{k}.svg', svg(mono_glyph(k, '#2B1C14'), 512))
    w(f'{OUT}/icons/mono/{k}-currentColor.svg', svg(mono_glyph(k, 'currentColor'), 512))
    for s in SIZES:
        out = f'{OUT}/icons/png/{k}-{s}.png'
        if s == 16:
            px16.to_png(px16.build(k), out)
            continue
        if s <= 32:
            body = svg(icon_flat(k, s, f'{k}f'), s)
        elif s <= 96:
            body = svg(icon_full(k, k + 'm', hero=False), s)
        else:
            body = svg(icon_full(k, k, hero=True), s)
        hp = f'{OUT}/work/html/{k}-{s}.html'
        w(hp, page(body, s, s))
        jobs.append(dict(**{'in': hp}, out=out, w=s, h=s))

# ------------------------------------------------ Android adaptive icons (108dp @ 4x = 432 px)
AND_BG = {
    'suite': ('#3A2218', '#140B07'),
    'write': ('#4C72E0', '#2445A8'), 'grid': ('#30A577', '#146B4B'),
    'deck': ('#F47B45', '#C9461B'), 'notes': ('#F5B73F', '#CC860F'),
}


def android(k):
    a, b = AND_BG[k]
    bg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 432 432" width="432" height="432"><defs>'
          f'<linearGradient id="g" x1="0.2" y1="0" x2="0.5" y2="1"><stop offset="0" stop-color="{a}"/><stop offset="1" stop-color="{b}"/></linearGradient>'
          f'<radialGradient id="e" cx="0.5" cy="0.78" r="0.55"><stop offset="0" stop-color="#FF7A2A" stop-opacity="{0.38 if k == "suite" else 0}"/>'
          f'<stop offset="1" stop-color="#FF7A2A" stop-opacity="0"/></radialGradient></defs>'
          f'<rect width="432" height="432" fill="url(#g)"/><rect width="432" height="432" fill="url(#e)"/></svg>')
    # foreground: content lives inside the 66dp (264 px) safe circle, centred
    if k == 'suite':
        # the full kiln tile, scaled into the safe zone
        inner = icon_full('suite', 'afg', hero=True)
        fg_inner = f'<g transform="translate(216 222) scale(0.56) translate(-256 -262)">{inner}</g>'
    else:
        top, base, pool, rim, ink = GLAZE[k]
        arch = kiln_arch(256, 40, 440, 420, P, Q, 40)
        g, _ = glyph(k, 'small')
        g = g.replace('var(--acc)', rim)
        fg_inner = (f'<g transform="translate(216 216) scale(0.62) translate(-256 -262)">'
                    f'<path d="{arch}" fill="#fff" fill-opacity="0.12"/>'
                    f'<g fill="{ink}" opacity="0.35" transform="translate(0 8)">{g.replace(rim, ink)}</g>'
                    f'<g fill="{BONE}">{g}</g></g>')
    fg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 432 432" width="432" height="432">{fg_inner}</svg>'
    mono = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 432 432" width="432" height="432">'
            f'<g transform="translate(216 216) scale(0.5) translate(-256 -256)">{mono_glyph(k, "#000")}</g></svg>')
    return bg, fg, mono


for k in KINDS:
    bg, fg, mono = android(k)
    for name, s in (('background', bg), ('foreground', fg), ('monochrome', mono)):
        p = f'{OUT}/android/{k}-{name}.svg'
        w(p, s)
        hp = f'{OUT}/work/html/and-{k}-{name}.html'
        w(hp, page(s, 432, 432))
        jobs.append(dict(**{'in': hp}, out=f'{OUT}/android/{k}-{name}.png', w=432, h=432))
    # a preview of the composed icon under circle and squircle masks
    prev = ''
    for mask in ('50%', '28%'):
        prev += (f'<div style="width:432px;height:432px;border-radius:{mask};overflow:hidden;position:relative;display:inline-block;margin:0 12px 0 0">'
                 f'<div style="position:absolute;inset:0">{bg}</div><div style="position:absolute;inset:0">{fg}</div></div>')
    hp = f'{OUT}/work/html/and-{k}-preview.html'
    w(hp, page(prev, 900, 432))
    jobs.append(dict(**{'in': hp}, out=f'{OUT}/work/and-{k}-preview.png', w=900, h=432))

# ------------------------------------------------ macOS 1024 (824 body, 100 px margins, soft shadow)
for k in KINDS:
    body = icon_full(k, k + 'mac', hero=True)
    s = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">'
         f'<defs><filter id="sh" x="-0.2" y="-0.2" width="1.4" height="1.5"><feGaussianBlur stdDeviation="14"/></filter></defs>'
         f'<path d="{TILE}" transform="translate(100 118) scale(1.609)" fill="#000" opacity="0.32" filter="url(#sh)"/>'
         f'<g transform="translate(100 88) scale(1.609)">{body}</g></svg>')
    p = f'{OUT}/icons/macos/{k}-1024.svg'
    os.makedirs(f'{OUT}/icons/macos', exist_ok=True)
    w(p, s)
    hp = f'{OUT}/work/html/mac-{k}.html'
    w(hp, page(s, 1024, 1024))
    jobs.append(dict(**{'in': hp}, out=f'{OUT}/icons/macos/{k}-1024.png', w=1024, h=1024))

# ------------------------------------------------ Windows 256 (fills the canvas, tiny contact shadow)
os.makedirs(f'{OUT}/icons/windows', exist_ok=True)
for k in KINDS:
    s = svg(icon_full(k, k + 'win', hero=True), 256)
    hp = f'{OUT}/work/html/win-{k}.html'
    w(hp, page(s, 256, 256))
    jobs.append(dict(**{'in': hp}, out=f'{OUT}/icons/windows/{k}-256.png', w=256, h=256))

json.dump(jobs, open(f'{OUT}/work/icon-jobs.json', 'w'), indent=0)
print(len(jobs), 'jobs')
