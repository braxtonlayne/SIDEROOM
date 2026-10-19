//! The three default price sources of D-288 (M-02): the installer's `.env.prod.example` carries them as JSON and
//! each pointer resolves against a recorded sample answer of that API (shapes as the services return them).

use sr_payments::rates::parse_usd_price;

fn sources() -> Vec<serde_json::Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../deploy/compose/.env.prod.example"
    );
    let text = std::fs::read_to_string(path).expect("deploy/compose/.env.prod.example");
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("SR_PAYMENTS_RATE_SOURCES="))
        .expect("SR_PAYMENTS_RATE_SOURCES in the env example");
    let json = line.trim().trim_matches('\'');
    serde_json::from_str(json).expect("the default sources are valid JSON")
}

#[test]
fn the_three_default_sources_resolve_against_recorded_answers() {
    let samples = [
        (
            "coinbase",
            "https://api.coinbase.com/v2/prices/BTC-USD/spot",
            r#"{"data":{"amount":"67123.45","base":"BTC","currency":"USD"}}"#,
        ),
        (
            "kraken",
            "https://api.kraken.com/0/public/Ticker?pair=XBTUSD",
            r#"{"error":[],"result":{"XXBTZUSD":{"a":["67130.10000","1","1.000"],"b":["67130.00000","1","1.000"],"c":["67125.50000","0.00100000"],"v":["1","2"]}}}"#,
        ),
        (
            "coingecko",
            "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd",
            r#"{"bitcoin":{"usd":67120}}"#,
        ),
    ];
    let found = sources();
    assert_eq!(found.len(), 3);
    for (name, url, body) in samples {
        let s = found
            .iter()
            .find(|s| s["name"] == name)
            .unwrap_or_else(|| panic!("{name} is missing"));
        assert_eq!(s["url"], url, "{name} url");
        let cents = parse_usd_price(body, s["pointer"].as_str().unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!((6_700_000..6_720_000).contains(&cents), "{name}: {cents}");
    }
}
