use super::*;
use proptest::prelude::*;

/// Compact textual rendering of the AST for concise assertions.
fn show(nodes: &[Node]) -> String {
    nodes.iter().map(show1).collect::<Vec<_>>().join(" ")
}

fn show1(n: &Node) -> String {
    let wrap = |name: &str, c: &[Node]| format!("{name}[{}]", show(c));
    match n {
        Node::Text { text } => format!("{text:?}"),
        Node::LineBreak => "BR".into(),
        Node::Bold { children } => wrap("b", children),
        Node::Italic { children } => wrap("i", children),
        Node::Underline { children } => wrap("u", children),
        Node::Strike { children } => wrap("s", children),
        Node::Spoiler { children } => wrap("sp", children),
        Node::InlineCode { code } => format!("code({code:?})"),
        Node::CodeBlock { lang, code } => format!("block({lang:?},{code:?})"),
        Node::Quote { children } => wrap("q", children),
        Node::Heading { level, children } => wrap(&format!("h{level}"), children),
        Node::Subtext { children } => wrap("sub", children),
        Node::List {
            ordered,
            start,
            items,
        } => {
            let it: Vec<String> = items.iter().map(|i| format!("({})", show(i))).collect();
            let tag = if *ordered {
                format!("ol{}", start.unwrap_or(1))
            } else {
                "ul".into()
            };
            format!("{tag}[{}]", it.join(" "))
        }
        Node::Link {
            url,
            kind,
            children,
        } => {
            let k = match kind {
                LinkKind::Masked => "link",
                LinkKind::Auto => "auto",
                LinkKind::AutoSuppressed => "nolink",
            };
            format!("{k}<{url}>[{}]", show(children))
        }
        Node::UserMention { id } => format!("@{id}"),
        Node::RoleMention { id } => format!("@&{id}"),
        Node::ChannelMention { id } => format!("#{id}"),
        Node::Everyone => "@everyone".into(),
        Node::Here => "@here".into(),
        Node::CustomEmoji { name, id, animated } => {
            format!("emoji({}{name},{id})", if *animated { "a:" } else { "" })
        }
        Node::Emoji { name, emoji } => format!("emo({name},{emoji})"),
        Node::Timestamp { unix, style } => format!("ts({unix},{style:?})"),
        Node::SlashCommand { name, id } => format!("cmd({name},{id})"),
        Node::GuildNav { kind, id } => format!("nav({kind},{id:?})"),
        Node::Cinema { id } => format!("cinema({id})"),
        Node::Library { id } => format!("library({id})"),
    }
}

fn p(s: &str) -> String {
    show(&parse(s))
}

#[test]
fn corpus() {
    let cases: &[(&str, &str)] = &[
        // plain
        ("", ""),
        ("hello", r#""hello""#),
        ("héllo wörld 😄", r#""héllo wörld 😄""#),
        ("a\nb", r#""a" BR "b""#),
        ("a\n\nb", r#""a" BR BR "b""#),
        // emphasis
        ("**b**", r#"b["b"]"#),
        ("*i*", r#"i["i"]"#),
        ("_i_", r#"i["i"]"#),
        ("__u__", r#"u["u"]"#),
        ("~~s~~", r#"s["s"]"#),
        ("||x||", r#"sp["x"]"#),
        ("***bi***", r#"b[i["bi"]]"#),
        ("___ui___", r#"u[i["ui"]]"#),
        ("a **b** c", r#""a " b["b"] " c""#),
        ("**bold *it***", r#"b["bold " i["it"]]"#),
        ("**a __b__ c**", r#"b["a " u["b"] " c"]"#),
        ("*a **b** c*", r#"i["a " b["b"] " c"]"#),
        ("||*x*||", r#"sp[i["x"]]"#),
        ("**multi\nline**", r#"b["multi" BR "line"]"#),
        // not emphasis
        ("**unclosed", r#""**unclosed""#),
        ("*a *", r#""*a *""#),
        ("**", r#""**""#),
        ("****", r#""****""#),
        ("snake_case_word", r#""snake_case_word""#),
        ("2 * 3 * 4", r#""2 * 3 * 4""#),
        ("~single~", r#""~single~""#),
        ("| x |", r#""| x |""#),
        ("||", r#""||""#),
        // escapes
        (r"\*not\*", r#""*not*""#),
        (r"\**x*\*", r#""*" i["x"] "*""#),
        (r"\\", r#""\\""#),
        (r"\a", r#""\\a""#),
        (r"\`x\`", r#""`x`""#),
        (r"\||x\||", r#""||x||""#),
        (r"\<@1>", r#""<@1>""#),
        (r"trailing\", r#""trailing\\""#),
        // inline code
        ("`c`", r#"code("c")"#),
        ("``a`b``", r#"code("a`b")"#),
        ("`**x**`", r#"code("**x**")"#),
        ("`` ` ``", r#"code("`")"#),
        ("`unclosed", r#""`unclosed""#),
        ("**a `*` b**", r#"b["a " code("*") " b"]"#),
        ("`a` and `b`", r#"code("a") " and " code("b")"#),
        // code blocks
        ("```\ncode\n```", r#"block(None,"code")"#),
        (
            "```rust\nfn x() {}\n```",
            r#"block(Some("rust"),"fn x() {}")"#,
        ),
        ("```js\n**no**\n```", r#"block(Some("js"),"**no**")"#),
        ("```one line```", r#"block(None,"one line")"#),
        ("a ```x``` b", r#""a " block(None,"x") " b""#),
        ("```\nunclosed", r#""```" BR "unclosed""#),
        ("``````", r#""``````""#),
        (
            "```py\nline1\nline2\n\n```",
            r#"block(Some("py"),"line1\nline2\n")"#,
        ),
        // quotes
        ("> q", r#"q["q"]"#),
        ("> a\n> b", r#"q["a" BR "b"]"#),
        ("> a\nb", r#"q["a"] "b""#),
        (">no space", r#"">no space""#),
        ("> **b**", r#"q[b["b"]]"#),
        (">>> a\nb\nc", r#"q["a" BR "b" BR "c"]"#),
        ("> > nested", r#"q["> nested"]"#),
        ("x\n> q", r#""x" q["q"]"#),
        // headers & subtext
        ("# H", r#"h1["H"]"#),
        ("## H", r#"h2["H"]"#),
        ("### H *i*", r#"h3["H " i["i"]]"#),
        ("#### H", "\"#### H\""),
        ("#H", "\"#H\""),
        ("# ", "\"# \""),
        ("-# small", r#"sub["small"]"#),
        ("a\n# H\nb", r#""a" h1["H"] "b""#),
        // lists
        ("- a\n- b", r#"ul[("a") ("b")]"#),
        ("* a\n* b", r#"ul[("a") ("b")]"#),
        ("1. a\n2. b", r#"ol1[("a") ("b")]"#),
        ("3. a", r#"ol3[("a")]"#),
        ("- a\n  - b\n- c", r#"ul[("a" ul[("b")]) ("c")]"#),
        ("- **b**", r#"ul[(b["b"])]"#),
        ("-a", r#""-a""#),
        ("- ", r#""- ""#),
        ("a\n- b", r#""a" ul[("b")]"#),
        // links
        ("[t](https://x.com)", r#"link<https://x.com>["t"]"#),
        (
            "[t](<https://x.com/a b>)",
            r#"link<https://x.com/a b>["t"]"#,
        ),
        ("[**t**](http://x.com)", r#"link<http://x.com>[b["t"]]"#),
        (
            r#"[t](https://x.com "title")"#,
            r#"link<https://x.com>["t"]"#,
        ),
        (
            "[t](https://x.com/a_(b))",
            r#"link<https://x.com/a_(b)>["t"]"#,
        ),
        ("[t](javascript:alert(1))", r#""[t](javascript:alert(1))""#),
        ("[t](x.com)", r#""[t](x.com)""#),
        (
            "[t] (https://x.com)",
            r#""[t] (" auto<https://x.com>["https://x.com"] ")""#,
        ),
        ("https://x.com", r#"auto<https://x.com>["https://x.com"]"#),
        (
            "see https://x.com/a?b=1.",
            r#""see " auto<https://x.com/a?b=1>["https://x.com/a?b=1"] ".""#,
        ),
        (
            "(https://x.com)",
            r#""(" auto<https://x.com>["https://x.com"] ")""#,
        ),
        (
            "<https://x.com>",
            r#"nolink<https://x.com>["https://x.com"]"#,
        ),
        (
            "||https://x.com||",
            r#"sp[auto<https://x.com>["https://x.com"]]"#,
        ),
        ("xhttps://x.com", r#""xhttps://x.com""#),
        ("https://", r#""https://""#),
        (
            "[https://a.com](https://b.com)",
            r#"link<https://b.com>["https://a.com"]"#,
        ),
        // mentions and tokens
        ("<@123>", "@123"),
        ("<@!123>", "@123"),
        ("<@&45>", "@&45"),
        ("<#67>", "#67"),
        ("<@abc>", r#""<@abc>""#),
        ("<@>", r#""<@>""#),
        (
            "<@99999999999999999999999>",
            r#""<@99999999999999999999999>""#,
        ),
        ("@everyone", "@everyone"),
        ("hi @here!", r#""hi " @here "!""#),
        ("`@everyone`", r#"code("@everyone")"#),
        ("<:smile:123>", "emoji(smile,123)"),
        ("<a:dance:456>", "emoji(a:dance,456)"),
        ("<:x:1>", r#""<" emo(x,❌) "1>""#),
        ("<:bad name:1>", r#""<:bad name:1>""#),
        ("<t:1618953630>", "ts(1618953630,None)"),
        ("<t:1618953630:R>", "ts(1618953630,Some(Relative))"),
        ("<t:1:F>", "ts(1,Some(LongDateTime))"),
        ("<t:1:Z>", r#""<t:1:Z>""#),
        ("<t:abc>", r#""<t:abc>""#),
        ("<t:1:RR>", r#""<t:1:RR>""#),
        ("</ping:99>", "cmd(ping,99)"),
        ("</role add:99>", "cmd(role add,99)"),
        ("</ping>", r#""</ping>""#),
        ("<123:customize>", "nav(customize,None)"),
        ("<123:browse>", "nav(browse,None)"),
        ("<123:guide>", "nav(guide,None)"),
        ("<123:linked-roles:5>", "nav(linked-roles,Some(5))"),
        ("<123:nope>", r#""<123:nope>""#),
        ("<cinema:abc123>", "cinema(abc123)"),
        ("<library:42>", "library(42)"),
        ("<cinema:>", r#""<cinema:>""#),
        ("<cinema:a b>", r#""<cinema:a b>""#),
        ("**<@1>**", "b[@1]"),
        ("hello <@1> and <@2>", r#""hello " @1 " and " @2"#),
        ("<unclosed", r#""<unclosed""#),
        ("a < b > c", r#""a < b > c""#),
    ];
    let mut bad = Vec::new();
    for (input, want) in cases {
        let got = p(input);
        if got != *want {
            bad.push(format!("input {input:?}\n  want {want}\n  got  {got}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} failures:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn mentions_extracted_and_deduped() {
    let ast = parse("<@1> <@!1> <@2> <@&9> <#5> <#5> @everyone @here <:em:7> `<@3>` ```<@4>```");
    let m = extract_mentions(&ast);
    assert_eq!(m.users, vec![1, 2]);
    assert_eq!(m.roles, vec![9]);
    assert_eq!(m.channels, vec![5]);
    assert_eq!(m.emoji, vec![7]);
    assert!(m.everyone && m.here);
}

#[test]
fn mentions_inside_containers() {
    let m = extract_mentions(&parse("> **<@1>**\n- ||<@&2>||\n# [<@3>](https://x.com)"));
    assert_eq!((m.users, m.roles), (vec![1, 3], vec![2]));
}

#[test]
fn escaped_mention_is_not_extracted() {
    let m = extract_mentions(&parse(r"\<@1> \@everyone"));
    assert!(m.users.is_empty());
    assert!(!m.everyone);
}

#[test]
fn limits() {
    let l = Limits::default();
    assert!(check_limits(&"a".repeat(2000), &l).is_ok());
    assert_eq!(
        check_limits(&"a".repeat(2001), &l),
        Err(LimitError::TooLong {
            max: 2000,
            actual: 2001
        })
    );
    // counted in chars, not bytes
    assert!(check_limits(&"é".repeat(2000), &l).is_ok());
    let many: String = (1..=5).map(|i| format!("<@{i}>")).collect();
    let tight = Limits {
        max_chars: 2000,
        max_mentions: 4,
    };
    assert_eq!(
        check_limits(&many, &tight),
        Err(LimitError::TooManyMentions { max: 4, actual: 5 })
    );
    let dup = "<@1>".repeat(50);
    assert!(check_limits(&dup, &tight).is_ok());
    assert!(
        LimitError::TooLong { max: 1, actual: 2 }
            .to_string()
            .contains("limit")
    );
}

#[test]
fn plain_text() {
    let t = |s: &str| to_plain_text(&parse(s));
    assert_eq!(t("**hi** _there_ ||secret||"), "hi there [spoiler]");
    assert_eq!(t("<@1> in <#2> <:wave:3>"), "@user in #channel :wave:");
    assert_eq!(t("[text](https://x.com)"), "text");
    assert_eq!(t("`code`"), "code");
    assert_eq!(t("# Title\nbody"), "Title\nbody");
    assert_eq!(t("- a\n- b"), "a\nb\n");
    struct R;
    impl Resolver for R {
        fn user(&self, id: u64) -> Option<String> {
            Some(format!("u{id}"))
        }
    }
    assert_eq!(to_plain_text_with(&parse("<@5> hi"), &R), "@u5 hi");
}

#[test]
fn json_shape() {
    let v: serde_json::Value = serde_json::from_str(&parse_to_json("**a** <@1>")).unwrap();
    assert_eq!(v[0]["type"], "bold");
    assert_eq!(v[0]["children"][0]["text"], "a");
    assert_eq!(v[2]["type"], "user_mention");
    assert_eq!(v[2]["id"], 1);
    let ts: serde_json::Value = serde_json::from_str(&parse_to_json("<t:5:R>")).unwrap();
    assert_eq!(ts[0]["style"], "relative");
}

#[test]
fn json_round_trips() {
    let ast =
        parse("# h\n> **q** [l](https://x.com)\n- a\n  - b\n```rs\nx\n``` <t:1:R> <:e:2> ||s||");
    let json = serde_json::to_string(&ast).unwrap();
    let back: Vec<Node> = serde_json::from_str(&json).unwrap();
    assert_eq!(ast, back);
}

#[test]
fn deep_nesting_does_not_overflow() {
    let s = format!("{}x{}", "**_".repeat(2000), "_**".repeat(2000));
    let _ = parse(&s);
    let s = "[".repeat(5000) + &"](https://x.com)".repeat(5000);
    let _ = parse(&s);
    let s = "> ".repeat(5000) + "x";
    let _ = parse(&s);
    let s: String = (0..3000)
        .map(|i| format!("{}- x\n", "  ".repeat(i % 50)))
        .collect();
    let _ = parse(&s);
}

#[test]
fn pathological_inputs_are_fast() {
    let start = std::time::Instant::now();
    for s in [
        "*a ".repeat(20_000),
        "`a ".repeat(20_000),
        "[a ".repeat(20_000),
        "||a ".repeat(20_000),
        "<@".repeat(20_000),
        "_a ".repeat(20_000),
        "```".repeat(20_000),
        "\\".repeat(40_000),
    ] {
        let _ = parse(&s);
    }
    assert!(
        start.elapsed().as_secs() < 10,
        "too slow: {:?}",
        start.elapsed()
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn never_panics_on_arbitrary_text(s in "\\PC{0,300}") {
        let ast = parse(&s);
        let _ = to_plain_text(&ast);
        let _ = extract_mentions(&ast);
        let _ = parse_to_json(&s);
    }

    #[test]
    fn never_panics_on_markdown_soup(
        parts in proptest::collection::vec(
            prop_oneof![
                Just("*"), Just("**"), Just("_"), Just("__"), Just("~~"), Just("||"),
                Just("`"), Just("```"), Just("\\"), Just("<"), Just(">"), Just("["),
                Just("]"), Just("("), Just(")"), Just("@"), Just("#"), Just("-# "),
                Just("> "), Just(">>> "), Just("- "), Just("1. "), Just("\n"), Just(" "),
                Just("<@12>"), Just("<t:5:R>"), Just("<:a:1>"), Just("https://x.y"),
                Just("é"), Just("😄"), Just("@everyone"), Just(":"), Just("a"),
            ],
            0..60,
        )
    ) {
        let s: String = parts.concat();
        let ast = parse(&s);
        let json = serde_json::to_string(&ast).unwrap();
        let back: Vec<Node> = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(ast, back);
    }

    #[test]
    fn plain_alnum_text_is_one_text_node(s in "[a-zA-Z0-9 ]{1,100}") {
        let ast = parse(&s);
        prop_assert_eq!(ast, vec![Node::Text { text: s.clone() }]);
    }
}

#[test]
fn shortcodes() {
    assert_eq!(p(":smile:"), "emo(smile,😄)");
    assert_eq!(p(":SMILE:"), "emo(smile,😄)");
    assert_eq!(p(":+1:"), "emo(+1,👍)");
    assert_eq!(p("a :fire: b"), r#""a " emo(fire,🔥) " b""#);
    assert_eq!(p(":fire::fire:"), "emo(fire,🔥) emo(fire,🔥)");
    assert_eq!(p(":nope:"), r#"":nope:""#);
    assert_eq!(p("12:30:45"), r#""12:30:45""#);
    assert_eq!(p("::"), r#""::""#);
    assert_eq!(p("`:fire:`"), r#"code(":fire:")"#);
    assert_eq!(p("<:fire:123>"), "emoji(fire,123)");
    assert_eq!(p(r"\:fire:"), r#"":fire:""#);
}

#[test]
fn shortcode_table_is_sane() {
    assert!(shortcode_count() >= 500);
    let mut seen = std::collections::HashSet::new();
    for (n, e) in super::emoji::table() {
        assert!(seen.insert(*n), "duplicate shortcode {n}");
        assert_eq!(n, &n.to_ascii_lowercase());
        assert!(n.bytes().all(crate::emoji::is_shortcode_char), "{n}");
        assert_eq!(count_emoji_only(e), Some(1), "{n} is not exactly one emoji");
    }
}

#[test]
fn jumbo() {
    let j = |s: &str| is_jumbo(&parse(s));
    assert!(j("😄"));
    assert!(j(":smile:"));
    assert!(j("😄 :fire: <:xx:1> <a:yy:2>"));
    assert!(j("😄😄
😄"));
    assert!(j("👍🏽"));
    assert!(j("👨‍👩‍👧‍👦"));
    assert!(j("🇺🇸"));
    assert!(j("1️⃣"));
    assert!(!j(""));
    assert!(!j("   "));
    assert!(!j("hi 😄"));
    assert!(!j("1"));
    assert!(!j("**😄**"));
    assert!(!j("> 😄"));
    assert!(!j("<@1>"));
    assert!(!j(":nope:"));
    assert!(j(&"😄".repeat(27)));
    assert!(!j(&"😄".repeat(28)));
    assert_eq!(emoji_only_count(&parse("😄 😄")), Some(2));
    assert_eq!(emoji_only_count(&parse("a")), None);
}

#[test]
fn emoji_plain_text() {
    assert_eq!(to_plain_text(&parse("hi :fire:")), "hi 🔥");
}
