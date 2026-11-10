// SPDX-License-Identifier: AGPL-3.0-or-later
//! `FORECAST.ETS`, `FORECAST.ETS.CONFINT`, `FORECAST.ETS.SEASONALITY` and `FORECAST.ETS.STAT`:
//! additive triple exponential smoothing (Holt-Winters, Excel's "AAA" version of ETS).
//!
//! The fitting follows LibreOffice's `ScETSForecastCalculation` (sc/source/core/tool/interpr8.cxx,
//! MPL-2.0), whose `FORECAST.ETS.ADD` is the oracle of the conformance rows:
//!
//! - the timeline is sorted; equal dates are aggregated (`aggregation`, AVERAGE by default); the
//!   step is the smallest gap and every gap must be a multiple of it; up to 30 % missing points are
//!   filled with the mean of their neighbours (or zero, `data_completion` 0); dates that all fall
//!   on the same day of the month step by months;
//! - the season length is detected from the differences of consecutive values (or given);
//! - the initial trend and seasonal indices come from the first two seasons (NIST handbook
//!   6.4.3.5, with the seasonal effect corrected for trend); a season length of 1 or 0 fits
//!   double exponential smoothing instead;
//! - alpha (level), gamma (trend) and beta (season) are fitted one inside the other by halving
//!   the interval that holds the lowest mean squared one-step error, to a resolution of 0.001.
//!
//! Excel's own fitting is not published; its parameters (and so its forecasts) can differ in the
//! later digits (`docs/formula-deviations.md`). `FORECAST.ETS.CONFINT` uses the analytic
//! prediction variance of the additive model (Hyndman et al., *Forecasting with Exponential
//! Smoothing*, 2008, class 1), where LibreOffice simulates 1,000 random scenarios.

use super::special::norm_inv;
use super::support::*;
use crate::engine::{Arg, CallContext};
use crate::registry::{ArgMask, FunctionSpec};
use crate::values::{ErrorValue, Value, days_in_month, serial_to_date};

/// Smallest change of alpha, beta and gamma the fitting resolves.
const RESOLUTION: f64 = 0.001;
/// Excel's largest season length.
const MAX_SEASON: f64 = 8760.0;
/// Largest serial that is a date.
const MAX_SERIAL: f64 = 2_958_465.0;

/// Neumaier-compensated summation (LibreOffice's `KahanSum`).
#[derive(Clone, Copy, Default)]
struct Kahan {
    sum: f64,
    c: f64,
}

impl Kahan {
    fn add(&mut self, x: f64) {
        let t = self.sum + x;
        if self.sum.abs() >= x.abs() {
            self.c += (self.sum - t) + x;
        } else {
            self.c += (x - t) + self.sum;
        }
        self.sum = t;
    }

    fn get(self) -> f64 {
        self.sum + self.c
    }
}

/// How equal dates are combined (`aggregation`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Aggregation {
    Average,
    Count,
    Max,
    Median,
    Min,
    Sum,
}

impl Aggregation {
    fn of(n: f64) -> R<Aggregation> {
        Ok(match n.trunc() as i64 {
            1 => Aggregation::Average,
            2 | 3 => Aggregation::Count,
            4 => Aggregation::Max,
            5 => Aggregation::Median,
            6 => Aggregation::Min,
            7 => Aggregation::Sum,
            _ => return Err(ErrorValue::Num),
        })
    }

    fn apply(self, ys: &mut [f64]) -> f64 {
        match self {
            Aggregation::Average => ys.iter().sum::<f64>() / ys.len() as f64,
            Aggregation::Count => ys.len() as f64,
            Aggregation::Max => ys.iter().copied().fold(f64::MIN, f64::max),
            Aggregation::Min => ys.iter().copied().fold(f64::MAX, f64::min),
            Aggregation::Sum => ys.iter().sum(),
            Aggregation::Median => {
                ys.sort_by(f64::total_cmp);
                let n = ys.len();
                if n % 2 == 1 {
                    ys[n / 2]
                } else {
                    (ys[n / 2] + ys[n / 2 - 1]) / 2.0
                }
            }
        }
    }
}

/// A fitted model.
struct Ets {
    x: Vec<f64>,
    y: Vec<f64>,
    base: Vec<f64>,
    trend: Vec<f64>,
    per_idx: Vec<f64>,
    forecast: Vec<f64>,
    /// Samples per season (0 or 1: no season, double exponential smoothing).
    season: usize,
    eds: bool,
    step: f64,
    alpha: f64,
    beta: f64,
    gamma: f64,
    /// The day of the month when the timeline steps by months.
    month_day: u32,
    date1904: bool,
    mae: f64,
    mase: f64,
    mse: f64,
    rmse: f64,
    smape: f64,
}

/// What the caller asked for the season length: detect it, none, or a given length.
#[derive(Clone, Copy)]
enum Season {
    Detect,
    None,
    Length(usize),
}

impl Ets {
    fn n(&self) -> usize {
        self.y.len()
    }

    /// Sorts, aggregates and completes the data, detects the season and initialises the model.
    fn prepare(
        mut points: Vec<(f64, f64)>,
        season: Season,
        complete: bool,
        aggregation: Aggregation,
        date1904: bool,
    ) -> R<Ets> {
        if points.len() < 2 {
            return Err(ErrorValue::Num);
        }
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Month steps: every date on the same day of the month.
        let date_of = |s: f64| {
            (0.0..=MAX_SERIAL)
                .contains(&s)
                .then(|| serial_to_date(s, date1904))
        };
        let mut month_day = date_of(points[0].0).map_or(0, |d| d.day);
        if month_day != 0 {
            let first = points[0].0.floor();
            for &(x, _) in &points[1..] {
                match date_of(x) {
                    Some(d) if x.floor() == first || d.day == month_day => {}
                    _ => {
                        month_day = 0;
                        break;
                    }
                }
            }
        }
        if month_day != 0 {
            for p in &mut points {
                if let Some(d) = date_of(p.0) {
                    p.0 = f64::from(d.year) * 12.0 + f64::from(d.month);
                }
            }
        }
        // Equal dates become one point.
        let mut x: Vec<f64> = Vec::with_capacity(points.len());
        let mut y: Vec<f64> = Vec::with_capacity(points.len());
        let mut i = 0;
        while i < points.len() {
            let mut j = i + 1;
            while j < points.len() && points[j].0 == points[i].0 {
                j += 1;
            }
            let mut ys: Vec<f64> = points[i..j].iter().map(|p| p.1).collect();
            x.push(points[i].0);
            y.push(if j - i == 1 {
                ys[0]
            } else {
                aggregation.apply(&mut ys)
            });
            i = j;
        }
        if x.len() < 2 {
            return Err(ErrorValue::Num);
        }
        let step = x.windows(2).map(|w| w[1] - w[0]).fold(f64::MAX, f64::min);
        if !(step > 0.0 && step.is_finite()) {
            return Err(ErrorValue::Num);
        }
        let mut gap = false;
        for w in x.windows(2) {
            let d = w[1] - w[0];
            if d != step {
                if d % step != 0.0 {
                    return Err(ErrorValue::Num);
                }
                gap = true;
            }
        }
        if gap {
            let original = x.len() as f64;
            let mut missing = 0usize;
            let (mut nx, mut ny) = (Vec::with_capacity(x.len()), Vec::with_capacity(y.len()));
            nx.push(x[0]);
            ny.push(y[0]);
            for i in 1..x.len() {
                if x[i] - x[i - 1] > step {
                    let fill = if complete {
                        (y[i] + y[i - 1]) / 2.0
                    } else {
                        0.0
                    };
                    let mut at = Kahan::default();
                    at.add(x[i - 1]);
                    at.add(step);
                    while at.get() < x[i] {
                        nx.push(at.get());
                        ny.push(fill);
                        missing += 1;
                        if missing as f64 / original > 0.3 {
                            return Err(ErrorValue::Num);
                        }
                        at.add(step);
                    }
                }
                nx.push(x[i]);
                ny.push(y[i]);
            }
            x = nx;
            y = ny;
        }
        let n = x.len();
        let mut m = Ets {
            x,
            y,
            base: vec![0.0; n],
            trend: vec![0.0; n],
            per_idx: vec![0.0; n],
            forecast: vec![0.0; n],
            season: 0,
            eds: false,
            step,
            alpha: 0.0,
            beta: 0.0,
            gamma: 0.0,
            month_day,
            date1904,
            mae: 0.0,
            mase: 0.0,
            mse: 0.0,
            rmse: 0.0,
            smape: 0.0,
        };
        match season {
            Season::None => m.eds = true,
            Season::Length(l) => m.season = l,
            Season::Detect => {
                m.season = m.period_length();
                if m.season == 1 {
                    m.eds = true;
                }
            }
        }
        m.init()?;
        m.fit();
        Ok(m)
    }

    /// The season length whose consecutive differences repeat best (the most recent samples).
    fn period_length(&self) -> usize {
        let n = self.n();
        let mut best = n;
        let mut best_err = f64::MAX;
        let mut len = n / 2;
        while len >= 1 {
            let mut err = Kahan::default();
            let periods = n / len;
            let start = n - periods * len + 1;
            for i in start..n.saturating_sub(len) {
                err.add(
                    ((self.y[i] - self.y[i - 1]) - (self.y[len + i] - self.y[len + i - 1])).abs(),
                );
            }
            let e = err.get() / ((periods as f64 - 1.0) * len as f64 - 1.0);
            if e <= best_err || e == 0.0 {
                best = len;
                best_err = e;
            }
            len -= 1;
        }
        best
    }

    fn init(&mut self) -> R<()> {
        let n = self.n();
        let m = self.season;
        self.forecast[0] = self.y[0];
        if self.eds {
            self.trend[0] = (self.y[n - 1] - self.y[0]) / (n as f64 - 1.0);
        } else {
            if m == 0 || n < 2 * m {
                return Err(ErrorValue::Num);
            }
            let mut s = Kahan::default();
            for i in 0..m {
                s.add(self.y[i + m]);
                s.add(-self.y[i]);
            }
            self.trend[0] = s.get() / (m * m) as f64;
            let periods = n / m;
            let mut avg = Vec::with_capacity(periods);
            for i in 0..periods {
                let mut s = Kahan::default();
                for j in 0..m {
                    s.add(self.y[i * m + j]);
                }
                let a = s.get() / m as f64;
                if a == 0.0 {
                    return Err(ErrorValue::Div0);
                }
                avg.push(a);
            }
            for j in 0..m {
                let mut s = Kahan::default();
                for (i, a) in avg.iter().enumerate() {
                    s.add(
                        self.y[i * m + j]
                            - (a + (j as f64 - 0.5 * (m as f64 - 1.0)) * self.trend[0]),
                    );
                }
                self.per_idx[j] = s.get() / periods as f64;
            }
            if m < n {
                self.per_idx[m] = 0.0;
            }
        }
        self.base[0] = if self.eds {
            self.y[0]
        } else {
            self.y[0] / self.per_idx[0]
        };
        Ok(())
    }

    /// Runs the smoothing with the current constants and measures the one-step errors.
    fn refill(&mut self) {
        let m = self.season;
        for i in 1..self.n() {
            if self.eds {
                self.base[i] = self.alpha * self.y[i]
                    + (1.0 - self.alpha) * (self.base[i - 1] + self.trend[i - 1]);
                self.trend[i] = self.gamma * (self.base[i] - self.base[i - 1])
                    + (1.0 - self.gamma) * self.trend[i - 1];
                self.forecast[i] = self.base[i - 1] + self.trend[i - 1];
            } else {
                let idx = if i > m { i - m } else { i };
                self.base[i] = self.alpha * (self.y[i] - self.per_idx[idx])
                    + (1.0 - self.alpha) * (self.base[i - 1] + self.trend[i - 1]);
                self.per_idx[i] =
                    self.beta * (self.y[i] - self.base[i]) + (1.0 - self.beta) * self.per_idx[idx];
                self.trend[i] = self.gamma * (self.base[i] - self.base[i - 1])
                    + (1.0 - self.gamma) * self.trend[i - 1];
                self.forecast[i] = self.base[i - 1] + self.trend[i - 1] + self.per_idx[idx];
            }
        }
        self.accuracy();
    }

    fn accuracy(&mut self) {
        let n = self.n();
        let (mut abs, mut sq, mut perc, mut div) = (
            Kahan::default(),
            Kahan::default(),
            Kahan::default(),
            Kahan::default(),
        );
        for i in 1..n {
            let e = self.forecast[i] - self.y[i];
            abs.add(e.abs());
            sq.add(e * e);
            perc.add(e.abs() / (self.forecast[i].abs() + self.y[i].abs()));
        }
        for i in 2..n {
            div.add((self.y[i] - self.y[i - 1]).abs());
        }
        let k = (n - 1) as f64;
        self.mae = abs.get() / k;
        self.mase = abs.get() / (k * div.get() / (k - 1.0));
        self.mse = sq.get() / k;
        self.rmse = self.mse.sqrt();
        self.smape = perc.get() * 2.0 / k;
    }

    fn set(&mut self, which: Param, v: f64) {
        match which {
            Param::Alpha => self.alpha = v,
            Param::Beta => self.beta = v,
            Param::Gamma => self.gamma = v,
        }
    }

    /// Fits the constant `which` (with the inner ones refitted at every trial value).
    fn fit_param(&mut self, which: Param) {
        let inner = |s: &mut Ets| match which {
            Param::Alpha => {
                if s.eds {
                    s.beta = 0.0;
                    s.fit_param(Param::Gamma);
                } else {
                    s.fit_param(Param::Beta);
                }
                s.refill();
            }
            Param::Beta => {
                s.fit_param(Param::Gamma);
                s.refill();
            }
            Param::Gamma => s.refill(),
        };
        let (mut f0, mut f1, mut f2) = (0.0, 0.5, 1.0);
        self.set(which, f0);
        inner(self);
        let mut e0 = self.mse;
        self.set(which, f2);
        inner(self);
        let mut e2 = self.mse;
        self.set(which, f1);
        inner(self);
        if e0 == self.mse && self.mse == e2 {
            self.set(which, 0.0);
            inner(self);
            return;
        }
        while f2 - f1 > RESOLUTION {
            if e2 > e0 {
                f2 = f1;
                e2 = self.mse;
                f1 = (f0 + f1) / 2.0;
            } else {
                f0 = f1;
                e0 = self.mse;
                f1 = (f1 + f2) / 2.0;
            }
            self.set(which, f1);
            inner(self);
        }
        if e2 > e0 {
            if e0 < self.mse {
                self.set(which, f0);
                inner(self);
            }
        } else if e2 < self.mse {
            self.set(which, f2);
            inner(self);
        }
    }

    fn fit(&mut self) {
        self.fit_param(Param::Alpha);
        self.accuracy();
    }

    /// A target date in the model's units (months when the timeline steps by months).
    fn target_x(&self, t: f64) -> R<f64> {
        if self.month_day == 0 {
            return Ok(t);
        }
        if !(0.0..=MAX_SERIAL).contains(&t) {
            return Err(ErrorValue::Num);
        }
        let d = serial_to_date(t, self.date1904);
        let len = f64::from(days_in_month(i64::from(d.year), d.month));
        Ok(12.0 * f64::from(d.year)
            + f64::from(d.month)
            + (f64::from(d.day) - f64::from(self.month_day)) / len)
    }

    fn ahead(&self, steps: usize) -> f64 {
        let n = self.n();
        let (b, t) = (self.base[n - 1], self.trend[n - 1]);
        if self.eds {
            b + steps as f64 * t
        } else {
            let m = self.season;
            b + steps as f64 * t + self.per_idx[n - 1 - m + steps % m]
        }
    }

    /// The forecast at a target on or after the last date.
    fn forecast_at(&self, target: f64) -> R<f64> {
        let t = self.target_x(target)?;
        let last = self.x[self.n() - 1];
        if t < last {
            return Err(ErrorValue::Num);
        }
        let h = (t - last) / self.step;
        if h > 1e9 {
            return Err(ErrorValue::Num);
        }
        let n = h.trunc() as usize;
        let frac = (t - last) % self.step;
        let mut f = self.ahead(n);
        if frac >= RESOLUTION {
            let next = self.ahead(n + 1);
            f += frac / self.step * (next - f);
        }
        finite(f)
    }

    /// The half-width of the prediction interval at a target.
    fn interval_at(&self, target: f64, level: f64) -> R<f64> {
        let t = self.target_x(target)?;
        let last = self.x[self.n() - 1];
        if t < last {
            return Err(ErrorValue::Num);
        }
        let h = (t - last) / self.step;
        if h > 1e7 {
            return Err(ErrorValue::Num);
        }
        let z = norm_inv((1.0 + level) / 2.0);
        // In error-correction form the trend moves by alpha·gamma and the season by
        // beta·(1 − alpha) of each one-step error.
        let (a, b, g) = (
            self.alpha,
            self.alpha * self.gamma,
            if self.eds {
                0.0
            } else {
                self.beta * (1.0 - self.alpha)
            },
        );
        let m = self.season.max(1);
        let half = |steps: usize| {
            let mut v = Kahan::default();
            v.add(1.0);
            for j in 1..steps {
                let d = if !self.eds && j % m == 0 { 1.0 } else { 0.0 };
                let c = a + b * j as f64 + g * d;
                v.add(c * c);
            }
            z * self.rmse * v.get().sqrt()
        };
        let steps = h.ceil().max(1.0) as usize;
        let hi = half(steps);
        let frac = h - h.floor();
        let out = if frac > 0.0 && steps > 1 {
            let lo = half(steps - 1);
            lo + frac * (hi - lo)
        } else {
            hi
        };
        finite(out)
    }
}

#[derive(Clone, Copy)]
enum Param {
    Alpha,
    Beta,
    Gamma,
}

/// Reads `values` and `timeline`: same size, numbers only; a blank value leaves its date out
/// (a missing point).
fn points(
    args: &mut [Arg<'_>],
    vi: usize,
    ti: usize,
    ctx: &mut dyn CallContext,
) -> R<Vec<(f64, f64)>> {
    let v = value_at(args, vi, ctx);
    let ys = ctx.to_array(v)?;
    let t = value_at(args, ti, ctx);
    let xs = ctx.to_array(t)?;
    if ys.len() != xs.len() {
        return Err(ErrorValue::NA);
    }
    let mut out = Vec::with_capacity(ys.len());
    for (y, x) in ys.values().iter().zip(xs.values()) {
        let x = match x {
            Value::Number(n) => *n,
            Value::Error(e) => return Err(*e),
            Value::Blank => continue,
            _ => return Err(ErrorValue::Value),
        };
        let y = match y {
            Value::Number(n) => *n,
            Value::Error(e) => return Err(*e),
            Value::Blank => continue,
            _ => return Err(ErrorValue::Value),
        };
        out.push((x, y));
    }
    Ok(out)
}

fn season_arg(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Season> {
    let s = num(args, i, ctx, 1.0)?;
    if !(0.0..=MAX_SEASON).contains(&s) {
        return Err(ErrorValue::Num);
    }
    Ok(match s.trunc() as usize {
        0 => Season::None,
        1 => Season::Detect,
        l => Season::Length(l),
    })
}

fn completion_arg(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<bool> {
    let c = num(args, i, ctx, 1.0)?;
    match c.trunc() as i64 {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ErrorValue::Num),
    }
}

/// Fits the model from the arguments: values, timeline, then the season, completion and
/// aggregation positions (`None` for a position the function does not have).
fn model(
    args: &mut [Arg<'_>],
    ctx: &mut dyn CallContext,
    (vi, ti): (usize, usize),
    season: Option<usize>,
    complete: usize,
    agg: usize,
) -> R<Ets> {
    let season = match season {
        Some(i) => season_arg(args, i, ctx)?,
        None => Season::Detect,
    };
    let complete = completion_arg(args, complete, ctx)?;
    let aggregation = Aggregation::of(num(args, agg, ctx, 1.0)?)?;
    let pts = points(args, vi, ti, ctx)?;
    Ets::prepare(pts, season, complete, aggregation, ctx.date1904())
}

fn forecast_ets(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let target = num(args, 0, ctx, 0.0)?;
    let m = model(args, ctx, (1, 2), Some(3), 4, 5)?;
    m.forecast_at(target)
}

fn forecast_confint(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let target = num(args, 0, ctx, 0.0)?;
    let level = num(args, 3, ctx, 0.95)?;
    if !(level > 0.0 && level < 1.0) {
        return Err(ErrorValue::Num);
    }
    let m = model(args, ctx, (1, 2), Some(4), 5, 6)?;
    m.interval_at(target, level)
}

fn forecast_seasonality(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let m = model(args, ctx, (0, 1), None, 2, 3)?;
    Ok(m.season as f64)
}

fn forecast_stat(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let kind = num(args, 2, ctx, 0.0)?.trunc() as i64;
    if !(1..=8).contains(&kind) {
        return Err(ErrorValue::Num);
    }
    let m = model(args, ctx, (0, 1), Some(3), 4, 5)?;
    finite(match kind {
        1 => m.alpha,
        2 => m.gamma,
        3 => m.beta,
        4 => m.mase,
        5 => m.smape,
        6 => m.mae,
        7 => m.rmse,
        _ => m.step,
    })
}

/// The ETS specs.
pub(crate) fn specs() -> Vec<FunctionSpec> {
    vec![
        spec("FORECAST.ETS", 3, 6, imp!(forecast_ets)).lift(ArgMask::of(&[0])),
        spec("FORECAST.ETS.CONFINT", 3, 7, imp!(forecast_confint)).lift(ArgMask::of(&[0, 3])),
        spec("FORECAST.ETS.SEASONALITY", 2, 4, imp!(forecast_seasonality)),
        spec("FORECAST.ETS.STAT", 3, 6, imp!(forecast_stat)).lift(ArgMask::of(&[2])),
    ]
}
