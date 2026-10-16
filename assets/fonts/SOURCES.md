# Bundled document fonts

These are the document fonts `kiln-text` bundles on every platform so that line breaks match Microsoft
Office without the Microsoft fonts (PLAN.md §1.2 item 7, §3.6). All three families are licensed under
the SIL Open Font License 1.1; each directory carries the licence as `OFL.txt`. They are metric-compatible
substitutes: the substitution table in `crates/kiln-text/src/substitution.rs` maps the Office names to them.

| Directory | Family | Metric-compatible with | Version | Upstream |
|---|---|---|---|---|
| `liberation/` | Liberation Sans, Serif, Mono | Arial, Times New Roman, Courier New | 2.1.5 | release tarball `liberation-fonts-ttf-2.1.5.tar.gz` |
| `carlito/` | Carlito | Calibri | upstream commit `3a810cab78ebd6e2e4eed42af9e8453c4f9b850a` | googlefonts/carlito |
| `caladea/` | Caladea | Cambria | upstream commit `336a529cfad3d103d6527752686f8331d13e820a` | huertatipografica/Caladea |

## Download URLs

- Liberation 2.1.5: <https://github.com/liberationfonts/liberation-fonts/files/7261482/liberation-fonts-ttf-2.1.5.tar.gz>
  (SHA-256 of the archive `7191c669bf38899f73a2094ed00f7b800553364f90e2637010a69c0e268f25d0`). The TTFs were
  extracted unchanged; the archive's `LICENSE` file is stored as `liberation/OFL.txt`.
- Carlito: `https://raw.githubusercontent.com/googlefonts/carlito/3a810cab78ebd6e2e4eed42af9e8453c4f9b850a/<path>`
  for `OFL.txt` and `fonts/ttf/Carlito-{Regular,Bold,Italic,BoldItalic}.ttf`.
- Caladea: `https://raw.githubusercontent.com/huertatipografica/Caladea/336a529cfad3d103d6527752686f8331d13e820a/<path>`
  for `OFL.txt` and `fonts/ttf/Caladea-{Regular,Bold,Italic,BoldItalic}.ttf`.

## Checksums

Verify with `cd assets/fonts && sha256sum -c SOURCES.md` (the lines below are in `sha256sum` format; the
other lines of this file are ignored with a warning).

```
ccabaa7b7e2fdf253d2b1a5fa699dd8a3df8d835a9eb285ad82631a677eb76c0  caladea/Caladea-BoldItalic.ttf
ae3cb2dcbc925809dd29d2a44e9802211cab66be541bacbfc9c08c74b27c3742  caladea/Caladea-Bold.ttf
4359a8e24f748b6447b1ff6d7a174febe70961d29f8bb8634b56dacd740a3deb  caladea/Caladea-Italic.ttf
f1e899278b7b4491aba5b6a8253c4b04c050cc59b21865be5c37559a775153cd  caladea/Caladea-Regular.ttf
b32928186c119599e03ca6a1ffc680fdcb7fac95772f4b95d989cf6cd3861517  carlito/Carlito-BoldItalic.ttf
bb5d20f79b82599ec72983597437373a80f2d2085fa91fc144fd74e876a594db  carlito/Carlito-Bold.ttf
0b019225e58d702bfedcbd35c21696769f8ee115cb6343f84c2f240312450d1c  carlito/Carlito-Italic.ttf
f6418f708baede9789daef5d458c0f53d2a888af9820e8062934e504fedc6595  carlito/Carlito-Regular.ttf
79451f3c09fe25116098853b7a2ca6e2436220ccc11af022979adbcf195be130  liberation/LiberationMono-BoldItalic.ttf
bd62a0672d0b9b6710b01df434c80ad54fa5f0835207eb7b17b7a761463067bb  liberation/LiberationMono-Bold.ttf
605c01c711b44480a7508d349dfbf3264e81fa43d69e61cfa7d10b86e764c4d1  liberation/LiberationMono-Italic.ttf
f2b83c763e8afd21709333370bed4774337fae82267937e2b5aea7e2fbd922c1  liberation/LiberationMono-Regular.ttf
698da70fc191cc5f33ad4d6d3fe830fe4624b898ea2e3169955928b7c491f1ee  liberation/LiberationSans-BoldItalic.ttf
788abee4c806d660e8aee46689dd8540cd4bb98da03dcc9d171ce3efd99a9173  liberation/LiberationSans-Bold.ttf
e5bae5c4cde31f22142753855f4f8fb86da6ff39955ed3c0a11248b0d16948b0  liberation/LiberationSans-Italic.ttf
76d04c18ea243f426b7de1f3ad208e927008f961dc5945e5aad352d0dfde8ee8  liberation/LiberationSans-Regular.ttf
f17db8af71e24d2066b587546021d4f0b296be389512b658dec3c09affeb11a7  liberation/LiberationSerif-BoldItalic.ttf
d754ba427cfe0bca54ae052384baa8f842da5bd6550ad4da024ac441e7a7d5ce  liberation/LiberationSerif-Bold.ttf
0e3dea9f8d613e006ccfa62201f33e265d19167bd0907725c3e145368b04fc2e  liberation/LiberationSerif-Italic.ttf
058ea80864aef09a23f45cbec2bb5400bc3dfbdea01c3f10538a21fcb497fb74  liberation/LiberationSerif-Regular.ttf
ccdab61d371d8c8683a128a92cd7d498dbdb1d37689f7cb21f1bf6b16658d213  caladea/OFL.txt
58402f82a7c332a700294988fe7554fbb0a63a8d27ccc1ee3bbc640311990a00  carlito/OFL.txt
93fed46019c38bbe566b479d22148e2e8a1e85ada614accb0211c37b2c61c19b  liberation/OFL.txt
```
