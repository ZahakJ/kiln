"""Brand board (2400 x 1720) and the icon size-check sheet."""
import sys, json
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from mock_common import flame, spark, I

O = '/home/muth/kiln-brand/final'
T = json.load(open(f'{O}/tokens.json'))
K = ['suite', 'write', 'grid', 'deck', 'notes']
R = '../..'

FONTS = f'''@font-face {{ font-family: Figtree; src: url({R}/fonts/Figtree-VF.ttf); font-weight: 300 900; }}
@font-face {{ font-family: Fraunces; src: url({R}/fonts/Fraunces-VF.ttf); font-weight: 100 900; }}'''


def sizecheck():
    bgs = [('#F6EFE6', 'bone'), ('#FFFFFF', 'white'), ('#2D4FB5', 'cobalt'), ('#17110E', 'soot')]
    h = f'<!doctype html><html><head><meta charset="utf-8"><style>{FONTS} body{{margin:0;font:600 13px Figtree;width:1960px}} .r{{display:flex;align-items:flex-end;gap:12px;padding:20px 28px;position:relative}} .k{{display:flex;align-items:flex-end;gap:9px;margin-right:22px}} img{{display:block}} .t{{position:absolute;right:24px;top:12px;opacity:.6}} .z img{{image-rendering:pixelated}}</style></head><body>'
    for bg, n in bgs:
        col = '#fff' if n in ('cobalt', 'soot') else '#2B1C14'
        h += f'<div class="r" style="background:{bg};color:{col}"><span class="t">{n} · 128 64 48 32 24 16</span>'
        for k in K:
            h += '<div class="k">' + ''.join(f'<img src="{R}/icons/png/{k}-{s}.png" width="{s}" height="{s}">' for s in (128, 64, 48, 32, 24, 16)) + '</div>'
        h += '</div>'
    for bg, n in (bgs[0], bgs[3]):
        col = '#fff' if n == 'soot' else '#2B1C14'
        h += f'<div class="r z" style="background:{bg};color:{col}"><span class="t">{n} · 32 24 16 at 4×</span>'
        for k in K:
            h += '<div class="k">' + ''.join(f'<img src="{R}/icons/png/{k}-{s}.png" width="{s * 4}" height="{s * 4}">' for s in (32, 24, 16)) + '</div>'
        h += '</div>'
    open(f'{O}/work/html/sizecheck.html', 'w').write(h + '</body></html>')


def board():
    P = T['color']['product']
    sw = ''
    for k in ('write', 'grid', 'deck', 'notes'):
        v = P[k]
        sw += f'''<div class="sw"><div class="chip" style="background:{v["solid"]};color:{v["onSolid"]}"><b>{v["glaze"].capitalize()}</b><span>{v["name"]}</span></div>
          <div class="hex"><span><i style="background:{v["fill"]}"></i>fill {v["fill"]}</span><span><i style="background:{v["textLight"]}"></i>text {v["textLight"]}</span>
          <span><i style="background:{v["textDark"]}"></i>dark {v["textDark"]}</span><span class="aa">AA {T["contrast"]["light." + k + " text on bg"]}:1</span></div></div>'''
    neutrals = ''.join(f'<div class="nt" style="background:{c};color:{"#2B1C14" if n in ("bone", "bisque") else "#FFF1DF"}"><b>{n}</b>{c}</div>'
                       for n, c in (('bone', '#FFF6E8'), ('bisque', '#EADBC6'), ('terracotta', '#B4462A'), ('clay', '#7A4A33'), ('tenmoku', '#2A1A12'), ('soot', '#17110E')))
    fam = ''.join(f'<div class="fi"><img src="{R}/icons/png/{k}-256.png"><b>{n}</b><span>{r}</span></div>'
                  for k, n, r in (('suite', 'Kiln', 'Home · the suite'), ('write', 'Write', 'replaces Word'), ('grid', 'Grid', 'replaces Excel'),
                                  ('deck', 'Deck', 'replaces PowerPoint'), ('notes', 'Notes', 'replaces OneNote')))
    small = ''
    for bg in ('#F6EFE6', '#17110E'):
        small += f'<div class="strip" style="background:{bg}">' + ''.join(
            f'<span>{"".join(f"<img src={R}/icons/png/{k}-{s}.png width={s} height={s}>" for s in (32, 24, 16))}</span>' for k in K) + '</div>'
    html = f'''<!doctype html><html><head><meta charset="utf-8"><style>{FONTS}
    * {{ box-sizing: border-box; }}
    body {{ margin: 0; width: 2400px; height: 1560px; background: #EFE5D8; font: 15px/1.45 Figtree; color: #2B1C14; padding: 56px 60px; position: relative; }}
    .grid {{ display: grid; gap: 24px; }}
    .card {{ border-radius: 28px; background: #FFFCF7; padding: 34px 38px; position: relative; overflow: hidden; box-shadow: 0 1px 2px rgba(60,30,10,.06), 0 18px 40px -24px rgba(60,30,10,.25); }}
    .cap {{ font: 800 12px/1 Figtree; letter-spacing: .16em; text-transform: uppercase; color: #B2431A; display: flex; gap: 8px; align-items: center; margin-bottom: 20px; }}
    .hero {{ background: #17110E; color: #FFF1DF; padding: 0; }}
    .hero img.bn {{ width: 100%; display: block; }}
    .fam {{ display: flex; justify-content: space-between; align-items: flex-end; margin-top: 14px; }}
    .fi {{ text-align: center; width: 150px; }} .fi img {{ width: 150px; height: 150px; display: block; }}
    .fi b {{ display: block; font: 800 22px/1 Fraunces; font-variation-settings: 'SOFT' 40; margin-top: 8px; }} .fi span {{ font-size: 13px; color: #8F7A6A; }}
    .strip {{ display: flex; justify-content: space-between; align-items: flex-end; padding: 14px 18px; border-radius: 14px; margin-top: 14px; }}
    .strip span {{ display: flex; align-items: flex-end; gap: 7px; }} .strip img {{ display: block; }}
    .sw {{ display: flex; gap: 16px; align-items: center; margin-bottom: 10px; }}
    .sw .chip {{ width: 180px; height: 66px; border-radius: 37px 37px 10px 10px / 30px 30px 10px 10px; padding: 0 16px 12px; display: flex; flex-direction: column; justify-content: flex-end; }}
    .sw .chip b {{ font: 800 20px/1 Fraunces; font-variation-settings: 'SOFT' 40; }} .sw .chip span {{ font-size: 12px; opacity: .85; }}
    .hex {{ display: grid; grid-template-columns: auto auto; gap: 4px 18px; font-size: 13px; color: #6B5547; }}
    .hex span {{ display: flex; align-items: center; gap: 7px; }} .hex i {{ width: 12px; height: 12px; border-radius: 6px 6px 2px 2px; }}
    .hex .aa {{ color: #17714E; font-weight: 700; }}
    .nts {{ display: flex; gap: 8px; margin-top: 6px; }}
    .nt {{ flex: 1; height: 58px; border-radius: 12px; padding: 10px 12px; font-size: 12px; display: flex; flex-direction: column; justify-content: flex-end; box-shadow: inset 0 0 0 1px rgba(0,0,0,.06); }}
    .nt b {{ font-size: 13px; }}
    .type .big {{ font: 800 100px/0.95 Fraunces; font-variation-settings: 'SOFT' 40; letter-spacing: -.03em; }}
    .type .fig {{ font: 700 56px/1 Figtree; letter-spacing: -.01em; margin-top: 18px; }}
    .type p {{ color: #6B5547; margin: 8px 0 0; font-size: 14px; }}
    .shot {{ border-radius: 18px; overflow: hidden; display: block; width: 100%; box-shadow: 0 20px 40px -24px rgba(40,20,5,.45); }}
    .lbl {{ position: absolute; left: 18px; bottom: 16px; font: 700 12px/1 Figtree; color: #FFF1DF; background: rgba(23,17,14,.82); padding: 7px 10px; border-radius: 8px; }}
    .voice {{ font: 800 34px/1.1 Fraunces; font-variation-settings: 'SOFT' 40; letter-spacing: -.01em; }}
    .voice small {{ display: block; font: 400 15px/1.5 Figtree; color: #6B5547; margin-top: 10px; letter-spacing: 0; }}
    .foot {{ position: absolute; left: 60px; right: 60px; bottom: 22px; display: flex; justify-content: space-between; font-size: 13px; color: #8F7A6A; }}
    </style></head><body>
    <div class="grid" style="grid-template-columns: 1380px 1fr;">
      <div class="card hero"><img class="bn" src="{R}/banner.png"></div>
      <div class="card"><div class="cap">{spark(12, "#C2481C")}App family · glazed kiln arches</div><div class="fam">{fam}</div>
        <div style="display:flex;gap:14px;margin-top:30px;align-items:stretch">
          <div style="flex:1.2;background:#F6EFE6;border-radius:16px;display:grid;place-items:center;height:110px"><img src="{R}/lockup-light.svg" style="height:58px"></div>
          <div style="flex:1.2;background:#17110E;border-radius:16px;display:grid;place-items:center"><img src="{R}/lockup.svg" style="height:58px"></div>
          <div style="flex:1;background:#E05A27;border-radius:16px;display:grid;place-items:center"><img src="{R}/lockup-mono-bone.svg" style="height:52px"></div>
        </div></div>
    </div>
    <div class="grid" style="grid-template-columns: 760px 470px 640px 1fr; margin-top: 24px; height: 470px;">
      <div class="card" style="padding-top:28px"><div class="cap">{spark(12, "#C2481C")}Glazes and neutrals · text tuned for AA</div>{sw}<div class="nts">{neutrals}</div></div>
      <div class="card type"><div class="cap">{spark(12, "#C2481C")}Type</div><div class="big">Fraunces</div><p>Display, wordmark, headings · 800 · SOFT 40 · OFL</p><div class="fig">Figtree</div><p>Interface and document body · 400–800 · OFL</p>
        <div class="voice" style="margin-top:26px">The kiln is warm.<small>Voice: plain, warm, a little playful. Say what happens. Never “empower”.</small></div></div>
      <div class="card"><div class="cap">{spark(12, "#C2481C")}Every size · bone, soot</div>{small}
        <div style="display:flex;gap:16px;align-items:center;margin-top:16px"><img src="{R}/icons/png/suite-16.png" style="width:64px;image-rendering:pixelated"><img src="{R}/icons/png/write-16.png" style="width:64px;image-rendering:pixelated"><img src="{R}/icons/png/grid-16.png" style="width:64px;image-rendering:pixelated"><img src="{R}/icons/png/deck-16.png" style="width:64px;image-rendering:pixelated"><img src="{R}/icons/png/notes-16.png" style="width:64px;image-rendering:pixelated">
        <span style="font-size:13px;color:#8F7A6A;width:150px">16 px tiles drawn pixel by pixel</span></div>
        <div style="display:flex;gap:14px;align-items:center;margin-top:16px">{''.join(f'<img src="{R}/icons/mono/{k}.svg" width="34" height="34">' for k in K)}<span style="font-size:13px;color:#8F7A6A">single-colour set: themed icons, favicons, toolbars</span></div></div>
      <div class="card" style="padding:18px;display:flex;justify-content:center;background:#2A1A12"><img src="{R}/mockups/phone-light.png" style="height:100%;border-radius:22px"></div>
    </div>
    <div class="grid" style="grid-template-columns: repeat(3, 1fr); margin-top: 24px;">
      <div style="position:relative"><img class="shot" src="{R}/mockups/home-dark.png"><span class="lbl">Kiln Home · dark</span></div>
      <div style="position:relative"><img class="shot" src="{R}/mockups/write-light.png"><span class="lbl">Kiln Write · light</span></div>
      <div style="position:relative"><img class="shot" src="{R}/mockups/deck-dark.png"><span class="lbl">Kiln Deck · dark</span></div>
    </div>
    <div class="foot"><span>kiln · where work gets made · an office suite with a little fire in it</span><span>AGPL-3.0 · Linux · Windows · macOS · Android</span></div>
    </body></html>'''
    open(f'{O}/work/html/board.html', 'w').write(html)


if __name__ == '__main__':
    sizecheck()
    board()
    json.dump([{'in': f'{O}/work/html/board.html', 'out': f'{O}/board.png', 'w': 2400, 'h': 1560, 'transparent': False},
               {'in': f'{O}/work/html/sizecheck.html', 'out': f'{O}/icons/size-check.png', 'w': 1960, 'h': 1010, 'transparent': False}],
              open(f'{O}/work/board-jobs.json', 'w'))
