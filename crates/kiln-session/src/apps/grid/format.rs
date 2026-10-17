// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cell formats: the number format combo and its buttons ($, %, comma, decimals), fonts (the
//! shared Bold/Italic/… commands toggle by the active cell, as Excel), fill, borders (every
//! preset of the Borders menu), alignment, orientation, wrap, merges, and Format Cells. Each
//! command is one undo step over the selected ranges (whole rows and columns set the row and
//! column formats, so new cells there take them).

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::{
    EditCommand, GridBorderSide, GridBorders, GridCommand, GridFormat, GridHAlign, GridLineStyle,
    GridVAlign, PasteMode,
};
use kiln_sheet::color::Color;
use kiln_sheet::commands::MergeMode;
use kiln_sheet::num::F64;
use kiln_sheet::styles::{
    AlignmentPatch, BorderPatch, BorderSide, BorderStyle, CellStyle, Fill, FontPatch, HAlign,
    Patch, PatternType, ProtectionPatch, StylePatch, Underline, VAlign, VertAlign,
};
use kiln_sheet::{Range, SheetId};

use super::GridDoc;
use crate::model::Change;

/// Excel's font size list (Grow/Shrink Font step through it).
const SIZES: [f64; 16] = [
    8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0,
];

/// The number format combo's presets (en-US, as Excel's).
pub const PRESETS: [(&str, &str); 11] = [
    ("General", "General"),
    ("Number", "0.00"),
    ("Currency", "\"$\"#,##0.00"),
    (
        "Accounting",
        "_(\"$\"* #,##0.00_);_(\"$\"* \\(#,##0.00\\);_(\"$\"* \"-\"??_);_(@_)",
    ),
    ("Short Date", "m/d/yyyy"),
    ("Long Date", "dddd, mmmm d, yyyy"),
    ("Time", "h:mm:ss AM/PM"),
    ("Percentage", "0.00%"),
    ("Fraction", "# ?/?"),
    ("Scientific", "0.00E+00"),
    ("Text", "@"),
];

/// The combo entry for a format code (`Custom` for anything else).
#[must_use]
pub fn preset_name(code: &str) -> &'static str {
    let c = code.trim();
    if c.is_empty() || c.eq_ignore_ascii_case("general") {
        return "General";
    }
    for (name, preset) in PRESETS {
        if preset == c {
            return name;
        }
    }
    let f = kiln_formula::numfmt::NumberFormat::parse(c);
    if c == "@" {
        "Text"
    } else if f.is_date() {
        if c.contains('h') || c.contains('s') {
            if c.contains('d') || c.contains('y') {
                "Custom"
            } else {
                "Time"
            }
        } else {
            "Short Date"
        }
    } else if f.is_percent() {
        "Percentage"
    } else if c.contains("E+") || c.contains("E-") {
        "Scientific"
    } else if c.contains('?') && c.contains('/') {
        "Fraction"
    } else if c.contains('*') {
        "Accounting"
    } else if c.contains('$') || c.contains('£') || c.contains('€') {
        "Currency"
    } else if c.chars().all(|ch| "0#,.".contains(ch)) {
        "Number"
    } else {
        "Custom"
    }
}

/// The Format Cells category of a code.
#[must_use]
pub fn category(code: &str) -> &'static str {
    match preset_name(code) {
        "Short Date" | "Long Date" => "Date",
        "Time" => "Time",
        "Custom" => {
            let f = kiln_formula::numfmt::NumberFormat::parse(code);
            if f.is_date() { "Date" } else { "Custom" }
        }
        other => other,
    }
}

fn accounting(currency: Option<&str>) -> String {
    let sym = match currency.map(str::to_ascii_uppercase).as_deref() {
        Some("GBP") => "£",
        Some("EUR") => "€",
        Some("JPY" | "CNY") => "¥",
        Some("CHF") => "CHF",
        _ => "$",
    };
    format!("_(\"{sym}\"* #,##0.00_);_(\"{sym}\"* \\(#,##0.00\\);_(\"{sym}\"* \"-\"??_);_(@_)")
}

/// Adds (`delta = 1`) or removes a decimal place in every numeric section of a code, as Excel's
/// Increase/Decrease Decimal buttons do. General starts from the decimals the value shows.
#[must_use]
pub fn change_decimals(code: &str, delta: i32, shown_decimals: usize) -> String {
    if code.trim().is_empty() || code.trim().eq_ignore_ascii_case("general") {
        let n = (shown_decimals as i32 + delta).max(0) as usize;
        return if n == 0 {
            "0".into()
        } else {
            format!("0.{}", "0".repeat(n))
        };
    }
    code.split(';')
        .map(|section| shift_section(section, delta))
        .collect::<Vec<_>>()
        .join(";")
}

fn shift_section(section: &str, delta: i32) -> String {
    // The last digit placeholder outside quotes, brackets and escapes.
    let chars: Vec<char> = section.chars().collect();
    let mut quoted = false;
    let mut bracket = false;
    let mut last_digit: Option<usize> = None;
    let mut dot: Option<usize> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            '\\' | '_' | '*' if !quoted && !bracket => {
                i += 2;
                continue;
            }
            '0' | '#' | '?' if !quoted && !bracket => last_digit = Some(i),
            '.' if !quoted && !bracket => dot = Some(i),
            _ => {}
        }
        i += 1;
    }
    let Some(last) = last_digit else {
        return section.to_owned();
    };
    let mut out = chars.clone();
    if delta > 0 {
        if dot.is_some_and(|d| d < last) {
            out.insert(last + 1, '0');
        } else {
            out.insert(last + 1, '0');
            out.insert(last + 1, '.');
        }
    } else if let Some(d) = dot.filter(|&d| d < last) {
        out.remove(last);
        // No decimals left: drop the point.
        if last == d + 1 {
            out.remove(d);
        }
    }
    out.into_iter().collect()
}

/// `RRGGBB` → a colour (`None` for empty or invalid).
pub(crate) fn parse_color(s: &str) -> Option<Color> {
    let hex = s.trim().trim_start_matches('#');
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
        .map(Color::rgb)
}

fn color_hex(c: Option<Color>, theme: &kiln_sheet::Theme, auto: u32) -> Option<String> {
    c.map(|c| format!("{:06X}", c.resolve(theme, auto)))
}

fn line_style(s: GridLineStyle) -> BorderStyle {
    match s {
        GridLineStyle::None => BorderStyle::None,
        GridLineStyle::Thin => BorderStyle::Thin,
        GridLineStyle::Medium => BorderStyle::Medium,
        GridLineStyle::Thick => BorderStyle::Thick,
        GridLineStyle::Double => BorderStyle::Double,
        GridLineStyle::Dotted => BorderStyle::Dotted,
        GridLineStyle::Dashed => BorderStyle::Dashed,
        GridLineStyle::Hair => BorderStyle::Hair,
        GridLineStyle::MediumDashed => BorderStyle::MediumDashed,
        GridLineStyle::DashDot => BorderStyle::DashDot,
        GridLineStyle::MediumDashDot => BorderStyle::MediumDashDot,
        GridLineStyle::DashDotDot => BorderStyle::DashDotDot,
        GridLineStyle::MediumDashDotDot => BorderStyle::MediumDashDotDot,
        GridLineStyle::SlantDashDot => BorderStyle::SlantDashDot,
    }
}

/// The protocol's line style of a model style.
#[must_use]
pub fn grid_line_style(s: BorderStyle) -> GridLineStyle {
    match s {
        BorderStyle::None => GridLineStyle::None,
        BorderStyle::Thin => GridLineStyle::Thin,
        BorderStyle::Medium => GridLineStyle::Medium,
        BorderStyle::Thick => GridLineStyle::Thick,
        BorderStyle::Double => GridLineStyle::Double,
        BorderStyle::Dotted => GridLineStyle::Dotted,
        BorderStyle::Dashed => GridLineStyle::Dashed,
        BorderStyle::Hair => GridLineStyle::Hair,
        BorderStyle::MediumDashed => GridLineStyle::MediumDashed,
        BorderStyle::DashDot => GridLineStyle::DashDot,
        BorderStyle::MediumDashDot => GridLineStyle::MediumDashDot,
        BorderStyle::DashDotDot => GridLineStyle::DashDotDot,
        BorderStyle::MediumDashDotDot => GridLineStyle::MediumDashDotDot,
        BorderStyle::SlantDashDot => GridLineStyle::SlantDashDot,
    }
}

/// The protocol's side of a model side.
pub(crate) fn grid_side(
    s: Option<BorderSide>,
    theme: &kiln_sheet::Theme,
) -> Option<GridBorderSide> {
    s.filter(|s| s.style != BorderStyle::None)
        .map(|s| GridBorderSide {
            style: grid_line_style(s.style),
            color: color_hex(s.color, theme, 0),
        })
}

fn side_patch(s: &GridBorderSide) -> Patch<BorderSide> {
    if s.style == GridLineStyle::None {
        return Patch::Clear;
    }
    Patch::Set(BorderSide {
        style: line_style(s.style),
        color: s.color.as_deref().and_then(parse_color),
    })
}

pub(crate) fn h_align(a: GridHAlign) -> HAlign {
    match a {
        GridHAlign::General => HAlign::General,
        GridHAlign::Left => HAlign::Left,
        GridHAlign::Center => HAlign::Center,
        GridHAlign::Right => HAlign::Right,
        GridHAlign::Fill => HAlign::Fill,
        GridHAlign::Justify => HAlign::Justify,
        GridHAlign::CenterAcross => HAlign::CenterContinuous,
        GridHAlign::Distributed => HAlign::Distributed,
    }
}

pub(crate) fn grid_h_align(a: HAlign) -> GridHAlign {
    match a {
        HAlign::General => GridHAlign::General,
        HAlign::Left => GridHAlign::Left,
        HAlign::Center => GridHAlign::Center,
        HAlign::Right => GridHAlign::Right,
        HAlign::Fill => GridHAlign::Fill,
        HAlign::Justify => GridHAlign::Justify,
        HAlign::CenterContinuous => GridHAlign::CenterAcross,
        HAlign::Distributed => GridHAlign::Distributed,
    }
}

pub(crate) fn v_align(a: GridVAlign) -> VAlign {
    match a {
        GridVAlign::Top => VAlign::Top,
        GridVAlign::Center => VAlign::Center,
        GridVAlign::Bottom => VAlign::Bottom,
        GridVAlign::Justify => VAlign::Justify,
        GridVAlign::Distributed => VAlign::Distributed,
    }
}

pub(crate) fn grid_v_align(a: VAlign) -> GridVAlign {
    match a {
        VAlign::Top => GridVAlign::Top,
        VAlign::Center => GridVAlign::Center,
        VAlign::Bottom => GridVAlign::Bottom,
        VAlign::Justify => GridVAlign::Justify,
        VAlign::Distributed => GridVAlign::Distributed,
    }
}

fn pattern_of(name: &str) -> Option<PatternType> {
    use PatternType::*;
    Some(match name {
        "none" => None,
        "solid" => Solid,
        "mediumGray" => MediumGray,
        "darkGray" => DarkGray,
        "lightGray" => LightGray,
        "darkHorizontal" => DarkHorizontal,
        "darkVertical" => DarkVertical,
        "darkDown" => DarkDown,
        "darkUp" => DarkUp,
        "darkGrid" => DarkGrid,
        "darkTrellis" => DarkTrellis,
        "lightHorizontal" => LightHorizontal,
        "lightVertical" => LightVertical,
        "lightDown" => LightDown,
        "lightUp" => LightUp,
        "lightGrid" => LightGrid,
        "lightTrellis" => LightTrellis,
        "gray125" => Gray125,
        "gray0625" => Gray0625,
        _ => return Option::None,
    })
}

/// The name of a pattern (the dialog's list).
#[must_use]
pub fn pattern_name(p: PatternType) -> &'static str {
    use PatternType::*;
    match p {
        None => "none",
        Solid => "solid",
        MediumGray => "mediumGray",
        DarkGray => "darkGray",
        LightGray => "lightGray",
        DarkHorizontal => "darkHorizontal",
        DarkVertical => "darkVertical",
        DarkDown => "darkDown",
        DarkUp => "darkUp",
        DarkGrid => "darkGrid",
        DarkTrellis => "darkTrellis",
        LightHorizontal => "lightHorizontal",
        LightVertical => "lightVertical",
        LightDown => "lightDown",
        LightUp => "lightUp",
        LightGrid => "lightGrid",
        LightTrellis => "lightTrellis",
        Gray125 => "gray125",
        Gray0625 => "gray0625",
    }
}

fn underline_of(name: &str) -> Option<Underline> {
    Some(match name {
        "none" => Underline::None,
        "single" => Underline::Single,
        "double" => Underline::Double,
        "singleAccounting" => Underline::SingleAccounting,
        "doubleAccounting" => Underline::DoubleAccounting,
        _ => return None,
    })
}

/// The name of an underline style.
#[must_use]
pub fn underline_name(u: Underline) -> &'static str {
    match u {
        Underline::None => "none",
        Underline::Single => "single",
        Underline::Double => "double",
        Underline::SingleAccounting => "singleAccounting",
        Underline::DoubleAccounting => "doubleAccounting",
    }
}

impl GridDoc {
    /// The format of the active cell.
    pub(crate) fn active_style(&self) -> CellStyle {
        let Ok(sid) = self.sid() else {
            return CellStyle::default();
        };
        let a = self.view().sel.active;
        let idx = self
            .workbook
            .sheet(sid)
            .map_or(0, |s| s.style_at(a.row, a.col));
        self.workbook.styles().get(idx)
    }

    /// Applies a patch to the selection.
    pub(crate) fn patch_selection(&mut self, label: &'static str, patch: StylePatch) -> Result<()> {
        let sid = self.sid()?;
        let ranges = self.view().sel.ranges;
        self.exec(
            label,
            kiln_sheet::Command::SetStyle {
                sheet: sid,
                ranges,
                patch: Box::new(patch),
            },
        )?;
        Ok(())
    }

    fn font(&mut self, label: &'static str, f: FontPatch) -> Result<()> {
        let size_changed = f.size.is_some();
        self.patch_selection(
            label,
            StylePatch {
                font: Some(f),
                ..StylePatch::default()
            },
        )?;
        if size_changed {
            self.autofit_selection_rows();
        }
        Ok(())
    }

    fn align(&mut self, label: &'static str, a: AlignmentPatch) -> Result<()> {
        let wraps = a.wrap_text.is_some() || a.text_rotation.is_some();
        self.patch_selection(
            label,
            StylePatch {
                alignment: Some(a),
                ..StylePatch::default()
            },
        )?;
        if wraps {
            self.autofit_selection_rows();
        }
        Ok(())
    }

    fn num_fmt(&mut self, label: &'static str, code: &str) -> Result<()> {
        if code.len() > 255 {
            return Err(KilnError::InvalidInput(
                "a number format has at most 255 characters".into(),
            ));
        }
        self.patch_selection(
            label,
            StylePatch {
                num_fmt: Some(code.to_owned()),
                ..StylePatch::default()
            },
        )
    }

    /// Border patches for a preset over each selected range.
    fn borders(
        &mut self,
        preset: GridBorders,
        style: BorderStyle,
        color: Option<Color>,
    ) -> Result<()> {
        let sid = self.sid()?;
        let side = BorderSide { style, color };
        let set = Patch::Set(side);
        let thick = Patch::Set(BorderSide {
            style: BorderStyle::Thick,
            color,
        });
        let mut cmds = Vec::new();
        let mut patch = |ranges: Vec<Range>, b: BorderPatch| {
            if !ranges.is_empty() {
                cmds.push(kiln_sheet::Command::SetStyle {
                    sheet: sid,
                    ranges,
                    patch: Box::new(StylePatch {
                        border: Some(b),
                        ..StylePatch::default()
                    }),
                });
            }
        };
        for r in self.view().sel.ranges {
            let top_row = Range::new(r.r0, r.c0, r.r0, r.c1);
            let bottom_row = Range::new(r.r1, r.c0, r.r1, r.c1);
            let left_col = Range::new(r.r0, r.c0, r.r1, r.c0);
            let right_col = Range::new(r.r0, r.c1, r.r1, r.c1);
            let outline = |p: &mut dyn FnMut(Vec<Range>, BorderPatch), s: &Patch<BorderSide>| {
                p(
                    vec![top_row],
                    BorderPatch {
                        top: s.clone(),
                        ..BorderPatch::default()
                    },
                );
                p(
                    vec![bottom_row],
                    BorderPatch {
                        bottom: s.clone(),
                        ..BorderPatch::default()
                    },
                );
                p(
                    vec![left_col],
                    BorderPatch {
                        left: s.clone(),
                        ..BorderPatch::default()
                    },
                );
                p(
                    vec![right_col],
                    BorderPatch {
                        right: s.clone(),
                        ..BorderPatch::default()
                    },
                );
            };
            let inside = |p: &mut dyn FnMut(Vec<Range>, BorderPatch), s: &Patch<BorderSide>| {
                if r.r1 > r.r0 {
                    p(
                        vec![Range::new(r.r0, r.c0, r.r1 - 1, r.c1)],
                        BorderPatch {
                            bottom: s.clone(),
                            ..BorderPatch::default()
                        },
                    );
                    p(
                        vec![Range::new(r.r0 + 1, r.c0, r.r1, r.c1)],
                        BorderPatch {
                            top: s.clone(),
                            ..BorderPatch::default()
                        },
                    );
                }
                if r.c1 > r.c0 {
                    p(
                        vec![Range::new(r.r0, r.c0, r.r1, r.c1 - 1)],
                        BorderPatch {
                            right: s.clone(),
                            ..BorderPatch::default()
                        },
                    );
                    p(
                        vec![Range::new(r.r0, r.c0 + 1, r.r1, r.c1)],
                        BorderPatch {
                            left: s.clone(),
                            ..BorderPatch::default()
                        },
                    );
                }
            };
            match preset {
                GridBorders::All => {
                    outline(&mut patch, &set);
                    inside(&mut patch, &set);
                }
                GridBorders::Outside => outline(&mut patch, &set),
                GridBorders::ThickBox => outline(&mut patch, &thick),
                GridBorders::Inside => inside(&mut patch, &set),
                GridBorders::Top => patch(
                    vec![top_row],
                    BorderPatch {
                        top: set.clone(),
                        ..BorderPatch::default()
                    },
                ),
                GridBorders::Bottom => patch(
                    vec![bottom_row],
                    BorderPatch {
                        bottom: set.clone(),
                        ..BorderPatch::default()
                    },
                ),
                GridBorders::Left => patch(
                    vec![left_col],
                    BorderPatch {
                        left: set.clone(),
                        ..BorderPatch::default()
                    },
                ),
                GridBorders::Right => patch(
                    vec![right_col],
                    BorderPatch {
                        right: set.clone(),
                        ..BorderPatch::default()
                    },
                ),
                GridBorders::BottomDouble => patch(
                    vec![bottom_row],
                    BorderPatch {
                        bottom: Patch::Set(BorderSide {
                            style: BorderStyle::Double,
                            color,
                        }),
                        ..BorderPatch::default()
                    },
                ),
                GridBorders::BottomThick => patch(
                    vec![bottom_row],
                    BorderPatch {
                        bottom: thick.clone(),
                        ..BorderPatch::default()
                    },
                ),
                GridBorders::TopBottom => {
                    patch(
                        vec![top_row],
                        BorderPatch {
                            top: set.clone(),
                            ..BorderPatch::default()
                        },
                    );
                    patch(
                        vec![bottom_row],
                        BorderPatch {
                            bottom: set.clone(),
                            ..BorderPatch::default()
                        },
                    );
                }
                GridBorders::None => {
                    patch(
                        vec![r],
                        BorderPatch {
                            left: Patch::Clear,
                            right: Patch::Clear,
                            top: Patch::Clear,
                            bottom: Patch::Clear,
                            diagonal: Patch::Clear,
                            diagonal_up: Some(false),
                            diagonal_down: Some(false),
                        },
                    );
                    // The neighbours' sides on the outline show on these cells too.
                    if r.r0 > 0 {
                        patch(
                            vec![Range::new(r.r0 - 1, r.c0, r.r0 - 1, r.c1)],
                            BorderPatch {
                                bottom: Patch::Clear,
                                ..BorderPatch::default()
                            },
                        );
                    }
                    if r.r1 + 1 < kiln_sheet::MAX_ROWS {
                        patch(
                            vec![Range::new(r.r1 + 1, r.c0, r.r1 + 1, r.c1)],
                            BorderPatch {
                                top: Patch::Clear,
                                ..BorderPatch::default()
                            },
                        );
                    }
                    if r.c0 > 0 {
                        patch(
                            vec![Range::new(r.r0, r.c0 - 1, r.r1, r.c0 - 1)],
                            BorderPatch {
                                right: Patch::Clear,
                                ..BorderPatch::default()
                            },
                        );
                    }
                    if r.c1 + 1 < kiln_sheet::MAX_COLS {
                        patch(
                            vec![Range::new(r.r0, r.c1 + 1, r.r1, r.c1 + 1)],
                            BorderPatch {
                                left: Patch::Clear,
                                ..BorderPatch::default()
                            },
                        );
                    }
                }
            }
        }
        self.exec_all("grid.font.borders", cmds)?;
        Ok(())
    }

    fn merge(&mut self, label: &'static str, mode: Option<MergeMode>) -> Result<()> {
        let sid = self.sid()?;
        let ranges = self.view().sel.ranges;
        let mut cmds = Vec::new();
        for r in &ranges {
            match mode {
                Some(m) => {
                    if r.is_single_cell() {
                        continue;
                    }
                    cmds.push(kiln_sheet::Command::Merge {
                        sheet: sid,
                        range: *r,
                        mode: m,
                    });
                }
                None => cmds.push(kiln_sheet::Command::Unmerge {
                    sheet: sid,
                    range: *r,
                }),
            }
        }
        self.exec_all(label, cmds)?;
        Ok(())
    }

    /// Applies Format Cells ▸ OK.
    fn apply_format(&mut self, f: &GridFormat) -> Result<()> {
        let sid = self.sid()?;
        let mut patch = StylePatch::default();
        if let Some(c) = &f.number_format {
            patch.num_fmt = Some(c.clone());
        }
        let mut font = FontPatch::default();
        let mut any_font = false;
        if let Some(n) = &f.font_family {
            font.name = Some(n.as_str().into());
            any_font = true;
        }
        if let Some(s) = f.font_size {
            if !(1.0..=409.0).contains(&s) {
                return Err(KilnError::InvalidInput(
                    "a font size is between 1 and 409 points".into(),
                ));
            }
            font.size = Some(F64(s));
            any_font = true;
        }
        if let Some(b) = f.bold {
            font.bold = Some(b);
            any_font = true;
        }
        if let Some(i) = f.italic {
            font.italic = Some(i);
            any_font = true;
        }
        if let Some(u) = &f.underline {
            font.underline = Some(
                underline_of(u)
                    .ok_or_else(|| KilnError::InvalidInput(format!("unknown underline “{u}”")))?,
            );
            any_font = true;
        }
        if let Some(s) = f.strike {
            font.strike = Some(s);
            any_font = true;
        }
        match (f.superscript, f.subscript) {
            (Some(true), _) => {
                font.vert_align = Some(VertAlign::Superscript);
                any_font = true;
            }
            (_, Some(true)) => {
                font.vert_align = Some(VertAlign::Subscript);
                any_font = true;
            }
            (Some(false), _) | (_, Some(false)) => {
                font.vert_align = Some(VertAlign::Baseline);
                any_font = true;
            }
            _ => {}
        }
        if let Some(c) = &f.font_color {
            font.color = Some(if c.is_empty() {
                Color::Auto
            } else {
                parse_color(c)
                    .ok_or_else(|| KilnError::InvalidInput(format!("“{c}” is not a colour")))?
            });
            any_font = true;
        }
        if any_font {
            patch.font = Some(font);
        }
        if let Some(p) = &f.pattern {
            let pattern = pattern_of(p)
                .ok_or_else(|| KilnError::InvalidInput(format!("unknown pattern “{p}”")))?;
            let bg = f.fill_color.as_deref().and_then(parse_color);
            let fg = f.pattern_color.as_deref().and_then(parse_color);
            patch.fill = if pattern == PatternType::None {
                Patch::Clear
            } else if pattern == PatternType::Solid {
                Patch::Set(Fill::solid(fg.or(bg).unwrap_or(Color::rgb(0xFF_FFFF))))
            } else {
                Patch::Set(Fill::Pattern { pattern, fg, bg })
            };
        } else if let Some(c) = &f.fill_color {
            patch.fill = if c.is_empty() {
                Patch::Clear
            } else {
                Patch::Set(Fill::solid(parse_color(c).ok_or_else(|| {
                    KilnError::InvalidInput(format!("“{c}” is not a colour"))
                })?))
            };
        }
        let mut al = AlignmentPatch::default();
        let mut any_al = false;
        if let Some(h) = f.h_align {
            al.horizontal = Some(h_align(h));
            any_al = true;
        }
        if let Some(v) = f.v_align {
            al.vertical = Some(v_align(v));
            any_al = true;
        }
        if let Some(w) = f.wrap {
            al.wrap_text = Some(w);
            any_al = true;
        }
        if let Some(s) = f.shrink {
            al.shrink_to_fit = Some(s);
            any_al = true;
        }
        if let Some(i) = f.indent {
            al.indent = Some(i.min(250));
            any_al = true;
        }
        if let Some(r) = f.rotation {
            if r > 180 && r != 255 {
                return Err(KilnError::InvalidInput(
                    "orientation is 0–180 or 255 (vertical)".into(),
                ));
            }
            al.text_rotation = Some(r);
            any_al = true;
        }
        if any_al {
            patch.alignment = Some(al);
        }
        if f.locked.is_some() || f.hidden.is_some() {
            patch.protection = Some(ProtectionPatch {
                locked: f.locked,
                hidden: f.hidden,
            });
        }
        let ranges = self.view().sel.ranges;
        let mut cmds = vec![kiln_sheet::Command::SetStyle {
            sheet: sid,
            ranges: ranges.clone(),
            patch: Box::new(patch),
        }];
        // Borders: the outline sides on the edges, the inside lines between cells, diagonals on
        // every cell.
        let mut border_cmd = |ranges: Vec<Range>, b: BorderPatch| {
            cmds.push(kiln_sheet::Command::SetStyle {
                sheet: sid,
                ranges,
                patch: Box::new(StylePatch {
                    border: Some(b),
                    ..StylePatch::default()
                }),
            });
        };
        for r in &ranges {
            if let Some(s) = &f.border_top {
                border_cmd(
                    vec![Range::new(r.r0, r.c0, r.r0, r.c1)],
                    BorderPatch {
                        top: side_patch(s),
                        ..BorderPatch::default()
                    },
                );
            }
            if let Some(s) = &f.border_bottom {
                border_cmd(
                    vec![Range::new(r.r1, r.c0, r.r1, r.c1)],
                    BorderPatch {
                        bottom: side_patch(s),
                        ..BorderPatch::default()
                    },
                );
            }
            if let Some(s) = &f.border_left {
                border_cmd(
                    vec![Range::new(r.r0, r.c0, r.r1, r.c0)],
                    BorderPatch {
                        left: side_patch(s),
                        ..BorderPatch::default()
                    },
                );
            }
            if let Some(s) = &f.border_right {
                border_cmd(
                    vec![Range::new(r.r0, r.c1, r.r1, r.c1)],
                    BorderPatch {
                        right: side_patch(s),
                        ..BorderPatch::default()
                    },
                );
            }
            if let Some(s) = &f.border_inside_h
                && r.r1 > r.r0
            {
                border_cmd(
                    vec![Range::new(r.r0, r.c0, r.r1 - 1, r.c1)],
                    BorderPatch {
                        bottom: side_patch(s),
                        ..BorderPatch::default()
                    },
                );
            }
            if let Some(s) = &f.border_inside_v
                && r.c1 > r.c0
            {
                border_cmd(
                    vec![Range::new(r.r0, r.c0, r.r1, r.c1 - 1)],
                    BorderPatch {
                        right: side_patch(s),
                        ..BorderPatch::default()
                    },
                );
            }
            let up = f.border_diagonal_up.as_ref();
            let down = f.border_diagonal_down.as_ref();
            if up.is_some() || down.is_some() {
                let line = up.or(down).filter(|s| s.style != GridLineStyle::None);
                border_cmd(
                    vec![*r],
                    BorderPatch {
                        diagonal: line.map_or(Patch::Clear, side_patch),
                        diagonal_up: up.map(|s| s.style != GridLineStyle::None),
                        diagonal_down: down.map(|s| s.style != GridLineStyle::None),
                        ..BorderPatch::default()
                    },
                );
            }
        }
        for r in &ranges {
            match f.merge {
                Some(true) if !r.is_single_cell() => cmds.push(kiln_sheet::Command::Merge {
                    sheet: sid,
                    range: *r,
                    mode: MergeMode::Merge,
                }),
                Some(false) => cmds.push(kiln_sheet::Command::Unmerge {
                    sheet: sid,
                    range: *r,
                }),
                _ => {}
            }
        }
        self.exec_all("grid.format.apply", cmds)?;
        if f.font_size.is_some() || f.wrap.is_some() || f.rotation.is_some() {
            self.autofit_selection_rows();
        }
        Ok(())
    }

    /// Decimals the active cell shows (for Increase/Decrease Decimal on General).
    fn shown_decimals(&self) -> usize {
        let Ok(sid) = self.sid() else {
            return 0;
        };
        let a = self.view().sel.active;
        let t = self.workbook.display_text(sid, a.row, a.col);
        t.split_once('.').map_or(0, |(_, d)| {
            d.chars().take_while(char::is_ascii_digit).count()
        })
    }

    /// The formatting commands.
    pub(crate) fn format(&mut self, cmd: &GridCommand) -> Result<Change> {
        use GridCommand as G;
        match cmd {
            G::Borders {
                preset,
                style,
                color,
            } => {
                let style = style.map_or(BorderStyle::Thin, line_style);
                let color = color.as_deref().and_then(parse_color);
                self.borders(*preset, style, color)?;
            }
            G::BorderOutline => self.borders(GridBorders::Outside, BorderStyle::Thin, None)?,
            G::BorderNone => self.borders(GridBorders::None, BorderStyle::None, None)?,
            G::FillColor { color } => {
                let fill = match color.as_deref().filter(|c| !c.is_empty()) {
                    Some(c) => Patch::Set(Fill::solid(parse_color(c).ok_or_else(|| {
                        KilnError::InvalidInput(format!("“{c}” is not a colour"))
                    })?)),
                    None => Patch::Clear,
                };
                self.patch_selection(
                    "grid.font.fill_color",
                    StylePatch {
                        fill,
                        ..StylePatch::default()
                    },
                )?;
            }
            G::AlignTop | G::AlignMiddle | G::AlignBottom => {
                let target = match cmd {
                    G::AlignTop => VAlign::Top,
                    G::AlignMiddle => VAlign::Center,
                    _ => VAlign::Bottom,
                };
                // Pressing the pressed button again returns to the default (bottom).
                let current = self.active_style().alignment.vertical;
                let v = if current == target && target != VAlign::Bottom {
                    VAlign::Bottom
                } else {
                    target
                };
                self.align(
                    cmd.meta().id,
                    AlignmentPatch {
                        vertical: Some(v),
                        ..AlignmentPatch::default()
                    },
                )?;
            }
            G::AlignHorizontal { align } => self.align(
                "grid.alignment.horizontal",
                AlignmentPatch {
                    horizontal: Some(h_align(*align)),
                    ..AlignmentPatch::default()
                },
            )?,
            G::Orientation { angle } => {
                let rot: u8 = match *angle {
                    255 => 255,
                    a if (0..=90).contains(&a) => a as u8,
                    a if (-90..0).contains(&a) => (90 - a) as u8,
                    _ => {
                        return Err(KilnError::InvalidInput(
                            "orientation is -90–90 degrees or 255 (vertical)".into(),
                        ));
                    }
                };
                // The same angle again sets it back to horizontal (Excel's toggle).
                let rot = if self.active_style().alignment.text_rotation == rot {
                    0
                } else {
                    rot
                };
                self.align(
                    "grid.alignment.orientation",
                    AlignmentPatch {
                        text_rotation: Some(rot),
                        ..AlignmentPatch::default()
                    },
                )?;
            }
            G::WrapText => {
                let on = !self.active_style().alignment.wrap_text;
                self.align(
                    "grid.alignment.wrap_text",
                    AlignmentPatch {
                        wrap_text: Some(on),
                        ..AlignmentPatch::default()
                    },
                )?;
            }
            G::MergeCenter => {
                let a = self.view().sel.active;
                let merged = self.geometry().merge_at(a.row, a.col).is_some();
                if merged {
                    self.merge("grid.alignment.merge_center", None)?;
                } else {
                    self.merge("grid.alignment.merge_center", Some(MergeMode::Center))?;
                }
            }
            G::MergeAcross => self.merge("grid.alignment.merge_across", Some(MergeMode::Across))?,
            G::MergeCells => self.merge("grid.alignment.merge_cells", Some(MergeMode::Merge))?,
            G::UnmergeCells => self.merge("grid.alignment.unmerge_cells", None)?,
            G::NumberFormat { format } => {
                let code = PRESETS
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(format.trim()))
                    .map_or(format.trim(), |(_, c)| *c)
                    .to_owned();
                if code.is_empty() {
                    return Err(KilnError::InvalidInput("a number format is needed".into()));
                }
                self.num_fmt("grid.number.number_format", &code)?;
            }
            G::NumberStyle => self.num_fmt("grid.number.number", "#,##0.00")?,
            G::CurrencyStyle => self.num_fmt(
                "grid.number.currency",
                "\"$\"#,##0.00_);\\(\"$\"#,##0.00\\)",
            )?,
            G::PercentStyle => self.num_fmt("grid.number.percent", "0%")?,
            G::AccountingStyle { currency } => {
                self.num_fmt("grid.number.accounting", &accounting(currency.as_deref()))?
            }
            G::CommaStyle => self.num_fmt(
                "grid.number.comma",
                "_(* #,##0.00_);_(* \\(#,##0.00\\);_(* \"-\"??_);_(@_)",
            )?,
            G::IncreaseDecimal | G::DecreaseDecimal => {
                let code = self.active_style().num_fmt.to_string();
                let delta = if matches!(cmd, G::IncreaseDecimal) {
                    1
                } else {
                    -1
                };
                let next = change_decimals(&code, delta, self.shown_decimals());
                self.num_fmt(cmd.meta().id, &next)?;
            }
            G::ApplyFormat { format } => self.apply_format(format)?,
            _ => {
                return Err(KilnError::Unsupported(format!(
                    "{} in Kiln Grid",
                    cmd.meta().id
                )));
            }
        }
        Ok(Change::Content)
    }

    /// The shared editing commands (`suite.format.*`, `suite.edit.*`) on the selected cells.
    pub(crate) fn edit_command(&mut self, cmd: &EditCommand) -> Result<Change> {
        use EditCommand as E;
        if !matches!(cmd, E::Copy) && self.edit.is_some() {
            self.commit_edit(None)?;
        }
        let style = self.active_style();
        let f = &style.font;
        match cmd {
            E::Cut => return self.copy_selection(true),
            E::Copy => return self.copy_selection(false),
            E::Paste { data } | E::PasteKeepSource { data } => {
                return self.paste(data.as_ref(), PasteMode::KeepSource);
            }
            E::PasteMerge { data } => return self.paste(data.as_ref(), PasteMode::Merge),
            E::PasteTextOnly { data } => return self.paste(data.as_ref(), PasteMode::TextOnly),
            E::PasteAs { mode, data } => return self.paste(data.as_ref(), *mode),
            E::Bold => self.font(
                "suite.format.bold",
                FontPatch {
                    bold: Some(!f.bold),
                    ..FontPatch::default()
                },
            )?,
            E::Italic => self.font(
                "suite.format.italic",
                FontPatch {
                    italic: Some(!f.italic),
                    ..FontPatch::default()
                },
            )?,
            E::Underline => {
                let u = if f.underline == Underline::Single {
                    Underline::None
                } else {
                    Underline::Single
                };
                self.font(
                    "suite.format.underline",
                    FontPatch {
                        underline: Some(u),
                        ..FontPatch::default()
                    },
                )?;
            }
            E::DoubleUnderline => {
                let u = if f.underline == Underline::Double {
                    Underline::None
                } else {
                    Underline::Double
                };
                self.font(
                    "suite.format.double_underline",
                    FontPatch {
                        underline: Some(u),
                        ..FontPatch::default()
                    },
                )?;
            }
            E::Strikethrough => self.font(
                "suite.format.strikethrough",
                FontPatch {
                    strike: Some(!f.strike),
                    ..FontPatch::default()
                },
            )?,
            E::Subscript => {
                let v = if f.vert_align == VertAlign::Subscript {
                    VertAlign::Baseline
                } else {
                    VertAlign::Subscript
                };
                self.font(
                    "suite.format.subscript",
                    FontPatch {
                        vert_align: Some(v),
                        ..FontPatch::default()
                    },
                )?;
            }
            E::Superscript => {
                let v = if f.vert_align == VertAlign::Superscript {
                    VertAlign::Baseline
                } else {
                    VertAlign::Superscript
                };
                self.font(
                    "suite.format.superscript",
                    FontPatch {
                        vert_align: Some(v),
                        ..FontPatch::default()
                    },
                )?;
            }
            E::FontFamily { family } => {
                let family = family.trim();
                if family.is_empty() || family.len() > 31 {
                    return Err(KilnError::InvalidInput(
                        "a font name has 1–31 characters".into(),
                    ));
                }
                self.font(
                    "suite.format.font_family",
                    FontPatch {
                        name: Some(family.into()),
                        ..FontPatch::default()
                    },
                )?;
            }
            E::FontSize { size } => {
                let s = f64::from(*size);
                if !(1.0..=409.0).contains(&s) {
                    return Err(KilnError::InvalidInput(
                        "a font size is between 1 and 409 points".into(),
                    ));
                }
                self.font(
                    "suite.format.font_size",
                    FontPatch {
                        size: Some(F64((s * 2.0).round() / 2.0)),
                        ..FontPatch::default()
                    },
                )?;
            }
            E::GrowFont { one_point } | E::ShrinkFont { one_point } => {
                let grow = matches!(cmd, E::GrowFont { .. });
                let cur = f.size.0;
                let next = if *one_point {
                    if grow { cur + 1.0 } else { cur - 1.0 }
                } else if grow {
                    SIZES
                        .iter()
                        .copied()
                        .find(|&s| s > cur)
                        .unwrap_or(cur + 10.0)
                } else {
                    SIZES
                        .iter()
                        .rev()
                        .copied()
                        .find(|&s| s < cur)
                        .unwrap_or(cur - 1.0)
                };
                let next = next.clamp(1.0, 409.0);
                let label = if grow {
                    "suite.format.grow_font"
                } else {
                    "suite.format.shrink_font"
                };
                self.font(
                    label,
                    FontPatch {
                        size: Some(F64(next)),
                        ..FontPatch::default()
                    },
                )?;
            }
            E::FontColor { color } => {
                let c = match color.as_deref() {
                    Some(c) => parse_color(c)
                        .ok_or_else(|| KilnError::InvalidInput(format!("“{c}” is not a colour")))?,
                    None => Color::Auto,
                };
                self.font(
                    "suite.format.font_color",
                    FontPatch {
                        color: Some(c),
                        ..FontPatch::default()
                    },
                )?;
            }
            E::ClearFormatting => {
                let sid = self.sid()?;
                let ranges = self.view().sel.ranges;
                self.exec(
                    "suite.format.clear_formatting",
                    kiln_sheet::Command::ClearRange {
                        sheet: sid,
                        ranges,
                        what: kiln_sheet::commands::ClearWhat::Formats,
                    },
                )?;
            }
            E::AlignLeft | E::AlignCenter | E::AlignRight | E::Justify => {
                let target = match cmd {
                    E::AlignLeft => HAlign::Left,
                    E::AlignCenter => HAlign::Center,
                    E::AlignRight => HAlign::Right,
                    _ => HAlign::Justify,
                };
                // Excel's toggle: the pressed alignment again returns to General.
                let h = if style.alignment.horizontal == target {
                    HAlign::General
                } else {
                    target
                };
                self.align(
                    cmd_label(cmd),
                    AlignmentPatch {
                        horizontal: Some(h),
                        ..AlignmentPatch::default()
                    },
                )?;
            }
            E::Indent | E::Outdent => {
                let cur = style.alignment.indent;
                let indent = if matches!(cmd, E::Indent) {
                    cur.saturating_add(1).min(250)
                } else {
                    cur.saturating_sub(1)
                };
                // Indenting general text aligns it left (Excel does the same).
                let horizontal = (matches!(cmd, E::Indent)
                    && matches!(style.alignment.horizontal, HAlign::General))
                .then_some(HAlign::Left);
                self.align(
                    cmd_label(cmd),
                    AlignmentPatch {
                        indent: Some(indent),
                        horizontal,
                        ..AlignmentPatch::default()
                    },
                )?;
            }
            E::InsertDate | E::InsertTime => {
                return self.editing(&GridCommand::InsertDateTime {
                    time: matches!(cmd, E::InsertTime),
                    utc_offset_minutes: 0,
                });
            }
            other => {
                return Err(KilnError::Unsupported(format!("{other:?} in Kiln Grid")));
            }
        }
        Ok(Change::Content)
    }
}

fn cmd_label(cmd: &EditCommand) -> &'static str {
    kiln_protocol::Command::Suite(kiln_protocol::SuiteCommand::Edit(cmd.clone())).id()
}

/// The protocol's colour of a model colour (`RRGGBB`).
pub(crate) fn hex(c: Option<Color>, theme: &kiln_sheet::Theme, auto: u32) -> Option<String> {
    color_hex(c, theme, auto)
}

#[allow(dead_code)]
fn _sheet(_: SheetId) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_grow_and_shrink_in_every_section() {
        assert_eq!(change_decimals("General", 1, 2), "0.000");
        assert_eq!(change_decimals("General", -1, 1), "0");
        assert_eq!(change_decimals("0", 1, 0), "0.0");
        assert_eq!(change_decimals("0.00", -1, 0), "0.0");
        assert_eq!(change_decimals("0.0", -1, 0), "0");
        assert_eq!(
            change_decimals("#,##0.00_);(#,##0.00)", 1, 0),
            "#,##0.000_);(#,##0.000)"
        );
        assert_eq!(change_decimals("0%", 1, 0), "0.0%");
        assert_eq!(change_decimals("\"$\"#,##0", 1, 0), "\"$\"#,##0.0");
    }

    #[test]
    fn names_presets() {
        assert_eq!(preset_name("General"), "General");
        assert_eq!(preset_name("0.00"), "Number");
        assert_eq!(preset_name("m/d/yyyy"), "Short Date");
        assert_eq!(preset_name("0%"), "Percentage");
        assert_eq!(preset_name("#,##0"), "Number");
        assert_eq!(preset_name("[Blue]0;[Red]-0"), "Custom");
        assert_eq!(category("dddd, mmmm d, yyyy"), "Date");
        assert!(accounting(Some("GBP")).contains('£'));
        assert_eq!(parse_color("#1d8a60"), Some(Color::rgb(0x1D_8A60)));
        assert_eq!(parse_color("nope"), None);
    }
}
