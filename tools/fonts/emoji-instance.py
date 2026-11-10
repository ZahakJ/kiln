#!/usr/bin/env python3
"""Makes the web core's emoji fallback font (assets/fonts/noto/, see SOURCES.md): the static
Regular instance of Noto Emoji's variable font.

  python3 -m venv .venv-fonts && .venv-fonts/bin/pip install fonttools==4.68.0
  .venv-fonts/bin/python tools/fonts/emoji-instance.py 'NotoEmoji[wght].ttf'

The CJK font's slices come from tools/fonts/cjk-slices.py.
"""
import sys

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer


def main(emoji):
    instancer.instantiateVariableFont(TTFont(emoji), {'wght': 400}).save(
        'assets/fonts/noto/NotoEmoji-Regular.ttf')


if __name__ == '__main__':
    main(sys.argv[1])
