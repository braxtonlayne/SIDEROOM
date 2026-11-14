//! D-719: one table from the three-letter ISO 639-2 codes to the two-letter ISO 639-1 codes.
//!
//! A language is stored in its short form (`eng` becomes `en`) so the catalog's filters, which compare codes as
//! stored, do not split one language in two. Every ISO 639-2 code that has a 639-1 code is here, both the
//! bibliographic and the terminologic forms where they differ (`ger` and `deu`). A language with no two-letter
//! code (`fil`, `mis`, `und`) is not here and stays as it is. Written by hand from the standard.
//!
//! The migration `2970_language_codes_short_form.sql` carries the same pairs in a `VALUES` list; the test
//! `the_sql_pairs_and_the_rust_table_are_the_same_list` keeps the two equal.

/// `(639-2 code, 639-1 code)`, sorted by the first element.
pub const LANGUAGE_PAIRS: &[(&str, &str)] = &[
    ("aar", "aa"),
    ("abk", "ab"),
    ("afr", "af"),
    ("aka", "ak"),
    ("alb", "sq"),
    ("amh", "am"),
    ("ara", "ar"),
    ("arg", "an"),
    ("arm", "hy"),
    ("asm", "as"),
    ("ava", "av"),
    ("ave", "ae"),
    ("aym", "ay"),
    ("aze", "az"),
    ("bak", "ba"),
    ("bam", "bm"),
    ("baq", "eu"),
    ("bel", "be"),
    ("ben", "bn"),
    ("bih", "bh"),
    ("bis", "bi"),
    ("bod", "bo"),
    ("bos", "bs"),
    ("bre", "br"),
    ("bul", "bg"),
    ("bur", "my"),
    ("cat", "ca"),
    ("ces", "cs"),
    ("cha", "ch"),
    ("che", "ce"),
    ("chi", "zh"),
    ("chu", "cu"),
    ("chv", "cv"),
    ("cor", "kw"),
    ("cos", "co"),
    ("cre", "cr"),
    ("cym", "cy"),
    ("cze", "cs"),
    ("dan", "da"),
    ("deu", "de"),
    ("div", "dv"),
    ("dut", "nl"),
    ("dzo", "dz"),
    ("ell", "el"),
    ("eng", "en"),
    ("epo", "eo"),
    ("est", "et"),
    ("eus", "eu"),
    ("ewe", "ee"),
    ("fao", "fo"),
    ("fas", "fa"),
    ("fij", "fj"),
    ("fin", "fi"),
    ("fra", "fr"),
    ("fre", "fr"),
    ("fry", "fy"),
    ("ful", "ff"),
    ("geo", "ka"),
    ("ger", "de"),
    ("gla", "gd"),
    ("gle", "ga"),
    ("glg", "gl"),
    ("glv", "gv"),
    ("gre", "el"),
    ("grn", "gn"),
    ("guj", "gu"),
    ("hat", "ht"),
    ("hau", "ha"),
    ("heb", "he"),
    ("her", "hz"),
    ("hin", "hi"),
    ("hmo", "ho"),
    ("hrv", "hr"),
    ("hun", "hu"),
    ("hye", "hy"),
    ("ibo", "ig"),
    ("ice", "is"),
    ("ido", "io"),
    ("iii", "ii"),
    ("iku", "iu"),
    ("ile", "ie"),
    ("ina", "ia"),
    ("ind", "id"),
    ("ipk", "ik"),
    ("isl", "is"),
    ("ita", "it"),
    ("jav", "jv"),
    ("jpn", "ja"),
    ("kal", "kl"),
    ("kan", "kn"),
    ("kas", "ks"),
    ("kat", "ka"),
    ("kau", "kr"),
    ("kaz", "kk"),
    ("khm", "km"),
    ("kik", "ki"),
    ("kin", "rw"),
    ("kir", "ky"),
    ("kom", "kv"),
    ("kon", "kg"),
    ("kor", "ko"),
    ("kua", "kj"),
    ("kur", "ku"),
    ("lao", "lo"),
    ("lat", "la"),
    ("lav", "lv"),
    ("lim", "li"),
    ("lin", "ln"),
    ("lit", "lt"),
    ("ltz", "lb"),
    ("lub", "lu"),
    ("lug", "lg"),
    ("mac", "mk"),
    ("mah", "mh"),
    ("mal", "ml"),
    ("mao", "mi"),
    ("mar", "mr"),
    ("may", "ms"),
    ("mkd", "mk"),
    ("mlg", "mg"),
    ("mlt", "mt"),
    ("mon", "mn"),
    ("mri", "mi"),
    ("msa", "ms"),
    ("mya", "my"),
    ("nau", "na"),
    ("nav", "nv"),
    ("nbl", "nr"),
    ("nde", "nd"),
    ("ndo", "ng"),
    ("nep", "ne"),
    ("nld", "nl"),
    ("nno", "nn"),
    ("nob", "nb"),
    ("nor", "no"),
    ("nya", "ny"),
    ("oci", "oc"),
    ("oji", "oj"),
    ("ori", "or"),
    ("orm", "om"),
    ("oss", "os"),
    ("pan", "pa"),
    ("per", "fa"),
    ("pli", "pi"),
    ("pol", "pl"),
    ("por", "pt"),
    ("pus", "ps"),
    ("que", "qu"),
    ("roh", "rm"),
    ("ron", "ro"),
    ("rum", "ro"),
    ("run", "rn"),
    ("rus", "ru"),
    ("sag", "sg"),
    ("san", "sa"),
    ("sin", "si"),
    ("slk", "sk"),
    ("slo", "sk"),
    ("slv", "sl"),
    ("sme", "se"),
    ("smo", "sm"),
    ("sna", "sn"),
    ("snd", "sd"),
    ("som", "so"),
    ("sot", "st"),
    ("spa", "es"),
    ("sqi", "sq"),
    ("srd", "sc"),
    ("srp", "sr"),
    ("ssw", "ss"),
    ("sun", "su"),
    ("swa", "sw"),
    ("swe", "sv"),
    ("tah", "ty"),
    ("tam", "ta"),
    ("tat", "tt"),
    ("tel", "te"),
    ("tgk", "tg"),
    ("tgl", "tl"),
    ("tha", "th"),
    ("tib", "bo"),
    ("tir", "ti"),
    ("ton", "to"),
    ("tsn", "tn"),
    ("tso", "ts"),
    ("tuk", "tk"),
    ("tur", "tr"),
    ("twi", "tw"),
    ("uig", "ug"),
    ("ukr", "uk"),
    ("urd", "ur"),
    ("uzb", "uz"),
    ("ven", "ve"),
    ("vie", "vi"),
    ("vol", "vo"),
    ("wel", "cy"),
    ("wln", "wa"),
    ("wol", "wo"),
    ("xho", "xh"),
    ("yid", "yi"),
    ("yor", "yo"),
    ("zha", "za"),
    ("zho", "zh"),
    ("zul", "zu"),
];

/// The two-letter code of a three-letter ISO 639-2 `primary` language subtag (ASCII case-insensitive), or
/// `primary` unchanged when it has none (`fil`, `und`, a two-letter code, junk). The answer is lower case when
/// it is a table hit.
pub fn short_language(primary: &str) -> &str {
    if primary.len() != 3 || !primary.is_ascii() {
        return primary;
    }
    let mut key = [0u8; 3];
    key.copy_from_slice(primary.as_bytes());
    key.make_ascii_lowercase();
    LANGUAGE_PAIRS
        .binary_search_by(|(long, _)| long.as_bytes().cmp(&key[..]))
        .map_or(primary, |i| LANGUAGE_PAIRS[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_pairs() {
        for (long, short) in [
            ("eng", "en"),
            ("fin", "fi"),
            ("jpn", "ja"),
            ("spa", "es"),
            ("por", "pt"),
            ("rus", "ru"),
            ("kor", "ko"),
            ("ara", "ar"),
            ("hin", "hi"),
            ("swe", "sv"),
            ("pol", "pl"),
            ("tur", "tr"),
            ("tgl", "tl"),
            ("heb", "he"),
        ] {
            assert_eq!(short_language(long), short, "{long}");
        }
    }

    #[test]
    fn both_forms_of_the_doubled_ones() {
        for (a, b, short) in [
            ("ger", "deu", "de"),
            ("fre", "fra", "fr"),
            ("chi", "zho", "zh"),
            ("cze", "ces", "cs"),
            ("dut", "nld", "nl"),
            ("gre", "ell", "el"),
            ("per", "fas", "fa"),
            ("rum", "ron", "ro"),
            ("slo", "slk", "sk"),
            ("tib", "bod", "bo"),
            ("wel", "cym", "cy"),
            ("alb", "sqi", "sq"),
        ] {
            assert_eq!(short_language(a), short, "{a}");
            assert_eq!(short_language(b), short, "{b}");
        }
    }

    #[test]
    fn input_case_is_ignored_and_the_answer_is_lower_case() {
        assert_eq!(short_language("ENG"), "en");
        assert_eq!(short_language("Fin"), "fi");
        assert_eq!(short_language("DEU"), "de");
    }

    #[test]
    fn a_language_without_a_short_code_stays() {
        for code in ["fil", "mis", "und", "mul", "yue", "haw", "xyz", ""] {
            assert_eq!(short_language(code), code);
        }
        // Not lower-cased when it is not a hit.
        assert_eq!(short_language("FIL"), "FIL");
    }

    #[test]
    fn a_two_letter_code_or_junk_stays() {
        for code in ["en", "pt", "EN", "e1g", "engl", "e", "\u{e9}ng", "\u{e9}n"] {
            assert_eq!(short_language(code), code);
        }
    }

    #[test]
    fn the_table_is_sorted_has_no_duplicate_key_and_holds_two_lower_case_letters() {
        for w in LANGUAGE_PAIRS.windows(2) {
            assert!(w[0].0 < w[1].0, "{} before {}", w[0].0, w[1].0);
        }
        for (long, short) in LANGUAGE_PAIRS {
            assert!(
                long.len() == 3 && long.bytes().all(|b| b.is_ascii_lowercase()),
                "{long}"
            );
            assert!(
                short.len() == 2 && short.bytes().all(|b| b.is_ascii_lowercase()),
                "{short}"
            );
        }
        let langs: std::collections::BTreeSet<_> = LANGUAGE_PAIRS.iter().map(|p| p.1).collect();
        assert_eq!(langs.len(), 184, "ISO 639-1 has 184 codes");
    }
}
