// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML shape geometry (ECMA-376 Part 1 §20.1.9): guide formulas, adjust values, paths, the
//! text rectangle, connection sites and adjust handles.
//!
//! One engine serves both `a:prstGeom` (through the transcribed preset table in
//! [`super::presets`]) and `a:custGeom`. A [`GeometryDef`] is the XML-level model (formulas kept as
//! written, so it round-trips); [`GeometryDef::compile`] turns it into a [`CompiledGeometry`] whose
//! guides are resolved to slots, so evaluating a shape at a size is a straight pass over an array
//! of numbers. The result is a [`ResolvedGeometry`] whose paths convert to `kurbo` paths (arcs as
//! cubic Béziers) or to SVG path data.

use std::collections::HashMap;
use std::f64::consts::PI;

use kurbo::{Arc, BezPath, Point, Rect, Vec2};

use crate::st::PathFillMode;
use crate::units::round_to;
use crate::xml::{XmlElement, format_number};

/// Why a geometry cannot be evaluated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GeometryError {
    /// A formula or path refers to a guide that is not defined before it.
    #[error("unknown guide \"{0}\"")]
    UnknownGuide(String),
    /// A formula uses an operator outside ECMA-376 §20.1.9.11.
    #[error("unknown formula operator \"{0}\"")]
    UnknownOperator(String),
    /// No preset geometry has this name.
    #[error("unknown preset geometry \"{0}\"")]
    UnknownPreset(String),
    /// A geometry with more guides than the engine addresses (65 535).
    #[error("too many guides")]
    TooManyGuides,
}

/// A guide or adjust value: `adj = "val 25000"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideDef {
    /// The guide name.
    pub name: String,
    /// The formula, as written.
    pub fmla: String,
}

impl GuideDef {
    /// A guide.
    #[must_use]
    pub fn new(name: &str, fmla: &str) -> Self {
        Self {
            name: name.to_owned(),
            fmla: fmla.to_owned(),
        }
    }
}

/// A path command; coordinates are guide names or numbers, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathCommand {
    /// `a:moveTo`.
    MoveTo(String, String),
    /// `a:lnTo`.
    LineTo(String, String),
    /// `a:arcTo`: radii, start angle and sweep angle.
    ArcTo {
        /// Horizontal radius.
        wr: String,
        /// Vertical radius.
        hr: String,
        /// Start angle (60 000ths of a degree).
        st_ang: String,
        /// Sweep angle.
        sw_ang: String,
    },
    /// `a:quadBezTo`: control point, end point.
    QuadBezTo([String; 4]),
    /// `a:cubicBezTo`: two control points, end point.
    CubicBezTo([String; 6]),
    /// `a:close`.
    Close,
}

/// `a:path`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathDef {
    /// Coordinate-space width; coordinates scale by shape width / `w`.
    pub w: Option<f64>,
    /// Coordinate-space height.
    pub h: Option<f64>,
    /// Fill mode (`norm` when absent).
    pub fill: Option<PathFillMode>,
    /// `false` = do not stroke this path.
    pub stroke: Option<bool>,
    /// `extrusionOk`.
    pub extrusion_ok: Option<bool>,
    /// The commands.
    pub cmds: Vec<PathCommand>,
}

/// An adjust handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdjustHandleDef {
    /// `a:ahXY`.
    Xy {
        /// Adjust value driven horizontally.
        gd_ref_x: Option<String>,
        /// Horizontal minimum.
        min_x: Option<String>,
        /// Horizontal maximum.
        max_x: Option<String>,
        /// Adjust value driven vertically.
        gd_ref_y: Option<String>,
        /// Vertical minimum.
        min_y: Option<String>,
        /// Vertical maximum.
        max_y: Option<String>,
        /// Handle position.
        pos: (String, String),
    },
    /// `a:ahPolar`.
    Polar {
        /// Adjust value driven by the radius.
        gd_ref_r: Option<String>,
        /// Radius minimum.
        min_r: Option<String>,
        /// Radius maximum.
        max_r: Option<String>,
        /// Adjust value driven by the angle.
        gd_ref_ang: Option<String>,
        /// Angle minimum.
        min_ang: Option<String>,
        /// Angle maximum.
        max_ang: Option<String>,
        /// Handle position.
        pos: (String, String),
    },
}

/// A connection site: angle and position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionSiteDef {
    /// The angle at which a connector leaves the site.
    pub ang: String,
    /// X.
    pub x: String,
    /// Y.
    pub y: String,
}

/// A geometry definition: the shape of `a:custGeom` and of every preset.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GeometryDef {
    /// Adjust values (`a:avLst`).
    pub av: Vec<GuideDef>,
    /// Guides (`a:gdLst`).
    pub gd: Vec<GuideDef>,
    /// Adjust handles (`a:ahLst`).
    pub ah: Vec<AdjustHandleDef>,
    /// Connection sites (`a:cxnLst`).
    pub cxn: Vec<ConnectionSiteDef>,
    /// The text rectangle `[l, t, r, b]`; the whole shape when absent.
    pub rect: Option<[String; 4]>,
    /// Paths (`a:pathLst`).
    pub path: Vec<PathDef>,
}

// ----------------------------------------------------------------- engine

const DEG: f64 = 60_000.0;

fn to_rad(a: f64) -> f64 {
    (a / DEG).to_radians()
}

fn to_ang(rad: f64) -> f64 {
    rad.to_degrees() * DEG
}

/// The built-in guide names (ECMA-376 §20.1.9.11), in slot order.
const BUILTINS: &[&str] = &[
    "w", "h", "l", "t", "r", "b", "hc", "vc", "ss", "ls", "cd2", "cd3", "cd4", "cd8", "3cd4",
    "3cd8", "5cd8", "7cd8", "wd2", "hd2", "wd3", "hd3", "wd4", "hd4", "wd5", "hd5", "wd6", "hd6",
    "wd8", "hd8", "wd10", "hd10", "wd12", "hd12", "wd32", "hd32", "ssd2", "ssd4", "ssd6", "ssd8",
    "ssd16", "ssd32",
];

fn fill_builtins(values: &mut [f64], w: f64, h: f64) {
    let ss = w.min(h);
    let ls = w.max(h);
    let fixed = [
        w,
        h,
        0.0,
        0.0,
        w,
        h,
        w / 2.0,
        h / 2.0,
        ss,
        ls,
        10_800_000.0,
        7_200_000.0,
        5_400_000.0,
        2_700_000.0,
        16_200_000.0,
        8_100_000.0,
        13_500_000.0,
        18_900_000.0,
    ];
    values[..fixed.len()].copy_from_slice(&fixed);
    let mut i = fixed.len();
    for d in [2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 32.0] {
        values[i] = w / d;
        values[i + 1] = h / d;
        i += 2;
    }
    for d in [2.0, 4.0, 6.0, 8.0, 16.0, 32.0] {
        values[i] = ss / d;
        i += 1;
    }
}

/// The built-in guide values for a shape of size `w` × `h`, by name.
#[must_use]
pub fn builtin_guides(w: f64, h: f64) -> HashMap<&'static str, f64> {
    let mut values = vec![0.0; BUILTINS.len()];
    fill_builtins(&mut values, w, h);
    BUILTINS.iter().copied().zip(values).collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Operand {
    Lit(f64),
    Slot(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Val,
    MulDiv,
    AddSub,
    AddDiv,
    IfElse,
    Abs,
    At2,
    Cat2,
    Sat2,
    Cos,
    Sin,
    Tan,
    Max,
    Min,
    Mod,
    Pin,
    Sqrt,
}

impl Op {
    fn parse(s: &str) -> Option<(Op, usize)> {
        Some(match s {
            "val" => (Op::Val, 1),
            "*/" => (Op::MulDiv, 3),
            "+-" => (Op::AddSub, 3),
            "+/" => (Op::AddDiv, 3),
            "?:" => (Op::IfElse, 3),
            "abs" => (Op::Abs, 1),
            "at2" => (Op::At2, 2),
            "cat2" => (Op::Cat2, 3),
            "sat2" => (Op::Sat2, 3),
            "cos" => (Op::Cos, 2),
            "sin" => (Op::Sin, 2),
            "tan" => (Op::Tan, 2),
            "max" => (Op::Max, 2),
            "min" => (Op::Min, 2),
            "mod" => (Op::Mod, 3),
            "pin" => (Op::Pin, 3),
            "sqrt" => (Op::Sqrt, 1),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Formula {
    op: Op,
    args: [Operand; 3],
}

impl Formula {
    /// Evaluates the formula. Results that are not finite (a division by zero or the square
    /// root of a negative number, which degenerate shapes such as zero-height arrows produce)
    /// are 0, so no NaN or infinity ever reaches a path.
    fn eval(&self, v: &[f64]) -> f64 {
        let r = self.eval_raw(v);
        if r.is_finite() { r } else { 0.0 }
    }

    fn eval_raw(&self, v: &[f64]) -> f64 {
        let a = |i: usize| match self.args[i] {
            Operand::Lit(x) => x,
            Operand::Slot(s) => v[usize::from(s)],
        };
        match self.op {
            Op::Val => a(0),
            Op::MulDiv => a(0) * a(1) / a(2),
            Op::AddSub => a(0) + a(1) - a(2),
            Op::AddDiv => (a(0) + a(1)) / a(2),
            Op::IfElse => {
                if a(0) > 0.0 {
                    a(1)
                } else {
                    a(2)
                }
            }
            Op::Abs => a(0).abs(),
            Op::At2 => to_ang(a(1).atan2(a(0))),
            Op::Cat2 => a(0) * a(2).atan2(a(1)).cos(),
            Op::Sat2 => a(0) * a(2).atan2(a(1)).sin(),
            Op::Cos => a(0) * to_rad(a(1)).cos(),
            Op::Sin => a(0) * to_rad(a(1)).sin(),
            Op::Tan => a(0) * to_rad(a(1)).tan(),
            Op::Max => a(0).max(a(1)),
            Op::Min => a(0).min(a(1)),
            Op::Mod => (a(0) * a(0) + a(1) * a(1) + a(2) * a(2)).sqrt(),
            Op::Pin => {
                let (lo, x, hi) = (a(0), a(1), a(2));
                if x < lo {
                    lo
                } else if x > hi {
                    hi
                } else {
                    x
                }
            }
            Op::Sqrt => a(0).sqrt(),
        }
    }
}

/// Name → slot while compiling: built-ins first, then each definition shadows earlier ones.
struct Scope {
    names: HashMap<String, u16>,
    count: usize,
}

impl Scope {
    fn new() -> Self {
        let mut names = HashMap::with_capacity(BUILTINS.len() + 64);
        for (i, n) in BUILTINS.iter().enumerate() {
            // BUILTINS has fewer than 65 536 entries.
            names.insert((*n).to_owned(), u16::try_from(i).unwrap_or(u16::MAX));
        }
        Self {
            names,
            count: BUILTINS.len(),
        }
    }

    fn define(&mut self, name: &str) -> Result<u16, GeometryError> {
        let slot = u16::try_from(self.count).map_err(|_| GeometryError::TooManyGuides)?;
        self.count += 1;
        self.names.insert(name.to_owned(), slot);
        Ok(slot)
    }

    fn operand(&self, v: &str) -> Result<Operand, GeometryError> {
        if let Some(&s) = self.names.get(v) {
            return Ok(Operand::Slot(s));
        }
        let t = v.trim();
        match t.parse::<f64>() {
            Ok(x) if !t.is_empty() && x.is_finite() => Ok(Operand::Lit(x)),
            _ => Err(GeometryError::UnknownGuide(v.to_owned())),
        }
    }

    fn formula(&self, fmla: &str) -> Result<Formula, GeometryError> {
        let mut parts = fmla.split_whitespace();
        let op_s = parts.next().unwrap_or("");
        let (op, arity) =
            Op::parse(op_s).ok_or_else(|| GeometryError::UnknownOperator(op_s.to_owned()))?;
        let mut args = [Operand::Lit(0.0); 3];
        for slot in args.iter_mut().take(arity) {
            *slot = self.operand(parts.next().unwrap_or(""))?;
        }
        Ok(Formula { op, args })
    }

    fn point(&self, x: &str, y: &str) -> Result<[Operand; 2], GeometryError> {
        Ok([self.operand(x)?, self.operand(y)?])
    }

    fn opt(&self, v: Option<&String>) -> Result<Option<Operand>, GeometryError> {
        v.map(|s| self.operand(s)).transpose()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum CompiledCmd {
    MoveTo([Operand; 2]),
    LineTo([Operand; 2]),
    ArcTo([Operand; 4]),
    QuadTo([Operand; 4]),
    CubicTo([Operand; 6]),
    Close,
}

#[derive(Debug, Clone, PartialEq)]
struct CompiledPath {
    w: Option<f64>,
    h: Option<f64>,
    fill: PathFillMode,
    stroke: bool,
    extrusion_ok: bool,
    cmds: Vec<CompiledCmd>,
}

/// Which kind of adjust handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    /// Moves in x and/or y.
    Xy,
    /// Moves in radius and/or angle.
    Polar,
}

#[derive(Debug, Clone, PartialEq)]
struct CompiledHandle {
    kind: HandleKind,
    refs: Vec<String>,
    /// minX/maxX/minY/maxY (xy) or minR/maxR/minAng/maxAng (polar).
    limits: [Option<Operand>; 4],
    pos: [Operand; 2],
}

/// A geometry with its guides resolved to slots, ready to evaluate at any size.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledGeometry {
    slot_count: usize,
    /// (name, slot, default formula) of each adjust value.
    adjust: Vec<(String, u16, Formula)>,
    guides: Vec<(u16, Formula)>,
    /// Final name → slot table (for [`GuideValues::get`]).
    names: HashMap<String, u16>,
    paths: Vec<CompiledPath>,
    rect: Option<[Operand; 4]>,
    cxn: Vec<[Operand; 3]>,
    handles: Vec<CompiledHandle>,
}

impl GeometryDef {
    /// Resolves every guide reference, so the geometry can be evaluated cheaply at any size.
    ///
    /// # Errors
    /// [`GeometryError::UnknownGuide`] for a reference to a guide not defined before it, and
    /// [`GeometryError::UnknownOperator`] for a formula outside the specification.
    pub fn compile(&self) -> Result<CompiledGeometry, GeometryError> {
        let mut scope = Scope::new();
        let mut adjust = Vec::with_capacity(self.av.len());
        for g in &self.av {
            let f = scope.formula(&g.fmla)?;
            let slot = scope.define(&g.name)?;
            adjust.push((g.name.clone(), slot, f));
        }
        let mut guides = Vec::with_capacity(self.gd.len());
        for g in &self.gd {
            let f = scope.formula(&g.fmla)?;
            let slot = scope.define(&g.name)?;
            guides.push((slot, f));
        }
        let paths = self
            .path
            .iter()
            .map(|p| {
                let cmds = p
                    .cmds
                    .iter()
                    .map(|c| {
                        Ok(match c {
                            PathCommand::MoveTo(x, y) => CompiledCmd::MoveTo(scope.point(x, y)?),
                            PathCommand::LineTo(x, y) => CompiledCmd::LineTo(scope.point(x, y)?),
                            PathCommand::ArcTo {
                                wr,
                                hr,
                                st_ang,
                                sw_ang,
                            } => CompiledCmd::ArcTo([
                                scope.operand(wr)?,
                                scope.operand(hr)?,
                                scope.operand(st_ang)?,
                                scope.operand(sw_ang)?,
                            ]),
                            PathCommand::QuadBezTo(p) => {
                                let [a, b] = scope.point(&p[0], &p[1])?;
                                let [c, d] = scope.point(&p[2], &p[3])?;
                                CompiledCmd::QuadTo([a, b, c, d])
                            }
                            PathCommand::CubicBezTo(p) => {
                                let [a, b] = scope.point(&p[0], &p[1])?;
                                let [c, d] = scope.point(&p[2], &p[3])?;
                                let [e, f] = scope.point(&p[4], &p[5])?;
                                CompiledCmd::CubicTo([a, b, c, d, e, f])
                            }
                            PathCommand::Close => CompiledCmd::Close,
                        })
                    })
                    .collect::<Result<Vec<_>, GeometryError>>()?;
                Ok(CompiledPath {
                    w: p.w,
                    h: p.h,
                    fill: p.fill.unwrap_or(PathFillMode::Norm),
                    stroke: p.stroke.unwrap_or(true),
                    extrusion_ok: p.extrusion_ok.unwrap_or(true),
                    cmds,
                })
            })
            .collect::<Result<Vec<_>, GeometryError>>()?;
        let rect = self
            .rect
            .as_ref()
            .map(|r| {
                Ok::<_, GeometryError>([
                    scope.operand(&r[0])?,
                    scope.operand(&r[1])?,
                    scope.operand(&r[2])?,
                    scope.operand(&r[3])?,
                ])
            })
            .transpose()?;
        let cxn = self
            .cxn
            .iter()
            .map(|c| {
                Ok([
                    scope.operand(&c.ang)?,
                    scope.operand(&c.x)?,
                    scope.operand(&c.y)?,
                ])
            })
            .collect::<Result<Vec<_>, GeometryError>>()?;
        let handles = self
            .ah
            .iter()
            .map(|h| {
                Ok(match h {
                    AdjustHandleDef::Xy {
                        gd_ref_x,
                        min_x,
                        max_x,
                        gd_ref_y,
                        min_y,
                        max_y,
                        pos,
                    } => CompiledHandle {
                        kind: HandleKind::Xy,
                        refs: [gd_ref_x, gd_ref_y]
                            .into_iter()
                            .flatten()
                            .filter(|r| !r.is_empty())
                            .cloned()
                            .collect(),
                        limits: [
                            scope.opt(min_x.as_ref())?,
                            scope.opt(max_x.as_ref())?,
                            scope.opt(min_y.as_ref())?,
                            scope.opt(max_y.as_ref())?,
                        ],
                        pos: scope.point(&pos.0, &pos.1)?,
                    },
                    AdjustHandleDef::Polar {
                        gd_ref_r,
                        min_r,
                        max_r,
                        gd_ref_ang,
                        min_ang,
                        max_ang,
                        pos,
                    } => CompiledHandle {
                        kind: HandleKind::Polar,
                        refs: [gd_ref_r, gd_ref_ang]
                            .into_iter()
                            .flatten()
                            .filter(|r| !r.is_empty())
                            .cloned()
                            .collect(),
                        limits: [
                            scope.opt(min_r.as_ref())?,
                            scope.opt(max_r.as_ref())?,
                            scope.opt(min_ang.as_ref())?,
                            scope.opt(max_ang.as_ref())?,
                        ],
                        pos: scope.point(&pos.0, &pos.1)?,
                    },
                })
            })
            .collect::<Result<Vec<_>, GeometryError>>()?;
        Ok(CompiledGeometry {
            slot_count: scope.count,
            adjust,
            guides,
            names: scope.names,
            paths,
            rect,
            cxn,
            handles,
        })
    }

    /// Compiles and evaluates for a `w` × `h` shape; see [`CompiledGeometry::resolve`].
    ///
    /// # Errors
    /// As [`GeometryDef::compile`].
    pub fn resolve(
        &self,
        w: f64,
        h: f64,
        adjustments: &[(&str, f64)],
    ) -> Result<ResolvedGeometry, GeometryError> {
        Ok(self.compile()?.resolve(w, h, adjustments))
    }
}

/// Guide values of a geometry evaluated at one size.
#[derive(Debug, Clone, PartialEq)]
pub struct GuideValues<'g> {
    geometry: &'g CompiledGeometry,
    values: Vec<f64>,
}

impl GuideValues<'_> {
    /// The value of a guide, adjust value or built-in by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<f64> {
        self.geometry
            .names
            .get(name)
            .map(|&s| self.values[usize::from(s)])
    }
}

/// An evaluated path segment, in shape coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    /// Start a subpath.
    MoveTo(Point),
    /// Straight line.
    LineTo(Point),
    /// Quadratic Bézier: control, end.
    QuadTo(Point, Point),
    /// Cubic Bézier: two controls, end.
    CubicTo(Point, Point, Point),
    /// Elliptical arc from the current point (`a:arcTo`).
    ArcTo {
        /// Where the arc starts (the current point).
        from: Point,
        /// Horizontal radius.
        wr: f64,
        /// Vertical radius.
        hr: f64,
        /// Start angle, 60 000ths of a degree (DrawingML's visual angle).
        st_ang: f64,
        /// Sweep angle, 60 000ths of a degree.
        sw_ang: f64,
    },
    /// Close the subpath.
    Close,
}

/// Point on an ellipse at *visual* angle `ang` (60 000ths of a degree; the angle of the ray from
/// the centre), relative to the centre.
fn ellipse_point(wr: f64, hr: f64, ang: f64) -> Vec2 {
    let th = to_rad(ang);
    let t = (wr * th.sin()).atan2(hr * th.cos());
    Vec2::new(wr * t.cos(), hr * t.sin())
}

/// The parametric angle of the ellipse point at visual angle `ang`.
fn param_angle(wr: f64, hr: f64, ang: f64) -> f64 {
    let th = to_rad(ang);
    (wr * th.sin()).atan2(hr * th.cos())
}

/// A full turn in DrawingML angle units (60 000ths of a degree).
const FULL_TURN: f64 = DEG * 360.0;

/// Sanitises an arc's sweep angle: Office draws any `|swAng| >= 21 600 000` as a full ellipse, so
/// longer sweeps are clamped to one turn (keeping their direction) and a non-finite sweep draws
/// nothing. Without this a file could ask for an arbitrarily long arc and make rendering run for
/// ever.
fn clamp_sweep(sw: f64) -> f64 {
    if sw.is_finite() {
        sw.clamp(-FULL_TURN, FULL_TURN)
    } else {
        0.0
    }
}

/// Sanitises an arc's start angle: a non-finite angle is treated as 0.
fn finite_angle(a: f64) -> f64 {
    if a.is_finite() { a } else { 0.0 }
}

impl Segment {
    /// The arc's end point.
    fn arc_end(from: Point, wr: f64, hr: f64, st: f64, sw: f64) -> Point {
        let (st, sw) = (finite_angle(st), clamp_sweep(sw));
        let center = from - ellipse_point(wr, hr, st);
        center + ellipse_point(wr, hr, st + sw)
    }
}

/// An evaluated path.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPath {
    /// The segments.
    pub segments: Vec<Segment>,
    /// How the path is filled.
    pub fill: PathFillMode,
    /// Whether it is stroked.
    pub stroke: bool,
    /// Whether it may be extruded (3-D).
    pub extrusion_ok: bool,
    /// True when every subpath ends with a close.
    pub closed: bool,
}

fn fmt3(n: f64) -> String {
    format_number(round_to(n, 3))
}

impl ResolvedPath {
    /// SVG path data. Arcs become SVG `A` commands, split so no single command sweeps more than
    /// 180°; numbers are rounded to 3 decimals.
    #[must_use]
    pub fn to_svg(&self) -> String {
        let mut parts: Vec<String> = Vec::with_capacity(self.segments.len());
        let pt = |p: Point| format!("{} {}", fmt3(p.x), fmt3(p.y));
        for s in &self.segments {
            match *s {
                Segment::MoveTo(p) => parts.push(format!("M{}", pt(p))),
                Segment::LineTo(p) => parts.push(format!("L{}", pt(p))),
                Segment::QuadTo(c, p) => parts.push(format!("Q{} {}", pt(c), pt(p))),
                Segment::CubicTo(c1, c2, p) => {
                    parts.push(format!("C{} {} {}", pt(c1), pt(c2), pt(p)));
                }
                Segment::ArcTo {
                    from,
                    wr,
                    hr,
                    st_ang,
                    sw_ang,
                } => {
                    // `Segment` is public, so a caller-built arc is sanitised here as well.
                    let (st_ang, sw_ang) = (finite_angle(st_ang), clamp_sweep(sw_ang));
                    let center = from - ellipse_point(wr, hr, st_ang);
                    if wr == 0.0 || hr == 0.0 || sw_ang == 0.0 {
                        parts.push(format!(
                            "L{}",
                            pt(center + ellipse_point(wr, hr, st_ang + sw_ang))
                        ));
                        continue;
                    }
                    // SVG cannot draw a full ellipse in one arc command, so long sweeps split.
                    let steps = ((sw_ang.abs() / (DEG * 180.0) - 1e-9).ceil()).max(1.0);
                    let sweep_flag = u8::from(sw_ang > 0.0);
                    let seg = sw_ang / steps;
                    let large = u8::from(seg.abs() > DEG * 180.0);
                    let mut i = 1.0;
                    while i <= steps {
                        let end = center + ellipse_point(wr, hr, st_ang + seg * i);
                        parts.push(format!(
                            "A{} {} 0 {large} {sweep_flag} {}",
                            fmt3(wr),
                            fmt3(hr),
                            pt(end)
                        ));
                        i += 1.0;
                    }
                }
                Segment::Close => parts.push("Z".to_owned()),
            }
        }
        parts.join(" ")
    }

    /// The path as a `kurbo` Bézier path; arcs are approximated by cubics within `tolerance`.
    #[must_use]
    pub fn to_bez_path(&self, tolerance: f64) -> BezPath {
        let tolerance = tolerance.max(1e-9);
        let mut path = BezPath::new();
        // kurbo requires every subpath to start with a move; DrawingML (like SVG) continues from
        // the current point, so a missing move is supplied.
        let mut open = false;
        let mut cur = Point::ZERO;
        let mut start = Point::ZERO;
        let ensure = |path: &mut BezPath, open: &mut bool, at: Point| {
            if !*open {
                path.move_to(at);
                *open = true;
            }
        };
        for s in &self.segments {
            match *s {
                Segment::MoveTo(p) => {
                    path.move_to(p);
                    open = true;
                    start = p;
                    cur = p;
                }
                Segment::LineTo(p) => {
                    ensure(&mut path, &mut open, cur);
                    path.line_to(p);
                    cur = p;
                }
                Segment::QuadTo(c, p) => {
                    ensure(&mut path, &mut open, cur);
                    path.quad_to(c, p);
                    cur = p;
                }
                Segment::CubicTo(c1, c2, p) => {
                    ensure(&mut path, &mut open, cur);
                    path.curve_to(c1, c2, p);
                    cur = p;
                }
                Segment::ArcTo {
                    from,
                    wr,
                    hr,
                    st_ang,
                    sw_ang,
                } => {
                    ensure(&mut path, &mut open, from);
                    append_arc(&mut path, from, wr, hr, st_ang, sw_ang, tolerance);
                    cur = Segment::arc_end(from, wr, hr, st_ang, sw_ang);
                }
                Segment::Close => {
                    if open {
                        path.close_path();
                        open = false;
                    }
                    cur = start;
                }
            }
        }
        path
    }
}

fn append_arc(path: &mut BezPath, from: Point, wr: f64, hr: f64, st: f64, sw: f64, tol: f64) {
    // `Segment` is public, so a caller-built arc is sanitised here as well.
    let (st, sw) = (finite_angle(st), clamp_sweep(sw));
    let end = Segment::arc_end(from, wr, hr, st, sw);
    if wr == 0.0 || hr == 0.0 || sw == 0.0 || !end.is_finite() || !wr.is_finite() || !hr.is_finite()
    {
        if end.is_finite() {
            path.line_to(end);
        }
        return;
    }
    let center = from - ellipse_point(wr, hr, st);
    // Accumulate the parametric sweep over visual steps of at most 90°, where the mapping is
    // monotonic and the parametric step has the visual step's sign.
    let steps = (sw.abs() / (DEG * 90.0)).ceil().max(1.0);
    let seg = sw / steps;
    let mut prev = param_angle(wr, hr, st);
    let start = prev;
    let mut sweep = 0.0;
    let mut i = 1.0;
    while i <= steps {
        let next = param_angle(wr, hr, st + seg * i);
        let mut d = next - prev;
        while d > PI {
            d -= 2.0 * PI;
        }
        while d <= -PI {
            d += 2.0 * PI;
        }
        sweep += d;
        prev = next;
        i += 1.0;
    }
    // Negative radii mirror the ellipse; express them with positive radii.
    let (sx, sy) = (wr.signum(), hr.signum());
    let start = (sy * start.sin()).atan2(sx * start.cos());
    let sweep = if sx * sy < 0.0 { -sweep } else { sweep };
    let radii = Vec2::new(wr.abs(), hr.abs());
    // kurbo subdivides by radius / tolerance; an error of a billionth of the radius is invisible
    // and keeps the cubic count bounded (about 32 per turn) however large a file makes the radii.
    let tol = tol.max(radii.x.max(radii.y) * 1e-9);
    let arc = Arc::new(center, radii, start, sweep, 0.0);
    for el in arc.append_iter(tol) {
        path.push(el);
    }
}

/// An evaluated adjust handle.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedHandle {
    /// XY or polar.
    pub kind: HandleKind,
    /// Where the handle is drawn.
    pub pos: Point,
    /// The adjust values it drives.
    pub refs: Vec<String>,
    /// Evaluated limits: `[minX, maxX, minY, maxY]` for XY handles, `[minR, maxR, minAng,
    /// maxAng]` for polar ones.
    pub limits: [Option<f64>; 4],
}

/// An evaluated connection site.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionSite {
    /// The angle a connector leaves at, 60 000ths of a degree.
    pub angle: f64,
    /// The position.
    pub pos: Point,
}

/// A geometry evaluated at one size.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedGeometry {
    /// Paths with their fill and stroke flags.
    pub paths: Vec<ResolvedPath>,
    /// The text rectangle (`x0 = l`, `y0 = t`, `x1 = r`, `y1 = b`).
    pub text_rect: Rect,
    /// Connection sites.
    pub connection_sites: Vec<ConnectionSite>,
    /// Adjust handles.
    pub handles: Vec<ResolvedHandle>,
}

impl ResolvedGeometry {
    /// All paths joined into one SVG path string (the shape's outline).
    #[must_use]
    pub fn to_svg(&self) -> String {
        self.paths
            .iter()
            .map(ResolvedPath::to_svg)
            .filter(|d| !d.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl CompiledGeometry {
    /// Evaluates adjust values (with overrides, as stored in `a:prstGeom/a:avLst`) and guides for a
    /// `w` × `h` shape.
    #[must_use]
    pub fn evaluate(&self, w: f64, h: f64, adjustments: &[(&str, f64)]) -> GuideValues<'_> {
        let mut v = vec![0.0; self.slot_count];
        fill_builtins(&mut v, w, h);
        for (name, slot, f) in &self.adjust {
            let over = adjustments
                .iter()
                .rev()
                .find(|(n, x)| n == name && x.is_finite())
                .map(|(_, x)| *x);
            v[usize::from(*slot)] = over.unwrap_or_else(|| f.eval(&v));
        }
        for (slot, f) in &self.guides {
            v[usize::from(*slot)] = f.eval(&v);
        }
        GuideValues {
            geometry: self,
            values: v,
        }
    }

    /// The adjust value names, in definition order.
    pub fn adjust_names(&self) -> impl Iterator<Item = &str> {
        self.adjust.iter().map(|(n, _, _)| n.as_str())
    }

    /// Evaluates the geometry for a `w` × `h` shape (any unit: EMU, points or pixels).
    #[must_use]
    pub fn resolve(&self, w: f64, h: f64, adjustments: &[(&str, f64)]) -> ResolvedGeometry {
        let g = self.evaluate(w, h, adjustments);
        let v = &g.values;
        let val = |o: Operand| match o {
            Operand::Lit(x) => x,
            Operand::Slot(s) => v[usize::from(s)],
        };
        let paths = self
            .paths
            .iter()
            .map(|p| {
                let sx = p.w.filter(|x| *x != 0.0).map_or(1.0, |pw| w / pw);
                let sy = p.h.filter(|x| *x != 0.0).map_or(1.0, |ph| h / ph);
                let pt = |x: Operand, y: Operand| Point::new(val(x) * sx, val(y) * sy);
                let mut segments = Vec::with_capacity(p.cmds.len());
                let mut cur = Point::ZERO;
                let mut start = Point::ZERO;
                let mut open = false;
                let mut all_closed = true;
                for c in &p.cmds {
                    match *c {
                        CompiledCmd::MoveTo([x, y]) => {
                            if open {
                                all_closed = false;
                            }
                            cur = pt(x, y);
                            start = cur;
                            segments.push(Segment::MoveTo(cur));
                            open = true;
                        }
                        CompiledCmd::LineTo([x, y]) => {
                            cur = pt(x, y);
                            segments.push(Segment::LineTo(cur));
                        }
                        CompiledCmd::QuadTo([a, b, c, d]) => {
                            let end = pt(c, d);
                            segments.push(Segment::QuadTo(pt(a, b), end));
                            cur = end;
                        }
                        CompiledCmd::CubicTo([a, b, c, d, e, f]) => {
                            let end = pt(e, f);
                            segments.push(Segment::CubicTo(pt(a, b), pt(c, d), end));
                            cur = end;
                        }
                        CompiledCmd::ArcTo([wr, hr, st, sw]) => {
                            let (wr, hr) = (val(wr) * sx, val(hr) * sy);
                            let (st, sw) = (finite_angle(val(st)), clamp_sweep(val(sw)));
                            segments.push(Segment::ArcTo {
                                from: cur,
                                wr,
                                hr,
                                st_ang: st,
                                sw_ang: sw,
                            });
                            cur = Segment::arc_end(cur, wr, hr, st, sw);
                        }
                        CompiledCmd::Close => {
                            segments.push(Segment::Close);
                            open = false;
                            cur = start;
                        }
                    }
                }
                if open {
                    all_closed = false;
                }
                ResolvedPath {
                    closed: all_closed && !segments.is_empty(),
                    segments,
                    fill: p.fill,
                    stroke: p.stroke,
                    extrusion_ok: p.extrusion_ok,
                }
            })
            .collect();
        let text_rect = self.rect.map_or(Rect::new(0.0, 0.0, w, h), |[l, t, r, b]| {
            Rect::new(val(l), val(t), val(r), val(b))
        });
        let connection_sites = self
            .cxn
            .iter()
            .map(|[a, x, y]| ConnectionSite {
                angle: val(*a),
                pos: Point::new(val(*x), val(*y)),
            })
            .collect();
        let handles = self
            .handles
            .iter()
            .map(|h| ResolvedHandle {
                kind: h.kind,
                pos: Point::new(val(h.pos[0]), val(h.pos[1])),
                refs: h.refs.clone(),
                limits: h.limits.map(|l| l.map(val)),
            })
            .collect();
        ResolvedGeometry {
            paths,
            text_rect,
            connection_sites,
            handles,
        }
    }
}

/// Evaluates one guide formula (`+- w 0 adj`) against named values.
///
/// # Errors
/// [`GeometryError`] for an unknown operator or operand.
pub fn eval_formula(fmla: &str, env: &[(&str, f64)]) -> Result<f64, GeometryError> {
    let mut scope = Scope {
        names: HashMap::new(),
        count: 0,
    };
    let mut values = Vec::with_capacity(env.len());
    for (name, v) in env {
        scope.define(name)?;
        values.push(*v);
    }
    Ok(scope.formula(fmla)?.eval(&values))
}

// ------------------------------------------------------------- XML mapping

fn point_of(e: Option<&XmlElement>) -> (String, String) {
    (
        e.and_then(|e| e.attr("x")).unwrap_or("0").to_owned(),
        e.and_then(|e| e.attr("y")).unwrap_or("0").to_owned(),
    )
}

fn guide_list(list: Option<&XmlElement>) -> Vec<GuideDef> {
    list.map(|l| {
        l.children_named("a:gd")
            .map(|g| GuideDef {
                name: g.attr("name").unwrap_or("").to_owned(),
                fmla: g.attr("fmla").unwrap_or("val 0").to_owned(),
            })
            .collect()
    })
    .unwrap_or_default()
}

fn opt_attr(e: &XmlElement, k: &str) -> Option<String> {
    e.attr(k).map(str::to_owned)
}

/// Parses `a:custGeom` (or any element with `a:avLst`/`a:gdLst`/`a:ahLst`/`a:cxnLst`/`a:rect`/
/// `a:pathLst` children).
#[must_use]
pub fn parse_geometry(node: &XmlElement) -> GeometryDef {
    let ah = node
        .child("a:ahLst")
        .map(|l| {
            l.children_named_any(&["a:ahXY", "a:ahPolar"])
                .map(|h| {
                    let pos = point_of(h.child("a:pos"));
                    if h.name == "a:ahXY" {
                        AdjustHandleDef::Xy {
                            gd_ref_x: opt_attr(h, "gdRefX"),
                            min_x: opt_attr(h, "minX"),
                            max_x: opt_attr(h, "maxX"),
                            gd_ref_y: opt_attr(h, "gdRefY"),
                            min_y: opt_attr(h, "minY"),
                            max_y: opt_attr(h, "maxY"),
                            pos,
                        }
                    } else {
                        AdjustHandleDef::Polar {
                            gd_ref_r: opt_attr(h, "gdRefR"),
                            min_r: opt_attr(h, "minR"),
                            max_r: opt_attr(h, "maxR"),
                            gd_ref_ang: opt_attr(h, "gdRefAng"),
                            min_ang: opt_attr(h, "minAng"),
                            max_ang: opt_attr(h, "maxAng"),
                            pos,
                        }
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let cxn = node
        .child("a:cxnLst")
        .map(|l| {
            l.children_named("a:cxn")
                .map(|c| {
                    let (x, y) = point_of(c.child("a:pos"));
                    ConnectionSiteDef {
                        ang: c.attr("ang").unwrap_or("0").to_owned(),
                        x,
                        y,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let rect = node
        .child("a:rect")
        .map(|r| ["l", "t", "r", "b"].map(|k| r.attr(k).unwrap_or(k).to_owned()));
    let flag = |p: &XmlElement, k: &str| p.attr(k).map(|v| !(v == "false" || v == "0"));
    let path = node
        .child("a:pathLst")
        .map(|l| {
            l.children_named("a:path")
                .map(|p| {
                    let cmds = p
                        .elements()
                        .filter_map(|c| {
                            Some(match c.name.as_str() {
                                "a:moveTo" => {
                                    let (x, y) = point_of(c.child("a:pt"));
                                    PathCommand::MoveTo(x, y)
                                }
                                "a:lnTo" => {
                                    let (x, y) = point_of(c.child("a:pt"));
                                    PathCommand::LineTo(x, y)
                                }
                                "a:arcTo" => PathCommand::ArcTo {
                                    wr: c.attr("wR").unwrap_or("0").to_owned(),
                                    hr: c.attr("hR").unwrap_or("0").to_owned(),
                                    st_ang: c.attr("stAng").unwrap_or("0").to_owned(),
                                    sw_ang: c.attr("swAng").unwrap_or("0").to_owned(),
                                },
                                "a:quadBezTo" => {
                                    let mut pts = c.children_named("a:pt");
                                    let (a, b) = point_of(pts.next());
                                    let (x, y) = point_of(pts.next());
                                    PathCommand::QuadBezTo([a, b, x, y])
                                }
                                "a:cubicBezTo" => {
                                    let mut pts = c.children_named("a:pt");
                                    let (a, b) = point_of(pts.next());
                                    let (c2, d) = point_of(pts.next());
                                    let (x, y) = point_of(pts.next());
                                    PathCommand::CubicBezTo([a, b, c2, d, x, y])
                                }
                                "a:close" => PathCommand::Close,
                                _ => return None,
                            })
                        })
                        .collect();
                    PathDef {
                        w: p.attr_num("w"),
                        h: p.attr_num("h"),
                        fill: p.attr("fill").and_then(PathFillMode::parse),
                        stroke: flag(p, "stroke"),
                        extrusion_ok: flag(p, "extrusionOk"),
                        cmds,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    GeometryDef {
        av: guide_list(node.child("a:avLst")),
        gd: guide_list(node.child("a:gdLst")),
        ah,
        cxn,
        rect,
        path,
    }
}

fn xsd_bool(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

fn gd_el(g: &GuideDef) -> XmlElement {
    XmlElement::new("a:gd")
        .with_attr("name", g.name.as_str())
        .with_attr("fmla", g.fmla.as_str())
}

fn pt_el(x: &str, y: &str) -> XmlElement {
    XmlElement::new("a:pt").with_attr("x", x).with_attr("y", y)
}

/// Serialises a geometry as `a:custGeom`.
#[must_use]
pub fn serialize_custom_geometry(def: &GeometryDef) -> XmlElement {
    let handles = def.ah.iter().map(|h| match h {
        AdjustHandleDef::Xy {
            gd_ref_x,
            min_x,
            max_x,
            gd_ref_y,
            min_y,
            max_y,
            pos,
        } => XmlElement::new("a:ahXY")
            .with_opt_attr("gdRefX", gd_ref_x.as_deref())
            .with_opt_attr("minX", min_x.as_deref())
            .with_opt_attr("maxX", max_x.as_deref())
            .with_opt_attr("gdRefY", gd_ref_y.as_deref())
            .with_opt_attr("minY", min_y.as_deref())
            .with_opt_attr("maxY", max_y.as_deref())
            .with_child(
                XmlElement::new("a:pos")
                    .with_attr("x", pos.0.as_str())
                    .with_attr("y", pos.1.as_str()),
            ),
        AdjustHandleDef::Polar {
            gd_ref_r,
            min_r,
            max_r,
            gd_ref_ang,
            min_ang,
            max_ang,
            pos,
        } => XmlElement::new("a:ahPolar")
            .with_opt_attr("gdRefR", gd_ref_r.as_deref())
            .with_opt_attr("minR", min_r.as_deref())
            .with_opt_attr("maxR", max_r.as_deref())
            .with_opt_attr("gdRefAng", gd_ref_ang.as_deref())
            .with_opt_attr("minAng", min_ang.as_deref())
            .with_opt_attr("maxAng", max_ang.as_deref())
            .with_child(
                XmlElement::new("a:pos")
                    .with_attr("x", pos.0.as_str())
                    .with_attr("y", pos.1.as_str()),
            ),
    });
    let paths = def.path.iter().map(|p| {
        XmlElement::new("a:path")
            .with_opt_attr("w", p.w)
            .with_opt_attr("h", p.h)
            .with_opt_attr("fill", p.fill)
            .with_opt_attr("stroke", p.stroke.map(xsd_bool))
            .with_opt_attr("extrusionOk", p.extrusion_ok.map(xsd_bool))
            .with_children(p.cmds.iter().map(|c| {
                match c {
                    PathCommand::MoveTo(x, y) => {
                        XmlElement::new("a:moveTo").with_child(pt_el(x, y))
                    }
                    PathCommand::LineTo(x, y) => XmlElement::new("a:lnTo").with_child(pt_el(x, y)),
                    PathCommand::ArcTo {
                        wr,
                        hr,
                        st_ang,
                        sw_ang,
                    } => XmlElement::new("a:arcTo")
                        .with_attr("wR", wr.as_str())
                        .with_attr("hR", hr.as_str())
                        .with_attr("stAng", st_ang.as_str())
                        .with_attr("swAng", sw_ang.as_str()),
                    PathCommand::QuadBezTo(p) => XmlElement::new("a:quadBezTo")
                        .with_child(pt_el(&p[0], &p[1]))
                        .with_child(pt_el(&p[2], &p[3])),
                    PathCommand::CubicBezTo(p) => XmlElement::new("a:cubicBezTo")
                        .with_child(pt_el(&p[0], &p[1]))
                        .with_child(pt_el(&p[2], &p[3]))
                        .with_child(pt_el(&p[4], &p[5])),
                    PathCommand::Close => XmlElement::new("a:close"),
                }
            }))
    });
    XmlElement::new("a:custGeom")
        .with_child(XmlElement::new("a:avLst").with_children(def.av.iter().map(gd_el)))
        .with_child(XmlElement::new("a:gdLst").with_children(def.gd.iter().map(gd_el)))
        .with_child(XmlElement::new("a:ahLst").with_children(handles))
        .with_child(
            XmlElement::new("a:cxnLst").with_children(def.cxn.iter().map(|c| {
                XmlElement::new("a:cxn")
                    .with_attr("ang", c.ang.as_str())
                    .with_child(
                        XmlElement::new("a:pos")
                            .with_attr("x", c.x.as_str())
                            .with_attr("y", c.y.as_str()),
                    )
            })),
        )
        .with_opt_child(def.rect.as_ref().map(|r| {
            XmlElement::new("a:rect")
                .with_attr("l", r[0].as_str())
                .with_attr("t", r[1].as_str())
                .with_attr("r", r[2].as_str())
                .with_attr("b", r[3].as_str())
        }))
        .with_child(XmlElement::new("a:pathLst").with_children(paths))
}

/// Reads the adjust values of `a:prstGeom/a:avLst` (or `a:custGeom/a:avLst`) as overrides: each
/// `a:gd` whose formula is `val n`.
#[must_use]
pub fn adjust_values(av_lst: Option<&XmlElement>) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for g in av_lst.into_iter().flat_map(|l| l.children_named("a:gd")) {
        let (Some(name), Some(fmla)) = (g.attr("name"), g.attr("fmla")) else {
            continue;
        };
        let mut parts = fmla.split_whitespace();
        if parts.next() == Some("val")
            && let Some(v) = parts.next().and_then(|v| v.parse::<f64>().ok())
            && v.is_finite()
        {
            out.push((name.to_owned(), v));
        }
    }
    out
}
