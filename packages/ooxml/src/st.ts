/**
 * ECMA-376 simple types (`ST_*`) used by more than one format, as `const` tuples with matching string
 * unions and type guards. Preset shape names (ST_ShapeType) live in `drawingml/presets`.
 */

function guard<T extends string>(values: readonly T[]): (v: unknown) => v is T {
  const set = new Set<string>(values);
  return (v: unknown): v is T => typeof v === 'string' && set.has(v);
}

export const ST_SchemeColorVal = [
  'bg1',
  'tx1',
  'bg2',
  'tx2',
  'accent1',
  'accent2',
  'accent3',
  'accent4',
  'accent5',
  'accent6',
  'hlink',
  'folHlink',
  'phClr',
  'dk1',
  'lt1',
  'dk2',
  'lt2',
] as const;
export type ST_SchemeColorVal = (typeof ST_SchemeColorVal)[number];
export const isSchemeColorVal = guard(ST_SchemeColorVal);

/** The twelve colour slots of `a:clrScheme`, in schema order. */
export const ST_ColorSchemeIndex = [
  'dk1',
  'lt1',
  'dk2',
  'lt2',
  'accent1',
  'accent2',
  'accent3',
  'accent4',
  'accent5',
  'accent6',
  'hlink',
  'folHlink',
] as const;
export type ST_ColorSchemeIndex = (typeof ST_ColorSchemeIndex)[number];
export const isColorSchemeIndex = guard(ST_ColorSchemeIndex);

export const ST_SystemColorVal = [
  'scrollBar',
  'background',
  'activeCaption',
  'inactiveCaption',
  'menu',
  'window',
  'windowFrame',
  'menuText',
  'windowText',
  'captionText',
  'activeBorder',
  'inactiveBorder',
  'appWorkspace',
  'highlight',
  'highlightText',
  'btnFace',
  'btnShadow',
  'grayText',
  'btnText',
  'inactiveCaptionText',
  'btnHighlight',
  '3dDkShadow',
  '3dLight',
  'infoText',
  'infoBk',
  'hotLight',
  'gradientActiveCaption',
  'gradientInactiveCaption',
  'menuHighlight',
  'menuBar',
] as const;
export type ST_SystemColorVal = (typeof ST_SystemColorVal)[number];
export const isSystemColorVal = guard(ST_SystemColorVal);

export const ST_TextAlignType = ['l', 'ctr', 'r', 'just', 'justLow', 'dist', 'thaiDist'] as const;
export type ST_TextAlignType = (typeof ST_TextAlignType)[number];
export const isTextAlignType = guard(ST_TextAlignType);

export const ST_TextAnchoringType = ['t', 'ctr', 'b', 'just', 'dist'] as const;
export type ST_TextAnchoringType = (typeof ST_TextAnchoringType)[number];
export const isTextAnchoringType = guard(ST_TextAnchoringType);

export const ST_TextVerticalType = [
  'horz',
  'vert',
  'vert270',
  'wordArtVert',
  'eaVert',
  'mongolianVert',
  'wordArtVertRtl',
] as const;
export type ST_TextVerticalType = (typeof ST_TextVerticalType)[number];
export const isTextVerticalType = guard(ST_TextVerticalType);

export const ST_TextWrappingType = ['none', 'square'] as const;
export type ST_TextWrappingType = (typeof ST_TextWrappingType)[number];
export const isTextWrappingType = guard(ST_TextWrappingType);

export const ST_TextHorzOverflowType = ['overflow', 'clip'] as const;
export type ST_TextHorzOverflowType = (typeof ST_TextHorzOverflowType)[number];
export const ST_TextVertOverflowType = ['overflow', 'ellipsis', 'clip'] as const;
export type ST_TextVertOverflowType = (typeof ST_TextVertOverflowType)[number];

export const ST_TextUnderlineType = [
  'none',
  'words',
  'sng',
  'dbl',
  'heavy',
  'dotted',
  'dottedHeavy',
  'dash',
  'dashHeavy',
  'dashLong',
  'dashLongHeavy',
  'dotDash',
  'dotDashHeavy',
  'dotDotDash',
  'dotDotDashHeavy',
  'wavy',
  'wavyHeavy',
  'wavyDbl',
] as const;
export type ST_TextUnderlineType = (typeof ST_TextUnderlineType)[number];
export const isTextUnderlineType = guard(ST_TextUnderlineType);

export const ST_TextStrikeType = ['noStrike', 'sngStrike', 'dblStrike'] as const;
export type ST_TextStrikeType = (typeof ST_TextStrikeType)[number];
export const isTextStrikeType = guard(ST_TextStrikeType);

export const ST_TextCapsType = ['none', 'small', 'all'] as const;
export type ST_TextCapsType = (typeof ST_TextCapsType)[number];
export const isTextCapsType = guard(ST_TextCapsType);

export const ST_TextFontAlignType = ['auto', 't', 'ctr', 'base', 'b'] as const;
export type ST_TextFontAlignType = (typeof ST_TextFontAlignType)[number];

export const ST_TextAutonumberScheme = [
  'alphaLcParenBoth',
  'alphaUcParenBoth',
  'alphaLcParenR',
  'alphaUcParenR',
  'alphaLcPeriod',
  'alphaUcPeriod',
  'arabicParenBoth',
  'arabicParenR',
  'arabicPeriod',
  'arabicPlain',
  'romanLcParenBoth',
  'romanUcParenBoth',
  'romanLcParenR',
  'romanUcParenR',
  'romanLcPeriod',
  'romanUcPeriod',
  'circleNumDbPlain',
  'circleNumWdBlackPlain',
  'circleNumWdWhitePlain',
  'arabicDbPeriod',
  'arabicDbPlain',
  'ea1ChsPeriod',
  'ea1ChsPlain',
  'ea1ChtPeriod',
  'ea1ChtPlain',
  'ea1JpnChsDbPeriod',
  'ea1JpnKorPlain',
  'ea1JpnKorPeriod',
  'arabic1Minus',
  'arabic2Minus',
  'hebrew2Minus',
  'thaiAlphaPeriod',
  'thaiAlphaParenR',
  'thaiAlphaParenBoth',
  'thaiNumPeriod',
  'thaiNumParenR',
  'thaiNumParenBoth',
  'hindiAlphaPeriod',
  'hindiNumPeriod',
  'hindiNumParenR',
  'hindiAlpha1Period',
] as const;
export type ST_TextAutonumberScheme = (typeof ST_TextAutonumberScheme)[number];
export const isTextAutonumberScheme = guard(ST_TextAutonumberScheme);

export const ST_LineCap = ['rnd', 'sq', 'flat'] as const;
export type ST_LineCap = (typeof ST_LineCap)[number];
export const isLineCap = guard(ST_LineCap);

export const ST_CompoundLine = ['sng', 'dbl', 'thickThin', 'thinThick', 'tri'] as const;
export type ST_CompoundLine = (typeof ST_CompoundLine)[number];
export const isCompoundLine = guard(ST_CompoundLine);

export const ST_PenAlignment = ['ctr', 'in'] as const;
export type ST_PenAlignment = (typeof ST_PenAlignment)[number];

export const ST_PresetLineDashVal = [
  'solid',
  'dot',
  'dash',
  'lgDash',
  'dashDot',
  'lgDashDot',
  'lgDashDotDot',
  'sysDash',
  'sysDot',
  'sysDashDot',
  'sysDashDotDot',
] as const;
export type ST_PresetLineDashVal = (typeof ST_PresetLineDashVal)[number];
export const isPresetLineDashVal = guard(ST_PresetLineDashVal);

export const ST_LineEndType = ['none', 'triangle', 'stealth', 'diamond', 'oval', 'arrow'] as const;
export type ST_LineEndType = (typeof ST_LineEndType)[number];
export const isLineEndType = guard(ST_LineEndType);

export const ST_LineEndWidth = ['sm', 'med', 'lg'] as const;
export type ST_LineEndWidth = (typeof ST_LineEndWidth)[number];
export const isLineEndSize = guard(ST_LineEndWidth);

export const ST_PathFillMode = ['none', 'norm', 'lighten', 'lightenLess', 'darken', 'darkenLess'] as const;
export type ST_PathFillMode = (typeof ST_PathFillMode)[number];

export const ST_RectAlignment = ['tl', 't', 'tr', 'l', 'ctr', 'r', 'bl', 'b', 'br'] as const;
export type ST_RectAlignment = (typeof ST_RectAlignment)[number];
export const isRectAlignment = guard(ST_RectAlignment);

export const ST_TileFlipMode = ['none', 'x', 'y', 'xy'] as const;
export type ST_TileFlipMode = (typeof ST_TileFlipMode)[number];
export const isTileFlipMode = guard(ST_TileFlipMode);

export const ST_PathShadeType = ['shape', 'circle', 'rect'] as const;
export type ST_PathShadeType = (typeof ST_PathShadeType)[number];
export const isPathShadeType = guard(ST_PathShadeType);

export const ST_PresetPatternVal = [
  'pct5',
  'pct10',
  'pct20',
  'pct25',
  'pct30',
  'pct40',
  'pct50',
  'pct60',
  'pct70',
  'pct75',
  'pct80',
  'pct90',
  'horz',
  'vert',
  'ltHorz',
  'ltVert',
  'dkHorz',
  'dkVert',
  'narHorz',
  'narVert',
  'dashHorz',
  'dashVert',
  'cross',
  'dnDiag',
  'upDiag',
  'ltDnDiag',
  'ltUpDiag',
  'dkDnDiag',
  'dkUpDiag',
  'wdDnDiag',
  'wdUpDiag',
  'dashDnDiag',
  'dashUpDiag',
  'diagCross',
  'smCheck',
  'lgCheck',
  'smGrid',
  'lgGrid',
  'dotGrid',
  'smConfetti',
  'lgConfetti',
  'horzBrick',
  'diagBrick',
  'solidDmnd',
  'openDmnd',
  'dotDmnd',
  'plaid',
  'sphere',
  'weave',
  'divot',
  'shingle',
  'wave',
  'trellis',
  'zigZag',
] as const;
export type ST_PresetPatternVal = (typeof ST_PresetPatternVal)[number];
export const isPresetPatternVal = guard(ST_PresetPatternVal);

export const ST_BlackWhiteMode = [
  'clr',
  'auto',
  'gray',
  'ltGray',
  'invGray',
  'grayWhite',
  'blackGray',
  'blackWhite',
  'black',
  'white',
  'hidden',
] as const;
export type ST_BlackWhiteMode = (typeof ST_BlackWhiteMode)[number];

export const ST_FontCollectionIndex = ['major', 'minor', 'none'] as const;
export type ST_FontCollectionIndex = (typeof ST_FontCollectionIndex)[number];

export const ST_OnOff = ['true', 'false', '1', '0', 'on', 'off'] as const;
export type ST_OnOff = (typeof ST_OnOff)[number];

export const ST_TargetMode = ['Internal', 'External'] as const;
export type ST_TargetMode = (typeof ST_TargetMode)[number];
