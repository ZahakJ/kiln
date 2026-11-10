// SPDX-License-Identifier: AGPL-3.0-or-later
//! Hostile numbers: section lengths that overflow, transforms at the edge of `i64`, broken
//! tables and extreme timings, in loaded files and in ops. Loading refuses them, ops refuse
//! them atomically, and nothing panics; whatever an op accepts can be saved and reopened.

use kiln_common::NodeId;
use proptest::prelude::*;

use super::*;
use crate::edit::tests::sample;
use crate::edit::{self, Align, AlignTo, ScaleMode};
use crate::geom::{ChildSpace, MAX_COORD, Rect, Transform};
use crate::presentation::Presentation;
use crate::shape::{MAX_TABLE_COLS, MAX_TABLE_ROWS, Shape, ShapeKind, Table, TableCell, TableRow};
use crate::slide::{Section, SlideSize, SlideSizeKind};

/// The slide with the most shapes in the sample (slide 3: every shape kind).
fn busy_slide(p: &Presentation) -> NodeId {
    p.slides
        .iter()
        .max_by_key(|s| s.shapes.len())
        .expect("slides")
        .id
}

fn reopen(p: &Presentation) -> Result<Presentation> {
    Presentation::from_bytes(&p.to_bytes()?)
}

fn sections(p: &mut Presentation, lens: &[usize]) -> Vec<Section> {
    lens.iter()
        .map(|len| Section {
            id: p.alloc_id(),
            name: "S".into(),
            len: *len,
        })
        .collect()
}

#[test]
fn overflowing_section_lengths_are_refused() {
    let mut p = sample();
    // Lengths that wrap to the slide count in release builds.
    let n = p.slides.len();
    let wrap = sections(&mut p, &[usize::MAX, n + 1]);
    let mut bad = p.clone();
    bad.sections = wrap.clone();
    assert!(matches!(reopen(&bad), Err(DeckError::Corrupt(_))));
    assert!(bad.validate().is_err());
    // A single length longer than the deck.
    bad.sections = sections(&mut p, &[usize::MAX]);
    assert!(matches!(reopen(&bad), Err(DeckError::Corrupt(_))));
    // The same through a transaction (the journal or control channel): refused, unchanged.
    let before = p.clone();
    let tx = Transaction::new(vec![Op::SetSections { sections: wrap }]);
    let tx = Transaction::from_bytes(&tx.to_bytes().unwrap()).unwrap();
    assert!(matches!(p.apply(tx), Err(DeckError::InvalidOp(_))));
    assert_eq!(p, before);
    // Lookups stay total even on a model nobody validated.
    assert_eq!(bad.section_of_index(usize::MAX - 1), Some(0));
    assert_eq!(bad.section_start(5), usize::MAX);
}

#[test]
fn out_of_range_transforms_are_refused() {
    let mut p = sample();
    let slide = busy_slide(&p);
    let far = Shape::text_box(p.alloc_id(), Rect::new(0, 0, 100, 100), "far");
    let near = Shape::text_box(p.alloc_id(), Rect::new(0, 0, 100, 100), "near");
    let (far_id, near_id) = (far.id, near.id);
    let tx = edit::insert_shapes(&mut p, slide, vec![far, near], None).unwrap();
    p.apply(tx).unwrap();
    // A saved file with the extreme transforms is refused on load.
    let mut bad = p.clone();
    bad.shape_mut(far_id).unwrap().xfrm = Some(Transform::new(i64::MAX - 10, 0, i64::MAX, 100));
    bad.shape_mut(near_id).unwrap().xfrm = Some(Transform::new(i64::MIN + 10, 0, 10, 100));
    assert!(matches!(reopen(&bad), Err(DeckError::Corrupt(_))));
    // Align, distribute and group do not panic on it either (a caller that skipped
    // validation gets garbage-free, clamped moves or an error, never an overflow).
    for how in [
        Align::Left,
        Align::Center,
        Align::Right,
        Align::Top,
        Align::Middle,
        Align::Bottom,
    ] {
        let _ = edit::align(&mut bad, &[far_id, near_id], how, AlignTo::Selection);
    }
    let _ = edit::distribute(&mut bad, &[far_id, near_id], true, AlignTo::Slide);
    let _ = edit::group(&mut bad, &[far_id, near_id]);
    // Ops refuse them and leave the presentation as it was.
    for xfrm in [
        Transform::new(i64::MAX - 10, 0, i64::MAX, 100),
        Transform::new(MAX_COORD + 1, 0, 1, 1),
        Transform::new(0, -MAX_COORD - 1, 1, 1),
        Transform::new(0, 0, MAX_COORD + 1, 1),
        Transform {
            rot: -1,
            ..Transform::new(0, 0, 1, 1)
        },
    ] {
        let tx = Transaction::new(vec![Op::SetTransform {
            shape: far_id,
            xfrm: Some(xfrm),
        }]);
        assert!(
            matches!(p.apply(tx), Err(DeckError::InvalidOp(_))),
            "{xfrm:?}"
        );
        let s = Shape::text_box(p.alloc_id(), Rect::new(0, 0, 1, 1), "x");
        let tx = Transaction::new(vec![Op::InsertShapes {
            parent: slide,
            index: 0,
            shapes: vec![Shape {
                xfrm: Some(xfrm),
                ..s
            }],
        }]);
        assert!(matches!(p.apply(tx), Err(DeckError::InvalidOp(_))));
    }
    // The edges of the range are fine, and align across them stays in range.
    let tx = Transaction::new(vec![
        Op::SetTransform {
            shape: far_id,
            xfrm: Some(Transform::new(MAX_COORD - 10, 0, 10, 100)),
        },
        Op::SetTransform {
            shape: near_id,
            xfrm: Some(Transform::new(-MAX_COORD, 0, MAX_COORD, 100)),
        },
    ]);
    p.apply(tx).unwrap();
    for how in [Align::Left, Align::Center, Align::Right] {
        let tx = edit::align(&mut p, &[far_id, near_id], how, AlignTo::Selection).unwrap();
        let inv = p.apply(tx).unwrap();
        reopen(&p).unwrap();
        p.apply(inv).unwrap();
    }
    let tx = edit::move_shapes(&mut p, &[far_id], i64::MAX, i64::MIN).unwrap();
    p.apply(tx).unwrap();
    let x = p.shape(far_id).unwrap().xfrm.unwrap();
    assert_eq!(
        (x.x, x.y),
        (MAX_COORD, -MAX_COORD),
        "nudges stop at the edge"
    );
    reopen(&p).unwrap();
    // Group child spaces are checked too.
    let group = p.slides[4].shapes.last().unwrap().id;
    let bad_space = ChildSpace {
        x: 0,
        y: 0,
        cx: -1,
        cy: 1,
    };
    let tx = Transaction::new(vec![Op::SetChildSpace {
        group,
        space: bad_space,
    }]);
    assert!(p.apply(tx).is_err());
}

fn table(grid: Vec<i64>, rows: Vec<Vec<u32>>) -> Table {
    Table {
        rows: rows
            .into_iter()
            .map(|spans| TableRow {
                height: 100,
                cells: spans
                    .into_iter()
                    .map(|g| TableCell {
                        grid_span: g,
                        ..TableCell::new()
                    })
                    .collect(),
            })
            .collect(),
        grid,
        ..Table::with_size(1, 1, 100, 100)
    }
}

#[test]
fn ops_never_insert_what_loading_refuses() {
    let mut p = sample();
    let slide = busy_slide(&p);
    let before = p.clone();
    let broken = [
        table(Vec::new(), vec![Vec::new()]),
        table(vec![100], Vec::new()),
        table(vec![100, 100], vec![vec![1]]),
        table(vec![100], vec![vec![0]]),
        table(vec![-5], vec![vec![1]]),
        table(vec![i64::MAX], vec![vec![1]]),
    ];
    for t in broken {
        let s = Shape::new(
            p.alloc_id(),
            "T",
            Some(Transform::new(0, 0, 100, 100)),
            ShapeKind::Table(t.clone()),
        );
        let sid = s.id;
        let tx = Transaction::new(vec![Op::InsertShapes {
            parent: slide,
            index: 0,
            shapes: vec![s.clone()],
        }]);
        assert!(matches!(p.apply(tx), Err(DeckError::InvalidOp(_))));
        assert_eq!(p, before);
        // Inside a group, and inside a new slide.
        let mut group = edit::new_table(&mut p, 1, 1, Rect::new(0, 0, 10, 10)).unwrap();
        group.kind = ShapeKind::Group(crate::shape::Group {
            child_space: ChildSpace::identity(Rect::new(0, 0, 10, 10)),
            children: vec![s.clone()],
            props: crate::shape::ShapeProps::default(),
        });
        let tx = Transaction::new(vec![Op::InsertShapes {
            parent: slide,
            index: 0,
            shapes: vec![group],
        }]);
        assert!(p.apply(tx).is_err());
        let layout = p.slides[0].layout;
        let mut new = edit::new_slide(&mut p, layout);
        new.shapes.push(s.clone());
        let tx = Transaction::new(vec![Op::InsertSlide {
            index: 0,
            slide: Box::new(new),
        }]);
        assert!(p.apply(tx).is_err());
        // Replacing an existing shape's kind.
        let existing = p.slide(slide).unwrap().shapes[0].id;
        let tx = Transaction::new(vec![Op::SetShapeKind {
            shape: existing,
            kind: Box::new(ShapeKind::Table(t)),
        }]);
        assert!(p.apply(tx).is_err());
        assert_eq!(p, before);
        assert!(p.shape(sid).is_none());
    }
}

#[test]
fn new_table_keeps_to_powerpoints_limits() {
    let mut p = sample();
    let r = Rect::new(0, 0, 1_000_000, 1_000_000);
    assert!(edit::new_table(&mut p, MAX_TABLE_ROWS, MAX_TABLE_COLS, r).is_ok());
    for (rows, cols) in [
        (0, 1),
        (1, 0),
        (MAX_TABLE_ROWS + 1, 1),
        (1, MAX_TABLE_COLS + 1),
        (usize::MAX, usize::MAX),
    ] {
        assert!(matches!(
            edit::new_table(&mut p, rows, cols, r),
            Err(DeckError::InvalidOp(_))
        ));
    }
    let t = Table::with_size(usize::MAX, usize::MAX, 10, 10);
    assert_eq!(
        (t.rows.len(), t.grid.len()),
        (MAX_TABLE_ROWS, MAX_TABLE_COLS)
    );
}

/// Hostile numbers for a transform: the edges of `i64`, of the DrawingML range, and ordinary
/// values.
fn coord() -> impl Strategy<Value = i64> {
    prop_oneof![
        Just(i64::MAX),
        Just(i64::MIN),
        Just(MAX_COORD),
        Just(-MAX_COORD),
        Just(MAX_COORD + 1),
        -2_000_000i64..20_000_000,
        any::<i64>(),
    ]
}

fn xfrm() -> impl Strategy<Value = Transform> {
    (
        coord(),
        coord(),
        coord(),
        coord(),
        prop_oneof![Just(0), any::<i32>()],
    )
        .prop_map(|(x, y, cx, cy, rot)| Transform {
            x,
            y,
            cx,
            cy,
            rot,
            flip_h: rot % 2 == 0,
            flip_v: false,
        })
}

fn lens() -> impl Strategy<Value = Vec<usize>> {
    proptest::collection::vec(
        prop_oneof![Just(usize::MAX), Just(usize::MAX / 2 + 1), 0usize..8],
        0..5,
    )
}

/// Damage to a sample presentation, chosen by the strategy.
#[derive(Debug, Clone)]
enum Damage {
    Sections(Vec<usize>),
    Xfrm(usize, Transform),
    ChildSpace(i64, i64, i64, i64),
    Grid(i64),
    Timing(u32, u32, u32),
}

fn damage() -> impl Strategy<Value = Damage> {
    prop_oneof![
        lens().prop_map(Damage::Sections),
        (any::<usize>(), xfrm()).prop_map(|(i, x)| Damage::Xfrm(i, x)),
        (coord(), coord(), coord(), coord())
            .prop_map(|(a, b, c, d)| Damage::ChildSpace(a, b, c, d)),
        coord().prop_map(Damage::Grid),
        (any::<u32>(), any::<u32>(), any::<u32>()).prop_map(|(a, b, c)| Damage::Timing(a, b, c)),
    ]
}

fn damage_sample(damages: &[Damage]) -> Presentation {
    let mut p = sample();
    let slide = busy_slide(&p);
    for d in damages {
        match d {
            Damage::Sections(l) => p.sections = sections(&mut p, l),
            Damage::Xfrm(i, x) => {
                let shapes = &mut p.slide_mut(slide).unwrap().shapes;
                let k = i % shapes.len();
                shapes[k].xfrm = Some(*x);
            }
            Damage::ChildSpace(x, y, cx, cy) => {
                for s in &mut p.slides[4].shapes {
                    if let ShapeKind::Group(g) = &mut s.kind {
                        g.child_space = ChildSpace {
                            x: *x,
                            y: *y,
                            cx: *cx,
                            cy: *cy,
                        };
                    }
                }
            }
            Damage::Grid(w) => {
                for s in &mut p.slide_mut(slide).unwrap().shapes {
                    if let ShapeKind::Table(t) = &mut s.kind {
                        t.grid[0] = *w;
                    }
                }
            }
            Damage::Timing(delay, duration, repeat) => {
                for s in &mut p.slides {
                    for e in &mut s.timeline.sequence {
                        e.delay_ms = *delay;
                        e.duration_ms = *duration;
                        e.repeat = *repeat;
                        e.start = crate::animation::EffectStart::AfterPrevious;
                    }
                }
            }
        }
    }
    p
}

/// Every edit builder that does arithmetic on positions, sizes, sections or timings, run on
/// a loaded presentation; each transaction that applies must leave a file that reopens.
fn exercise(p: &mut Presentation, n: i64) -> std::result::Result<(), TestCaseError> {
    let ids: Vec<NodeId> = p
        .slides
        .iter()
        .flat_map(|s| s.shapes.iter().map(|x| x.id))
        .collect();
    let slide = busy_slide(p);
    let on_slide: Vec<NodeId> = p
        .slide(slide)
        .unwrap()
        .shapes
        .iter()
        .map(|s| s.id)
        .collect();
    let free: Vec<NodeId> = p
        .slide(slide)
        .unwrap()
        .shapes
        .iter()
        .filter(|s| s.placeholder.is_none())
        .map(|s| s.id)
        .collect();
    let first_section = p.sections.first().map(|s| s.id);
    let mut txs = Vec::new();
    for how in [
        Align::Left,
        Align::Center,
        Align::Right,
        Align::Top,
        Align::Middle,
        Align::Bottom,
    ] {
        for to in [AlignTo::Selection, AlignTo::Slide] {
            txs.push(edit::align(p, &on_slide, how, to));
        }
    }
    for horizontal in [true, false] {
        for to in [AlignTo::Selection, AlignTo::Slide] {
            txs.push(edit::distribute(p, &on_slide, horizontal, to));
        }
        txs.push(edit::flip(p, &on_slide, horizontal));
    }
    txs.push(edit::group(p, &free));
    txs.push(edit::rotate_by(
        p,
        &on_slide,
        i32::try_from(n % i64::from(i32::MAX)).unwrap_or(0),
    ));
    txs.push(edit::move_shapes(p, &on_slide, n, n.wrapping_neg()));
    let size = SlideSize {
        cx: 12_700,
        cy: 51_206_400,
        kind: SlideSizeKind::Custom,
    };
    txs.push(edit::set_slide_size(p, size, ScaleMode::Maximize));
    let at = usize::try_from(n.rem_euclid(8)).unwrap_or(0);
    txs.push(edit::add_section(p, at, "N"));
    txs.push(edit::delete_slides(p, &[p.slides[0].id]));
    txs.push(edit::move_slides(p, &[p.slides[1].id], at));
    txs.push(edit::duplicate_slides(p, &[p.slides[2].id]));
    if let Some(s) = first_section {
        txs.push(edit::move_section(p, s, 1));
        txs.push(edit::remove_section(p, s, n % 2 == 0));
    }
    for s in &p.slides {
        let _ = s.timeline.schedule();
        let _ = s.timeline.badges();
    }
    for id in &ids {
        let _ = edit::ungroup(p, *id);
        let _ = p.resolved_xfrm(*id);
        let _ = p.chain_of(*id);
    }
    for tx in txs.into_iter().flatten() {
        let before = p.clone();
        match p.apply(tx) {
            Ok(inv) => {
                prop_assert!(p.validate().is_ok(), "{:?}", p.validate());
                prop_assert!(reopen(p).is_ok(), "{:?}", reopen(p).err());
                p.apply(inv).unwrap();
                prop_assert_eq!(&*p, &before);
            }
            Err(_) => prop_assert_eq!(&*p, &before),
        }
    }
    Ok(())
}

/// A hostile op built from the strategy's numbers.
fn hostile_op(p: &mut Presentation, k: u8, a: usize, x: Transform, l: &[usize]) -> Op {
    let slide = busy_slide(p);
    let shapes: Vec<NodeId> = p
        .slide(slide)
        .unwrap()
        .shapes
        .iter()
        .map(|s| s.id)
        .collect();
    let shape = shapes[a % shapes.len()];
    let ext = || i64::try_from(a % 3).unwrap_or(0);
    match k % 9 {
        0 => Op::SetSections {
            sections: sections(p, l),
        },
        1 => Op::SetTransform {
            shape,
            xfrm: Some(x),
        },
        2 => Op::InsertShapes {
            parent: slide,
            index: a % (shapes.len() + 2),
            shapes: vec![Shape {
                xfrm: Some(x),
                ..Shape::text_box(p.alloc_id(), Rect::new(0, 0, 1, 1), "h")
            }],
        },
        3 => {
            let cols = a % 3;
            let t = table(
                vec![x.cx; cols],
                (0..a % 3)
                    .map(|r| vec![u32::from(r != 1); cols + r % 2])
                    .collect(),
            );
            Op::InsertShapes {
                parent: slide,
                index: 0,
                shapes: vec![Shape::new(p.alloc_id(), "T", Some(x), ShapeKind::Table(t))],
            }
        }
        4 => Op::SetChildSpace {
            group: p.slides[4].shapes.last().unwrap().id,
            space: ChildSpace {
                x: x.x,
                y: x.y,
                cx: x.cx.saturating_mul(ext()),
                cy: x.cy,
            },
        },
        5 => Op::MoveSlide {
            from: a,
            to: l.first().copied().unwrap_or(0),
        },
        6 => Op::RemoveShapes {
            parent: slide,
            index: a,
            count: l.first().copied().unwrap_or(usize::MAX),
        },
        7 => Op::MoveShape {
            parent: slide,
            from: a,
            to: a.wrapping_add(1),
        },
        _ => Op::SetShapeKind {
            shape,
            kind: Box::new(ShapeKind::Table(table(vec![x.cx], vec![vec![1]; a % 2]))),
        },
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    /// Structurally valid files with hostile numbers: loading refuses them or accepts a model
    /// that validates, and every edit builder and op on what loads neither panics nor leaves
    /// something that cannot be reopened.
    #[test]
    fn hostile_models_load_or_refuse_and_edit_safely(
        damages in proptest::collection::vec(damage(), 0..4),
        n in any::<i64>(),
    ) {
        let p = damage_sample(&damages);
        let bytes = p.to_bytes().unwrap();
        match Presentation::from_bytes(&bytes) {
            Ok(mut q) => {
                prop_assert!(q.validate().is_ok());
                exercise(&mut q, n)?;
            }
            Err(e) => prop_assert!(matches!(e, DeckError::Corrupt(_)), "{e:?}"),
        }
    }

    /// Hostile ops (as from the journal or control channel): refused atomically, or applied
    /// to a presentation that validates, reopens and undoes exactly.
    #[test]
    fn hostile_ops_apply_atomically_and_reopen(
        steps in proptest::collection::vec((any::<u8>(), any::<usize>(), xfrm(), lens()), 1..6),
    ) {
        let mut p = sample();
        for (k, a, x, l) in steps {
            let op = hostile_op(&mut p, k, a, x, &l);
            let tx = Transaction::new(vec![op]);
            let tx = Transaction::from_bytes(&tx.to_bytes().unwrap()).unwrap();
            let before = p.clone();
            match p.apply(tx) {
                Ok(inv) => {
                    prop_assert!(p.validate().is_ok(), "{:?}", p.validate());
                    let back = reopen(&p);
                    prop_assert!(back.is_ok(), "{:?}", back.err());
                    prop_assert_eq!(back.unwrap(), p.clone());
                    let after = p.clone();
                    p.apply(inv.clone()).unwrap();
                    prop_assert_eq!(&p, &before);
                    p = after;
                }
                Err(_) => prop_assert_eq!(&p, &before),
            }
        }
    }
}
