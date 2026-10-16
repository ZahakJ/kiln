import sys
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from mock_common import *

CSS = '''
.fx { height: 40px; margin: 8px 10px 0; display: flex; align-items: center; gap: 8px; }
.namebox { width: 92px; height: 32px; border-radius: 8px; border: 1px solid var(--kiln-line); background: var(--kiln-panel); display: flex; align-items: center; justify-content: space-between; padding: 0 8px 0 10px; font-weight: 700; font-size: 13px; }
.fxbtn { display:flex; gap: 2px; color: var(--kiln-faint); }
.fxin { flex: 1; height: 32px; border-radius: 8px; border: 1px solid var(--kiln-line); background: var(--kiln-panel); display: flex; align-items: center; padding: 0 10px; gap: 10px; font: 500 13px/1 var(--kiln-font-ui); }
.fxin .fn { font-family: var(--kiln-font-display); font-style: italic; font-weight: 700; color: var(--app-text); font-size: 15px; }
.ref { color: var(--kiln-grid-text); background: var(--kiln-grid-tint); border-radius: 4px; padding: 2px 4px; font-weight: 700; }
.sheetwrap { flex: 1; margin: 8px 10px 0; display: flex; gap: 10px; min-height: 0; }
.sheet { flex: 1; background: var(--kiln-page); border: 1px solid var(--kiln-line); border-radius: 12px 12px 0 0; overflow: hidden; position: relative; }
table.g { border-collapse: separate; border-spacing: 0; font-size: 12.5px; table-layout: fixed; }
table.g th { height: 26px; background: var(--kiln-panel); color: var(--kiln-muted); font-weight: 600; font-size: 11.5px; border-right: 1px solid var(--kiln-line); border-bottom: 1px solid var(--kiln-line); }
table.g th.on { background: var(--kiln-grid-tint); color: var(--kiln-grid-text); box-shadow: inset 0 -2px 0 var(--app); }
table.g td.rh { background: var(--kiln-panel); color: var(--kiln-muted); text-align: center; font-weight: 600; font-size: 11.5px; border-right: 1px solid var(--kiln-line); width: 44px; }
table.g td.rh.on { background: var(--kiln-grid-tint); color: var(--kiln-grid-text); box-shadow: inset -2px 0 0 var(--app); }
table.g td { height: 28px; padding: 0 9px; border-right: 1px solid color-mix(in srgb, var(--kiln-line) 70%, transparent); border-bottom: 1px solid color-mix(in srgb, var(--kiln-line) 70%, transparent); white-space: nowrap; overflow: hidden; }
table.g td.n { text-align: right; font-variant-numeric: tabular-nums; }
table.g tr.hd td.h { background: var(--app-solid); color: var(--app-on-solid); font-weight: 700; font-size: 11.5px; letter-spacing: .04em; text-transform: uppercase; border-color: transparent; }
table.g tr.tt td.h { font-weight: 800; border-top: 2px solid var(--kiln-fg); }
table.g tr.title td:not(.rh) { font: 800 22px/1 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; letter-spacing: -.01em; border-color: transparent; height: 40px; }
table.g tr.sub td:not(.rh) { color: var(--kiln-muted); border-color: transparent; }
td.rng { background: color-mix(in srgb, var(--app) 9%, transparent); }
.selbox { position: absolute; border: 2.5px solid var(--app); border-radius: 2px; pointer-events: none; }
.selbox i { position: absolute; right: -6px; bottom: -6px; width: 9px; height: 9px; background: var(--app); border: 2px solid var(--kiln-page); border-radius: 2px; }
.rngbox { position: absolute; border: 1.5px dashed var(--app); border-radius: 2px; }
.pill { display: inline-block; padding: 2px 8px; border-radius: 999px; font-size: 11px; font-weight: 700; }
.chart { position: absolute; right: 22px; top: 300px; width: 400px; height: 270px; background: var(--kiln-panel); border: 1px solid var(--kiln-line); border-radius: 14px; box-shadow: var(--kiln-shadow-2); padding: 14px 16px; }
.chart h5 { font: 700 15px/1.2 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; }
.chart .sub { font-size: 11.5px; color: var(--kiln-faint); margin-bottom: 8px; }
.sheetbar { height: 38px; margin: 0 10px; display: flex; align-items: flex-start; gap: 4px; padding-left: 4px; }
.stab { height: 30px; padding: 0 16px; display:flex; align-items:center; gap: 6px; font-weight: 500; color: var(--kiln-muted); border-radius: 0 0 10px 10px; font-size: 12.5px; }
.stab.on { background: var(--kiln-page); color: var(--app-text); font-weight: 700; border: 1px solid var(--kiln-line); border-top: 0; margin-top: -1px; height: 32px; border-radius: 0 0 50% 50% / 0 0 16px 16px; padding: 0 22px; position: relative; }
.stab .dot { width: 8px; height: 8px; border-radius: 50%; }
.sadd { width: 30px; height: 30px; display: grid; place-items: center; color: var(--kiln-muted); }
'''

ROWS = [
    ('title', ['Studio budget 2026', '', '', '', '', '', '', '']),
    ('sub', ['Kiln Co-op · all figures in £', '', '', '', '', '', '', '']),
    ('', [''] * 8),
    ('hd', ['Item', 'Category', 'Q1', 'Q2', 'Q3', 'Q4', 'Total', 'Status']),
    ('', ['Clay, stoneware 25 kg', 'Materials', '420', '380', '455', '510', '1,765', 'ok']),
    ('', ['Glaze chemicals', 'Materials', '310', '125', '290', '140', '865', 'ok']),
    ('', ['Gas for the big kiln', 'Firing', '1,180', '640', '1,240', '1,320', '4,380', 'watch']),
    ('', ['Kiln shelves + posts', 'Equipment', '0', '560', '0', '0', '560', 'ok']),
    ('', ['Open studio weekend', 'Events', '0', '0', '0', '380', '380', 'plan']),
    ('tt', ['Total', '', '1,910', '1,705', '1,985', '2,350', '7,950', '']),
    ('', [''] * 8), ('', [''] * 8),
    ('', ['Members', '', '18', '19', '21', '24', '', '']),
    ('', ['Cost per member', '', '106.11', '89.74', '94.52', '97.92', '', '']),
    ('', [''] * 8), ('', [''] * 8), ('', [''] * 8), ('', [''] * 8), ('', [''] * 8), ('', [''] * 8), ('', [''] * 8),
]
COLW = [200, 110, 84, 84, 84, 84, 92, 92, 84, 84, 84, 84, 84, 84, 84]
STATUS = {'ok': ('On track', 'var(--kiln-grid-tint)', 'var(--kiln-grid-text)'),
          'watch': ('Watch', 'var(--kiln-deck-tint)', 'var(--kiln-deck-text)'),
          'plan': ('Planned', 'var(--kiln-notes-tint)', 'var(--kiln-notes-text)')}


def table():
    cols = 'ABCDEFGHIJKLMNO'
    h = f'<table class="g" style="width:{44 + sum(COLW)}px"><colgroup><col style="width:44px">' + ''.join(f'<col style="width:{w}px">' for w in COLW) + '</colgroup>'
    h += '<tr><th></th>' + ''.join(f'<th class="{"on" if c == "F" else ""}">{c}</th>' for c in cols) + '</tr>'
    for r, (cls, vals) in enumerate(ROWS, start=1):
        h += f'<tr class="{cls}"><td class="rh{" on" if r == 10 else ""}">{r}</td>'
        for c, v in enumerate(vals + [''] * (15 - len(vals))):
            num = c >= 2 and c <= 6 and v not in ('',) and cls not in ('title', 'sub', 'hd')
            klass = 'n' if num or (cls == 'hd' and 2 <= c <= 6) else ''
            if cls == "" and c == 6 and 5 <= r <= 9:
                klass += ' rng'
            if c == 7 and v in STATUS:
                t, bg, fg = STATUS[v]
                v = f'<span class="pill" style="background:{bg};color:{fg}">{t}</span>'
            if cls == 'title' and c == 0:
                h += f'<td colspan="4">{v}</td>'
                continue
            if cls == 'title' and c in (1, 2, 3):
                continue
            if c < 8: klass += ' h'
            h += f'<td class="{klass.strip()}">{v}</td>'
        h += '</tr>'
    return h + '</table>'


def chart(theme):
    vals = [1910, 1705, 1985, 2350]
    mx = 2600
    bars = ''
    for i, v in enumerate(vals):
        hgt = v / mx * 150
        x = 40 + i * 84
        col = '#1D8A60' if i != 3 else '#E05A27'
        bars += f'<rect x="{x}" y="{170 - hgt:.0f}" width="46" height="{hgt:.0f}" rx="8" fill="{col}"/>'
        bars += f'<rect x="{x}" y="{170 - hgt + 8:.0f}" width="46" height="{hgt - 8:.0f}" fill="{col}"/>'
        bars += f'<text x="{x + 23}" y="{164 - hgt:.0f}" text-anchor="middle" font-size="11" font-weight="700" fill="currentColor">£{v:,}</text>'
        bars += f'<text x="{x + 23}" y="188" text-anchor="middle" font-size="11" fill="var(--kiln-muted)">Q{i + 1}</text>'
    grid = ''.join(f'<line x1="30" x2="370" y1="{170 - g / mx * 150:.0f}" y2="{170 - g / mx * 150:.0f}" stroke="var(--kiln-line)" stroke-dasharray="3 4"/>' for g in (500, 1000, 1500, 2000, 2500))
    return f'''<div class="chart"><h5>Spend by quarter</h5><div class="sub">Q4 runs hot: the open studio weekend lands in November.</div>
      <svg width="368" height="196" style="font-family:var(--kiln-font-ui);color:var(--kiln-fg)">{grid}<line x1="30" x2="370" y1="170" y2="170" stroke="var(--kiln-line-strong)"/>{bars}</svg></div>'''


def build(theme):
    right = (f'<div class="btn ghost">{I("message-square", "s14")}Comments</div>'
             f'<div class="btn">{I("pencil", "s14")}Editing{I("chevron-down", "s14")}</div>'
             f'<div class="btn solid">{I("share-2", "s14")}Share</div>')
    rib = (clipboard_grp() +
           grp('Font', f'''<div class="col"><div class="row" style="gap:4px">{dd("Figtree", 120)}{dd("11", 52)}{ib("a-arrow-up")}{ib("a-arrow-down")}</div>
             <div class="row">{ib("bold", True)}{ib("italic")}{ib("underline")}{ib("grid-3x3")}{ib("paint-bucket", bar="#F5B73F")}{ib("baseline", bar="#E05A27")}</div></div>''') +
           grp('Alignment', f'''<div class="col"><div class="row">{ib("align-left")}{ib("align-center")}{ib("align-right", True)}{ib("wrap-text")}</div>
             <div class="row">{ib("indent-decrease")}{ib("indent-increase")}{ib("merge")}{ib("arrow-up-down")}</div></div>''') +
           grp('Number', f'''<div class="col"><div class="row">{dd("Currency £", 128)}</div>
             <div class="row">{ib("percent")}{ib("hash")}<div class="ib" style="font-weight:700;font-size:11px;width:34px">.0←</div><div class="ib" style="font-weight:700;font-size:11px;width:34px">.00→</div></div></div>''') +
           grp('Styles', f'<div class="col" style="gap:2px">{sm("palette", "Conditional formatting")}{sm("table-2", "Format as table")}{sm("paintbrush", "Cell styles")}</div>') +
           grp('Cells', f'<div class="col" style="gap:2px">{sm("plus", "Insert")}{sm("minus", "Delete")}{sm("sliders-horizontal", "Format")}</div>') +
           grp('Editing', f'<div class="col" style="gap:2px">{sm("sigma", "AutoSum")}{sm("arrow-down-wide-narrow", "Sort &amp; filter")}{sm("text-search", "Find")}</div>') +
           grp('Kiln', big('sparkles', 'Explain', True)))
    fx = f'''<div class="fx"><div class="namebox">G10{I("chevron-down", "s14")}</div>
      <div class="fxbtn"><div class="ib">{I("x", "s14")}</div><div class="ib">{I("check", "s14")}</div></div>
      <div class="fxin"><span class="fn">fx</span><span>=SUM(<span class="ref">G5:G9</span>)</span><span class="muted" style="margin-left:auto;font-size:12px">Total spend for 2026</span></div></div>'''
    # selection overlay: column G (index 7) row 10
    x_g = 44 + sum(COLW[:6])
    y10 = 27 + 40 + 28 * 8
    y5 = 27 + 40 + 28 * 3
    sheet = (f'<div class="sheetwrap"><div class="sheet">{table()}'
             f'<div class="rngbox" style="left:{x_g - 1}px;top:{y5 - 1}px;width:{COLW[6] + 1}px;height:{28 * 5 + 1}px"></div>'
             f'<div class="selbox" style="left:{x_g - 2}px;top:{y10 - 2}px;width:{COLW[6] + 3}px;height:31px"><i></i></div>'
             f'{chart(theme)}</div></div>')
    # fix: highlight the referenced range G not F
    sheet = sheet.replace('class="on">F<', 'class="">F<').replace('<th class="">G</th>', '<th class="on">G</th>')
    sbar = (f'<div class="sheetbar"><div class="sadd">{I("plus", "s14")}</div>'
            f'<div class="stab on"><span class="dot" style="background:var(--app)"></span>Budget</div>'
            f'<div class="stab"><span class="dot" style="background:#E05A27"></span>Firings</div>'
            f'<div class="stab"><span class="dot" style="background:#E3A126"></span>Clay orders</div><div class="stab">Members</div></div>')
    sb = status(f'<div class="it">Ready</div>{autosave("Saved just now")}<div class="it">{I("circle-check", "s14")}Accessibility: good to go</div>',
                f'<div class="it tnum">Average <b style="color:var(--kiln-fg)">£1,590</b></div><div class="it tnum">Count <b style="color:var(--kiln-fg)">5</b></div>'
                f'<div class="it tnum">Sum <b style="color:var(--kiln-fg)">£7,950</b></div><div class="vm"><span class="on">{I("table", "s14")}</span><span>{I("file-text", "s14")}</span></div>{zoom("100%")}')
    body = (f'<div class="app">{titlebar("grid", "Studio budget 2026", ".xlsx")}'
            f'{tabs("Home", ["Home", "Insert", "Layout", "Formulas", "Data", "Review", "View", "Help"], right)}'
            f'<div class="ribbon">{rib}</div>{fx}{sheet}{sbar}{sb}</div>')
    body = body.replace('<td class=" rng">', '<td class="rng">')
    return page('grid', theme, body, CSS)
