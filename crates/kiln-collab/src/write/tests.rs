// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use kiln_doc::{CommentMeta, Pos, ToggleProp, edit};
use std::path::PathBuf;

fn fixtures(dir: &str) -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(dir);
    let mut out: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    out.retain(|p| p.extension().is_some_and(|e| e == "docx"));
    out.sort();
    out
}

fn first_para(doc: &Document) -> NodeId {
    doc.body_paragraphs()[0].id
}

fn edit_with(doc: &mut Document, b: &mut WriteBinding, tx: Transaction) -> Result<()> {
    doc.apply(tx.clone())?;
    b.commit(doc, &tx)
}

fn send(from: &WriteBinding, to: &mut WriteBinding, doc: &mut Document) -> Result<WriteChange> {
    let update = from.crdt().export_from(&to.crdt().version())?;
    to.apply_remote(doc, &update)
}

fn pair() -> Result<(WriteBinding, Document, WriteBinding, Document)> {
    let mut a_doc = Document::new();
    let a = WriteBinding::share(&mut a_doc, 1, IdLanes::new(1, 8))?;
    let (b, b_doc) = WriteBinding::join(&a.crdt().snapshot()?, 2, IdLanes::new(2, 8))?;
    Ok((a, a_doc, b, b_doc))
}

#[test]
fn every_fixture_round_trips_through_the_crdt() -> Result<()> {
    let mut files = fixtures("docx/features");
    files.extend(fixtures("docx/manual"));
    assert!(files.len() > 15, "fixtures found: {}", files.len());
    let mut read_ok = 0;
    for f in &files {
        let bytes = std::fs::read(f).map_err(|e| corrupt(e.to_string()))?;
        let Ok(read) = kiln_io_docx::read_docx(&bytes) else {
            continue;
        };
        read_ok += 1;
        let mut doc = read.document;
        let b = WriteBinding::share(&mut doc, 7, IdLanes::default())?;
        let back = b.decode()?;
        assert!(back == doc, "{} changed through the CRDT", f.display());
        let (_, joined) = WriteBinding::join(&b.crdt().snapshot()?, 8, IdLanes::default())?;
        assert!(joined == doc, "{} changed for a joiner", f.display());
    }
    assert_eq!(read_ok, files.len());
    Ok(())
}

#[test]
fn concurrent_typing_in_one_paragraph_converges() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "Hello world")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let change = send(&a, &mut b, &mut b_doc)?;
    assert!(!change.reloaded);
    assert!(matches!(
        change.tx.ops.as_slice(),
        [Op::InsertInline { .. }]
    ));
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 5), ", dear")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::toggle(&b_doc, Pos::new(p, 6), Pos::new(p, 11), ToggleProp::Bold)?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let tx = edit::insert_text(&mut b_doc, Pos::new(p, 11), "!")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    send(&b, &mut a, &mut a_doc)?;
    assert_eq!(a_doc, b_doc);
    assert_eq!(a_doc.body_paragraphs()[0].text(), "Hello, dear world!");
    assert_eq!(a.decode()?, a_doc);
    Ok(())
}

#[test]
fn paragraphs_tables_and_comments_converge() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "One two three")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 4))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::insert_table(&mut b_doc, Pos::new(p, 13), 2, 2)?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let meta = CommentMeta {
        author: "B".into(),
        ..CommentMeta::default()
    };
    let (tx, _) = edit::add_comment(&mut b_doc, Pos::new(p, 0), Pos::new(p, 3), meta, "Why?")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    assert!(!send(&a, &mut b, &mut b_doc)?.reloaded);
    assert!(!send(&b, &mut a, &mut a_doc)?.reloaded);
    assert_eq!(a_doc, b_doc);
    assert_eq!(a.decode()?, a_doc);
    assert_eq!(a_doc.comments.len(), 1);
    a_doc.validate()?;
    Ok(())
}

#[test]
fn undo_reverts_only_local_edits() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "mine ")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let tx = edit::insert_text(&mut b_doc, Pos::new(p, 5), "theirs")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    send(&b, &mut a, &mut a_doc)?;
    assert_eq!(a_doc.body_paragraphs()[0].text(), "mine theirs");
    let change = a
        .undo(&mut a_doc)?
        .ok_or_else(|| corrupt("nothing to undo"))?;
    assert!(!change.reloaded);
    assert_eq!(a_doc.body_paragraphs()[0].text(), "theirs");
    assert!(
        change.tx.selection_after.is_some(),
        "undo restores the selection"
    );
    assert!(
        a.undo(&mut a_doc)?.is_none(),
        "B's typing is not A's to undo"
    );
    a.redo(&mut a_doc)?;
    assert_eq!(a_doc.body_paragraphs()[0].text(), "mine theirs");
    send(&a, &mut b, &mut b_doc)?;
    assert_eq!(a_doc, b_doc);
    Ok(())
}

#[test]
fn list_edits_keep_the_longest_ordered_run() {
    let ids = |v: &[u64]| v.iter().map(|i| NodeId(*i)).collect::<Vec<_>>();
    let e = list_edits(&ids(&[1, 2, 3, 4]), &ids(&[1, 3, 5, 4]));
    assert_eq!(e.removals, vec![1]);
    assert_eq!(e.inserts, vec![(2, NodeId(5))]);
    let e = list_edits(&ids(&[1, 2]), &ids(&[3]));
    assert_eq!(e.kept, 0);
    assert_eq!(e.split_removals(), (&[0][..], true));
}

#[test]
fn lists_emptied_by_concurrent_deletes_are_repaired() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 0))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let section = a_doc.sections[0].id;
    let remove = |i| {
        Transaction::new(vec![Op::RemoveBlocks {
            container: section,
            index: i,
            count: 1,
        }])
    };
    edit_with(&mut a_doc, &mut a, remove(0))?;
    edit_with(&mut b_doc, &mut b, remove(1))?;
    send(&a, &mut b, &mut b_doc)?;
    send(&b, &mut a, &mut a_doc)?;
    send(&a, &mut b, &mut b_doc)?;
    a_doc.validate()?;
    b_doc.validate()?;
    assert_eq!(a_doc, b_doc);
    assert_eq!(a.decode()?, a_doc);
    assert_eq!(b.decode()?, b_doc);
    assert!(!a_doc.sections[0].blocks.is_empty());
    Ok(())
}

#[test]
fn styles_and_settings_merge_field_by_field() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let Some(mut normal) = a_doc.styles.iter().next().cloned() else {
        return Ok(());
    };
    normal.name = "Renamed".into();
    let tx = Transaction::new(vec![Op::PutStyle {
        style: Box::new(normal),
        index: 0,
    }]);
    edit_with(&mut a_doc, &mut a, tx)?;
    let mut settings = b_doc.settings.clone();
    settings.default_tab_stop += 10;
    let tx = Transaction::new(vec![Op::SetSettings {
        settings: Box::new(settings),
    }]);
    edit_with(&mut b_doc, &mut b, tx)?;
    let mut props = a_doc.properties.clone();
    props.title = Some("Shared".into());
    let tx = Transaction::new(vec![Op::SetProperties {
        properties: Box::new(props),
    }]);
    edit_with(&mut a_doc, &mut a, tx)?;
    assert!(!send(&a, &mut b, &mut b_doc)?.reloaded);
    assert!(!send(&b, &mut a, &mut a_doc)?.reloaded);
    assert_eq!(a_doc, b_doc);
    assert_eq!(a_doc.properties.title.as_deref(), Some("Shared"));
    assert_eq!(
        a_doc.settings.default_tab_stop,
        Document::new().settings.default_tab_stop + 10
    );
    Ok(())
}

#[test]
fn anchors_follow_other_peers_edits() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "world")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    // B's caret before "d"; A types in front of it.
    let caret = b
        .anchor(Pos::new(p, 4))
        .ok_or_else(|| corrupt("no anchor"))?;
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "hello ")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&b, &mut a, &mut a_doc)?;
    assert_eq!(a.resolve(&a_doc, &caret), Some(Pos::new(p, 10)));
    Ok(())
}

#[test]
fn text_beyond_ascii_merges_by_character() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "naïve 😀 café")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    assert!(!send(&a, &mut b, &mut b_doc)?.reloaded);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 8), "🎉")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::toggle(&b_doc, Pos::new(p, 6), Pos::new(p, 12), ToggleProp::Italic)?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let tx = edit::delete_range(&b_doc, Pos::new(p, 2), Pos::new(p, 3))?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let to_b = send(&a, &mut b, &mut b_doc)?;
    let to_a = send(&b, &mut a, &mut a_doc)?;
    assert!(!to_a.reloaded && !to_b.reloaded);
    assert_eq!(a_doc, b_doc);
    assert_eq!(a.decode()?, a_doc);
    assert_eq!(a_doc.body_paragraphs()[0].text(), "nave 😀 🎉café");
    Ok(())
}

fn texts(doc: &Document) -> Vec<String> {
    doc.body_paragraphs().iter().map(|p| p.text()).collect()
}

/// Exchanges updates until neither side has anything the other lacks.
fn settle(
    a: &mut WriteBinding,
    a_doc: &mut Document,
    b: &mut WriteBinding,
    b_doc: &mut Document,
) -> Result<()> {
    for _ in 0..4 {
        send(a, b, b_doc)?;
        send(b, a, a_doc)?;
    }
    Ok(())
}

#[test]
fn text_typed_into_a_tail_being_split_off_follows_it() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "Hello world")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    // B types at the end and inside the tail while A splits the paragraph before it.
    let tx = edit::insert_text(&mut b_doc, Pos::new(p, 11), "!")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let tx = edit::insert_text(&mut b_doc, Pos::new(p, 8), "-")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let tx = edit::toggle(&b_doc, Pos::new(p, 8), Pos::new(p, 9), ToggleProp::Bold)?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 6))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    assert_eq!(texts(&a_doc), ["Hello ", "wo-rld!"]);
    assert_eq!(a_doc, b_doc);
    assert_eq!(a.decode()?, a_doc);
    assert_eq!(b.decode()?, b_doc);
    let moved = &a_doc.body_paragraphs()[1];
    assert_eq!(
        moved.content.props_at(2).and_then(|r| r.bold),
        Some(true),
        "the moved text keeps its formatting"
    );
    Ok(())
}

#[test]
fn text_typed_at_the_split_point_stays_before_it() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "Hello world")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let tx = edit::insert_text(&mut b_doc, Pos::new(p, 6), "big ")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 6))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    assert_eq!(texts(&a_doc), ["Hello big ", "world"]);
    assert_eq!(a_doc, b_doc);
    Ok(())
}

#[test]
fn text_typed_into_a_paragraph_being_joined_follows_it() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "Helloworld")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 5))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let q = a_doc.body_paragraphs()[1].id;
    let tx = edit::insert_text(&mut b_doc, Pos::new(q, 5), "!")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let tx = edit::insert_text(&mut b_doc, Pos::new(q, 2), "-")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let tx = edit::delete_range(&a_doc, Pos::new(p, 5), Pos::new(q, 0))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    assert_eq!(texts(&a_doc), ["Hellowo-rld!"]);
    assert_eq!(a_doc, b_doc);
    assert_eq!(a.decode()?, a_doc);
    Ok(())
}

#[test]
fn two_splits_of_one_paragraph_keep_the_tail_once_and_in_order() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "one two three")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 4))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::split_paragraph(&mut b_doc, Pos::new(p, 8))?;
    edit_with(&mut b_doc, &mut b, tx)?;
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    assert_eq!(texts(&a_doc), ["one ", "two ", "three"]);
    assert_eq!(a_doc, b_doc);
    assert_eq!(a.decode()?, a_doc);
    Ok(())
}

#[test]
fn two_joins_of_the_same_paragraphs_keep_the_text_once() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "first second")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 6))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let q = a_doc.body_paragraphs()[1].id;
    // Both press Backspace at the start of the second paragraph.
    let tx = edit::delete_range(&a_doc, Pos::new(p, 6), Pos::new(q, 0))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::delete_range(&b_doc, Pos::new(p, 6), Pos::new(q, 0))?;
    edit_with(&mut b_doc, &mut b, tx)?;
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    assert_eq!(texts(&a_doc), ["first second"]);
    assert_eq!(a_doc, b_doc);
    assert_eq!(a.decode()?, a_doc);
    Ok(())
}

#[test]
fn text_deleted_from_a_tail_being_split_off_stays_deleted() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "Hello cruel world")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let tx = edit::delete_range(&b_doc, Pos::new(p, 6), Pos::new(p, 12))?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 6))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    assert_eq!(texts(&a_doc), ["Hello ", "world"]);
    assert_eq!(a_doc, b_doc);
    Ok(())
}

#[test]
fn text_typed_by_someone_who_left_before_the_split_reached_them_follows_it() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let (mut c, mut c_doc) = WriteBinding::join(&a.crdt().snapshot()?, 3, IdLanes::new(3, 8))?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "Helloworld")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 5))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    send(&a, &mut c, &mut c_doc)?;
    let q = a_doc.body_paragraphs()[1].id;
    // C types into the second paragraph, sends it and leaves for good; meanwhile A joins the
    // paragraphs, and B (which never typed there) is the only other one left.
    let tx = edit::insert_text(&mut c_doc, Pos::new(q, 2), "-")?;
    edit_with(&mut c_doc, &mut c, tx)?;
    let tx = edit::split_paragraph(&mut c_doc, Pos::new(p, 2))?;
    edit_with(&mut c_doc, &mut c, tx)?;
    let tx = edit::delete_range(&a_doc, Pos::new(p, 5), Pos::new(q, 0))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&c, &mut b, &mut b_doc)?;
    send(&c, &mut a, &mut a_doc)?;
    drop((c, c_doc));
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    let all: String = texts(&a_doc).concat();
    assert_eq!(all.matches('-').count(), 1, "{:?}", texts(&a_doc));
    assert_eq!(
        all.chars()
            .filter(|c| c.is_alphabetic())
            .collect::<String>(),
        "Helloworld"
    );
    assert_eq!(a_doc, b_doc);
    Ok(())
}

#[test]
fn a_list_two_people_empty_at_once_keeps_one_empty_paragraph() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "one two")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 4))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let section = a_doc.sections[0].id;
    let remove = |i| {
        Transaction::new(vec![Op::RemoveBlocks {
            container: section,
            index: i,
            count: 1,
        }])
    };
    edit_with(&mut a_doc, &mut a, remove(0))?;
    edit_with(&mut b_doc, &mut b, remove(1))?;
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    assert_eq!(texts(&a_doc), [""], "{:?}", texts(&b_doc));
    assert_eq!(a_doc, b_doc);
    assert_eq!(a.decode()?, a_doc);
    Ok(())
}

#[test]
fn text_typed_into_two_repairs_of_one_emptied_list_is_kept() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "one two")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 4))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let section = a_doc.sections[0].id;
    let remove = |i| {
        Transaction::new(vec![Op::RemoveBlocks {
            container: section,
            index: i,
            count: 1,
        }])
    };
    edit_with(&mut a_doc, &mut a, remove(0))?;
    edit_with(&mut b_doc, &mut b, remove(1))?;
    // Each gets the other's deletion only, repairs the emptied list and types into it.
    let to_a = b.crdt().export_from(&a.crdt().version())?;
    let to_b = a.crdt().export_from(&b.crdt().version())?;
    a.apply_remote(&mut a_doc, &to_a)?;
    b.apply_remote(&mut b_doc, &to_b)?;
    for (doc, binding, text) in [
        (&mut a_doc, &mut a, "from A"),
        (&mut b_doc, &mut b, "from B"),
    ] {
        let q = first_para(doc);
        let tx = edit::insert_text(doc, Pos::new(q, 0), text)?;
        edit_with(doc, binding, tx)?;
    }
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    let all = texts(&a_doc).concat();
    assert_eq!(texts(&a_doc).len(), 1, "{:?}", texts(&a_doc));
    assert!(all.contains("from A") && all.contains("from B"), "{all:?}");
    assert_eq!(all.len(), "from Afrom B".len());
    assert_eq!(a_doc, b_doc);
    assert_eq!(a.decode()?, a_doc);
    Ok(())
}

#[test]
fn undoing_a_split_someone_typed_after_never_removes_their_text() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "Hello world")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    a.break_undo_group();
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 6))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let q = b_doc.body_paragraphs()[1].id;
    let tx = edit::insert_text(&mut b_doc, Pos::new(q, 5), " and more")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    send(&b, &mut a, &mut a_doc)?;
    // Undoing the split would remove the paragraph B typed into: it is refused.
    assert!(a.undo(&mut a_doc).is_err());
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    assert!(
        texts(&a_doc).concat().contains("and more"),
        "{:?}",
        texts(&a_doc)
    );
    assert_eq!(a_doc, b_doc);
    assert!(
        !a.crdt().can_undo(),
        "the steps before it go too, as in Word"
    );
    Ok(())
}

#[test]
fn undoing_a_split_nobody_typed_after_works() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "Hello world")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    a.break_undo_group();
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 6))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let tx = edit::insert_text(&mut b_doc, Pos::new(p, 0), "Oh, ")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    send(&b, &mut a, &mut a_doc)?;
    assert!(a.undo(&mut a_doc)?.is_some());
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    assert_eq!(texts(&a_doc), ["Oh, Hello world"]);
    assert_eq!(a_doc, b_doc);
    Ok(())
}

#[test]
fn undoing_a_join_someone_typed_after_keeps_their_text() -> Result<()> {
    let (mut a, mut a_doc, mut b, mut b_doc) = pair()?;
    let p = first_para(&a_doc);
    let tx = edit::insert_text(&mut a_doc, Pos::new(p, 0), "Helloworld")?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let tx = edit::split_paragraph(&mut a_doc, Pos::new(p, 5))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    let q = a_doc.body_paragraphs()[1].id;
    a.break_undo_group();
    let tx = edit::delete_range(&a_doc, Pos::new(p, 5), Pos::new(q, 0))?;
    edit_with(&mut a_doc, &mut a, tx)?;
    send(&a, &mut b, &mut b_doc)?;
    let tx = edit::insert_text(&mut b_doc, Pos::new(p, 10), "!")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    let tx = edit::insert_text(&mut b_doc, Pos::new(p, 7), "-")?;
    edit_with(&mut b_doc, &mut b, tx)?;
    send(&b, &mut a, &mut a_doc)?;
    let _ = a.undo(&mut a_doc);
    settle(&mut a, &mut a_doc, &mut b, &mut b_doc)?;
    let all = texts(&a_doc).concat();
    assert_eq!(
        all.matches('-').count() + all.matches('!').count(),
        2,
        "{:?}",
        texts(&a_doc)
    );
    assert_eq!(a_doc, b_doc);
    Ok(())
}
