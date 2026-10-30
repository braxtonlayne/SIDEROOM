//! The card's message: an embed (what is on, when, how many seats, who is going) and one row of buttons.
//! Rendered from the card row and its claims, never from the previous message, so any node can rewrite it.

use super::{Card, CardAction, Claim, LISTED_CLAIMANTS, custom_id};
use crate::message::{Component, Embed, EmbedAuthor, EmbedField, EmbedFooter};

/// Card colour while it is open (the theme gold, like the watch party card) and after it ended.
pub const OPEN_COLOR: u32 = 0x00C9_A24D;
pub const DONE_COLOR: u32 = 0x0074_7F8D;

/// Discord button styles.
const PRIMARY: u8 = 1;
const SECONDARY: u8 = 2;
const SUCCESS: u8 = 3;

/// The name shown above the title.
pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "cinema" => "Cinema ticket",
        "live_tv" => "Live TV",
        _ => "Game night",
    }
}

/// `<@id>` for the first [`LISTED_CLAIMANTS`] claimants and "+N more".
pub fn going_list(claims: &[Claim]) -> String {
    if claims.is_empty() {
        return "No one yet. Be the first to claim a seat.".to_owned();
    }
    let mut out: Vec<String> = claims
        .iter()
        .take(LISTED_CLAIMANTS)
        .map(|c| format!("<@{}>", c.user_id))
        .collect();
    if claims.len() > LISTED_CLAIMANTS {
        out.push(format!("+{} more", claims.len() - LISTED_CLAIMANTS));
    }
    out.join(" ")
}

fn button(style: u8, label: &str, action: CardAction, card: u64, disabled: bool) -> Component {
    Component {
        kind: 2,
        style: Some(style),
        label: Some(label.to_owned()),
        custom_id: Some(custom_id(action, card)),
        disabled: disabled.then_some(true),
        ..Component::default()
    }
}

/// The embed and the buttons of a card.
pub fn render(card: &Card, claims: &[Claim]) -> (Embed, Vec<Component>) {
    let taken = claims.len();
    let seats = usize::try_from(card.seats).unwrap_or(0);
    let canceled = card.canceled_ms.is_some();
    let closed = card.closed_ms.is_some();
    let done = canceled || closed;
    let secs = card.starts_ms / 1000;

    let mut description = card.note.clone();
    if let Some(host) = card.created_by {
        if !description.is_empty() {
            description.push_str("\n\n");
        }
        description.push_str(&format!("Hosted by <@{host}>"));
    }

    let status = if canceled {
        "Canceled".to_owned()
    } else if closed {
        "Ended".to_owned()
    } else if card.started_ms.is_some() {
        format!("Started <t:{secs}:R>")
    } else {
        format!("<t:{secs}:R>")
    };
    let when = format!("<t:{secs}:F>\n{status}");
    let seats_line = if taken >= seats {
        format!("{taken} of {seats} taken (full)")
    } else {
        format!("{taken} of {seats} taken")
    };

    let title = if canceled {
        format!("Canceled: {}", card.title)
    } else {
        card.title.clone()
    };
    let embed = Embed {
        title: Some(title),
        kind: Some("rich".to_owned()),
        description: (!description.is_empty()).then_some(description),
        url: card.link_path.clone(),
        color: if done { DONE_COLOR } else { OPEN_COLOR },
        author: Some(EmbedAuthor {
            name: kind_label(&card.kind).to_owned(),
            url: None,
            icon_url: None,
        }),
        footer: Some(EmbedFooter {
            text: format!("Time picked in {}", card.timezone),
            icon_url: None,
        }),
        fields: vec![
            EmbedField {
                name: "When".to_owned(),
                value: when,
                inline: true,
            },
            EmbedField {
                name: "Seats".to_owned(),
                value: seats_line,
                inline: true,
            },
            EmbedField {
                name: "Going".to_owned(),
                value: going_list(claims),
                inline: false,
            },
        ],
        ..Embed::default()
    };

    if done {
        return (embed, Vec::new());
    }
    let mut row = vec![
        button(
            SUCCESS,
            if taken >= seats {
                "Full"
            } else {
                "Claim a seat"
            },
            CardAction::Claim,
            card.id,
            taken >= seats,
        ),
        button(
            SECONDARY,
            "Give up my seat",
            CardAction::Unclaim,
            card.id,
            false,
        ),
        button(
            SECONDARY,
            "Add to calendar",
            CardAction::Ics,
            card.id,
            false,
        ),
    ];
    // The watch-party hand-off: once the start time has passed, a cinema card with a title offers to start the
    // party. The button is for everyone; `START_WATCH_PARTY` is checked by the watch party API when it is used.
    if card.started_ms.is_some() && card.title_id.is_some() {
        row.push(button(
            PRIMARY,
            "Start watch party",
            CardAction::Party,
            card.id,
            false,
        ));
    }
    (
        embed,
        vec![Component {
            kind: 1,
            components: row,
            ..Component::default()
        }],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> Card {
        Card {
            id: 90,
            channel_id: 2,
            guild_id: Some(1),
            kind: "cinema".into(),
            title: "Spirited Away".into(),
            note: "Dubbed, sorry.".into(),
            starts_ms: 1_794_079_800_000,
            duration_min: 125,
            timezone: "Europe/Berlin".into(),
            seats: 2,
            title_id: Some(55),
            link_path: Some("/cinema/title/55".into()),
            created_by: Some(7),
            created_ms: 1,
            canceled_ms: None,
            started_ms: None,
            closed_ms: None,
        }
    }

    fn claim(user: u64) -> Claim {
        Claim {
            card_id: 90,
            user_id: user,
            claimed_ms: 1,
            remind: true,
            reminded_hour: false,
            reminded_ten: false,
        }
    }

    fn labels(cs: &[Component]) -> Vec<(String, bool)> {
        cs[0]
            .components
            .iter()
            .map(|c| (c.label.clone().unwrap(), c.disabled == Some(true)))
            .collect()
    }

    #[test]
    fn an_open_card_has_the_time_the_seats_and_three_buttons() {
        let (e, c) = render(&card(), &[claim(11)]);
        assert_eq!(e.title.as_deref(), Some("Spirited Away"));
        assert_eq!(e.author.unwrap().name, "Cinema ticket");
        assert_eq!(e.url.as_deref(), Some("/cinema/title/55"));
        assert!(e.description.unwrap().contains("Hosted by <@7>"));
        let f: Vec<_> = e
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f.value.as_str()))
            .collect();
        assert_eq!(f[0].0, "When");
        assert!(f[0].1.starts_with("<t:1794079800:F>"));
        assert_eq!(f[1], ("Seats", "1 of 2 taken"));
        assert_eq!(f[2], ("Going", "<@11>"));
        assert_eq!(
            labels(&c),
            [
                ("Claim a seat".to_owned(), false),
                ("Give up my seat".to_owned(), false),
                ("Add to calendar".to_owned(), false)
            ]
        );
        let ids: Vec<_> = c[0]
            .components
            .iter()
            .map(|b| b.custom_id.clone().unwrap())
            .collect();
        assert_eq!(ids, ["card:claim:90", "card:unclaim:90", "card:ics:90"]);
    }

    #[test]
    fn a_full_card_disables_claiming() {
        let (e, c) = render(&card(), &[claim(11), claim(12)]);
        assert!(e.fields[1].value.contains("(full)"));
        assert_eq!(labels(&c)[0], ("Full".to_owned(), true));
    }

    #[test]
    fn the_party_button_appears_at_the_start_for_a_cinema_card_with_a_title() {
        let mut c = card();
        c.started_ms = Some(c.starts_ms);
        let (e, comps) = render(&c, &[]);
        assert!(e.fields[0].value.contains("Started"));
        assert_eq!(comps[0].components.len(), 4);
        assert_eq!(
            comps[0].components[3].custom_id.as_deref(),
            Some("card:party:90")
        );
        c.title_id = None;
        assert_eq!(render(&c, &[]).1[0].components.len(), 3);
    }

    #[test]
    fn a_canceled_or_closed_card_has_no_buttons() {
        let mut c = card();
        c.canceled_ms = Some(5);
        let (e, comps) = render(&c, &[claim(1)]);
        assert!(comps.is_empty());
        assert_eq!(e.title.as_deref(), Some("Canceled: Spirited Away"));
        assert_eq!(e.color, DONE_COLOR);
        let mut c = card();
        c.closed_ms = Some(5);
        let (e, comps) = render(&c, &[]);
        assert!(comps.is_empty());
        assert!(e.fields[0].value.ends_with("Ended"));
    }

    #[test]
    fn a_long_guest_list_is_cut() {
        let claims: Vec<_> = (1..=25).map(claim).collect();
        let list = going_list(&claims);
        assert!(list.ends_with("+5 more"));
        assert_eq!(list.matches("<@").count(), LISTED_CLAIMANTS);
    }
}
