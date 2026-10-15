//! FTR-TICK-001: ticket threads. Opening a ticket starts a private thread in the server's ticket channel as the
//! member (so the member needs `CREATE_PRIVATE_THREADS` there, which the setting's help says), adds the staff
//! role's members, and greets with a ping. Closing archives and locks the thread.

use super::{UserFeatDeps, me, raw, row_id, uerr};
use crate::routes::Ext;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgResult, parse_id};
use crate::routes::settings::deps::SettingsDeps;
use crate::routes::threads::ThreadsDeps;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path};
use serde::Deserialize;
use sr_chat::ChannelType;
use sr_chat::threads::{StartThread, ThreadPatch};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, GuildId, RoleId, UserId};
use sr_userfeat::features::TICKETS_ID;
use sr_userfeat::tickets::{Ticket, TicketInput};
use sr_userfeat::{Card, config::render_template};

/// Most staff members added to a ticket thread (a thread holds at most 1000 members).
const MAX_STAFF_ADDED: usize = 50;
const PRIVATE_THREAD_ARCHIVE_MINUTES: u32 = 10_080;

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// Whether the member answers tickets: they manage the server or hold the staff role.
pub(super) fn is_staff(
    access: &crate::routes::settings::deps::GuildAccess,
    staff_role: Option<i64>,
) -> bool {
    access.perms.has(Permissions::MANAGE_GUILD)
        || staff_role.is_some_and(|r| {
            access
                .member
                .role_ids
                .iter()
                .any(|m| i64::try_from(m.get()).ok() == Some(r))
        })
}

/// The caller's tickets, or every ticket of the server for staff.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/tickets",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("status" = Option<String>, Query, description = "open or closed"),
        ("limit" = Option<i64>, Query, description = "1 to 100, default 50"),
    ),
    responses((status = 200, body = Vec<Ticket>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> MsgResult<Json<Vec<Ticket>>> {
    let user = me(&p)?;
    let gid: GuildId = parse_id(&guild, ErrorCode::UnknownGuild)?;
    let access = s.guild_access(user, gid).await?;
    let g = i64::try_from(gid.get()).unwrap_or(0);
    let cfg = d.uf.settings(TICKETS_ID, Some(g)).await.map_err(uerr)?;
    let all = is_staff(&access, cfg.id("staff_role"));
    Ok(Json(
        d.uf.list_tickets(
            g,
            (!all).then(|| raw(user)),
            q.status.as_deref(),
            q.limit.unwrap_or(50),
        )
        .await
        .map_err(uerr)?,
    ))
}

/// Open a ticket: starts a private thread with the staff role in it.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/tickets",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = TicketInput,
    responses(
        (status = 200, body = Ticket),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "Too many open tickets or no ticket channel"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn open(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Ext(t): Ext<ThreadsDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<TicketInput>,
) -> MsgResult<Json<Ticket>> {
    let user = me(&p)?;
    let gid: GuildId = parse_id(&guild, ErrorCode::UnknownGuild)?;
    s.guild_access(user, gid).await?;
    let g = i64::try_from(gid.get()).unwrap_or(0);
    let subject =
        d.uf.check_can_open_ticket(g, raw(user), &body.subject)
            .await
            .map_err(uerr)?;
    let cfg = d.uf.settings(TICKETS_ID, Some(g)).await.map_err(uerr)?;
    let parent = ChannelId::new(cfg.id("channel").unwrap_or(0) as u64);
    let staff_role = cfg.id("staff_role");

    let name: String = format!("ticket: {subject}").chars().take(100).collect();
    let started = t
        .ops
        .start_standalone(
            user,
            parent,
            StartThread {
                name,
                auto_archive_duration: Some(PRIVATE_THREAD_ARCHIVE_MINUTES),
                kind: Some(ChannelType::PrivateThread),
                // Only the opener (the thread's owner) adds people.
                invitable: Some(false),
                ..StartThread::default()
            },
        )
        .await?;
    crate::routes::threads::start::announce_started(&t, &m, &started).await?;
    let thread = started.view.channel.id;

    // Staff join the thread. A failure for one of them never stops the ticket.
    if let Some(role) = staff_role {
        let role = RoleId::new(role as u64);
        let mut added = Vec::new();
        let mut after: Option<UserId> = None;
        'pages: loop {
            let page = m.repo.page_members(gid, after, 500).await?;
            let Some(last) = page.last().map(|x| x.user_id) else {
                break;
            };
            for member in page.iter().filter(|x| x.role_ids.contains(&role)) {
                if member.user_id == user {
                    continue;
                }
                if added.len() >= MAX_STAFF_ADDED {
                    break 'pages;
                }
                match t.ops.add_member(user, thread, member.user_id).await {
                    Ok(change) => {
                        if let (true, Some(row)) = (change.changed, change.member) {
                            added.push(row);
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "could not add staff to a ticket"),
                }
            }
            if page.len() < 500 {
                break;
            }
            after = Some(last);
        }
        if !added.is_empty() {
            t.publish_members(&started.view.channel, &added, &[]).await;
        }
    }

    let ticket =
        d.uf.record_ticket(
            g,
            raw(user),
            i64::try_from(thread.get()).unwrap_or(0),
            &subject,
        )
        .await
        .map_err(uerr)?;

    let mention = format!("<@{}>", raw(user));
    let greeting = render_template(
        cfg.text("greeting")
            .unwrap_or("Thanks {user}. Staff will be with you shortly."),
        &[("user", &mention), ("subject", &subject)],
    );
    let ping = staff_role.map(|r| format!(" <@&{r}>")).unwrap_or_default();
    // Best effort: the ticket exists either way.
    let _ = d
        .poster
        .post(
            i64::try_from(thread.get()).unwrap_or(0),
            Card {
                content: format!("{greeting}{ping}"),
                ping_users: vec![raw(user)],
                ping_roles: staff_role.into_iter().collect(),
                ..Card::default()
            },
        )
        .await;
    Ok(Json(ticket))
}

/// Close a ticket (its opener or staff): archives and locks the thread.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/tickets/{ticket_id}/close",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("ticket_id" = String, Path, description = "Ticket snowflake"),
    ),
    responses((status = 200, body = Ticket), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn close(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Ext(t): Ext<ThreadsDeps>,
    Path((guild, ticket)): Path<(String, String)>,
) -> MsgResult<Json<Ticket>> {
    let user = me(&p)?;
    let gid: GuildId = parse_id(&guild, ErrorCode::UnknownGuild)?;
    let access = s.guild_access(user, gid).await?;
    let g = i64::try_from(gid.get()).unwrap_or(0);
    let row =
        d.uf.get_ticket(g, row_id(&ticket, "ticket")?)
            .await
            .map_err(uerr)?;
    let cfg = d.uf.settings(TICKETS_ID, Some(g)).await.map_err(uerr)?;
    let staff = is_staff(&access, cfg.id("staff_role"));
    if !staff && row.opener_id != raw(user).to_string() {
        return Err(ErrorCode::MissingPermissions.into());
    }
    if row.status == "open"
        && let Ok(thread) = row.thread_id.parse::<u64>()
    {
        // Archive, and lock where the closer may; the ticket is closed even if the thread already went.
        let patch = |locked: bool| ThreadPatch {
            archived: Some(true),
            locked: locked.then_some(true),
            ..ThreadPatch::default()
        };
        let thread = ChannelId::new(thread);
        if t.modify(user, thread, patch(true)).await.is_err() {
            let _ = t.modify(user, thread, patch(false)).await;
        }
    }
    Ok(Json(
        d.uf.close_ticket(g, row_id(&ticket, "ticket")?, raw(user))
            .await
            .map_err(uerr)?,
    ))
}
