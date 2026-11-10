# Bundled document fonts

These are the document fonts `kiln-text` bundles on every platform so that line breaks match Microsoft
Office without the Microsoft fonts. All three families are licensed under
the SIL Open Font License 1.1; each directory carries the licence as `OFL.txt`. They are metric-compatible
substitutes: the substitution table in `crates/kiln-text/src/substitution.rs` maps the Office names to them.

| Directory | Family | Metric-compatible with | Version | Upstream |
|---|---|---|---|---|
| `liberation/` | Liberation Sans, Serif, Mono | Arial, Times New Roman, Courier New | 2.1.5 | release tarball `liberation-fonts-ttf-2.1.5.tar.gz` |
| `carlito/` | Carlito | Calibri | upstream commit `3a810cab78ebd6e2e4eed42af9e8453c4f9b850a` | googlefonts/carlito |
| `caladea/` | Caladea | Cambria | upstream commit `336a529cfad3d103d6527752686f8331d13e820a` | huertatipografica/Caladea |
| `noto/` | Noto Naskh Arabic (Regular, Bold), Noto Sans Symbols 2, Noto Sans Math | — (fallbacks for Arabic text, symbols such as ☃ ✓ ♠, and equations) | 2.021, 2.008, 3.000 | notofonts/arabic, notofonts/symbols, notofonts/math (Noto release of 2026-08-01) |
| `noto/` | Noto Sans Arabic (Regular, Medium, Bold) | — (the interface font for Arabic; `@kiln/i18n/fonts.css`) | 2.013 | notofonts/arabic (Noto release of 2026-08-01) |
| `noto/cjk/` | Noto Sans CJK SC, whole, in 98 slices (`NotoSansCJKsc-00.otf` to `-97.otf`, map in `slices.json`) | — (Chinese, Japanese and Korean text in documents: the web core fetches the slices a document's characters are in) | 2.004 | notofonts/noto-cjk, `NotoSansCJK-Regular.ttc` (SHA-256 `b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a`), face 2 |
| `noto/` | Noto Emoji, the Regular instance (`NotoEmoji-Regular.ttf`) | — (emoji in documents, monochrome) | 3.002 | googlefonts/noto-emoji, `NotoEmoji[wght].ttf` (SHA-256 `de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551`) |

## Download URLs

- Liberation 2.1.5: <https://github.com/liberationfonts/liberation-fonts/files/7261482/liberation-fonts-ttf-2.1.5.tar.gz>
  (SHA-256 of the archive `7191c669bf38899f73a2094ed00f7b800553364f90e2637010a69c0e268f25d0`). The TTFs were
  extracted unchanged; the archive's `LICENSE` file is stored as `liberation/OFL.txt`.
- Carlito: `https://raw.githubusercontent.com/googlefonts/carlito/3a810cab78ebd6e2e4eed42af9e8453c4f9b850a/<path>`
  for `OFL.txt` and `fonts/ttf/Carlito-{Regular,Bold,Italic,BoldItalic}.ttf`.
- Caladea: `https://raw.githubusercontent.com/huertatipografica/Caladea/336a529cfad3d103d6527752686f8331d13e820a/<path>`
  for `OFL.txt` and `fonts/ttf/Caladea-{Regular,Bold,Italic,BoldItalic}.ttf`.

- Noto: the TTFs of the `noto-fonts` 2026.08.01 release of the Noto project (<https://github.com/notofonts>),
  unchanged; each font's name table says SIL Open Font License 1.1, stored as `noto/OFL.txt`.
- Noto Sans CJK SC slices: `tools/fonts/cjk-slices.py` (fontTools 4.68) cuts the font's 44,810 characters
  into 98 slices, the most common first (CJK punctuation and kana; KS X 1001's Hangul; GB 2312's level 1;
  JIS X 0208's level 1 and Big5's common characters; the second levels and KS X 1001's hanja; the rest of
  Hangul; everything else), each with every OpenType feature of the font for its glyphs (`locl` for
  Japanese, Korean and Traditional Chinese forms, `vert`, `ljmo`, `vjmo`, `tjmo`, `palt`, `kern` and the
  rest) and a family of its own (`Noto Sans CJK SC 00` and on); 15.4 MB in all, at most 0.33 MB each,
  fetched only when a document has their characters.
  Noto Emoji: the static Regular instance of the variable font (`fontTools.varLib.instancer`,
  `wght=400`, `tools/fonts/emoji-instance.py`); 0.9 MB, 0.6 MB gzipped. Both are licensed under the SIL
  Open Font License 1.1.

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
04017b0ccdd1156dcde3bc386734c1b03165261762f16a025c8afc9088ae76e9  noto/NotoNaskhArabic-Bold.ttf
6f0a92031367b2f5a2078fe9d24f3433122b61a0bad57c423aad8f3c39aa2e6e  noto/NotoNaskhArabic-Regular.ttf
d51afd5739c7ba6c44fcab35a88160e25dfb69a2d4ad0bd99533f8d894af1f96  noto/NotoSansMath-Regular.ttf
c4a0a80f0041ce4be81e2478faad22776d23edb98ae3f0d19bd37044820ecf9d  noto/NotoSansSymbols2-Regular.ttf
4e5462d2e8be880317b9f49b5b2da109ddb6a3563d91cc604b67f3535832a555  noto/NotoSansArabic-Bold.ttf
5aaa32f0fe7fdd828342aea06a3c645edb51bcea2620af68b4c385a3b9fb0c42  noto/NotoSansArabic-Medium.ttf
bdff3e5659d67e67def05b33f749683b9376ae819d65d3dd62ac4640b3aaef48  noto/NotoSansArabic-Regular.ttf
f7c2f1b69d750178895b191edc6a755e291ad1158b1a1392a9d74f0dd648d42e  noto/NotoEmoji-Regular.ttf
d5074d803beea37ea5a5356abd0414eb635be6cb7ad2991e51a538ddab06dc69  noto/OFL.txt
d064e68205cbdafeed001ee208f962fb59067062bb67d3032b6d3bbf61c79bcf  noto/cjk/slices.json
f7d24ad906e6afebc72762574ac56e5a3c2c95e2b78964544e5e5d1ca6fe35b4  noto/cjk/NotoSansCJKsc-00.otf
7e97c136bcf56f248550e52178b70610964513d9d5ebe4a739330406a95be6ff  noto/cjk/NotoSansCJKsc-01.otf
f90d41232f18570674fb5183b81877741b701a3603f14555f48eb09ad1d39ba2  noto/cjk/NotoSansCJKsc-02.otf
1ed788e1b5dcdaaa5fe0a0c70ebb4476c8d63cc120357ba837c743ae86851998  noto/cjk/NotoSansCJKsc-03.otf
5872912e50cb3a1131d612ccb81a5b93d3b573c3d00e56e206cef662f05b00fa  noto/cjk/NotoSansCJKsc-04.otf
cb738df8f95cab23ba7986499c0f4ac2e27c2b28d297c4b28344757dfe3ba27e  noto/cjk/NotoSansCJKsc-05.otf
edd7b32bb5debee4499581db0bcba5f37cb638e047dd14c433ac8a95fcd93b65  noto/cjk/NotoSansCJKsc-06.otf
0edf420cc34db13a85349ee8ea6dc86b787b6d50be21cd0e02b7a6804bb444fb  noto/cjk/NotoSansCJKsc-07.otf
38334a8e3209ff6512cdbb4a5e571799653d85b6a27a1e083c8287bff701d2ac  noto/cjk/NotoSansCJKsc-08.otf
c811c9eb40e8ec8de4780dc3c8e500d1b6111e34a7136e67ca8d3a782348cd00  noto/cjk/NotoSansCJKsc-09.otf
47c42b67d83f6b0cc553e97dc4410e100ad174ad22d21978967beb15b66931f5  noto/cjk/NotoSansCJKsc-10.otf
cc1230101d9bbe7b279c3ef62cfa0b233a5246a87080a19c0c39b7e2f8d7b17f  noto/cjk/NotoSansCJKsc-11.otf
4b7aa682aa939d573e22cfe035e3c8d178e91e0fe98ec1bb7b58d7cbd19be5e1  noto/cjk/NotoSansCJKsc-12.otf
d7f9a55a3d26c3a9d1d6806574d6694f8424cfa7925b7c7747eeddf79dc49d53  noto/cjk/NotoSansCJKsc-13.otf
671c2c1382a729286ddd44a792cd95b591ea726c1f4800f53ec02149c1118d63  noto/cjk/NotoSansCJKsc-14.otf
2c890028583706b0d3842ba17fa07f5cdefe2cc21faafaf7fbfe3df03840d5bd  noto/cjk/NotoSansCJKsc-15.otf
279ca031fcf90727fb33aedf7c8798585cf3da5db52348e3d68640af91bbffc0  noto/cjk/NotoSansCJKsc-16.otf
a7f1d6c9c49dec69d3e3108a59425a7925b4e18e67952c0ced2eb47c7b65efac  noto/cjk/NotoSansCJKsc-17.otf
1de6a42757074693d5188fb92932a93346a4d0508a7964df5b006ba6e5a8631c  noto/cjk/NotoSansCJKsc-18.otf
3a2ef2c1e6e41ac29865442c2d2bdcfe037c35db2de5a1631ddfd028ff1cfafa  noto/cjk/NotoSansCJKsc-19.otf
ab3c070ab2a87f1b532fd1211cbd392f95706d4ad811159f517bce2766760dc6  noto/cjk/NotoSansCJKsc-20.otf
a632e89ffba6be3d9d433500e9ec6cfe92f57121167ddbdd4bc921ee8f2bf0ea  noto/cjk/NotoSansCJKsc-21.otf
40c6d70055ee2ee85b529d268ff96fa91ff244b6829d56878de05b0e8e9ad709  noto/cjk/NotoSansCJKsc-22.otf
0e34e01d3f11910e51583e7c92a69acd44314d282b5b24079ab6dc02ea96b1c6  noto/cjk/NotoSansCJKsc-23.otf
214d01639fefd0da2158aae76bde9c41c11c6e7a6ac180db21e7ac46413449e3  noto/cjk/NotoSansCJKsc-24.otf
e64e885fb83b82311ea68f9b43ca1728520aa611021224621a80af72f85dc49d  noto/cjk/NotoSansCJKsc-25.otf
40cf044ae0d27d9ad3555b67b12c2112451d28b5db2002d4ee215a3993f4d284  noto/cjk/NotoSansCJKsc-26.otf
fa5ef984d37682c3639a340c893010751fb60d82ff3909b1680a31eb1db0b138  noto/cjk/NotoSansCJKsc-27.otf
da331eb7d6a5ec7862da838cc2351f72b72f79766421417938ce556e05e9bc32  noto/cjk/NotoSansCJKsc-28.otf
563703786d1e9377bfeb792c60460e40362cc17ff639b7742385b3f5ae6f4413  noto/cjk/NotoSansCJKsc-29.otf
b7b5efdf97cdd9b822dd1afc527bdc3b77b9a57405d36c1acd2875839bc4f1bf  noto/cjk/NotoSansCJKsc-30.otf
ad32839c7ad37bb627034515d354560f6f881e1d579579ab076a7fba830da6b2  noto/cjk/NotoSansCJKsc-31.otf
4fb1217b7bc8b0b985ebdd67b097f08088726430ccde47a7e2350b3b6fbf7157  noto/cjk/NotoSansCJKsc-32.otf
fa13bd4488602b889523c752fa637e355a6a38df78a46398b2db7aa0fa0abcb0  noto/cjk/NotoSansCJKsc-33.otf
ab8a3bd0be2b5c259f602d783b959226fda1504bb3fef6851cd6b79829789599  noto/cjk/NotoSansCJKsc-34.otf
1693d5e2e1c02d7d99511dda60e3a96cf3d5e6e103dedee45660a5e89f7c97fd  noto/cjk/NotoSansCJKsc-35.otf
08074d77323844967b7ff3c92919aae2662f001138b5d6fcace2d645a9fa3734  noto/cjk/NotoSansCJKsc-36.otf
08260164f343bebc0709950a20a2db47c0d14d4a9ef24840a88dc731e0b44eed  noto/cjk/NotoSansCJKsc-37.otf
3ce66190b45e5cffef9f00fd8270b18a40fc695cb9d7039e1b2d70370b840240  noto/cjk/NotoSansCJKsc-38.otf
519d374e9d857e8914575dfaa51604533faf71199d7acd7886390583fc8abf7d  noto/cjk/NotoSansCJKsc-39.otf
7c05ac9b00b6d3082e20ccafaa33a998d8a901d79619fd7cbe5e45508299d46c  noto/cjk/NotoSansCJKsc-40.otf
1f47864630fac92a45b35a9f33fd1558d8cd3c33d5014c5691fdff8b49155da4  noto/cjk/NotoSansCJKsc-41.otf
7850d9812336c7429f7dba2f302e4b0e211a1eb448ad3b7af7e79a8ec2b63df1  noto/cjk/NotoSansCJKsc-42.otf
ebdc5d5f399c3499cde256e755bab23418226416e89f7f9a0f60d46c692905d9  noto/cjk/NotoSansCJKsc-43.otf
a61cf6f7f77978c4b978839df43faf93d2d5a82517168f078525f8ab6b0f07a0  noto/cjk/NotoSansCJKsc-44.otf
95d76155c56d7788bf489490aa6d58bbc793520723350dd94748cdc6174785e4  noto/cjk/NotoSansCJKsc-45.otf
e0ed00a358c22bfc80b40cf9685f0e4e104517404e289028d5c861d32d32140d  noto/cjk/NotoSansCJKsc-46.otf
eaf65c7a91f38d99b3b1a2cb5b34f35229b2fa5954443a9e7c8c78becc4d656c  noto/cjk/NotoSansCJKsc-47.otf
cda9685062a9530991c254c922654a9983e1b2d05931e11c416adc8dd6b785ed  noto/cjk/NotoSansCJKsc-48.otf
296ef9e83b686ed680bb6e7e8912b283da5c753d46aa74fc6ad5c9356f817432  noto/cjk/NotoSansCJKsc-49.otf
37bf1c8d71ec3159f06f90a18c846dab37b8fe82ef30b05d89d2b872e5e7e81e  noto/cjk/NotoSansCJKsc-50.otf
deabb3e6c95761d69b48c29e81c2dff100df1731104f707a0553fef4b1670872  noto/cjk/NotoSansCJKsc-51.otf
d75e3a3c274ed9cc11d82e9c584955a6ffd3e494d75fa6601d73cc7ce7b24735  noto/cjk/NotoSansCJKsc-52.otf
c809e2ae90e0f5e5f83136599e597740c45c0408456c302eff9d0ffde76f8690  noto/cjk/NotoSansCJKsc-53.otf
a1d856c63e7f97ec18689d59adb5fe86e93eac0dc82b67a54ccfc78aad5dff12  noto/cjk/NotoSansCJKsc-54.otf
60cdc93c67db57bb37e919e536e915f28efe57327f74892951e0c2d968f6cefd  noto/cjk/NotoSansCJKsc-55.otf
1c480df4ad85e505823300904715d6d203c921cbd3fac984a4a62aca844b5601  noto/cjk/NotoSansCJKsc-56.otf
82542146be451057893741b0e13d2b03f2ebdfc365bf087ccef7d014f4780b47  noto/cjk/NotoSansCJKsc-57.otf
f4773bb68cc2209bc024f8c3155c54cc87644f06680efa2e0d3cfa98f93bdfab  noto/cjk/NotoSansCJKsc-58.otf
f876d102b57ee7133293a3b8b881b24e7cd76fad308e9b854144c80bc8d1f71b  noto/cjk/NotoSansCJKsc-59.otf
df5cb0340b400e8d7061be109ca7460100d7dc2a545f5da255ec5785cc028058  noto/cjk/NotoSansCJKsc-60.otf
e44e1a2b6b3718791c0d972bf8df9a5cbac2531c1c6905797da5b2d0957e8046  noto/cjk/NotoSansCJKsc-61.otf
5c168a4f2a9becb1c1864ddfffd55acc5a27169ea68070ce94415ca4815fbdbd  noto/cjk/NotoSansCJKsc-62.otf
99da46a6561e1ebe642aded6703768674819b6a413432299807ae2b38f5839f8  noto/cjk/NotoSansCJKsc-63.otf
c14b3723e98e7348ddcccc1a6effafdaa5e88c31c691816e65086a0c99c968fb  noto/cjk/NotoSansCJKsc-64.otf
f80212ad31b2bcb89dbca7ac6e8dd49719ff4d45afeb66cb7c6964371ed17d4e  noto/cjk/NotoSansCJKsc-65.otf
3645c592564bc632185a6d001d7ac5980e938c9bc2d63d293dfb057067225c3e  noto/cjk/NotoSansCJKsc-66.otf
7ee42a34dcd35c877b298177dc686ef3501ae48c4704c04eaa50e700ca44c64b  noto/cjk/NotoSansCJKsc-67.otf
d1c471e74d16c9b6efbc180f6abe342d381d6b90823184e7668043c0bf152358  noto/cjk/NotoSansCJKsc-68.otf
74867eff4909c4d25f3deff1a6d4c5b7aff56de055fc690d7aab914fdd01b0c7  noto/cjk/NotoSansCJKsc-69.otf
e47d6575daa640bbaf2809ed7af24341dfd2845492ce0959fffd9c404b6b56a0  noto/cjk/NotoSansCJKsc-70.otf
80689199d316bd1e96cd282ddd2b8145b99bc3f342f573381dc3323d444fc4c2  noto/cjk/NotoSansCJKsc-71.otf
6f3a6a6c030a33119826abed23d8158dbd9341de18a63d1e8961078ef53acea8  noto/cjk/NotoSansCJKsc-72.otf
7c16c882ce09244ffbd353597e21b3c87023ebf51d9240e249398938b7f9670c  noto/cjk/NotoSansCJKsc-73.otf
ba95d4452a8f658f6a37590e650227101a478d82f9cd42131e71f1cdfa3e2d15  noto/cjk/NotoSansCJKsc-74.otf
bc09bb6c583d4c606be13e826d303265a4aee6c97948cf1dc4b64482badae32d  noto/cjk/NotoSansCJKsc-75.otf
8664b3c405d82f9bec2dd88c11045164c57d843bfc310d18b17fe82f440c5c70  noto/cjk/NotoSansCJKsc-76.otf
a431956f911e05313834b734f12c6620f8b4410dfebd5bd9b10a6084ad76aaf1  noto/cjk/NotoSansCJKsc-77.otf
40eb9b9125f3fd93350749e7a8f5fbf3c1e625bdee4bdfd44801eb902e3546c2  noto/cjk/NotoSansCJKsc-78.otf
472366a9ed57f551ede0033f003aeabc839561bc80111e2ab8a11a934594e031  noto/cjk/NotoSansCJKsc-79.otf
672c5a2daba375a86dec598d9543fd69c0384e68c4a5ed457a7bf3d48c873eeb  noto/cjk/NotoSansCJKsc-80.otf
efc8996c6b5997ab94ee74e6044b4225edcb4a237f5700104605d56d4bc808f7  noto/cjk/NotoSansCJKsc-81.otf
45809f608d92b796dbe31ed96a6013719671613e9925c5fc52b2d0597e2aca85  noto/cjk/NotoSansCJKsc-82.otf
d077f42b3683f600f7a240a92d145df63c7bb1d6dbe0e52e1d9edcc4d11db4dc  noto/cjk/NotoSansCJKsc-83.otf
22edca8f358b7683e54b966e73b3342d20d5e325906a744ad15611aaa0232e89  noto/cjk/NotoSansCJKsc-84.otf
cb707d800ab62e62604ada1694b6870412ca7eb3e8cecc67aaeb8999193dd1ee  noto/cjk/NotoSansCJKsc-85.otf
4dc43bf8a78101019fa599c919052ca1402e45b63620c135e3ed498f065ae72f  noto/cjk/NotoSansCJKsc-86.otf
8221ff513497987ffc80993b35fc9c03515944eac457e16f73b515a66bfddb04  noto/cjk/NotoSansCJKsc-87.otf
e187a4e4dc3e495b6402eaef1ef56cd734ebe0ab528e3e37574f7c9634d80f6d  noto/cjk/NotoSansCJKsc-88.otf
9cb1c5d752979465aa6a834232f32c2516390f937f65e4e0336f177b5393a39e  noto/cjk/NotoSansCJKsc-89.otf
9b0778226a8749ca4c1aa6abb4ae6753be37bb0f3698ee148b5bb68efa9c6587  noto/cjk/NotoSansCJKsc-90.otf
3526523cb99c4235c092ba828571db5b1ee2262e5da63fc06612a547fd8da50e  noto/cjk/NotoSansCJKsc-91.otf
282d0b9e22ac07a0ff2f6fdb2c3789918e87f93517f4e2a5a006abb6dcef57b5  noto/cjk/NotoSansCJKsc-92.otf
0d73ce0b8110eca2c97cab2ff5d2a155569f699b926d681ec9b255c0fed4da96  noto/cjk/NotoSansCJKsc-93.otf
9bc9c4ba3fdf36e52c5ba90a674631489bd182ef1a7bf3a7f3360cc48ed96c5b  noto/cjk/NotoSansCJKsc-94.otf
0fabf73a21167753ff98da8c13765d3fa1e7969c3cc8bb373ab84ec09162017b  noto/cjk/NotoSansCJKsc-95.otf
82f3a08fe8266661019da57d2cf9d9673da57ecc607e9958477d9626ff4d9a65  noto/cjk/NotoSansCJKsc-96.otf
d6a8b45d6dbebcd49c52de3694be93041908321c158360cf0c5440106c08babb  noto/cjk/NotoSansCJKsc-97.otf
```
