// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slide Master view: a master or layout on the stage instead of a slide, its shapes
//! selected, moved, formatted and typed into as a slide's are (every slide on it follows); Insert
//! Slide Master and Insert Layout, Delete, Rename and Preserve; Insert Placeholder and the layouts'
//! Title and Footers; and the thumbnails of the view's rail. Close Master View goes back to the
//! slide that was on the stage.

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::ops::{Op, Transaction};
use kiln_deck::themes::{ThemeFile, bundled, layout_placeholder};
use kiln_deck::{Layout, LayoutKind, Owner, Placeholder, PlaceholderType, Rect, Shape};
use kiln_protocol::commands::DeckCommand;
use kiln_protocol::commands::deck_content::{DeckMasterItem, DeckMasterView, DeckPlaceholderKind};

use super::DeckDoc;
use crate::model::Change;

/// The longest name a master or layout takes.
const MAX_NAME_CHARS: usize = 255;

/// The placeholder types of the footers.
const FOOTERS: [PlaceholderType; 3] = [
    PlaceholderType::Date,
    PlaceholderType::Footer,
    PlaceholderType::SlideNumber,
];

fn is_title(kind: PlaceholderType) -> bool {
    matches!(
        kind,
        PlaceholderType::Title | PlaceholderType::CenteredTitle
    )
}

impl DeckDoc {
    /// The master of the master or layout on the stage.
    fn view_master(&self) -> Option<NodeId> {
        let id = self.view.master?;
        if self.pres.master(id).is_some() {
            return Some(id);
        }
        self.pres.layout(id).map(|(m, _)| m.id)
    }

    /// The layout on the stage (not a master).
    fn view_layout(&self, label: &'static str) -> Result<NodeId> {
        let id = self
            .view
            .master
            .ok_or_else(|| KilnError::unavailable(label, "open View ▸ Slide Master first"))?;
        if self.pres.layout(id).is_none() {
            return Err(KilnError::unavailable(
                label,
                "select a layout under the master first",
            ));
        }
        Ok(id)
    }

    /// Slides on a layout, or on any layout of a master.
    fn slides_on(&self, id: NodeId) -> u32 {
        let layouts: Vec<NodeId> = match self.pres.master(id) {
            Some(m) => m.layouts.iter().map(|l| l.id).collect(),
            None => vec![id],
        };
        let n = self
            .pres
            .slides
            .iter()
            .filter(|s| layouts.contains(&s.layout))
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// Shows `id` on the stage with nothing selected.
    fn show_master_item(&mut self, id: NodeId) {
        self.end_text();
        self.view.master = Some(id);
        self.view.shapes.clear();
        self.view.rail = false;
        self.epoch += 1;
    }

    /// The master's placeholder of `kind` (where a layout's Title or Footers go).
    fn master_placeholder(
        &self,
        master: NodeId,
        kind: PlaceholderType,
    ) -> Option<(Placeholder, Rect)> {
        let m = self.pres.master(master)?;
        m.shapes.iter().find_map(|s| {
            let ph = s.placeholder?;
            let same = ph.kind == kind || (is_title(kind) && is_title(ph.kind));
            same.then(|| Some((Placeholder::new(kind, ph.idx), s.xfrm.as_ref()?.rect())))
                .flatten()
        })
    }

    /// The Slide Master commands.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn master_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        use DeckCommand as D;
        let label = cmd.meta().id;
        let shown = match (cmd, self.view.master) {
            (D::MasterView { .. }, _) => NodeId(0),
            (_, Some(id)) => id,
            (_, None) => {
                return Err(KilnError::unavailable(
                    label,
                    "open View ▸ Slide Master first",
                ));
            }
        };
        match cmd {
            D::MasterView { on: true } => {
                let id = self
                    .pres
                    .slides
                    .get(self.view.current)
                    .map(|s| s.layout)
                    .or_else(|| self.pres.masters.first().map(|m| m.id))
                    .ok_or_else(|| {
                        KilnError::InvalidInput("the presentation has no master".into())
                    })?;
                self.show_master_item(id);
                Ok(Change::Selection)
            }
            D::MasterView { on: false } => {
                self.end_text();
                self.view.master = None;
                self.view.shapes.clear();
                self.epoch += 1;
                Ok(Change::Selection)
            }
            D::SelectMasterItem { id } => {
                if self.pres.master(*id).is_none() && self.pres.layout(*id).is_none() {
                    return Err(KilnError::InvalidInput(
                        "there is no such master or layout".into(),
                    ));
                }
                self.show_master_item(*id);
                Ok(Change::Selection)
            }
            D::InsertSlideMaster => {
                let theme = self
                    .view_master()
                    .and_then(|m| self.pres.master(m))
                    .and_then(|m| {
                        bundled()
                            .into_iter()
                            .find(|t| t.name.eq_ignore_ascii_case(&m.theme.name))
                    })
                    .or_else(|| bundled().into_iter().next())
                    .unwrap_or_else(ThemeFile::fallback);
                let size = self.pres.slide_size.clone();
                let mut master = theme.build_master(&mut self.pres, &size, None);
                if self.pres.is_right_to_left() {
                    kiln_deck::themes::mirror_master(&mut master, size.cx);
                }
                // A second master of the same theme is numbered, as PowerPoint names it.
                let taken = |n: &str| self.pres.masters.iter().any(|m| m.name == n);
                if taken(&master.name) {
                    let base = master.name.clone();
                    master.name = (1..)
                        .map(|k| format!("{k}_{base}"))
                        .find(|n| !taken(n))
                        .unwrap_or(base);
                }
                let id = master.id;
                self.end_text();
                self.apply(
                    label,
                    Transaction::new(vec![Op::InsertMaster {
                        index: self.pres.masters.len(),
                        master: Box::new(master),
                    }]),
                )?;
                self.show_master_item(id);
                Ok(Change::Content)
            }
            D::InsertLayout => {
                let master = self
                    .view_master()
                    .ok_or_else(|| KilnError::InvalidInput("no master".into()))?;
                let mut layout = Layout::new(self.pres.alloc_id(), LayoutKind::Custom);
                let m = self
                    .pres
                    .master(master)
                    .ok_or_else(|| KilnError::InvalidInput("no master".into()))?;
                let names: Vec<String> = m.layouts.iter().map(|l| l.name.clone()).collect();
                layout.name = std::iter::once("Custom Layout".to_owned())
                    .chain((1..).map(|k| format!("{k}_Custom Layout")))
                    .find(|n| !names.contains(n))
                    .unwrap_or_default();
                let index = self
                    .view
                    .master
                    .and_then(|id| m.layouts.iter().position(|l| l.id == id))
                    .map_or(m.layouts.len(), |i| i + 1);
                for kind in std::iter::once(PlaceholderType::Title).chain(FOOTERS) {
                    if let Some((ph, rect)) = self.master_placeholder(master, kind) {
                        let id = self.pres.alloc_id();
                        layout.shapes.push(layout_placeholder(id, ph, rect));
                    }
                }
                let id = layout.id;
                self.end_text();
                self.apply(
                    label,
                    Transaction::new(vec![Op::InsertLayout {
                        master,
                        index,
                        layout: Box::new(layout),
                    }]),
                )?;
                self.show_master_item(id);
                Ok(Change::Content)
            }
            D::DeleteMasterItem => {
                let id = shown;
                let used = self.slides_on(id);
                if used > 0 {
                    return Err(KilnError::InvalidInput(format!(
                        "{used} slide{} use{} it; give {} another layout first",
                        if used == 1 { "" } else { "s" },
                        if used == 1 { "s" } else { "" },
                        if used == 1 { "it" } else { "them" },
                    )));
                }
                let (op, next) =
                    if let Some(index) = self.pres.masters.iter().position(|m| m.id == id) {
                        if self.pres.masters.len() == 1 {
                            return Err(KilnError::InvalidInput(
                                "a presentation keeps at least one master".into(),
                            ));
                        }
                        let next = self.pres.masters[if index == 0 { 1 } else { index - 1 }].id;
                        (Op::RemoveMaster { index }, next)
                    } else {
                        let (m, _) = self
                            .pres
                            .layout(id)
                            .ok_or_else(|| KilnError::InvalidInput("no such layout".into()))?;
                        if m.layouts.len() == 1 {
                            return Err(KilnError::InvalidInput(
                                "a master keeps at least one layout".into(),
                            ));
                        }
                        let index = m.layouts.iter().position(|l| l.id == id).unwrap_or(0);
                        let next = if index == 0 {
                            m.id
                        } else {
                            m.layouts[index - 1].id
                        };
                        (
                            Op::RemoveLayout {
                                master: m.id,
                                index,
                            },
                            next,
                        )
                    };
                self.end_text();
                self.apply(label, Transaction::new(vec![op]))?;
                self.show_master_item(next);
                Ok(Change::Content)
            }
            D::RenameMasterItem { name } => {
                let name = name.trim();
                if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
                    return Err(KilnError::InvalidInput(format!(
                        "a name has 1 to {MAX_NAME_CHARS} characters"
                    )));
                }
                let id = shown;
                let preserve = self.pres.master(id).map_or_else(
                    || self.pres.layout(id).is_some_and(|(_, l)| l.preserve),
                    |m| m.preserve,
                );
                self.apply(
                    label,
                    Transaction::new(vec![Op::SetMasterInfo {
                        owner: id,
                        name: name.to_owned(),
                        preserve,
                    }]),
                )?;
                Ok(Change::Content)
            }
            D::PreserveMaster { on } => {
                let master = self
                    .view_master()
                    .and_then(|m| self.pres.master(m))
                    .map(|m| (m.id, m.name.clone(), m.preserve))
                    .ok_or_else(|| KilnError::InvalidInput("no master".into()))?;
                let on = on.unwrap_or(!master.2);
                if master.2 == on {
                    return Ok(Change::None);
                }
                self.apply(
                    label,
                    Transaction::new(vec![Op::SetMasterInfo {
                        owner: master.0,
                        name: master.1,
                        preserve: on,
                    }]),
                )?;
                Ok(Change::Content)
            }
            D::InsertPlaceholder { kind } => {
                let layout = self.view_layout(label)?;
                let kind = match kind {
                    DeckPlaceholderKind::Content => PlaceholderType::Object,
                    DeckPlaceholderKind::Text => PlaceholderType::Body,
                    DeckPlaceholderKind::Picture => PlaceholderType::Picture,
                    DeckPlaceholderKind::Chart => PlaceholderType::Chart,
                    DeckPlaceholderKind::Table => PlaceholderType::Table,
                    DeckPlaceholderKind::Media => PlaceholderType::Media,
                };
                // A new index, past those of the layout and its master (as PowerPoint numbers).
                let idx = self
                    .pres
                    .owner_shapes(Owner::Layout(layout))
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.placeholder.map(|p| p.idx))
                    .max()
                    .map_or(10, |m| m.max(9) + 1);
                let (w, h) = (self.pres.slide_size.cx, self.pres.slide_size.cy);
                let rect = Rect::new(w / 4, h / 4, w / 2, h / 2);
                let id = self.pres.alloc_id();
                let shape: Shape = layout_placeholder(id, Placeholder::new(kind, idx), rect);
                self.end_text();
                self.apply_with(label, |p| {
                    kiln_deck::edit::insert_shapes(p, layout, vec![shape], None)
                })?;
                self.view.shapes = vec![id];
                Ok(Change::Content)
            }
            D::LayoutTitle { on } => {
                let on = on.unwrap_or_else(|| !self.master_view_state().is_some_and(|v| v.title));
                self.layout_placeholders(label, &[PlaceholderType::Title], on)
            }
            D::LayoutFooters { on } => {
                let on = on.unwrap_or_else(|| !self.master_view_state().is_some_and(|v| v.footers));
                self.layout_placeholders(label, &FOOTERS, on)
            }
            _ => Ok(Change::None),
        }
    }

    /// Slide Master ▸ Title and Footers: adds the master's placeholders of `kinds` to the layout
    /// shown, or removes the layout's.
    fn layout_placeholders(
        &mut self,
        label: &'static str,
        kinds: &[PlaceholderType],
        on: bool,
    ) -> Result<Change> {
        let layout = self.view_layout(label)?;
        let master = self
            .view_master()
            .ok_or_else(|| KilnError::InvalidInput("no master".into()))?;
        let matches = |s: &Shape, k: PlaceholderType| {
            s.placeholder
                .is_some_and(|p| p.kind == k || (is_title(k) && is_title(p.kind)))
        };
        let shapes = self
            .pres
            .owner_shapes(Owner::Layout(layout))
            .cloned()
            .unwrap_or_default();
        self.end_text();
        if on {
            let mut add = Vec::new();
            for k in kinds {
                if shapes.iter().any(|s| matches(s, *k)) {
                    continue;
                }
                if let Some((ph, rect)) = self.master_placeholder(master, *k) {
                    let id = self.pres.alloc_id();
                    add.push(layout_placeholder(id, ph, rect));
                }
            }
            if add.is_empty() {
                return Ok(Change::None);
            }
            self.apply_with(label, |p| {
                kiln_deck::edit::insert_shapes(p, layout, add, None)
            })?;
        } else {
            let gone: Vec<NodeId> = shapes
                .iter()
                .filter(|s| kinds.iter().any(|k| matches(s, *k)))
                .map(|s| s.id)
                .collect();
            if gone.is_empty() {
                return Ok(Change::None);
            }
            self.apply_with(label, |p| kiln_deck::edit::delete_shapes(p, &gone))?;
            self.view.shapes.retain(|s| !gone.contains(s));
        }
        Ok(Change::Content)
    }

    /// What the Slide Master tab and the view's rail show.
    pub(crate) fn master_view_state(&self) -> Option<DeckMasterView> {
        let current = self.view.master?;
        let version = |id: NodeId| {
            self.global_version
                .wrapping_mul(1_000_003)
                .wrapping_add(id.0.wrapping_mul(7))
                & kiln_scene::HASH_MASK
        };
        let mut items = Vec::new();
        for m in &self.pres.masters {
            items.push(DeckMasterItem {
                id: m.id,
                name: m.name.clone(),
                master: true,
                preserved: m.preserve,
                used: self.slides_on(m.id),
                version: version(m.id),
            });
            for l in &m.layouts {
                items.push(DeckMasterItem {
                    id: l.id,
                    name: l.name.clone(),
                    master: false,
                    preserved: l.preserve,
                    used: self.slides_on(l.id),
                    version: version(l.id),
                });
            }
        }
        let shapes = self.pres.layout(current).map(|(_, l)| &l.shapes);
        let has = |k: PlaceholderType| {
            shapes.is_some_and(|v| {
                v.iter().any(|s| {
                    s.placeholder
                        .is_some_and(|p| p.kind == k || (is_title(k) && is_title(p.kind)))
                })
            })
        };
        Some(DeckMasterView {
            items,
            current,
            title: has(PlaceholderType::Title),
            footers: FOOTERS.iter().all(|k| has(*k)),
        })
    }

    /// The rail's thumbnails of Slide Master view: every master and layout in order.
    pub(crate) fn master_thumbnails(&self, width: f32, dpr: f32) -> kiln_scene::SceneDelta {
        let owners: Vec<(u32, Owner)> = self
            .pres
            .masters
            .iter()
            .flat_map(|m| {
                std::iter::once(Owner::Master(m.id))
                    .chain(m.layouts.iter().map(|l| Owner::Layout(l.id)))
            })
            .enumerate()
            .map(|(i, o)| (u32::try_from(i).unwrap_or(0), o))
            .collect();
        kiln_deck_layout::scene::thumbnails(
            &mut self.renderer.borrow_mut(),
            &self.pres,
            &owners,
            width.clamp(16.0, 800.0),
            dpr,
            self.epoch,
        )
    }
}

#[cfg(test)]
mod tests {
    use kiln_deck::Presentation;
    use kiln_protocol::commands::{DeckDirection, DeckFill};
    use kiln_protocol::{Command, FileFormat};

    use super::super::DeckDoc;
    use super::*;
    use crate::model::DocModel;

    fn run(d: &mut DeckDoc, c: DeckCommand) {
        d.execute(&Command::Deck(c.clone()))
            .unwrap_or_else(|e| panic!("{c:?}: {e}"));
    }

    fn fails(d: &mut DeckDoc, c: DeckCommand) -> String {
        d.execute(&Command::Deck(c))
            .expect_err("refused")
            .to_string()
    }

    fn open(bytes: &[u8]) -> DeckDoc {
        let read = kiln_io_pptx::read_pptx(bytes).expect("open");
        let (pres, original) = super::super::save::Original::from_read(read, bytes);
        DeckDoc::new(pres, Some(original), None)
    }

    fn title_of(shapes: &[Shape]) -> NodeId {
        shapes
            .iter()
            .find(|s| s.placeholder.is_some_and(|p| is_title(p.kind)))
            .map(|s| s.id)
            .expect("a title placeholder")
    }

    #[test]
    fn a_layout_edited_in_master_view_changes_its_slides() {
        let mut d = DeckDoc::new(Presentation::new(), None, None);
        run(&mut d, DeckCommand::NewSlide { layout: None });
        let layout = d.pres.slides[1].layout;
        let slide_title = title_of(&d.pres.slides[1].shapes);
        let before = d.xfrm_of(slide_title).expect("inherited");
        run(&mut d, DeckCommand::MasterView { on: true });
        let st = d.deck_state();
        let view = st.master_view.expect("master view");
        assert_eq!(view.current, layout);
        assert!(view.title);
        assert!(view.items[0].master);
        assert!(
            st.shapes.iter().any(|s| s.name.starts_with("Title")),
            "{:?}",
            st.shapes
        );
        assert!(d.current_slide().is_none());
        // The layout's title is selected, moved down and filled: the slide follows.
        let title = title_of(&d.pres.layout(layout).expect("layout").1.shapes);
        run(&mut d, DeckCommand::SelectShapes { ids: vec![title] });
        run(
            &mut d,
            DeckCommand::Nudge {
                direction: DeckDirection::Down,
                fine: false,
            },
        );
        run(
            &mut d,
            DeckCommand::SetFill {
                fill: DeckFill::Solid {
                    color: "accent2".into(),
                    brightness: None,
                    transparency: None,
                },
            },
        );
        run(&mut d, DeckCommand::MasterView { on: false });
        assert_eq!(d.current_slide(), Some(d.pres.slides[1].id));
        let after = d.xfrm_of(slide_title).expect("inherited");
        assert!(after.y > before.y, "{before:?} → {after:?}");
        let again = open(&d.save(FileFormat::Pptx).expect("save"));
        let t = title_of(&again.pres.slides[1].shapes);
        assert_eq!(again.xfrm_of(t).map(|x| x.y), Some(after.y));
    }

    #[test]
    fn custom_layouts_are_made_saved_and_used_from_the_imported_master() {
        let mut d = DeckDoc::new(Presentation::new(), None, None);
        run(&mut d, DeckCommand::MasterView { on: true });
        let master_layouts = d.pres.masters[0].layouts.len();
        run(&mut d, DeckCommand::InsertLayout);
        let custom = d.view.master.expect("the new layout is shown");
        assert_eq!(d.pres.masters[0].layouts.len(), master_layouts + 1);
        let view = d.master_view_state().expect("view");
        assert!(view.title && view.footers);
        run(
            &mut d,
            DeckCommand::RenameMasterItem {
                name: "Quote".into(),
            },
        );
        run(
            &mut d,
            DeckCommand::InsertPlaceholder {
                kind: DeckPlaceholderKind::Picture,
            },
        );
        run(&mut d, DeckCommand::LayoutFooters { on: None });
        let view = d.master_view_state().expect("view");
        assert!(view.title && !view.footers);
        run(&mut d, DeckCommand::PreserveMaster { on: Some(true) });
        assert!(d.pres.masters[0].preserve);
        run(&mut d, DeckCommand::MasterView { on: false });
        // The file keeps it; opened again, a slide is made on it.
        let mut again = open(&d.save(FileFormat::Pptx).expect("save"));
        let (_, quote) = again
            .pres
            .masters
            .iter()
            .flat_map(|m| m.layouts.iter().map(move |l| (m, l)))
            .find(|(_, l)| l.name == "Quote")
            .expect("the custom layout");
        assert_eq!(quote.kind, LayoutKind::Custom);
        let quote = quote.id;
        run(
            &mut again,
            DeckCommand::NewSlide {
                layout: Some(quote),
            },
        );
        let s = again.pres.slides.last().expect("slide");
        assert_eq!(s.layout, quote);
        let kinds: Vec<PlaceholderType> = s
            .shapes
            .iter()
            .filter_map(|x| x.placeholder.map(|p| p.kind))
            .collect();
        assert!(kinds.contains(&PlaceholderType::Picture), "{kinds:?}");
        assert!(kinds.iter().any(|k| is_title(*k)), "{kinds:?}");
        assert_ne!(custom, quote, "ids are the file's");
    }

    #[test]
    fn masters_are_inserted_and_used_layouts_are_not_deleted() {
        let mut d = DeckDoc::new(Presentation::new(), None, None);
        run(&mut d, DeckCommand::MasterView { on: true });
        // The Title Slide layout carries the first slide.
        assert!(fails(&mut d, DeckCommand::DeleteMasterItem).contains("slide"));
        run(&mut d, DeckCommand::InsertSlideMaster);
        assert_eq!(d.pres.masters.len(), 2);
        assert_eq!(d.view.master, Some(d.pres.masters[1].id));
        assert!(d.pres.masters[1].name.starts_with("1_"));
        // An unused master goes; the last one stays.
        run(&mut d, DeckCommand::DeleteMasterItem);
        assert_eq!(d.pres.masters.len(), 1);
        let m = d.pres.masters[0].id;
        run(&mut d, DeckCommand::SelectMasterItem { id: m });
        assert!(fails(&mut d, DeckCommand::DeleteMasterItem).contains("slide"));
        assert!(
            fails(
                &mut d,
                DeckCommand::InsertPlaceholder {
                    kind: DeckPlaceholderKind::Text
                }
            )
            .contains("layout")
        );
        // An unused layout goes, in one undo step.
        let unused = d.pres.masters[0]
            .layouts
            .last()
            .map(|l| l.id)
            .expect("layouts");
        run(&mut d, DeckCommand::SelectMasterItem { id: unused });
        let n = d.pres.masters[0].layouts.len();
        run(&mut d, DeckCommand::DeleteMasterItem);
        assert_eq!(d.pres.masters[0].layouts.len(), n - 1);
        assert!(d.undo());
        assert_eq!(d.pres.masters[0].layouts.len(), n);
        // Outside the view the commands are refused.
        run(&mut d, DeckCommand::MasterView { on: false });
        assert!(fails(&mut d, DeckCommand::InsertLayout).contains("Slide Master"));
    }

    #[test]
    fn the_masters_title_is_typed_into() {
        let mut d = DeckDoc::new(Presentation::new(), None, None);
        run(&mut d, DeckCommand::MasterView { on: true });
        let m = d.pres.masters[0].id;
        run(&mut d, DeckCommand::SelectMasterItem { id: m });
        let title = title_of(&d.pres.masters[0].shapes);
        run(&mut d, DeckCommand::SelectShapes { ids: vec![title] });
        run(&mut d, DeckCommand::EditText);
        run(&mut d, DeckCommand::SelectAll);
        run(
            &mut d,
            DeckCommand::InsertText {
                text: "Edit the title".into(),
            },
        );
        run(&mut d, DeckCommand::Escape);
        let text = d
            .pres
            .shape(title)
            .and_then(Shape::text)
            .map(kiln_deck::TextBody::text);
        assert_eq!(text.as_deref(), Some("Edit the title"));
        let thumbs = d.master_thumbnails(160.0, 1.0);
        let items = d.master_view_state().expect("view").items.len();
        assert_eq!(thumbs.tiles.len(), items);
    }
}
