import sys
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from mock_common import *

CSS = '''
.nav { width: 250px; flex: none; padding: 14px 12px; display: flex; flex-direction: column; gap: 12px; }
.nav .h { display:flex; align-items:center; justify-content:space-between; font-weight:700; font-size:13px; padding: 0 4px; }
.toc { display: flex; flex-direction: column; gap: 1px; margin-top: 2px; }
.toc div { padding: 6px 10px; border-radius: 8px; font-size: 12.5px; color: var(--kiln-fg); white-space: nowrap; }
.toc .l2 { padding-left: 24px; color: var(--kiln-muted); }
.toc .on { background: var(--app-tint); color: var(--app-text); font-weight: 600; position: relative; }
.toc .on::before { content:''; position:absolute; left:3px; top:7px; bottom:7px; width:3px; border-radius:2px; background: var(--app); }
.rgoal { margin-top: auto; border: 1px solid var(--kiln-line); border-radius: 12px; padding: 12px; background: var(--kiln-raised); }
.rgoal .t { display:flex; justify-content:space-between; font-size:12px; font-weight:600; }
.rgoal .bar { height: 6px; border-radius: 3px; background: var(--kiln-sunken); margin: 9px 0 8px; overflow:hidden; }
.rgoal .bar i { display:block; height:100%; width:86%; border-radius:3px; background: linear-gradient(90deg,#FF6A26,#FFB547); }
.rgoal .stats { display:grid; grid-template-columns: 1fr 1fr; gap: 6px; margin-top: 10px; }
.rgoal .stats b { font: 700 17px/1.1 var(--kiln-font-display); font-variation-settings:'SOFT' 40; display:block; }
.rgoal .stats span { font-size: 11px; color: var(--kiln-faint); }
.canvas { flex: 1; position: relative; overflow: hidden; display:flex; flex-direction: column; align-items: center; }
.ruler { width: 720px; height: 22px; margin: 2px 0 10px; border-radius: 6px; background: var(--kiln-panel); border: 1px solid var(--kiln-line); position: relative;
  background-image: repeating-linear-gradient(90deg, var(--kiln-line-strong) 0 1px, transparent 1px 18px); background-size: 600px 6px; background-repeat: no-repeat; background-position: 60px 14px; }
.ruler i { position:absolute; top: 3px; width: 10px; height: 9px; background: var(--app); clip-path: polygon(0 0,100% 0,100% 55%,50% 100%,0 55%); }
.pg { width: 720px; flex: 1; background: var(--kiln-page); border-radius: 6px 6px 0 0; padding: 64px 76px 0; box-shadow: var(--kiln-shadow-page); position: relative; color: var(--kiln-fg); }
.eyebrow { font: 700 11px/1 var(--kiln-font-ui); letter-spacing: .12em; text-transform: uppercase; color: var(--kiln-ember-text); margin-bottom: 14px; }
.pg h1 { font: 800 38px/1.08 var(--kiln-font-display); font-variation-settings: 'SOFT' 40, 'WONK' 0; letter-spacing: -.015em; margin-bottom: 14px; }
.pg .lede { font: 400 17px/1.55 var(--kiln-font-display); font-variation-settings: 'SOFT' 60, 'opsz' 14; color: var(--kiln-muted); margin-bottom: 22px; }
.pg h2 { font: 700 21px/1.2 var(--kiln-font-display); font-variation-settings: 'SOFT' 40; color: var(--kiln-write-text); margin: 4px 0 10px; }
.pg p { font: 400 14.5px/1.65 var(--kiln-font-ui); margin-bottom: 12px; }
.pg ul { margin: 0 0 14px 2px; list-style: none; }
.pg li { font-size: 14.5px; line-height: 1.6; padding-left: 20px; position: relative; }
.pg li::before { content:''; position:absolute; left: 3px; top: .55em; width: 7px; height: 8px; background: var(--app); border-radius: 4px 4px 1px 1px; }
.sel { background: var(--kiln-selection); border-radius: 2px; box-shadow: 0 0 0 1px color-mix(in srgb, var(--kiln-ember) 25%, transparent); }
.cmt-anchor { background: color-mix(in srgb, #E3A126 26%, transparent); border-bottom: 2px solid #E3A126; }
table.t { width: 100%; border-collapse: separate; border-spacing: 0; font-size: 13px; border: 1px solid var(--kiln-line); border-radius: 10px; overflow: hidden; margin-top: 6px; }
table.t th { text-align: left; font-size: 11px; letter-spacing: .06em; text-transform: uppercase; color: var(--kiln-muted); background: var(--kiln-write-tint); padding: 9px 12px; font-weight: 700; }
table.t td { padding: 9px 12px; border-top: 1px solid var(--kiln-line); }
table.t td.n, table.t th.n { text-align: right; font-variant-numeric: tabular-nums; }
.sw { display:inline-block; width:10px; height:10px; border-radius: 5px 5px 2px 2px; margin-right: 7px; vertical-align: -1px; }
.comments { width: 270px; flex: none; position: relative; }
.cm { position: absolute; left: 0; right: 0; background: var(--kiln-panel); border: 1px solid var(--kiln-line); border-radius: 12px; padding: 12px; box-shadow: var(--kiln-shadow-2); font-size: 12.5px; }
.cm.on { border-color: #E3A126; box-shadow: 0 0 0 2px color-mix(in srgb, #E3A126 25%, transparent), var(--kiln-shadow-2); }
.cm .who { display: flex; align-items: center; gap: 8px; font-weight: 700; margin-bottom: 6px; }
.cm .av { width: 22px; height: 22px; border-radius: 50%; display: grid; place-items: center; font-size: 9.5px; font-weight: 800; color: #fff; }
.cm .when { margin-left: auto; font-weight: 500; color: var(--kiln-faint); font-size: 11.5px; }
.cm .acts { display:flex; gap: 6px; margin-top: 10px; }
.cm .acts span { height: 26px; padding: 0 10px; border-radius: 7px; display:inline-flex; align-items:center; font-weight: 600; font-size: 12px; border: 1px solid var(--kiln-line-strong); }
.cm .acts span.p { background: var(--app-solid); color: var(--app-on-solid); border-color: transparent; }
.leader { position: absolute; height: 0; border-top: 1.5px dashed #E3A126; }
'''


def build(theme):
    right = (f'<div class="btn ghost">{I("message-square", "s14")}Comments<span class="chip" style="height:18px;padding:0 6px">2</span></div>'
             f'<div class="btn">{I("pencil", "s14")}Editing{I("chevron-down", "s14")}</div>'
             f'<div class="btn solid">{I("share-2", "s14")}Share</div>')
    rib = (clipboard_grp() +
           grp('Font', f'''<div class="col"><div class="row" style="gap:4px">{dd("Figtree", 150)}{dd("11", 54)}{ib("a-arrow-up")}{ib("a-arrow-down")}</div>
             <div class="row">{ib("bold", True)}{ib("italic")}{ib("underline")}{ib("strikethrough")}{ib("subscript")}{ib("superscript")}
             {ib("baseline", bar="#E05A27")}{ib("highlighter", bar="#F5B73F")}</div></div>''') +
           grp('Paragraph', f'''<div class="col"><div class="row">{ib("list")}{ib("list-ordered")}{ib("indent-decrease")}{ib("indent-increase")}{ib("pilcrow")}</div>
             <div class="row">{ib("align-left", True)}{ib("align-center")}{ib("align-right")}{ib("align-justify")}{ib("arrow-up-down")}</div></div>''') +
           grp('Styles', '''<div class="gal">
             <div class="sty on"><div class="s" style="font-family:var(--kiln-font-ui)">Aa Bb</div><div class="n">Normal</div></div>
             <div class="sty"><div class="s disp" style="font-weight:800">Aa Bb</div><div class="n">Title</div></div>
             <div class="sty"><div class="s disp" style="font-weight:700;color:var(--kiln-write-text)">Aa Bb</div><div class="n">Heading 1</div></div>
             <div class="sty"><div class="s" style="font-weight:700">Aa Bb</div><div class="n">Heading 2</div></div>
             <div class="sty"><div class="s disp" style="font-style:italic;color:var(--kiln-muted)">“Aa”</div><div class="n">Quote</div></div>
             <div class="galmore">''' + I('chevron-down', 's14') + '</div></div>') +
           grp('Editing', f'<div class="col" style="gap:2px">{sm("text-search", "Find")}{sm("replace", "Replace")}{sm("mouse-pointer-2", "Select")}</div>') +
           grp('Voice', big('mic', 'Dictate', True)) +
           grp('Kiln', big('sparkles', 'Rewrite')))
    nav = f'''<div class="nav pane">
      <div class="h">Navigation{I("x", "s14")}</div>
      <div class="field">{I("search", "s14")}Search document</div>
      <div class="seg"><span class="on">Headings</span><span>Pages</span><span>Results</span></div>
      <div class="toc">
        <div>The Autumn Firing</div><div class="on">What goes in the kiln</div><div class="l2">Loading order</div><div class="l2">Glaze tests</div>
        <div>The schedule</div><div class="l2">Candling overnight</div><div class="l2">Body reduction</div><div>Unloading day</div><div>Who brings what</div>
      </div>
      <div class="rgoal"><div class="t"><span>{flame(10,12)} Word goal</span><span class="tnum muted">1,284 / 1,500</span></div>
        <div class="bar"><i></i></div><div class="faint" style="font-size:11.5px">216 words to go. About 6 minutes to read.</div>
        <div class="stats"><div><b class="tnum">1,284</b><span>words</span></div><div><b class="tnum">2</b><span>open comments</span></div></div></div>
    </div>'''
    doc = f'''<div class="canvas"><div class="ruler"><i style="left:58px"></i><i style="left:652px"></i></div>
      <div class="pg glowpage">
        <div class="eyebrow">Studio Co-op · Draft 3 · for Saturday</div>
        <h1>The Autumn Firing</h1>
        <div class="lede">Twice a year we load the big catenary kiln together. This is the plan for October: what goes in, how hot we take it, and who stays up for the overnight shift.</div>
        <h2>What goes in the kiln</h2>
        <p>We have room for roughly <b>240 pieces</b> across six shelves. Stoneware takes the back of the chamber, where the flame path is longest. Porcelain and the test tiles sit up front so we can watch them through the spy hole. Please wax your feet before Friday, and <span class="cmt-anchor">label anything glazed with the new tenmoku</span> so we can track how it <span class="sel">breaks over the rims</span><span class="caret"></span>.</p>
        <ul><li>Bisque by Wednesday, glaze by Thursday noon</li><li>Cones 9, 10 and 11 on every shelf</li></ul>
        <table class="t"><tr><th>Shelf</th><th>Body</th><th>Glaze</th><th class="n">Cone</th></tr>
          <tr><td>1 – 2</td><td>Stoneware</td><td><span class="sw" style="background:#3A2318;box-shadow:0 0 0 1px #8A5232"></span>Tenmoku</td><td class="n">10</td></tr>
          <tr><td>3 – 4</td><td>Stoneware</td><td><span class="sw" style="background:#1D8A60"></span>Celadon</td><td class="n">10</td></tr>
          <tr><td>5</td><td>Porcelain</td><td><span class="sw" style="background:#3159C6"></span>Cobalt slip</td><td class="n">9</td></tr></table>
      </div></div>'''
    comments = f'''<div class="comments">
      <div class="leader" style="left:-76px;width:76px;top:404px"></div>
      <div class="cm on" style="top:352px"><div class="who"><span class="av" style="background:#1D8A60">RO</span>Rana O.<span class="when">2 h</span></div>
        Can we keep the tenmoku pieces off shelf 1? It ran last spring.
        <div class="acts"><span class="p">Reply</span><span>Resolve</span></div></div>
      <div class="cm" style="top:510px"><div class="who"><span class="av" style="background:#C2481C">JK</span>Jun K.<span class="when">Yesterday</span></div>
        Added the cone 9 shelf for porcelain. {I("check", "s14", "vertical-align:-2px;color:var(--kiln-success)")}
        <div class="acts"><span>Reply</span><span>Resolve</span></div></div>
    </div>'''
    sb = status(f'<div class="it">Page 1 of 4</div><div class="it tnum">1,284 words</div><div class="goal">{I("flame", "s14")}<div class="m"><i></i></div><span class="tnum">86%</span></div>'
                f'<div class="it">{I("languages", "s14")}English (UK)</div><div class="it ok">{I("circle-check", "s14")}No issues</div>{autosave("Autosave on")}',
                f'<div class="it">{I("focus", "s14")}Focus</div><div class="vm"><span>{I("book-open", "s14")}</span><span class="on">{I("file-text", "s14")}</span><span>{I("globe", "s14")}</span></div>{zoom()}')
    body = (f'<div class="app">{titlebar("write", "Autumn firing plan", ".docx")}'
            f'{tabs("Home", ["Home", "Insert", "Layout", "References", "Review", "View", "Help"], right)}'
            f'<div class="ribbon">{rib}</div><div class="work">{nav}{doc}{comments}</div>{sb}</div>')
    return page('write', theme, body, CSS)
