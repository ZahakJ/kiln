import sys
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from mock_common import *
from geo import kiln_arch
DOOR = kiln_arch(135, 0, 270, 246, 2.6, 1.6, 20)

LOCKUP = {'light': '../../lockup-light.svg', 'dark': '../../lockup.svg'}

CSS = '''
.home { display: flex; height: 900px; }
.side { width: 240px; flex: none; display: flex; flex-direction: column; padding: 16px 12px 14px; gap: 2px; }
.side .logo { height: 30px; margin: 0 6px 18px; display: block; }
.newbtn { height: 40px; border-radius: 11px; background: var(--kiln-ember); color: var(--kiln-on-ember); display: flex; align-items: center; gap: 8px; padding: 0 12px; font-weight: 700; font-size: 14px; margin-bottom: 12px; box-shadow: inset 0 -2px 0 rgba(0,0,0,.16), 0 6px 16px -6px rgba(194,72,28,.55); }
.newbtn .kbd { background: rgba(255,255,255,.16); border-color: rgba(255,255,255,.25); color: inherit; }
.nv { display: flex; align-items: center; gap: 11px; height: 36px; padding: 0 11px; border-radius: 9px; font-size: 13.5px; font-weight: 500; color: var(--kiln-fg); }
.nv.on { background: var(--kiln-panel); box-shadow: var(--kiln-shadow-1); font-weight: 700; }
.nv.on svg { color: var(--kiln-ember-text); }
.nv img { width: 20px; height: 20px; }
.nv .c { margin-left: auto; font-size: 11.5px; color: var(--kiln-faint); }
.side .cap { font-size: 11px; font-weight: 700; letter-spacing: .1em; text-transform: uppercase; color: var(--kiln-faint); padding: 16px 11px 6px; }
.local { margin-top: auto; border-radius: 12px; padding: 12px; background: var(--kiln-panel); border: 1px solid var(--kiln-line); font-size: 12px; color: var(--kiln-muted); }
.local b { color: var(--kiln-fg); display: flex; align-items: center; gap: 6px; font-size: 12.5px; margin-bottom: 3px; }
.local .m { height: 5px; background: var(--kiln-sunken); border-radius: 3px; margin: 9px 0 5px; overflow: hidden; }
.local .m i { display: block; width: 12%; height: 100%; background: linear-gradient(90deg,#FF6A26,#FFB547); }
.main { flex: 1; display: flex; flex-direction: column; background: var(--kiln-panel); border-left: 1px solid var(--kiln-line); position: relative; overflow: hidden; }
.main::before { content: ''; position: absolute; left: -140px; top: -260px; width: 900px; height: 620px; background: radial-gradient(closest-side, color-mix(in srgb, #FF7A2A 13%, transparent), transparent); pointer-events: none; }
[data-theme="dark"] .main::before { background: radial-gradient(closest-side, rgba(255,106,38,.16), transparent); }
.mtb { height: 54px; display: flex; align-items: center; padding: 0 10px 0 36px; position: relative; z-index: 1; }
.mtb .search { margin: 0 auto; width: 460px; height: 36px; background: var(--kiln-raised); border: 1px solid var(--kiln-line); }
.content { padding: 4px 36px 0; position: relative; z-index: 1; display: flex; flex-direction: column; gap: 18px; flex: 1; min-height: 0; }
.hello { display: flex; align-items: flex-end; justify-content: space-between; }
.hello .eb { display: flex; align-items: center; gap: 8px; font-size: 13.5px; color: var(--kiln-muted); margin-bottom: 6px; }
.hello h1 { font: 800 46px/1.02 var(--kiln-font-display); font-variation-settings: 'SOFT' 40, 'WONK' 0; letter-spacing: -.022em; }
.doors { display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; }
.door { height: 246px; clip-path: path('__DOOR__'); position: relative; overflow: hidden; padding: 0 24px 18px 30px; display: flex; flex-direction: column; justify-content: flex-end; color: var(--on); background: var(--solid); box-shadow: inset 0 -3px 0 rgba(0,0,0,.18), 0 14px 30px -18px var(--solid); }
.door::before { content: ''; position: absolute; left: 50%; bottom: -70px; width: 420px; height: 260px; margin-left: -210px; background: radial-gradient(closest-side, rgba(255,214,150,.42), rgba(255,180,90,.12) 55%, transparent); pointer-events: none; }
.door .rim { position: absolute; inset: 0; pointer-events: none; }
.door .ic { position: absolute; left: 50%; top: 22px; width: 92px; height: 92px; margin-left: -46px; filter: drop-shadow(0 8px 12px rgba(0,0,0,.25)); }
.door .k { font-size: 11px; font-weight: 800; letter-spacing: .12em; text-transform: uppercase; opacity: .8; position: relative; }
.door .t { font: 800 27px/1.05 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; letter-spacing: -.01em; margin: 3px 0 4px; position: relative; }
.door .d { font-size: 12.5px; opacity: .85; position: relative; }
.door .meta { display: flex; gap: 6px; margin-top: 12px; position: relative; align-items: center; }
.door .meta span { font-size: 11px; font-weight: 700; padding: 3px 7px; border-radius: 6px; background: rgba(255,255,255,.18); }
.door.dk .meta span { background: rgba(28,13,6,.12); }
.door .plus { margin-left: auto; width: 30px; height: 30px; border-radius: 50%; display: grid; place-items: center; background: rgba(255,255,255,.22); }
.door.dk .plus { background: rgba(28,13,6,.14); }
.switch { display: flex; align-items: center; gap: 14px; padding: 12px 14px 12px 12px; border-radius: 14px; background: var(--kiln-bg); border: 1px solid var(--kiln-line); }
.switch .fmt { display: flex; }
.switch .fmt span { width: 40px; height: 48px; border-radius: 6px 12px 6px 6px; display: grid; place-items: end center; padding-bottom: 7px; font: 800 9.5px/1 var(--kiln-font-ui); color: #fff; margin-left: 5px; box-shadow: 0 0 0 2px var(--kiln-bg); transform: rotate(var(--r)); }
.switch .fmt span:first-child { margin-left: 0; }
.switch b { font-size: 14px; display: block; }
.switch .tx { font-size: 12.5px; color: var(--kiln-muted); }
.lower { display: flex; gap: 28px; flex: 1; min-height: 0; }
.recent { flex: 1; min-width: 0; }
.lh { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; }
.lh h3 { font: 800 19px/1 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; }
.rt { width: 100%; border-collapse: collapse; font-size: 13px; }
.rt th { text-align: left; font-size: 11px; letter-spacing: .08em; text-transform: uppercase; color: var(--kiln-faint); font-weight: 700; padding: 6px 10px; }
.rt td { padding: 7px 10px; border-top: 1px solid var(--kiln-line); white-space: nowrap; }
.rt td .nm { display: flex; align-items: center; gap: 11px; }
.rt td .nm img { width: 24px; height: 24px; }
.rt td .nm b { font-weight: 600; } .rt td .nm .x { color: var(--kiln-faint); font-weight: 500; }
.rt td .nm small { display: block; color: var(--kiln-faint); font-size: 11.5px; }
.rt tr.hl td { background: var(--kiln-bg); }
.rt tr.hl td:first-child { border-radius: 10px 0 0 10px; } .rt tr.hl td:last-child { border-radius: 0 10px 10px 0; }
.tpl { width: 400px; flex: none; }
.tg { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.tc .im { height: 104px; border-radius: 12px; border: 1px solid var(--kiln-line); overflow: hidden; position: relative; background: var(--kiln-bg); }
.tc .nm { display: flex; align-items: center; gap: 7px; font-size: 12.5px; font-weight: 600; margin-top: 7px; }
.tc .nm img { width: 16px; height: 16px; }
'''

DOORS = [
    ('write', 'Kiln Write', 'Document', 'Letters, essays, reports', ['.docx', '.odt'], 'Ctrl Alt W', '#2D4FB5', '#FFFFFF', False),
    ('grid', 'Kiln Grid', 'Workbook', 'Budgets, lists, models', ['.xlsx', '.csv'], 'Ctrl Alt G', '#17714E', '#FFFFFF', False),
    ('deck', 'Kiln Deck', 'Deck', 'Talks, pitches, lessons', ['.pptx', '.odp'], 'Ctrl Alt D', '#E05A27', '#1C0D06', True),
    ('notes', 'Kiln Notes', 'Notebook', 'Ideas, logs, clippings', ['.one import'], 'Ctrl Alt N', '#E8A82C', '#1C0D06', True),
]

RECENT = [
    ('write', 'Autumn firing plan', '.docx', 'Studio › Co-op', '10 minutes ago', 'Rana, Jun'),
    ('grid', 'Studio budget 2026', '.xlsx', 'Studio › Accounts', '1 hour ago', 'Only you'),
    ('deck', 'Open studio weekend', '.pptx', 'Desktop', 'Yesterday, 18:40', 'Only you'),
    ('notes', 'Studio notebook', '', 'Notes › Studio', 'Yesterday, 09:12', 'Co-op'),
    ('write', 'Residency application', '.docx', 'Documents', 'Monday', 'Only you'),
    ('grid', 'Clay order tracker', '.xlsx', 'Studio › Supplies', 'Sunday', 'Jun'),
]


def tmpl_img(kind):
    if kind == 'letter':
        return ('<div style="position:absolute;left:50%;top:14px;width:84px;margin-left:-42px;bottom:-6px;background:var(--kiln-page);border-radius:4px;box-shadow:var(--kiln-shadow-2);padding:10px">'
                '<div style="width:30px;height:5px;border-radius:3px;background:#3159C6;margin-bottom:8px"></div>'
                + ''.join(f'<div style="height:3px;border-radius:2px;background:var(--kiln-line-strong);margin-bottom:5px;width:{w}%"></div>' for w in (100, 92, 96, 70, 100, 84)) + '</div>')
    if kind == 'budget':
        cells = ''.join(f'<div style="height:9px;border-radius:2px;background:{"#1D8A60" if r == 0 else "var(--kiln-line)"}"></div>' for r in range(5) for c in range(4))
        bars = ''.join(f'<i style="width:9px;height:{h}px;border-radius:3px 3px 0 0;background:{"#E05A27" if h == 52 else "#1D8A60"}"></i>' for h in (24, 36, 30, 52))
        return (f'<div style="position:absolute;left:12px;top:14px;width:92px;display:grid;grid-template-columns:repeat(4,1fr);gap:4px">{cells}</div>'
                f'<div style="position:absolute;right:14px;bottom:12px;display:flex;gap:4px;align-items:flex-end">{bars}</div>')
    if kind == 'pitch':
        return ('<div style="position:absolute;inset:12px;border-radius:6px;background:#21140D;overflow:hidden">'
                '<div style="position:absolute;left:10px;top:12px;font:800 15px/1 Fraunces;color:#FFF1DF;font-variation-settings:\'SOFT\' 40">Warm<br>pitch</div>'
                '<div style="position:absolute;right:-6px;bottom:-26px;width:70px;height:80px;border-radius:50% 50% 6px 6px/60% 60% 6px 6px;background:radial-gradient(circle at 50% 85%,#FFB547,#E05A27 40%,#4A1A0A 75%)"></div></div>')
    if kind == 'log':
        return ('<div style="position:absolute;left:24px;top:12px;width:120px;bottom:-10px;background:#FCE6A8;border-radius:4px;transform:rotate(-4deg);padding:10px;box-shadow:var(--kiln-shadow-2)">'
                '<div style="font:800 12px/1 Fraunces;color:#6B4A0A;margin-bottom:8px">Daily log</div>'
                + ''.join(f'<div style="display:flex;gap:5px;align-items:center;margin-bottom:5px"><i style="width:8px;height:8px;border-radius:2px;border:1.5px solid #B97A0C;background:{"#E8A82C" if i < 2 else "transparent"}"></i><i style="height:3px;width:{w}px;border-radius:2px;background:#E2C27A"></i></div>' for i, w in enumerate((60, 48, 70)))
                + '</div>')


def build(theme):
    side = f'''<div class="side">
      <img class="logo" src="{LOCKUP[theme]}">
      <div class="newbtn">{I("plus", "s18")}New<span class="kbd">Ctrl N</span></div>
      <div class="nv on">{I("house", "s18")}Home</div>
      <div class="nv">{I("clock", "s18")}Recent</div>
      <div class="nv">{I("users", "s18")}Shared with me<span class="c">3</span></div>
      <div class="nv">{I("pin", "s18")}Pinned</div>
      <div class="nv">{I("layout-template", "s18")}Templates</div>
      <div class="nv">{I("folder-open", "s18")}Open file<span class="c">Ctrl O</span></div>
      <div class="cap">Apps</div>
      {''.join(f'<div class="nv"><img src="../../icons/svg/{k}-32.svg">{n}</div>' for k, n in (("write", "Write"), ("grid", "Grid"), ("deck", "Deck"), ("notes", "Notes")))}
      <div class="local"><b>{flame(10, 12)}Local first</b>Your files live on this device. Sync is optional.<div class="m"><i></i></div><span class="tnum">7.6 GB of 64 GB</span></div>
      <div class="nv" style="margin-top:8px">{I("settings", "s18")}Settings</div>
    </div>'''
    doors = ''
    for k, kick, t, d, fm, sc, solid, on, dk in DOORS:
        chips = ''.join(f'<span>{x}</span>' for x in fm) + f'<span>{sc}</span>'
        rim = (f'<svg class="rim" width="270" height="246"><defs><linearGradient id="r{k}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".55"/>'
               f'<stop offset=".5" stop-color="#fff" stop-opacity=".08"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>'
               f'<path d="{DOOR}" fill="none" stroke="url(#r{k})" stroke-width="3"/></svg>')
        doors += (f'<div class="door{" dk" if dk else ""}" style="--solid:{solid};--on:{on}">{rim}<img class="ic" src="../../icons/svg/{k}.svg">'
                  f'<div class="k">{kick}</div><div class="t">{t}</div><div class="d">{d}</div>'
                  f'<div class="meta">{chips}<span class="plus">{I("plus", "s18")}</span></div></div>')
    sw = f'''<div class="switch"><div class="fmt"><span style="background:#2D4FB5;--r:-6deg">.docx</span><span style="background:#17714E;--r:2deg">.xlsx</span><span style="background:#E05A27;--r:8deg">.pptx</span></div>
      <div><b>Coming from Microsoft Office?</b><div class="tx">Your .docx, .xlsx and .pptx open as they are. The ribbon is where you left it, and your shortcuts still work.</div></div>
      <div style="margin-left:auto;display:flex;gap:8px"><div class="btn ember">{spark(13, "currentColor")}Take the 2‑minute tour</div><div class="btn">{I("import", "s14")}Import settings</div><div class="qa">{I("x", "s14")}</div></div></div>'''
    rows = ''
    for i, (k, n, x, loc, when, sh) in enumerate(RECENT):
        rows += (f'<tr class="{"hl" if i == 0 else ""}"><td><div class="nm"><img src="../../icons/svg/{k}-32.svg"><div><b>{n}</b><span class="x">{x}</span><small>{loc}</small></div></div></td>'
                 f'<td class="muted">{when}</td><td class="muted">{I("users", "s14", "vertical-align:-2px;margin-right:5px") if sh not in ("Only you",) else ""}{sh}</td>'
                 f'<td style="text-align:right;color:var(--kiln-faint)">{I("pin", "s14") if i < 2 else I("ellipsis", "s14")}</td></tr>')
    recent = f'''<div class="recent"><div class="lh"><div class="seg" style="width:300px"><span class="on">Recent</span><span>Pinned</span><span>Shared with me</span></div>
      <div style="display:flex;gap:4px;color:var(--kiln-faint)"><div class="qa">{I("list", "s14")}</div><div class="qa">{I("layout-grid", "s14")}</div></div></div>
      <table class="rt"><tr><th>Name</th><th>Modified</th><th>Shared</th><th></th></tr>{rows}</table></div>'''
    tpl = f'''<div class="tpl"><div class="lh"><h3>Fresh from the kiln</h3><span style="color:var(--kiln-ember-text);font-weight:600;font-size:12.5px">All templates ›</span></div>
      <div class="tg">{''.join(f'<div class="tc"><div class="im">{tmpl_img(t)}</div><div class="nm"><img src="../../icons/svg/{k}-16.svg">{n}</div></div>' for t, k, n in (("letter", "write", "Studio letter"), ("budget", "grid", "Monthly budget"), ("pitch", "deck", "Warm pitch"), ("log", "notes", "Daily log")))}</div></div>'''
    main = f'''<div class="main"><div class="mtb"><div class="search">{I("search", "s14")}<span>Search files, templates and help</span><span class="kbd">Ctrl K</span></div>
        <div class="qa">{I("bell", "s18")}</div><div class="avatar">MA</div><div class="win">{I("minus", "s14")}</div><div class="win">{I("square", "s14")}</div><div class="win">{I("x", "s14")}</div></div>
      <div class="content">
        <div class="hello"><div><div class="eb">{flame(12, 14)}Good evening, Muthana. The kiln is warm.</div><h1>What are we making today?</h1></div>
          <div style="display:flex;gap:8px"><div class="btn">{I("folder-open", "s14")}Open file</div><div class="btn">{I("layout-template", "s14")}All templates</div></div></div>
        <div class="doors">{doors}</div>
        {sw}
        <div class="lower">{recent}{tpl}</div>
      </div></div>'''
    body = f'<div class="home">{side}{main}</div>'
    return page('home', theme, body, CSS.replace('__DOOR__', DOOR))
