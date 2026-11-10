// SPDX-License-Identifier: AGPL-3.0-or-later
//! Design ▸ Variants ▸ Colors, Fonts, Effects and Background Styles, and Design ▸ Themes ▸
//! Browse for Themes: the schemes of the bundled themes and of Office's, the twelve
//! background styles (for every slide), and the theme of an Office theme file (`.thmx`) or the master, layouts and
//! theme of another presentation or template.

use std::collections::BTreeMap;

use kiln_common::{KilnError, Result};
use kiln_deck::ops::{EditKind, Op, Transaction};
use kiln_deck::themes::bundled;
use kiln_deck::{Background, ColorMapping, Part, Shape};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::theme::{ColorScheme, Theme, office_theme, parse_theme_bytes};
use kiln_ooxml::opc::OpcPackage;
use kiln_protocol::commands::DeckCommand;

use super::DeckDoc;
use super::features::reuse::{blip, blip_mut, fresh, rename_shape_refs, shape_refs};
use crate::model::Change;

/// Office's Grayscale colour scheme.
fn grayscale() -> ColorScheme {
    let c = [
        "000000", "FFFFFF", "000000", "F8F8F8", "DDDDDD", "B2B2B2", "969696", "808080", "5F5F5F",
        "4D4D4D", "5F5F5F", "919191",
    ];
    ColorScheme {
        name: "Grayscale".into(),
        colors: c.map(DrawingColor::srgb),
    }
}

/// The theme a scheme name of the Colors, Fonts or Effects menus comes from.
fn scheme_theme(name: &str) -> Option<Theme> {
    if name.eq_ignore_ascii_case("office") {
        return Some(office_theme());
    }
    bundled()
        .into_iter()
        .find(|t| t.id.eq_ignore_ascii_case(name) || t.name.eq_ignore_ascii_case(name))
        .map(|t| t.theme)
}

/// Background style 1–12: the theme's three background fills (columns) in four rows — Light 1,
/// Light 2, Dark 1, Dark 2 — as PowerPoint's gallery lays them out, with the colour map that keeps
/// text readable on it (the dark rows map the background colours to the dark slots).
fn background_style(style: u8) -> Option<(Background, ColorMapping)> {
    if !(1..=12).contains(&style) {
        return None;
    }
    let i = u32::from(style - 1);
    let row = i / 3;
    let background = Background::Theme {
        idx: 1001 + i % 3,
        color: Some(DrawingColor::scheme(if row % 2 == 0 {
            "bg1"
        } else {
            "bg2"
        })),
    };
    let map = if row < 2 {
        ColorMapping::default()
    } else {
        ColorMapping::dark()
    };
    Some((background, map))
}

impl DeckDoc {
    /// The Design commands of this module.
    pub(crate) fn design_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        let label = cmd.meta().id;
        let master = self
            .master()
            .map(|m| (m.id, m.theme.clone(), m.background.clone()))
            .ok_or_else(|| KilnError::InvalidInput("the presentation has no master".into()))?;
        let unknown =
            |name: &str| KilnError::InvalidInput(format!("there is no scheme called {name}"));
        let mut theme = master.1.clone();
        match cmd {
            DeckCommand::SetThemeColors { scheme } => {
                theme.color_scheme = if scheme.eq_ignore_ascii_case("grayscale") {
                    grayscale()
                } else {
                    scheme_theme(scheme)
                        .ok_or_else(|| unknown(scheme))?
                        .color_scheme
                };
            }
            DeckCommand::SetThemeFonts { scheme } => {
                theme.font_scheme = scheme_theme(scheme)
                    .ok_or_else(|| unknown(scheme))?
                    .font_scheme;
            }
            DeckCommand::SetThemeEffects { scheme } => {
                theme.format_scheme = scheme_theme(scheme)
                    .ok_or_else(|| unknown(scheme))?
                    .format_scheme;
            }
            DeckCommand::SetBackgroundStyle { style } => {
                let (bg, map) = background_style(*style).ok_or_else(|| {
                    KilnError::InvalidInput("background styles are numbered 1 to 12".into())
                })?;
                // Every slide shows it, as PowerPoint applies a style to all slides: the master
                // takes it with its colour map, and the layouts' and slides' own backgrounds and
                // colour maps go.
                let mut ops = Vec::new();
                if bg != master.2 {
                    ops.push(Op::SetBackground {
                        owner: master.0,
                        background: Some(bg),
                    });
                }
                if let Some(m) = self.master() {
                    if m.color_map != map {
                        ops.push(Op::SetColorMap {
                            owner: m.id,
                            map: Some(map),
                        });
                    }
                    let layouts: Vec<_> = m.layouts.iter().map(|l| l.id).collect();
                    let owners = m
                        .layouts
                        .iter()
                        .map(|l| (l.id, l.background.is_some(), l.color_map.is_some()))
                        .chain(
                            self.pres
                                .slides
                                .iter()
                                .filter(|s| layouts.contains(&s.layout))
                                .map(|s| (s.id, s.background.is_some(), s.color_map.is_some())),
                        );
                    for (owner, own_bg, own_map) in owners {
                        if own_bg {
                            ops.push(Op::SetBackground {
                                owner,
                                background: None,
                            });
                        }
                        if own_map {
                            ops.push(Op::SetColorMap { owner, map: None });
                        }
                    }
                }
                if ops.is_empty() {
                    return Ok(Change::None);
                }
                self.end_text();
                self.apply(label, Transaction::new(ops))?;
                return Ok(Change::Content);
            }
            DeckCommand::ApplyThemeFile { bytes } => return self.apply_theme_file(bytes),
            _ => return Ok(Change::None),
        }
        if theme == master.1 {
            return Ok(Change::None);
        }
        self.end_text();
        self.apply(
            label,
            Transaction::new(vec![Op::SetTheme {
                master: master.0,
                theme: Box::new(theme),
            }])
            .with_kind(EditKind::Format),
        )?;
        Ok(Change::Content)
    }

    /// Browse for Themes: an Office theme (`.thmx`) gives the current master its theme; a
    /// presentation or template (`.pptx`, `.potx`, `.pptm`) gives the presentation its first
    /// master with its layouts and theme, the slides moving to the layouts of their kinds.
    fn apply_theme_file(&mut self, bytes: &[u8]) -> Result<Change> {
        const ID: &str = "deck.design.browse_themes";
        let not_a_theme =
            || KilnError::InvalidInput("the file is not an Office theme or a presentation".into());
        let pkg = OpcPackage::open(bytes).map_err(|_| not_a_theme())?;
        if pkg.has("/ppt/presentation.xml") {
            let read = kiln_io_pptx::read_pptx(bytes)
                .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
            let other = read.presentation;
            let source = other.masters.first().ok_or_else(not_a_theme)?;
            let mut master = kiln_deck::edit::import_master(&mut self.pres, source);
            // The master's and layouts' pictures come with them, under names free here.
            let mut refs = Vec::new();
            for s in master
                .shapes
                .iter()
                .chain(master.layouts.iter().flat_map(|l| &l.shapes))
            {
                shape_refs(s, &mut refs);
            }
            for bg in std::iter::once(&master.background)
                .chain(master.layouts.iter().filter_map(|l| l.background.as_ref()))
            {
                if let Background::Fill { fill, .. } = bg {
                    refs.extend(blip(Some(fill)).cloned());
                }
            }
            refs.sort();
            refs.dedup();
            let mut names: BTreeMap<String, String> = BTreeMap::new();
            let mut ops = Vec::new();
            for r in refs {
                let Some(part) = other.parts.get(&r) else {
                    continue;
                };
                let taken =
                    |n: &str| self.pres.parts.contains_key(n) || names.values().any(|v| v == n);
                let name = if self
                    .pres
                    .parts
                    .get(&r)
                    .is_some_and(|p| p.bytes == part.bytes)
                {
                    r.clone()
                } else {
                    fresh(&r, &taken)
                };
                if !self.pres.parts.contains_key(&name) {
                    ops.push(Op::SetPart {
                        key: name.clone(),
                        part: Some(Part {
                            content_type: part.content_type.clone(),
                            bytes: part.bytes.clone(),
                        }),
                    });
                }
                names.insert(r, name);
            }
            let rename = |shapes: &mut Vec<Shape>| {
                for s in shapes {
                    rename_shape_refs(s, &names);
                }
            };
            rename(&mut master.shapes);
            for l in &mut master.layouts {
                rename(&mut l.shapes);
            }
            let fix_bg = |bg: &mut Background| {
                if let Background::Fill { fill, .. } = bg
                    && let Some(k) = blip_mut(Some(fill))
                    && let Some(n) = names.get(k.as_str())
                {
                    k.clone_from(n);
                }
            };
            fix_bg(&mut master.background);
            for l in &mut master.layouts {
                if let Some(bg) = l.background.as_mut() {
                    fix_bg(bg);
                }
            }
            let tx = kiln_deck::edit::apply_master(&self.pres, master)
                .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
            ops.extend(tx.ops);
            self.end_text();
            self.apply(ID, Transaction::new(ops).with_kind(EditKind::Structure))?;
            return Ok(Change::Content);
        }
        // An Office theme: its theme part.
        let part = pkg
            .part_names()
            .find(|n| {
                pkg.content_type(n)
                    .is_some_and(|t| t == "application/vnd.openxmlformats-officedocument.theme+xml")
            })
            .map(str::to_owned)
            .ok_or_else(not_a_theme)?;
        let xml = pkg.part(&part).ok().flatten().ok_or_else(not_a_theme)?;
        let theme = parse_theme_bytes(&xml).map_err(|_| not_a_theme())?;
        let master = self
            .master()
            .map(|m| m.id)
            .ok_or_else(|| KilnError::InvalidInput("the presentation has no master".into()))?;
        self.end_text();
        self.apply(
            ID,
            Transaction::new(vec![Op::SetTheme {
                master,
                theme: Box::new(theme),
            }])
            .with_kind(EditKind::Format),
        )?;
        Ok(Change::Content)
    }
}

#[cfg(test)]
mod tests {
    use kiln_protocol::{Command, FileFormat};

    use kiln_deck::Presentation;

    use super::super::DeckDoc;
    use super::*;
    use crate::model::DocModel;

    fn run(d: &mut DeckDoc, c: DeckCommand) {
        d.execute(&Command::Deck(c.clone()))
            .unwrap_or_else(|e| panic!("{c:?}: {e}"));
    }

    fn doc() -> DeckDoc {
        DeckDoc::new(Presentation::new(), None, None)
    }

    fn reopen(d: &DeckDoc) -> Presentation {
        kiln_io_pptx::read_pptx(&d.save(FileFormat::Pptx).expect("save"))
            .expect("reopen")
            .presentation
    }

    #[test]
    fn colors_fonts_effects_and_background_styles_change_the_master_and_are_saved() {
        let mut d = doc();
        run(
            &mut d,
            DeckCommand::SetThemeColors {
                scheme: "Cobalt".into(),
            },
        );
        let cobalt = scheme_theme("cobalt").expect("bundled");
        assert_eq!(d.pres.masters[0].theme.color_scheme, cobalt.color_scheme);
        run(
            &mut d,
            DeckCommand::SetThemeFonts {
                scheme: "Office".into(),
            },
        );
        assert_eq!(
            d.pres.masters[0].theme.font_scheme.major.latin.typeface,
            "Calibri Light"
        );
        run(
            &mut d,
            DeckCommand::SetThemeEffects {
                scheme: "Raku".into(),
            },
        );
        run(&mut d, DeckCommand::SetBackgroundStyle { style: 10 });
        // The layouts' own backgrounds go, so every slide shows the style.
        assert!(
            d.pres.masters[0]
                .layouts
                .iter()
                .all(|l| l.background.is_none())
        );
        // A dark style: the second background colour on the dark colour map.
        assert_eq!(
            d.pres.masters[0].background,
            Background::Theme {
                idx: 1001,
                color: Some(DrawingColor::scheme("bg2"))
            }
        );
        assert_eq!(d.pres.masters[0].color_map, ColorMapping::dark());
        run(
            &mut d,
            DeckCommand::SetThemeColors {
                scheme: "Grayscale".into(),
            },
        );
        let p = reopen(&d);
        let m = &p.masters[0];
        assert_eq!(m.theme.color_scheme.colors, grayscale().colors);
        assert_eq!(m.theme.font_scheme.minor.latin.typeface, "Calibri");
        assert_eq!(m.background, d.pres.masters[0].background);
        assert!(
            d.execute(&Command::Deck(DeckCommand::SetBackgroundStyle {
                style: 13
            }))
            .is_err()
        );
        assert!(
            d.execute(&Command::Deck(DeckCommand::SetThemeColors {
                scheme: "Nope".into()
            }))
            .is_err()
        );
        // Each change is one undo step.
        assert!(d.undo());
        assert_eq!(d.pres.masters[0].theme.color_scheme, cobalt.color_scheme);
    }

    #[test]
    fn browse_for_themes_takes_a_presentations_master_or_a_themes_colours() {
        let mut d = doc();
        run(&mut d, DeckCommand::NewSlide { layout: None });
        let file = super::super::tests::fixture("theme-colors.pptx");
        let other = kiln_io_pptx::read_pptx(&file)
            .expect("fixture")
            .presentation;
        run(&mut d, DeckCommand::ApplyThemeFile { bytes: file });
        assert_eq!(d.pres.masters.len(), 1);
        let m = &d.pres.masters[0];
        assert_eq!(m.theme.color_scheme, other.masters[0].theme.color_scheme);
        assert_eq!(m.layouts.len(), other.masters[0].layouts.len());
        // Every slide is on a layout of the new master.
        for s in &d.pres.slides {
            assert!(m.layouts.iter().any(|l| l.id == s.layout));
        }
        let p = reopen(&d);
        assert_eq!(
            p.masters[0].theme.color_scheme,
            other.masters[0].theme.color_scheme
        );
        // An Office theme file: a package with only a theme part.
        let mut pkg = OpcPackage::new();
        let xml = kiln_ooxml::xml::document_to_bytes(
            &kiln_ooxml::drawingml::theme::serialize_theme(&scheme_theme("Jade").expect("bundled")),
        )
        .expect("theme xml");
        pkg.add_part(
            "/theme/theme/theme1.xml",
            "application/vnd.openxmlformats-officedocument.theme+xml",
            xml,
        )
        .expect("part");
        let bytes = pkg.save().expect("save");
        run(&mut d, DeckCommand::ApplyThemeFile { bytes });
        assert_eq!(
            d.pres.masters[0].theme.color_scheme,
            scheme_theme("Jade").expect("bundled").color_scheme
        );
        assert!(
            d.execute(&Command::Deck(DeckCommand::ApplyThemeFile {
                bytes: b"not a zip".to_vec()
            }))
            .is_err()
        );
    }
}
