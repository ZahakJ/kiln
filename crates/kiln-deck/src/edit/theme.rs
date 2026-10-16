// SPDX-License-Identifier: AGPL-3.0-or-later
//! Design tab: apply a theme (a new master and layouts; slides move to the layout of the same
//! kind), a variant, or theme colours and fonts.

use kiln_common::NodeId;
use kiln_ooxml::drawingml::theme::{ColorScheme, FontScheme};

use super::tx;
use crate::error::{DeckError, Result};
use crate::ops::{EditKind, Op, Transaction};
use crate::presentation::Presentation;
use crate::slide::{ColorMapping, LayoutKind, Master};
use crate::themes::ThemeFile;

/// The layout of `new` a slide on `old_layout` moves to: same kind, else same name, else Title
/// and Content, else the first.
fn counterpart(new: &Master, kind: &LayoutKind, name: &str) -> NodeId {
    new.layouts
        .iter()
        .find(|l| &l.kind == kind && *kind != LayoutKind::Custom)
        .or_else(|| new.layouts.iter().find(|l| l.name == name))
        .or_else(|| new.layout_of_kind(&LayoutKind::TitleContent))
        .unwrap_or(&new.layouts[0])
        .id
}

/// Applies a theme to the whole presentation: its master (with a variant) replaces every
/// master, and each slide moves to the new layout of its layout's kind. Slide content and
/// overrides stay.
///
/// # Errors
/// A theme that builds no layouts (cannot happen for parsed theme files).
pub fn apply_theme(
    pres: &mut Presentation,
    theme: &ThemeFile,
    variant: Option<usize>,
) -> Result<Transaction> {
    let size = pres.slide_size.clone();
    let master = theme.build_master(pres, &size, variant);
    if master.layouts.is_empty() {
        return Err(DeckError::Theme("the theme has no layouts".into()));
    }
    let mut ops = vec![Op::InsertMaster {
        index: pres.masters.len(),
        master: Box::new(master.clone()),
    }];
    for s in &pres.slides {
        let (kind, name) = pres
            .layout(s.layout)
            .map(|(_, l)| (l.kind.clone(), l.name.clone()))
            .unwrap_or_default();
        ops.push(Op::SetSlideLayout {
            slide: s.id,
            layout: counterpart(&master, &kind, &name),
        });
    }
    for index in (0..pres.masters.len()).rev() {
        ops.push(Op::RemoveMaster { index });
    }
    Ok(tx(ops, EditKind::Structure, Vec::new()))
}

/// Applies a theme file's variant to one master: its colours, fonts, colour map and background.
///
/// # Errors
/// An unknown master or variant.
pub fn apply_variant(
    pres: &mut Presentation,
    master: NodeId,
    theme: &ThemeFile,
    variant: usize,
) -> Result<Transaction> {
    let m = pres.master(master).ok_or(DeckError::UnknownNode(master))?;
    let v = theme.variants.get(variant).ok_or(DeckError::OutOfRange {
        index: variant,
        len: theme.variants.len(),
    })?;
    let mut t = m.theme.clone();
    if let Some(c) = &v.colors {
        t.color_scheme = c.clone();
    }
    if let Some(f) = &v.fonts {
        t.font_scheme = f.clone();
    }
    let mut ops = Vec::new();
    if t != m.theme {
        ops.push(Op::SetTheme {
            master,
            theme: Box::new(t),
        });
    }
    if let Some(dark) = v.dark {
        let map = if dark {
            ColorMapping::dark()
        } else {
            ColorMapping::default()
        };
        if map != m.color_map {
            ops.push(Op::SetColorMap {
                owner: master,
                map: Some(map),
            });
        }
    }
    let bg = v
        .background
        .clone()
        .unwrap_or_else(|| theme.master.background.clone());
    if bg != m.background {
        ops.push(Op::SetBackground {
            owner: master,
            background: Some(bg),
        });
    }
    Ok(tx(ops, EditKind::Format, Vec::new()))
}

/// Design ▸ Variants ▸ Colors: replaces a master's colour scheme.
///
/// # Errors
/// An unknown master.
pub fn set_theme_colors(
    pres: &mut Presentation,
    master: NodeId,
    colors: ColorScheme,
) -> Result<Transaction> {
    let m = pres.master(master).ok_or(DeckError::UnknownNode(master))?;
    let mut t = m.theme.clone();
    t.color_scheme = colors;
    Ok(tx(
        vec![Op::SetTheme {
            master,
            theme: Box::new(t),
        }],
        EditKind::Format,
        Vec::new(),
    ))
}

/// Design ▸ Variants ▸ Fonts: replaces a master's font scheme.
///
/// # Errors
/// An unknown master.
pub fn set_theme_fonts(
    pres: &mut Presentation,
    master: NodeId,
    fonts: FontScheme,
) -> Result<Transaction> {
    let m = pres.master(master).ok_or(DeckError::UnknownNode(master))?;
    let mut t = m.theme.clone();
    t.font_scheme = fonts;
    Ok(tx(
        vec![Op::SetTheme {
            master,
            theme: Box::new(t),
        }],
        EditKind::Format,
        Vec::new(),
    ))
}
