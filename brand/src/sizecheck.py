import sys
O='/home/muth/kiln-brand/final'
K=['suite','write','grid','deck','notes']
bgs=[('#F6EFE6','bone'),('#FFFFFF','white'),('#3159C6','cobalt'),('#17110E','soot')]
h='<!doctype html><style>body{margin:0;font:12px Figtree,sans-serif}.r{display:flex;align-items:flex-end;gap:14px;padding:18px 24px}.k{display:flex;align-items:flex-end;gap:8px;margin-right:18px}img{display:block}.z img{image-rendering:pixelated}</style>'
for bg,n in bgs:
    h+=f'<div class="r" style="background:{bg}">'
    for k in K:
        h+='<div class="k">'+''.join(f'<img src="{O}/icons/png/{k}-{s}.png" width="{s}" height="{s}">' for s in (128,64,48,32,24,16))+'</div>'
    h+='</div>'
# zoomed smalls
for bg,n in bgs[:1]+bgs[3:]:
    h+=f'<div class="r z" style="background:{bg}">'
    for k in K:
        h+='<div class="k">'+''.join(f'<img src="{O}/icons/png/{k}-{s}.png" width="{s*4}" height="{s*4}">' for s in (32,24,16))+'</div>'
    h+='</div>'
open(f'{O}/work/sizecheck.html','w').write(h)
