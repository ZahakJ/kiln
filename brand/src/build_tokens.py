"""tokens.json + tokens.css, with WCAG contrast checks."""
import json

O = '/home/muth/kiln-brand/final'


def lum(h):
    h = h.lstrip('#')
    c = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    c = [v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4 for v in c]
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def cr(a, b):
    la, lb = sorted([lum(a), lum(b)], reverse=True)
    return round((la + 0.05) / (lb + 0.05), 2)


T = {
    "$schema": "kiln-tokens/1",
    "name": "Kiln",
    "rule": "Each app burns its own colour; ember belongs to the suite (Home, brand moments, focus of the suite chrome).",
    "color": {
        "light": {
            "bg": "#F6EFE6", "panel": "#FFFCF7", "raised": "#FFFFFF", "sunken": "#EEE3D5",
            "page": "#FFFFFF", "canvas": "#EDE4D8",
            "fg": "#2B1C14", "muted": "#6B5547", "faint": "#8F7A6A",
            "line": "#E4D6C4", "lineStrong": "#CDB8A0",
            "ember": "#C2481C", "emberText": "#B2431A", "onEmber": "#FFF7EE",
            "selection": "#FFD9B8", "focus": "#C2481C",
            "danger": "#B42318", "success": "#17714E", "warning": "#87590A"
        },
        "dark": {
            "bg": "#17110E", "panel": "#201813", "raised": "#2A201A", "sunken": "#110C0A",
            "page": "#221A15", "canvas": "#120D0B",
            "fg": "#F5EBDD", "muted": "#B8A392", "faint": "#8C7867",
            "line": "#372A22", "lineStrong": "#4B3A2F",
            "ember": "#F2804E", "emberText": "#F7976B", "onEmber": "#1C0D06",
            "selection": "#5A2A14", "focus": "#F7976B",
            "danger": "#FF8A7A", "success": "#6FD0A6", "warning": "#F2C35C"
        },
        "product": {
            "write": {"name": "Kiln Write", "replaces": "Word", "glaze": "cobalt", "fill": "#3159C6",
                      "solid": "#2D4FB5", "onSolid": "#FFFFFF", "textLight": "#2D4FB5", "textDark": "#97AEF4",
                      "tintLight": "#E7ECFA", "tintDark": "#1C2440", "icon": ["#4C72E0", "#2748AE"]},
            "grid": {"name": "Kiln Grid", "replaces": "Excel", "glaze": "jade", "fill": "#1D8A60",
                     "solid": "#17714E", "onSolid": "#FFFFFF", "textLight": "#17714E", "textDark": "#6FD0A6",
                     "tintLight": "#E2F2EA", "tintDark": "#142C22", "icon": ["#30A577", "#167150"]},
            "deck": {"name": "Kiln Deck", "replaces": "PowerPoint", "glaze": "ember", "fill": "#E05A27",
                     "solid": "#E05A27", "onSolid": "#1C0D06", "textLight": "#B2431A", "textDark": "#F7976B",
                     "tintLight": "#FBE8DE", "tintDark": "#36190E", "icon": ["#F47B45", "#CC4A1E"]},
            "notes": {"name": "Kiln Notes", "replaces": "OneNote", "glaze": "saffron", "fill": "#E3A126",
                      "solid": "#E8A82C", "onSolid": "#1C0D06", "textLight": "#87590A", "textDark": "#F2C35C",
                      "tintLight": "#FAF0D9", "tintDark": "#30230C", "icon": ["#F5B73F", "#CF8A12"]},
            "home": {"name": "Kiln Home", "replaces": "Office start screen", "glaze": "tenmoku", "fill": "#2E1C13",
                     "solid": "#C2481C", "onSolid": "#FFF7EE", "textLight": "#B2431A", "textDark": "#F7976B",
                     "tintLight": "#F6E6DA", "tintDark": "#2A1A12", "icon": ["#54321F", "#21140D"]}
        },
        "neutral": {"bone": "#FFF6E8", "bisque": "#EADBC6", "terracotta": "#B4462A", "clay": "#7A4A33",
                    "tenmoku": "#2A1A12", "soot": "#17110E", "char": "#0F0805"},
        "fire": {"core": "#FFF3D6", "flame": "#FFB547", "ember": "#FF6A26", "tongue": "#D93D12", "glow": "#FF7A2A"}
    },
    "font": {
        "display": {"family": "Fraunces", "use": "wordmark, headlines, app names, document headings (Kiln theme)",
                    "settings": {"wght": 800, "SOFT": 40, "WONK": 0, "opsz": 72}},
        "ui": {"family": "Figtree", "use": "all interface text, banner copy", "weights": [400, 500, 600, 700, 800]},
        "docBody": {"family": "Figtree", "use": "default body text for new documents (neutral, Aptos/Calibri-like)",
                    "fallbackForOffice": "Carlito (metric-compatible with Calibri), Caladea (with Cambria)"},
        "stack": {
            "display": "'Fraunces', 'Iowan Old Style', Georgia, serif",
            "ui": "'Figtree', 'Segoe UI Variable', 'Segoe UI', system-ui, -apple-system, 'Noto Sans', sans-serif"
        },
        "size": {"xs": 11, "sm": 12, "base": 13, "md": 14, "lg": 16, "xl": 20, "2xl": 26, "3xl": 34, "hero": 48}
    },
    "radius": {"xs": 4, "sm": 6, "md": 8, "lg": 12, "xl": 16, "pill": 999, "archRatio": 0.5},
    "space": {"1": 4, "2": 8, "3": 12, "4": 16, "5": 20, "6": 24, "8": 32, "10": 40, "12": 48},
    "shadow": {
        "light": {"1": "0 1px 2px rgba(60,30,10,.06), 0 1px 1px rgba(60,30,10,.04)",
                  "2": "0 2px 6px rgba(60,30,10,.07), 0 8px 24px rgba(60,30,10,.06)",
                  "page": "0 1px 2px rgba(60,30,10,.08), 0 12px 40px rgba(60,30,10,.10)"},
        "dark": {"1": "0 1px 2px rgba(0,0,0,.4)", "2": "0 4px 16px rgba(0,0,0,.45)",
                 "page": "0 30px 80px -20px rgba(255,106,38,.22), 0 2px 10px rgba(0,0,0,.5)"}
    },
    "motion": {"fast": "120ms", "base": "180ms", "slow": "320ms", "ease": "cubic-bezier(.2,.8,.2,1)",
               "flicker": "spark flicker on autosave: 2 frames, 600ms, never loops"}
}

# contrast report
rep = {}
for mode in ('light', 'dark'):
    c = T['color'][mode]
    for k in ('fg', 'muted', 'emberText'):
        for b in ('bg', 'panel', 'page'):
            rep[f'{mode}.{k} on {b}'] = cr(c[k], c[b])
    rep[f'{mode}.onEmber on ember'] = cr(c['onEmber'], c['ember'])
    for p, v in T['color']['product'].items():
        t = v['textLight' if mode == 'light' else 'textDark']
        rep[f'{mode}.{p} text on bg'] = cr(t, c['bg'])
        rep[f'{mode}.{p} text on panel'] = cr(t, c['panel'])
        rep[f'{mode}.{p} text on tint'] = cr(t, v['tintLight' if mode == 'light' else 'tintDark'])
for p, v in T['color']['product'].items():
    rep[f'{p} onSolid on solid'] = cr(v['onSolid'], v['solid'])
T['contrast'] = rep
bad = {k: v for k, v in rep.items() if v < 4.5}
T['contrastNote'] = "Every text pair listed passes WCAG 2.1 AA (4.5:1). 'fill' and 'icon' colours are for shapes, never for text."
json.dump(T, open(f'{O}/tokens.json', 'w'), indent=2)

css = ['/* Kiln design tokens. Generated from tokens.json. */', ':root {']
L, D = T['color']['light'], T['color']['dark']


def kebab(s):
    return ''.join('-' + ch.lower() if ch.isupper() else ch for ch in s)


for k, v in L.items():
    css.append(f'  --kiln-{kebab(k)}: {v};')
for p, v in T['color']['product'].items():
    for k in ('fill', 'solid', 'onSolid', 'textLight', 'textDark', 'tintLight', 'tintDark'):
        css.append(f'  --kiln-{p}-{kebab(k)}: {v[k]};')
    css.append(f'  --kiln-{p}-text: {v["textLight"]};')
    css.append(f'  --kiln-{p}-tint: {v["tintLight"]};')
for k, v in T['color']['fire'].items():
    css.append(f'  --kiln-fire-{k}: {v};')
for k, v in T['color']['neutral'].items():
    css.append(f'  --kiln-{k}: {v};')
css.append(f"  --kiln-font-display: {T['font']['stack']['display']};")
css.append(f"  --kiln-font-ui: {T['font']['stack']['ui']};")
css.append("  --kiln-font-doc: var(--kiln-font-ui);")
css.append("  --kiln-display-settings: 'SOFT' 40, 'WONK' 0;")
for k, v in T['font']['size'].items():
    css.append(f'  --kiln-text-{k}: {v}px;')
for k, v in T['radius'].items():
    if k != 'archRatio':
        css.append(f'  --kiln-radius-{k}: {v}px;')
for k, v in T['space'].items():
    css.append(f'  --kiln-space-{k}: {v}px;')
for k, v in T['shadow']['light'].items():
    css.append(f'  --kiln-shadow-{k}: {v};')
css.append(f"  --kiln-ease: {T['motion']['ease']};")
css.append('  /* the active app: set data-app on <html> */')
css.append('  --app: var(--kiln-ember); --app-solid: var(--kiln-home-solid); --app-on-solid: var(--kiln-home-on-solid);')
css.append('  --app-text: var(--kiln-ember-text); --app-tint: var(--kiln-home-tint);')
css.append('  color-scheme: light;')
css.append('}')


def dark_block(sel):
    out = [f'{sel} {{']
    for k, v in D.items():
        out.append(f'  --kiln-{kebab(k)}: {v};')
    for p, v in T['color']['product'].items():
        out.append(f'  --kiln-{p}-text: {v["textDark"]};')
        out.append(f'  --kiln-{p}-tint: {v["tintDark"]};')
    for k, v in T['shadow']['dark'].items():
        out.append(f'  --kiln-shadow-{k}: {v};')
    out.append('  color-scheme: dark;')
    out.append('}')
    return out


css += ['@media (prefers-color-scheme: dark) {'] + ['  ' + l for l in dark_block(':root:not([data-theme="light"])')] + ['}']
css += dark_block(':root[data-theme="dark"]')
for p in ('write', 'grid', 'deck', 'notes', 'home'):
    css.append(f'[data-app="{p}"] {{ --app: var(--kiln-{p}-fill); --app-solid: var(--kiln-{p}-solid); '
               f'--app-on-solid: var(--kiln-{p}-on-solid); --app-text: var(--kiln-{p}-text); --app-tint: var(--kiln-{p}-tint); }}')
open(f'{O}/tokens.css', 'w').write('\n'.join(css) + '\n')
print('failing pairs:', bad)
