//! Member side: invoices, entitlement status, server support and leaderboards.

use super::{DonorCtx, dto::*, id, map_pay, now_ms};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use sr_core::{ApiError, ErrorCode};
use sr_payments::{
    Entitlements, PayError, Purpose,
    entitlements::DAY_MS,
    repo::{DonorPrefs, LeaderRow, PaymentRepo},
    service::InvoiceRequest,
    utc_month_start_ms,
};
use std::sync::Arc;
use utoipa::IntoParams;

fn perks_json(p: &sr_payments::PerkSet) -> serde_json::Value {
    serde_json::to_value(p).unwrap_or_else(|_| json!({}))
}

async fn invoice_view(ctx: &DonorCtx, inv: &sr_payments::Invoice) -> ApiResult<InvoiceView> {
    let txs = ctx
        .svc()
        .repo()
        .invoice_txs(inv.id)
        .await
        .map_err(map_pay)?;
    let cfg = ctx.svc().entitlements().config().await.map_err(map_pay)?;
    let explorer = txs
        .iter()
        .find(|t| t.dropped_ms.is_none())
        .and_then(|t| cfg.explorer_url(&t.txid));
    Ok(InvoiceView::build(inv, &txs, explorer, now_ms()))
}

/// Public donation settings: prices, tiers, perks, the current BTC price, and this month's
/// donations against the hosting cost.
#[utoipa::path(
    get, path = "/donations/config", operation_id = "donations_config",
    responses((status = 200, body = DonationConfigView), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn config(ctx: DonorCtx) -> ApiResult<Json<DonationConfigView>> {
    let svc = ctx.svc();
    let cfg = svc.entitlements().config().await.map_err(map_pay)?;
    let wallet = svc.repo().active_wallet().await.map_err(map_pay)?;
    let now = now_ms();
    let rate = svc.rates().quote(now).await.ok().map(|q| RateView {
        cents_per_btc: q.cents_per_btc,
        source: q.source,
        age_secs: u64::try_from((now - q.at_ms) / 1000).unwrap_or(0),
        stale: q.stale,
    });
    let month = utc_month_start_ms(now);
    let donated = svc.repo().donated_since(month).await.map_err(map_pay)?;
    Ok(Json(DonationConfigView {
        enabled: cfg.enabled && wallet.is_some() && svc.chain_configured(),
        network: wallet.map(|w| w.network),
        confirmations: cfg.confirmations,
        invoice_expiry_minutes: cfg.invoice_expiry_minutes,
        rate_lock_minutes: cfg.rate_lock_minutes,
        patron: PatronOffer {
            price_cents: cfg.patron_price_cents,
            max_months: cfg.patron_max_months,
            days_per_month: cfg.patron_days_per_month,
            grace_days: cfg.grace_days,
            perks: perks_json(&cfg.patron),
        },
        levels: cfg
            .levels
            .iter()
            .enumerate()
            .map(|(i, p)| LevelOffer {
                level: (i + 1) as u8,
                threshold_cents: cfg.level_thresholds_cents[i],
                perks: perks_json(p),
            })
            .collect(),
        free_perks: perks_json(&cfg.free),
        invite_credit_price_cents: cfg.invite_credit_price_cents,
        max_invite_credits: cfg.max_invite_credits,
        min_donation_cents: cfg.min_donation_cents,
        max_donation_cents: cfg.max_donation_cents,
        rate,
        hosting: HostingView {
            cost_cents: cfg.hosting_cost_cents,
            donated_this_month_cents: donated,
            month_start_ms: month,
        },
    }))
}

/// Create an invoice: a fresh address derived from the operator's watch-only wallet, the amount
/// in satoshis locked at the current rate. 503 when the address gap limit is reached or no fresh
/// price exists; 403 `SR_FEATURE_DISABLED` when donations are off; 429 over 10 open invoices.
#[utoipa::path(
    post, path = "/donations/invoices", operation_id = "donations_create_invoice",
    request_body = CreateInvoiceBody,
    responses((status = 201, body = InvoiceView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 429, body = ErrorBody),
              (status = 503, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_invoice(
    ctx: DonorCtx,
    Json(b): Json<CreateInvoiceBody>,
) -> ApiResult<(StatusCode, Json<InvoiceView>)> {
    let purpose = Purpose::parse(&b.purpose).ok_or_else(|| {
        ApiError::invalid_form().field("purpose", "ENUM_TYPE_INVALID", "Unknown purpose.")
    })?;
    let guild_id = match &b.guild_id {
        Some(g) => Some(id(g, "server")?),
        None => None,
    };
    let inv = ctx
        .svc()
        .create_invoice(
            ctx.user,
            InvoiceRequest {
                purpose,
                usd_cents: b.usd_cents,
                months: b.months,
                guild_id,
                credits: b.credits,
                anonymous: b.anonymous.unwrap_or(false),
            },
        )
        .await
        .map_err(map_pay)?;
    Ok((StatusCode::CREATED, Json(invoice_view(&ctx, &inv).await?)))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct PageQuery {
    /// Return invoices older than this id.
    pub before: Option<String>,
    /// At most 50 (default 25).
    pub limit: Option<u32>,
}

/// The caller's invoices, newest first (transactions are only in the single-invoice read).
#[utoipa::path(
    get, path = "/donations/invoices", operation_id = "donations_list_invoices",
    params(PageQuery),
    responses((status = 200, body = InvoicePage), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_invoices(
    ctx: DonorCtx,
    Query(q): Query<PageQuery>,
) -> ApiResult<Json<InvoicePage>> {
    let limit = q.limit.unwrap_or(25).clamp(1, 50);
    let before = match &q.before {
        Some(b) => Some(id(b, "invoice")?),
        None => None,
    };
    let rows = ctx
        .svc()
        .repo()
        .user_invoices(ctx.user, before, limit + 1)
        .await
        .map_err(map_pay)?;
    let more = rows.len() > limit as usize;
    let now = now_ms();
    let items: Vec<InvoiceView> = rows
        .iter()
        .take(limit as usize)
        .map(|i| InvoiceView::build(i, &[], None, now))
        .collect();
    let next_before = more.then(|| items.last().map(|i| i.id.clone())).flatten();
    Ok(Json(InvoicePage { items, next_before }))
}

/// One of the caller's invoices with its transactions.
#[utoipa::path(
    get, path = "/donations/invoices/{invoice_id}", operation_id = "donations_get_invoice",
    params(("invoice_id" = String, Path, description = "Invoice snowflake")),
    responses((status = 200, body = InvoiceView), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_invoice(
    ctx: DonorCtx,
    Path(invoice_id): Path<String>,
) -> ApiResult<Json<InvoiceView>> {
    let inv = ctx
        .svc()
        .own_invoice(ctx.user, id(&invoice_id, "invoice")?)
        .await
        .map_err(map_pay)?;
    Ok(Json(invoice_view(&ctx, &inv).await?))
}

/// The "I've paid" button: reads the chain for this invoice now (at most once per 10 seconds).
#[utoipa::path(
    post, path = "/donations/invoices/{invoice_id}/check", operation_id = "donations_check_invoice",
    params(("invoice_id" = String, Path, description = "Invoice snowflake")),
    responses((status = 200, body = InvoiceView), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn check_invoice(
    ctx: DonorCtx,
    Path(invoice_id): Path<String>,
) -> ApiResult<Json<InvoiceView>> {
    let inv = ctx
        .svc()
        .check_now(ctx.user, id(&invoice_id, "invoice")?)
        .await
        .map_err(map_pay)?;
    Ok(Json(invoice_view(&ctx, &inv).await?))
}

/// Cancel an open invoice that has received nothing (409 otherwise).
#[utoipa::path(
    post, path = "/donations/invoices/{invoice_id}/cancel", operation_id = "donations_cancel_invoice",
    params(("invoice_id" = String, Path, description = "Invoice snowflake")),
    responses((status = 200, body = InvoiceView), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn cancel_invoice(
    ctx: DonorCtx,
    Path(invoice_id): Path<String>,
) -> ApiResult<Json<InvoiceView>> {
    let inv = ctx
        .svc()
        .cancel(ctx.user, id(&invoice_id, "invoice")?)
        .await
        .map_err(map_pay)?;
    Ok(Json(invoice_view(&ctx, &inv).await?))
}

fn prefs_view(p: DonorPrefs) -> PrefsView {
    PrefsView {
        hide_name: p.hide_name,
        hide_amount: p.hide_amount,
        hide_badge: p.hide_badge,
    }
}

/// The caller's Patron status, effective limits and leaderboard preferences.
#[utoipa::path(
    get, path = "/donations/me", operation_id = "donations_me",
    responses((status = 200, body = MeView), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn me(ctx: DonorCtx) -> ApiResult<Json<MeView>> {
    let ents = ctx.svc().entitlements();
    let user = ents.user(ctx.user).await.map_err(map_pay)?;
    let limits = ents.limits(ctx.user, None).await.map_err(map_pay)?;
    let prefs = ctx
        .svc()
        .repo()
        .donor_prefs(ctx.user)
        .await
        .map_err(map_pay)?;
    Ok(Json(MeView {
        patron: user.patron.into(),
        limits: serde_json::to_value(limits).unwrap_or_else(|_| json!({})),
        preferences: prefs_view(prefs),
    }))
}

/// Change leaderboard privacy and the Patron badge.
#[utoipa::path(
    patch, path = "/donations/me/preferences", operation_id = "donations_patch_preferences",
    request_body = PrefsBody,
    responses((status = 200, body = PrefsView), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn patch_preferences(
    ctx: DonorCtx,
    Json(b): Json<PrefsBody>,
) -> ApiResult<Json<PrefsView>> {
    let repo = ctx.svc().repo();
    let cur = repo.donor_prefs(ctx.user).await.map_err(map_pay)?;
    let next = DonorPrefs {
        hide_name: b.hide_name.unwrap_or(cur.hide_name),
        hide_amount: b.hide_amount.unwrap_or(cur.hide_amount),
        hide_badge: b.hide_badge.unwrap_or(cur.hide_badge),
    };
    repo.set_donor_prefs(ctx.user, next)
        .await
        .map_err(map_pay)?;
    Ok(Json(prefs_view(next)))
}

/// A server's support level, pool and top contributors (members only).
#[utoipa::path(
    get, path = "/guilds/{guild_id}/support", operation_id = "donations_guild_support",
    params(("guild_id" = String, Path, description = "Server snowflake")),
    responses((status = 200, body = GuildSupportView), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn guild_support(
    ctx: DonorCtx,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<GuildSupportView>> {
    let gid = id(&guild_id, "server")?;
    let svc = ctx.svc();
    let access = svc
        .repo()
        .guild_access(ctx.user, gid)
        .await
        .map_err(map_pay)?;
    if !access.member {
        return Err(ErrorCode::SrNotFound.into());
    }
    let cfg = svc.entitlements().config().await.map_err(map_pay)?;
    let ent = svc.entitlements().guild(gid).await.map_err(map_pay)?;
    let now = now_ms();
    let rows = svc
        .repo()
        .top_contributors(gid, now - i64::from(cfg.pool_window_days) * DAY_MS, 10)
        .await
        .map_err(map_pay)?;
    let ids: Vec<i64> = rows.iter().filter_map(|r| r.user_id).collect();
    let profiles = svc.repo().profiles(&ids).await.map_err(map_pay)?;
    let top_contributors = rows
        .iter()
        .map(|r| {
            let p = r.user_id.and_then(|u| profiles.iter().find(|p| p.id == u));
            let anon = r.hide_name;
            ContributorView {
                user_id: (!anon).then(|| r.user_id.map(|u| u.to_string())).flatten(),
                name: if anon {
                    "Anonymous".to_owned()
                } else {
                    p.map_or_else(|| "Member".to_owned(), |p| p.name.clone())
                },
                avatar: if anon {
                    None
                } else {
                    p.and_then(|p| p.avatar.clone())
                },
                cents: (!r.hide_amount).then_some(r.cents),
                anonymous: anon,
            }
        })
        .collect();
    let level = usize::from(ent.level.min(3));
    Ok(Json(GuildSupportView {
        guild_id: gid.to_string(),
        level: ent.level,
        pool_cents: ent.pool_cents,
        pool_window_days: cfg.pool_window_days,
        next_threshold_cents: cfg.level_thresholds_cents.get(level).copied(),
        grace_until_ms: ent.grace_until_ms,
        perks: perks_json(if level == 0 {
            &cfg.free
        } else {
            &cfg.levels[level - 1]
        }),
        top_contributors,
        hidden_from_leaderboard: svc.repo().guild_hidden(gid).await.map_err(map_pay)?,
        is_owner: access.owner,
    }))
}

/// Hide the server from the public leaderboard (owner only).
#[utoipa::path(
    patch, path = "/guilds/{guild_id}/support", operation_id = "donations_patch_guild_support",
    params(("guild_id" = String, Path, description = "Server snowflake")),
    request_body = GuildSupportPatch,
    responses((status = 200, body = GuildSupportPatch), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn patch_guild_support(
    ctx: DonorCtx,
    Path(guild_id): Path<String>,
    Json(b): Json<GuildSupportPatch>,
) -> ApiResult<Json<GuildSupportPatch>> {
    let gid = id(&guild_id, "server")?;
    let access = ctx
        .svc()
        .repo()
        .guild_access(ctx.user, gid)
        .await
        .map_err(map_pay)?;
    if !access.member {
        return Err(ErrorCode::SrNotFound.into());
    }
    if !access.owner {
        return Err(ErrorCode::MissingAccess.into());
    }
    ctx.svc()
        .repo()
        .set_guild_hidden(gid, b.hidden_from_leaderboard)
        .await
        .map_err(map_pay)?;
    Ok(Json(b))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct BoardQuery {
    /// `30d` (default) or `lifetime`.
    pub window: Option<String>,
    /// At most 100 (default 50).
    pub limit: Option<u32>,
}

fn board_items(
    rows: &[LeaderRow],
    names: &[sr_payments::repo::Profile],
    guilds: bool,
) -> Vec<BoardItem> {
    rows.iter()
        .enumerate()
        .map(|(i, r)| {
            let key = if guilds { r.guild_id } else { r.user_id };
            let p = key.and_then(|k| names.iter().find(|p| p.id == k));
            let anon = r.hide_name && !guilds;
            BoardItem {
                rank: (i + 1) as u32,
                user_id: (!guilds && !anon)
                    .then(|| r.user_id.map(|u| u.to_string()))
                    .flatten(),
                guild_id: guilds.then(|| r.guild_id.map(|g| g.to_string())).flatten(),
                name: if anon {
                    "Anonymous patron".to_owned()
                } else {
                    p.map_or_else(|| "Unknown".to_owned(), |p| p.name.clone())
                },
                avatar: if anon {
                    None
                } else {
                    p.and_then(|p| p.avatar.clone())
                },
                cents: (!r.hide_amount).then_some(r.cents),
                anonymous: anon,
            }
        })
        .collect()
}

pub(super) async fn build_board(
    repo: &Arc<dyn PaymentRepo>,
    board: &str,
    since: Option<i64>,
) -> Result<Vec<BoardItem>, PayError> {
    match board {
        "donors" => {
            let rows = repo.leaderboard_donors(since, 100).await?;
            let ids: Vec<i64> = rows.iter().filter_map(|r| r.user_id).collect();
            let names = repo.profiles(&ids).await?;
            Ok(board_items(&rows, &names, false))
        }
        _ => {
            let rows = repo.leaderboard_servers(since, 100).await?;
            let ids: Vec<i64> = rows.iter().filter_map(|r| r.guild_id).collect();
            let names = repo.guild_profiles(&ids).await?;
            Ok(board_items(&rows, &names, true))
        }
    }
}

/// A public leaderboard: `donors` (individuals) or `servers`. Donors who hide their name show as
/// "Anonymous patron"; hidden amounts are omitted (rank only). Cached for an hour.
#[utoipa::path(
    get, path = "/donations/leaderboards/{board}", operation_id = "donations_leaderboard",
    params(("board" = String, Path, description = "`donors` or `servers`"), BoardQuery),
    responses((status = 200, body = BoardView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn leaderboard(
    ctx: DonorCtx,
    Path(board): Path<String>,
    Query(q): Query<BoardQuery>,
) -> ApiResult<Json<BoardView>> {
    if board != "donors" && board != "servers" {
        return Err(ApiError::invalid_form()
            .field("board", "ENUM_TYPE_INVALID", "Use donors or servers.")
            .into());
    }
    let window = q.window.unwrap_or_else(|| "30d".to_owned());
    let since = match window.as_str() {
        "30d" => Some(now_ms() - 30 * DAY_MS),
        "lifetime" => None,
        _ => {
            return Err(ApiError::invalid_form()
                .field("window", "ENUM_TYPE_INVALID", "Use 30d or lifetime.")
                .into());
        }
    };
    let key = (board.clone(), window.clone());
    let cached = match ctx.deps.boards.get(&key).await {
        Some(c) => c,
        None => {
            let items = build_board(ctx.svc().repo(), &board, since)
                .await
                .map_err(map_pay)?;
            let entry = Arc::new((now_ms(), items));
            ctx.deps.boards.insert(key, entry.clone()).await;
            entry
        }
    };
    let limit = q.limit.unwrap_or(50).clamp(1, 100) as usize;
    let me = if board == "donors" {
        ctx.svc()
            .repo()
            .donor_rank(ctx.user, since)
            .await
            .map_err(map_pay)?
            .map(|(rank, cents)| MyRank { rank, cents })
    } else {
        None
    };
    Ok(Json(BoardView {
        board,
        window,
        items: cached.1.iter().take(limit).cloned().collect(),
        me,
        generated_at_ms: cached.0,
    }))
}
