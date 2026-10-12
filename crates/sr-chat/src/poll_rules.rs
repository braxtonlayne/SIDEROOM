//! Pure poll rules (LNG-MSG-010): validating a new poll and deciding whether it
//! still accepts votes. Votes themselves are stored through
//! [`crate::rich_repo::RichStore`].

use crate::error::{ChatError, ChatResult};
use crate::message::{Poll, PollAnswer, PollEmoji, PollMedia};
use serde::Deserialize;

pub const MIN_ANSWERS: usize = 2;
pub const MAX_ANSWERS: usize = 10;
pub const MAX_QUESTION_CHARS: usize = 300;
pub const MAX_ANSWER_CHARS: usize = 55;
pub const DEFAULT_DURATION_HOURS: u32 = 24;
/// One week, the longest poll this server allows.
pub const MAX_DURATION_HOURS: u32 = 168;
const HOUR_MS: i64 = 3_600_000;

/// The `poll` object of a create-message request (Discord shape).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PollCreate {
    #[serde(default)]
    pub question: PollMediaIn,
    #[serde(default)]
    pub answers: Vec<PollAnswerIn>,
    /// Hours; Discord calls it `duration`.
    #[serde(default, alias = "duration_hours")]
    pub duration: Option<u32>,
    #[serde(default)]
    pub allow_multiselect: bool,
    #[serde(default)]
    pub layout_type: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PollAnswerIn {
    #[serde(default)]
    pub poll_media: PollMediaIn,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PollMediaIn {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub emoji: Option<PollEmoji>,
}

fn bad(field: &str, code: &'static str, msg: impl Into<String>) -> ChatError {
    ChatError::invalid(field, code, msg)
}

fn check_emoji(field: &str, e: &PollEmoji) -> ChatResult<()> {
    match (&e.id, &e.name) {
        (Some(id), _) => {
            if id.parse::<u64>().ok().filter(|n| *n != 0).is_none() {
                return Err(bad(field, "NUMBER_TYPE_COERCE", "Value is not snowflake."));
            }
        }
        (None, Some(name)) => {
            if name.is_empty() || name.chars().count() > 32 || name.chars().any(char::is_control) {
                return Err(bad(field, "BASE_TYPE_BAD_LENGTH", "Invalid emoji."));
            }
        }
        (None, None) => return Err(bad(field, "BASE_TYPE_REQUIRED", "Emoji needs an id or a name.")),
    }
    Ok(())
}

/// Validate a new poll and fix its answer ids (1..n in order) and expiry.
pub fn plan_poll(c: PollCreate, now_ms: i64) -> ChatResult<Poll> {
    let question = c
        .question
        .text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            bad(
                "poll.question.text",
                "BASE_TYPE_REQUIRED",
                "This field is required",
            )
        })?;
    if question.chars().count() > MAX_QUESTION_CHARS {
        return Err(bad(
            "poll.question.text",
            "BASE_TYPE_MAX_LENGTH",
            format!("Must be {MAX_QUESTION_CHARS} or fewer in length."),
        ));
    }
    if c.answers.len() < MIN_ANSWERS || c.answers.len() > MAX_ANSWERS {
        return Err(bad(
            "poll.answers",
            "BASE_TYPE_BAD_LENGTH",
            format!("Must be between {MIN_ANSWERS} and {MAX_ANSWERS} in length."),
        ));
    }
    let hours = c.duration.unwrap_or(DEFAULT_DURATION_HOURS);
    if !(1..=MAX_DURATION_HOURS).contains(&hours) {
        return Err(bad(
            "poll.duration",
            "NUMBER_TYPE_MAX",
            format!("Must be between 1 and {MAX_DURATION_HOURS}."),
        ));
    }
    if c.layout_type.is_some_and(|l| l != 1) {
        return Err(bad("poll.layout_type", "BASE_TYPE_CHOICES", "Must be 1."));
    }
    let mut answers = Vec::with_capacity(c.answers.len());
    for (i, a) in c.answers.iter().enumerate() {
        let field = format!("poll.answers.{i}.poll_media");
        let text = a
            .poll_media
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        if text.is_none() && a.poll_media.emoji.is_none() {
            return Err(bad(&field, "BASE_TYPE_REQUIRED", "An answer needs text or an emoji."));
        }
        if text.is_some_and(|t| t.chars().count() > MAX_ANSWER_CHARS) {
            return Err(bad(
                &format!("{field}.text"),
                "BASE_TYPE_MAX_LENGTH",
                format!("Must be {MAX_ANSWER_CHARS} or fewer in length."),
            ));
        }
        if let Some(e) = &a.poll_media.emoji {
            check_emoji(&format!("{field}.emoji"), e)?;
        }
        answers.push(PollAnswer {
            answer_id: u32::try_from(i + 1).unwrap_or(u32::MAX),
            poll_media: PollMedia {
                text: text.map(str::to_owned),
                emoji: a.poll_media.emoji.clone(),
            },
        });
    }
    if let Some(e) = &c.question.emoji {
        check_emoji("poll.question.emoji", e)?;
    }
    Ok(Poll {
        question: PollMedia {
            text: Some(question.to_owned()),
            emoji: c.question.emoji,
        },
        answers,
        allow_multiselect: c.allow_multiselect,
        duration_hours: Some(hours),
        layout_type: Some(1),
        expiry_ms: Some(now_ms + i64::from(hours) * HOUR_MS),
        finalized: false,
    })
}

/// Whether votes are still accepted at `now_ms`.
pub fn is_open(p: &Poll, now_ms: i64) -> bool {
    !p.finalized && p.expiry_ms.is_none_or(|e| now_ms < e)
}

/// Whether the poll has run out and still needs finalising.
pub fn needs_finalizing(p: &Poll, now_ms: i64) -> bool {
    !p.finalized && p.expiry_ms.is_some_and(|e| now_ms >= e)
}

/// Whether `answer_id` is one of the poll's answers.
pub fn has_answer(p: &Poll, answer_id: u32) -> bool {
    p.answers.iter().any(|a| a.answer_id == answer_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create(v: serde_json::Value) -> PollCreate {
        serde_json::from_value(v).unwrap()
    }

    fn ok_poll() -> serde_json::Value {
        json!({
            "question": {"text": "  Best pet? "},
            "answers": [
                {"poll_media": {"text": "Cat", "emoji": {"name": "\u{1F431}"}}},
                {"poll_media": {"text": "Dog"}},
            ],
            "duration": 48,
            "allow_multiselect": true
        })
    }

    #[test]
    fn plans_a_valid_poll() {
        let p = plan_poll(create(ok_poll()), 1_000).unwrap();
        assert_eq!(p.question.text.as_deref(), Some("Best pet?"));
        assert_eq!(p.answers[0].answer_id, 1);
        assert_eq!(p.answers[1].answer_id, 2);
        assert_eq!(p.expiry_ms, Some(1_000 + 48 * HOUR_MS));
        assert!(p.allow_multiselect && !p.finalized);
        assert!(is_open(&p, 1_000));
        assert!(!is_open(&p, 1_000 + 48 * HOUR_MS));
        assert!(needs_finalizing(&p, 1_000 + 48 * HOUR_MS));
    }

    #[test]
    fn default_duration_is_a_day_and_hours_alias_works() {
        let mut v = ok_poll();
        v.as_object_mut().unwrap().remove("duration");
        let p = plan_poll(create(v), 0).unwrap();
        assert_eq!(p.duration_hours, Some(24));
        let mut v = ok_poll();
        v["duration_hours"] = json!(5);
        v.as_object_mut().unwrap().remove("duration");
        assert_eq!(plan_poll(create(v), 0).unwrap().duration_hours, Some(5));
    }

    #[test]
    fn rejects_bad_polls() {
        let cases = [
            ("no question", json!({"answers": ok_poll()["answers"]})),
            (
                "one answer",
                json!({"question": {"text": "q"}, "answers": [{"poll_media": {"text": "a"}}]}),
            ),
            ("duration 0", {
                let mut v = ok_poll();
                v["duration"] = json!(0);
                v
            }),
            ("duration over a week", {
                let mut v = ok_poll();
                v["duration"] = json!(169);
                v
            }),
            ("empty answer", {
                let mut v = ok_poll();
                v["answers"][1] = json!({"poll_media": {"text": "  "}});
                v
            }),
            ("long answer", {
                let mut v = ok_poll();
                v["answers"][1] = json!({"poll_media": {"text": "x".repeat(56)}});
                v
            }),
            ("long question", {
                let mut v = ok_poll();
                v["question"] = json!({"text": "q".repeat(301)});
                v
            }),
            ("bad emoji id", {
                let mut v = ok_poll();
                v["answers"][0]["poll_media"]["emoji"] = json!({"id": "abc"});
                v
            }),
            ("bad layout", {
                let mut v = ok_poll();
                v["layout_type"] = json!(7);
                v
            }),
        ];
        for (name, v) in cases {
            assert!(plan_poll(create(v), 0).is_err(), "{name} must be rejected");
        }
        let mut eleven = ok_poll();
        eleven["answers"] = json!((0..11)
            .map(|i| json!({"poll_media": {"text": format!("a{i}")}}))
            .collect::<Vec<_>>());
        assert!(plan_poll(create(eleven), 0).is_err());
    }
}
