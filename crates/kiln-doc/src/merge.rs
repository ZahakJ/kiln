// SPDX-License-Identifier: AGPL-3.0-or-later
//! Property-level merging used by style resolution.
//!
//! Word inherits each attribute independently (`w:ind/@w:left` from one level and `@w:firstLine`
//! from another), so nested groups merge per field, with the WordprocessingML exclusivity rules:
//!
//! - `first_line` and `hanging` are mutually exclusive: setting one clears the other;
//! - an explicit font name replaces a theme font reference in the same slot and vice versa;
//! - tab stops accumulate, and a `clear` stop removes an inherited stop at its position;
//! - borders merge per edge; colours, shading and underline replace as a whole;
//! - `style_id` is never inherited.

use crate::props::{
    CellBorders, CellMargins, Indent, Lang, NumberingRef, ParagraphBorders, ParagraphProps,
    RunFonts, RunProps, Spacing, TabStop, TableBorders, TableCellProps, TableProps,
};
use crate::st::TabAlignment;

/// Replaces `base` with `over` when `over` is set.
fn set<T: Clone>(base: &mut Option<T>, over: &Option<T>) {
    if let Some(v) = over {
        *base = Some(v.clone());
    }
}

/// Merges an indent group; `first_line` and `hanging` exclude each other.
#[must_use]
pub fn merge_indent(base: Option<Indent>, over: Option<Indent>) -> Option<Indent> {
    let Some(over) = over else { return base };
    let mut out = base.unwrap_or_default();
    set(&mut out.left, &over.left);
    set(&mut out.right, &over.right);
    if over.first_line.is_some() {
        out.first_line = over.first_line;
        out.hanging = None;
    }
    if over.hanging.is_some() {
        out.hanging = over.hanging;
        out.first_line = None;
    }
    Some(out)
}

/// Applies tab stops: a stop at the same position replaces the inherited one, `clear` removes
/// it, and the result is sorted by position.
#[must_use]
pub fn merge_tabs(base: Option<&[TabStop]>, over: Option<&[TabStop]>) -> Option<Vec<TabStop>> {
    let Some(over) = over else {
        return base.map(<[TabStop]>::to_vec);
    };
    let mut out: Vec<TabStop> = base.map(<[TabStop]>::to_vec).unwrap_or_default();
    for t in over {
        out.retain(|x| x.pos != t.pos);
        if t.align != TabAlignment::Clear {
            out.push(t.clone());
        }
    }
    out.sort_by_key(|t| t.pos);
    Some(out)
}

fn merge_spacing(base: Option<Spacing>, over: Option<Spacing>) -> Option<Spacing> {
    let Some(over) = over else { return base };
    let mut out = base.unwrap_or_default();
    set(&mut out.before, &over.before);
    set(&mut out.after, &over.after);
    set(&mut out.line, &over.line);
    set(&mut out.line_rule, &over.line_rule);
    set(&mut out.before_autospacing, &over.before_autospacing);
    set(&mut out.after_autospacing, &over.after_autospacing);
    Some(out)
}

fn merge_numbering(base: Option<NumberingRef>, over: Option<NumberingRef>) -> Option<NumberingRef> {
    let Some(over) = over else { return base };
    let mut out = base.unwrap_or_default();
    set(&mut out.num_id, &over.num_id);
    set(&mut out.level, &over.level);
    Some(out)
}

/// Merges paragraph borders per edge.
#[must_use]
pub fn merge_paragraph_borders(
    base: Option<&ParagraphBorders>,
    over: Option<&ParagraphBorders>,
) -> Option<ParagraphBorders> {
    let Some(over) = over else {
        return base.cloned();
    };
    let mut out = base.cloned().unwrap_or_default();
    set(&mut out.top, &over.top);
    set(&mut out.left, &over.left);
    set(&mut out.bottom, &over.bottom);
    set(&mut out.right, &over.right);
    set(&mut out.between, &over.between);
    set(&mut out.bar, &over.bar);
    Some(out)
}

/// Merges table borders per edge.
#[must_use]
pub fn merge_table_borders(
    base: Option<&TableBorders>,
    over: Option<&TableBorders>,
) -> Option<TableBorders> {
    let Some(over) = over else {
        return base.cloned();
    };
    let mut out = base.cloned().unwrap_or_default();
    set(&mut out.top, &over.top);
    set(&mut out.left, &over.left);
    set(&mut out.bottom, &over.bottom);
    set(&mut out.right, &over.right);
    set(&mut out.inside_h, &over.inside_h);
    set(&mut out.inside_v, &over.inside_v);
    Some(out)
}

/// Merges cell borders per edge.
#[must_use]
pub fn merge_cell_borders(
    base: Option<&CellBorders>,
    over: Option<&CellBorders>,
) -> Option<CellBorders> {
    let Some(over) = over else {
        return base.cloned();
    };
    let mut out = base.cloned().unwrap_or_default();
    set(&mut out.top, &over.top);
    set(&mut out.left, &over.left);
    set(&mut out.bottom, &over.bottom);
    set(&mut out.right, &over.right);
    set(&mut out.inside_h, &over.inside_h);
    set(&mut out.inside_v, &over.inside_v);
    set(&mut out.tl2br, &over.tl2br);
    set(&mut out.tr2bl, &over.tr2bl);
    Some(out)
}

fn merge_margins(base: Option<CellMargins>, over: Option<CellMargins>) -> Option<CellMargins> {
    let Some(over) = over else { return base };
    let mut out = base.unwrap_or_default();
    set(&mut out.top, &over.top);
    set(&mut out.left, &over.left);
    set(&mut out.bottom, &over.bottom);
    set(&mut out.right, &over.right);
    Some(out)
}

/// Merges paragraph properties; `over` wins per attribute and `style_id` is never inherited.
#[must_use]
pub fn merge_paragraph_props(
    base: &ParagraphProps,
    over: Option<&ParagraphProps>,
) -> ParagraphProps {
    let Some(o) = over else { return base.clone() };
    let mut out = base.clone();
    set(&mut out.alignment, &o.alignment);
    out.indent = merge_indent(out.indent, o.indent);
    out.spacing = merge_spacing(out.spacing, o.spacing);
    out.numbering = merge_numbering(out.numbering, o.numbering);
    set(&mut out.keep_next, &o.keep_next);
    set(&mut out.keep_lines, &o.keep_lines);
    set(&mut out.page_break_before, &o.page_break_before);
    set(&mut out.widow_control, &o.widow_control);
    set(&mut out.contextual_spacing, &o.contextual_spacing);
    set(&mut out.suppress_auto_hyphens, &o.suppress_auto_hyphens);
    set(&mut out.suppress_line_numbers, &o.suppress_line_numbers);
    set(&mut out.mirror_indents, &o.mirror_indents);
    set(&mut out.snap_to_grid, &o.snap_to_grid);
    set(&mut out.outline_level, &o.outline_level);
    out.tabs = merge_tabs(out.tabs.as_deref(), o.tabs.as_deref());
    out.borders = merge_paragraph_borders(out.borders.as_ref(), o.borders.as_ref());
    set(&mut out.shading, &o.shading);
    set(&mut out.bidi, &o.bidi);
    set(&mut out.text_alignment, &o.text_alignment);
    set(&mut out.drop_cap, &o.drop_cap);
    out
}

fn merge_font_slot(
    name: &mut Option<String>,
    theme: &mut Option<crate::st::ThemeFontRef>,
    over_name: &Option<String>,
    over_theme: Option<crate::st::ThemeFontRef>,
) {
    if over_theme.is_some() {
        *theme = over_theme;
        name.clone_from(over_name);
    } else if over_name.is_some() {
        name.clone_from(over_name);
        *theme = None;
    }
}

/// Merges font slots: a theme reference replaces an explicit name in its slot (the cached name
/// stays alongside when the overriding level gives both), an explicit name drops the reference.
#[must_use]
pub fn merge_fonts(base: Option<&RunFonts>, over: Option<&RunFonts>) -> Option<RunFonts> {
    let Some(o) = over else { return base.cloned() };
    let mut out = base.cloned().unwrap_or_default();
    merge_font_slot(
        &mut out.ascii,
        &mut out.ascii_theme,
        &o.ascii,
        o.ascii_theme,
    );
    merge_font_slot(
        &mut out.h_ansi,
        &mut out.h_ansi_theme,
        &o.h_ansi,
        o.h_ansi_theme,
    );
    merge_font_slot(
        &mut out.east_asia,
        &mut out.east_asia_theme,
        &o.east_asia,
        o.east_asia_theme,
    );
    merge_font_slot(&mut out.cs, &mut out.cs_theme, &o.cs, o.cs_theme);
    Some(out)
}

fn merge_lang(base: Option<&Lang>, over: Option<&Lang>) -> Option<Lang> {
    let Some(o) = over else { return base.cloned() };
    let mut out = base.cloned().unwrap_or_default();
    set(&mut out.val, &o.val);
    set(&mut out.east_asia, &o.east_asia);
    set(&mut out.bidi, &o.bidi);
    Some(out)
}

/// Merges run properties with plain override semantics (`over` wins). The XOR of toggle
/// properties between style types is the resolver's job, not this function's. `style_id` is never
/// inherited.
#[must_use]
pub fn merge_run_props(base: &RunProps, over: Option<&RunProps>) -> RunProps {
    let Some(o) = over else { return base.clone() };
    let mut out = base.clone();
    out.fonts = merge_fonts(out.fonts.as_ref(), o.fonts.as_ref());
    set(&mut out.bold, &o.bold);
    set(&mut out.bold_cs, &o.bold_cs);
    set(&mut out.italic, &o.italic);
    set(&mut out.italic_cs, &o.italic_cs);
    set(&mut out.underline, &o.underline);
    set(&mut out.strike, &o.strike);
    set(&mut out.dstrike, &o.dstrike);
    set(&mut out.caps, &o.caps);
    set(&mut out.small_caps, &o.small_caps);
    set(&mut out.hidden, &o.hidden);
    set(&mut out.emboss, &o.emboss);
    set(&mut out.imprint, &o.imprint);
    set(&mut out.outline, &o.outline);
    set(&mut out.shadow, &o.shadow);
    set(&mut out.color, &o.color);
    set(&mut out.highlight, &o.highlight);
    set(&mut out.shading, &o.shading);
    set(&mut out.border, &o.border);
    set(&mut out.size, &o.size);
    set(&mut out.size_cs, &o.size_cs);
    set(&mut out.vert_align, &o.vert_align);
    set(&mut out.spacing, &o.spacing);
    set(&mut out.scale, &o.scale);
    set(&mut out.kern, &o.kern);
    set(&mut out.position, &o.position);
    set(&mut out.rtl, &o.rtl);
    out.lang = merge_lang(out.lang.as_ref(), o.lang.as_ref());
    out
}

/// Merges table properties; `style_id` is never inherited.
#[must_use]
pub fn merge_table_props(base: &TableProps, over: Option<&TableProps>) -> TableProps {
    let Some(o) = over else { return base.clone() };
    let mut out = base.clone();
    set(&mut out.width, &o.width);
    set(&mut out.alignment, &o.alignment);
    set(&mut out.indent, &o.indent);
    out.borders = merge_table_borders(out.borders.as_ref(), o.borders.as_ref());
    set(&mut out.shading, &o.shading);
    out.cell_margins = merge_margins(out.cell_margins, o.cell_margins);
    set(&mut out.cell_spacing, &o.cell_spacing);
    set(&mut out.layout, &o.layout);
    set(&mut out.look, &o.look);
    set(&mut out.row_band_size, &o.row_band_size);
    set(&mut out.col_band_size, &o.col_band_size);
    set(&mut out.bidi_visual, &o.bidi_visual);
    set(&mut out.position, &o.position);
    set(&mut out.allow_overlap, &o.allow_overlap);
    set(&mut out.caption, &o.caption);
    set(&mut out.description, &o.description);
    out
}

/// Merges cell properties.
#[must_use]
pub fn merge_cell_props(base: &TableCellProps, over: Option<&TableCellProps>) -> TableCellProps {
    let Some(o) = over else { return base.clone() };
    let mut out = base.clone();
    set(&mut out.width, &o.width);
    out.borders = merge_cell_borders(out.borders.as_ref(), o.borders.as_ref());
    set(&mut out.shading, &o.shading);
    out.margins = merge_margins(out.margins, o.margins);
    set(&mut out.v_align, &o.v_align);
    set(&mut out.no_wrap, &o.no_wrap);
    set(&mut out.text_direction, &o.text_direction);
    set(&mut out.fit_text, &o.fit_text);
    set(&mut out.hide_mark, &o.hide_mark);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::{Border, DocColor};
    use crate::st::{Alignment, BorderStyle, ThemeFontRef};

    #[test]
    fn indent_first_line_and_hanging_exclude_each_other() {
        let base = Indent {
            left: Some(360),
            first_line: Some(300),
            ..Indent::default()
        };
        let out = merge_indent(
            Some(base),
            Some(Indent {
                hanging: Some(100),
                ..Indent::default()
            }),
        );
        assert_eq!(
            out,
            Some(Indent {
                left: Some(360),
                hanging: Some(100),
                ..Indent::default()
            })
        );
        assert_eq!(merge_indent(Some(base), None), Some(base));
    }

    #[test]
    fn tabs_accumulate_clear_and_sort() {
        let base = [
            TabStop::new(1440, TabAlignment::Center),
            TabStop::new(720, TabAlignment::Left),
        ];
        let over = [
            TabStop::new(1440, TabAlignment::Clear),
            TabStop::new(2000, TabAlignment::Right),
            TabStop::new(720, TabAlignment::Decimal),
        ];
        let out = merge_tabs(Some(&base), Some(&over)).unwrap_or_default();
        let pairs: Vec<_> = out.iter().map(|t| (t.pos, t.align)).collect();
        assert_eq!(
            pairs,
            vec![(720, TabAlignment::Decimal), (2000, TabAlignment::Right)]
        );
        assert_eq!(merge_tabs(None, None), None);
    }

    #[test]
    fn paragraph_merge_never_inherits_the_style_id() {
        let base = ParagraphProps {
            style_id: Some("A".into()),
            alignment: Some(Alignment::Center),
            ..ParagraphProps::default()
        };
        let over = ParagraphProps {
            style_id: Some("B".into()),
            spacing: Some(Spacing {
                after: Some(0),
                ..Spacing::default()
            }),
            borders: Some(ParagraphBorders {
                bottom: Some(Border::new(BorderStyle::Double, 6)),
                ..ParagraphBorders::default()
            }),
            ..ParagraphProps::default()
        };
        let out = merge_paragraph_props(&base, Some(&over));
        assert_eq!(out.style_id.as_deref(), Some("A"));
        assert_eq!(out.alignment, Some(Alignment::Center));
        assert_eq!(out.spacing.and_then(|s| s.after), Some(0));
        assert!(out.borders.and_then(|b| b.bottom).is_some());
    }

    #[test]
    fn fonts_swap_between_explicit_and_theme() {
        let base = RunFonts {
            ascii_theme: Some(ThemeFontRef::MinorHAnsi),
            h_ansi_theme: Some(ThemeFontRef::MinorHAnsi),
            ..RunFonts::default()
        };
        let out = merge_fonts(Some(&base), Some(&RunFonts::latin("Arial"))).unwrap_or_default();
        assert_eq!(out.ascii.as_deref(), Some("Arial"));
        assert_eq!(out.ascii_theme, None);
        let back = merge_fonts(
            Some(&out),
            Some(&RunFonts {
                ascii_theme: Some(ThemeFontRef::MajorHAnsi),
                ..RunFonts::default()
            }),
        )
        .unwrap_or_default();
        assert_eq!(back.ascii, None);
        assert_eq!(back.ascii_theme, Some(ThemeFontRef::MajorHAnsi));
        assert_eq!(back.h_ansi.as_deref(), Some("Arial"));
    }

    #[test]
    fn run_merge_overrides_and_merges_lang() {
        let base = RunProps {
            style_id: Some("Strong".into()),
            size: Some(22),
            lang: Some(Lang {
                val: Some("en-US".into()),
                bidi: Some("ar-SA".into()),
                ..Lang::default()
            }),
            ..RunProps::default()
        };
        let over = RunProps {
            style_id: Some("Emphasis".into()),
            size: Some(28),
            color: Some(DocColor::rgb(0xFF0000)),
            lang: Some(Lang {
                val: Some("de-DE".into()),
                ..Lang::default()
            }),
            ..RunProps::default()
        };
        let out = merge_run_props(&base, Some(&over));
        assert_eq!(out.style_id.as_deref(), Some("Strong"));
        assert_eq!(out.size, Some(28));
        let lang = out.lang.unwrap_or_default();
        assert_eq!(lang.val.as_deref(), Some("de-DE"));
        assert_eq!(lang.bidi.as_deref(), Some("ar-SA"));
    }

    #[test]
    fn table_and_cell_merge_per_edge_and_margin() {
        let base = TableProps {
            cell_margins: Some(CellMargins {
                left: Some(108),
                right: Some(108),
                ..CellMargins::default()
            }),
            borders: Some(TableBorders {
                top: Some(Border::new(BorderStyle::Single, 4)),
                ..TableBorders::default()
            }),
            ..TableProps::default()
        };
        let over = TableProps {
            style_id: Some("X".into()),
            cell_margins: Some(CellMargins {
                top: Some(10),
                ..CellMargins::default()
            }),
            borders: Some(TableBorders {
                inside_h: Some(Border::new(BorderStyle::Dotted, 2)),
                ..TableBorders::default()
            }),
            ..TableProps::default()
        };
        let out = merge_table_props(&base, Some(&over));
        assert_eq!(out.style_id, None);
        let m = out.cell_margins.unwrap_or_default();
        assert_eq!((m.top, m.left), (Some(10), Some(108)));
        let b = out.borders.unwrap_or_default();
        assert!(b.top.is_some() && b.inside_h.is_some());

        let cell = merge_cell_props(
            &TableCellProps {
                borders: Some(CellBorders {
                    tl2br: Some(Border::new(BorderStyle::Single, 4)),
                    ..CellBorders::default()
                }),
                ..TableCellProps::default()
            },
            Some(&TableCellProps {
                borders: Some(CellBorders {
                    left: Some(Border::new(BorderStyle::Double, 4)),
                    ..CellBorders::default()
                }),
                margins: Some(CellMargins {
                    left: Some(0),
                    ..CellMargins::default()
                }),
                ..TableCellProps::default()
            }),
        );
        let cb = cell.borders.unwrap_or_default();
        assert!(cb.tl2br.is_some() && cb.left.is_some());
        assert_eq!(cell.margins.and_then(|m| m.left), Some(0));
    }
}
