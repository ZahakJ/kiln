/**
 * Date serial numbers.
 *
 * Excel stores dates as days since an epoch with the time of day as the fraction. In the 1900 date
 * system serial 1 is 1900-01-01 and, for Lotus 1-2-3 compatibility, 1900 is treated as a leap year:
 * serial 60 is the fictitious 1900-02-29 and every later serial is one higher than the true day count.
 * In the 1904 system serial 0 is 1904-01-01 and there is no such quirk.
 */

const DAY_MS = 86_400_000;
const EPOCH_1900 = Date.UTC(1899, 11, 31); // serial 0 ("1900-01-00")
const EPOCH_1904 = Date.UTC(1904, 0, 1); // serial 0 in the 1904 system

/** Largest valid serial: 9999-12-31. */
export const MAX_DATE_SERIAL_1900 = 2_958_465;
export const MAX_DATE_SERIAL_1904 = 2_957_003;

export interface DateParts {
  year: number;
  month: number; // 1-12
  day: number; // 1-31 (0 for serial 0 in the 1900 system)
}

export interface TimeParts {
  hour: number;
  minute: number;
  second: number;
  millisecond: number;
}

/**
 * Converts a calendar date to a serial, normalising overflowing months and days the way DATE() does
 * (month 13 → January next year, day 0 → last day of the previous month). Returns `undefined` if the
 * result is outside the supported range. Years 0–1899 are taken as 1900+year, like DATE().
 */
export function dateToSerial(year: number, month: number, day: number, date1904 = false): number | undefined {
  let y = Math.trunc(year);
  const m = Math.trunc(month);
  const d = Math.trunc(day);
  if (y < 0 || y >= 10000) return undefined;
  if (y < 1900) y += 1900;
  const totalMonths = y * 12 + (m - 1);
  const yy = Math.floor(totalMonths / 12);
  const mm = totalMonths - yy * 12; // 0-11
  if (yy < 0 || yy > 9999) return undefined;
  const monthStart = utc(yy, mm, 1);
  let serial: number;
  if (date1904) {
    serial = (monthStart - EPOCH_1904) / DAY_MS + (d - 1);
  } else {
    serial = (monthStart - EPOCH_1900) / DAY_MS + (d - 1);
    // Lotus bug: every month start after February 1900 gains the phantom 29 February.
    if (yy > 1900 || (yy === 1900 && mm > 1)) serial += 1;
  }
  const max = date1904 ? MAX_DATE_SERIAL_1904 : MAX_DATE_SERIAL_1900;
  if (serial < 0 || serial > max) return undefined;
  return serial;
}

/** Date.UTC that also works for years 0–99. */
function utc(year: number, monthIndex: number, day: number): number {
  const d = new Date(Date.UTC(2000, monthIndex, day));
  d.setUTCFullYear(year);
  return d.getTime();
}

/** Converts the integer part of a serial to a calendar date. */
export function serialToDate(serial: number, date1904 = false): DateParts {
  const n = Math.floor(serial);
  if (date1904) return partsOf(EPOCH_1904 + n * DAY_MS);
  if (n === 0) return { year: 1900, month: 1, day: 0 };
  if (n === 60) return { year: 1900, month: 2, day: 29 };
  return partsOf(EPOCH_1900 + (n < 60 ? n : n - 1) * DAY_MS);
}

function partsOf(ms: number): DateParts {
  const d = new Date(ms);
  return { year: d.getUTCFullYear(), month: d.getUTCMonth() + 1, day: d.getUTCDate() };
}

/** Splits the fractional part of a serial into a time of day (rounded to the millisecond). */
export function serialToTime(serial: number): TimeParts {
  let ms = Math.round((serial - Math.floor(serial)) * DAY_MS);
  if (ms >= DAY_MS) ms = DAY_MS - 1;
  const hour = Math.floor(ms / 3_600_000);
  ms -= hour * 3_600_000;
  const minute = Math.floor(ms / 60_000);
  ms -= minute * 60_000;
  const second = Math.floor(ms / 1000);
  return { hour, minute, second, millisecond: ms - second * 1000 };
}

/** Fraction of a day for a time of day (values may overflow, like TIME()). */
export function timeToFraction(hour: number, minute: number, second: number): number {
  return (hour * 3600 + minute * 60 + second) / 86_400;
}

/** Weekday of a serial, 0 = Sunday … 6 = Saturday (serial 0 and the phantom 1900-02-29 follow Excel). */
export function serialWeekday(serial: number, date1904 = false): number {
  const n = Math.floor(serial) + (date1904 ? 1462 : 0);
  // Excel: serial 1 (1900-01-01) is a Sunday in its (Lotus) calendar.
  return (((n - 1) % 7) + 7) % 7;
}

/** Serial for a JavaScript Date using its *local* calendar date and time (what NOW()/TODAY() show). */
export function jsDateToSerial(date: Date, date1904 = false): number {
  const day = dateToSerial(date.getFullYear(), date.getMonth() + 1, date.getDate(), date1904) ?? 0;
  const ms =
    date.getHours() * 3_600_000 +
    date.getMinutes() * 60_000 +
    date.getSeconds() * 1000 +
    date.getMilliseconds();
  return day + ms / DAY_MS;
}

/** Converts a serial to a JavaScript Date in UTC (the phantom 1900-02-29 maps to 1900-03-01). */
export function serialToJsDate(serial: number, date1904 = false): Date {
  const n = Math.floor(serial);
  const frac = serial - n;
  let base: number;
  if (date1904) base = EPOCH_1904 + n * DAY_MS;
  else base = EPOCH_1900 + (n < 60 ? n : n - 1) * DAY_MS;
  return new Date(base + Math.round(frac * DAY_MS));
}

/** Days in a month of the given year (proleptic Gregorian; February 1900 has 29 days in Excel). */
export function daysInMonth(year: number, month: number): number {
  if (month === 2) return year === 1900 || isLeapYear(year) ? 29 : 28;
  return new Date(Date.UTC(2001, month, 0)).getUTCDate();
}

export function isLeapYear(year: number): boolean {
  return (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
}
