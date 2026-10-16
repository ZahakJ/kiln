import sys
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from mock_common import *
from geo import kiln_arch

W, H = 167, 158
DOOR = kiln_arch(W / 2, 0, W, H, 2.6, 1.6, 16)

CSS = '''
body { width: 390px; height: 844px; }
.ph { width: 390px; height: 844px; position: relative; overflow: hidden; background: var(--kiln-bg); display: flex; flex-direction: column; }
.ph::before { content: ''; position: absolute; left: -120px; top: -200px; width: 600px; height: 480px; background: radial-gradient(closest-side, color-mix(in srgb, #FF7A2A 16%, transparent), transparent); }
.st { height: 44px; padding-top: 4px !important; flex: none; display: flex; align-items: center; justify-content: space-between; padding: 0 22px 0 26px; font: 700 14px/1 var(--kiln-font-ui); position: relative; }
.st .r { display: flex; gap: 6px; align-items: center; }
.top { display: flex; align-items: center; justify-content: space-between; padding: 10px 18px 0; position: relative; }
.top img { height: 26px; }
.top .avatar { margin: 0; width: 34px; height: 34px; }
.hi { padding: 18px 18px 0; position: relative; }
.hi .eb { display: flex; gap: 7px; align-items: center; font-size: 13px; color: var(--kiln-muted); margin-bottom: 6px; }
.hi h1 { font: 800 31px/1.04 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; letter-spacing: -.02em; }
.dg { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; padding: 18px 18px 0; position: relative; }
.d { height: 158px; clip-path: path('__DOOR__'); background: var(--solid); color: var(--on); position: relative; padding: 0 16px 14px 20px; display: flex; flex-direction: column; justify-content: flex-end; }
.d::before { content: ''; position: absolute; left: 50%; bottom: -60px; width: 260px; height: 180px; margin-left: -130px; background: radial-gradient(closest-side, rgba(255,214,150,.42), transparent); }
.d img { position: absolute; left: 50%; top: 16px; width: 60px; height: 60px; margin-left: -30px; filter: drop-shadow(0 6px 8px rgba(0,0,0,.25)); }
.d b { font: 800 19px/1.05 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; position: relative; }
.d span { font-size: 11.5px; opacity: .85; position: relative; margin-top: 2px; }
.d .rim { position: absolute; inset: 0; }
.rec { padding: 20px 18px 0; position: relative; flex: 1; min-height: 0; overflow: hidden; }
.rec .h { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 6px; }
.rec h3 { font: 800 18px/1 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; }
.ri { display: flex; align-items: center; gap: 12px; padding: 10px 0; border-bottom: 1px solid var(--kiln-line); }
.ri img { width: 30px; height: 30px; }
.ri b { font-size: 14px; font-weight: 600; display: block; }
.ri small { font-size: 12px; color: var(--kiln-faint); }
.ri .t { margin-left: auto; font-size: 12px; color: var(--kiln-faint); }
.nav { height: 84px; flex: none; display: flex; align-items: flex-start; justify-content: space-around; padding: 10px 8px 0; background: var(--kiln-panel); border-top: 1px solid var(--kiln-line); position: relative; }
.nav div { display: flex; flex-direction: column; align-items: center; gap: 4px; font-size: 11.5px; font-weight: 600; color: var(--kiln-muted); width: 64px; }
.nav div.on { color: var(--kiln-ember-text); }
.nav div.on .pill { background: color-mix(in srgb, var(--kiln-ember) 14%, transparent); }
.nav .pill { width: 54px; height: 30px; border-radius: 15px; display: grid; place-items: center; }
.fab { position: absolute; right: 18px; bottom: 102px; height: 54px; border-radius: 18px; padding: 0 20px 0 16px; display: flex; align-items: center; gap: 8px; background: var(--kiln-ember); color: var(--kiln-on-ember); font: 700 15px/1 var(--kiln-font-ui); box-shadow: 0 12px 24px -10px rgba(194,72,28,.7), inset 0 -2px 0 rgba(0,0,0,.15); }
.gesture { position: absolute; left: 50%; bottom: 8px; width: 120px; height: 4px; margin-left: -60px; border-radius: 2px; background: var(--kiln-fg); opacity: .5; }
'''


def build(theme):
    lock = '../../lockup-light.svg' if theme == 'light' else '../../lockup.svg'
    doors = ''
    for k, t, d, solid, on in (('write', 'Document', 'Write', '#2D4FB5', '#fff'), ('grid', 'Workbook', 'Grid', '#17714E', '#fff'),
                               ('deck', 'Deck', 'Deck', '#E05A27', '#1C0D06'), ('notes', 'Notebook', 'Notes', '#E8A82C', '#1C0D06')):
        rim = (f'<svg class="rim" width="{W}" height="{H}"><defs><linearGradient id="r{k}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".55"/>'
               f'<stop offset=".5" stop-color="#fff" stop-opacity=".06"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>'
               f'<path d="{DOOR}" fill="none" stroke="url(#r{k})" stroke-width="3"/></svg>')
        doors += f'<div class="d" style="--solid:{solid};--on:{on}">{rim}<img src="../../icons/svg/{k}.svg"><b>{t}</b><span>Kiln {d}</span></div>'
    rec = ''.join(f'<div class="ri"><img src="../../icons/svg/{k}-32.svg"><div><b>{n}</b><small>{loc}</small></div><span class="t">{t}</span></div>'
                  for k, n, loc, t in (('write', 'Autumn firing plan', 'Studio › Co-op', '10 min'), ('grid', 'Studio budget 2026', 'Studio › Accounts', '1 h'),
                                       ('deck', 'Open studio weekend', 'Desktop', 'Yesterday'), ('notes', 'Studio notebook', 'Notes › Studio', 'Yesterday')))
    body = f'''<div class="ph">
      <div class="st"><span>9:41</span><span class="r">{I("hard-drive", "s14") if False else ""}<svg width="17" height="12" viewBox="0 0 17 12"><path d="M1 11h2V8H1zm4 0h2V6H5zm4 0h2V3H9zm4 0h2V1h-2z" fill="currentColor"/></svg>
        <svg width="16" height="12" viewBox="0 0 16 12"><path d="M8 11.5 1 4.6a10 10 0 0 1 14 0Z" fill="currentColor"/></svg>
        <svg width="26" height="13" viewBox="0 0 26 13"><rect x=".5" y=".5" width="22" height="12" rx="3.5" fill="none" stroke="currentColor" opacity=".5"/><rect x="2" y="2" width="16" height="9" rx="2" fill="currentColor"/><rect x="23.5" y="4" width="2" height="5" rx="1" fill="currentColor" opacity=".5"/></svg></span></div>
      <div class="top"><img src="{lock}"><div style="display:flex;gap:10px;align-items:center"><div class="qa">{I("search", "s20")}</div><div class="avatar">MA</div></div></div>
      <div class="hi"><div class="eb">{flame(11, 13)}The kiln is warm.</div><h1>What are we<br>making today?</h1></div>
      <div class="dg">{doors}</div>
      <div class="rec"><div class="h"><h3>Recent</h3><span style="color:var(--kiln-ember-text);font-weight:600;font-size:13px">See all</span></div>{rec}</div>
      <div class="fab">{spark(15, "currentColor")}New</div>
      <div class="nav"><div class="on"><span class="pill">{I("house", "s20")}</span>Home</div><div><span class="pill">{I("folder", "s20")}</span>Files</div>
        <div><span class="pill">{I("users", "s20")}</span>Shared</div><div><span class="pill">{I("layout-template", "s20")}</span>Templates</div></div>
      <div class="gesture"></div>
    </div>'''
    return page('home', theme, body, CSS.replace('__DOOR__', DOOR), 390, 844)
