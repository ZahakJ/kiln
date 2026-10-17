// SPDX-License-Identifier: AGPL-3.0-or-later
//! The parts around worksheets, as parse/serialise pairs: notes (`comments*.xml` and their VML
//! shapes), threaded comments and the people list, tables, drawing anchors, the dynamic-array
//! cell metadata and the theme colours and fonts.

use std::fmt::Write as _;

use kiln_ooxml::xml::namespaces::local_name;
use kiln_ooxml::xml::{XmlElement, XmlNode};
use kiln_sheet::meta::{
    Anchor, AnchorPoint, DrawingAnchor, DrawingKind, EditAs, Mention, Note, Person, Table,
    TableColumn, TableFilter, TableStyleInfo, ThreadedComment, TotalsFunction,
};
use kiln_sheet::{CellPos, Theme};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::sheet_xml::{
    auto_filter_el, file_formula, model_formula, parse_filter_columns, parse_ref, parse_sort_state,
    ref_text,
};
use crate::text::{cell_ref, parse_cell_ref};
use crate::xmlx::{abool_or, ai64, au32, child_local, children_local, flag, set_attr, text_el};

/// The SpreadsheetML main namespace.
pub(crate) const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
/// Threaded comments and people (Excel 365).
pub(crate) const NS_THREADED: &str =
    "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments";
/// Relationship type of threaded comments.
pub(crate) const REL_THREADED: &str =
    "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
/// Relationship type of the people part.
pub(crate) const REL_PERSON: &str =
    "http://schemas.microsoft.com/office/2017/10/relationships/person";
/// Content type of threaded comments.
pub(crate) const CT_THREADED: &str = "application/vnd.ms-excel.threadedcomments+xml";
/// Content type of the people part.
pub(crate) const CT_PERSON: &str = "application/vnd.ms-excel.person+xml";
/// Content type of comments.
pub(crate) const CT_COMMENTS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";
/// Content type of VML drawings.
pub(crate) const CT_VML: &str = "application/vnd.openxmlformats-officedocument.vmlDrawing";
/// Content type of tables.
pub(crate) const CT_TABLE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
/// Content type of the cell metadata part.
pub(crate) const CT_METADATA: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml";

// ── notes ─────────────────────────────────────────────────────────────────

fn rich_text_plain(e: &XmlElement) -> String {
    let mut out = String::new();
    for c in e.elements() {
        match local_name(&c.name) {
            "t" => out.push_str(&crate::text::decode_x_escapes(&c.text())),
            "r" => {
                for t in children_local(c, "t") {
                    out.push_str(&crate::text::decode_x_escapes(&t.text()));
                }
            }
            _ => {}
        }
    }
    out
}

/// Notes of a comments part (visibility and size come from the VML shapes).
pub(crate) fn parse_comments(root: &XmlElement) -> Vec<Note> {
    let authors: Vec<String> = root
        .child("authors")
        .map(|a| a.children_named("author").map(XmlElement::text).collect())
        .unwrap_or_default();
    root.child("commentList")
        .into_iter()
        .flat_map(|l| l.children_named("comment"))
        .filter_map(|c| {
            let (row, col) = parse_cell_ref(c.attr("ref")?.as_bytes())?;
            Some(Note {
                at: CellPos::new(row, col),
                author: au32(c, "authorId")
                    .and_then(|i| authors.get(i as usize).cloned())
                    .unwrap_or_default(),
                text: c.child("text").map(rich_text_plain).unwrap_or_default(),
                visible: false,
                size: None,
            })
        })
        .collect()
}

fn css_length_px(v: &str) -> Option<u32> {
    let v = v.trim();
    let (num, unit) = v
        .find(|c: char| c.is_ascii_alphabetic())
        .map_or((v, "px"), |i| (&v[..i], &v[i..]));
    let n: f64 = num.trim().parse().ok()?;
    let px = match unit {
        "pt" => n * 96.0 / 72.0,
        "in" => n * 96.0,
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 96.0 / 25.4,
        _ => n,
    };
    (px.is_finite() && px > 0.0).then(|| px.round() as u32)
}

/// What a note's VML shape says: shown, and its size in px.
pub(crate) type NoteShape = (bool, Option<(u32, u32)>);

/// Note shapes of a VML part by (row, col).
pub(crate) fn parse_vml_notes(root: &XmlElement) -> FxHashMap<(u32, u32), NoteShape> {
    let mut out = FxHashMap::default();
    for shape in root.descendants("v:shape") {
        let Some(cd) = shape
            .elements()
            .find(|e| local_name(&e.name) == "ClientData")
        else {
            continue;
        };
        if cd.attr("ObjectType") != Some("Note") {
            continue;
        }
        let num = |name: &str| {
            cd.elements()
                .find(|e| local_name(&e.name) == name)
                .and_then(|e| e.text().trim().parse::<u32>().ok())
        };
        let (Some(row), Some(col)) = (num("Row"), num("Column")) else {
            continue;
        };
        let visible = cd.elements().any(|e| local_name(&e.name) == "Visible");
        let style = shape.attr("style").unwrap_or("");
        let mut w = None;
        let mut h = None;
        for decl in style.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                match k.trim() {
                    "width" => w = css_length_px(v),
                    "height" => h = css_length_px(v),
                    _ => {}
                }
            }
        }
        out.insert((row, col), (visible, w.zip(h)));
    }
    out
}

/// A comments part for notes.
pub(crate) fn comments_el(notes: &[Note]) -> XmlElement {
    let mut authors: Vec<&str> = Vec::new();
    for n in notes {
        if !authors.contains(&n.author.as_str()) {
            authors.push(&n.author);
        }
    }
    let mut a = XmlElement::new("authors");
    for au in &authors {
        a.children
            .push(XmlNode::Element(XmlElement::new("author").with_text(*au)));
    }
    let mut list = XmlElement::new("commentList");
    for n in notes {
        let idx = authors.iter().position(|x| *x == n.author).unwrap_or(0);
        list.children.push(XmlNode::Element(
            XmlElement::new("comment")
                .with_attr("ref", cell_ref(n.at.row, n.at.col))
                .with_attr("authorId", idx)
                .with_attr("shapeId", "0")
                .with_child(
                    XmlElement::new("text")
                        .with_child(text_el("t", &crate::text::encode_x_escapes(&n.text))),
                ),
        ));
    }
    XmlElement::new("comments")
        .with_attr("xmlns", NS_MAIN)
        .with_child(a)
        .with_child(list)
}

/// The VML shapes Excel needs to show notes. `sheet_number` (1-based) spaces the shape ids.
pub(crate) fn notes_vml(notes: &[Note], sheet_number: u32) -> String {
    let mut s = String::with_capacity(512 + notes.len() * 700);
    s.push_str("<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\">");
    let _ = write!(
        s,
        "<o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"{sheet_number}\"/></o:shapelayout>"
    );
    s.push_str("<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>");
    for (i, n) in notes.iter().enumerate() {
        let id = u64::from(sheet_number) * 1024 + 1 + i as u64;
        let (w, h) = n.size.unwrap_or((144, 79));
        let vis = if n.visible { "visible" } else { "hidden" };
        let _ = write!(
            s,
            "<v:shape id=\"_x0000_s{id}\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:{w}px;height:{h}px;z-index:{z};visibility:{vis}\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\"><v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/><v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"></div></v:textbox><x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>{c1}, 15, {r0}, 10, {c2}, 15, {r2}, 4</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{row}</x:Row><x:Column>{col}</x:Column>{visible}</x:ClientData></v:shape>",
            z = i + 1,
            c1 = n.at.col + 1,
            r0 = n.at.row.saturating_sub(1),
            c2 = n.at.col + 3,
            r2 = n.at.row + 3,
            row = n.at.row,
            col = n.at.col,
            visible = if n.visible { "<x:Visible/>" } else { "" },
        );
    }
    s.push_str("</xml>");
    s
}

// ── threaded comments ─────────────────────────────────────────────────────

pub(crate) fn parse_persons(root: &XmlElement) -> Vec<Person> {
    children_local(root, "person")
        .filter_map(|p| {
            Some(Person {
                id: p.attr("id")?.to_owned(),
                display_name: p.attr("displayName").unwrap_or("").to_owned(),
                user_id: p.attr("userId").map(str::to_owned),
                provider_id: p.attr("providerId").map(str::to_owned),
            })
        })
        .collect()
}

pub(crate) fn persons_el(persons: &[Person]) -> XmlElement {
    let mut root = XmlElement::new("personList")
        .with_attr("xmlns", NS_THREADED)
        .with_attr("xmlns:x", NS_MAIN);
    for p in persons {
        let mut e = XmlElement::new("person")
            .with_attr("displayName", p.display_name.clone())
            .with_attr("id", p.id.clone());
        if let Some(u) = &p.user_id {
            e.attrs.set("userId", u.clone());
        }
        if let Some(pr) = &p.provider_id {
            e.attrs.set("providerId", pr.clone());
        }
        root.children.push(XmlNode::Element(e));
    }
    root
}

pub(crate) fn parse_threaded(root: &XmlElement, persons: &[Person]) -> Vec<ThreadedComment> {
    children_local(root, "threadedComment")
        .filter_map(|t| {
            let (row, col) = parse_cell_ref(t.attr("ref")?.as_bytes())?;
            let person = t.attr("personId").map(str::to_owned);
            let author = person
                .as_deref()
                .and_then(|id| persons.iter().find(|p| p.id == id))
                .map(|p| p.display_name.clone())
                .unwrap_or_default();
            Some(ThreadedComment {
                id: t.attr("id")?.to_owned(),
                at: CellPos::new(row, col),
                author,
                author_id: person,
                date: t.attr("dT").unwrap_or("").to_owned(),
                text: child_local(t, "text")
                    .map(XmlElement::text)
                    .unwrap_or_default(),
                parent_id: t.attr("parentId").map(str::to_owned),
                done: abool_or(t, "done", false),
                mentions: child_local(t, "mentions")
                    .into_iter()
                    .flat_map(|m| children_local(m, "mention"))
                    .filter_map(|m| {
                        Some(Mention {
                            person_id: m.attr("mentionpersonId")?.to_owned(),
                            start: au32(m, "startIndex").unwrap_or(0),
                            length: au32(m, "length").unwrap_or(0),
                        })
                    })
                    .collect(),
            })
        })
        .collect()
}

pub(crate) fn threaded_el(threads: &[ThreadedComment]) -> XmlElement {
    let mut root = XmlElement::new("ThreadedComments")
        .with_attr("xmlns", NS_THREADED)
        .with_attr("xmlns:x", NS_MAIN);
    for (i, t) in threads.iter().enumerate() {
        let mut e = XmlElement::new("threadedComment")
            .with_attr("ref", cell_ref(t.at.row, t.at.col))
            .with_attr("dT", t.date.clone());
        if let Some(p) = &t.author_id {
            e.attrs.set("personId", p.clone());
        }
        e.attrs.set("id", t.id.clone());
        if let Some(p) = &t.parent_id {
            e.attrs.set("parentId", p.clone());
        }
        set_attr(&mut e, "done", flag(t.done));
        e.children.push(XmlNode::Element(text_el("text", &t.text)));
        if !t.mentions.is_empty() {
            let mut m = XmlElement::new("mentions");
            for (k, x) in t.mentions.iter().enumerate() {
                m.children.push(XmlNode::Element(
                    XmlElement::new("mention")
                        .with_attr("mentionpersonId", x.person_id.clone())
                        .with_attr(
                            "mentionId",
                            format!("{{00000000-0000-4000-8000-{:06X}{:06X}}}", i, k),
                        )
                        .with_attr("startIndex", x.start)
                        .with_attr("length", x.length),
                ));
            }
            e.children.push(XmlNode::Element(m));
        }
        root.children.push(XmlNode::Element(e));
    }
    root
}

// ── tables ────────────────────────────────────────────────────────────────

const TOTALS: &[(TotalsFunction, &str)] = &[
    (TotalsFunction::None, "none"),
    (TotalsFunction::Sum, "sum"),
    (TotalsFunction::Min, "min"),
    (TotalsFunction::Max, "max"),
    (TotalsFunction::Average, "average"),
    (TotalsFunction::Count, "count"),
    (TotalsFunction::CountNums, "countNums"),
    (TotalsFunction::StdDev, "stdDev"),
    (TotalsFunction::Var, "var"),
    (TotalsFunction::Custom, "custom"),
];

pub(crate) fn parse_table(root: &XmlElement) -> Option<Table> {
    let range = parse_ref(root.attr("ref")?)?;
    let name = root
        .attr("name")
        .or_else(|| root.attr("displayName"))?
        .to_owned();
    let header_row = au32(root, "headerRowCount").unwrap_or(1) > 0;
    let totals_row = au32(root, "totalsRowCount").unwrap_or(0) > 0;
    let columns = root
        .child("tableColumns")
        .map(|c| {
            c.children_named("tableColumn")
                .map(|tc| TableColumn {
                    name: tc.attr("name").unwrap_or("").to_owned(),
                    totals_function: tc
                        .attr("totalsRowFunction")
                        .and_then(|f| TOTALS.iter().find(|(_, s)| *s == f))
                        .map_or(TotalsFunction::None, |(t, _)| *t),
                    totals_label: tc.attr("totalsRowLabel").map(str::to_owned),
                    totals_formula: child_local(tc, "totalsRowFormula")
                        .map(|f| model_formula(&f.text())),
                    calculated_formula: child_local(tc, "calculatedColumnFormula")
                        .map(|f| model_formula(&f.text())),
                    data_dxf: au32(tc, "dataDxfId"),
                    header_dxf: au32(tc, "headerRowDxfId"),
                    totals_dxf: au32(tc, "totalsRowDxfId"),
                })
                .collect()
        })
        .unwrap_or_default();
    let display = root
        .attr("displayName")
        .filter(|d| *d != name)
        .map(str::to_owned);
    Some(Table {
        id: au32(root, "id").unwrap_or(1),
        name,
        display_name: display,
        range,
        header_row,
        totals_row,
        columns,
        style: root.child("tableStyleInfo").map(|s| TableStyleInfo {
            name: s.attr("name").unwrap_or("").to_owned(),
            show_first_column: abool_or(s, "showFirstColumn", false),
            show_last_column: abool_or(s, "showLastColumn", false),
            show_row_stripes: abool_or(s, "showRowStripes", false),
            show_column_stripes: abool_or(s, "showColumnStripes", false),
        }),
        auto_filter: root.child("autoFilter").map(|af| TableFilter {
            columns: parse_filter_columns(af),
            sort: child_local(af, "sortState").and_then(parse_sort_state),
        }),
        comment: root.attr("comment").map(str::to_owned),
    })
}

/// A table part, based on `base` (the element read) for attributes the model does not cover.
pub(crate) fn table_el(t: &Table, base: Option<&XmlElement>) -> XmlElement {
    let mut e = XmlElement::new("table").with_attr("xmlns", NS_MAIN);
    if let Some(b) = base {
        for (k, v) in b.attrs.iter() {
            if k.starts_with("xmlns") || k == "mc:Ignorable" || k == "xr:uid" {
                e.attrs.set(k, v);
            }
        }
    }
    e.attrs.set("id", t.id.to_string());
    e.attrs.set("name", t.name.clone());
    e.attrs.set(
        "displayName",
        t.display_name.clone().unwrap_or_else(|| t.name.clone()),
    );
    e.attrs.set("ref", ref_text(&t.range));
    if !t.header_row {
        e.attrs.set("headerRowCount", "0");
    }
    if t.totals_row {
        e.attrs.set("totalsRowCount", "1");
    } else if base.is_some_and(|b| b.attr("totalsRowShown") == Some("0")) || base.is_none() {
        e.attrs.set("totalsRowShown", "0");
    }
    if let Some(c) = &t.comment {
        e.attrs.set("comment", c.clone());
    }
    if let Some(af) = &t.auto_filter {
        let data_end = if t.totals_row {
            t.range.r1.saturating_sub(1)
        } else {
            t.range.r1
        };
        let r = kiln_sheet::Range::new(t.range.r0, t.range.c0, data_end, t.range.c1);
        e.children.push(XmlNode::Element(auto_filter_el(
            &r,
            &af.columns,
            af.sort.as_ref(),
        )));
    }
    let mut cols = XmlElement::new("tableColumns").with_attr("count", t.columns.len());
    for (i, c) in t.columns.iter().enumerate() {
        let mut x = XmlElement::new("tableColumn")
            .with_attr("id", i + 1)
            .with_attr("name", c.name.clone());
        if c.totals_function != TotalsFunction::None {
            let f = TOTALS
                .iter()
                .find(|(k, _)| *k == c.totals_function)
                .map_or("none", |(_, s)| s);
            x.attrs.set("totalsRowFunction", f);
        }
        if let Some(l) = &c.totals_label {
            x.attrs.set("totalsRowLabel", l.clone());
        }
        for (name, v) in [
            ("dataDxfId", c.data_dxf),
            ("headerRowDxfId", c.header_dxf),
            ("totalsRowDxfId", c.totals_dxf),
        ] {
            if let Some(v) = v {
                x.attrs.set(name, v.to_string());
            }
        }
        if let Some(f) = &c.calculated_formula {
            x.children.push(XmlNode::Element(text_el(
                "calculatedColumnFormula",
                &file_formula(f),
            )));
        }
        if let Some(f) = &c.totals_formula {
            x.children.push(XmlNode::Element(text_el(
                "totalsRowFormula",
                &file_formula(f),
            )));
        }
        cols.children.push(XmlNode::Element(x));
    }
    e.children.push(XmlNode::Element(cols));
    if let Some(s) = &t.style {
        let mut x = XmlElement::new("tableStyleInfo");
        if !s.name.is_empty() {
            x.attrs.set("name", s.name.clone());
        }
        for (name, v) in [
            ("showFirstColumn", s.show_first_column),
            ("showLastColumn", s.show_last_column),
            ("showRowStripes", s.show_row_stripes),
            ("showColumnStripes", s.show_column_stripes),
        ] {
            x.attrs.set(name, if v { "1" } else { "0" });
        }
        e.children.push(XmlNode::Element(x));
    }
    e
}

// ── drawings ──────────────────────────────────────────────────────────────

fn anchor_point(e: Option<&XmlElement>) -> AnchorPoint {
    let Some(e) = e else {
        return AnchorPoint::default();
    };
    let n = |name: &str| {
        e.elements()
            .find(|c| local_name(&c.name) == name)
            .and_then(|c| c.text().trim().parse::<i64>().ok())
            .unwrap_or(0)
    };
    AnchorPoint {
        col: u32::try_from(n("col").max(0)).unwrap_or(0),
        col_off: n("colOff"),
        row: u32::try_from(n("row").max(0)).unwrap_or(0),
        row_off: n("rowOff"),
    }
}

fn ext_of(e: Option<&XmlElement>, x: &str, y: &str) -> (i64, i64) {
    e.map_or((0, 0), |e| {
        (ai64(e, x).unwrap_or(0), ai64(e, y).unwrap_or(0))
    })
}

/// The object of an anchor: (kind, cNvPr, relationship id of its chart or picture).
fn anchor_object(
    anchor: &XmlElement,
) -> Option<(DrawingKind, Option<&XmlElement>, Option<String>)> {
    for c in anchor.elements() {
        let ln = local_name(&c.name);
        let obj = if ln == "AlternateContent" {
            c.child("mc:Choice")
                .and_then(XmlElement::first_element)
                .or_else(|| c.child("mc:Fallback").and_then(XmlElement::first_element))
        } else {
            Some(c)
        };
        let Some(obj) = obj else { continue };
        let kind = match local_name(&obj.name) {
            "sp" => DrawingKind::Shape,
            "pic" => DrawingKind::Picture,
            "grpSp" => DrawingKind::Group,
            "cxnSp" => DrawingKind::Connector,
            "graphicFrame" => {
                let chart = obj.descendants("c:chart").into_iter().next().is_some()
                    || obj.descendants("cx:chart").into_iter().next().is_some();
                if chart {
                    DrawingKind::Chart
                } else {
                    DrawingKind::Other
                }
            }
            "contentPart" => DrawingKind::Other,
            _ => continue,
        };
        let cnv = obj
            .elements()
            .find(|e| local_name(&e.name).starts_with("nv"))
            .and_then(|nv| nv.elements().find(|e| local_name(&e.name) == "cNvPr"));
        let rid = match kind {
            DrawingKind::Chart => obj
                .descendants("c:chart")
                .into_iter()
                .chain(obj.descendants("cx:chart"))
                .find_map(|c| c.attr("r:id").map(str::to_owned)),
            DrawingKind::Picture => obj
                .descendants("a:blip")
                .into_iter()
                .find_map(|b| b.attr("r:embed").map(str::to_owned)),
            _ => None,
        };
        return Some((kind, cnv, rid));
    }
    None
}

/// Reads the anchors of a drawing part. `resolve` turns a relationship id into a part name.
pub(crate) fn parse_drawing(
    root: &XmlElement,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Vec<DrawingAnchor> {
    let mut out = Vec::new();
    let mut seen: FxHashSet<String> = FxHashSet::default();
    for (i, a) in root.elements().enumerate() {
        let anchor = match local_name(&a.name) {
            "twoCellAnchor" => Anchor::TwoCell {
                from: anchor_point(a.elements().find(|e| local_name(&e.name) == "from")),
                to: anchor_point(a.elements().find(|e| local_name(&e.name) == "to")),
                edit_as: match a.attr("editAs") {
                    Some("oneCell") => EditAs::OneCell,
                    Some("absolute") => EditAs::Absolute,
                    _ => EditAs::TwoCell,
                },
            },
            "oneCellAnchor" => Anchor::OneCell {
                from: anchor_point(a.elements().find(|e| local_name(&e.name) == "from")),
                ext: ext_of(
                    a.elements().find(|e| local_name(&e.name) == "ext"),
                    "cx",
                    "cy",
                ),
            },
            "absoluteAnchor" => Anchor::Absolute {
                pos: ext_of(
                    a.elements().find(|e| local_name(&e.name) == "pos"),
                    "x",
                    "y",
                ),
                ext: ext_of(
                    a.elements().find(|e| local_name(&e.name) == "ext"),
                    "cx",
                    "cy",
                ),
            },
            _ => continue,
        };
        let (kind, cnv, rid) = anchor_object(a).unwrap_or((DrawingKind::Other, None, None));
        let mut id = cnv
            .and_then(|c| c.attr("id"))
            .map_or_else(|| format!("anchor{i}"), str::to_owned);
        if !seen.insert(id.clone()) {
            id = format!("anchor{i}");
            seen.insert(id.clone());
        }
        let locked = a
            .elements()
            .find(|e| local_name(&e.name) == "clientData")
            .is_none_or(|c| abool_or(c, "fLocksWithSheet", true));
        out.push(DrawingAnchor {
            id,
            kind,
            reference: rid.as_deref().and_then(resolve).unwrap_or_default(),
            anchor,
            name: cnv.and_then(|c| c.attr("name")).map(str::to_owned),
            alt_text: cnv.and_then(|c| c.attr("descr")).map(str::to_owned),
            hidden: cnv.is_some_and(|c| abool_or(c, "hidden", false)),
            locked,
        });
    }
    out
}

fn point_el(name: &str, p: &AnchorPoint) -> XmlElement {
    XmlElement::new(name)
        .with_child(XmlElement::new("xdr:col").with_text(p.col.to_string()))
        .with_child(XmlElement::new("xdr:colOff").with_text(p.col_off.to_string()))
        .with_child(XmlElement::new("xdr:row").with_text(p.row.to_string()))
        .with_child(XmlElement::new("xdr:rowOff").with_text(p.row_off.to_string()))
}

/// Rewrites a drawing part for moved, resized, reordered or removed anchors: each anchor is the
/// element read for it (matched by id) with its placement replaced. Anchors the part never had
/// cannot be created here and are reported.
pub(crate) fn patch_drawing(
    root: &XmlElement,
    read: &[DrawingAnchor],
    anchors: &[DrawingAnchor],
    objects: &rustc_hash::FxHashMap<String, Vec<XmlElement>>,
    issues: &mut Vec<String>,
) -> XmlElement {
    let elements: Vec<&XmlElement> = root
        .elements()
        .filter(|e| {
            matches!(
                local_name(&e.name),
                "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor"
            )
        })
        .collect();
    let mut out = root.clone();
    out.children.retain(|c| match c {
        XmlNode::Element(e) => !matches!(
            local_name(&e.name),
            "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor"
        ),
        _ => true,
    });
    let insert_at = out.children.len();
    let mut new_children = Vec::new();
    for a in anchors {
        let object: Vec<XmlNode> = if let Some(made) = objects.get(&a.id) {
            // A frame made by the writer (a chart added in Kiln, or re-typed).
            made.iter().cloned().map(XmlNode::Element).collect()
        } else {
            let Some(k) = read.iter().position(|r| r.id == a.id) else {
                issues.push(format!(
                    "the drawing object \"{}\" is new; it is not written (only charts are added here)",
                    a.name.as_deref().unwrap_or(&a.id)
                ));
                continue;
            };
            let Some(orig) = elements.get(k) else {
                continue;
            };
            orig.children
                .iter()
                .filter(|c| match c {
                    XmlNode::Element(e) => {
                        !matches!(local_name(&e.name), "from" | "to" | "ext" | "pos")
                    }
                    _ => false,
                })
                .cloned()
                .collect()
        };
        let mut e = match a.anchor {
            Anchor::TwoCell { from, to, edit_as } => {
                let mut e = XmlElement::new("xdr:twoCellAnchor");
                match edit_as {
                    EditAs::TwoCell => {}
                    EditAs::OneCell => e.attrs.set("editAs", "oneCell"),
                    EditAs::Absolute => e.attrs.set("editAs", "absolute"),
                }
                e.children
                    .push(XmlNode::Element(point_el("xdr:from", &from)));
                e.children.push(XmlNode::Element(point_el("xdr:to", &to)));
                e
            }
            Anchor::OneCell { from, ext } => XmlElement::new("xdr:oneCellAnchor")
                .with_child(point_el("xdr:from", &from))
                .with_child(
                    XmlElement::new("xdr:ext")
                        .with_attr("cx", ext.0)
                        .with_attr("cy", ext.1),
                ),
            Anchor::Absolute { pos, ext } => XmlElement::new("xdr:absoluteAnchor")
                .with_child(
                    XmlElement::new("xdr:pos")
                        .with_attr("x", pos.0)
                        .with_attr("y", pos.1),
                )
                .with_child(
                    XmlElement::new("xdr:ext")
                        .with_attr("cx", ext.0)
                        .with_attr("cy", ext.1),
                ),
        };
        e.children.extend(object);
        new_children.push(XmlNode::Element(e));
    }
    for (k, n) in new_children.into_iter().enumerate() {
        out.children.insert(insert_at + k, n);
    }
    out
}

// ── cell metadata ─────────────────────────────────────────────────────────

/// The 1-based `cm` values that mark dynamic-array formulas.
pub(crate) fn dynamic_cell_metadata(root: &XmlElement) -> FxHashSet<u32> {
    let mut out = FxHashSet::default();
    let Some(t) = root
        .child("metadataTypes")
        .map(|m| {
            m.children_named("metadataType")
                .position(|t| t.attr("name") == Some("XLDAPR"))
        })
        .and_then(|p| p.map(|i| i as u32 + 1))
    else {
        return out;
    };
    // Future metadata blocks that are dynamic (fDynamic) — by 0-based index.
    let dynamic_blocks: Vec<bool> = root
        .children_named("futureMetadata")
        .find(|f| f.attr("name") == Some("XLDAPR"))
        .map(|f| {
            f.children_named("bk")
                .map(|bk| {
                    bk.descendants("xda:dynamicArrayProperties")
                        .first()
                        .is_none_or(|p| abool_or(p, "fDynamic", false))
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(cm) = root.child("cellMetadata") {
        for (i, bk) in cm.children_named("bk").enumerate() {
            let dynamic = bk.children_named("rc").any(|rc| {
                au32(rc, "t") == Some(t)
                    && au32(rc, "v")
                        .is_none_or(|v| dynamic_blocks.get(v as usize).copied().unwrap_or(true))
            });
            if dynamic {
                out.insert(i as u32 + 1);
            }
        }
    }
    out
}

/// The metadata part Excel writes for dynamic arrays (`cm="1"`).
pub(crate) const DYNAMIC_METADATA: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<metadata xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:xda=\"http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray\"><metadataTypes count=\"1\"><metadataType name=\"XLDAPR\" minSupportedVersion=\"120000\" copy=\"1\" pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" clearComments=\"1\" assign=\"1\" coerce=\"1\" cellMeta=\"1\"/></metadataTypes><futureMetadata name=\"XLDAPR\" count=\"1\"><bk><extLst><ext uri=\"{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}\"><xda:dynamicArrayProperties fDynamic=\"1\" fCollapsed=\"0\"/></ext></extLst></bk></futureMetadata><cellMetadata count=\"1\"><bk><rc t=\"1\" v=\"0\"/></bk></cellMetadata></metadata>";

// ── theme ─────────────────────────────────────────────────────────────────

/// The model theme of a theme part.
pub(crate) fn read_theme(bytes: &[u8]) -> Option<(Theme, kiln_ooxml::drawingml::theme::Theme)> {
    let t = kiln_ooxml::drawingml::theme::parse_theme_bytes(bytes).ok()?;
    let table = kiln_ooxml::drawingml::theme::theme_color_table(&t);
    let mut colors = [0u32; 12];
    for (i, c) in table.iter().enumerate() {
        let ch = |v: f64| v.round().clamp(0.0, 255.0) as u32;
        colors[i] = (ch(c.r) << 16) | (ch(c.g) << 8) | ch(c.b);
    }
    let model = Theme {
        name: t.name.clone(),
        colors,
        major_font: t.font_scheme.major.latin.typeface.clone(),
        minor_font: t.font_scheme.minor.latin.typeface.clone(),
    };
    Some((model, t))
}

/// A theme part with the model's colours and fonts (based on the part read, or Office's theme).
pub(crate) fn write_theme(
    model: &Theme,
    base: Option<kiln_ooxml::drawingml::theme::Theme>,
) -> Vec<u8> {
    let mut t = base.unwrap_or_else(kiln_ooxml::drawingml::theme::office_theme);
    t.name.clone_from(&model.name);
    for (i, c) in model.colors.iter().enumerate() {
        if let Some(slot) = t.color_scheme.colors.get_mut(i) {
            *slot = kiln_ooxml::drawingml::color::DrawingColor::srgb(&format!("{c:06X}"));
        }
    }
    t.font_scheme
        .major
        .latin
        .typeface
        .clone_from(&model.major_font);
    t.font_scheme
        .minor
        .latin
        .typeface
        .clone_from(&model.minor_font);
    let doc = kiln_ooxml::drawingml::theme::serialize_theme(&t);
    kiln_ooxml::xml::document_to_bytes(&doc).unwrap_or_default()
}

// ── charts ────────────────────────────────────────────────────────────────

/// The chart part handler: the `c:chartSpace` and `cx:chartSpace` parts the drawings reference,
/// read into [`kiln_charts::model::ChartSpace`] (`kiln_charts::package::read_part`) and written
/// back when a chart was edited (`write_part`). Unedited charts stay byte for byte.
impl crate::XlsxSource {
    /// Every chart part of the workbook with its chart (or why it could not be read).
    #[must_use]
    pub fn charts(&self) -> Vec<(String, Result<kiln_charts::model::ChartSpace, String>)> {
        kiln_charts::package::chart_parts(&self.package)
            .into_iter()
            .map(|name| {
                let chart = kiln_charts::package::read_part(&self.package, &name)
                    .map_err(|e| e.to_string());
                (name, chart)
            })
            .collect()
    }

    /// Replaces the chart part `name` with an edited chart; the next save writes it. A chart
    /// that changed vocabulary (`c:` ↔ `cx:`) would need its drawing relationship retyped, which
    /// the writer does not do yet: it is refused.
    ///
    /// # Errors
    ///
    /// `NotWorkbook` when `name` is not a chart part of this package or the chart changed vocabulary.
    pub fn set_chart(
        &mut self,
        name: &str,
        chart: &kiln_charts::model::ChartSpace,
    ) -> Result<(), crate::XlsxError> {
        let ct = self.package.content_type(name).map(str::to_owned);
        if !ct
            .as_deref()
            .is_some_and(kiln_charts::package::is_chart_content_type)
        {
            return Err(crate::XlsxError::NotWorkbook(format!(
                "{name} is not a chart part"
            )));
        }
        if ct.as_deref() != Some(chart.format.content_type()) {
            return Err(crate::XlsxError::NotWorkbook(format!(
                "{name}: changing between a chart and a chartex part is not supported yet"
            )));
        }
        kiln_charts::package::write_part(&mut self.package, name, chart)
            .map_err(|e| crate::XlsxError::NotWorkbook(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::parse_xml_str;
    use kiln_sheet::Range;

    fn el(xml: &str) -> XmlElement {
        parse_xml_str(xml).expect("xml").root
    }

    #[test]
    fn notes_round_trip_with_vml() {
        let notes = vec![
            Note {
                at: CellPos::new(0, 0),
                author: "Ann".into(),
                text: "Check\nthis".into(),
                visible: true,
                size: Some((200, 80)),
            },
            Note {
                at: CellPos::new(2, 2),
                author: "Bo".into(),
                text: "x".into(),
                visible: false,
                size: Some((144, 79)),
            },
        ];
        let xml = kiln_ooxml::xml::write_element(
            &comments_el(&notes),
            &kiln_ooxml::xml::WriteOptions::default(),
        )
        .expect("xml");
        let mut back = parse_comments(&el(&xml));
        let vml = parse_vml_notes(&el(&notes_vml(&notes, 1)));
        for n in &mut back {
            if let Some((v, s)) = vml.get(&(n.at.row, n.at.col)) {
                n.visible = *v;
                n.size = *s;
            }
        }
        assert_eq!(back, notes);
    }

    #[test]
    fn threaded_comments_round_trip() {
        let persons = vec![Person {
            id: "{P1}".into(),
            display_name: "Ann".into(),
            user_id: Some("ann@example.org".into()),
            provider_id: Some("None".into()),
        }];
        let t = vec![ThreadedComment {
            id: "{C1}".into(),
            at: CellPos::new(1, 1),
            author: "Ann".into(),
            author_id: Some("{P1}".into()),
            date: "2026-10-16T09:00:00.00".into(),
            text: "Hi @Ann".into(),
            parent_id: None,
            done: true,
            mentions: vec![Mention {
                person_id: "{P1}".into(),
                start: 3,
                length: 4,
            }],
        }];
        let ser = |e: &XmlElement| {
            kiln_ooxml::xml::write_element(e, &kiln_ooxml::xml::WriteOptions::default())
                .expect("xml")
        };
        let p = parse_persons(&el(&ser(&persons_el(&persons))));
        assert_eq!(p, persons);
        assert_eq!(parse_threaded(&el(&ser(&threaded_el(&t))), &p), t);
    }

    #[test]
    fn tables_round_trip() {
        let t = Table {
            id: 3,
            name: "Sales".into(),
            display_name: None,
            range: Range::new(0, 0, 10, 2),
            header_row: true,
            totals_row: true,
            columns: vec![
                TableColumn::named("Region"),
                TableColumn {
                    name: "Total".into(),
                    totals_function: TotalsFunction::Sum,
                    calculated_formula: Some("Sales[[#This Row],[Units]]*2".into()),
                    ..TableColumn::default()
                },
                TableColumn {
                    name: "Note".into(),
                    totals_label: Some("Sum".into()),
                    ..TableColumn::default()
                },
            ],
            style: Some(TableStyleInfo::default()),
            auto_filter: Some(TableFilter::default()),
            comment: None,
        };
        assert_eq!(parse_table(&table_el(&t, None)), Some(t));
    }

    #[test]
    fn drawings_are_read_and_patched() {
        let xml = r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><xdr:oneCellAnchor><xdr:from><xdr:col>5</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx="100" cy="200"/><xdr:graphicFrame><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="Chart 1"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm/><a:graphic><a:graphicData uri="c"><c:chart r:id="rId1"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor><xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>0</xdr:col><xdr:colOff>1</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>2</xdr:rowOff></xdr:from><xdr:to><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>4</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="3" name="Picture 2" descr="logo"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId2"/></xdr:blipFill><xdr:spPr/></xdr:pic><xdr:clientData fLocksWithSheet="0"/></xdr:twoCellAnchor></xdr:wsDr>"#;
        let root = el(xml);
        let resolve = |id: &str| Some(format!("/part/{id}"));
        let read = parse_drawing(&root, &resolve);
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].kind, DrawingKind::Chart);
        assert_eq!(read[0].reference, "/part/rId1");
        assert_eq!(read[1].kind, DrawingKind::Picture);
        assert_eq!(read[1].alt_text.as_deref(), Some("logo"));
        assert!(!read[1].locked);
        let mut moved = read.clone();
        moved.reverse();
        if let Anchor::OneCell { from, .. } = &mut moved[1].anchor {
            from.row = 9;
        }
        moved.pop();
        let mut issues = Vec::new();
        let patched = patch_drawing(
            &root,
            &read,
            &moved,
            &rustc_hash::FxHashMap::default(),
            &mut issues,
        );
        let again = parse_drawing(&patched, &resolve);
        assert_eq!(again, vec![moved[0].clone()]);
        assert!(issues.is_empty());
    }

    #[test]
    fn dynamic_metadata_is_found() {
        let root = el(DYNAMIC_METADATA
            .trim_start_matches(|c| c != '<')
            .split_once("?>")
            .map_or("", |x| x.1)
            .trim());
        let set = dynamic_cell_metadata(&root);
        assert!(set.contains(&1) && set.len() == 1);
    }

    #[test]
    fn themes_round_trip_through_parts() {
        let model = Theme {
            name: "Kiln".into(),
            colors: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            major_font: "Caladea".into(),
            minor_font: "Carlito".into(),
        };
        let bytes = write_theme(&model, None);
        let (back, _) = read_theme(&bytes).expect("theme");
        assert_eq!(back, model);
    }
}
