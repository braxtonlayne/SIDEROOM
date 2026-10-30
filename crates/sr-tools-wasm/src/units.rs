//! Unit conversion (UTL-CALC-001): length, mass, volume, temperature, data, speed, time and area.
//!
//! One table, `UNITS`: each unit has a stable id (the key the UI localises the name with), a symbol, a factor to
//! the category's base unit and the spellings the text parser accepts. Temperature is the only category with
//! offsets and is converted through kelvin. A month is 30.4375 days and a year 365.25 days; US customary
//! volumes are US ones (`impgal` is the imperial gallon); data has decimal (`kb`, `mb`...) and binary (`kib`...)
//! prefixes, bytes and bits.
//!
//! Text queries: `5 km to mi`, `12 lb in kg`, `98.6 F -> C`, `1/2 cup = ml`; the number may be an expression.

use crate::calc::{CalcError, Options, err, error_json, eval, format_number};
use serde_json::json;
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Length,
    Mass,
    Volume,
    Temperature,
    Data,
    Speed,
    Time,
    Area,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Length,
        Category::Mass,
        Category::Volume,
        Category::Temperature,
        Category::Data,
        Category::Speed,
        Category::Time,
        Category::Area,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Category::Length => "length",
            Category::Mass => "mass",
            Category::Volume => "volume",
            Category::Temperature => "temperature",
            Category::Data => "data",
            Category::Speed => "speed",
            Category::Time => "time",
            Category::Area => "area",
        }
    }
}

#[derive(Debug)]
pub struct Unit {
    pub id: &'static str,
    pub cat: Category,
    pub symbol: &'static str,
    /// How many base units one of this unit is (unused for temperature).
    pub factor: f64,
    /// Spellings accepted by the text parser (exact case first, then lowercase and without a plural `s`).
    pub aliases: &'static [&'static str],
}

use Category as C;

const fn u(
    id: &'static str,
    cat: Category,
    symbol: &'static str,
    factor: f64,
    aliases: &'static [&'static str],
) -> Unit {
    Unit {
        id,
        cat,
        symbol,
        factor,
        aliases,
    }
}

/// Base units: metre, kilogram, litre, kelvin, bit, metre per second, second, square metre.
pub static UNITS: &[Unit] = &[
    // length (m)
    u(
        "um",
        C::Length,
        "\u{00b5}m",
        1e-6,
        &[
            "um",
            "\u{00b5}m",
            "\u{03bc}m",
            "micron",
            "micrometer",
            "micrometre",
        ],
    ),
    u(
        "mm",
        C::Length,
        "mm",
        1e-3,
        &["mm", "millimeter", "millimetre"],
    ),
    u(
        "cm",
        C::Length,
        "cm",
        1e-2,
        &["cm", "centimeter", "centimetre"],
    ),
    u("m", C::Length, "m", 1.0, &["m", "meter", "metre"]),
    u(
        "km",
        C::Length,
        "km",
        1e3,
        &["km", "kilometer", "kilometre"],
    ),
    u(
        "in",
        C::Length,
        "in",
        0.0254,
        &["in", "inch", "inches", "\""],
    ),
    u("ft", C::Length, "ft", 0.3048, &["ft", "foot", "feet", "'"]),
    u("yd", C::Length, "yd", 0.9144, &["yd", "yard"]),
    u("mi", C::Length, "mi", 1609.344, &["mi", "mile"]),
    u("nmi", C::Length, "nmi", 1852.0, &["nmi", "nautical mile"]),
    // mass (kg)
    u("mg", C::Mass, "mg", 1e-6, &["mg", "milligram"]),
    u("g", C::Mass, "g", 1e-3, &["g", "gram"]),
    u("kg", C::Mass, "kg", 1.0, &["kg", "kilogram", "kilo"]),
    u("t", C::Mass, "t", 1000.0, &["t", "tonne", "metric ton"]),
    u("oz", C::Mass, "oz", 0.028_349_523_125, &["oz", "ounce"]),
    u("lb", C::Mass, "lb", 0.453_592_37, &["lb", "lbs", "pound"]),
    u("st", C::Mass, "st", 6.350_293_18, &["st", "stone"]),
    u(
        "ton",
        C::Mass,
        "ton",
        907.184_74,
        &["ton", "short ton", "us ton"],
    ),
    // volume (L)
    u(
        "ml",
        C::Volume,
        "ml",
        1e-3,
        &["ml", "mL", "milliliter", "millilitre"],
    ),
    u(
        "cl",
        C::Volume,
        "cl",
        1e-2,
        &["cl", "cL", "centiliter", "centilitre"],
    ),
    u("l", C::Volume, "L", 1.0, &["l", "L", "liter", "litre"]),
    u(
        "m3",
        C::Volume,
        "m\u{00b3}",
        1000.0,
        &["m3", "m\u{00b3}", "cubic meter", "cubic metre"],
    ),
    u(
        "tsp",
        C::Volume,
        "tsp",
        0.004_928_921_593_75,
        &["tsp", "teaspoon"],
    ),
    u(
        "tbsp",
        C::Volume,
        "tbsp",
        0.014_786_764_781_25,
        &["tbsp", "tablespoon"],
    ),
    u(
        "floz",
        C::Volume,
        "fl oz",
        0.029_573_529_562_5,
        &["floz", "fl oz", "fl. oz", "fluid ounce"],
    ),
    u("cup", C::Volume, "cup", 0.236_588_236_5, &["cup"]),
    u("pt", C::Volume, "pt", 0.473_176_473, &["pt", "pint"]),
    u("qt", C::Volume, "qt", 0.946_352_946, &["qt", "quart"]),
    u(
        "gal",
        C::Volume,
        "gal",
        3.785_411_784,
        &["gal", "gallon", "us gallon"],
    ),
    u(
        "impgal",
        C::Volume,
        "imp gal",
        4.546_09,
        &["impgal", "imp gal", "imperial gallon"],
    ),
    // temperature (through kelvin)
    u(
        "c",
        C::Temperature,
        "\u{00b0}C",
        1.0,
        &["c", "C", "\u{00b0}c", "\u{00b0}C", "celsius", "centigrade"],
    ),
    u(
        "f",
        C::Temperature,
        "\u{00b0}F",
        1.0,
        &["f", "F", "\u{00b0}f", "\u{00b0}F", "fahrenheit"],
    ),
    u("k", C::Temperature, "K", 1.0, &["k", "K", "kelvin"]),
    // data (bit)
    u("bit", C::Data, "bit", 1.0, &["bit", "b"]),
    u("byte", C::Data, "B", 8.0, &["B", "byte"]),
    u("kbit", C::Data, "kbit", 1e3, &["kbit", "Kb", "kilobit"]),
    u(
        "mbit",
        C::Data,
        "Mbit",
        1e6,
        &["mbit", "Mbit", "Mb", "megabit"],
    ),
    u(
        "gbit",
        C::Data,
        "Gbit",
        1e9,
        &["gbit", "Gbit", "Gb", "gigabit"],
    ),
    u("kb", C::Data, "kB", 8e3, &["kB", "KB", "kb", "kilobyte"]),
    u("mb", C::Data, "MB", 8e6, &["MB", "mb", "megabyte"]),
    u("gb", C::Data, "GB", 8e9, &["GB", "gb", "gigabyte"]),
    u("tb", C::Data, "TB", 8e12, &["TB", "tb", "terabyte"]),
    u("pb", C::Data, "PB", 8e15, &["PB", "pb", "petabyte"]),
    u(
        "kib",
        C::Data,
        "KiB",
        8.0 * 1024.0,
        &["KiB", "kib", "kibibyte"],
    ),
    u(
        "mib",
        C::Data,
        "MiB",
        8.0 * 1_048_576.0,
        &["MiB", "mib", "mebibyte"],
    ),
    u(
        "gib",
        C::Data,
        "GiB",
        8.0 * 1_073_741_824.0,
        &["GiB", "gib", "gibibyte"],
    ),
    u(
        "tib",
        C::Data,
        "TiB",
        8.0 * 1_099_511_627_776.0,
        &["TiB", "tib", "tebibyte"],
    ),
    u(
        "pib",
        C::Data,
        "PiB",
        8.0 * 1_125_899_906_842_624.0,
        &["PiB", "pib", "pebibyte"],
    ),
    // speed (m/s)
    u(
        "mps",
        C::Speed,
        "m/s",
        1.0,
        &["m/s", "mps", "meters per second", "metres per second"],
    ),
    u(
        "kmh",
        C::Speed,
        "km/h",
        1000.0 / 3600.0,
        &[
            "km/h",
            "kmh",
            "kph",
            "kmph",
            "kilometers per hour",
            "kilometres per hour",
        ],
    ),
    u(
        "mph",
        C::Speed,
        "mph",
        0.447_04,
        &["mph", "mi/h", "miles per hour"],
    ),
    u(
        "fps",
        C::Speed,
        "ft/s",
        0.3048,
        &["ft/s", "fps", "feet per second"],
    ),
    u("kn", C::Speed, "kn", 1852.0 / 3600.0, &["kn", "kt", "knot"]),
    u("mach", C::Speed, "Mach", 340.29, &["mach"]),
    // time (s)
    u("ns", C::Time, "ns", 1e-9, &["ns", "nanosecond"]),
    u(
        "us",
        C::Time,
        "\u{00b5}s",
        1e-6,
        &["us", "\u{00b5}s", "\u{03bc}s", "microsecond"],
    ),
    u("ms", C::Time, "ms", 1e-3, &["ms", "millisecond"]),
    u("s", C::Time, "s", 1.0, &["s", "sec", "second"]),
    u("min", C::Time, "min", 60.0, &["min", "minute"]),
    u("h", C::Time, "h", 3600.0, &["h", "hr", "hour"]),
    u("d", C::Time, "d", 86_400.0, &["d", "day"]),
    u("wk", C::Time, "wk", 604_800.0, &["wk", "w", "week"]),
    u("mo", C::Time, "mo", 2_629_800.0, &["mo", "month"]),
    u("yr", C::Time, "yr", 31_557_600.0, &["yr", "y", "year"]),
    // area (m2)
    u(
        "mm2",
        C::Area,
        "mm\u{00b2}",
        1e-6,
        &["mm2", "mm\u{00b2}", "sq mm"],
    ),
    u(
        "cm2",
        C::Area,
        "cm\u{00b2}",
        1e-4,
        &["cm2", "cm\u{00b2}", "sq cm"],
    ),
    u(
        "m2",
        C::Area,
        "m\u{00b2}",
        1.0,
        &[
            "m2",
            "m\u{00b2}",
            "sqm",
            "sq m",
            "square meter",
            "square metre",
        ],
    ),
    u("ha", C::Area, "ha", 10_000.0, &["ha", "hectare"]),
    u(
        "km2",
        C::Area,
        "km\u{00b2}",
        1e6,
        &["km2", "km\u{00b2}", "sq km"],
    ),
    u(
        "in2",
        C::Area,
        "in\u{00b2}",
        0.000_645_16,
        &["in2", "in\u{00b2}", "sq in", "square inch", "square inches"],
    ),
    u(
        "ft2",
        C::Area,
        "ft\u{00b2}",
        0.092_903_04,
        &[
            "ft2",
            "ft\u{00b2}",
            "sqft",
            "sq ft",
            "square foot",
            "square feet",
        ],
    ),
    u(
        "yd2",
        C::Area,
        "yd\u{00b2}",
        0.836_127_36,
        &["yd2", "yd\u{00b2}", "sq yd", "square yard"],
    ),
    u("ac", C::Area, "ac", 4_046.856_422_4, &["ac", "acre"]),
    u(
        "mi2",
        C::Area,
        "mi\u{00b2}",
        2_589_988.110_336,
        &["mi2", "mi\u{00b2}", "sq mi", "square mile"],
    ),
];

pub fn find(id: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|u| u.id == id)
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The unit a spelling names: an exact (case-sensitive) alias, else lowercase, else lowercase without a plural.
pub fn lookup(spelling: &str) -> Option<&'static Unit> {
    let s = squash(spelling.trim().trim_end_matches('.'));
    if s.is_empty() {
        return None;
    }
    let exact = |needle: &str| UNITS.iter().find(|u| u.aliases.contains(&needle));
    if let Some(u) = exact(&s) {
        return Some(u);
    }
    let lower = s.to_lowercase();
    let by_lower = |needle: &str| {
        UNITS
            .iter()
            .find(|u| u.aliases.iter().any(|a| a.to_lowercase() == needle))
    };
    // A lowercase spelling prefers an alias that is itself lowercase (`mb` is megabyte, `Mb` megabit).
    if let Some(u) = exact(&lower).or_else(|| by_lower(&lower)) {
        return Some(u);
    }
    for suffix in ["es", "s"] {
        if let Some(stem) = lower.strip_suffix(suffix)
            && let Some(u) = exact(stem).or_else(|| by_lower(stem))
        {
            return Some(u);
        }
    }
    None
}

fn kelvin_of(v: f64, id: &str) -> f64 {
    match id {
        "c" => v + 273.15,
        "f" => (v - 32.0) * 5.0 / 9.0 + 273.15,
        _ => v,
    }
}

fn from_kelvin(k: f64, id: &str) -> f64 {
    match id {
        "c" => k - 273.15,
        "f" => (k - 273.15) * 9.0 / 5.0 + 32.0,
        _ => k,
    }
}

/// Convert `value` between two units of the same category.
pub fn convert(value: f64, from: &Unit, to: &Unit) -> Result<f64, CalcError> {
    if from.cat != to.cat {
        return Err(err("category_mismatch", 0));
    }
    if !value.is_finite() {
        return Err(err("overflow", 0));
    }
    let out = if from.cat == Category::Temperature {
        let k = kelvin_of(value, from.id);
        if k < -1e-9 {
            return Err(err("below_absolute_zero", 0));
        }
        from_kelvin(k.max(0.0), to.id)
    } else {
        value * from.factor / to.factor
    };
    if out.is_finite() {
        Ok(out)
    } else {
        Err(err("overflow", 0))
    }
}

fn clean(x: f64) -> f64 {
    // 12 significant digits are shown; drop the float noise from the stored value too (0.1 + 0.2 style).
    format_number(x).parse().unwrap_or(x)
}

/// Splits `5 km to mi` into the left and right text.
fn split_query(text: &str) -> Option<(&str, &str)> {
    for sep in ["->", "\u{2192}", "=>", "="] {
        if let Some((l, r)) = text.split_once(sep) {
            return Some((l, r));
        }
    }
    let lower = text.to_ascii_lowercase();
    for sep in [" to ", " as "] {
        if let Some(at) = lower.find(sep) {
            return Some((&text[..at], &text[at + sep.len()..]));
        }
    }
    if let Some(at) = lower.rfind(" in ") {
        return Some((&text[..at], &text[at + 4..]));
    }
    None
}

/// Length in bytes of the leading numeric expression of `s` (digits, operators, parentheses, exponent marks).
fn numeric_prefix(s: &str) -> usize {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut end = 0;
    let mut k = 0;
    while k < chars.len() {
        let (at, c) = chars[k];
        let ok = match c {
            '0'..='9' | '.' | '_' | ' ' | '+' | '-' | '*' | '/' | '^' | '(' | ')' => true,
            'e' | 'E' => {
                let n1 = chars.get(k + 1).map(|x| x.1);
                let n2 = chars.get(k + 2).map(|x| x.1);
                matches!(n1, Some('0'..='9'))
                    || (matches!(n1, Some('+' | '-')) && matches!(n2, Some('0'..='9')))
            }
            _ => false,
        };
        if !ok {
            break;
        }
        end = at + c.len_utf8();
        k += 1;
    }
    end
}

/// A parsed query.
#[derive(Debug, Clone, Copy)]
pub struct Query {
    pub value: f64,
    pub from: &'static Unit,
    pub to: &'static Unit,
}

pub fn parse_query(text: &str) -> Result<Query, CalcError> {
    let (left, right) = split_query(text.trim()).ok_or_else(|| err("no_target", 0))?;
    let split = numeric_prefix(left);
    let (num, from_text) = left.split_at(split);
    let num = num
        .trim()
        .trim_end_matches(['+', '-', '*', '/', '^'])
        .trim();
    let value = if num.is_empty() {
        1.0
    } else {
        eval(num, &Options::default())?
    };
    let from = lookup(from_text).ok_or_else(|| {
        let mut e = err("unknown_unit", 0);
        e.detail = from_text.trim().to_owned();
        e
    })?;
    let to = lookup(right).ok_or_else(|| {
        let mut e = err("unknown_unit", 0);
        e.detail = right.trim().to_owned();
        e
    })?;
    Ok(Query { value, from, to })
}

/// Every unit, grouped: `[{category, units:[{id, symbol}]}]`.
#[wasm_bindgen(js_name = unitsList)]
pub fn units_list() -> String {
    let cats: Vec<_> = Category::ALL
        .iter()
        .map(|c| {
            let units: Vec<_> = UNITS
                .iter()
                .filter(|u| u.cat == *c)
                .map(|u| json!({ "id": u.id, "symbol": u.symbol }))
                .collect();
            json!({ "category": c.id(), "units": units })
        })
        .collect();
    json!({ "categories": cats }).to_string()
}

/// `{ok, value, text, category}` or an error.
#[wasm_bindgen(js_name = unitsConvert)]
pub fn units_convert(value: f64, from: &str, to: &str) -> String {
    let (Some(f), Some(t)) = (find(from), find(to)) else {
        return error_json(&err("unknown_unit", 0));
    };
    match convert(value, f, t) {
        Ok(v) => {
            let v = clean(v);
            json!({ "ok": true, "value": v, "text": format_number(v), "category": f.cat.id() })
                .to_string()
        }
        Err(e) => error_json(&e),
    }
}

/// The value in every unit of `from`'s category: `{ok, rows:[{id, value, text}]}`.
#[wasm_bindgen(js_name = unitsConvertAll)]
pub fn units_convert_all(value: f64, from: &str) -> String {
    let Some(f) = find(from) else {
        return error_json(&err("unknown_unit", 0));
    };
    let mut rows = Vec::new();
    for t in UNITS.iter().filter(|u| u.cat == f.cat) {
        match convert(value, f, t) {
            Ok(v) => {
                let v = clean(v);
                rows.push(json!({ "id": t.id, "value": v, "text": format_number(v) }));
            }
            Err(e) => return error_json(&e),
        }
    }
    json!({ "ok": true, "rows": rows, "category": f.cat.id() }).to_string()
}

/// A text query such as `5 km to mi`: `{ok, value, from, to, result, text, category}` or an error.
#[wasm_bindgen(js_name = unitsQuery)]
pub fn units_query(text: &str) -> String {
    let q = match parse_query(text) {
        Ok(q) => q,
        Err(e) => return error_json(&e),
    };
    match convert(q.value, q.from, q.to) {
        Ok(v) => {
            let v = clean(v);
            json!({
                "ok": true,
                "value": q.value,
                "from": q.from.id,
                "to": q.to.id,
                "result": v,
                "text": format_number(v),
                "category": q.from.cat.id(),
            })
            .to_string()
        }
        Err(e) => error_json(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn conv(v: f64, from: &str, to: &str) -> String {
        match convert(v, find(from).unwrap(), find(to).unwrap()) {
            Ok(x) => format_number(x),
            Err(e) => format!("!{}", e.code),
        }
    }

    fn query(text: &str) -> String {
        match parse_query(text).and_then(|q| convert(q.value, q.from, q.to)) {
            Ok(x) => format_number(x),
            Err(e) => format!("!{}", e.code),
        }
    }

    #[test]
    fn table_is_sound() {
        let mut ids = HashSet::new();
        let mut spellings = HashSet::new();
        for u in UNITS {
            assert!(ids.insert(u.id), "duplicate id {}", u.id);
            assert!(
                u.id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "id {} is not a safe key segment",
                u.id
            );
            assert!(u.factor > 0.0, "{}", u.id);
            assert!(!u.aliases.is_empty(), "{}", u.id);
            for a in u.aliases {
                assert!(spellings.insert(*a), "alias {a} is used twice");
            }
        }
        for c in Category::ALL {
            assert!(
                UNITS.iter().filter(|u| u.cat == c).count() >= 3,
                "{}",
                c.id()
            );
        }
    }

    #[test]
    fn every_symbol_and_id_resolves() {
        for u in UNITS {
            assert_eq!(lookup(u.id).map(|x| x.id), Some(u.id), "id {}", u.id);
            assert!(lookup(u.symbol).is_some(), "symbol {}", u.symbol);
        }
    }

    #[test]
    fn conversions() {
        let cases: &[(f64, &str, &str, &str)] = &[
            (1.0, "km", "m", "1000"),
            (1.0, "mi", "km", "1.609344"),
            (1.0, "km", "mi", "0.621371192237"),
            (12.0, "in", "ft", "1"),
            (1.0, "ft", "cm", "30.48"),
            (1.0, "yd", "m", "0.9144"),
            (100.0, "cm", "m", "1"),
            (1.0, "nmi", "m", "1852"),
            (1.0, "m", "um", "1000000"),
            (1.0, "kg", "g", "1000"),
            (1.0, "lb", "kg", "0.45359237"),
            (1.0, "kg", "lb", "2.20462262185"),
            (16.0, "oz", "lb", "1"),
            (1.0, "st", "lb", "14"),
            (1.0, "t", "kg", "1000"),
            (1.0, "ton", "lb", "2000"),
            (500.0, "mg", "g", "0.5"),
            (1.0, "l", "ml", "1000"),
            (1.0, "gal", "l", "3.785411784"),
            (1.0, "gal", "qt", "4"),
            (1.0, "qt", "pt", "2"),
            (1.0, "pt", "cup", "2"),
            (1.0, "cup", "floz", "8"),
            (1.0, "floz", "tbsp", "2"),
            (1.0, "tbsp", "tsp", "3"),
            (1.0, "m3", "l", "1000"),
            (1.0, "impgal", "l", "4.54609"),
            (0.0, "c", "f", "32"),
            (100.0, "c", "f", "212"),
            (-40.0, "c", "f", "-40"),
            (98.6, "f", "c", "37"),
            (0.0, "c", "k", "273.15"),
            (0.0, "k", "c", "-273.15"),
            (300.0, "k", "f", "80.33"),
            (212.0, "f", "k", "373.15"),
            (-300.0, "c", "k", "!below_absolute_zero"),
            (-500.0, "f", "c", "!below_absolute_zero"),
            (1.0, "byte", "bit", "8"),
            (1.0, "kb", "byte", "1000"),
            (1.0, "kib", "byte", "1024"),
            (1.0, "mb", "kb", "1000"),
            (1.0, "mib", "kib", "1024"),
            (1.0, "gb", "mb", "1000"),
            (1.0, "gib", "mib", "1024"),
            (1.0, "tb", "gb", "1000"),
            (1.0, "pib", "tib", "1024"),
            (8.0, "mbit", "mb", "1"),
            (1.0, "gb", "gib", "0.931322574615"),
            (1.0, "kmh", "mps", "0.277777777778"),
            (36.0, "kmh", "mps", "10"),
            (60.0, "mph", "kmh", "96.56064"),
            (1.0, "kn", "kmh", "1.852"),
            (1.0, "fps", "mps", "0.3048"),
            (1.0, "mach", "mps", "340.29"),
            (1.0, "min", "s", "60"),
            (1.0, "h", "min", "60"),
            (1.0, "d", "h", "24"),
            (1.0, "wk", "d", "7"),
            (1.0, "yr", "d", "365.25"),
            (1.0, "mo", "d", "30.4375"),
            (1500.0, "ms", "s", "1.5"),
            (1.0, "s", "ns", "1000000000"),
            (1.0, "ha", "m2", "10000"),
            (1.0, "km2", "ha", "100"),
            (1.0, "ac", "m2", "4046.8564224"),
            (1.0, "ft2", "in2", "144"),
            (1.0, "mi2", "ac", "640"),
            (1.0, "yd2", "ft2", "9"),
            (10000.0, "cm2", "m2", "1"),
            (0.0, "kg", "lb", "0"),
            (-5.0, "m", "cm", "-500"),
            (1e308, "km", "mm", "!overflow"),
            (1.0, "km", "kg", "!category_mismatch"),
            (1.0, "c", "m", "!category_mismatch"),
        ];
        let mut bad = Vec::new();
        for (v, f, t, want) in cases {
            let got = conv(*v, f, t);
            if got != *want {
                bad.push(format!("{v} {f} -> {t}: got {got}, want {want}"));
            }
        }
        assert!(
            bad.is_empty(),
            "{} of {}:\n{}",
            bad.len(),
            cases.len(),
            bad.join("\n")
        );
    }

    #[test]
    fn round_trips_within_a_category() {
        for from in UNITS {
            for to in UNITS.iter().filter(|u| u.cat == from.cat) {
                if from.cat == Category::Temperature {
                    continue;
                }
                let there = convert(123.456, from, to).unwrap();
                let back = convert(there, to, from).unwrap();
                assert!(
                    (back - 123.456).abs() < 1e-9 * 123.456,
                    "{} -> {} -> {}: {back}",
                    from.id,
                    to.id,
                    from.id
                );
            }
        }
    }

    #[test]
    fn temperature_round_trips() {
        for from in ["c", "f", "k"] {
            for to in ["c", "f", "k"] {
                let there = convert(300.0, find(from).unwrap(), find(to).unwrap()).unwrap();
                let back = convert(there, find(to).unwrap(), find(from).unwrap()).unwrap();
                assert!((back - 300.0).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn text_queries() {
        let cases: &[(&str, &str)] = &[
            ("5 km to mi", "3.10685596119"),
            ("5km to mi", "3.10685596119"),
            ("5 KM TO MI", "3.10685596119"),
            ("12 lb in kg", "5.44310844"),
            ("12 pounds to kilograms", "5.44310844"),
            ("98.6 F -> C", "37"),
            ("98.6 \u{00b0}F to \u{00b0}C", "37"),
            ("100 c = f", "212"),
            ("1/2 cup to ml", "118.29411825"),
            ("2.5e3 m to km", "2.5"),
            ("1e3 g to kg", "1"),
            ("2 * 3 ft to in", "72"),
            ("km to m", "1000"),
            ("10 in to cm", "25.4"),
            ("10 in in cm", "25.4"),
            ("10 inches to centimeters", "25.4"),
            ("5'  to cm", "152.4"),
            ("1 mb to kb", "1000"),
            ("1 Mb to kb", "125"),
            ("1 MiB to KiB", "1024"),
            ("1 GB to MB", "1000"),
            ("100 km/h to mph", "62.1371192237"),
            ("60 mph to km/h", "96.56064"),
            ("1 hour to minutes", "60"),
            ("2 weeks to days", "14"),
            ("1 acre to sq ft", "43560"),
            ("1 sq mi to acres", "640"),
            ("3 tsp to tbsp", "1"),
            ("1 fl oz to ml", "29.5735295625"),
            ("1 metre to feet", "3.28083989501"),
            ("-40 c to f", "-40"),
            ("5 km", "!no_target"),
            ("5 xyz to km", "!unknown_unit"),
            ("5 km to xyz", "!unknown_unit"),
            ("5 km to kg", "!category_mismatch"),
            ("5 / 0 km to m", "!divide_by_zero"),
            ("", "!no_target"),
        ];
        let mut bad = Vec::new();
        for (q, want) in cases {
            let got = query(q);
            if got != *want {
                bad.push(format!("{q:?}: got {got}, want {want}"));
            }
        }
        assert!(
            bad.is_empty(),
            "{} of {}:\n{}",
            bad.len(),
            cases.len(),
            bad.join("\n")
        );
    }

    #[test]
    fn json_shapes() {
        let v: serde_json::Value = serde_json::from_str(&units_query("5 km to mi")).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["from"], "km");
        assert_eq!(v["to"], "mi");
        assert_eq!(v["category"], "length");
        assert_eq!(v["text"], "3.10685596119");
        let v: serde_json::Value = serde_json::from_str(&units_convert_all(1.0, "m")).unwrap();
        assert_eq!(v["rows"].as_array().unwrap().len(), 10);
        let v: serde_json::Value = serde_json::from_str(&units_convert(1.0, "m", "nope")).unwrap();
        assert_eq!(v["error"]["code"], "unknown_unit");
        let v: serde_json::Value = serde_json::from_str(&units_list()).unwrap();
        assert_eq!(v["categories"].as_array().unwrap().len(), 8);
    }
}
