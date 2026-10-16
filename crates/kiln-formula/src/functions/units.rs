// SPDX-License-Identifier: AGPL-3.0-or-later
//! The unit table of `CONVERT` (Excel's units; factors are the defining values from CODATA/NIST).
//! `factor` converts one unit to its category's base unit; `prefix` allows SI prefixes (and binary
//! prefixes for information units); `power` raises the prefix for area (2) and volume (3) units.

/// A unit category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cat {
    Mass,
    Distance,
    Time,
    Pressure,
    Force,
    Energy,
    Power,
    Magnetism,
    Temperature,
    Volume,
    Area,
    Info,
    Speed,
}

/// Temperature scales (converted through kelvin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Temp {
    C,
    F,
    K,
    Rank,
    Reau,
}

/// One unit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitDef {
    pub(crate) cat: Cat,
    pub(crate) factor: f64,
    pub(crate) prefix: bool,
    pub(crate) power: i32,
    pub(crate) temp: Option<Temp>,
}

const LY: f64 = 9.460_730_472_580_8e15;
const PICA: f64 = 0.0254 / 72.0;
const PICA2: f64 = PICA * PICA;
const PICA3: f64 = PICA * PICA * PICA;
const LY2: f64 = LY * LY;
const LY3: f64 = LY * LY * LY;

const fn u(cat: Cat, factor: f64) -> UnitDef {
    UnitDef {
        cat,
        factor,
        prefix: false,
        power: 1,
        temp: None,
    }
}

const fn p(cat: Cat, factor: f64) -> UnitDef {
    UnitDef {
        cat,
        factor,
        prefix: true,
        power: 1,
        temp: None,
    }
}

const fn pw(cat: Cat, factor: f64, power: i32) -> UnitDef {
    UnitDef {
        cat,
        factor,
        prefix: true,
        power,
        temp: None,
    }
}

const fn t(temp: Temp, prefix: bool) -> UnitDef {
    UnitDef {
        cat: Cat::Temperature,
        factor: 1.0,
        prefix,
        power: 1,
        temp: Some(temp),
    }
}

use Cat::*;

/// Every unit name with its definition (case-sensitive names, as in Excel).
const UNITS: &[(&[&str], UnitDef)] = &[
    // mass (grams)
    (&["g"], p(Mass, 1.0)),
    (&["sg"], u(Mass, 14_593.902_937_206_363)),
    (&["lbm"], u(Mass, 453.592_37)),
    (&["u"], p(Mass, 1.660_538_782e-24)),
    (&["ozm"], u(Mass, 28.349_523_125)),
    (&["grain"], u(Mass, 0.064_798_91)),
    (&["cwt", "shweight"], u(Mass, 45_359.237)),
    (&["uk_cwt", "lcwt", "hweight"], u(Mass, 50_802.345_44)),
    (&["stone"], u(Mass, 6_350.293_18)),
    (&["ton"], u(Mass, 907_184.74)),
    (&["uk_ton", "LTON", "brton"], u(Mass, 1_016_046.908_8)),
    // distance (metres)
    (&["m"], p(Distance, 1.0)),
    (&["mi"], u(Distance, 1_609.344)),
    (&["Nmi"], u(Distance, 1_852.0)),
    (&["in"], u(Distance, 0.0254)),
    (&["ft"], u(Distance, 0.3048)),
    (&["yd"], u(Distance, 0.9144)),
    (&["ang"], p(Distance, 1e-10)),
    (&["ell"], u(Distance, 1.143)),
    (&["ly"], p(Distance, LY)),
    (&["parsec", "pc"], p(Distance, 3.085_677_581_281_55e16)),
    (&["Picapt", "Pica"], u(Distance, PICA)),
    (&["pica"], u(Distance, 0.0254 / 6.0)),
    (&["survey_mi"], u(Distance, 1_609.347_218_694_437)),
    // time (seconds)
    (&["yr"], u(Time, 31_557_600.0)),
    (&["day", "d"], u(Time, 86_400.0)),
    (&["hr"], u(Time, 3_600.0)),
    (&["mn", "min"], u(Time, 60.0)),
    (&["sec", "s"], p(Time, 1.0)),
    // pressure (pascals)
    (&["Pa", "p"], p(Pressure, 1.0)),
    (&["atm", "at"], p(Pressure, 101_325.0)),
    (&["mmHg"], p(Pressure, 133.322_387_415)),
    (&["psi"], u(Pressure, 6_894.757_293_168_36)),
    (&["Torr"], u(Pressure, 101_325.0 / 760.0)),
    // force (newtons)
    (&["N"], p(Force, 1.0)),
    (&["dyn", "dy"], p(Force, 1e-5)),
    (&["lbf"], u(Force, 4.448_221_615_260_5)),
    (&["pond"], p(Force, 0.009_806_65)),
    // energy (joules)
    (&["J"], p(Energy, 1.0)),
    (&["e"], p(Energy, 1e-7)),
    (&["c"], p(Energy, 4.184)),
    (&["cal"], p(Energy, 4.1868)),
    (&["eV", "ev"], p(Energy, 1.602_176_487e-19)),
    (&["HPh", "hh"], u(Energy, 2_684_519.537_696_17)),
    (&["Wh", "wh"], p(Energy, 3_600.0)),
    (&["flb"], u(Energy, 1.355_817_948_331_4)),
    (&["BTU", "btu"], u(Energy, 1_055.055_852_62)),
    // power (watts)
    (&["HP", "h"], u(Power, 745.699_871_582_27)),
    (&["PS"], u(Power, 735.498_75)),
    (&["W", "w"], p(Power, 1.0)),
    // magnetism (tesla)
    (&["T"], p(Magnetism, 1.0)),
    (&["ga"], p(Magnetism, 1e-4)),
    // temperature (converted through kelvin)
    (&["C", "cel"], t(Temp::C, false)),
    (&["F", "fah"], t(Temp::F, false)),
    (&["K", "kel"], t(Temp::K, true)),
    (&["Rank"], t(Temp::Rank, false)),
    (&["Reau"], t(Temp::Reau, false)),
    // volume (cubic metres)
    (&["tsp"], u(Volume, 4.928_921_593_75e-6)),
    (&["tspm"], u(Volume, 5e-6)),
    (&["tbs"], u(Volume, 1.478_676_478_125e-5)),
    (&["oz"], u(Volume, 2.957_352_956_25e-5)),
    (&["cup"], u(Volume, 2.365_882_365e-4)),
    (&["pt", "us_pt"], u(Volume, 4.731_764_73e-4)),
    (&["uk_pt"], u(Volume, 5.682_612_5e-4)),
    (&["qt"], u(Volume, 9.463_529_46e-4)),
    (&["uk_qt"], u(Volume, 1.136_522_5e-3)),
    (&["gal"], u(Volume, 3.785_411_784e-3)),
    (&["uk_gal"], u(Volume, 4.546_09e-3)),
    (&["l", "L", "lt"], p(Volume, 1e-3)),
    (&["ang3", "ang^3"], pw(Volume, 1e-30, 3)),
    (&["barrel"], u(Volume, 0.158_987_294_928)),
    (&["bushel"], u(Volume, 0.035_239_070_166_88)),
    (&["ft3", "ft^3"], u(Volume, 0.028_316_846_592)),
    (&["in3", "in^3"], u(Volume, 1.638_706_4e-5)),
    (&["ly3", "ly^3"], pw(Volume, LY3, 3)),
    (&["m3", "m^3"], pw(Volume, 1.0, 3)),
    (
        &["mi3", "mi^3"],
        u(Volume, 1_609.344 * 1_609.344 * 1_609.344),
    ),
    (&["yd3", "yd^3"], u(Volume, 0.764_554_857_984)),
    (&["Nmi3", "Nmi^3"], u(Volume, 1_852.0 * 1_852.0 * 1_852.0)),
    (
        &["Picapt3", "Picapt^3", "Pica3", "Pica^3"],
        u(Volume, PICA3),
    ),
    (&["GRT", "regton"], u(Volume, 2.831_684_659_2)),
    (&["MTON"], u(Volume, 1.132_673_863_68)),
    // area (square metres)
    (&["uk_acre"], u(Area, 4_046.856_422_4)),
    (&["us_acre"], u(Area, 4_046.872_609_874_25)),
    (&["ang2", "ang^2"], pw(Area, 1e-20, 2)),
    (&["ar"], p(Area, 100.0)),
    (&["ft2", "ft^2"], u(Area, 0.092_903_04)),
    (&["ha"], u(Area, 10_000.0)),
    (&["in2", "in^2"], u(Area, 0.000_645_16)),
    (&["ly2", "ly^2"], pw(Area, LY2, 2)),
    (&["m2", "m^2"], pw(Area, 1.0, 2)),
    (&["Morgen"], u(Area, 2_500.0)),
    (&["mi2", "mi^2"], u(Area, 1_609.344 * 1_609.344)),
    (&["Nmi2", "Nmi^2"], u(Area, 1_852.0 * 1_852.0)),
    (&["Picapt2", "Picapt^2", "Pica2", "Pica^2"], u(Area, PICA2)),
    (&["yd2", "yd^2"], u(Area, 0.836_127_36)),
    // information (bits)
    (&["bit"], p(Info, 1.0)),
    (&["byte"], p(Info, 8.0)),
    // speed (metres per second)
    (&["admkn"], u(Speed, 1_853.184 / 3_600.0)),
    (&["kn"], u(Speed, 1_852.0 / 3_600.0)),
    (&["m/h", "m/hr"], p(Speed, 1.0 / 3_600.0)),
    (&["m/s", "m/sec"], p(Speed, 1.0)),
    (&["mph"], u(Speed, 0.447_04)),
];

/// SI prefixes, in the order they are tried (`da` before `d`).
const PREFIXES: &[(&str, f64)] = &[
    ("Y", 1e24),
    ("Z", 1e21),
    ("E", 1e18),
    ("P", 1e15),
    ("T", 1e12),
    ("G", 1e9),
    ("M", 1e6),
    ("k", 1e3),
    ("h", 1e2),
    ("da", 1e1),
    ("e", 1e1),
    ("d", 1e-1),
    ("c", 1e-2),
    ("m", 1e-3),
    ("u", 1e-6),
    ("n", 1e-9),
    ("p", 1e-12),
    ("f", 1e-15),
    ("a", 1e-18),
    ("z", 1e-21),
    ("y", 1e-24),
];

/// Binary prefixes for information units.
const BINARY_PREFIXES: &[(&str, f64)] = &[
    ("ki", 1_024.0),
    ("Mi", 1_048_576.0),
    ("Gi", 1_073_741_824.0),
    ("Ti", 1_099_511_627_776.0),
    ("Pi", 1_125_899_906_842_624.0),
    ("Ei", 1_152_921_504_606_846_976.0),
    ("Zi", 1_180_591_620_717_411_303_424.0),
    ("Yi", 1_208_925_819_614_629_174_706_176.0),
];

fn direct(name: &str) -> Option<UnitDef> {
    UNITS
        .iter()
        .find(|(names, _)| names.contains(&name))
        .map(|(_, d)| *d)
}

/// A resolved unit: category, factor to the base unit and temperature scale.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitHit {
    pub(crate) cat: Cat,
    pub(crate) factor: f64,
    pub(crate) temp: Option<Temp>,
}

/// Looks a unit up, with SI and binary prefixes.
pub(crate) fn lookup_unit(name: &str) -> Option<UnitHit> {
    if let Some(d) = direct(name) {
        return Some(UnitHit {
            cat: d.cat,
            factor: d.factor,
            temp: d.temp,
        });
    }
    for (p, f) in BINARY_PREFIXES {
        if let Some(rest) = name.strip_prefix(p)
            && let Some(base) = direct(rest)
            && base.cat == Cat::Info
        {
            return Some(UnitHit {
                cat: base.cat,
                factor: base.factor * f,
                temp: None,
            });
        }
    }
    for (p, f) in PREFIXES {
        if name.len() > p.len()
            && let Some(rest) = name.strip_prefix(p)
            && let Some(base) = direct(rest)
            && base.prefix
        {
            return Some(UnitHit {
                cat: base.cat,
                factor: base.factor * f.powi(base.power),
                temp: base.temp,
            });
        }
    }
    None
}

/// A temperature in kelvin.
pub(crate) fn to_kelvin(t: Temp, x: f64) -> f64 {
    match t {
        Temp::C => x + 273.15,
        Temp::F => (x - 32.0) * 5.0 / 9.0 + 273.15,
        Temp::K => x,
        Temp::Rank => x * 5.0 / 9.0,
        Temp::Reau => x * 1.25 + 273.15,
    }
}

/// A temperature from kelvin.
pub(crate) fn from_kelvin(t: Temp, x: f64) -> f64 {
    match t {
        Temp::C => x - 273.15,
        Temp::F => (x - 273.15) * 9.0 / 5.0 + 32.0,
        Temp::K => x,
        Temp::Rank => x * 9.0 / 5.0,
        Temp::Reau => (x - 273.15) * 0.8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_and_categories() {
        assert_eq!(lookup_unit("km").map(|u| u.factor), Some(1000.0));
        assert_eq!(lookup_unit("dam").map(|u| u.factor), Some(10.0));
        assert_eq!(lookup_unit("kibyte").map(|u| u.factor), Some(8192.0));
        assert!(lookup_unit("kft").is_none());
        assert_eq!(lookup_unit("cm2").map(|u| u.factor), Some(1e-4));
        assert_eq!(lookup_unit("mK").and_then(|u| u.temp), Some(Temp::K));
        assert!(lookup_unit("xyz").is_none());
    }
}
