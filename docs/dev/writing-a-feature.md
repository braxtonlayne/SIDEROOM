# Writing a feature module

Everything optional in SIDE ROOM is a feature module (AGENTS.md rule 4; doc 11 section 1): an id, a settings schema, and handlers. The settings menu, the per-server on/off switch, the platform kill switch and event delivery are generic, so a module is usually one Rust file. This page walks through adding one. The built-ins in `crates/sr-features/src/modules/` (starboard, welcome, sticky, ...) are the reference.

## 1. Scaffold

```sh
just new-feature lounge.birthday_roles
```

The generator (`scripts/new-feature.sh`) takes `<section>.<snake_name>`, writes `crates/sr-features/src/modules/birthday_roles.rs` from `crates/sr-features/templates/feature.rs.tmpl`, declares the module and adds it to `register_all` in `modules/mod.rs`. It refuses an id that exists. Then:

```sh
cargo test -p sr-features birthday_roles
```

The template compiles and its two tests pass before you change anything, so you always start green.

## 2. The pieces of a module

| Piece | What it is |
|---|---|
| `id()` | Stable string, `section.snake_name`. Stored in `feature_enablements`; never rename it. |
| `meta()` | Name, description, settings-menu section, phase, `default_enabled`. New modules are **off by default**. |
| `settings_schema()` | Typed fields (`Bool`, `Int`, `Enum`, `String`, `Duration`, `Color`, `RolePicker`, `ChannelPicker`, `EmojiPicker`, `List`, `Group`). The web app renders it with the generic `SchemaForm`; nothing to build on the client. Values are validated server-side against the schema before they are stored. |
| `scopes()` | Where it can be configured: `Platform`, `Guild`, `Channel`, `User`. Most modules use `[Platform, Guild]`; the platform scope is a kill switch. |
| `on_event()` | Called for every guild event while the module is enabled in that guild. One event at a time per module, in order. |
| `on_tick()` | Called every 30 s for time-based work (expiries, schedules). Not gated by the guild flag: use `ctx.settings.enabled_guilds(ID)` to walk the guilds that use you. |

## 3. Reading settings

```rust
let cfg: Config = ctx.guild_config(MY_ID, guild).await?;
```

`Config` is a `#[derive(Deserialize)] #[serde(default)]` struct. The schema defaults are merged in first, so only list a default once, in the schema. Pickers store ids as decimal strings; parse them with `modules::snowflake`. Reading is cheap enough to do per event, and always reflects the admin's latest change.

## 4. Acting on the platform

A module never calls the chat or API crates. It uses `ctx.actions` (`FeatureActions`, `crates/sr-features/src/actions.rs`):

* `post_message`, `edit_message`, `delete_message`, `send_dm`: posted as the system user. Only users named in `mention_users` are pinged; `@everyone` and role mentions can never be set. Put user-controlled text through `template::defuse`.
* `add_role`, `remove_role`, `has_role`, `set_slowmode`, `set_channel_name`, `create_thread`: go through the guild services as the server owner, so they are validated, audited and broadcast like manual changes.
* `get_message`, `get_channel` (with `everyone_can_view`), `get_user`, `get_guild`, `reaction_count`, `remove_reaction`.

Ignore events authored by `ctx.actions.self_user()` so a module cannot trigger itself. If you need a capability that is not on the port, add the method to the trait, to `FakeActions` in `testing.rs`, and to `PgFeatureActions` in `crates/sr-server/src/feature_actions.rs`.

## 5. Remembering things

Settings are what an admin chose; state is what the module learned. `ctx.state` is a keyed JSON store per (feature, guild): `get`, `put`, `delete`, `list(prefix)`, and `due(feature, now)` for rows with a due time (`StateEntry::new(..).due(ms)`). The sticky module keeps its live post id there, the starboard its source-to-copy links, temporary roles their expiries. Rows live in `feature_state` (migration 0680) and go with the guild. Keys should be `kind:id` so `list` can find them.

## 6. Tests

Use `testing::Harness`: a `FeatureCtx` over a recording `FakeActions`, in-memory stores and a manual clock.

```rust
let h = Harness::new(registry).await;      // enables every registered module in guild 1
h.configure(MY_ID, json!({"channel": "10"})).await;
MyFeature.on_event(&event, &h.ctx).await.unwrap();
assert_eq!(h.actions.posts().len(), 1);
h.clock.advance_ms(60_000);
MyFeature.on_tick(&h.ctx).await.unwrap();
```

Cover at least: does nothing when unconfigured, the happy path, the module's own posts being ignored, and that `settings_schema().defaults(..)` validates against the schema. Anything touching Postgres goes in `crates/sr-server/src/feature_actions_pg_tests.rs` (needs `SR_TEST_DATABASE_URL`).

## 7. Shipping

* Tick the inventory box in `docs/inventory/09-feature-modules.md` with a note on where it lives.
* The settings form shows English labels from the schema. Platform-wide strings live in the i18n catalogs; schema labels are data and are translated with the module later.
* A module that needs a screen of its own (reaction-role message picker, leaderboards) adds a route under `Feature::routes()` or a normal API route plus a web page. The generic form still covers the settings.
