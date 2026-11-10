// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Kiln Write commands that complete Word's ribbon: symbol fonts and Alt+X,
//! AutoCorrect, Page Movement, several selected objects (align, distribute, group), hiding
//! objects, Show Markup ▸ Balloons, drag and drop of text, Table Properties, Sort, the proofing
//! language, Shape Styles, gradients and effects, text direction in shapes, Crop to Shape and
//! picture corrections.

pub(super) mod autocorrect;
mod encoding;
pub(crate) mod equation;
mod objects;
mod shapes;
mod symbols;
#[cfg(test)]
mod tests;
mod text;

use kiln_common::Result;
use kiln_protocol::WriteCommand;
use kiln_protocol::commands::write_office::BalloonMode;
use kiln_protocol::commands::{WriteQuery, WriteQueryResult};

use super::WriteDoc;
use crate::model::Change;

pub(super) use objects::MultiSelection;

/// The editor state of these commands.
#[derive(Debug, Default)]
pub(super) struct OfficeState {
    /// The AutoCorrect list.
    pub autocorrect: autocorrect::AutoCorrectList,
    /// Objects added to the selection.
    pub multi: MultiSelection,
    /// View ▸ Page Movement ▸ Side to Side.
    pub side_to_side: bool,
    /// Show Markup ▸ Balloons.
    pub balloons: BalloonMode,
    /// View ▸ Split: the second pane's region (document px).
    pub split: Option<kiln_protocol::geom::Rect>,
}

/// Whether this module handles a command.
#[must_use]
pub(super) fn handles(cmd: &WriteCommand) -> bool {
    use WriteCommand as W;
    matches!(
        cmd,
        W::InsertSymbol { .. }
            | W::ToggleUnicode
            | W::SetAutoCorrect { .. }
            | W::TextEncoding { .. }
            | W::PageMovement { .. }
            | W::SplitRegion { .. }
            | W::ObjectVisible { .. }
            | W::Balloons { .. }
            | W::MoveSelection { .. }
            | W::TableProperties { .. }
            | W::Sort { .. }
            | W::SetLanguage { .. }
            | W::SelectObject {
                add: Some(true),
                ..
            }
            | W::ObjectAlign { .. }
            | W::ShapeStyle { .. }
            | W::ShapeGradient { .. }
            | W::ShapeEffect { .. }
            | W::ShapeTextDirection { .. }
            | W::CropToShape { .. }
            | W::PictureCorrections { .. }
            | W::InsertEquation { .. }
            | W::GroupObjects { .. }
    )
}

impl WriteDoc {
    /// Runs one of this module's commands.
    pub(super) fn run_office(&mut self, cmd: &WriteCommand) -> Result<Change> {
        use WriteCommand as W;
        self.end_composition();
        match cmd {
            W::PageMovement { side_to_side } => {
                let changed = self.office.side_to_side != *side_to_side;
                self.office.side_to_side = *side_to_side;
                self.sync_page_movement();
                return Ok(if changed {
                    Change::Selection
                } else {
                    Change::None
                });
            }
            W::SplitRegion { region } => {
                let region = region.filter(|r| {
                    [r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite())
                        && r.width > 0.0
                        && r.height > 0.0
                        && r.width < 100_000.0
                        && r.height < 100_000.0
                });
                let changed = self.office.split != region;
                self.office.split = region;
                return Ok(if changed {
                    Change::Selection
                } else {
                    Change::None
                });
            }
            W::Balloons { mode } => {
                let changed = self.office.balloons != *mode;
                self.office.balloons = *mode;
                self.sync_review_display();
                return Ok(if changed {
                    Change::Selection
                } else {
                    Change::None
                });
            }
            W::SelectObject { id, .. } => {
                // Adding to the selection keeps the selection's range: report it changed so the
                // chrome redraws the frames of the selected objects.
                self.select_add(*id)?;
                return Ok(Change::Selection);
            }
            W::SetAutoCorrect { entries } => {
                self.office.autocorrect = autocorrect::AutoCorrectList::new(entries);
                return Ok(Change::None);
            }
            W::TextEncoding {
                encoding,
                reload: false,
            } => {
                self.set_text_encoding(encoding, false)?;
                return Ok(Change::None);
            }
            _ => {}
        }
        self.measure(|d| match cmd {
            W::InsertSymbol { font, code } => d.insert_symbol(font, *code),
            W::ToggleUnicode => d.toggle_unicode(),
            W::ObjectVisible { id, visible } => d.set_object_visible(*id, *visible),
            W::MoveSelection { to, copy } => d.move_selection(*to, *copy),
            W::TableProperties { spec } => d.set_table_properties(spec),
            W::Sort { spec } => d.sort(*spec),
            W::SetLanguage { lang, no_proof } => d.set_language(lang, *no_proof),
            W::ObjectAlign { align, relative_to } => d.align_selected(*align, *relative_to),
            W::ShapeStyle { preset } => d.shape_style(*preset),
            W::ShapeGradient { gradient } => d.shape_gradient(gradient.as_ref()),
            W::ShapeEffect { effect } => d.shape_effect(*effect),
            W::ShapeTextDirection { direction } => d.shape_text_direction(*direction),
            W::CropToShape { preset } => d.crop_to_shape(preset),
            W::PictureCorrections { correction } => d.picture_corrections(*correction),
            W::InsertEquation { linear } => d.insert_equation(linear),
            W::GroupObjects { ungroup } => d.group_objects(*ungroup),
            W::TextEncoding { encoding, .. } => d.set_text_encoding(encoding, true),
            _ => Ok(()),
        })
    }

    /// This module's queries.
    pub(super) fn office_query(&self, q: &WriteQuery) -> Option<WriteQueryResult> {
        Some(match q {
            WriteQuery::SymbolTable { font } => WriteQueryResult::SymbolTable(symbols::table(font)),
            WriteQuery::TableProperties => {
                WriteQueryResult::TableProperties(self.table_properties().map(Box::new))
            }
            WriteQuery::RevisionBalloons => {
                WriteQueryResult::RevisionBalloons(self.revision_balloons())
            }
            #[allow(clippy::cast_precision_loss)]
            WriteQuery::DropPoint { x, y } => {
                WriteQueryResult::DropPoint(self.drop_point(*x as f32, *y as f32))
            }
            WriteQuery::ObjectPicture { id } => {
                WriteQueryResult::ObjectPicture(self.object_picture(*id).map(Box::new))
            }
            WriteQuery::TextEncoding => {
                WriteQueryResult::TextEncoding(self.text_encoding_info().map(Box::new))
            }
            _ => return None,
        })
    }

    /// A picture's image as stored (pictures with a correction: as stored too; the zoomed view
    /// shows the original).
    fn object_picture(
        &self,
        id: kiln_common::NodeId,
    ) -> Option<kiln_protocol::commands::write_office::PictureData> {
        let (_, _, d) = self.find_drawing(id)?;
        let kiln_doc::DrawingKind::Picture { media, .. } = &d.kind else {
            return None;
        };
        let m = self.doc.media.get(media)?;
        Some(kiln_protocol::commands::write_office::PictureData {
            content_type: m.content_type.clone(),
            bytes: m.data.clone(),
            alt_text: d.description.clone(),
        })
    }

    /// The revision balloons of the laid-out pages.
    fn revision_balloons(&self) -> Vec<kiln_protocol::commands::write_office::RevisionBalloonInfo> {
        let Some(v) = &self.view else {
            return Vec::new();
        };
        let height = |id: kiln_common::NodeId| self.comment_balloon_height(id);
        kiln_doc_layout::annotations::place_balloons(&v.layout, &height)
            .into_iter()
            .filter_map(|b| {
                let r = b.revision?;
                let text = if r.deleted {
                    let t: String = r.text.chars().take(120).collect();
                    format!("Deleted: {t}")
                } else {
                    "Formatted".to_owned()
                };
                Some(kiln_protocol::commands::write_office::RevisionBalloonInfo {
                    para: b.comment,
                    offset: u32::try_from(r.offset).unwrap_or(u32::MAX),
                    deleted: r.deleted,
                    text,
                    color: kiln_doc_layout::annotations::author_hex(&r.author),
                    author: r.author,
                    rect: kiln_protocol::geom::Rect::new(b.rect.x, b.rect.y, b.rect.w, b.rect.h),
                    anchor: kiln_protocol::geom::Point::new(b.anchor.0, b.anchor.1),
                })
            })
            .collect()
    }

    /// Side to Side page movement in the layout.
    pub(super) fn sync_page_movement(&mut self) {
        let side = self.office.side_to_side;
        if let Some(v) = &mut self.view {
            let mut o = v.layout.options().clone();
            if o.side_to_side != side {
                o.side_to_side = side;
                v.layout.set_options(o);
            }
        }
    }
}
