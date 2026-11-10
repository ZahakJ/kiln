// SPDX-License-Identifier: AGPL-3.0-or-later
//! Structural facts about a chart for the fidelity scorer (`kiln-testing`): one string per modelled
//! node or attribute, so a round trip that loses or changes anything lowers the multiset overlap.

use std::fmt::Write as _;

use kiln_ooxml::xml::{XmlElement, format_number};

use crate::model::*;

fn opt<T: std::fmt::Debug>(v: &Option<T>) -> String {
    match v {
        Some(v) => format!("{v:?}"),
        None => "-".into(),
    }
}

fn shape(prefix: &str, sp: &ShapeProps, out: &mut Vec<String>) {
    if sp.is_empty() {
        return;
    }
    out.push(format!("{prefix}/spPr@fill={}", opt(&sp.fill)));
    out.push(format!("{prefix}/spPr@line={}", opt(&sp.line)));
}

fn text(prefix: &str, t: &TextProps, out: &mut Vec<String>) {
    if t.is_empty() {
        return;
    }
    out.push(format!(
        "{prefix}/txPr@sz={}@b={}@rot={}",
        opt(&t.run.size),
        opt(&t.run.bold),
        opt(&t.rotation)
    ));
}

fn extras(prefix: &str, ex: &Extras, out: &mut Vec<String>) {
    for el in ex.elements() {
        out.push(format!("{prefix}/{}#{}", el.name, element_digest(el)));
    }
}

/// A short, order-sensitive digest of an element (name, attributes, children, text).
fn element_digest(el: &XmlElement) -> String {
    let mut s = String::new();
    digest_into(el, &mut s, 0);
    // Keep facts short: long kept elements contribute their size and a hash.
    if s.len() > 120 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in s.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        return format!("{}b:{h:016x}", s.len());
    }
    s
}

fn digest_into(el: &XmlElement, out: &mut String, depth: usize) {
    if depth > 64 {
        return;
    }
    let _ = write!(out, "<{}", el.name);
    for (k, v) in el.attrs.iter() {
        if k.starts_with("xmlns") {
            continue;
        }
        let _ = write!(out, " {k}={v}");
    }
    out.push('>');
    for c in &el.children {
        match c {
            kiln_ooxml::xml::XmlNode::Element(e) => digest_into(e, out, depth + 1),
            kiln_ooxml::xml::XmlNode::Text(t) => out.push_str(&t.text),
            _ => {}
        }
    }
}

fn data(prefix: &str, d: &DataSource, out: &mut Vec<String>) {
    match d {
        DataSource::Num(n) => {
            out.push(format!(
                "{prefix}@num f={} fmt={}",
                opt(&n.formula),
                opt(&n.format_code)
            ));
            for (i, p) in n.points.iter().enumerate() {
                if let Some(v) = p {
                    out.push(format!("{prefix}/pt[{i}]={}", format_number(*v)));
                }
            }
        }
        DataSource::Str(s) => {
            out.push(format!("{prefix}@str f={}", opt(&s.formula)));
            for (i, p) in s.points.iter().enumerate() {
                if let Some(v) = p {
                    out.push(format!("{prefix}/pt[{i}]={v}"));
                }
            }
        }
        DataSource::MultiLevel(m) => {
            out.push(format!("{prefix}@multi f={}", m.formula));
            for (l, level) in m.levels.iter().enumerate() {
                for (i, p) in level.iter().enumerate() {
                    if let Some(v) = p {
                        out.push(format!("{prefix}/lvl[{l}]/pt[{i}]={v}"));
                    }
                }
            }
        }
    }
}

fn labels(prefix: &str, d: &DataLabels, out: &mut Vec<String>) {
    out.push(format!(
        "{prefix}/dLbls@del={}@val={}@cat={}@ser={}@pct={}@bub={}@key={}@pos={}@sep={}",
        opt(&d.delete),
        opt(&d.show_value),
        opt(&d.show_category),
        opt(&d.show_series),
        opt(&d.show_percent),
        opt(&d.show_bubble_size),
        opt(&d.show_legend_key),
        opt(&d.position),
        opt(&d.separator)
    ));
    if let Some(f) = &d.num_fmt {
        out.push(format!("{prefix}/dLbls/numFmt={f:?}"));
    }
    for p in &d.point_labels {
        out.push(format!("{prefix}/dLbls/dLbl#{}", element_digest(p)));
    }
    shape(&format!("{prefix}/dLbls"), &d.shape, out);
    text(&format!("{prefix}/dLbls"), &d.text, out);
    extras(&format!("{prefix}/dLbls"), &d.extras, out);
}

fn title(prefix: &str, t: &Title, out: &mut Vec<String>) {
    out.push(format!(
        "{prefix}/title@text={}@overlay={}",
        opt(&t.text.as_ref().and_then(TitleText::plain)),
        opt(&t.overlay)
    ));
    if let Some(TitleText::Ref { formula, .. }) = &t.text {
        out.push(format!("{prefix}/title@f={formula}"));
    }
    shape(&format!("{prefix}/title"), &t.shape, out);
    text(&format!("{prefix}/title"), &t.text_props, out);
    extras(&format!("{prefix}/title"), &t.extras, out);
}

fn marker(prefix: &str, m: &Marker, out: &mut Vec<String>) {
    out.push(format!(
        "{prefix}/marker@symbol={}@size={}",
        opt(&m.symbol),
        opt(&m.size)
    ));
    shape(&format!("{prefix}/marker"), &m.shape, out);
}

fn group_kind(k: &GroupKind) -> String {
    match k {
        GroupKind::Other(el) => format!("Other({})", element_digest(el)),
        other => format!("{other:?}"),
    }
}

/// The structural facts of a chart part.
#[must_use]
pub fn chart_facts(cs: &ChartSpace) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!(
        "chartSpace@format={:?}@date1904={}@lang={}@rounded={}@style={}",
        cs.format,
        opt(&cs.date1904),
        opt(&cs.lang),
        opt(&cs.rounded_corners),
        opt(&cs.style)
    ));
    if let Some(e) = &cs.external_data {
        out.push(format!("externalData@{}@{}", e.rel_id, opt(&e.auto_update)));
    }
    shape("chartSpace", &cs.shape, &mut out);
    text("chartSpace", &cs.text, &mut out);
    extras("chartSpace", &cs.extras, &mut out);
    let c = &cs.chart;
    if let Some(t) = &c.title {
        title("chart", t, &mut out);
    }
    out.push(format!(
        "chart@autoTitleDeleted={}@plotVisOnly={}@blanks={}",
        opt(&c.auto_title_deleted),
        opt(&c.plot_vis_only),
        opt(&c.disp_blanks_as)
    ));
    extras("chart", &c.extras, &mut out);
    let pa = &c.plot_area;
    if let Some(l) = &pa.layout {
        out.push(format!(
            "plotArea/layout@{}@{}@{}@{}",
            opt(&l.x),
            opt(&l.y),
            opt(&l.w),
            opt(&l.h)
        ));
    }
    shape("plotArea", &pa.shape, &mut out);
    extras("plotArea", &pa.extras, &mut out);
    for (gi, g) in pa.groups.iter().enumerate() {
        let gp = format!("group[{gi}]");
        out.push(format!("{gp}={}", group_kind(&g.kind)));
        out.push(format!(
            "{gp}@vary={}@axes={:?}",
            opt(&g.vary_colors),
            g.axis_ids
        ));
        if let Some(d) = &g.data_labels {
            labels(&gp, d, &mut out);
        }
        extras(&gp, &g.extras, &mut out);
        for (si, s) in g.series.iter().enumerate() {
            let sp = format!("{gp}/ser[{si}]");
            out.push(format!(
                "{sp}@idx={}@order={}@name={}",
                s.idx,
                s.order,
                opt(&s.name)
            ));
            out.push(format!(
                "{sp}@inv={}@expl={}@smooth={}@b3d={}",
                opt(&s.invert_if_negative),
                opt(&s.explosion),
                opt(&s.smooth),
                opt(&s.bubble_3d)
            ));
            shape(&sp, &s.shape, &mut out);
            if let Some(m) = &s.marker {
                marker(&sp, m, &mut out);
            }
            for p in &s.points {
                let pp = format!("{sp}/dPt[{}]", p.idx);
                out.push(format!(
                    "{pp}@inv={}@expl={}@b3d={}",
                    opt(&p.invert_if_negative),
                    opt(&p.explosion),
                    opt(&p.bubble_3d)
                ));
                if let Some(m) = &p.marker {
                    marker(&pp, m, &mut out);
                }
                shape(&pp, &p.shape, &mut out);
            }
            if let Some(d) = &s.data_labels {
                labels(&sp, d, &mut out);
            }
            for (ti, t) in s.trendlines.iter().enumerate() {
                out.push(format!(
                    "{sp}/trendline[{ti}]@{:?}@{}@{}@{}@{}@{}",
                    t.kind,
                    opt(&t.order),
                    opt(&t.period),
                    opt(&t.name),
                    opt(&t.show_equation),
                    opt(&t.show_r_squared)
                ));
            }
            if let Some(d) = &s.categories {
                data(&format!("{sp}/cat"), d, &mut out);
            }
            if let Some(d) = &s.values {
                data(&format!("{sp}/val"), d, &mut out);
            }
            if let Some(d) = &s.bubble_sizes {
                data(&format!("{sp}/size"), d, &mut out);
            }
            extras(&sp, &s.extras, &mut out);
        }
    }
    for a in &pa.axes {
        let ap = format!("axis[{}]", a.id);
        out.push(format!(
            "{ap}={:?}@pos={}@del={}@cross={}@crosses={}@between={}",
            a.kind,
            opt(&a.position),
            opt(&a.delete),
            a.cross_axis,
            opt(&a.crosses),
            opt(&a.cross_between)
        ));
        out.push(format!(
            "{ap}/scaling@log={}@rev={}@min={}@max={}",
            opt(&a.scaling.log_base),
            opt(&a.scaling.reversed),
            opt(&a.scaling.min),
            opt(&a.scaling.max)
        ));
        out.push(format!(
            "{ap}@major={}@minor={}@lbl={}@mu={}@nu={}@fmt={}",
            opt(&a.major_tick),
            opt(&a.minor_tick),
            opt(&a.tick_label_pos),
            opt(&a.major_unit),
            opt(&a.minor_unit),
            opt(&a.num_fmt)
        ));
        out.push(format!(
            "{ap}@auto={}@algn={}@off={}@skip={}/{}@multi={}@time={}/{}/{}",
            opt(&a.auto),
            opt(&a.label_align),
            opt(&a.label_offset),
            opt(&a.tick_label_skip),
            opt(&a.tick_mark_skip),
            opt(&a.no_multi_level),
            opt(&a.base_time_unit),
            opt(&a.major_time_unit),
            opt(&a.minor_time_unit)
        ));
        if let Some(g) = &a.major_gridlines {
            out.push(format!("{ap}/majorGridlines"));
            shape(&format!("{ap}/majorGridlines"), g, &mut out);
        }
        if let Some(g) = &a.minor_gridlines {
            out.push(format!("{ap}/minorGridlines"));
            shape(&format!("{ap}/minorGridlines"), g, &mut out);
        }
        if let Some(t) = &a.title {
            title(&ap, t, &mut out);
        }
        shape(&ap, &a.shape, &mut out);
        text(&ap, &a.text, &mut out);
        extras(&ap, &a.extras, &mut out);
    }
    if let Some(l) = &c.legend {
        out.push(format!(
            "legend@pos={}@overlay={}",
            opt(&l.position),
            opt(&l.overlay)
        ));
        for e in &l.entries {
            out.push(format!("legend/legendEntry#{}", element_digest(e)));
        }
        shape("legend", &l.shape, &mut out);
        text("legend", &l.text, &mut out);
        extras("legend", &l.extras, &mut out);
    }
    out
}

/// Multiset overlap of two fact lists: `2·|A ∩ B| / (|A| + |B|)` (1 when both are empty).
#[must_use]
pub fn overlap(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let mut counts: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
    for f in a {
        *counts.entry(f).or_default() += 1;
    }
    let mut common = 0usize;
    for f in b {
        if let Some(c) = counts.get_mut(f.as_str())
            && *c > 0
        {
            *c -= 1;
            common += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    {
        2.0 * common as f64 / (a.len() + b.len()) as f64
    }
}
