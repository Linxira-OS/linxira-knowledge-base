/**
 * Billing data access (`/api/v1/billing/...`).
 *
 * The deployment feature gate (`features.billing`) decides whether the tab
 * renders at all; these calls go through `api()` and therefore carry the
 * session CSRF header on writes. Records mirror the backend serde shapes;
 * amounts are integer micros (1/1 000 000 of the currency unit).
 */
import { api, queryString } from './client'

export interface BillingPlan {
  plan_id: string
  name: string
  price_micros: number
  included_tokens: number
  currency: string
  period: string
  status: string
  features_json: string
  created_at: string
  updated_at: string
}

export interface BillingWallet {
  wallet_id: string
  tenant_id: string
  currency: string
  balance_micros: number
  status: string
  updated_at: string
}

export interface BillingLedgerEntry {
  entry_id: string
  group_id: string
  wallet_id: string
  tenant_id: string
  account: string
  direction: string
  amount_micros: number
  currency: string
  idempotency_key: string
  request_id: string | null
  metadata_json: string
  created_at: string
}

export interface BillingTopup {
  topup_id: string
  wallet_id: string
  tenant_id: string
  amount_micros: number
  currency: string
  channel: string
  provider_ref: string | null
  status: string
  idempotency_key: string
  created_at: string
  updated_at: string
}

export const SUBSCRIPTION_STATUSES = ['trial', 'active', 'past_due', 'suspended', 'cancelled'] as const
export type SubscriptionStatus = (typeof SUBSCRIPTION_STATUSES)[number]

export interface BillingSubscription {
  subscription_id: string
  tenant_id: string
  plan_id: string
  status: SubscriptionStatus
  trial_ends_at: string | null
  current_period_start: string | null
  current_period_end: string | null
  past_due_at: string | null
  suspended_at: string | null
  cancelled_at: string | null
  updated_at: string
}

export interface BillingInvoice {
  invoice_id: string
  tenant_id: string
  period_start: string | null
  period_end: string | null
  amount_micros: number
  currency: string
  status: string
  created_at: string
}

export interface BillingEntitlement {
  tenant_id: string
  plan_id: string | null
  status: string
  balance_micros: number
  included_tokens: number
  used_tokens: number
  currency: string
  period_start: string | null
  period_end: string | null
  updated_at: string
}

/**
 * Legal subscription transitions, mirrored from
 * `SubscriptionStatus::can_transition` in `src/billing.rs`. The UI only offers
 * edges the backend accepts; the backend still validates (400
 * `invalid_billing_request` on an illegal move).
 */
export const SUBSCRIPTION_TRANSITIONS: Record<SubscriptionStatus, readonly SubscriptionStatus[]> = {
  trial: ['active', 'cancelled'],
  active: ['past_due', 'cancelled'],
  past_due: ['active', 'suspended', 'cancelled'],
  suspended: ['active', 'cancelled'],
  cancelled: [],
}

export function isSubscriptionStatus(value: string): value is SubscriptionStatus {
  return (SUBSCRIPTION_STATUSES as readonly string[]).includes(value)
}

/* ------------------------------------------------------------------- reads */

export function getBillingPlans(): Promise<{ plans: BillingPlan[] }> {
  return api('/api/v1/billing/plans')
}

export function getBillingWallet(): Promise<{ wallet: BillingWallet; entitlement: BillingEntitlement | null }> {
  return api('/api/v1/billing/wallet')
}

export function getBillingSubscription(): Promise<{
  subscription: BillingSubscription | null
  entitlement: BillingEntitlement | null
}> {
  return api('/api/v1/billing/subscription')
}

/** Ledger pages by `limit` only — the backend exposes no cursor. */
export function listBillingLedger(limit: number): Promise<{ entries: BillingLedgerEntry[] }> {
  return api(`/api/v1/billing/ledger${queryString({ limit })}`)
}

export function listBillingInvoices(limit: number): Promise<{ invoices: BillingInvoice[] }> {
  return api(`/api/v1/billing/invoices${queryString({ limit })}`)
}

/* ------------------------------------------------------------------ writes */

export function subscribeBillingPlan(
  planId: string,
  trialDays = 0,
): Promise<{ subscription: BillingSubscription; entitlement: BillingEntitlement | null }> {
  return api('/api/v1/billing/subscribe', {
    method: 'POST',
    body: { plan_id: planId, trial_days: trialDays },
  })
}

export function transitionBillingSubscription(
  status: SubscriptionStatus,
): Promise<{ subscription: BillingSubscription; entitlement: BillingEntitlement | null }> {
  return api('/api/v1/billing/subscription', { method: 'PATCH', body: { status } })
}

export function topupBillingWallet(amountMicros: number): Promise<{ topup: BillingTopup; wallet: BillingWallet }> {
  return api('/api/v1/billing/topup', {
    method: 'POST',
    body: { amount_micros: amountMicros, channel: 'manual' },
  })
}

export interface BillingCheckoutSession {
  provider: string
  session_id: string
  checkout_url: string
  topup_id: string
  plan_id: string
  amount_micros: number
}

export function createBillingCheckout(planId: string): Promise<BillingCheckoutSession> {
  return api('/api/v1/billing/checkout', { method: 'POST', body: { plan_id: planId } })
}
