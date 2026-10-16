"""Shared building blocks for the Kiln UI mockups."""
import re, sys
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
from geo import spark as spark_path
from icons import FLAME_BODY, FLAME_BACK

O = '/home/muth/kiln-brand/final'
SRC = f'{O}/mockups/src'
_SPRITE = open(f'{O}/mockups/vendor/lucide-sprite.svg').read()
_SYM = dict(re.findall(r'<symbol[^>]*id="([^"]*)"[^>]*>(.*?)</symbol>', _SPRITE, re.S))

open(f'{SRC}/spark.svg', 'w').write(
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="{spark_path(12, 12, 11, 0.42, 0.74)}" fill="#FF6A26"/></svg>')


def I(name, cls='', style=''):
    body = _SYM[name]
    st = f' style="{style}"' if style else ''
    return f'<svg class="i {cls}" viewBox="0 0 24 24"{st}>{body}</svg>'


def spark(size=11, color='currentColor', cls='sp'):
    return f'<svg class="{cls}" viewBox="0 0 24 24" width="{size}" height="{size}"><path d="{spark_path(12, 12, 11.8, 0.42, 0.74)}" fill="{color}"/></svg>'


def flame(w=11, h=13, body='#FF7A2E', back='#D93D12'):
    return (f'<svg class="fl" viewBox="120 50 260 390" width="{w}" height="{h}"><path d="{FLAME_BACK}" fill="{back}"/>'
            f'<path d="{FLAME_BODY}" fill="{body}"/></svg>')


def page(app, theme, body, extra_css='', w=1440, h=900):
    return f'''<!doctype html><html data-theme="{theme}" data-app="{app}"><head><meta charset="utf-8">
<link rel="stylesheet" href="../../tokens.css"><link rel="stylesheet" href="kiln-ui.css">
<style>body{{width:{w}px;height:{h}px}}{extra_css}</style></head><body>{body}</body></html>'''


APPNAME = {'write': 'Write', 'grid': 'Grid', 'deck': 'Deck', 'notes': 'Notes', 'home': 'Kiln'}


def titlebar(app, title, ext, saved='Saved to Studio', search='Search', initials='MA'):
    icon = f'../../icons/svg/{app if app != "home" else "suite"}-32.svg'
    return f'''<div class="tb">
  <div class="brand"><img src="{icon}"><b>{APPNAME[app]}</b></div>
  <div class="qa">{I("save")}</div><div class="qa">{I("undo-2")}</div><div class="qa">{I("redo-2")}</div>
  <div class="title">{title}<span class="ext">{ext}</span><span class="chip">{flame()}{saved}</span></div>
  <div class="search">{I("search", "s14")}<span>{search}</span><span class="kbd">Alt Q</span></div>
  <div class="avatar">{initials}</div>
  <div class="win">{I("minus", "s14")}</div><div class="win">{I("square", "s14")}</div><div class="win">{I("x", "s14")}</div>
</div>'''


def tabs(active, names, right=''):
    t = ''.join(f'<div class="tab{" on" if n == active else ""}">{n}</div>' for n in names)
    return f'<div class="tabs"><div class="file">{spark(14, "currentColor")}File</div>{t}<div class="right">{right}</div></div>'


def grp(label, inner, style=''):
    st = f' style="{style}"' if style else ''
    return f'<div class="grp"{st}><div class="body">{inner}</div><div class="lbl">{label}</div></div>'


def ib(icon, on=False, bar=None, cls=''):
    b = f'<i class="bar" style="background:{bar}"></i>' if bar else ''
    return f'<div class="ib{" on" if on else ""} {cls}">{I(icon)}{b}</div>'


def big(icon, label, app=False):
    return f'<div class="big"><div class="ic{" app" if app else ""}">{I(icon, "s24")}</div>{label}{I("chevron-down", "s14") if False else ""}</div>'


def sm(icon, label):
    return f'<div class="sm">{I(icon, "s14")}{label}</div>'


def dd(label, w):
    return f'<div class="dd" style="width:{w}px"><span>{label}</span>{I("chevron-down", "s14")}</div>'


def clipboard_grp():
    return grp('Clipboard', big('clipboard-paste', 'Paste') +
               f'<div class="col" style="gap:2px;padding-top:0">{sm("scissors", "Cut")}{sm("copy", "Copy")}{sm("paintbrush", "Format")}</div>')


def status(items_left, items_right):
    return f'<div class="sb">{items_left}<span class="sp"></span>{items_right}</div>'


def zoom(pct='110%'):
    return f'<div class="zoom">{I("minus", "s14")}<div class="track"><i></i></div>{I("plus", "s14")}<span class="tnum" style="width:36px">{pct}</span></div>'


def autosave(text='Saved just now'):
    return f'<div class="it">{flame(10, 12)}<span>{text}</span></div>'
