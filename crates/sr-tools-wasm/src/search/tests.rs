use super::norm::{clamp, normalize, stem, tokens};
use super::*;
use serde_json::Value;

fn corpus() -> Value {
    serde_json::from_str(CORPUS_JSON).unwrap()
}

fn ids(r: &Results) -> Vec<&str> {
    r.hits.iter().map(|h| h.id.as_str()).collect()
}

fn rank(idx: &SearchIndex, q: &str, id: &str) -> Option<usize> {
    idx.search(q, 100).hits.iter().position(|h| h.id == id)
}

fn tools() -> Vec<(String, String, Vec<String>)> {
    let v = corpus();
    v["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["id"].as_str().unwrap().to_owned(),
                t["name"].as_str().unwrap().to_owned(),
                t["phrases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_str().unwrap().to_owned())
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn the_embedded_corpus_parses_and_every_tool_has_phrases() {
    let idx = SearchIndex::embedded();
    assert!(idx.tool_count() >= 30);
    for (id, _, phrases) in tools() {
        assert!(
            (10..=30).contains(&phrases.len()),
            "{id} has {} phrases",
            phrases.len()
        );
    }
}

#[test]
fn normalize_folds_case_diacritics_and_punctuation() {
    assert_eq!(normalize("  Café-Zoë!! #General "), "cafe zoe general");
    assert_eq!(normalize("Ünit   Converter"), "unit converter");
    assert_eq!(normalize("???"), "");
    assert_eq!(normalize(""), "");
}

#[test]
fn the_suffix_stripper_joins_word_forms() {
    for (a, b) in [
        ("converting", "converts"),
        ("converted", "converting"),
        ("rolls", "rolling"),
        ("tables", "table"),
        ("boxes", "box"),
        ("queries", "query"),
        ("sizes", "size"),
    ] {
        assert_eq!(stem(a), stem(b), "{a} / {b}");
    }
    assert_eq!(stem("class"), "class");
    assert_eq!(stem("is"), "is");
    assert_eq!(stem("running"), "run");
    assert_eq!(
        tokens("How do I convert the images?"),
        vec!["convert", "image"]
    );
}

#[test]
fn clamp_cuts_on_a_character_boundary() {
    let long = "é".repeat(500);
    assert_eq!(clamp(&long).chars().count(), 200);
    assert_eq!(clamp("short"), "short");
}

#[test]
fn stage_one_exact_prefix_and_word_prefix() {
    let idx = SearchIndex::embedded();
    let r = idx.search("calculator", 5);
    assert_eq!(r.hits[0].id, "calculator");
    assert_eq!(r.hits[0].stage, 1);
    assert_eq!(idx.search("calc", 3).hits[0].id, "calculator");
    // An alias.
    assert_eq!(idx.search("sha256", 3).hits[0].id, "hash");
    // A later word of the name.
    assert!(rank(&idx, "converter", "units").is_some_and(|i| i < 3));
    assert_eq!(idx.search("converter", 5).hits[0].stage, 1);
}

#[test]
fn stage_two_fuzzy_with_did_you_mean() {
    let idx = SearchIndex::embedded();
    let r = idx.search("calcualtor", 5);
    assert_eq!(r.hits[0].id, "calculator");
    assert_eq!(r.hits[0].stage, 2);
    assert_eq!(r.did_you_mean.as_deref(), Some("Calculator"));
    // A stage 1 hit means no suggestion.
    assert_eq!(idx.search("calc", 5).did_you_mean, None);
    // Far from everything: no suggestion and no stage 2 hit.
    let far = idx.search("qqqqqqqq", 5);
    assert_eq!(far.did_you_mean, None);
    assert!(far.hits.iter().all(|h| h.stage == 3));
}

#[test]
fn stage_three_finds_a_tool_by_task() {
    let idx = SearchIndex::embedded();
    let r = idx.search("make my picture smaller", 3);
    assert_eq!(r.hits[0].id, "image-studio");
    assert_eq!(r.hits[0].stage, 3);
    assert_eq!(
        idx.search("how do I roll for initiative", 3).hits[0].id,
        "dice"
    );
    assert_eq!(idx.search("turn a video into a gif", 3).hits[0].id, "gif");
}

#[test]
fn channels_and_people_share_the_engine() {
    let mut idx = SearchIndex::embedded();
    let n = idx
        .set_entities(
            r#"[{"kind":"channel","id":"c1","name":"general"},
                {"kind":"channel","id":"c2","name":"Café Talk","aliases":["coffee"]},
                {"kind":"person","id":"u1","name":"Alex","aliases":["alexander"]},
                {"kind":"person","id":"u2","name":"Jo"},
                {"kind":"nonsense"}]"#,
        )
        .unwrap();
    assert_eq!(n, 4);
    assert_eq!(ids(&idx.search("general", 3))[0], "c1");
    assert_eq!(ids(&idx.search("alex", 3))[0], "u1");
    assert_eq!(ids(&idx.search("alx", 3))[0], "u1");
    assert_eq!(ids(&idx.search("cafe", 3))[0], "c2");
    assert_eq!(ids(&idx.search("coffe", 3))[0], "c2");
    assert!(
        idx.search("unit converter", 5)
            .hits
            .iter()
            .any(|h| h.kind == "tool" && h.id == "units")
    );
    // Filters.
    assert_eq!(ids(&idx.search("#gen", 5)), ["c1"]);
    assert!(idx.search("@a", 5).hits.iter().all(|h| h.kind == "person"));
    assert!(idx.search(">cal", 5).hits.iter().all(|h| h.kind == "tool"));
    // Entities are not in stage 3 and set_entities replaces the list.
    idx.set_entities("[]").unwrap();
    assert!(
        idx.search("general", 5)
            .hits
            .iter()
            .all(|h| h.kind == "tool")
    );
    assert!(idx.set_entities("{").is_err());
    assert!(idx.set_entities("{}").is_err());
}

#[test]
fn limit_and_empty_input() {
    let idx = SearchIndex::embedded();
    assert!(idx.search("", 5).hits.is_empty());
    assert!(idx.search("   ", 5).hits.is_empty());
    assert!(idx.search("###", 5).hits.is_empty());
    assert!(idx.search("calc", 0).hits.is_empty());
    assert!(idx.search("t", 4).hits.len() <= 4);
}

#[test]
fn bad_corpora_are_errors_not_panics() {
    for bad in [
        "",
        "[]",
        "{}",
        r#"{"tools":[{"id":"","name":"x"}]}"#,
        r#"{"tools":[{"id":"x"}]}"#,
    ] {
        assert!(SearchIndex::from_corpus(bad).is_err(), "{bad}");
    }
    let ok = SearchIndex::from_corpus(r#"{"tools":[{"id":"x","name":"Xylophone"}]}"#).unwrap();
    assert_eq!(ids(&ok.search("xylo", 3)), ["x"]);
}

#[test]
fn every_tool_is_reachable_by_three_of_its_phrases() {
    let idx = SearchIndex::embedded();
    let mut weak = Vec::new();
    for (id, _, phrases) in tools() {
        let found = phrases
            .iter()
            .filter(|p| rank(&idx, p, &id).is_some_and(|i| i < 3))
            .count();
        if found < 3 {
            weak.push(format!("{id}: {found}/{}", phrases.len()));
        }
    }
    assert!(
        weak.is_empty(),
        "tools with fewer than three phrases in the top three: {weak:?}"
    );
}

#[test]
fn every_tool_is_reachable_by_its_name_with_one_typo() {
    let idx = SearchIndex::embedded();
    let mut bad = Vec::new();
    for (id, name, _) in tools() {
        let chars: Vec<char> = name.to_lowercase().chars().collect();
        let mid = chars.len() / 2;
        let mut swap = chars.clone();
        swap.swap(mid - 1, mid);
        let mut del = chars.clone();
        del.remove(mid);
        let mut sub = chars.clone();
        sub[mid] = if sub[mid] == 'q' { 'z' } else { 'q' };
        let mut ins = chars.clone();
        ins.insert(mid, 'k');
        for (kind, v) in [
            ("swap", swap),
            ("delete", del),
            ("substitute", sub),
            ("insert", ins),
        ] {
            let q: String = v.into_iter().collect();
            if !rank(&idx, &q, &id).is_some_and(|i| i < 3) {
                bad.push(format!("{id} {kind} {q:?} -> {:?}", rank(&idx, &q, &id)));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn every_tool_is_reachable_by_its_prefix() {
    let idx = SearchIndex::embedded();
    let mut bad = Vec::new();
    for (id, name, _) in tools() {
        let norm = normalize(&name);
        let prefix: String = norm.chars().take(4).collect();
        if !rank(&idx, &prefix, &id).is_some_and(|i| i < 5) {
            bad.push(format!("{id} {prefix:?} -> {:?}", rank(&idx, &prefix, &id)));
        }
        // The full name always wins.
        assert_eq!(rank(&idx, &name, &id), Some(0), "{id}");
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn the_json_shape_is_stable() {
    let idx = SearchIndex::embedded();
    let v: Value = serde_json::from_str(&results_json(&idx.search("calcualtor", 2))).unwrap();
    assert_eq!(v["didYouMean"], "Calculator");
    let h = &v["hits"][0];
    assert_eq!(h["kind"], "tool");
    assert_eq!(h["id"], "calculator");
    assert_eq!(h["stage"], 2);
    assert!(h["score"].as_f64().is_some() && h["label"].is_string());
    let none: Value = serde_json::from_str(&results_json(&Results::default())).unwrap();
    assert!(none["didYouMean"].is_null() && none["hits"].as_array().unwrap().is_empty());
}

/// No input panics: random strings over a hostile alphabet (control characters, combining marks,
/// zero-width joiners, astral characters, RTL, very long runs), as queries and as entity names.
#[test]
fn fuzz_no_input_panics() {
    let alphabet: Vec<char> =
        "abcxyz 09#@>-_.\"\\\0\n\t\u{301}\u{200d}\u{fe0f}éßıİǅ\u{1f600}\u{10ffff}中文한ا"
            .chars()
            .collect();
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut idx = SearchIndex::embedded();
    let mut entities = Vec::new();
    for i in 0..200 {
        let len = (next() % 12) as usize;
        let name: String = (0..len)
            .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
            .collect();
        entities.push(serde_json::json!({
            "kind": if i % 2 == 0 { "channel" } else { "person" },
            "id": i.to_string(),
            "name": name,
            "aliases": [name.clone() + "x"],
        }));
    }
    idx.set_entities(&Value::Array(entities).to_string())
        .unwrap();
    for round in 0..3000 {
        let len = match round % 10 {
            0 => 0,
            9 => 1000 + (next() % 500) as usize,
            _ => (next() % 40) as usize,
        };
        let q: String = (0..len)
            .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
            .collect();
        let limit = (next() % 150) as usize;
        let r = idx.search(&q, limit);
        assert!(r.hits.len() <= limit.min(100));
        let _ = results_json(&r);
    }
    // Hostile corpora and entity documents.
    for junk in [
        "null",
        "[null]",
        "[[]]",
        r#"[{"kind":1,"id":2,"name":3}]"#,
        "[1,2,3]",
        "\u{0}",
    ] {
        let _ = idx.set_entities(junk);
        let _ = SearchIndex::from_corpus(junk);
    }
    // Every prefix of every phrase.
    for (_, _, phrases) in tools() {
        for p in phrases {
            let cs: Vec<char> = p.chars().collect();
            for end in 0..=cs.len() {
                let _ = idx.search(&cs[..end].iter().collect::<String>(), 8);
            }
        }
    }
}
