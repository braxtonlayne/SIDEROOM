//! Who may add and take away packs in a server (D-282), over the in-memory guild with a Postgres-backed
//! packages service. Run with `SR_TEST_DATABASE_URL`, `--include-ignored`, `--test-threads=1`.

use super::*;
use sr_db::{
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_packages::{
    Checks,
    service::{Decision, PackageService},
    zipfiles::build_zip,
};

const EXPR: u64 = 5; // MANAGE_GUILD_EXPRESSIONS alone
const MGR: u64 = 6; // MANAGE_GUILD alone

static SEQ: AtomicU64 = AtomicU64::new(1_000_000);

fn uid() -> i64 {
    i64::try_from(SEQ.fetch_add(1, Ordering::SeqCst)).unwrap()
}

async fn account(pool: &sqlx::PgPool, id: i64) {
    let name = format!("t{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
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
}

fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(32, 32, image::Rgba([200, 30, 30, 255]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn emoji_zip() -> Vec<u8> {
    let m = json!({"kind": "emoji_pack", "id": "faces", "name": "Faces", "version": "1.0.0", "author": "me",
                   "items": [{"name": "wave", "file": "emoji/wave.png"}]});
    build_zip(&[
        ("pack.json", m.to_string().as_bytes()),
        ("emoji/wave.png", &png()),
    ])
}

fn theme_zip() -> Vec<u8> {
    let m = json!({"id": "ocean", "name": "Ocean", "version": "1.0.0", "author": "me",
                   "targets": ["lounge"], "scheme": "dark"});
    let t = json!({"--ui-accent": "#5865f2"});
    build_zip(&[
        ("theme.json", m.to_string().as_bytes()),
        ("tokens.json", t.to_string().as_bytes()),
    ])
}

fn recipe_zip() -> Vec<u8> {
    let m = json!({"id": "starter", "name": "Starter", "version": "1.0.0", "author": "me",
                   "features": [{"id": "lounge.starboard", "enabled": true}]});
    build_zip(&[("recipe.json", m.to_string().as_bytes())])
}

async fn approved(svc: &PackageService, author: i64, staff: i64, zip: Vec<u8>) -> i64 {
    let v = svc.submit(author, zip, &Checks::lenient()).await.unwrap();
    let (pid, vid): (i64, i64) = (v.package_id.parse().unwrap(), v.id.parse().unwrap());
    svc.decide(vid, staff, Decision::Approve, None)
        .await
        .unwrap();
    pid
}

fn add_member(env: &Env, user: u64, perms: Permissions) {
    let role = RoleId::new(user * 1000);
    let mut d = env.backend.data();
    d.roles.push(Role {
        id: role,
        guild_id: env.guild,
        name: format!("r{user}"),
        color: 0,
        secondary_color: None,
        tertiary_color: None,
        hoist: false,
        position: 1,
        permissions: perms,
        mentionable: false,
        managed: false,
        unicode_emoji: None,
        icon_key: None,
    });
    d.members.push(Member {
        guild_id: env.guild,
        user_id: UserId::new(user),
        nick: None,
        role_ids: vec![role],
        joined_at_ms: T0,
        communication_disabled_until_ms: None,
        pending: false,
        avatar_key: None,
        banner_key: None,
        bio: None,
        extras: Default::default(),
    });
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn packs_need_manage_guild_expressions_alone_and_themes_and_recipes_keep_manage_guild() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let storage: Arc<dyn sr_storage::ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let svc = PackageService::new(pool.clone(), Arc::new(uid)).with_storage(storage);
    let (author, staff) = (uid(), uid());
    account(&pool, author).await;
    account(&pool, staff).await;
    account(&pool, i64::try_from(OWNER).unwrap()).await;
    let pack = approved(&svc, author, staff, emoji_zip()).await;
    let theme = approved(&svc, author, staff, theme_zip()).await;
    let recipe = approved(&svc, author, staff, recipe_zip()).await;

    let env = env_with(Opts {
        packages: Some(Arc::new(crate::routes::packages::PackagesDeps {
            service: svc,
        })),
        features: true,
        ..Opts::default()
    })
    .await;
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Club', $2)")
        .bind(i64::try_from(env.guild.get()).unwrap())
        .bind(i64::try_from(OWNER).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    add_member(&env, EXPR, Permissions::MANAGE_GUILD_EXPRESSIONS);
    add_member(&env, MGR, Permissions::MANAGE_GUILD);

    let import = |p: i64| env.g(&format!("/packages/{p}/import"));
    let one = |p: i64| env.g(&format!("/packages/{p}"));
    let user = |u: u64, m: Method, path: String, b: Option<Value>| {
        let env = &env;
        async move { env.as_user(u, m, &path, b).await }
    };

    // A member with no management permission may do none of it.
    let (st, _) = user(BOB, Method::POST, import(pack), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // MANAGE_GUILD_EXPRESSIONS alone adds a pack, sees only packs, and counts what it added.
    let (st, body) = user(EXPR, Method::POST, import(pack), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(body["added"].as_array().unwrap().len(), 1, "{body}");
    let (st, body) = user(EXPR, Method::GET, env.g("/packages"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["kind"], "emoji_pack");
    assert_eq!(body[0]["item_count"], 1);

    // ... but not a theme, a recipe, or taking a theme away.
    let (st, _) = user(
        EXPR,
        Method::PUT,
        one(theme),
        Some(json!({"section": "all"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = user(
        EXPR,
        Method::POST,
        env.g(&format!("/packages/{recipe}/apply")),
        Some(json!({})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // MANAGE_GUILD alone sees the whole list; a theme enabled for `all` is stored as `lounge` (D-281).
    let (st, body) = user(
        MGR,
        Method::PUT,
        one(theme),
        Some(json!({"section": "all"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(body["section"], "lounge");
    let (_, body) = user(MGR, Method::GET, env.g("/packages"), None).await;
    assert_eq!(body.as_array().unwrap().len(), 2);
    let (_, body) = user(EXPR, Method::GET, env.g("/packages"), None).await;
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Removing a theme needs MANAGE_GUILD; removing a pack needs only the expressions permission.
    let (st, _) = user(EXPR, Method::DELETE, one(theme), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = user(BOB, Method::DELETE, one(pack), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = user(EXPR, Method::DELETE, one(pack), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (_, body) = user(MGR, Method::GET, env.g("/packages"), None).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    let (st, _) = user(EXPR, Method::DELETE, one(pack), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = user(MGR, Method::DELETE, one(theme), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    // MANAGE_GUILD without the expressions permission may still import, as before (the items it cannot
    // create are skipped by the item permission).
    let (st, body) = user(MGR, Method::POST, import(pack), None).await;
    assert_eq!(st, StatusCode::OK, "{body}");
}
