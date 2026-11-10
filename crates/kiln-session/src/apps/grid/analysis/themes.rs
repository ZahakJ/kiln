// SPDX-License-Identifier: AGPL-3.0-or-later
//! Page Layout ▸ Themes, Colors and Fonts: the twelve Kiln themes Deck uses (`kiln_deck::themes`,
//! DrawingML themes in `kiln-ooxml`'s model) plus Excel's Office theme, applied to the workbook
//! theme that theme colours and theme fonts resolve against; one undo step.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::grid_analysis::{GridThemeInfo, GridThemePart};
use kiln_sheet::{Command, Theme};

use super::super::GridDoc;
use crate::model::Change;

/// A gallery theme: its id and the sheet model's theme.
fn sheet_theme(t: &kiln_ooxml::drawingml::theme::Theme) -> Theme {
    let table = kiln_ooxml::drawingml::theme::theme_color_table(t);
    let mut colors = [0u32; 12];
    for (i, c) in table.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ch = |v: f64| v.round().clamp(0.0, 255.0) as u32;
        colors[i] = (ch(c.r) << 16) | (ch(c.g) << 8) | ch(c.b);
    }
    Theme {
        name: t.name.clone(),
        colors,
        major_font: t.font_scheme.major.latin.typeface.clone(),
        minor_font: t.font_scheme.minor.latin.typeface.clone(),
    }
}

/// The gallery: Office first (Excel's default), then the Kiln themes.
pub(crate) fn gallery() -> Vec<(String, Theme)> {
    let mut out = vec![("office".to_owned(), Theme::default())];
    for f in kiln_deck::themes::bundled() {
        let mut t = sheet_theme(&f.theme);
        t.name.clone_from(&f.name);
        out.push((f.id.clone(), t));
    }
    out
}

/// The Themes gallery's entries.
pub(crate) fn infos() -> Vec<GridThemeInfo> {
    gallery()
        .into_iter()
        .map(|(id, t)| GridThemeInfo {
            id,
            name: t.name.clone(),
            colors: t.colors.iter().map(|c| format!("{c:06X}")).collect(),
            major_font: t.major_font.clone(),
            minor_font: t.minor_font.clone(),
        })
        .collect()
}

impl GridDoc {
    /// Applies a theme (or its colours, or its fonts).
    pub(crate) fn set_theme(&mut self, theme: &str, part: GridThemePart) -> Result<Change> {
        let (_, t) = gallery()
            .into_iter()
            .find(|(id, t)| id.eq_ignore_ascii_case(theme) || t.name.eq_ignore_ascii_case(theme))
            .ok_or_else(|| KilnError::InvalidInput(format!("there is no theme called {theme}")))?;
        let now = self.workbook.theme().clone();
        let next = match part {
            GridThemePart::All => t,
            GridThemePart::Colors => Theme {
                colors: t.colors,
                name: t.name,
                ..now.clone()
            },
            GridThemePart::Fonts => Theme {
                major_font: t.major_font,
                minor_font: t.minor_font,
                ..now.clone()
            },
        };
        if next == now {
            return Ok(Change::None);
        }
        self.exec(
            "grid.page_layout.apply_theme",
            Command::SetTheme {
                theme: Box::new(next),
            },
        )?;
        Ok(Change::Content)
    }
}
