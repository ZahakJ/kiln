// SPDX-License-Identifier: AGPL-3.0-or-later
//! The value model: errors, blanks, numbers, text, logicals, arrays and lambdas; coercion and
//! comparison rules; Excel's 15-significant-digit number handling; date serials in the 1900 system
//! (with the Lotus 1900-02-29 bug) and the 1904 system.

mod coerce;
mod compare;
mod dates;
mod error;
mod number;
mod value;

pub use coerce::{
    CoercionOptions, display_text, parse_date_time_text, parse_number_text, to_bool, to_number,
    to_text,
};
pub use compare::{compare_text, compare_values, lookup_equals, sort_compare, text_equals};
pub use dates::{
    DateParts, MAX_DATE_SERIAL_1900, MAX_DATE_SERIAL_1904, TimeParts, civil_from_days,
    date_to_serial, days_from_civil, days_in_month, is_leap_year, serial_from_unix_ms,
    serial_to_date, serial_to_time, serial_weekday, time_to_fraction,
};
pub use error::ErrorValue;
pub use number::{
    excel_round, excel_trunc, number_to_text, numbers_equal, round_significant, round15,
    shortest_number_text,
};
pub use value::{Array2D, Value};
