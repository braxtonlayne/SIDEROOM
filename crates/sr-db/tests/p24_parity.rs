//! Postgres-backed tests for the phase-24 parity items. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p24_parity -- --ignored --test-threads=1`

use std::sync::Arc;

use sr_chat::social::SocialLimits;
use sr_chat::social::dm::{RequestReason, RequestState};
use sr_chat::social::repo::DmRepository;
use sr_chat::social::service::SocialService;
use sr_core::{ChannelId, Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::PgPool;
use sr_db::social_repo::PgSocialRepo;
use sr_db::testing::TestDb;

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

fn ids(worker: u8) -> Ids {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    SnowflakeGenerator::new(clock, worker, worker).expect("ids")
}

async fn account(pool: &PgPool, ids: &Ids) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    let name = format!("u{}", acct.get());
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(acct.get() as i64)
    .bind(person)
    .bind(&name[..name.len().min(32)])
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
    UserId::new(acct.get())
}

/// RDY-PAR-004 (migration 1310): the `spam` reason is accepted by the column, and the DM spam filter moves a
/// stranger's pending request to "Likely spam" when the first message trips the heuristics.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_dm_spam_filter_files_a_spammy_first_message_as_likely_spam() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(7);
    let stranger = account(&pool, &test_ids).await;
    let me = account(&pool, &test_ids).await;
    let repo = PgSocialRepo::new(pool.clone());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let svc = SocialService::new(
        PgSocialRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock, 7, 8).unwrap(),
        SocialLimits::default(),
    );

    let ch = ChannelId::new(test_ids.generate().get());
    repo.insert_dm(ch, stranger, me).await.unwrap();
    repo.set_request_state(me, ch, RequestState::Pending)
        .await
        .unwrap();

    // A plain hello stays pending; a link flood is filed as spam with the reason stored in Postgres.
    assert_eq!(
        svc.screen_dm_message(ch, stranger, "hello there")
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repo.get_request_state(me, ch).await.unwrap(),
        Some(RequestState::Pending)
    );
    let spam = "https://a.example.com https://b.example.org https://c.example.net";
    assert_eq!(
        svc.screen_dm_message(ch, stranger, spam).await.unwrap(),
        Some("several links")
    );
    assert_eq!(
        repo.get_request_state(me, ch).await.unwrap(),
        Some(RequestState::Spam)
    );
    assert_eq!(
        repo.get_request_reason(me, ch).await.unwrap(),
        Some(RequestReason::Spam)
    );

    // The recipient filing a request as spam by hand stores the same reason.
    let other = account(&pool, &test_ids).await;
    let ch2 = ChannelId::new(test_ids.generate().get());
    repo.insert_dm(ch2, other, me).await.unwrap();
    repo.set_request_state(me, ch2, RequestState::Pending)
        .await
        .unwrap();
    svc.respond_to_request(me, ch2, RequestState::Spam)
        .await
        .unwrap();
    assert_eq!(
        repo.get_request_reason(me, ch2).await.unwrap(),
        Some(RequestReason::Spam)
    );
    db.cleanup().await.unwrap();
}

/// PAR-FU-002 (U-03): the filter level "All" also reads a friend's message. The friend stays in the conversation
/// (no request state is filed) and the message is flagged `LIKELY_SPAM`; "Non-friends" and "Off" never read it.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_all_level_flags_a_friends_spammy_message_without_filing_a_request() {
    use sr_chat::social::safety::SpamFilter;
    use sr_chat::social::user_settings::PrivacyUpdate;

    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(9);
    let friend = account(&pool, &test_ids).await;
    let me = account(&pool, &test_ids).await;
    let repo = PgSocialRepo::new(pool.clone());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let svc = SocialService::new(
        PgSocialRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock, 9, 10).unwrap(),
        SocialLimits::default(),
    );
    svc.send_friend_request(friend, me).await.unwrap();
    svc.accept_friend_request(me, friend).await.unwrap();
    let ch = ChannelId::new(test_ids.generate().get());
    repo.insert_dm(ch, friend, me).await.unwrap();

    let spam = "https://a.example.com https://b.example.org https://c.example.net";
    let level = |l| PrivacyUpdate {
        dm_spam_filter: Some(l),
        ..Default::default()
    };
    for l in [SpamFilter::NonFriends, SpamFilter::Off] {
        svc.update_privacy(me, level(l)).await.unwrap();
        assert!(!svc.friend_dm_spam_flag(ch, friend, spam).await.unwrap());
    }
    svc.update_privacy(me, level(SpamFilter::All))
        .await
        .unwrap();
    assert!(svc.friend_dm_spam_flag(ch, friend, spam).await.unwrap());
    assert!(
        !svc.friend_dm_spam_flag(ch, friend, "see you at nine")
            .await
            .unwrap()
    );
    // The conversation is not a request, and screening it leaves it alone.
    assert_eq!(svc.screen_dm_message(ch, friend, spam).await.unwrap(), None);
    assert_ne!(
        repo.get_request_state(me, ch).await.unwrap(),
        Some(RequestState::Spam)
    );
    db.cleanup().await.unwrap();
}

/// PAR-FU-002 (P26-LNG-1): a content edit re-screens a friend's message. Editing a clean message into the
/// three-host text sets `LIKELY_SPAM` in Postgres without filing a request; editing it back clears the bit.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn editing_a_friends_message_into_spam_sets_the_flag_and_back_clears_it() {
    use sr_chat::domain::message_flags::LIKELY_SPAM;
    use sr_chat::message_hooks::MessageHooks;
    use sr_chat::message_rules::EditMessageInput;
    use sr_chat::repo::MessageRepository;
    use sr_chat::social::safety::SpamFilter;
    use sr_chat::social::user_settings::PrivacyUpdate;
    use sr_chat::{ChatLimits, ChatService, SendMessage};
    use sr_db::chat_repo::PgChatRepo;

    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(11);
    let friend = account(&pool, &test_ids).await;
    let me = account(&pool, &test_ids).await;
    let repo = PgSocialRepo::new(pool.clone());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let social = Arc::new(SocialService::new(
        PgSocialRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 11, 12).unwrap(),
        SocialLimits::default(),
    ));
    social.send_friend_request(friend, me).await.unwrap();
    social.accept_friend_request(me, friend).await.unwrap();
    social
        .update_privacy(
            me,
            PrivacyUpdate {
                dm_spam_filter: Some(SpamFilter::All),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let ch = ChannelId::new(test_ids.generate().get());
    repo.insert_dm(ch, friend, me).await.unwrap();

    let hooks: Arc<dyn MessageHooks> = social.clone();
    let chat = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock, 11, 13).unwrap(),
        ChatLimits::default(),
    )
    .with_hooks(hooks);
    let sent = chat
        .send_message(
            friend,
            ch,
            SendMessage {
                content: "see you at nine".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(sent.flags & LIKELY_SPAM, 0);

    let edit = |text: &str| EditMessageInput {
        content: Some(text.into()),
        flags: None,
    };
    chat.edit_message(
        friend,
        ch,
        sent.id,
        edit("https://a.example.com https://b.example.org https://c.example.net"),
    )
    .await
    .unwrap();
    let stored = PgChatRepo::new(pool.clone())
        .get_message(ch, sent.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.flags & LIKELY_SPAM, LIKELY_SPAM);
    assert_ne!(
        repo.get_request_state(me, ch).await.unwrap(),
        Some(RequestState::Spam)
    );

    chat.edit_message(friend, ch, sent.id, edit("sorry, wrong chat"))
        .await
        .unwrap();
    let stored = PgChatRepo::new(pool.clone())
        .get_message(ch, sent.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.flags & LIKELY_SPAM, 0);
    db.cleanup().await.unwrap();
}
