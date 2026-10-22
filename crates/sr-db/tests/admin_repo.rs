//! DB-backed tests of the admin repository. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test admin_repo -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use serde_json::json;
use sr_admin::{
    AccessKind, AccountQuery, AccountState, AdminError, AdminRepository, AuditEntry, BanCategory,
    chain,
};
use sr_auth::invite::InviteCode;
use sr_core::PlatformPermissions as P;
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, NewAccount},
    admin_repo::{PgAdminRepo, verify_chain},
    registration::{NewRegistration, Rejection, register_with_invite},
};

fn uid() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

fn repo(pool: &PgPool) -> PgAdminRepo {
    PgAdminRepo::new(pool.clone(), Arc::new(uid))
}

async fn account(pool: &PgPool, person: Option<i64>) -> i64 {
    let id = uid();
    let name = format!("t{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: person.unwrap_or_else(uid),
            username: &name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        uid(),
    )
    .await
    .unwrap();
    assert!(ok);
    id
}

async fn person_of(pool: &PgPool, account: i64) -> i64 {
    sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(account)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn entry(actor: i64, action: &str, target: Option<i64>) -> AuditEntry {
    AuditEntry {
        actor_id: actor,
        kind: AccessKind::AdminAction,
        action: action.to_owned(),
        target_kind: Some("account".into()),
        target_id: target,
        reason: "test".into(),
        ip: Some("127.0.0.1".into()),
        detail: "{}".into(),
    }
}

/// The one Staff Server (unique index): reuse it or create it.
async fn staff_guild(pool: &PgPool) -> i64 {
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM guilds WHERE is_staff_guild")
        .fetch_optional(pool)
        .await
        .unwrap()
    {
        return id;
    }
    let owner = account(pool, None).await;
    let id = uid();
    sqlx::query(
        "INSERT INTO guilds (id, name, owner_id, is_staff_guild) VALUES ($1, 'Staff', $2, true)",
    )
    .bind(id)
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO roles (id, guild_id, name) VALUES ($1, $1, '@everyone')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(id)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn role(pool: &PgPool, guild: i64, perms: P) -> i64 {
    let id = uid();
    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, platform_permissions, position) VALUES ($1,$2,$3,$4,1)",
    )
    .bind(id)
    .bind(guild)
    .bind(format!("r{id}"))
    .bind(perms.bits() as i64)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn staff_role_templates_match_the_rust_defaults() {
    let pool = pool().await;
    let rows: Vec<(String, i32, i32, i64)> = sqlx::query_as(
        "SELECT name, position, color, platform_permissions FROM staff_role_templates ORDER BY position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let want = sr_admin::rules::default_staff_roles();
    assert_eq!(rows.len(), want.len());
    for (row, w) in rows.iter().zip(&want) {
        assert_eq!(row.0, w.name);
        assert_eq!(row.1, w.position);
        assert_eq!(row.2, w.color);
        assert_eq!(row.3 as u64, w.permissions.bits(), "{}", w.name);
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn appointment_drives_standing_and_staff_server_membership() {
    let pool = pool().await;
    let r = repo(&pool);
    let guild = staff_guild(&pool).await;
    let mod_role = role(&pool, guild, P::STAFF_BASIC | P::MANAGE_ACCOUNTS).await;
    let actor = account(&pool, None).await;
    let a = account(&pool, None).await;

    assert!(r.standing(a).await.unwrap().is_none());
    let m = r
        .appoint(
            a,
            &[mod_role],
            actor,
            "needed",
            &entry(actor, "staff.appoint", Some(a)),
        )
        .await
        .unwrap();
    assert_eq!(m.roles.len(), 1);
    let s = r.standing(a).await.unwrap().unwrap();
    assert_eq!(s.permissions, P::STAFF_BASIC | P::MANAGE_ACCOUNTS);
    assert!(!s.is_owner);
    assert!(
        r.list_staff()
            .await
            .unwrap()
            .iter()
            .any(|x| x.account_id == a)
    );
    // Already staff.
    assert!(matches!(
        r.appoint(
            a,
            &[mod_role],
            actor,
            "again",
            &entry(actor, "staff.appoint", Some(a))
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
    // An action and its record are one transaction: both rows exist.
    let n: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM platform_audit_log WHERE actor_id = $1 AND target_id = $2 AND action = 'staff.appoint') \
              + (SELECT count(*) FROM staff_access_log WHERE actor_id = $1 AND target_id = $2 AND action = 'staff.appoint')",
    )
    .bind(actor)
    .bind(a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 2);

    // Roles in the Staff Server without an appointment have no platform effect.
    let b = account(&pool, None).await;
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild)
        .bind(b)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1,$2,$3)")
        .bind(guild)
        .bind(b)
        .bind(mod_role)
        .execute(&pool)
        .await
        .unwrap();
    assert!(r.standing(b).await.unwrap().is_none());

    // Removing the member from the Staff Server ends access at once, appointment row or not.
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
        .bind(guild)
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
    assert!(r.standing(a).await.unwrap().is_none());
    assert!(r.get_staff(a).await.unwrap().is_none());

    // Revoke an active appointment.
    let c = account(&pool, None).await;
    r.appoint(
        c,
        &[mod_role],
        actor,
        "x",
        &entry(actor, "staff.appoint", Some(c)),
    )
    .await
    .unwrap();
    r.revoke_staff(c, actor, "bye", &entry(actor, "staff.revoke", Some(c)))
        .await
        .unwrap();
    assert!(r.standing(c).await.unwrap().is_none());
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = $2)",
    )
    .bind(guild)
    .bind(c)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!member);
    assert!(matches!(
        r.revoke_staff(c, actor, "again", &entry(actor, "staff.revoke", Some(c)))
            .await,
        Err(AdminError::NotFound(_))
    ));
    // Re-appointing after a revocation works (the open-appointment index is partial).
    r.appoint(
        c,
        &[mod_role],
        actor,
        "back",
        &entry(actor, "staff.appoint", Some(c)),
    )
    .await
    .unwrap();
    // Roles of another guild are rejected.
    let d = account(&pool, None).await;
    assert!(matches!(
        r.appoint(
            d,
            &[uid()],
            actor,
            "x",
            &entry(actor, "staff.appoint", Some(d))
        )
        .await,
        Err(AdminError::NotFound(_))
    ));
    // The failed appointment left no row behind.
    let leaked: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM staff_appointments WHERE account_id = $1)")
            .bind(d)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!leaked);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn owner_is_all_powerful_and_ownership_transfers() {
    let pool = pool().await;
    let r = repo(&pool);
    let guild = staff_guild(&pool).await;
    let owner: i64 = sqlx::query_scalar("SELECT owner_id FROM guilds WHERE id = $1")
        .bind(guild)
        .fetch_one(&pool)
        .await
        .unwrap();
    let s = r.standing(owner).await.unwrap().unwrap();
    assert!(s.is_owner && s.permissions.contains(P::OWNER));
    assert!(matches!(
        r.revoke_staff(
            owner,
            owner,
            "x",
            &entry(owner, "staff.revoke", Some(owner))
        )
        .await,
        Err(AdminError::Forbidden(_))
    ));

    // Transfer to a fresh account and back, so the shared database ends as it began.
    let new = account(&pool, None).await;
    let op = account(&pool, None).await;
    r.transfer_owner(new, &entry(op, "staff.transfer_owner", Some(new)))
        .await
        .unwrap();
    assert!(r.standing(new).await.unwrap().unwrap().is_owner);
    let old = r.standing(owner).await.unwrap().map(|s| s.is_owner);
    assert_ne!(old, Some(true), "the former Owner is no longer the Owner");
    r.transfer_owner(owner, &entry(op, "staff.transfer_owner", Some(owner)))
        .await
        .unwrap();
    assert!(r.standing(owner).await.unwrap().unwrap().is_owner);
    assert!(matches!(
        r.transfer_owner(owner, &entry(op, "staff.transfer_owner", Some(owner)))
            .await,
        Err(AdminError::Conflict(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn suspend_revokes_sessions_and_reactivation_respects_the_cap() {
    let pool = pool().await;
    let r = repo(&pool);
    let actor = account(&pool, None).await;
    let a = account(&pool, None).await;
    for _ in 0..2 {
        sqlx::query(
            "INSERT INTO sessions (id, account_id, token_hash, expires_at) VALUES ($1,$2,$3,now() + interval '1 day')",
        )
        .bind(uid())
        .bind(a)
        .bind(uid().to_be_bytes().repeat(4))
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(
        r.account_detail(a).await.unwrap().unwrap().sessions_active,
        2
    );
    let s = r
        .set_account_state(
            a,
            AccountState::Deactivated,
            1_000_000,
            &entry(actor, "account.suspend", Some(a)),
        )
        .await
        .unwrap();
    assert_eq!(s.state, AccountState::Deactivated);
    assert_eq!(
        r.account_detail(a).await.unwrap().unwrap().sessions_active,
        0
    );
    // Suspending a suspended account conflicts; so does reactivating at the cap.
    assert!(matches!(
        r.set_account_state(
            a,
            AccountState::Deactivated,
            1,
            &entry(actor, "account.suspend", Some(a))
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
    assert!(matches!(
        r.set_account_state(
            a,
            AccountState::Active,
            0,
            &entry(actor, "account.unsuspend", Some(a))
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
    let s = r
        .set_account_state(
            a,
            AccountState::Active,
            1_000_000_000,
            &entry(actor, "account.unsuspend", Some(a)),
        )
        .await
        .unwrap();
    assert_eq!(s.state, AccountState::Active);

    // Search by username fragment and by exact id, state filter, cursor.
    let hit = r
        .search_accounts(&AccountQuery {
            q: Some(s.username.clone()),
            limit: 5,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(hit.len(), 1);
    let hit = r
        .search_accounts(&AccountQuery {
            q: Some(a.to_string()),
            limit: 5,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(hit[0].id, a);
    let none = r
        .search_accounts(&AccountQuery {
            q: Some(a.to_string()),
            state: Some(AccountState::Deleted),
            limit: 5,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(none.is_empty());
    // LIKE wildcards in the search term are literal. Other tests may leave accounts whose
    // names contain a literal `%`, so a bare `%` may match those (and only those); a
    // username followed by `%` must match nothing.
    let wild = r
        .search_accounts(&AccountQuery {
            q: Some(format!("{}%", s.username)),
            limit: 5,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(wild.is_empty());
    let bare = r
        .search_accounts(&AccountQuery {
            q: Some("%".into()),
            limit: 50,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(bare.iter().all(|h| {
        h.username.contains('%')
            || h.email.contains('%')
            || h.display_name.as_deref().is_some_and(|d| d.contains('%'))
    }));

    // 2FA reset runs the existing sr-db function inside the audited transaction.
    sqlx::query("INSERT INTO mfa_totp (id, account_id, secret_enc, confirmed_at) VALUES ($1,$2,'\\x00',now())")
        .bind(uid())
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
    assert!(r.account_detail(a).await.unwrap().unwrap().mfa_enabled);
    let rep = r
        .reset_2fa(a, &entry(actor, "account.reset_2fa", Some(a)))
        .await
        .unwrap();
    assert_eq!(rep.totp, 1);
    assert!(!r.account_detail(a).await.unwrap().unwrap().mfa_enabled);

    // Deletion request stub: one pending request per account.
    r.create_delete_request(
        a,
        actor,
        "asked",
        &entry(actor, "account.delete_request", Some(a)),
    )
    .await
    .unwrap();
    assert!(matches!(
        r.create_delete_request(
            a,
            actor,
            "again",
            &entry(actor, "account.delete_request", Some(a))
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
}

/// `inviter` invites `invitee` (a redeemed platform invite wired to `invited_by_invite_id`).
async fn link(pool: &PgPool, inviter: i64, invitee: i64) {
    let inv = uid();
    sqlx::query(
        "INSERT INTO platform_invites (id, code, issuer_account_id, state, redeemed_by_account_id, redeemed_at) \
         VALUES ($1, $2, $3, 'redeemed', $4, now())",
    )
    .bind(inv)
    .bind(InviteCode::generate().unwrap().canonical())
    .bind(inviter)
    .bind(invitee)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE accounts SET invited_by_invite_id = $2 WHERE id = $1")
        .bind(invitee)
        .bind(inv)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn serious_ban_blocks_registration_raises_inviter_suspicion_and_unban_restores() {
    let pool = pool().await;
    let r = repo(&pool);
    let actor = account(&pool, None).await;
    let inviter = account(&pool, None).await;
    let target = account(&pool, None).await;
    link(&pool, inviter, target).await;
    let target_person = person_of(&pool, target).await;
    let inviter_person = person_of(&pool, inviter).await;
    // A second account of the same Person.
    let twin = account(&pool, Some(target_person)).await;

    let out = r
        .ban_person(
            target_person,
            Some(target),
            BanCategory::Serious,
            "fraud",
            actor,
            2,
            3,
            &entry(actor, "person.ban", Some(target_person)),
        )
        .await
        .unwrap();
    assert_eq!(out.accounts_banned, 2);
    let s = out.inviter_suspicion.unwrap();
    assert_eq!(
        (s.person_id, s.inviter_account_id, s.level, s.frozen),
        (inviter_person, inviter, 1, false)
    );
    for id in [target, twin] {
        assert_eq!(
            r.account_detail(id).await.unwrap().unwrap().account.state,
            AccountState::PlatformBanned
        );
    }
    assert!(
        r.account_detail(target)
            .await
            .unwrap()
            .unwrap()
            .person_banned
    );
    assert!(matches!(
        r.ban_person(
            target_person,
            None,
            BanCategory::Standard,
            "again",
            actor,
            2,
            3,
            &entry(actor, "person.ban", Some(target_person))
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
    assert_eq!(
        r.list_suspects(2)
            .await
            .unwrap()
            .iter()
            .find(|p| p.person_id == inviter_person)
            .unwrap()
            .suspicion_level,
        1
    );

    // The banned Person cannot register again with a verified email of a banned account.
    let code = InviteCode::generate().unwrap();
    let issuer = account(&pool, None).await;
    sqlx::query("INSERT INTO platform_invites (id, code, issuer_account_id, emergency) VALUES ($1,$2,$3,true)")
        .bind(uid())
        .bind(code.canonical())
        .bind(issuer)
        .execute(&pool)
        .await
        .unwrap();
    // Person matching goes through the Discord identity (a verified email is already unique).
    let discord = format!("d{}", uid());
    sqlx::query("INSERT INTO external_identities (id, account_id, provider, provider_user_id) VALUES ($1,$2,'discord',$3)")
        .bind(uid())
        .bind(target)
        .bind(&discord)
        .execute(&pool)
        .await
        .unwrap();
    let res = register_with_invite(
        &pool,
        &NewRegistration {
            account_id: uid(),
            person_id: uid(),
            ledger_ids: [uid(), uid()],
            code: code.canonical().to_owned(),
            username: format!("n{}", uid()),
            display_name: None,
            email: format!("fresh{}@example.test", uid()),
            password_hash: "x".into(),
            discord_id: Some(discord),
            identity_id: uid(),
            account_cap: i64::MAX,
            initial_credits: 0,
        },
    )
    .await
    .unwrap();
    assert_eq!(res.unwrap_err(), Rejection::PersonBanned);
    // D-348: the refusal is written on the banned Person for staff; the inviter's level is untouched.
    let (n, kind, reason): (i64, String, String) = sqlx::query_as(
        "SELECT count(*), max(kind), max(reason) FROM suspicion_events \n         WHERE person_id = $1 AND kind = 'reregistration_refused'",
    )
    .bind(target_person)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((n, kind.as_str()), (1, "reregistration_refused"));
    assert!(reason.contains("matched by discord"), "{reason}");
    assert!(!reason.contains(&discord), "never the matching value");
    let inviter_level: i32 =
        sqlx::query_scalar("SELECT suspicion_level::int FROM persons WHERE id = $1")
            .bind(inviter_person)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(inviter_level, 1, "unchanged");

    // Clearing suspicion resets the level.
    r.clear_suspicion(
        inviter_person,
        actor,
        &entry(actor, "invites.clear_suspicion", Some(inviter_person)),
    )
    .await
    .unwrap();
    assert!(
        r.list_suspects(2)
            .await
            .unwrap()
            .iter()
            .all(|p| p.person_id != inviter_person)
    );

    // Unban: both accounts return while under the cap; a tiny cap leaves them deactivated.
    let list = r.list_bans(true, None, 500).await.unwrap();
    assert!(list.iter().any(|b| b.person_id == target_person));
    let u = r
        .unban_person(
            target_person,
            0,
            actor,
            "appeal",
            &entry(actor, "person.unban", Some(target_person)),
        )
        .await
        .unwrap();
    assert_eq!(
        (u.accounts_reactivated, u.accounts_left_deactivated),
        (0, 2)
    );
    assert_eq!(
        r.account_detail(target)
            .await
            .unwrap()
            .unwrap()
            .account
            .state,
        AccountState::Deactivated
    );
    assert!(matches!(
        r.unban_person(
            target_person,
            0,
            actor,
            "again",
            &entry(actor, "person.unban", Some(target_person))
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
    let pd = r.person_detail(target_person).await.unwrap().unwrap();
    assert!(pd.ban.unwrap().lifted_at_ms.is_some());
    assert_eq!(pd.accounts.len(), 2);

    // A standard ban does not propagate.
    let inviter2 = account(&pool, None).await;
    let target2 = account(&pool, None).await;
    link(&pool, inviter2, target2).await;
    let out = r
        .ban_person(
            person_of(&pool, target2).await,
            Some(target2),
            BanCategory::Standard,
            "rude",
            actor,
            2,
            3,
            &entry(actor, "person.ban", Some(target2)),
        )
        .await
        .unwrap();
    assert!(out.inviter_suspicion.is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn invite_graph_credits_and_invites() {
    let pool = pool().await;
    let r = repo(&pool);
    let actor = account(&pool, None).await;
    let root = account(&pool, None).await;
    let mid = account(&pool, None).await;
    let leaf = account(&pool, None).await;
    let leaf2 = account(&pool, None).await;
    link(&pool, root, mid).await;
    link(&pool, mid, leaf).await;
    link(&pool, mid, leaf2).await;

    let t = r.invite_tree(leaf).await.unwrap().unwrap();
    let path: Vec<i64> = t.ancestors.iter().map(|n| n.account_id).collect();
    assert_eq!(path, vec![root, mid], "root first, direct inviter last");
    assert_eq!(t.account.account_id, leaf);
    assert_eq!(
        (t.account.depth, t.ancestors[0].depth, t.ancestors[1].depth),
        (0, -2, -1)
    );
    let t = r.invite_tree(root).await.unwrap().unwrap();
    assert!(t.ancestors.is_empty());
    assert_eq!(
        (t.stats.issued, t.stats.redeemed, t.stats.descendants),
        (1, 1, 3)
    );
    assert!(r.invite_tree(uid()).await.unwrap().is_none());

    let d = r.invite_descendants(root, 1, 100).await.unwrap().unwrap();
    assert_eq!(d.nodes.len(), 1);
    let d = r.invite_descendants(root, 5, 100).await.unwrap().unwrap();
    assert_eq!(d.nodes.len(), 3);
    assert!(!d.truncated);
    let d = r.invite_descendants(root, 5, 2).await.unwrap().unwrap();
    assert!(d.truncated && d.nodes.len() == 2);

    // Credits: the ledger is append-only, the balance never goes negative.
    assert_eq!(
        r.adjust_credits(
            mid,
            4,
            actor,
            &entry(actor, "invites.adjust_credits", Some(mid))
        )
        .await
        .unwrap(),
        4
    );
    assert!(matches!(
        r.adjust_credits(
            mid,
            -5,
            actor,
            &entry(actor, "invites.adjust_credits", Some(mid))
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
    assert_eq!(
        r.adjust_credits(
            mid,
            -4,
            actor,
            &entry(actor, "invites.adjust_credits", Some(mid))
        )
        .await
        .unwrap(),
        0
    );
    let l = r.credit_ledger(mid, 10).await.unwrap().unwrap();
    assert_eq!(l.balance, 0);
    assert_eq!(
        l.entries
            .iter()
            .map(|e| e.reason.as_str())
            .collect::<Vec<_>>(),
        ["staff_revoke", "staff_grant"]
    );
    assert!(r.credit_ledger(uid(), 10).await.unwrap().is_none());

    // List and revoke invites.
    let open = uid();
    sqlx::query("INSERT INTO platform_invites (id, code, issuer_account_id) VALUES ($1,$2,$3)")
        .bind(open)
        .bind(InviteCode::generate().unwrap().canonical())
        .bind(root)
        .execute(&pool)
        .await
        .unwrap();
    let rows = r
        .list_invites(&sr_admin::InviteQuery {
            issuer_id: Some(root),
            state: Some("open".into()),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let rv = r
        .revoke_invite(open, &entry(actor, "invites.revoke", None))
        .await
        .unwrap();
    assert_eq!(rv.state, "revoked");
    assert!(matches!(
        r.revoke_invite(open, &entry(actor, "invites.revoke", None))
            .await,
        Err(AdminError::Conflict(_))
    ));
    assert!(matches!(
        r.revoke_invite(uid(), &entry(actor, "invites.revoke", None))
            .await,
        Err(AdminError::NotFound(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn settings_and_job_queue() {
    let pool = pool().await;
    let r = repo(&pool);
    let actor = account(&pool, None).await;
    let key = format!("test.admin.{}", uid());
    r.put_settings(
        &[(key.clone(), json!(42))],
        actor,
        &entry(actor, "settings.patch", None),
    )
    .await
    .unwrap();
    r.put_settings(
        &[(key.clone(), json!(43))],
        actor,
        &entry(actor, "settings.patch", None),
    )
    .await
    .unwrap();
    let s = r.settings().await.unwrap();
    assert_eq!(s[&key].value, json!(43));
    assert_eq!(s[&key].updated_by, Some(actor));
    sqlx::query("DELETE FROM platform_settings WHERE key = $1")
        .bind(&key)
        .execute(&pool)
        .await
        .unwrap();

    // Dead jobs: list, retry (resets to queued), delete.
    let kind = format!("test.k{}", uid());
    let (j1, j2) = (uid(), uid());
    for id in [j1, j2] {
        sqlx::query("INSERT INTO jobs (id, kind, state, attempts, max_attempts, finished_at, last_error) VALUES ($1,$2,'dead',5,5,now(),'boom')")
            .bind(id)
            .bind(&kind)
            .execute(&pool)
            .await
            .unwrap();
    }
    let dead = r.list_jobs(Some("dead"), None, 1000).await.unwrap();
    assert!(
        dead.iter()
            .any(|j| j.id == j1 && j.last_error.as_deref() == Some("boom"))
    );
    r.retry_job(j1, &entry(actor, "jobs.retry", Some(j1)))
        .await
        .unwrap();
    assert!(matches!(
        r.retry_job(j1, &entry(actor, "jobs.retry", Some(j1))).await,
        Err(AdminError::NotFound(_))
    ));
    r.delete_job(j2, &entry(actor, "jobs.delete", Some(j2)))
        .await
        .unwrap();
    assert!(matches!(
        r.delete_job(j2, &entry(actor, "jobs.delete", Some(j2)))
            .await,
        Err(AdminError::NotFound(_))
    ));
    sqlx::query("DELETE FROM jobs WHERE kind = $1")
        .bind(&kind)
        .execute(&pool)
        .await
        .unwrap();
    let c = r.counts().await.unwrap();
    assert!(c.accounts_active > 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn access_log_is_chained_serialised_append_only_and_tamper_evident() {
    let pool = pool().await;
    let r = repo(&pool);
    let actor = account(&pool, None).await;

    // Concurrent writers still produce one unforked, contiguous chain.
    let mut tasks = Vec::new();
    for i in 0..12 {
        let r = r.clone();
        tasks.push(tokio::spawn(async move {
            r.append_log(&entry(actor, &format!("test.concurrent.{i}"), Some(i)))
                .await
                .unwrap()
        }));
    }
    let mut seqs: Vec<i64> = Vec::new();
    for t in tasks {
        seqs.push(t.await.unwrap().seq);
    }
    seqs.sort_unstable();
    seqs.dedup();
    assert_eq!(seqs.len(), 12, "every writer got its own sequence number");
    assert_eq!(seqs[11] - seqs[0], 11, "and they are contiguous");

    let mut conn = pool.acquire().await.unwrap();
    let report = verify_chain(&mut conn).await.unwrap();
    assert!(report.ok, "{report:?}");
    assert!(report.rows_checked >= 12);
    drop(conn);

    // Filters.
    let f = r
        .list_log(&sr_admin::AccessLogFilter {
            actor_id: Some(actor),
            action: Some("test.concurrent.3".into()),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].ip.as_deref(), Some("127.0.0.1"));
    // The stored hash is the documented function of the previous hash and the row.
    let row = &f[0];
    let again = chain::seal(
        &chain::from_hex(&row.prev_hash).unwrap(),
        row.seq,
        row.at_us,
        &entry(actor, "test.concurrent.3", Some(3)),
    );
    assert_eq!(again.hash, row.hash);

    // UPDATE, DELETE and TRUNCATE are refused by triggers.
    let seq = seqs[0];
    let e = sqlx::query("UPDATE staff_access_log SET reason = 'x' WHERE seq = $1")
        .bind(seq)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("append-only"), "{e}");
    let e = sqlx::query("DELETE FROM staff_access_log WHERE seq = $1")
        .bind(seq)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("append-only"), "{e}");
    let e = sqlx::query("TRUNCATE staff_access_log")
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("append-only"), "{e}");
    // A private read without a reason is refused by the table itself.
    let e = sqlx::query(
        "INSERT INTO staff_access_log (seq, at_us, actor_id, kind, action, prev_hash, hash) \
         VALUES (-1, 0, $1, 'private_read', 'x', $2, $3)",
    )
    .bind(actor)
    .bind([0u8; 32].as_slice())
    .bind([1u8; 32].as_slice())
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(e.to_string().contains("check"), "{e}");

    // A privileged operator who bypasses the trigger and edits a row is caught. This runs in a
    // transaction that is rolled back, so the shared chain stays intact.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("ALTER TABLE staff_access_log DISABLE TRIGGER staff_access_log_append_only")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE staff_access_log SET reason = 'edited by an attacker' WHERE seq = $1")
        .bind(seq)
        .execute(&mut *tx)
        .await
        .unwrap();
    let report = verify_chain(&mut tx).await.unwrap();
    assert!(!report.ok);
    assert_eq!(report.first_bad_seq, Some(seq));
    assert!(report.problem.unwrap().contains("hash"));
    tx.rollback().await.unwrap();

    // Deleting a row in the middle shows up as a gap.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("ALTER TABLE staff_access_log DISABLE TRIGGER staff_access_log_append_only")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("DELETE FROM staff_access_log WHERE seq = $1")
        .bind(seqs[5])
        .execute(&mut *tx)
        .await
        .unwrap();
    let report = verify_chain(&mut tx).await.unwrap();
    assert!(!report.ok);
    assert_eq!(report.first_bad_seq, Some(seqs[6]));
    tx.rollback().await.unwrap();

    // Rolled back: the real chain is intact and the repository agrees.
    assert!(r.verify_log().await.unwrap().ok);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn frozen_issuers_cannot_have_invites_redeemed() {
    let pool = pool().await;
    let issuer = account(&pool, None).await;
    sqlx::query("INSERT INTO invite_credits_ledger (id, account_id, delta, reason) VALUES ($1,$2,5,'initial_grant')")
        .bind(uid())
        .bind(issuer)
        .execute(&pool)
        .await
        .unwrap();
    let person = person_of(&pool, issuer).await;
    let invite = |emergency: bool| {
        let pool = pool.clone();
        async move {
            let code = InviteCode::generate().unwrap();
            sqlx::query("INSERT INTO platform_invites (id, code, issuer_account_id, emergency) VALUES ($1,$2,$3,$4)")
                .bind(uid())
                .bind(code.canonical())
                .bind(issuer)
                .bind(emergency)
                .execute(&pool)
                .await
                .unwrap();
            code
        }
    };
    let register = |code: InviteCode| {
        let pool = pool.clone();
        async move {
            register_with_invite(
                &pool,
                &NewRegistration {
                    account_id: uid(),
                    person_id: uid(),
                    ledger_ids: [uid(), uid()],
                    code: code.canonical().to_owned(),
                    username: format!("n{}", uid()),
                    display_name: None,
                    email: format!("fresh{}@example.test", uid()),
                    password_hash: "x".into(),
                    discord_id: None,
                    identity_id: uid(),
                    account_cap: i64::MAX,
                    initial_credits: 0,
                },
            )
            .await
            .unwrap()
        }
    };
    // Level 1 is under the default threshold of 2: the invite works.
    sqlx::query("UPDATE persons SET suspicion_level = 1 WHERE id = $1")
        .bind(person)
        .execute(&pool)
        .await
        .unwrap();
    assert!(register(invite(false).await).await.is_ok());
    // At the threshold the issuer is frozen: a normal invite is refused, an operator one is not.
    sqlx::query("UPDATE persons SET suspicion_level = 2 WHERE id = $1")
        .bind(person)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        register(invite(false).await).await.unwrap_err(),
        Rejection::InviteUnavailable
    );
    assert!(register(invite(true).await).await.is_ok());
    // Clearing suspicion unfreezes.
    sqlx::query("UPDATE persons SET suspicion_level = 0 WHERE id = $1")
        .bind(person)
        .execute(&pool)
        .await
        .unwrap();
    assert!(register(invite(false).await).await.is_ok());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn appointments_record_how_they_ended() {
    use sr_chat::admin_repo::GuildAdminRepository;
    use sr_core::{GuildId, UserId};
    let pool = pool().await;
    let r = repo(&pool);
    let guild = staff_guild(&pool).await;
    let role_id = role(&pool, guild, P::STAFF_BASIC).await;
    let actor = account(&pool, None).await;
    let (left, kicked, revoked) = (
        account(&pool, None).await,
        account(&pool, None).await,
        account(&pool, None).await,
    );
    for who in [left, kicked, revoked] {
        r.appoint(
            who,
            &[role_id],
            actor,
            "x",
            &entry(actor, "staff.appoint", Some(who)),
        )
        .await
        .unwrap();
    }
    // The member leaves, is kicked by a moderator, or staff revoke the appointment.
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
        .bind(guild)
        .bind(left)
        .execute(&pool)
        .await
        .unwrap();
    let chat = sr_db::chat_repo::PgChatRepo::new(pool.clone());
    chat.remove_member_kicked(GuildId::new(guild as u64), UserId::new(kicked as u64))
        .await
        .unwrap();
    r.revoke_staff(
        revoked,
        actor,
        "bye",
        &entry(actor, "staff.revoke", Some(revoked)),
    )
    .await
    .unwrap();
    let former = r.former_staff(50).await.unwrap();
    let why = |id: i64| {
        former
            .iter()
            .find(|f| f.account_id == id)
            .map(|f| f.revoked_reason.as_str())
    };
    assert_eq!(why(left), Some("left"));
    assert_eq!(why(kicked), Some("kicked"));
    assert_eq!(why(revoked), Some("revoked"));
    // An open appointment is not listed.
    let current = account(&pool, None).await;
    r.appoint(
        current,
        &[role_id],
        actor,
        "x",
        &entry(actor, "staff.appoint", Some(current)),
    )
    .await
    .unwrap();
    assert!(
        r.former_staff(50)
            .await
            .unwrap()
            .iter()
            .all(|f| f.account_id != current)
    );
}
