import sys, math
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from mock_common import *

CSS = '''
.rail { width: 196px; flex: none; padding: 10px 10px 10px 4px; display: flex; flex-direction: column; gap: 10px; overflow: hidden; }
.th { display: flex; gap: 8px; align-items: flex-start; }
.th .no { width: 16px; text-align: right; font-size: 11.5px; color: var(--kiln-faint); font-weight: 600; padding-top: 2px; }
.th .sl { width: 160px; height: 90px; border-radius: 6px; overflow: hidden; border: 1px solid var(--kiln-line); position: relative; flex: none; }
.th.on .sl { box-shadow: 0 0 0 2.5px var(--app), 0 0 0 5px color-mix(in srgb, var(--app) 22%, transparent); border-color: transparent; }
.th.on .no { color: var(--app-text); }
.stage { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 12px; min-width: 0; padding-top: 14px; }
.slide { width: 880px; height: 495px; border-radius: 6px; overflow: hidden; position: relative; background: #FFF6EA; color: #2B1C14; box-shadow: var(--kiln-shadow-page); flex: none; }
.notes { width: 880px; flex: 1; margin-bottom: 0; border-radius: 12px 12px 0 0; padding: 12px 16px; font-size: 13px; line-height: 1.55; color: var(--kiln-fg); }
.notes .h { font-size: 11px; letter-spacing: .08em; text-transform: uppercase; color: var(--kiln-faint); font-weight: 700; margin-bottom: 4px; }
.ideas { width: 236px; flex: none; padding: 14px 12px; display: flex; flex-direction: column; gap: 10px; }
.idea { border-radius: 10px; overflow: hidden; border: 1px solid var(--kiln-line); height: 118px; position: relative; }
.idea.on { box-shadow: 0 0 0 2px var(--app); border-color: transparent; }
.s-title { position: absolute; left: 64px; top: 58px; font: 800 52px/1.0 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; letter-spacing: -.02em; width: 430px; }
.s-kicker { position: absolute; left: 66px; top: 36px; font: 700 12px/1 var(--kiln-font-ui); letter-spacing: .14em; text-transform: uppercase; color: #B2431A; }
.s-body { position: absolute; left: 66px; top: 250px; width: 380px; font: 400 16px/1.5 var(--kiln-font-ui); color: #6B5547; }
.legend { position: absolute; left: 66px; bottom: 50px; display: grid; grid-template-columns: auto auto; gap: 10px 28px; font: 600 14px/1.2 var(--kiln-font-ui); }
.legend span { display: flex; align-items: center; gap: 9px; }
.legend i { width: 14px; height: 14px; border-radius: 7px 7px 2px 2px; }
.legend b { font: 800 18px/1 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; margin-left: 4px; }
.foot { position: absolute; left: 66px; right: 40px; bottom: 18px; display: flex; justify-content: space-between; font: 500 11px/1 var(--kiln-font-ui); color: #8F7A6A; }
.present { position: absolute; }
'''


def pie_svg(cx, cy, r, parts, pull=0, pull_idx=0, size=420, lab=True):
    out = ''
    a0 = -90
    total = sum(p[0] for p in parts)
    for i, (v, col, name) in enumerate(parts):
        a1 = a0 + v / total * 360
        mid = math.radians((a0 + a1) / 2)
        dx, dy = (math.cos(mid) * pull, math.sin(mid) * pull) if i == pull_idx else (0, 0)
        x0, y0 = cx + dx + r * math.cos(math.radians(a0)), cy + dy + r * math.sin(math.radians(a0))
        x1, y1 = cx + dx + r * math.cos(math.radians(a1)), cy + dy + r * math.sin(math.radians(a1))
        large = 1 if a1 - a0 > 180 else 0
        out += (f'<path d="M{cx + dx:.1f},{cy + dy:.1f} L{x0:.1f},{y0:.1f} A{r},{r} 0 {large} 1 {x1:.1f},{y1:.1f} Z" fill="{col}" '
                f'stroke="#FFF6EA" stroke-width="5" stroke-linejoin="round"/>')
        if lab:
            lx, ly = cx + dx + r * 0.64 * math.cos(mid), cy + dy + r * 0.64 * math.sin(mid)
            tc = '#1C0D06' if col in ('#E8A82C', '#F47B45', '#FFD08A') else '#FFF6EA'
            out += f'<text x="{lx:.1f}" y="{ly + 8:.1f}" text-anchor="middle" font-family="Fraunces" font-weight="800" font-size="24" fill="{tc}" style="font-variation-settings:\'SOFT\' 40">{v}%</text>'
        a0 = a1
    return out


PARTS = [(42, '#E05A27', 'Gas and kiln time'), (24, '#1D8A60', 'Clay and glaze'), (19, '#3159C6', 'Bursaries'), (15, '#E8A82C', 'Shelves and repairs')]


def main_slide():
    return f'''<div class="slide glowpage">
      <svg width="880" height="495" style="position:absolute;inset:0">
        <defs><radialGradient id="sg" cx="640" cy="250" r="300" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#FFD9B8"/><stop offset="1" stop-color="#FFF6EA" stop-opacity="0"/></radialGradient></defs>
        <rect width="880" height="495" fill="url(#sg)"/>
        {pie_svg(650, 245, 158, PARTS, pull=22, pull_idx=0)}
      </svg>
      <div class="s-kicker">Open Studio Weekend · 15–16 Nov</div>
      <div class="s-title">Every ticket fires the next kiln.</div>
      <div class="s-body">Ticket money goes straight back into the studio. Here is where last year’s £6,400 went.</div>
      <div class="legend">{''.join(f'<span><i style="background:{c}"></i>{n}</span>' for v, c, n in PARTS)}</div>
      <div class="foot"><span>Kiln Co-op</span><span>3</span></div>
    </div>'''


def thumbs():
    def mini(inner, bg):
        return f'<div class="sl" style="background:{bg}">{inner}</div>'
    t = []
    t.append(mini('<div style="position:absolute;right:-16px;bottom:-34px;width:84px;height:100px;border-radius:50% 50% 8px 8px/60% 60% 8px 8px;background:radial-gradient(circle at 50% 80%,#FF9A3C,#C2481C 45%,#3A2318 72%)"></div>'
                  '<div style="position:absolute;left:12px;top:22px;font:800 15px/1 Fraunces;color:#FFF1DF;font-variation-settings:\'SOFT\' 40">Open Studio<br>Weekend</div>'
                  '<div style="position:absolute;left:12px;top:62px;font:600 8px/1 Figtree;color:#F7976B;letter-spacing:.1em">15–16 NOV</div>', '#21140D'))
    t.append(mini('<div style="position:absolute;left:12px;top:14px;font:800 11px/1 Fraunces;color:#2B1C14">This year</div>' +
                  ''.join(f'<div style="position:absolute;left:12px;top:{34 + i * 14}px;width:{90 - i * 14}px;height:6px;border-radius:3px;background:#E4D6C4"></div>' for i in range(4)), '#FFF6EA'))
    t.append(mini(f'<svg width="160" height="90"><g transform="scale(0.1818)">{pie_svg(650, 245, 158, PARTS, 22, 0, lab=False)}</g></svg>'
                  '<div style="position:absolute;left:12px;top:12px;font:800 10px/1.05 Fraunces;color:#2B1C14;width:70px">Every ticket fires the next kiln.</div>', '#FFF6EA'))
    t.append(mini('<div style="position:absolute;inset:0;background:linear-gradient(135deg,#1D8A60,#0D4A34)"></div><div style="position:absolute;left:12px;bottom:12px;font:800 12px/1 Fraunces;color:#FFF6E8">The schedule</div>'
                  + ''.join(f'<div style="position:absolute;left:{14 + i * 34}px;top:22px;width:24px;height:24px;border-radius:12px 12px 4px 4px;background:rgba(255,246,232,{0.9 - i * 0.18})"></div>' for i in range(4)), '#1D8A60'))
    t.append(mini('<div style="position:absolute;left:12px;top:14px;font:800 11px/1 Fraunces;color:#2B1C14">Who does what</div>'
                  + ''.join(f'<div style="position:absolute;left:{12 + (i % 3) * 46}px;top:{34 + (i // 3) * 26}px;width:40px;height:20px;border-radius:5px;background:{c}"></div>' for i, c in enumerate(['#FBE8DE', '#E2F2EA', '#E7ECFA', '#FAF0D9', '#FBE8DE', '#E2F2EA'])), '#FFF6EA'))
    t.append(mini('<div style="position:absolute;left:0;right:0;top:30px;text-align:center;font:800 16px/1 Fraunces;color:#FFF1DF">See you<br>by the fire.</div>', '#C2481C'))
    return ''.join(f'<div class="th{" on" if i == 2 else ""}"><div class="no">{i + 1}</div>{s}</div>' for i, s in enumerate(t))


def ideas():
    a = f'''<div class="idea on" style="background:#FFF6EA"><svg width="212" height="118"><g transform="translate(110 8) scale(0.30) translate(-490 -80)">{pie_svg(650, 245, 158, PARTS, 22, 0, lab=False)}</g></svg>
      <div style="position:absolute;left:12px;top:16px;font:800 14px/1.05 Fraunces;color:#2B1C14;width:96px;font-variation-settings:'SOFT' 40">Every ticket fires the next kiln.</div></div>'''
    b = f'''<div class="idea" style="background:#21140D"><div style="position:absolute;left:12px;top:14px;font:800 14px/1.05 Fraunces;color:#FFF1DF;width:110px">Every ticket fires the next kiln.</div>
      <div style="position:absolute;left:12px;right:12px;bottom:14px;display:flex;height:16px;border-radius:8px;overflow:hidden">{''.join(f'<i style="flex:{v};background:{c}"></i>' for v, c, n in PARTS)}</div></div>'''
    c = f'''<div class="idea" style="background:#FBE8DE"><div style="position:absolute;left:12px;top:14px;font:800 30px/1 Fraunces;color:#B2431A">42%</div>
      <div style="position:absolute;left:12px;top:52px;font:600 11px/1.3 Figtree;color:#6B5547;width:120px">of every ticket buys gas and kiln time</div>
      <div style="position:absolute;right:-14px;bottom:-20px;width:90px;height:100px;border-radius:50% 50% 8px 8px/60% 60% 8px 8px;background:#E05A27"></div></div>'''
    return f'''<div class="ideas pane"><h4>Design ideas {I("x", "s14")}</h4><div class="faint" style="font-size:12px;margin-top:-4px">Three ways to lay out this slide.</div>{a}{b}{c}</div>'''


def build(theme):
    right = (f'<div class="btn ghost">{I("message-square", "s14")}Comments</div>'
             f'<div class="btn">{I("video", "s14")}Record</div>'
             f'<div class="btn solid">{I("play", "s14")}Present{I("chevron-down", "s14")}</div>'
             f'<div class="btn">{I("share-2", "s14")}Share</div>')
    shapes = ''.join(f'<div class="ib">{s}</div>' for s in [
        '<svg width="16" height="16" viewBox="0 0 16 16"><rect x="2" y="3" width="12" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>',
        '<svg width="16" height="16" viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>',
        '<svg width="16" height="16" viewBox="0 0 16 16"><path d="M3 13.5V8a5 5 0 0 1 10 0v5.5Z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/></svg>',
        '<svg width="16" height="16" viewBox="0 0 16 16"><path d="M8 2.5 13.5 13h-11Z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/></svg>',
        '<svg width="16" height="16" viewBox="0 0 16 16"><path d="M2.5 8h10m-3.5-3.5L12.5 8 9 11.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>'])
    rib = (clipboard_grp() +
           grp('Slides', big('layout-template', 'New slide', True) + f'<div class="col" style="gap:2px">{sm("panels-top-left", "Layout")}{sm("rotate-ccw", "Reset")}{sm("rows-3", "Section")}</div>') +
           grp('Font', f'''<div class="col"><div class="row" style="gap:4px">{dd("Fraunces", 118)}{dd("52", 52)}</div>
             <div class="row">{ib("bold", True)}{ib("italic")}{ib("underline")}{ib("baseline", bar="#E05A27")}{ib("highlighter", bar="#F5B73F")}</div></div>''') +
           grp('Paragraph', f'''<div class="col"><div class="row">{ib("list")}{ib("list-ordered")}{ib("indent-increase")}</div>
             <div class="row">{ib("align-left", True)}{ib("align-center")}{ib("align-right")}</div></div>''') +
           grp('Drawing', f'<div class="col"><div class="row">{shapes}</div><div class="row">{ib("paint-bucket", bar="#E05A27")}{ib("pen-line")}{ib("sparkle")}{ib("shapes")}{ib("chart-pie")}</div></div>') +
           grp('Arrange', f'<div class="col" style="gap:2px">{sm("layout-grid", "Arrange")}{sm("palette", "Quick styles")}{sm("chart-column", "Chart")}</div>') +
           grp('Kiln', big('wand-sparkles', 'Ideas', True)))
    stage = f'''<div class="stage">{main_slide()}<div class="notes pane"><div class="h">Speaker notes</div>
      Start with the number people remember: <b>42% goes on gas</b>. Then point at the jade slice: that’s every bag of clay we bought for members this year.</div></div>'''
    sb = status(f'<div class="it">Slide 3 of 12</div><div class="it">{I("languages", "s14")}English (UK)</div>{autosave("Saved just now")}<div class="it ok">{I("circle-check", "s14")}No issues</div>',
                f'<div class="it">{I("file-text", "s14")}Notes</div><div class="vm"><span class="on">{I("layout-panel-left", "s14")}</span><span>{I("layout-grid", "s14")}</span><span>{I("book-open", "s14")}</span><span>{I("monitor-play", "s14")}</span></div>{zoom("72%")}')
    body = (f'<div class="app">{titlebar("deck", "Open studio weekend", ".pptx")}'
            f'{tabs("Home", ["Home", "Insert", "Design", "Transitions", "Animations", "Slide Show", "Review", "View"], right)}'
            f'<div class="ribbon">{rib}</div><div class="work"><div class="rail">{thumbs()}</div>{stage}{ideas()}</div>{sb}</div>')
    return page('deck', theme, body, CSS)
