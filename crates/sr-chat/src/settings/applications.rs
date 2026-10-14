//! Membership applications (LNG-SSET-042): a questions form per guild, answers from
//! people who are not members yet, and a moderator review queue.

use serde::{Deserialize, Serialize};
use sr_core::{GuildId, UserId};

use super::error::{SettingsError, SettingsResult};

pub const MAX_QUESTIONS: usize = 5;
pub const MAX_LABEL: usize = 100;
pub const MAX_FORM_DESCRIPTION: usize = 300;
pub const MAX_SHORT_ANSWER: usize = 200;
pub const MAX_PARAGRAPH_ANSWER: usize = 1000;
pub const MAX_REJECTION_REASON: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestionKind {
    /// One line, up to 200 characters.
    Short,
    /// Several lines, up to 1000 characters.
    Paragraph,
}

impl QuestionKind {
    pub const fn max_chars(self) -> usize {
        match self {
            Self::Short => MAX_SHORT_ANSWER,
            Self::Paragraph => MAX_PARAGRAPH_ANSWER,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub id: u64,
    pub label: String,
    pub kind: QuestionKind,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ApplicationForm {
    pub guild_id: GuildId,
    /// "Apply to join" is on: non-members may submit.
    pub enabled: bool,
    pub description: Option<String>,
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub question_id: u64,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplicationStatus {
    Pending,
    Approved,
    Rejected,
}

impl ApplicationStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    pub id: u64,
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub answers: Vec<Answer>,
    pub status: ApplicationStatus,
    pub created_at_ms: i64,
    pub reviewed_by: Option<UserId>,
    pub reviewed_at_ms: Option<i64>,
    pub rejection_reason: Option<String>,
}

fn bad(field: &str, msg: &str) -> SettingsError {
    SettingsError::invalid(field, "APPLICATION_INVALID", msg)
}

/// Validate and normalise a form. `new_id` mints ids for questions that have none (0).
pub fn validate_form(
    mut f: ApplicationForm,
    mut new_id: impl FnMut() -> u64,
) -> SettingsResult<ApplicationForm> {
    f.description = f
        .description
        .take()
        .map(|d| d.trim().to_owned())
        .filter(|d| !d.is_empty());
    if f.description
        .as_deref()
        .is_some_and(|d| d.chars().count() > MAX_FORM_DESCRIPTION)
    {
        return Err(bad("description", "at most 300 characters"));
    }
    if f.questions.len() > MAX_QUESTIONS {
        return Err(bad("questions", "at most 5 questions"));
    }
    for q in &mut f.questions {
        q.label = q.label.trim().to_owned();
        if !(1..=MAX_LABEL).contains(&q.label.chars().count()) {
            return Err(bad("questions", "question labels are 1-100 characters"));
        }
        if q.id == 0 {
            q.id = new_id();
        }
    }
    let mut seen: Vec<u64> = f.questions.iter().map(|q| q.id).collect();
    seen.sort_unstable();
    if seen.windows(2).any(|w| w[0] == w[1]) {
        return Err(bad("questions", "question ids must be unique"));
    }
    Ok(f)
}

/// Check submitted answers against the form: known questions, required ones answered,
/// lengths within the limit of the question kind. Returns the answers in question order.
pub fn validate_answers(form: &ApplicationForm, given: &[Answer]) -> SettingsResult<Vec<Answer>> {
    for a in given {
        if !form.questions.iter().any(|q| q.id == a.question_id) {
            return Err(bad("answers", "unknown question"));
        }
    }
    let mut out = Vec::new();
    for q in &form.questions {
        let mut matching = given.iter().filter(|a| a.question_id == q.id);
        let value = matching.next().map(|a| a.value.trim().to_owned());
        if matching.next().is_some() {
            return Err(bad("answers", "a question was answered twice"));
        }
        match value.filter(|v| !v.is_empty()) {
            Some(v) => {
                if v.chars().count() > q.kind.max_chars() {
                    return Err(bad("answers", "an answer is too long"));
                }
                out.push(Answer {
                    question_id: q.id,
                    value: v,
                });
            }
            None if q.required => return Err(bad("answers", "a required question is unanswered")),
            None => {}
        }
    }
    Ok(out)
}

/// Trim and bound a rejection reason.
pub fn normalize_rejection_reason(reason: Option<&str>) -> SettingsResult<Option<String>> {
    let r = reason.map(str::trim).filter(|r| !r.is_empty());
    if r.is_some_and(|r| r.chars().count() > MAX_REJECTION_REASON) {
        return Err(bad("reason", "at most 512 characters"));
    }
    Ok(r.map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(id: u64, required: bool, kind: QuestionKind) -> Question {
        Question {
            id,
            label: format!("Question {id}"),
            kind,
            required,
        }
    }

    fn form() -> ApplicationForm {
        ApplicationForm {
            guild_id: GuildId::new(1),
            enabled: true,
            description: None,
            questions: vec![
                q(1, true, QuestionKind::Short),
                q(2, false, QuestionKind::Paragraph),
            ],
        }
    }

    fn ans(id: u64, v: &str) -> Answer {
        Answer {
            question_id: id,
            value: v.into(),
        }
    }

    #[test]
    fn form_limits_and_id_minting() {
        let mut next = 100;
        let mut f = form();
        f.questions[0].id = 0;
        let f = validate_form(f, || {
            next += 1;
            next
        })
        .unwrap();
        assert_eq!(f.questions[0].id, 101);
        let mut many = form();
        many.questions = (1..=6).map(|n| q(n, false, QuestionKind::Short)).collect();
        assert!(validate_form(many, || 1).is_err());
        let mut dup = form();
        dup.questions[1].id = 1;
        assert!(validate_form(dup, || 1).is_err());
        let mut blank = form();
        blank.questions[0].label = "  ".into();
        assert!(validate_form(blank, || 1).is_err());
    }

    #[test]
    fn answers_follow_the_form() {
        let f = form();
        let ok = validate_answers(&f, &[ans(2, " long text "), ans(1, "me")]).unwrap();
        assert_eq!(ok[0], ans(1, "me"));
        assert_eq!(ok[1], ans(2, "long text"));
        assert!(
            validate_answers(&f, &[ans(2, "x")]).is_err(),
            "required unanswered"
        );
        assert!(validate_answers(&f, &[ans(1, "  ")]).is_err());
        assert!(validate_answers(&f, &[ans(1, "a"), ans(9, "b")]).is_err());
        assert!(validate_answers(&f, &[ans(1, "a"), ans(1, "b")]).is_err());
        assert!(validate_answers(&f, &[ans(1, &"x".repeat(MAX_SHORT_ANSWER + 1))]).is_err());
        assert_eq!(validate_answers(&f, &[ans(1, "a")]).unwrap().len(), 1);
    }

    #[test]
    fn rejection_reason_is_bounded() {
        assert_eq!(
            normalize_rejection_reason(Some(" no ")).unwrap().as_deref(),
            Some("no")
        );
        assert_eq!(normalize_rejection_reason(Some("  ")).unwrap(), None);
        assert!(normalize_rejection_reason(Some(&"x".repeat(513))).is_err());
    }
}
