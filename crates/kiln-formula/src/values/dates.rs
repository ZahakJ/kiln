// SPDX-License-Identifier: AGPL-3.0-or-later
//! Date serial numbers.
//!
//! Excel stores dates as days since an epoch with the time of day as the fraction. In the 1900
//! date system serial 1 is 1900-01-01 and, for Lotus 1-2-3 compatibility, 1900 is treated as a leap
//! year: serial 60 is the fictitious 1900-02-29 and every later serial is one higher than the true
//! day count. In the 1904 system serial 0 is 1904-01-01 and there is no such quirk.

/// Largest valid serial in the 1900 system: 9999-12-31.
pub const MAX_DATE_SERIAL_1900: f64 = 2_958_465.0;
/// Largest valid serial in the 1904 system: 9999-12-31.
pub const MAX_DATE_SERIAL_1904: f64 = 2_957_003.0;

const DAY_MS: f64 = 86_400_000.0;

/// A calendar date. `day` is 0 for serial 0 in the 1900 system ("1900-01-00").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateParts {
    /// Year.
    pub year: i32,
    /// Month, 1–12.
    pub month: u32,
    /// Day of the month.
    pub day: u32,
}

/// A time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeParts {
    /// Hour, 0–23.
    pub hour: u32,
    /// Minute, 0–59.
    pub minute: u32,
    /// Second, 0–59.
    pub second: u32,
    /// Millisecond, 0–999.
    pub millisecond: u32,
}

/// Days since 1970-01-01 of a proleptic Gregorian date (H. Hinnant's algorithm).
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The proleptic Gregorian date `days` after 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Day number of serial 0 in each system, relative to 1970-01-01.
fn epoch(date1904: bool) -> i64 {
    if date1904 {
        days_from_civil(1904, 1, 1)
    } else {
        days_from_civil(1899, 12, 31)
    }
}

/// Converts a calendar date to a serial, normalising overflowing months and days the way `DATE()`
/// does (month 13 → January next year, day 0 → last day of the previous month). Returns `None`
/// when the result is outside the supported range. Years 0–1899 are taken as 1900 + year.
pub fn date_to_serial(year: f64, month: f64, day: f64, date1904: bool) -> Option<f64> {
    if !(year.is_finite() && month.is_finite() && day.is_finite()) {
        return None;
    }
    let mut y = year.trunc();
    let m = month.trunc();
    let d = day.trunc();
    if !(0.0..10_000.0).contains(&y) || m.abs() > 1e9 || d.abs() > 1e10 {
        return None;
    }
    if y < 1900.0 {
        y += 1900.0;
    }
    let total_months = y as i64 * 12 + (m as i64 - 1);
    let yy = total_months.div_euclid(12);
    let mm = total_months.rem_euclid(12) as u32; // 0–11
    if !(0..=9999).contains(&yy) {
        return None;
    }
    let month_start = days_from_civil(yy, mm + 1, 1);
    let mut serial = (month_start - epoch(date1904)) as f64 + (d - 1.0);
    // Lotus bug: every month start after February 1900 gains the phantom 29 February.
    if !date1904 && (yy > 1900 || (yy == 1900 && mm > 1)) {
        serial += 1.0;
    }
    let max = if date1904 {
        MAX_DATE_SERIAL_1904
    } else {
        MAX_DATE_SERIAL_1900
    };
    (0.0..=max).contains(&serial).then_some(serial)
}

/// Converts the integer part of a serial to a calendar date.
pub fn serial_to_date(serial: f64, date1904: bool) -> DateParts {
    let n = serial.floor() as i64;
    let (y, m, d) = if date1904 {
        civil_from_days(epoch(true) + n)
    } else if n == 0 {
        return DateParts {
            year: 1900,
            month: 1,
            day: 0,
        };
    } else if n == 60 {
        return DateParts {
            year: 1900,
            month: 2,
            day: 29,
        };
    } else {
        civil_from_days(epoch(false) + if n < 60 { n } else { n - 1 })
    };
    DateParts {
        year: y as i32,
        month: m,
        day: d,
    }
}

/// Splits the fractional part of a serial into a time of day (rounded to the millisecond).
pub fn serial_to_time(serial: f64) -> TimeParts {
    let mut ms = ((serial - serial.floor()) * DAY_MS + 0.5).floor();
    if ms >= DAY_MS {
        ms = DAY_MS - 1.0;
    }
    let ms = ms.max(0.0) as u32;
    let hour = ms / 3_600_000;
    let minute = ms / 60_000 % 60;
    let second = ms / 1000 % 60;
    TimeParts {
        hour,
        minute,
        second,
        millisecond: ms % 1000,
    }
}

/// Fraction of a day for a time of day (values may overflow, like `TIME()`).
pub fn time_to_fraction(hour: f64, minute: f64, second: f64) -> f64 {
    (hour * 3600.0 + minute * 60.0 + second) / 86_400.0
}

/// Weekday of a serial, 0 = Sunday … 6 = Saturday (serial 0 and the phantom 1900-02-29 follow
/// Excel's Lotus calendar, in which serial 1 is a Sunday).
pub fn serial_weekday(serial: f64, date1904: bool) -> u32 {
    let n = serial.floor() as i64 + if date1904 { 1462 } else { 0 };
    (n - 1).rem_euclid(7) as u32
}

/// True for Gregorian leap years.
pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days in a month (February 1900 has 29 days in Excel's calendar).
pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if year == 1900 || is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The serial of a moment given as milliseconds since 1970-01-01 in the wall-clock time the
/// serial should show (pass local time to get what `NOW()` shows).
pub fn serial_from_unix_ms(ms: f64, date1904: bool) -> f64 {
    let days = (ms / DAY_MS).floor();
    let frac = (ms - days * DAY_MS) / DAY_MS;
    let n = days as i64 - epoch(date1904);
    let lotus = if !date1904 && n >= 60 { 1.0 } else { 0.0 };
    n as f64 + lotus + frac
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(y: f64, m: f64, d: f64) -> Option<f64> {
        date_to_serial(y, m, d, false)
    }

    #[test]
    fn the_1900_system_has_the_lotus_leap_day() {
        assert_eq!(s(1900.0, 1.0, 1.0), Some(1.0));
        assert_eq!(s(1900.0, 2.0, 28.0), Some(59.0));
        assert_eq!(s(1900.0, 2.0, 29.0), Some(60.0));
        assert_eq!(s(1900.0, 3.0, 1.0), Some(61.0));
        assert_eq!(s(2024.0, 1.0, 15.0), Some(45306.0));
        assert_eq!(s(9999.0, 12.0, 31.0), Some(2_958_465.0));
        assert_eq!(s(10_000.0, 1.0, 1.0), None);
    }

    #[test]
    fn overflowing_months_and_days_normalise_like_date() {
        assert_eq!(s(2023.0, 13.0, 1.0), s(2024.0, 1.0, 1.0));
        assert_eq!(s(2024.0, 3.0, 0.0), s(2024.0, 2.0, 29.0));
        assert_eq!(s(2024.0, 0.0, 1.0), s(2023.0, 12.0, 1.0));
        assert_eq!(s(99.0, 1.0, 1.0), s(1999.0, 1.0, 1.0));
        assert_eq!(s(1900.0, 1.0, 0.0), Some(0.0));
        assert_eq!(s(1900.0, 1.0, -1.0), None);
    }

    #[test]
    fn serials_convert_back() {
        let d = |y, m, day| DateParts {
            year: y,
            month: m,
            day,
        };
        assert_eq!(serial_to_date(0.0, false), d(1900, 1, 0));
        assert_eq!(serial_to_date(60.0, false), d(1900, 2, 29));
        assert_eq!(serial_to_date(61.0, false), d(1900, 3, 1));
        assert_eq!(serial_to_date(45306.75, false), d(2024, 1, 15));
        let mut serial = 1.0;
        while serial < 73_051.0 {
            let p = serial_to_date(serial, false);
            assert_eq!(
                s(f64::from(p.year), f64::from(p.month), f64::from(p.day)),
                Some(serial)
            );
            serial += 7.0;
        }
    }

    #[test]
    fn the_1904_system() {
        assert_eq!(date_to_serial(1904.0, 1.0, 1.0, true), Some(0.0));
        assert_eq!(
            date_to_serial(2024.0, 1.0, 15.0, true),
            Some(45306.0 - 1462.0)
        );
        assert_eq!(
            serial_to_date(45306.0 - 1462.0, true),
            DateParts {
                year: 2024,
                month: 1,
                day: 15
            }
        );
    }

    #[test]
    fn times_weekdays_and_the_clock() {
        assert_eq!(
            serial_to_time(0.75),
            TimeParts {
                hour: 18,
                minute: 0,
                second: 0,
                millisecond: 0
            }
        );
        let t = serial_to_time(45306.5 + 1.0 / 86400.0);
        assert_eq!((t.hour, t.second), (12, 1));
        assert_eq!(serial_weekday(1.0, false), 0);
        assert_eq!(serial_weekday(45306.0, false), 1);
        assert_eq!(serial_weekday(0.0, true), 5);
        // 2024-01-15T18:00 wall clock.
        let ms = days_from_civil(2024, 1, 15) as f64 * DAY_MS + 18.0 * 3_600_000.0;
        assert_eq!(serial_from_unix_ms(ms, false), 45306.75);
        assert_eq!(days_in_month(1900, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 4), 30);
    }
}
