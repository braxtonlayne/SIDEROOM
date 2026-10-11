# Future Idea — Content Bounties

**Status: documented only. Do not build** (D-090). Captured so the idea isn't lost and so current schemas don't block it.

## 1. Idea
A member requests a title or series (normal Request, see the submissions spec §7) and **attaches a bounty**: a promise to pay whoever gets a matching submission approved.

## 2. Options considered
| Model | Description | Trade-offs |
|---|---|---|
| **Good-faith pledge** (simplest) | Pledge amount shown on the request. When fulfilled, the requester is asked to pay the fulfiller directly (BTC address from the fulfiller's profile). Staff can mark pledges "honoured" or "unpaid" | No custody, no liability. Relies on trust. Reputation badges help |
| **Staff-mediated** | Same, but staff confirm both sides (submission approved, payment txid observed) before marking it complete. Unpaid pledges hurt the requester's ability to post new bounties | Small staff workload. Still no custody |
| **Escrow** | Requester pays an invoice to a platform-controlled address. Funds are released to the fulfiller on approval, refunded on expiry | Real custody of funds (keys on the server or multisig with staff), with legal and operational complexity. Only reasonable with crypto. Not recommended unless the operator explicitly accepts that responsibility |

**Recommended if ever built:** good-faith pledge + staff mediation, built on the existing `invoices` machinery for transparent payment proof (the fulfiller generates an invoice, and the requester pays it).

## 3. Schema reservations
`requests` will gain `bounty_amount_usd_cents`, `bounty_state`, `bounty_fulfiller_id`, `bounty_invoice_id`. No other existing tables need changes.

## 4. Open questions for the owner (when revisited)
Minimum and maximum amounts, multiple pledgers per request, partial fulfilment (e.g. 10 of 12 episodes), dispute process, leaderboard impact.
