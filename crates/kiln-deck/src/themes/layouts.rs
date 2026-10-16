// SPDX-License-Identifier: AGPL-3.0-or-later
//! The eleven standard layouts: which placeholders each has (types, indices and size hints as
//! PowerPoint's own layouts use them, so files match up when they are opened in PowerPoint),
//! where they sit on a theme's grid, and their prompt text.

use crate::geom::Rect;
use crate::shape::{Placeholder, PlaceholderSize, PlaceholderType};
use crate::slide::LayoutKind;

/// A theme's layout grid, in fractions of the slide width (`x`, gutters) and height (`y`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Left and right margin.
    pub margin_x: f64,
    /// Top margin (above the title).
    pub margin_top: f64,
    /// Bottom margin (below the footer band).
    pub margin_bottom: f64,
    /// Title height.
    pub title_height: f64,
    /// Space between the title and the body, and between the body and the footer band.
    pub gap: f64,
    /// Footer band height.
    pub footer_height: f64,
    /// Space between columns.
    pub gutter: f64,
}

impl Default for Grid {
    /// PowerPoint's Office Theme proportions.
    fn default() -> Self {
        Self {
            margin_x: 0.06875,
            margin_top: 0.0532,
            margin_bottom: 0.02,
            title_height: 0.1933,
            gap: 0.0197,
            footer_height: 0.0532,
            gutter: 0.0275,
        }
    }
}

/// One placeholder of a standard layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceholderSpec {
    /// Type, index, orientation and size hint.
    pub ph: Placeholder,
    /// Where it sits.
    pub rect: Rect,
    /// Its prompt text (the layout placeholder's text; one paragraph per line, the line's
    /// position is its list level for body placeholders).
    pub prompt: &'static str,
    /// The `n`-th placeholder of its type in the layout (1-based), for theme overrides.
    pub ordinal: usize,
}

/// The master's body prompt: one paragraph per list level.
pub const BODY_PROMPT: &str =
    "Click to edit Master text styles\nSecond level\nThird level\nFourth level\nFifth level";
/// The title prompt.
pub const TITLE_PROMPT: &str = "Click to edit Master title style";
/// The subtitle prompt.
pub const SUBTITLE_PROMPT: &str = "Click to edit Master subtitle style";
/// A caption or section prompt.
pub const TEXT_PROMPT: &str = "Click to edit Master text styles";

fn emu(f: f64, total: i64) -> i64 {
    crate::geom::round_emu(f * total as f64)
}

/// Geometry derived from a grid for one slide size.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    w: i64,
    h: i64,
    /// Left content edge.
    pub left: i64,
    /// Content width.
    pub width: i64,
    /// Title top.
    pub title_top: i64,
    /// Title height.
    pub title_h: i64,
    /// Body top.
    pub body_top: i64,
    /// Body height.
    pub body_h: i64,
    /// Footer top.
    pub footer_top: i64,
    /// Footer height.
    pub footer_h: i64,
    gutter: i64,
}

impl Frame {
    /// The frame of a grid on a slide.
    #[must_use]
    pub fn new(grid: &Grid, w: i64, h: i64) -> Self {
        let left = emu(grid.margin_x, w);
        let width = (w - 2 * left).max(1);
        let title_top = emu(grid.margin_top, h);
        let title_h = emu(grid.title_height, h).max(1);
        let footer_h = emu(grid.footer_height, h).max(1);
        let footer_top = h - emu(grid.margin_bottom, h) - footer_h;
        let gap = emu(grid.gap, h);
        let body_top = title_top + title_h + gap;
        let body_h = (footer_top - gap - body_top).max(1);
        Self {
            w,
            h,
            left,
            width,
            title_top,
            title_h,
            body_top,
            body_h,
            footer_top,
            footer_h,
            gutter: emu(grid.gutter, w),
        }
    }

    /// A rectangle from fractions of the slide.
    #[must_use]
    pub fn frac(&self, x: f64, y: f64, cx: f64, cy: f64) -> Rect {
        Rect::new(
            emu(x, self.w),
            emu(y, self.h),
            emu(cx, self.w),
            emu(cy, self.h),
        )
    }

    /// The title box.
    #[must_use]
    pub fn title(&self) -> Rect {
        Rect::new(self.left, self.title_top, self.width, self.title_h)
    }

    /// The body box.
    #[must_use]
    pub fn body(&self) -> Rect {
        Rect::new(self.left, self.body_top, self.width, self.body_h)
    }

    /// Column `i` of `n` within the body (or within `r`).
    #[must_use]
    pub fn column(&self, r: Rect, i: usize, n: usize) -> Rect {
        let n_i = i64::try_from(n.max(1)).unwrap_or(1);
        let i_i = i64::try_from(i).unwrap_or(0);
        let cw = (r.cx - self.gutter * (n_i - 1)) / n_i;
        Rect::new(r.x + i_i * (cw + self.gutter), r.y, cw, r.cy)
    }

    /// The date, footer and slide number boxes.
    #[must_use]
    pub fn footers(&self) -> [Rect; 3] {
        let side = self.width * 26 / 100;
        let mid = self.width * 39 / 100;
        let y = self.footer_top;
        let h = self.footer_h;
        [
            Rect::new(self.left, y, side, h),
            Rect::new(self.left + (self.width - mid) / 2, y, mid, h),
            Rect::new(self.left + self.width - side, y, side, h),
        ]
    }
}

fn ph(kind: PlaceholderType, idx: u32) -> Placeholder {
    Placeholder::new(kind, idx)
}

fn sized(mut p: Placeholder, size: PlaceholderSize) -> Placeholder {
    p.size = size;
    p
}

fn vertical(mut p: Placeholder) -> Placeholder {
    p.vertical = true;
    p
}

/// The footer placeholders every layout carries (date idx 10, footer 11, number 12).
#[must_use]
pub fn footer_specs(f: &Frame) -> Vec<PlaceholderSpec> {
    let [d, ft, n] = f.footers();
    vec![
        PlaceholderSpec {
            ph: sized(ph(PlaceholderType::Date, 10), PlaceholderSize::Half),
            rect: d,
            prompt: "",
            ordinal: 1,
        },
        PlaceholderSpec {
            ph: sized(ph(PlaceholderType::Footer, 11), PlaceholderSize::Quarter),
            rect: ft,
            prompt: "",
            ordinal: 1,
        },
        PlaceholderSpec {
            ph: sized(
                ph(PlaceholderType::SlideNumber, 12),
                PlaceholderSize::Quarter,
            ),
            rect: n,
            prompt: "",
            ordinal: 1,
        },
    ]
}

/// The content placeholders of a standard layout (footers excluded), back to front.
#[must_use]
pub fn layout_specs(kind: &LayoutKind, f: &Frame) -> Vec<PlaceholderSpec> {
    use PlaceholderType as T;
    let title = PlaceholderSpec {
        ph: ph(T::Title, 0),
        rect: f.title(),
        prompt: TITLE_PROMPT,
        ordinal: 1,
    };
    let spec = |p: Placeholder, rect: Rect, prompt: &'static str, ordinal: usize| PlaceholderSpec {
        ph: p,
        rect,
        prompt,
        ordinal,
    };
    let body = f.body();
    match kind {
        LayoutKind::Title => {
            // Centred block in the upper middle, subtitle beneath (PowerPoint's proportions).
            let w = f.width * 85 / 100;
            let x = f.left + (f.width - w) / 2;
            let t = f.frac(0.0, 0.1637, 0.0, 0.3481);
            let s = f.frac(0.0, 0.5252, 0.0, 0.2414);
            vec![
                spec(
                    ph(T::CenteredTitle, 0),
                    Rect::new(x, t.y, w, t.cy),
                    TITLE_PROMPT,
                    1,
                ),
                spec(
                    ph(T::Subtitle, 1),
                    Rect::new(x, s.y, w, s.cy),
                    SUBTITLE_PROMPT,
                    1,
                ),
            ]
        }
        LayoutKind::TitleContent => vec![title, spec(ph(T::Object, 1), body, BODY_PROMPT, 1)],
        LayoutKind::SectionHeader => {
            let t = f.frac(0.0, 0.2493, 0.0, 0.4160);
            let b = f.frac(0.0, 0.6692, 0.0, 0.2187);
            vec![
                spec(
                    ph(T::Title, 0),
                    Rect::new(f.left, t.y, f.width, t.cy),
                    TITLE_PROMPT,
                    1,
                ),
                spec(
                    ph(T::Body, 1),
                    Rect::new(f.left, b.y, f.width, b.cy),
                    TEXT_PROMPT,
                    1,
                ),
            ]
        }
        LayoutKind::TwoContent => vec![
            title,
            spec(
                sized(ph(T::Object, 1), PlaceholderSize::Half),
                f.column(body, 0, 2),
                BODY_PROMPT,
                1,
            ),
            spec(
                sized(ph(T::Object, 2), PlaceholderSize::Half),
                f.column(body, 1, 2),
                BODY_PROMPT,
                2,
            ),
        ],
        LayoutKind::Comparison => {
            let head_h = body.cy * 19 / 100;
            let head = Rect::new(body.x, body.y, body.cx, head_h);
            let content = Rect::new(body.x, body.y + head_h, body.cx, body.cy - head_h);
            vec![
                title,
                spec(
                    sized(ph(T::Body, 1), PlaceholderSize::Quarter),
                    f.column(head, 0, 2),
                    TEXT_PROMPT,
                    1,
                ),
                spec(
                    sized(ph(T::Object, 2), PlaceholderSize::Quarter),
                    f.column(content, 0, 2),
                    BODY_PROMPT,
                    1,
                ),
                spec(
                    sized(ph(T::Body, 3), PlaceholderSize::Quarter),
                    f.column(head, 1, 2),
                    TEXT_PROMPT,
                    2,
                ),
                spec(
                    sized(ph(T::Object, 4), PlaceholderSize::Quarter),
                    f.column(content, 1, 2),
                    BODY_PROMPT,
                    2,
                ),
            ]
        }
        LayoutKind::TitleOnly => vec![title],
        LayoutKind::Blank => Vec::new(),
        LayoutKind::ContentCaption | LayoutKind::PictureCaption => {
            // Caption column on the left (a third), content on the right.
            let top = f.title_top + (f.body_top - f.title_top) / 3;
            let bottom = f.footer_top;
            let cap_w = f.width * 37 / 100;
            let content_x = f.left + cap_w + f.gutter * 2;
            let title_h = (bottom - top) * 30 / 100;
            let cap_title = Rect::new(f.left, top, cap_w, title_h);
            let cap_text = Rect::new(f.left, top + title_h, cap_w, bottom - top - title_h);
            let content = Rect::new(content_x, top, f.left + f.width - content_x, bottom - top);
            let main = if *kind == LayoutKind::PictureCaption {
                spec(ph(T::Picture, 1), content, "", 1)
            } else {
                spec(ph(T::Object, 1), content, BODY_PROMPT, 1)
            };
            vec![
                spec(ph(T::Title, 0), cap_title, TITLE_PROMPT, 1),
                main,
                spec(
                    sized(ph(T::Body, 2), PlaceholderSize::Half),
                    cap_text,
                    TEXT_PROMPT,
                    1,
                ),
            ]
        }
        LayoutKind::TitleVerticalText => {
            vec![title, spec(vertical(ph(T::Body, 1)), body, BODY_PROMPT, 1)]
        }
        LayoutKind::VerticalTitleText => {
            let top = f.title_top;
            let h = f.footer_top - f.title_top - (f.body_top - f.title_top - f.title_h);
            let title_w = f.width * 22 / 100;
            let tx = f.left + f.width - title_w;
            vec![
                spec(
                    vertical(ph(T::Title, 0)),
                    Rect::new(tx, top, title_w, h),
                    TITLE_PROMPT,
                    1,
                ),
                spec(
                    vertical(ph(T::Body, 1)),
                    Rect::new(f.left, top, f.width - title_w - f.gutter, h),
                    BODY_PROMPT,
                    1,
                ),
            ]
        }
        LayoutKind::Custom | LayoutKind::Other(_) => vec![title],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_fit_the_slide_and_do_not_overlap() {
        for (w, h) in [(12_192_000, 6_858_000), (9_144_000, 6_858_000)] {
            let f = Frame::new(&Grid::default(), w, h);
            let slide = Rect::new(0, 0, w, h);
            for kind in LayoutKind::STANDARD {
                let mut specs = layout_specs(&kind, &f);
                specs.extend(footer_specs(&f));
                for s in &specs {
                    assert!(s.rect.cx > 0 && s.rect.cy > 0, "{kind:?} {s:?}");
                    assert_eq!(
                        s.rect.union(&slide),
                        slide,
                        "{kind:?} {s:?} leaves the slide"
                    );
                }
                for (i, a) in specs.iter().enumerate() {
                    for b in &specs[i + 1..] {
                        assert!(
                            !a.rect.intersects(&b.rect),
                            "{kind:?}: {a:?} overlaps {b:?}"
                        );
                    }
                }
                // Indices are unique within a layout.
                let mut idx: Vec<u32> = specs.iter().map(|s| s.ph.idx).collect();
                idx.sort_unstable();
                idx.dedup();
                assert_eq!(idx.len(), specs.len(), "{kind:?}");
            }
        }
    }

    #[test]
    fn layouts_have_powerpoints_placeholders() {
        use PlaceholderType as T;
        let f = Frame::new(&Grid::default(), 12_192_000, 6_858_000);
        let types = |k: LayoutKind| -> Vec<(T, u32)> {
            layout_specs(&k, &f)
                .iter()
                .map(|s| (s.ph.kind, s.ph.idx))
                .collect()
        };
        assert_eq!(
            types(LayoutKind::Title),
            vec![(T::CenteredTitle, 0), (T::Subtitle, 1)]
        );
        assert_eq!(
            types(LayoutKind::TitleContent),
            vec![(T::Title, 0), (T::Object, 1)]
        );
        assert_eq!(
            types(LayoutKind::Comparison),
            vec![
                (T::Title, 0),
                (T::Body, 1),
                (T::Object, 2),
                (T::Body, 3),
                (T::Object, 4)
            ]
        );
        assert_eq!(types(LayoutKind::Blank), vec![]);
        assert_eq!(
            types(LayoutKind::PictureCaption),
            vec![(T::Title, 0), (T::Picture, 1), (T::Body, 2)]
        );
        assert!(
            layout_specs(&LayoutKind::VerticalTitleText, &f)
                .iter()
                .all(|s| s.ph.vertical)
        );
    }

    /// A new slide on a standard layout of every bundled theme: it gets the layout's content
    /// placeholders, each inheriting its position from the layout and its text style from the
    /// master (through the layout's own overrides).
    fn check_layout(kind: LayoutKind, expect: &[PlaceholderType]) {
        use crate::inherit::TextStyleKind;
        use crate::presentation::Presentation;
        for theme in crate::themes::bundled() {
            let mut p = Presentation::with_theme(&theme, crate::slide::SlideSize::WIDESCREEN, None);
            let layout = p.masters[0].layout_of_kind(&kind).unwrap().id;
            let tx = crate::edit::insert_slide(&mut p, layout, 0).unwrap();
            p.apply(tx).unwrap();
            let slide = &p.slides[0];
            let kinds: Vec<PlaceholderType> = slide
                .shapes
                .iter()
                .map(|s| s.placeholder.unwrap().kind)
                .collect();
            assert_eq!(kinds, expect, "{} {kind:?}", theme.id);
            let lay = &p.layout(layout).unwrap().1;
            for s in &slide.shapes {
                let chain = p.chain_of(s.id).unwrap();
                let from = chain.layout.expect("matched a layout placeholder");
                assert_eq!(
                    p.resolved_xfrm(s.id),
                    from.xfrm,
                    "{} {kind:?} {}",
                    theme.id,
                    s.name
                );
                assert!(lay.shapes.iter().any(|l| l.id == from.id));
                assert!(
                    chain.master.is_some(),
                    "{} {kind:?}: {} reaches the master",
                    theme.id,
                    s.name
                );
                // The text style: the master's style for the kind, under the placeholders'
                // list styles.
                let m = &p.masters[0];
                let master_style = match TextStyleKind::of(s) {
                    TextStyleKind::Title => &m.title_style,
                    TextStyleKind::Body => &m.body_style,
                    TextStyleKind::Other => &m.other_style,
                };
                let own = from
                    .text()
                    .and_then(|t| t.list_style.get_level(0))
                    .and_then(|l| l.run.size);
                let resolved = p.level_style(s.id, 0).unwrap().run.size;
                assert_eq!(
                    resolved,
                    own.or(master_style.level(0).run.size),
                    "{} {kind:?}",
                    theme.id
                );
            }
            p.validate().unwrap();
        }
    }

    #[test]
    fn title() {
        check_layout(
            LayoutKind::Title,
            &[PlaceholderType::CenteredTitle, PlaceholderType::Subtitle],
        );
    }

    #[test]
    fn title_content() {
        check_layout(
            LayoutKind::TitleContent,
            &[PlaceholderType::Title, PlaceholderType::Object],
        );
    }

    #[test]
    fn section_header() {
        check_layout(
            LayoutKind::SectionHeader,
            &[PlaceholderType::Title, PlaceholderType::Body],
        );
    }

    #[test]
    fn two_content() {
        check_layout(
            LayoutKind::TwoContent,
            &[
                PlaceholderType::Title,
                PlaceholderType::Object,
                PlaceholderType::Object,
            ],
        );
    }

    #[test]
    fn comparison() {
        use PlaceholderType as T;
        check_layout(
            LayoutKind::Comparison,
            &[T::Title, T::Body, T::Object, T::Body, T::Object],
        );
    }

    #[test]
    fn title_only() {
        check_layout(LayoutKind::TitleOnly, &[PlaceholderType::Title]);
    }

    #[test]
    fn blank() {
        check_layout(LayoutKind::Blank, &[]);
    }

    #[test]
    fn content_caption() {
        use PlaceholderType as T;
        check_layout(LayoutKind::ContentCaption, &[T::Title, T::Object, T::Body]);
    }

    #[test]
    fn picture_caption() {
        use PlaceholderType as T;
        check_layout(LayoutKind::PictureCaption, &[T::Title, T::Picture, T::Body]);
    }

    #[test]
    fn title_vertical_text() {
        check_layout(
            LayoutKind::TitleVerticalText,
            &[PlaceholderType::Title, PlaceholderType::Body],
        );
    }

    #[test]
    fn vertical_title_text() {
        check_layout(
            LayoutKind::VerticalTitleText,
            &[PlaceholderType::Title, PlaceholderType::Body],
        );
    }
}
