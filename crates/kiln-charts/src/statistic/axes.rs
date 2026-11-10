// SPDX-License-Identifier: AGPL-3.0-or-later
//! The axes of a statistic chart. The file keeps them as `cx:axis` elements of the plot area
//! (`cx:catScaling` for the category axis, `cx:valScaling` for a value axis), which Kiln carries
//! as they are. Format Axis and Chart Elements edit them there, the Format pane reads them from
//! there, and [`super::derive`] draws the classic chart with them.

use kiln_ooxml::xml::{XmlElement, format_number};
use kiln_protocol::commands::ChartElementKind;

use crate::model::*;
use crate::read::{read_num_fmt, read_sp_pr, read_tx_pr};

const VAL: &str = "cx:valScaling";
const CAT: &str = "cx:catScaling";

/// `CT_Axis`'s children in schema order.
const ORDER: &[&str] = &[
    "cx:catScaling",
    "cx:valScaling",
    "cx:title",
    "cx:units",
    "cx:majorGridlines",
    "cx:minorGridlines",
    "cx:majorTickMarks",
    "cx:minorTickMarks",
    "cx:tickLabels",
    "cx:numFmt",
    "cx:spPr",
    "cx:txPr",
    "cx:extLst",
];

/// The statistic layout of `cs`, if it is a statistic chart.
#[must_use]
pub(crate) fn layout_of(cs: &ChartSpace) -> Option<StatLayout> {
    match cs.chart.plot_area.groups.first()?.kind {
        GroupKind::Statistic { layout } => Some(layout),
        _ => None,
    }
}

/// The kept `cx:axis` that plays a role: the category axis, or the first (primary) or second
/// (secondary) value axis.
#[must_use]
pub(crate) fn axis(cs: &ChartSpace, value: bool, secondary: bool) -> Option<&XmlElement> {
    let scaling = if value { VAL } else { CAT };
    cs.chart
        .plot_area
        .extras
        .elements()
        .filter(|e| e.name == "cx:axis" && e.child(scaling).is_some())
        .nth(usize::from(secondary))
}

fn axis_mut(cs: &mut ChartSpace, value: bool, secondary: bool) -> Option<&mut XmlElement> {
    layout_of(cs)?;
    if !cs
        .chart
        .plot_area
        .extras
        .elements()
        .any(|e| e.name == "cx:axis")
    {
        // A file without axes gets Excel's.
        crate::chartex::stat_defaults(cs);
    }
    let scaling = if value { VAL } else { CAT };
    cs.chart
        .plot_area
        .extras
        .elements_mut()
        .filter(|e| e.name == "cx:axis" && e.child(scaling).is_some())
        .nth(usize::from(secondary))
}

/// Whether the horizontal axis is the value axis (the funnel's bars run across).
fn across(cs: &ChartSpace) -> bool {
    layout_of(cs) == Some(StatLayout::Funnel)
}

/// Puts `child` into `axis` in schema order (replacing one of the same name).
fn put(axis: &mut XmlElement, child: XmlElement) {
    let i = ORDER
        .iter()
        .position(|n| *n == child.name)
        .map_or(ORDER.len(), |i| i + 1);
    axis.upsert_child(child, &ORDER[i..]);
}

/// Format Axis on a statistic chart: bounds and major unit of a value axis, and the number
/// format of the labels. These charts have no logarithmic scale and no reversed order.
///
/// `min`, `max` and `unit`: `None` leaves the value, `Some(None)` makes it automatic.
///
/// # Errors
///
/// A message when the chart has no such axis or the option does not exist on it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn format_axis(
    cs: &mut ChartSpace,
    value: bool,
    secondary: bool,
    min: Option<Option<f64>>,
    max: Option<Option<f64>>,
    unit: Option<Option<f64>>,
    log: Option<Option<f64>>,
    reversed: Option<bool>,
    number_format: Option<&str>,
) -> Result<(), String> {
    let role = match (value, secondary) {
        (true, false) => "value",
        (true, true) => "secondary value",
        (false, false) => "category",
        (false, true) => "secondary category",
    };
    let a =
        axis_mut(cs, value, secondary).ok_or_else(|| format!("the chart has no {role} axis"))?;
    if matches!(log, Some(Some(_))) {
        return Err("this chart's axes have no logarithmic scale".into());
    }
    if reversed == Some(true) {
        return Err("this chart's axes cannot be put in reverse order".into());
    }
    if value {
        let Some(sc) = a.child_mut(VAL) else {
            return Err(format!("the chart has no {role} axis"));
        };
        let lo = min.unwrap_or_else(|| sc.attr_num("min"));
        let hi = max.unwrap_or_else(|| sc.attr_num("max"));
        if let (Some(lo), Some(hi)) = (lo, hi)
            && lo >= hi
        {
            return Err(format!(
                "the minimum ({lo}) must be below the maximum ({hi})"
            ));
        }
        sc.set_attr("min", lo.map(format_number));
        sc.set_attr("max", hi.map(format_number));
        if let Some(u) = unit {
            sc.set_attr("majorUnit", u.map(format_number));
        }
    } else if [min, max, unit].iter().any(|b| matches!(b, Some(Some(_)))) {
        return Err("the category axis has no bounds or units".into());
    }
    if let Some(code) = number_format {
        let (code, linked) = if code.is_empty() {
            ("General", "1")
        } else {
            (code, "0")
        };
        put(
            a,
            XmlElement::new("cx:numFmt")
                .with_attr("formatCode", code)
                .with_attr("sourceLinked", linked),
        );
    }
    Ok(())
}

/// Chart Elements on a statistic chart's axes, gridlines and axis titles. Returns whether
/// `element` is one of those (otherwise the caller handles it).
pub(crate) fn set_element(cs: &mut ChartSpace, element: ChartElementKind, on: bool) -> bool {
    if layout_of(cs).is_none() {
        return false;
    }
    let across = across(cs);
    // (value axis, what to change)
    let (value, part) = match element {
        ChartElementKind::PrimaryHorizontalAxis => (across, "axis"),
        ChartElementKind::PrimaryVerticalAxis => (!across, "axis"),
        ChartElementKind::PrimaryHorizontalAxisTitle => (across, "cx:title"),
        ChartElementKind::PrimaryVerticalAxisTitle => (!across, "cx:title"),
        ChartElementKind::MajorGridlines => (true, "cx:majorGridlines"),
        ChartElementKind::MinorGridlines => (true, "cx:minorGridlines"),
        ChartElementKind::CategoryGridlines => (false, "cx:majorGridlines"),
        _ => return false,
    };
    let Some(a) = axis_mut(cs, value, false) else {
        return true;
    };
    match (part, on) {
        ("axis", _) => a.set_attr("hidden", (!on).then_some("1")),
        ("cx:title", true) => {
            if a.child("cx:title").is_none() {
                put(
                    a,
                    XmlElement::new("cx:title").with_child(
                        XmlElement::new("cx:tx").with_child(
                            XmlElement::new("cx:txData")
                                .with_child(XmlElement::new("cx:v").with_text("Axis Title")),
                        ),
                    ),
                );
            }
        }
        (name, true) => {
            if a.child(name).is_none() {
                put(a, XmlElement::new(name));
            }
        }
        (name, false) => {
            a.remove_children(name);
        }
    }
    true
}

/// Whether an element of the Chart Elements list is shown on a statistic chart, for the axis
/// ones (`None` for the others or another chart).
#[must_use]
pub(crate) fn element_on(cs: &ChartSpace, element: ChartElementKind) -> Option<bool> {
    layout_of(cs)?;
    let across = across(cs);
    let shown =
        |value: bool| axis(cs, value, false).is_some_and(|a| a.attr_bool("hidden") != Some(true));
    let has =
        |value: bool, name: &str| axis(cs, value, false).is_some_and(|a| a.child(name).is_some());
    Some(match element {
        ChartElementKind::PrimaryHorizontalAxis => shown(across),
        ChartElementKind::PrimaryVerticalAxis => shown(!across),
        ChartElementKind::PrimaryHorizontalAxisTitle => has(across, "cx:title"),
        ChartElementKind::PrimaryVerticalAxisTitle => has(!across, "cx:title"),
        ChartElementKind::MajorGridlines => has(true, "cx:majorGridlines"),
        ChartElementKind::MinorGridlines => has(true, "cx:minorGridlines"),
        ChartElementKind::CategoryGridlines => has(false, "cx:majorGridlines"),
        _ => return None,
    })
}

/// The bounds the file fixes on a value axis: minimum, maximum and major unit.
#[must_use]
pub(crate) fn fixed_bounds(
    cs: &ChartSpace,
    secondary: bool,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    axis(cs, true, secondary)
        .and_then(|a| a.child(VAL))
        .map_or((None, None, None), |s| {
            (
                s.attr_num("min"),
                s.attr_num("max"),
                s.attr_num("majorUnit"),
            )
        })
}

/// The number format the file gives an axis's labels (not linked to the data).
#[must_use]
pub(crate) fn number_format(cs: &ChartSpace, value: bool, secondary: bool) -> Option<String> {
    axis(cs, value, secondary)
        .and_then(|a| a.child("cx:numFmt"))
        .map(read_num_fmt)
        .filter(|f| f.source_linked != Some(true))
        .map(|f| f.code)
}

/// Gives the axes of `out` (the classic chart that draws `cs`) what the file sets on `cs`'s
/// axes: hidden, title, gridlines, labels, bounds, number format, line and text.
pub(crate) fn apply(cs: &ChartSpace, out: &mut ChartSpace) {
    use crate::edit::{CAT_AX, VAL_AX, VAL_AX2};
    for a in &mut out.chart.plot_area.axes {
        let (value, secondary) = match a.id {
            CAT_AX => (false, false),
            VAL_AX => (true, false),
            VAL_AX2 => (true, true),
            _ => continue,
        };
        let Some(el) = axis(cs, value, secondary) else {
            continue;
        };
        a.delete = Some(el.attr_bool("hidden") == Some(true));
        if let Some(t) = el.child("cx:title") {
            a.title = Some(crate::chartex::read_title(t));
        }
        a.major_gridlines = el
            .child("cx:majorGridlines")
            .map(|g| read_sp_pr(g.child("cx:spPr")));
        a.minor_gridlines = el
            .child("cx:minorGridlines")
            .map(|g| read_sp_pr(g.child("cx:spPr")));
        if el.child("cx:tickLabels").is_none() {
            a.tick_label_pos = Some(TickLabelPos::None);
        } else if !value {
            // The categories are named along the bottom (or the left), wherever zero is.
            a.tick_label_pos = Some(TickLabelPos::Low);
        }
        if let Some(s) = el.child(VAL) {
            if let Some(v) = s.attr_num("min") {
                a.scaling.min = Some(v);
            }
            if let Some(v) = s.attr_num("max") {
                a.scaling.max = Some(v);
            }
            if let Some(v) = s.attr_num("majorUnit") {
                a.major_unit = Some(v);
            }
            if let Some(v) = s.attr_num("minorUnit") {
                a.minor_unit = Some(v);
            }
        }
        if let Some(n) = el.child("cx:numFmt") {
            a.num_fmt = Some(read_num_fmt(n));
        }
        if let Some(sp) = el.child("cx:spPr") {
            a.shape = read_sp_pr(Some(sp));
        }
        if let Some(tx) = el.child("cx:txPr") {
            a.text = read_tx_pr(Some(tx));
        }
    }
}
