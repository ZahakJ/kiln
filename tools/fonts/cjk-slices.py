#!/usr/bin/env python3
"""Makes the web core's Chinese, Japanese and Korean fallback font: Noto Sans CJK SC, whole, cut
into slices by how common their characters are, as web font services serve CJK fonts. The web core
fetches only the slices a document's text needs (assets/fonts/noto/cjk/slices.json says which
characters each one has). Every slice keeps every OpenType feature of the font for its glyphs:
`locl` (Japanese, Korean and Traditional Chinese forms, by the text's language), `vert`, the
jamo features (`ljmo`, `vjmo`, `tjmo`), `ccmp`, `palt`, `kern` and the rest.

  python3 -m venv .venv-fonts && .venv-fonts/bin/pip install fonttools==4.68.0
  .venv-fonts/bin/python tools/fonts/cjk-slices.py NotoSansCJK-Regular.ttc

The order of the slices, most common first:
  1. CJK punctuation and symbols, kana, fullwidth forms, bopomofo, Hangul letters;
  2. the 2,350 Hangul syllables of KS X 1001;
  3. GB 2312's level 1 characters (3,755, the common Chinese ones);
  4. JIS X 0208's level 1 kanji and Big5's common characters not in 3;
  5. GB 2312's level 2, JIS X 0208's level 2 and KS X 1001's hanja not yet in;
  6. the other 8,822 Hangul syllables;
  7. everything else the font has (the rest of the CJK ideographs, extensions A to G, old Hangul
     with its conjoining jamo, and the font's Latin, Greek and Cyrillic).
"""
import json
import pathlib
import sys

from fontTools import subset
from fontTools.ttLib import TTCollection, TTFont

OUT = pathlib.Path('assets/fonts/noto/cjk')
FAMILY = 'Noto Sans CJK SC'
# Characters per slice, by tier: Han and Hangul glyphs are of a size, so slices come out alike.
HAN_PER_SLICE = 400
HANGUL_PER_SLICE = 1200


def chars(codec, lead, trail):
    out = []
    for a in lead:
        for b in trail:
            try:
                out.extend(bytes([a, b]).decode(codec))
            except UnicodeDecodeError:
                pass
    return [ord(c) for c in out]


def tiers(cmap):
    seen = set()

    def take(cps):
        got = [c for c in cps if c in cmap and c not in seen]
        seen.update(got)
        return got

    common = []
    for lo, hi in [(0x3000, 0x303F), (0x3040, 0x30FF), (0x31F0, 0x31FF), (0xFF00, 0xFFEF),
                   (0xFE10, 0xFE1F), (0xFE30, 0xFE4F), (0x3100, 0x312F), (0x31A0, 0x31BF),
                   (0x3130, 0x318F), (0x3200, 0x32FF), (0x3300, 0x33FF), (0x31C0, 0x31EF),
                   (0x3190, 0x319F), (0x2E80, 0x2FFF)]:
        common.extend(range(lo, hi + 1))
    hangul_common = [c for c in chars('euc_kr', range(0xB0, 0xC9), range(0xA1, 0xFF))
                     if 0xAC00 <= c <= 0xD7A3]
    gb1 = chars('gb2312', range(0xB0, 0xD8), range(0xA1, 0xFF))
    jis1 = chars('euc_jp', range(0xB0, 0xD0), range(0xA1, 0xFF))
    big5 = chars('big5', range(0xA4, 0xC7), list(range(0x40, 0x7F)) + list(range(0xA1, 0xFF)))
    gb2 = chars('gb2312', range(0xD8, 0xF8), range(0xA1, 0xFF))
    jis2 = chars('euc_jp', range(0xD0, 0xF5), range(0xA1, 0xFF))
    hanja = [c for c in chars('euc_kr', range(0xCA, 0xFE), range(0xA1, 0xFF)) if c >= 0x3400]
    return [
        ('common', take(common), HANGUL_PER_SLICE),
        ('hangul', take(hangul_common), HANGUL_PER_SLICE),
        ('han1', take(gb1), HAN_PER_SLICE),
        ('han2', take(jis1 + big5), HAN_PER_SLICE),
        ('han3', take(gb2 + jis2 + hanja), HAN_PER_SLICE),
        ('hangul2', take(range(0xAC00, 0xD7A4)), HANGUL_PER_SLICE),
        ('rest', take(sorted(cmap)), HAN_PER_SLICE),
    ]


def ranges(cps):
    out = []
    for c in sorted(cps):
        if out and out[-1][1] == c - 1:
            out[-1][1] = c
        else:
            out.append([c, c])
    return out


def encode(rs):
    """The ranges as the web core reads them: hex tokens, each the start's distance from the end
    of the range before (`d`), and `d+n` for a range of n + 1 characters."""
    out, prev = [], 0
    for a, b in rs:
        out.append(f'{a - prev:x}' + ('' if a == b else f'+{b - a:x}'))
        prev = b
    return ' '.join(out)


def rename(font, name):
    table = font['name']
    for rec in table.names:
        if rec.nameID in (1, 16):
            rec.string = name
        elif rec.nameID == 4:
            rec.string = name
        elif rec.nameID == 6:
            rec.string = name.replace(' ', '')
        elif rec.nameID == 3:
            rec.string = f'{rec.toUnicode()};{name}'


def main(ttc):
    source = pathlib.Path(ttc)
    cmap = TTCollection(source).fonts[2].getBestCmap()
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob('*.otf'):
        old.unlink()
    slices = []
    for tier, cps, per in tiers(cmap):
        for i in range(0, len(cps), per):
            part = cps[i:i + per]
            n = len(slices)
            name = f'{FAMILY} {n:02d}'
            font = TTFont(source, fontNumber=2)
            opts = subset.Options()
            opts.layout_features = ['*']
            opts.name_IDs = ['*']
            opts.name_languages = ['*']
            opts.notdef_outline = True
            opts.hinting = False
            opts.glyph_names = False
            sub = subset.Subsetter(opts)
            sub.populate(unicodes=part)
            sub.subset(font)
            rename(font, name)
            file = f'NotoSansCJKsc-{n:02d}.otf'
            font.save(OUT / file)
            slices.append({'file': file, 'family': name, 'ranges': encode(ranges(part))})
            print(file, tier, len(part), (OUT / file).stat().st_size)
    (OUT / 'slices.json').write_text(json.dumps({'family': FAMILY, 'slices': slices},
                                                separators=(',', ':')) + '\n')


if __name__ == '__main__':
    main(sys.argv[1])
