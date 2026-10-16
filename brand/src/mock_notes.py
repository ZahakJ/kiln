import sys
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from mock_common import *

CSS = '''
.secs { width: 228px; flex: none; padding: 12px 10px; display: flex; flex-direction: column; gap: 8px; }
.nbk { display: flex; align-items: center; gap: 10px; padding: 8px; border-radius: 10px; background: var(--kiln-notes-tint); }
.nbk .cv { width: 28px; height: 34px; border-radius: 4px 8px 8px 4px; background: linear-gradient(90deg, #B97A0C 0 5px, #E8A82C 5px); box-shadow: inset -2px 0 0 rgba(0,0,0,.08); }
.nbk b { font-size: 13px; display: block; } .nbk span { font-size: 11.5px; color: var(--kiln-muted); }
.sec { display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-radius: 9px; font-size: 13px; font-weight: 500; }
.sec .tb2 { width: 10px; height: 14px; border-radius: 5px 5px 2px 2px; flex: none; }
.sec.on { background: var(--kiln-raised); box-shadow: var(--kiln-shadow-1); font-weight: 700; }
.sec .c { margin-left: auto; font-size: 11.5px; color: var(--kiln-faint); }
.pages { width: 262px; flex: none; padding: 12px 10px; display: flex; flex-direction: column; gap: 4px; }
.pgi { padding: 9px 11px; border-radius: 10px; }
.pgi b { display: block; font-size: 13px; font-weight: 600; }
.pgi span { display: block; font-size: 11.5px; color: var(--kiln-faint); margin-top: 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.pgi.on { background: var(--kiln-notes-tint); position: relative; }
.pgi.on::before { content:''; position:absolute; left: 0; top: 10px; bottom: 10px; width: 3px; border-radius: 2px; background: var(--app); }
.pgi.on b { color: var(--kiln-notes-text); }
.pgi.sub { padding-left: 26px; }
.leaf { flex: 1; position: relative; overflow: hidden; background: var(--kiln-page); border-radius: 14px 14px 0 0; border: 1px solid var(--kiln-line); border-bottom: 0;
  background-image: radial-gradient(color-mix(in srgb, var(--kiln-line-strong) 70%, transparent) 1px, transparent 1.2px); background-size: 22px 22px; background-position: 11px 11px; }
.leaf h1 { position: absolute; left: 56px; top: 40px; font: 800 34px/1.1 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; letter-spacing: -.015em; padding-bottom: 8px; border-bottom: 2px solid var(--kiln-line-strong); min-width: 520px; background: var(--kiln-page); }
.leaf .when { position: absolute; left: 56px; top: 102px; font-size: 12px; color: var(--kiln-faint); display: flex; gap: 16px; background: var(--kiln-page); padding-right: 6px; }
.box { position: absolute; background: var(--kiln-page); padding: 8px 10px; border-radius: 8px; }
.box.sel { box-shadow: 0 0 0 1px var(--kiln-line-strong); }
.box.sel::before { content: ''; position: absolute; left: 0; right: 0; top: -9px; height: 8px; border-radius: 4px 4px 0 0; background: var(--kiln-sunken); border: 1px solid var(--kiln-line-strong); border-bottom: 0; }
.box h3 { font: 700 17px/1.2 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; margin-bottom: 8px; }
.todo { display: flex; align-items: center; gap: 10px; font-size: 14px; line-height: 1.5; padding: 3px 0; }
.todo .cb { width: 17px; height: 17px; border-radius: 5px; border: 1.75px solid var(--kiln-line-strong); flex: none; display: grid; place-items: center; }
.todo.done .cb { background: var(--app-solid); border-color: transparent; color: var(--app-on-solid); }
.todo.done span { color: var(--kiln-faint); text-decoration: line-through; text-decoration-color: color-mix(in srgb, var(--kiln-faint) 60%, transparent); }
.tagimp { display: inline-flex; align-items: center; gap: 5px; font-size: 11px; font-weight: 700; color: var(--kiln-deck-text); background: var(--kiln-deck-tint); padding: 2px 8px; border-radius: 999px; margin-left: 6px; }
table.mini { border-collapse: collapse; font-size: 13px; }
table.mini td, table.mini th { border: 1px solid var(--kiln-line-strong); padding: 6px 12px; text-align: left; }
table.mini th { background: var(--kiln-notes-tint); font-size: 11.5px; text-transform: uppercase; letter-spacing: .05em; color: var(--kiln-notes-text); }
table.mini td.n { text-align: right; font-variant-numeric: tabular-nums; }
.ink { position: absolute; }
.sticky { position: absolute; width: 210px; padding: 14px 16px; background: #FCE6A8; color: #3B2A10; border-radius: 4px 4px 4px 18px; transform: rotate(2deg); box-shadow: 0 10px 24px -8px rgba(80,50,0,.35); font: 500 14px/1.45 var(--kiln-font-ui); }
.sticky b { font: 800 15px/1.2 var(--kiln-font-display); display: block; margin-bottom: 4px; }
.audio { display: inline-flex; align-items: center; gap: 10px; padding: 8px 12px 8px 8px; border-radius: 12px; border: 1px solid var(--kiln-line); background: var(--kiln-raised); font-size: 12.5px; }
.audio .pl { width: 28px; height: 28px; border-radius: 50%; background: var(--app-solid); color: var(--app-on-solid); display: grid; place-items: center; }
.wave { display: flex; align-items: center; gap: 2px; height: 22px; }
.wave i { width: 3px; border-radius: 2px; background: var(--kiln-line-strong); }
.wave i.p { background: var(--app); }
'''


def ink_pot():
    # a quick pen sketch of a jar, with dimension notes
    return '''<svg class="ink" style="left:470px;top:340px" width="260" height="230" viewBox="0 0 260 230" fill="none" stroke-linecap="round" stroke-linejoin="round">
      <path d="M78 40 C70 52 74 60 70 72 C56 98 46 128 52 160 C58 190 84 204 118 205 C152 206 176 194 182 162 C188 130 176 98 160 72 C156 62 160 52 152 40" stroke="var(--kiln-write-text)" stroke-width="3"/>
      <path d="M74 40 C100 34 130 34 156 40" stroke="var(--kiln-write-text)" stroke-width="3"/>
      <path d="M60 122 C98 132 140 132 178 120" stroke="var(--kiln-write-text)" stroke-width="2" stroke-dasharray="1 7"/>
      <path d="M200 40 V205 M194 40 H206 M194 205 H206" stroke="var(--kiln-deck-text)" stroke-width="2"/>
      <text x="212" y="128" font-family="Figtree" font-size="15" font-weight="700" fill="var(--kiln-deck-text)" stroke="none">24 cm</text>
      <text x="56" y="225" font-family="Figtree" font-size="13" font-weight="600" fill="var(--kiln-muted)" stroke="none">tenmoku line ≈ here</text>
    </svg>'''


def build(theme):
    right = (f'<div class="btn ghost">{I("history", "s14")}History</div>'
             f'<div class="btn">{I("bell", "s14")}Follow</div>'
             f'<div class="btn solid">{I("share-2", "s14")}Share</div>')
    tags = ''.join(f'<div class="sm">{x}</div>' for x in [
        f'<span class="cbx" style="width:14px;height:14px;border-radius:4px;border:1.75px solid currentColor;display:inline-block"></span>To do',
        f'{I("star", "s14", "color:#E05A27")}Important', f'{I("circle-help", "s14", "color:#3159C6")}Question'])
    rib = (clipboard_grp() +
           grp('Basic text', f'''<div class="col"><div class="row" style="gap:4px">{dd("Figtree", 118)}{dd("11", 52)}</div>
             <div class="row">{ib("bold")}{ib("italic")}{ib("underline")}{ib("highlighter", bar="#F5B73F")}{ib("list")}{ib("list-checks", True)}</div></div>''') +
           grp('Styles', f'''<div class="col" style="gap:2px"><div class="sm" style="font:800 14px/1 var(--kiln-font-display)">Heading 1</div>
             <div class="sm" style="font-weight:700">Heading 2</div><div class="sm">Normal</div></div>''') +
           grp('Tags', f'<div class="col" style="gap:2px">{tags}</div>' + big('tag', 'All tags')) +
           grp('Draw', f'''<div class="col"><div class="row">{ib("pen-tool")}{ib("highlighter")}{ib("eraser")}{ib("lasso")}</div>
             <div class="row"><div class="ib"><i style="width:14px;height:14px;border-radius:50%;background:#3159C6"></i></div><div class="ib"><i style="width:14px;height:14px;border-radius:50%;background:#E05A27"></i></div><div class="ib on"><i style="width:14px;height:14px;border-radius:50%;background:#E8A82C"></i></div><div class="ib"><i style="width:14px;height:14px;border-radius:50%;background:var(--kiln-fg)"></i></div></div></div>''') +
           grp('Insert', f'<div class="col" style="gap:2px">{sm("table", "Table")}{sm("mic", "Record audio")}{sm("paperclip", "File")}</div>') +
           grp('Kiln', big('sparkles', 'Summarise', True)))
    secs = f'''<div class="secs pane">
      <div class="nbk"><div class="cv"></div><div><b>Studio notebook</b><span>Kiln Co-op · shared</span></div>{I("chevron-down", "s14", "margin-left:auto")}</div>
      <div class="field" style="margin:4px 0">{I("search", "s14")}Search notes</div>
      <div class="sec on"><span class="tb2" style="background:#E8A82C"></span>Kiln log<span class="c">14</span></div>
      <div class="sec"><span class="tb2" style="background:#E05A27"></span>Glaze recipes<span class="c">22</span></div>
      <div class="sec"><span class="tb2" style="background:#1D8A60"></span>Members<span class="c">6</span></div>
      <div class="sec"><span class="tb2" style="background:#3159C6"></span>Teaching<span class="c">9</span></div>
      <div class="sec"><span class="tb2" style="background:#7A4A33"></span>Repairs<span class="c">3</span></div>
      <div class="sec muted">{I("plus", "s14")}Add section</div>
    </div>'''
    pages = f'''<div class="pages pane">
      <div class="btn" style="justify-content:center;margin-bottom:6px">{I("file-plus", "s14")}Add page</div>
      <div class="pgi on"><b>October firing</b><span>Thu 16 Oct · Candled overnight, cone 10 down at…</span></div>
      <div class="pgi sub"><b>Cone pack photos</b><span>Thu 16 Oct · 6 images</span></div>
      <div class="pgi"><b>September bisque</b><span>Sat 20 Sep · Two shelves of test tiles…</span></div>
      <div class="pgi"><b>Spring firing</b><span>Sat 12 Apr · Tenmoku ran on shelf 1</span></div>
      <div class="pgi"><b>Burner service</b><span>Mon 3 Mar · Thermocouple replaced</span></div>
      <div class="pgi"><b>Winter firing</b><span>Sat 18 Jan · Reduction from 1000 °C</span></div>
      <div class="pgi"><b>Kiln wash recipe</b><span>Thu 9 Jan · 50 alumina / 50 kaolin</span></div>
    </div>'''
    bars = ''.join(f'<i class="{"p" if i < 14 else ""}" style="height:{h}px"></i>' for i, h in enumerate([6, 10, 16, 12, 20, 14, 8, 18, 22, 12, 9, 15, 19, 11, 7, 13, 17, 10, 6, 12, 16, 9, 5, 11]))
    leaf = f'''<div class="leaf">
      <h1>Kiln log: October firing</h1>
      <div class="when"><span>Thursday 16 October 2026</span><span>21:40</span><span>{I("users", "s14", "vertical-align:-2px")} Rana, Jun, Muthana</span></div>
      <div class="box sel" style="left:48px;top:150px;width:400px">
        <h3>Before we light</h3>
        <div class="todo done"><span class="cb">{I("check", "s14")}</span><span>Wax the feet on everything</span></div>
        <div class="todo done"><span class="cb">{I("check", "s14")}</span><span>Cone packs on shelves 1, 3 and 6</span></div>
        <div class="todo"><span class="cb"></span><span>Tenmoku pieces off shelf 1<span class="tagimp">{I("star", "s14")}Important</span></span></div>
        <div class="todo"><span class="cb"></span><span>Check the spy-hole bungs</span></div>
      </div>
      <div class="box" style="left:48px;top:356px">
        <h3>Temperature log</h3>
        <table class="mini"><tr><th>Time</th><th>°C</th><th>Note</th></tr>
          <tr><td>18:00</td><td class="n">210</td><td>Candling, door ajar</td></tr>
          <tr><td>20:30</td><td class="n">640</td><td>Door closed</td></tr>
          <tr><td>21:40</td><td class="n">1,012</td><td>Start body reduction</td></tr></table>
      </div>
      <div class="box" style="left:48px;top:540px"><div class="audio"><span class="pl">{I("play", "s14")}</span><div class="wave">{bars}</div><span class="muted tnum">0:42 / 1:18</span><span>Jun on the flame colour</span></div></div>
      {ink_pot()}
      <div class="sticky" style="left:520px;top:158px"><b>Next time</b>Order more cone 10s. We used the last pack tonight.</div>
    </div>'''
    sb = status(f'{autosave("Saved to Studio")}<div class="it">{I("users", "s14")}3 people editing</div>',
                f'<div class="it">Page 1 of 14</div>{zoom("100%")}')
    body = (f'<div class="app">{titlebar("notes", "Studio notebook", "")}'
            f'{tabs("Home", ["Home", "Insert", "Draw", "History", "Review", "View"], right)}'
            f'<div class="ribbon">{rib}</div><div class="work">{secs}{pages}{leaf}</div>{sb}</div>')
    return page('notes', theme, body, CSS)
