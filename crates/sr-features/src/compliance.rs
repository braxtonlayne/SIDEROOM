//! The compliance settings (`platform.compliance`, M-06, D-293 to D-295): the minimum registration
//! age, the strike-review threshold and window, the data-request deadline and the operator's
//! country and contact. A platform-scope module edited under Admin > Features (with a reason,
//! D-254). `minimum_age`, `operator_country` and `operator_contact` are shown to visitors through
//! `GET /sr/compliance/public`; nothing else leaves the server. Not legal advice: the project
//! defaults are named in the decisions log together with what a lawyer should check.

use serde_json::{Map, Value, json};

use crate::feature::{Feature, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};

/// Stable id of the compliance module.
pub const COMPLIANCE_ID: &str = "platform.compliance";

/// Lowest and highest minimum registration age an operator may set (GDPR article 8: 13 to 16;
/// 18 for an adults-only instance).
pub const MIN_AGE_RANGE: (i64, i64) = (13, 18);

/// Range of `compliance.invoice_retention_years` (D-325).
pub const INVOICE_YEARS_RANGE: (i64, i64) = (3, 15);

#[derive(Default)]
pub struct ComplianceFeature;

impl Feature for ComplianceFeature {
    fn id(&self) -> &'static str {
        COMPLIANCE_ID
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Compliance".into(),
            description: "Minimum registration age, copyright strike review, data-request deadline and the operator country and contact.".into(),
            section: "Moderation".into(),
            phase: 17,
            default_enabled: true,
        }
    }
    fn settings_schema(&self) -> SettingsSchema {
        let int = |min: i64, max: i64| FieldKind::Int {
            min: Some(min),
            max: Some(max),
        };
        SettingsSchema::new(vec![
            Field::new(
                "accounts",
                "Accounts",
                FieldKind::Group {
                    fields: vec![
                        Field::new(
                            "minimum_age",
                            "Minimum age to register",
                            int(MIN_AGE_RANGE.0, MIN_AGE_RANGE.1),
                        )
                        .default_value(json!(13))
                        .help("Self-declared at registration and shown on the entrance pages. Raise it where the digital-consent age is higher (for example 16)."),
                    ],
                },
            ),
            Field::new(
                "moderation",
                "Copyright strikes",
                FieldKind::Group {
                    fields: vec![
                        Field::new(
                            "strike_review_threshold",
                            "Upheld takedowns that open a review",
                            int(1, 10),
                        )
                        .default_value(json!(3))
                        .help("A review case is opened for the person; nobody is suspended automatically."),
                        Field::new("strike_window_months", "Window in months", int(1, 60))
                            .default_value(json!(12)),
                    ],
                },
            ),
            Field::new(
                "compliance",
                "Data requests and operator",
                FieldKind::Group {
                    fields: vec![
                        Field::new(
                            "data_request_deadline_days",
                            "Data request deadline (days)",
                            int(7, 90),
                        )
                        .default_value(json!(30))
                        .help("The due date staff see on a data-subject request when it arrives."),
                        Field::new(
                            "invoice_retention_years",
                            "Invoice retention (years)",
                            int(INVOICE_YEARS_RANGE.0, INVOICE_YEARS_RANGE.1),
                        )
                        .default_value(json!(10))
                        .help("Years invoices stay linked to the member after the year of payment; set it to what accounting law requires where you operate."),
                        Field::new(
                            "operator_country",
                            "Operator country",
                            FieldKind::String { max_len: Some(2) },
                        )
                        .help("ISO 3166-1 alpha-2, for example DE. Shown on the privacy page when set."),
                        Field::new(
                            "operator_contact",
                            "Operator contact email",
                            FieldKind::String { max_len: Some(254) },
                        )
                        .help("Shown on the privacy page and on /help as the address for copyright notices when set."),
                    ],
                },
            ),
        ])
    }
    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform]
    }
}

/// The compliance settings as the server uses them. Values out of range or malformed fall back
/// to the defaults, so a hand-edited row never breaks registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplianceSettings {
    pub minimum_age: u8,
    pub strike_review_threshold: u8,
    pub strike_window_months: u8,
    pub data_request_deadline_days: u16,
    /// Years invoices stay linked to the member after the year of payment (N-07, D-325).
    pub invoice_retention_years: u8,
    /// Upper-case ISO 3166-1 alpha-2, when valid.
    pub operator_country: Option<String>,
    pub operator_contact: Option<String>,
}

impl Default for ComplianceSettings {
    fn default() -> Self {
        Self {
            minimum_age: 13,
            strike_review_threshold: 3,
            strike_window_months: 12,
            data_request_deadline_days: 30,
            invoice_retention_years: 10,
            operator_country: None,
            operator_contact: None,
        }
    }
}

fn number(g: Option<&Map<String, Value>>, key: &str, range: (i64, i64), default: i64) -> i64 {
    g.and_then(|g| g.get(key))
        .and_then(Value::as_i64)
        .filter(|n| (range.0..=range.1).contains(n))
        .unwrap_or(default)
}

fn text(g: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    g.and_then(|g| g.get(key))
        .and_then(Value::as_str)
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// A plain `local@domain.tld` shape check (no delivery is attempted).
fn looks_like_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !s.chars().any(char::is_whitespace)
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains('@')
}

impl ComplianceSettings {
    pub fn from_values(values: &Map<String, Value>) -> Self {
        let group = |k: &str| values.get(k).and_then(Value::as_object);
        let (accounts, moderation, compliance) =
            (group("accounts"), group("moderation"), group("compliance"));
        let d = Self::default();
        let small = |g, key, range, default: u8| {
            u8::try_from(number(g, key, range, i64::from(default))).unwrap_or(default)
        };
        Self {
            minimum_age: small(accounts, "minimum_age", MIN_AGE_RANGE, d.minimum_age),
            strike_review_threshold: small(
                moderation,
                "strike_review_threshold",
                (1, 10),
                d.strike_review_threshold,
            ),
            strike_window_months: small(
                moderation,
                "strike_window_months",
                (1, 60),
                d.strike_window_months,
            ),
            data_request_deadline_days: u16::try_from(number(
                compliance,
                "data_request_deadline_days",
                (7, 90),
                i64::from(d.data_request_deadline_days),
            ))
            .unwrap_or(d.data_request_deadline_days),
            invoice_retention_years: small(
                compliance,
                "invoice_retention_years",
                INVOICE_YEARS_RANGE,
                d.invoice_retention_years,
            ),
            operator_country: text(compliance, "operator_country")
                .map(|c| c.to_ascii_uppercase())
                .filter(|c| c.len() == 2 && c.chars().all(|ch| ch.is_ascii_uppercase())),
            operator_contact: text(compliance, "operator_contact").filter(|c| looks_like_email(c)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};
    use std::sync::Arc;

    fn service() -> FeatureService {
        let mut r = FeatureRegistry::new();
        r.register(ComplianceFeature).unwrap();
        FeatureService::new(Arc::new(r), Arc::new(InMemorySettingsStore::new()), None)
    }

    async fn effective(svc: &FeatureService) -> ComplianceSettings {
        ComplianceSettings::from_values(
            &svc.effective_settings(COMPLIANCE_ID, &ScopeRef::Platform)
                .await
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn defaults_apply_until_saved_and_a_saved_row_round_trips() {
        let svc = service();
        assert_eq!(effective(&svc).await, ComplianceSettings::default());
        let v = json!({
            "accounts": { "minimum_age": 16 },
            "moderation": { "strike_review_threshold": 5, "strike_window_months": 6 },
            "compliance": {
                "data_request_deadline_days": 45,
                "invoice_retention_years": 7,
                "operator_country": "de",
                "operator_contact": " legal@example.org "
            }
        });
        svc.set_settings(
            COMPLIANCE_ID,
            &ScopeRef::Platform,
            v.as_object().unwrap().clone(),
        )
        .await
        .unwrap();
        let s = effective(&svc).await;
        assert_eq!(s.minimum_age, 16);
        assert_eq!((s.strike_review_threshold, s.strike_window_months), (5, 6));
        assert_eq!(s.data_request_deadline_days, 45);
        assert_eq!(s.invoice_retention_years, 7);
        assert_eq!(ComplianceSettings::default().invoice_retention_years, 10);
        let wild = json!({"compliance": {"invoice_retention_years": 99}});
        assert_eq!(
            ComplianceSettings::from_values(wild.as_object().unwrap()).invoice_retention_years,
            10,
            "a hand-edited out-of-range value falls back to the default"
        );
        assert_eq!(s.operator_country.as_deref(), Some("DE"));
        assert_eq!(s.operator_contact.as_deref(), Some("legal@example.org"));
    }

    #[tokio::test]
    async fn out_of_range_values_are_refused_on_save() {
        let svc = service();
        for bad in [
            json!({"accounts": {"minimum_age": 12}}),
            json!({"accounts": {"minimum_age": 19}}),
            json!({"moderation": {"strike_review_threshold": 0}}),
            json!({"compliance": {"data_request_deadline_days": 91}}),
            json!({"compliance": {"invoice_retention_years": 2}}),
            json!({"compliance": {"invoice_retention_years": 16}}),
            json!({"compliance": {"operator_country": "DEU"}}),
        ] {
            assert!(
                svc.set_settings(
                    COMPLIANCE_ID,
                    &ScopeRef::Platform,
                    bad.as_object().unwrap().clone()
                )
                .await
                .is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn malformed_stored_values_fall_back() {
        let v = json!({
            "accounts": {"minimum_age": 99},
            "compliance": {"operator_country": "1x", "operator_contact": "not an email"}
        });
        let s = ComplianceSettings::from_values(v.as_object().unwrap());
        assert_eq!(s, ComplianceSettings::default());
    }
}
