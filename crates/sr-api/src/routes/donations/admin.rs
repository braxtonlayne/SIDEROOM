//! Staff side (`MANAGE_DONATIONS`): wallet and health, configuration, invoices and manual
//! resolution, entitlement grants, rates, reports and leaderboard moderation. Every write
//! requires a reason and appends to the staff access log; the descriptor is never returned.

use super::{MoneyCtx, dto::*, id, map_pay, member::build_board, now_ms};
use crate::{
    error::ApiResult,
    openapi::ErrorBody,
    routes::admin::{ReasonBody, reason_from},
};
use axum::{
    Json,
    extract::{Path, Query},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sr_core::{ApiError, ErrorCode};
use sr_payments::{
    DonationConfig, Invoice, PayError,
    descriptor::{Network, WatchDescriptor},
    entitlements::{DAY_MS, Grant, GrantKind},
    invoice::{InvoiceState, Purpose, ResolutionKind},
    repo::{GrantFilter, GrantSpec, InvoiceFilter},
    service::PaymentService,
    utc_month_start_ms,
};
use utoipa::IntoParams;

fn bad(field: &str, msg: &str) -> crate::error::HttpError {
    ApiError::invalid_form()
        .field(field, "BASE_TYPE_INVALID", msg)
        .into()
}

fn rate_view(q: sr_payments::rates::RateQuote, now: i64) -> RateView {
    RateView {
        cents_per_btc: q.cents_per_btc,
        source: q.source,
        age_secs: u64::try_from((now - q.at_ms) / 1000).unwrap_or(0),
        stale: q.stale,
    }
}

fn source_views(svc: &PaymentService) -> Vec<RateSourceView> {
    svc.rates()
        .statuses()
        .into_iter()
        .map(|s| RateSourceView {
            name: s.name,
            last_cents: s.last_cents,
            last_ok_ms: s.last_ok_ms,
            last_error: s.last_error,
        })
        .collect()
}

/// Wallet health, watcher status, rate sources and invoice counts. Reveals no descriptor or key.
#[utoipa::path(
    get, path = "/admin/donations/status", operation_id = "admin_donations_status",
    responses((status = 200, body = StatusView), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn status(ctx: MoneyCtx) -> ApiResult<Json<StatusView>> {
    ctx.require("donations.status").await?;
    let svc = ctx.svc();
    let cfg = svc.entitlements().config().await.map_err(map_pay)?;
    let wallet = svc.repo().active_wallet().await.map_err(map_pay)?;
    let now = now_ms();
    let wallet_status = match &wallet {
        Some(w) => Some(WalletStatus {
            network: w.network.clone(),
            fingerprint: w.fingerprint.clone(),
            next_index: w.next_index,
            gap_limit: w.gap_limit,
            gap_used: svc.repo().gap_used(w.id).await.map_err(map_pay)?,
        }),
        None => None,
    };
    let w = svc.repo().watcher_status().await.map_err(map_pay)?;
    let report = svc
        .repo()
        .report(now - 90 * DAY_MS, now + 1)
        .await
        .map_err(map_pay)?;
    let mut counts = serde_json::Map::new();
    for (s, n) in &report.by_state {
        counts.insert(s.as_str().to_owned(), json!(n));
    }
    let attention = svc
        .repo()
        .search_invoices(&InvoiceFilter {
            needs_attention: true,
            limit: 200,
            ..InvoiceFilter::default()
        })
        .await
        .map_err(map_pay)?
        .len();
    Ok(Json(StatusView {
        enabled: cfg.enabled && wallet.is_some() && svc.chain_configured(),
        wallet: wallet_status,
        watcher: WatcherView {
            last_run_ms: w.last_run_ms,
            last_ok_ms: w.last_ok_ms,
            last_error: w.last_error,
            tip_height: w.tip_height,
            polled: w.polled,
        },
        counts: Value::Object(counts),
        needs_attention: i64::try_from(attention).unwrap_or(0),
        rate: svc.rates().last().map(|q| rate_view(q, now)),
        rate_sources: source_views(svc),
    }))
}

fn network(raw: &str) -> Result<Network, crate::error::HttpError> {
    Network::parse(raw).ok_or_else(|| bad("network", "Use mainnet, testnet, signet or regtest."))
}

/// Set the active wallet from a watch-only descriptor. Descriptors with private keys are
/// refused. Older wallets stay watched for their open invoices. Returns the first addresses to
/// compare with the operator's own wallet.
#[utoipa::path(
    put, path = "/admin/donations/wallet", operation_id = "admin_donations_set_wallet",
    request_body = SetWalletBody,
    responses((status = 200, body = WalletResult), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_wallet(
    ctx: MoneyCtx,
    Json(b): Json<SetWalletBody>,
) -> ApiResult<Json<WalletResult>> {
    ctx.require("donations.wallet.set").await?;
    let reason = MoneyCtx::reason(b.reason.as_deref())?;
    let net = network(&b.network)?;
    let (wallet, addresses) = ctx
        .svc()
        .set_wallet(&b.descriptor, net, b.gap_limit.unwrap_or(20), ctx.user())
        .await
        .map_err(map_pay)?;
    ctx.log(
        "donations.wallet.set",
        Some(("wallet", wallet.id)),
        &reason,
        json!({ "fingerprint": wallet.fingerprint, "network": wallet.network, "gap_limit": wallet.gap_limit }),
    )
    .await?;
    Ok(Json(WalletResult {
        network: wallet.network,
        fingerprint: wallet.fingerprint,
        addresses,
    }))
}

/// Derive the first receive addresses of a descriptor without saving anything.
#[utoipa::path(
    post, path = "/admin/donations/wallet/test", operation_id = "admin_donations_test_wallet",
    request_body = TestWalletBody,
    responses((status = 200, body = TestWalletResult), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn test_wallet(
    ctx: MoneyCtx,
    Json(b): Json<TestWalletBody>,
) -> ApiResult<Json<TestWalletResult>> {
    ctx.require("donations.wallet.test").await?;
    let net = network(&b.network)?;
    let d = WatchDescriptor::parse(&b.descriptor, net).map_err(|e| map_pay(e.into()))?;
    let addresses = PaymentService::test_derivation(&b.descriptor, net, 5).map_err(map_pay)?;
    Ok(Json(TestWalletResult {
        fingerprint: d.fingerprint(),
        addresses,
    }))
}

/// The donation settings (prices, thresholds, perks, depths, expiry).
#[utoipa::path(
    get, path = "/admin/donations/config", operation_id = "admin_donations_get_config",
    responses((status = 200, body = Object), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_config(ctx: MoneyCtx) -> ApiResult<Json<Value>> {
    ctx.require("donations.config.view").await?;
    let cfg = ctx.svc().repo().config().await.map_err(map_pay)?;
    Ok(Json(
        serde_json::to_value(cfg).unwrap_or_else(|_| json!({})),
    ))
}

fn merge(into: &mut Value, patch: &Value) {
    match (into, patch) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge(a.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (slot, v) => *slot = v.clone(),
    }
}

/// Change settings with a JSON merge patch. The result is validated as a whole; nothing applies
/// on a failure. Takes effect immediately (no restart).
#[utoipa::path(
    patch, path = "/admin/donations/config", operation_id = "admin_donations_patch_config",
    request_body = PatchConfigBody,
    responses((status = 200, body = Object), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn patch_config(ctx: MoneyCtx, Json(b): Json<PatchConfigBody>) -> ApiResult<Json<Value>> {
    ctx.require("donations.config.set").await?;
    let reason = MoneyCtx::reason(b.reason.as_deref())?;
    if !b.changes.is_object() {
        return Err(bad("changes", "Send an object."));
    }
    let cur = ctx.svc().repo().config().await.map_err(map_pay)?;
    let mut doc = serde_json::to_value(&cur).unwrap_or_else(|_| json!({}));
    merge(&mut doc, &b.changes);
    let next: DonationConfig = serde_json::from_value(doc)
        .map_err(|e| bad("changes", &format!("Not a valid setting: {e}")))?;
    next.validate().map_err(|m| bad("changes", &m))?;
    ctx.svc()
        .repo()
        .set_config(&next, ctx.user(), now_ms())
        .await
        .map_err(map_pay)?;
    ctx.svc().entitlements().invalidate_config().await;
    ctx.log(
        "donations.config.set",
        None,
        &reason,
        json!({ "changes": b.changes }),
    )
    .await?;
    Ok(Json(
        serde_json::to_value(next).unwrap_or_else(|_| json!({})),
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct InvoiceQuery {
    pub state: Option<String>,
    pub purpose: Option<String>,
    pub user_id: Option<String>,
    /// An invoice id, an address, or a transaction id prefix.
    pub q: Option<String>,
    /// Only underpaid, overpaid and late-paid invoices.
    pub needs_attention: Option<bool>,
    pub before: Option<String>,
    pub limit: Option<u32>,
}

async fn admin_view(
    ctx: &MoneyCtx,
    inv: &Invoice,
    with_audit: bool,
) -> ApiResult<AdminInvoiceView> {
    let svc = ctx.svc();
    let txs = svc.repo().invoice_txs(inv.id).await.map_err(map_pay)?;
    let cfg = svc.entitlements().config().await.map_err(map_pay)?;
    let explorer = txs
        .iter()
        .find(|t| t.dropped_ms.is_none())
        .and_then(|t| cfg.explorer_url(&t.txid));
    let name = svc
        .repo()
        .profiles(&[inv.user_id])
        .await
        .map_err(map_pay)?
        .into_iter()
        .next()
        .map(|p| p.name);
    let audit = if with_audit {
        svc.repo()
            .invoice_audit(inv.id)
            .await
            .map_err(map_pay)?
            .into_iter()
            .map(|a| AuditView {
                at_ms: a.at_ms,
                kind: a.kind,
                detail: a.detail,
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(AdminInvoiceView {
        invoice: InvoiceView::build(inv, &txs, explorer, now_ms()),
        user_id: inv.user_id.to_string(),
        user_name: name,
        derivation_index: inv.derivation_index,
        resolution_note: resolution_note(&inv.resolution),
        resolved_by: inv.resolution.as_ref().map(|r| r.by.to_string()),
        audit,
    })
}

/// Search invoices (newest first).
#[utoipa::path(
    get, path = "/admin/donations/invoices", operation_id = "admin_donations_invoices",
    params(InvoiceQuery),
    responses((status = 200, body = AdminInvoicePage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn invoices(
    ctx: MoneyCtx,
    Query(q): Query<InvoiceQuery>,
) -> ApiResult<Json<AdminInvoicePage>> {
    ctx.require("donations.invoices.list").await?;
    let state = match q.state.as_deref() {
        Some(s) => Some(InvoiceState::parse(s).ok_or_else(|| bad("state", "Unknown state."))?),
        None => None,
    };
    let purpose = match q.purpose.as_deref() {
        Some(s) => Some(Purpose::parse(s).ok_or_else(|| bad("purpose", "Unknown purpose."))?),
        None => None,
    };
    let limit = q.limit.unwrap_or(25).clamp(1, 100);
    let rows = ctx
        .svc()
        .repo()
        .search_invoices(&InvoiceFilter {
            state,
            purpose,
            user_id: q.user_id.as_deref().map(|u| id(u, "account")).transpose()?,
            q: q.q
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_owned()),
            needs_attention: q.needs_attention.unwrap_or(false),
            before_id: q.before.as_deref().map(|u| id(u, "invoice")).transpose()?,
            limit: limit + 1,
        })
        .await
        .map_err(map_pay)?;
    let more = rows.len() > limit as usize;
    let mut items = Vec::new();
    for inv in rows.iter().take(limit as usize) {
        items.push(admin_view(&ctx, inv, false).await?);
    }
    let next_before = more
        .then(|| items.last().map(|i| i.invoice.id.clone()))
        .flatten();
    Ok(Json(AdminInvoicePage { items, next_before }))
}

/// One invoice with its payer, transactions and full audit trail. The read is access-logged.
#[utoipa::path(
    get, path = "/admin/donations/invoices/{invoice_id}", operation_id = "admin_donations_invoice",
    params(("invoice_id" = String, Path, description = "Invoice snowflake")),
    responses((status = 200, body = AdminInvoiceView), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn invoice(
    ctx: MoneyCtx,
    Path(invoice_id): Path<String>,
) -> ApiResult<Json<AdminInvoiceView>> {
    ctx.require("donations.invoice.view").await?;
    let iid = id(&invoice_id, "invoice")?;
    let inv = ctx
        .svc()
        .repo()
        .invoice(iid)
        .await
        .map_err(map_pay)?
        .ok_or(ErrorCode::SrNotFound)?;
    ctx.log_read("donations.invoice.view", Some(("invoice", iid)))
        .await?;
    Ok(Json(admin_view(&ctx, &inv, true).await?))
}

/// Close an invoice by hand: `mark_paid` (credits the purpose once), `refund_noted` (the operator
/// refunded outside this system; a credit already given is reversed) or `dismiss`. The server
/// never moves funds.
#[utoipa::path(
    post, path = "/admin/donations/invoices/{invoice_id}/resolve", operation_id = "admin_donations_resolve",
    params(("invoice_id" = String, Path, description = "Invoice snowflake")),
    request_body = ResolveBody,
    responses((status = 200, body = AdminInvoiceView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn resolve(
    ctx: MoneyCtx,
    Path(invoice_id): Path<String>,
    Json(b): Json<ResolveBody>,
) -> ApiResult<Json<AdminInvoiceView>> {
    ctx.require("donations.invoice.resolve").await?;
    let reason = MoneyCtx::reason(b.reason.as_deref())?;
    let iid = id(&invoice_id, "invoice")?;
    let kind = ResolutionKind::parse(&b.resolution)
        .ok_or_else(|| bad("resolution", "Use mark_paid, refund_noted or dismiss."))?;
    let note = b.note.trim();
    if note.is_empty() || note.chars().count() > 1000 {
        return Err(bad("note", "Write a note of up to 1000 characters."));
    }
    let out = ctx
        .svc()
        .resolve(iid, kind, note, ctx.user())
        .await
        .map_err(map_pay)?;
    ctx.log(
        "donations.invoice.resolve",
        Some(("invoice", iid)),
        &reason,
        json!({ "resolution": kind.as_str(), "credit": format!("{:?}", out.credit).to_lowercase(),
                "note": note }),
    )
    .await?;
    Ok(Json(admin_view(&ctx, &out.invoice, true).await?))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct GrantQuery {
    /// `patron` or `server_pool`.
    pub kind: Option<String>,
    pub user_id: Option<String>,
    pub guild_id: Option<String>,
    pub active: Option<bool>,
    pub before: Option<String>,
    pub limit: Option<u32>,
}

fn grant_view(g: &Grant) -> GrantView {
    GrantView {
        id: g.id.to_string(),
        kind: g.kind.as_str().to_owned(),
        user_id: g.user_id.map(|u| u.to_string()),
        guild_id: g.guild_id.map(|u| u.to_string()),
        source: g.source.as_str().to_owned(),
        source_id: g.source_id.map(|u| u.to_string()),
        days: g.days,
        ends_at_ms: g.ends_ms,
        permanent: g.permanent,
        usd_cents: g.usd_cents,
        starts_at_ms: g.starts_ms,
        revoked_at_ms: g.revoked_ms,
        reason: g.reason.clone(),
    }
}

/// Entitlement grants (from invoices, staff, or the legacy download table), newest first.
#[utoipa::path(
    get, path = "/admin/donations/entitlements", operation_id = "admin_donations_entitlements",
    params(GrantQuery),
    responses((status = 200, body = GrantPage), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn entitlements(
    ctx: MoneyCtx,
    Query(q): Query<GrantQuery>,
) -> ApiResult<Json<GrantPage>> {
    ctx.require("donations.entitlements.list").await?;
    let kind = match q.kind.as_deref() {
        Some(k) => {
            Some(GrantKind::parse(k).ok_or_else(|| bad("kind", "Use patron or server_pool."))?)
        }
        None => None,
    };
    let limit = q.limit.unwrap_or(25).clamp(1, 100);
    let rows = ctx
        .svc()
        .repo()
        .list_grants(&GrantFilter {
            kind,
            user_id: q.user_id.as_deref().map(|u| id(u, "account")).transpose()?,
            guild_id: q.guild_id.as_deref().map(|u| id(u, "server")).transpose()?,
            active_only: q.active.unwrap_or(false),
            before_id: q.before.as_deref().map(|u| id(u, "grant")).transpose()?,
            limit: limit + 1,
        })
        .await
        .map_err(map_pay)?;
    let more = rows.len() > limit as usize;
    let items: Vec<GrantView> = rows.iter().take(limit as usize).map(grant_view).collect();
    let next_before = more.then(|| items.last().map(|i| i.id.clone())).flatten();
    Ok(Json(GrantPage { items, next_before }))
}

/// Grant Patron time or a server pool contribution by hand.
#[utoipa::path(
    post, path = "/admin/donations/entitlements", operation_id = "admin_donations_grant",
    request_body = GrantBody,
    responses((status = 200, body = GrantView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn grant(ctx: MoneyCtx, Json(b): Json<GrantBody>) -> ApiResult<Json<GrantView>> {
    ctx.require("donations.entitlements.grant").await?;
    let reason = MoneyCtx::reason(b.reason.as_deref())?;
    let kind =
        GrantKind::parse(&b.kind).ok_or_else(|| bad("kind", "Use patron or server_pool."))?;
    let permanent = b.permanent.unwrap_or(false);
    let spec = match kind {
        GrantKind::Patron => {
            let user = id(b.user_id.as_deref().unwrap_or(""), "account")?;
            if !permanent && !b.days.is_some_and(|d| (1..=3650).contains(&d)) {
                return Err(bad("days", "Give 1 to 3650 days, or make it permanent."));
            }
            GrantSpec {
                kind,
                user_id: Some(user),
                guild_id: None,
                days: (!permanent).then_some(b.days).flatten(),
                usd_cents: None,
            }
        }
        GrantKind::ServerPool => {
            let guild = id(b.guild_id.as_deref().unwrap_or(""), "server")?;
            if permanent || !b.usd_cents.is_some_and(|c| (1..=100_000_000).contains(&c)) {
                return Err(bad("usd_cents", "Give the contribution in cents."));
            }
            GrantSpec {
                kind,
                user_id: None,
                guild_id: Some(guild),
                days: None,
                usd_cents: b.usd_cents,
            }
        }
    };
    let target = spec.user_id.or(spec.guild_id).unwrap_or(0);
    let g = ctx
        .svc()
        .admin_grant(spec, permanent, ctx.user(), &reason)
        .await
        .map_err(map_pay)?;
    ctx.log(
        "donations.entitlement.grant",
        Some((if kind == GrantKind::Patron { "account" } else { "guild" }, target)),
        &reason,
        json!({ "grant": g.id.to_string(), "kind": kind.as_str(), "days": g.days, "permanent": g.permanent,
                "usd_cents": g.usd_cents }),
    )
    .await?;
    Ok(Json(grant_view(&g)))
}

/// Revoke a grant.
#[utoipa::path(
    delete, path = "/admin/donations/entitlements/{grant_id}", operation_id = "admin_donations_revoke",
    params(("grant_id" = String, Path, description = "Grant snowflake")),
    request_body(content = ReasonBody, description = "The reason may also be sent as X-Audit-Log-Reason."),
    responses((status = 200, body = GrantView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn revoke(
    ctx: MoneyCtx,
    Path(grant_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<GrantView>> {
    ctx.require("donations.entitlement.revoke").await?;
    let r = reason_from(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    let reason = MoneyCtx::reason(r.as_deref())?;
    let gid = id(&grant_id, "grant")?;
    let g = ctx
        .svc()
        .admin_revoke(gid, ctx.user(), &reason)
        .await
        .map_err(map_pay)?;
    ctx.log(
        "donations.entitlement.revoke",
        Some(("grant", gid)),
        &reason,
        json!({ "kind": g.kind.as_str() }),
    )
    .await?;
    Ok(Json(grant_view(&g)))
}

/// The price sources and the current median.
#[utoipa::path(
    get, path = "/admin/donations/rates", operation_id = "admin_donations_rates",
    responses((status = 200, body = RatesView), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn rates(ctx: MoneyCtx) -> ApiResult<Json<RatesView>> {
    ctx.require("donations.rates.view").await?;
    let svc = ctx.svc();
    let now = now_ms();
    let current = svc.rates().quote(now).await.ok().map(|q| rate_view(q, now));
    Ok(Json(RatesView {
        current,
        sources: source_views(svc),
    }))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ReportQuery {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
}

/// Credited totals by purpose and by day, and invoice counts by state (default: this month).
#[utoipa::path(
    get, path = "/admin/donations/report", operation_id = "admin_donations_report",
    params(ReportQuery),
    responses((status = 200, body = ReportView), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn report(ctx: MoneyCtx, Query(q): Query<ReportQuery>) -> ApiResult<Json<ReportView>> {
    ctx.require("donations.report").await?;
    let now = now_ms();
    let from = q.from_ms.unwrap_or_else(|| utc_month_start_ms(now));
    let to = q.to_ms.unwrap_or(now + 1);
    if to <= from {
        return Err(bad("to_ms", "to_ms must be after from_ms."));
    }
    let r = ctx.svc().repo().report(from, to).await.map_err(map_pay)?;
    let mut states = serde_json::Map::new();
    for (s, n) in &r.by_state {
        states.insert(s.as_str().to_owned(), json!(n));
    }
    Ok(Json(ReportView {
        from_ms: from,
        to_ms: to,
        credited_cents: r.credited_cents,
        by_purpose: r
            .by_purpose
            .iter()
            .map(|(p, c, n)| json!({ "purpose": p, "cents": c, "count": n }))
            .collect(),
        by_day: r
            .by_day
            .iter()
            .map(|(d, c)| json!({ "day": d, "cents": c }))
            .collect(),
        by_state: Value::Object(states),
    }))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AdminBoardQuery {
    /// `30d` (default) or `lifetime`.
    pub window: Option<String>,
}

/// The ranking with real names and amounts (staff view; the read is access-logged). Entries staff
/// hid are not listed.
#[utoipa::path(
    get, path = "/admin/donations/leaderboard/{board}", operation_id = "admin_donations_leaderboard",
    params(("board" = String, Path, description = "`donors` or `servers`"), AdminBoardQuery),
    responses((status = 200, body = BoardView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn leaderboard(
    ctx: MoneyCtx,
    Path(board): Path<String>,
    Query(q): Query<AdminBoardQuery>,
) -> ApiResult<Json<BoardView>> {
    ctx.require("donations.leaderboard.view").await?;
    if board != "donors" && board != "servers" {
        return Err(bad("board", "Use donors or servers."));
    }
    let window = q.window.unwrap_or_else(|| "30d".to_owned());
    let since = match window.as_str() {
        "30d" => Some(now_ms() - 30 * DAY_MS),
        "lifetime" => None,
        _ => return Err(bad("window", "Use 30d or lifetime.")),
    };
    ctx.log_read("donations.leaderboard.view", None).await?;
    let mut items = build_board(ctx.svc().repo(), &board, since)
        .await
        .map_err(map_pay)?;
    // Staff see the real names and amounts; `anonymous` keeps telling what the donor chose.
    if board == "donors" {
        let rows = ctx
            .svc()
            .repo()
            .leaderboard_donors(since, 100)
            .await
            .map_err(map_pay)?;
        let ids: Vec<i64> = rows.iter().filter_map(|r| r.user_id).collect();
        let names = ctx.svc().repo().profiles(&ids).await.map_err(map_pay)?;
        for (item, row) in items.iter_mut().zip(&rows) {
            if let Some(u) = row.user_id {
                item.user_id = Some(u.to_string());
                if let Some(p) = names.iter().find(|p| p.id == u) {
                    item.name = p.name.clone();
                    item.avatar = p.avatar.clone();
                }
                item.cents = Some(row.cents);
            }
        }
    }
    Ok(Json(BoardView {
        board,
        window,
        items,
        me: None,
        generated_at_ms: now_ms(),
    }))
}

/// Keep a donor or a server off the public leaderboards (or put them back).
#[utoipa::path(
    post, path = "/admin/donations/leaderboard/hide", operation_id = "admin_donations_hide",
    request_body = HideBody,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn hide(ctx: MoneyCtx, Json(b): Json<HideBody>) -> ApiResult<axum::http::StatusCode> {
    ctx.require("donations.leaderboard.hide").await?;
    let reason = MoneyCtx::reason(b.reason.as_deref())?;
    let user = b.user_id.as_deref().map(|u| id(u, "account")).transpose()?;
    let guild = b.guild_id.as_deref().map(|u| id(u, "server")).transpose()?;
    if user.is_none() == guild.is_none() {
        return Err(bad("user_id", "Give either user_id or guild_id."));
    }
    ctx.svc()
        .repo()
        .set_staff_hidden(user, guild, b.hidden)
        .await
        .map_err(|e: PayError| map_pay(e))?;
    ctx.deps.boards.invalidate_all();
    ctx.log(
        "donations.leaderboard.hide",
        user.map(|u| ("account", u)).or(guild.map(|g| ("guild", g))),
        &reason,
        json!({ "hidden": b.hidden }),
    )
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
