// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use kiln_deck::{Rect, edit};
use kiln_doc::Pos;
use std::path::PathBuf;

fn fixtures() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pptx/features");
    let mut out: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    out.retain(|p| p.extension().is_some_and(|e| e == "pptx"));
    out.sort();
    out
}

fn edit_with(pres: &mut Presentation, b: &mut DeckBinding, tx: Transaction) -> Result<()> {
    pres.apply(tx.clone())?;
    b.commit(pres, &tx)
}

fn send(from: &DeckBinding, to: &mut DeckBinding, pres: &mut Presentation) -> Result<DeckChange> {
    let update = from.crdt().export_from(&to.crdt().version())?;
    to.apply_remote(pres, &update)
}

fn pair() -> Result<(DeckBinding, Presentation, DeckBinding, Presentation)> {
    let mut a_pres = Presentation::new();
    let a = DeckBinding::share(&mut a_pres, 1, IdLanes::new(1, 8))?;
    let (b, b_pres) = DeckBinding::join(&a.crdt().snapshot()?, 2, IdLanes::new(2, 8))?;
    Ok((a, a_pres, b, b_pres))
}

fn text_box(pres: &mut Presentation, text: &str) -> Shape {
    let id = pres.alloc_id();
    Shape::text_box(
        id,
        Rect {
            x: 100,
            y: 100,
            cx: 2_000_000,
            cy: 500_000,
        },
        text,
    )
}

#[test]
fn every_fixture_round_trips_through_the_crdt() -> Result<()> {
    let files = fixtures();
    assert!(files.len() > 15);
    for f in &files {
        let bytes = std::fs::read(f).map_err(|e| corrupt(e.to_string()))?;
        let mut pres = kiln_io_pptx::read_pptx(&bytes)
            .map_err(|e| corrupt(e.to_string()))?
            .presentation;
        let b = DeckBinding::share(&mut pres, 7, IdLanes::default())?;
        assert!(
            b.decode()? == pres,
            "{} changed through the CRDT",
            f.display()
        );
        let (_, joined) = DeckBinding::join(&b.crdt().snapshot()?, 8, IdLanes::default())?;
        assert!(joined == pres, "{} changed for a joiner", f.display());
    }
    Ok(())
}

#[test]
fn concurrent_slide_and_text_edits_converge() -> Result<()> {
    let (mut a, mut a_pres, mut b, mut b_pres) = pair()?;
    let first = a_pres.slides[0].id;
    let shape = text_box(&mut a_pres, "Hello");
    let id = shape.id;
    let tx = edit::insert_shapes(&mut a_pres, first, vec![shape], None)?;
    edit_with(&mut a_pres, &mut a, tx)?;
    let change = send(&a, &mut b, &mut b_pres)?;
    assert!(!change.reloaded);
    assert!(matches!(
        change.tx.ops.as_slice(),
        [Op::InsertShapes { .. }]
    ));
    // A types into the box and adds a slide; B moves the box and types too.
    let para = a_pres.shape(id).and_then(|s| match &s.kind {
        ShapeKind::Geometry(g) => g
            .text
            .as_ref()
            .and_then(|t| t.paragraphs().next().map(|p| p.id)),
        _ => None,
    });
    let para = para.ok_or_else(|| corrupt("no paragraph"))?;
    let tx = edit::insert_text(&a_pres, TextTarget::Shape(id), Pos::new(para, 5), " world")?;
    edit_with(&mut a_pres, &mut a, tx)?;
    let layout = a_pres.slides[0].layout;
    let tx = edit::insert_slide(&mut a_pres, layout, 1)?;
    edit_with(&mut a_pres, &mut a, tx)?;
    let tx = edit::move_shapes(&mut b_pres, &[id], 1000, 0)?;
    edit_with(&mut b_pres, &mut b, tx)?;
    let tx = edit::insert_text(&b_pres, TextTarget::Shape(id), Pos::new(para, 0), "Oh, ")?;
    edit_with(&mut b_pres, &mut b, tx)?;
    let to_b = send(&a, &mut b, &mut b_pres)?;
    let to_a = send(&b, &mut a, &mut a_pres)?;
    assert!(!to_a.reloaded && !to_b.reloaded);
    assert!(to_b.tx.ops.iter().any(|o| matches!(o, Op::EditText { .. })));
    assert!(a_pres == b_pres);
    assert!(a.decode()? == a_pres);
    let text = a_pres.shape(id).and_then(|s| match &s.kind {
        ShapeKind::Geometry(g) => g.text.as_ref().map(TextBody::text),
        _ => None,
    });
    assert_eq!(text.as_deref(), Some("Oh, Hello world"));
    assert_eq!(a_pres.slides.len(), 2);
    a_pres.validate()?;
    Ok(())
}

#[test]
fn sections_and_slide_moves_converge() -> Result<()> {
    let (mut a, mut a_pres, mut b, mut b_pres) = pair()?;
    let layout = a_pres.slides[0].layout;
    for i in 1..4 {
        let tx = edit::insert_slide(&mut a_pres, layout, i)?;
        edit_with(&mut a_pres, &mut a, tx)?;
    }
    send(&a, &mut b, &mut b_pres)?;
    let tx = edit::add_section(&mut a_pres, 0, "Intro")?;
    edit_with(&mut a_pres, &mut a, tx)?;
    let tx = edit::add_section(&mut a_pres, 2, "Body")?;
    edit_with(&mut a_pres, &mut a, tx)?;
    let ids: Vec<NodeId> = b_pres.slides.iter().map(|s| s.id).collect();
    let tx = edit::move_slides(&mut b_pres, &[ids[3]], 0)?;
    edit_with(&mut b_pres, &mut b, tx)?;
    send(&a, &mut b, &mut b_pres)?;
    send(&b, &mut a, &mut a_pres)?;
    assert!(a_pres == b_pres);
    assert!(a.decode()? == a_pres);
    a_pres.validate()?;
    assert_eq!(a_pres.sections.len(), 2);
    Ok(())
}

#[test]
fn undo_reverts_only_local_edits() -> Result<()> {
    let (mut a, mut a_pres, mut b, mut b_pres) = pair()?;
    let first = a_pres.slides[0].id;
    let mine = text_box(&mut a_pres, "mine");
    let tx = edit::insert_shapes(&mut a_pres, first, vec![mine], None)?;
    edit_with(&mut a_pres, &mut a, tx)?;
    send(&a, &mut b, &mut b_pres)?;
    let theirs = text_box(&mut b_pres, "theirs");
    let tx = edit::insert_shapes(&mut b_pres, first, vec![theirs], None)?;
    edit_with(&mut b_pres, &mut b, tx)?;
    send(&b, &mut a, &mut a_pres)?;
    let before = a_pres.slides[0].shapes.len();
    let change = a
        .undo(&mut a_pres)?
        .ok_or_else(|| corrupt("nothing to undo"))?;
    assert!(!change.reloaded);
    assert_eq!(a_pres.slides[0].shapes.len(), before - 1);
    assert!(
        a_pres.slides[0]
            .shapes
            .iter()
            .any(|s| TextBody::text(text(s)) == "theirs")
    );
    assert!(a.undo(&mut a_pres)?.is_none());
    send(&a, &mut b, &mut b_pres)?;
    assert!(a_pres == b_pres);
    Ok(())
}

fn text(s: &Shape) -> &TextBody {
    match &s.kind {
        ShapeKind::Geometry(g) => g
            .text
            .as_ref()
            .unwrap_or_else(|| unreachable!("a text box")),
        _ => unreachable!("a text box"),
    }
}

fn paragraphs(pres: &Presentation, id: NodeId) -> Vec<String> {
    pres.shape(id)
        .and_then(|s| match &s.kind {
            ShapeKind::Geometry(g) => g.text.as_ref(),
            _ => None,
        })
        .map(|t| t.paragraphs().map(kiln_doc::Paragraph::text).collect())
        .unwrap_or_default()
}

fn settle(
    a: &mut DeckBinding,
    a_pres: &mut Presentation,
    b: &mut DeckBinding,
    b_pres: &mut Presentation,
) -> Result<()> {
    for _ in 0..4 {
        send(a, b, b_pres)?;
        send(b, a, a_pres)?;
    }
    Ok(())
}

#[test]
fn text_typed_into_a_split_or_joined_paragraph_follows_it() -> Result<()> {
    let (mut a, mut a_pres, mut b, mut b_pres) = pair()?;
    let first = a_pres.slides[0].id;
    let shape = text_box(&mut a_pres, "Hello world");
    let id = shape.id;
    let tx = edit::insert_shapes(&mut a_pres, first, vec![shape], None)?;
    edit_with(&mut a_pres, &mut a, tx)?;
    send(&a, &mut b, &mut b_pres)?;
    let target = TextTarget::Shape(id);
    let para = |pres: &Presentation, i: usize| {
        pres.shape(id)
            .and_then(|s| match &s.kind {
                ShapeKind::Geometry(g) => g.text.as_ref().and_then(|t| t.paragraphs().nth(i)),
                _ => None,
            })
            .map(|p| p.id)
            .ok_or_else(|| corrupt("no paragraph"))
    };
    // B types at the end of the box while A splits its paragraph.
    let p = para(&b_pres, 0)?;
    let tx = edit::insert_text(&b_pres, target, Pos::new(p, 11), "!")?;
    edit_with(&mut b_pres, &mut b, tx)?;
    let tx = edit::edit_text(&a_pres, target, |d| {
        kiln_doc::edit::split_paragraph(d, Pos::new(p, 6))
    })?;
    edit_with(&mut a_pres, &mut a, tx)?;
    settle(&mut a, &mut a_pres, &mut b, &mut b_pres)?;
    assert_eq!(paragraphs(&a_pres, id), ["Hello ", "world!"]);
    assert!(a_pres == b_pres);
    // B types into the second paragraph while A joins them again.
    let q = para(&b_pres, 1)?;
    let tx = edit::insert_text(&b_pres, target, Pos::new(q, 2), "-")?;
    edit_with(&mut b_pres, &mut b, tx)?;
    let tx = edit::edit_text(&a_pres, target, |d| {
        kiln_doc::edit::delete_range(d, Pos::new(p, 6), Pos::new(q, 0))
    })?;
    edit_with(&mut a_pres, &mut a, tx)?;
    settle(&mut a, &mut a_pres, &mut b, &mut b_pres)?;
    assert_eq!(paragraphs(&a_pres, id), ["Hello wo-rld!"]);
    assert!(a_pres == b_pres);
    assert!(a.decode()? == a_pres);
    a_pres.validate()?;
    Ok(())
}

#[test]
fn undo_never_removes_text_someone_else_typed() -> Result<()> {
    let (mut a, mut a_pres, mut b, mut b_pres) = pair()?;
    let first = a_pres.slides[0].id;
    // A adds a box; B types into it; undoing the box would take B's text: refused.
    let shape = text_box(&mut a_pres, "Hello world");
    let id = shape.id;
    let tx = edit::insert_shapes(&mut a_pres, first, vec![shape], None)?;
    edit_with(&mut a_pres, &mut a, tx)?;
    send(&a, &mut b, &mut b_pres)?;
    let target = TextTarget::Shape(id);
    let p = text(b_pres.shape(id).ok_or_else(|| corrupt("no box"))?)
        .paragraphs()
        .next()
        .map(|p| p.id)
        .ok_or_else(|| corrupt("no paragraph"))?;
    let tx = edit::insert_text(&b_pres, target, Pos::new(p, 11), "!")?;
    edit_with(&mut b_pres, &mut b, tx)?;
    send(&b, &mut a, &mut a_pres)?;
    assert!(a.undo(&mut a_pres).is_err());
    settle(&mut a, &mut a_pres, &mut b, &mut b_pres)?;
    assert_eq!(paragraphs(&a_pres, id), ["Hello world!"]);
    // A splits the box's paragraph; B types into the new one; undoing the split is refused.
    let tx = edit::edit_text(&a_pres, target, |d| {
        kiln_doc::edit::split_paragraph(d, Pos::new(p, 6))
    })?;
    edit_with(&mut a_pres, &mut a, tx)?;
    send(&a, &mut b, &mut b_pres)?;
    let q = text(b_pres.shape(id).ok_or_else(|| corrupt("no box"))?)
        .paragraphs()
        .nth(1)
        .map(|p| p.id)
        .ok_or_else(|| corrupt("no second paragraph"))?;
    let tx = edit::insert_text(&b_pres, target, Pos::new(q, 0), "big ")?;
    edit_with(&mut b_pres, &mut b, tx)?;
    send(&b, &mut a, &mut a_pres)?;
    assert!(a.undo(&mut a_pres).is_err());
    settle(&mut a, &mut a_pres, &mut b, &mut b_pres)?;
    assert_eq!(paragraphs(&a_pres, id), ["Hello ", "big world!"]);
    assert!(a_pres == b_pres);
    Ok(())
}
