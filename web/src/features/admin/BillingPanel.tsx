import { useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import {
  SUBSCRIPTION_TRANSITIONS,
  createBillingCheckout,
  getBillingPlans,
  getBillingSubscription,
  getBillingWallet,
  isSubscriptionStatus,
  listBillingInvoices,
  listBillingLedger,
  subscribeBillingPlan,
  topupBillingWallet,
  transitionBillingSubscription,
  type BillingCheckoutSession,
  type BillingWallet,
  type SubscriptionStatus,
} from '../../api/billing'
import {
  Badge,
  Banner,
  Button,
  Card,
  EmptyState,
  ErrorState,
  KeyValue,
  Table,
  TableWrap,
  TBody,
  TD,
  TH,
  THead,
  TR,
  statusTone,
  type BadgeTone,
} from '../../components/ui'
import { asJsonObject } from '../../lib/json'
import { useI18n, type MessageKey } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import { AdminQueryGate, SelectInput, TextInput, adminKeys, useAdminInvalidate } from './shared'

/** Ledger/invoice page size; the backend ledger endpoint pages by `limit` only. */
const LEDGER_PAGE = 100

const STATUS_KEYS: Record<SubscriptionStatus, MessageKey> = {
  trial: 'admin.billing.status.trial',
  active: 'admin.billing.status.active',
  past_due: 'admin.billing.status.past_due',
  suspended: 'admin.billing.status.suspended',
  cancelled: 'admin.billing.status.cancelled',
}

const ACCOUNT_KEYS: Record<string, MessageKey> = {
  wallet: 'admin.billing.account.wallet',
  payment_clearing: 'admin.billing.account.payment_clearing',
  revenue: 'admin.billing.account.revenue',
}

const DIRECTION_KEYS: Record<string, MessageKey> = {
  credit: 'admin.billing.direction.credit',
  debit: 'admin.billing.direction.debit',
}

const KIND_KEYS: Record<string, MessageKey> = {
  topup: 'admin.billing.kind.topup',
  usage: 'admin.billing.kind.usage',
  topup_refund: 'admin.billing.kind.topup_refund',
  refund: 'admin.billing.kind.refund',
}

function subscriptionTone(status: SubscriptionStatus): BadgeTone {
  switch (status) {
    case 'active':
      return 'accent'
    case 'past_due':
      return 'warn'
    case 'suspended':
    case 'cancelled':
      return 'danger'
    case 'trial':
      return 'neutral'
  }
}

/** `metadata_json` is a JSON string on the wire; the ledger `kind` is best-effort. */
function entryKind(metadataJson: string): string | null {
  try {
    const kind = asJsonObject(JSON.parse(metadataJson))?.kind
    return typeof kind === 'string' ? kind : null
  } catch {
    return null
  }
}

/** Major units typed by a human → integer micros; `null` means invalid. */
function parseAmountMicros(text: string): number | null {
  const trimmed = text.trim()
  if (trimmed === '') return null
  const value = Number(trimmed)
  if (!Number.isFinite(value) || value <= 0) return null
  return Math.round(value * 1_000_000)
}

function parseTrialDays(text: string): number | null {
  const trimmed = text.trim()
  if (!/^\d{1,3}$/.test(trimmed)) return null
  return Number(trimmed)
}

/**
 * Billing console. The feature gate is client-side first: while
 * `features.billing` is off this renders a neutral notice and never calls the
 * billing endpoints (the backend would answer 403 `feature_disabled`).
 */
export function BillingPanel() {
  const { t } = useI18n()
  const session = useSession()

  if (session.features === null) {
    return (
      <Card title={t('admin.billing.title')} description={t('admin.billing.subtitle')}>
        <Banner tone="neutral" title={t('admin.billing.unknownTitle')}>
          {t('admin.billing.unknownBody')}
        </Banner>
      </Card>
    )
  }

  if (!session.features.features.billing) {
    return (
      <Card title={t('admin.billing.title')} description={t('admin.billing.subtitle')}>
        <Banner tone="neutral" title={t('admin.billing.disabledTitle')}>
          {t('admin.billing.disabledBody')}
        </Banner>
      </Card>
    )
  }

  const role = session.principal?.tenant_role
  return <BillingConsole isAdmin={role === 'owner' || role === 'admin'} />
}

function BillingConsole({ isAdmin }: { isAdmin: boolean }) {
  const { t } = useI18n()

  return (
    <div className="space-y-4">
      <PlansCard />
      <SubscriptionCard isAdmin={isAdmin} />
      <WalletCard isAdmin={isAdmin} />
      <CheckoutCard />
      <LedgerCard />
      <InvoicesCard />
      {!isAdmin && <p className="text-[11.5px] text-fg-3">{t('admin.billing.adminOnly')}</p>}
    </div>
  )
}

function PlansCard() {
  const { t, format } = useI18n()
  const query = useQuery({ queryKey: [...adminKeys.billing, 'plans'], queryFn: getBillingPlans })
  const plans = query.data?.plans ?? []

  return (
    <Card title={t('admin.billing.section.plans')}>
      <AdminQueryGate
        pending={query.isPending}
        error={query.error}
        onRetry={() => void query.refetch()}
        featureDisabledKey="admin.billing.disabled"
      >
        {plans.length === 0 ? (
          <EmptyState title={t('admin.billing.emptyPlans')} />
        ) : (
          <TableWrap>
            <Table>
              <THead>
                <TR>
                  <TH>{t('admin.billing.field.plan')}</TH>
                  <TH>{t('admin.billing.field.price')}</TH>
                  <TH>{t('admin.billing.field.includedTokens')}</TH>
                  <TH>{t('admin.billing.field.planPeriod')}</TH>
                  <TH>{t('admin.billing.field.status')}</TH>
                </TR>
              </THead>
              <TBody>
                {plans.map((plan) => (
                  <TR key={plan.plan_id}>
                    <TD>
                      <span className="font-medium text-fg">{plan.name}</span>
                      <span className="ml-1.5 font-mono text-[10.5px] text-fg-3">{plan.plan_id}</span>
                    </TD>
                    <TD>
                      {plan.price_micros === 0
                        ? t('admin.billing.planFree')
                        : format.currency(plan.price_micros, plan.currency)}
                    </TD>
                    <TD>{format.number(plan.included_tokens)}</TD>
                    <TD>{plan.period}</TD>
                    <TD>
                      <Badge tone={statusTone(plan.status)}>{plan.status}</Badge>
                    </TD>
                  </TR>
                ))}
              </TBody>
            </Table>
          </TableWrap>
        )}
      </AdminQueryGate>
    </Card>
  )
}

function SubscriptionCard({ isAdmin }: { isAdmin: boolean }) {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const query = useQuery({ queryKey: [...adminKeys.billing, 'subscription'], queryFn: getBillingSubscription })
  const plansQuery = useQuery({ queryKey: [...adminKeys.billing, 'plans'], queryFn: getBillingPlans })
  const [planId, setPlanId] = useState('')
  const [trialDays, setTrialDays] = useState('0')

  const subscription = query.data?.subscription ?? null
  const entitlement = query.data?.entitlement ?? null
  const status = subscription && isSubscriptionStatus(subscription.status) ? subscription.status : null
  const nextStatuses = status ? SUBSCRIPTION_TRANSITIONS[status] : []
  const plans = plansQuery.data?.plans ?? []
  const selectedPlanId = planId !== '' ? planId : (plans[0]?.plan_id ?? '')
  const trialParsed = parseTrialDays(trialDays)

  const subscribe = useMutation({
    mutationFn: () => subscribeBillingPlan(selectedPlanId, trialParsed ?? 0),
    onSuccess: () => invalidate(adminKeys.billing),
  })
  const transition = useMutation({
    mutationFn: transitionBillingSubscription,
    onSuccess: () => invalidate(adminKeys.billing),
  })

  return (
    <Card title={t('admin.billing.section.subscription')}>
      <AdminQueryGate
        pending={query.isPending}
        error={query.error}
        onRetry={() => void query.refetch()}
        featureDisabledKey="admin.billing.disabled"
      >
        <div className="space-y-3">
          {subscription === null ? (
            <p className="text-[12px] text-fg-3">{t('admin.billing.noSubscription')}</p>
          ) : (
            <KeyValue
              items={[
                [t('admin.billing.field.subscriptionId'), subscription.subscription_id],
                [t('admin.billing.field.plan'), subscription.plan_id],
                [
                  t('admin.billing.field.status'),
                  status ? (
                    <Badge tone={subscriptionTone(status)}>{t(STATUS_KEYS[status])}</Badge>
                  ) : (
                    subscription.status
                  ),
                ],
                [
                  t('admin.billing.field.period'),
                  `${format.dateTime(subscription.current_period_start)} → ${format.dateTime(subscription.current_period_end)}`,
                ],
                [
                  t('admin.billing.field.trialEndsAt'),
                  subscription.trial_ends_at ? format.dateTime(subscription.trial_ends_at) : '—',
                ],
                [t('admin.billing.field.updatedAt'), format.dateTime(subscription.updated_at)],
              ]}
            />
          )}
          {entitlement !== null && (
            <KeyValue
              items={[
                [t('admin.billing.field.includedTokens'), format.number(entitlement.included_tokens)],
                [t('admin.billing.field.usedTokens'), format.number(entitlement.used_tokens)],
                [
                  t('admin.billing.field.balance'),
                  format.currency(entitlement.balance_micros, entitlement.currency),
                ],
              ]}
            />
          )}

          {isAdmin ? (
            <div className="space-y-2">
              {subscription === null && (
                <div className="grid items-end gap-3 sm:grid-cols-3">
                  <SelectInput
                    label={t('admin.billing.subscribePlan')}
                    value={selectedPlanId}
                    onChange={setPlanId}
                    options={plans.map((plan) => ({ value: plan.plan_id, label: `${plan.name} (${plan.plan_id})` }))}
                  />
                  <TextInput
                    type="number"
                    label={t('admin.billing.trialDays')}
                    value={trialDays}
                    onChange={setTrialDays}
                    hint={t('admin.billing.trialDaysHint')}
                    error={trialParsed === null ? t('error.bad_request') : undefined}
                  />
                  <Button
                    variant="primary"
                    disabled={subscribe.isPending || selectedPlanId === '' || trialParsed === null}
                    onClick={() => {
                      if (selectedPlanId !== '' && trialParsed !== null) subscribe.mutate()
                    }}
                  >
                    {t('admin.billing.action.subscribe')}
                  </Button>
                </div>
              )}
              {nextStatuses.length > 0 && (
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-[11px] text-fg-3">{t('admin.billing.action.transition')}</span>
                  {nextStatuses.map((next) => (
                    <Button key={next} size="sm" disabled={transition.isPending} onClick={() => transition.mutate(next)}>
                      {t(STATUS_KEYS[next])}
                    </Button>
                  ))}
                </div>
              )}
              {subscribe.error != null && <ErrorState error={subscribe.error} compact />}
              {transition.error != null && <ErrorState error={transition.error} compact />}
            </div>
          ) : (
            <p className="text-[11.5px] text-fg-3">{t('admin.billing.adminOnly')}</p>
          )}
        </div>
      </AdminQueryGate>
    </Card>
  )
}

function WalletCard({ isAdmin }: { isAdmin: boolean }) {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const query = useQuery({ queryKey: [...adminKeys.billing, 'wallet'], queryFn: getBillingWallet })
  const [amount, setAmount] = useState('')
  const micros = parseAmountMicros(amount)
  const topup = useMutation({
    mutationFn: () => topupBillingWallet(micros ?? 0),
    onSuccess: () => {
      setAmount('')
      invalidate(adminKeys.billing)
    },
  })
  const wallet: BillingWallet | null = query.data?.wallet ?? null

  return (
    <Card title={t('admin.billing.section.wallet')}>
      <AdminQueryGate
        pending={query.isPending}
        error={query.error}
        onRetry={() => void query.refetch()}
        featureDisabledKey="admin.billing.disabled"
      >
        <div className="space-y-3">
          {wallet !== null && (
            <KeyValue
              items={[
                [t('admin.billing.field.walletId'), wallet.wallet_id],
                [t('admin.billing.field.currency'), wallet.currency],
                [t('admin.billing.field.status'), <Badge tone={statusTone(wallet.status)}>{wallet.status}</Badge>],
                [t('admin.billing.field.balance'), format.currency(wallet.balance_micros, wallet.currency)],
                [t('admin.billing.field.updatedAt'), format.dateTime(wallet.updated_at)],
              ]}
            />
          )}
          {isAdmin ? (
            <>
              <div className="grid items-end gap-3 sm:grid-cols-3">
                <TextInput
                  type="text"
                  label={t('admin.billing.topupAmount', { currency: wallet?.currency ?? 'USD' })}
                  value={amount}
                  onChange={setAmount}
                  hint={t('admin.billing.topupHint')}
                  error={amount.trim() !== '' && micros === null ? t('error.bad_request') : undefined}
                />
                <Button
                  variant="primary"
                  disabled={topup.isPending || micros === null}
                  onClick={() => {
                    if (micros !== null) topup.mutate()
                  }}
                >
                  {t('admin.billing.action.topup')}
                </Button>
              </div>
              {topup.isSuccess && <Banner tone="accent">{t('admin.billing.topupDone')}</Banner>}
              {topup.error != null && <ErrorState error={topup.error} compact />}
            </>
          ) : (
            <p className="text-[11.5px] text-fg-3">{t('admin.billing.adminOnly')}</p>
          )}
        </div>
      </AdminQueryGate>
    </Card>
  )
}

function CheckoutCard() {
  const { t, format } = useI18n()
  const invalidate = useAdminInvalidate()
  const plansQuery = useQuery({ queryKey: [...adminKeys.billing, 'plans'], queryFn: getBillingPlans })
  const [planId, setPlanId] = useState('')
  const [session, setSession] = useState<BillingCheckoutSession | null>(null)
  const checkout = useMutation({
    mutationFn: () => createBillingCheckout(selectedPlanId),
    onSuccess: (result) => {
      setSession(result)
      invalidate(adminKeys.billing)
    },
  })

  const plans = (plansQuery.data?.plans ?? []).filter((plan) => plan.price_micros > 0)
  const selectedPlanId = planId !== '' ? planId : (plans[0]?.plan_id ?? '')
  const sessionCurrency = session
    ? plans.find((plan) => plan.plan_id === session.plan_id)?.currency ?? 'USD'
    : 'USD'

  return (
    <Card title={t('admin.billing.section.checkout')} description={t('admin.billing.checkoutHint')}>
      <AdminQueryGate
        pending={plansQuery.isPending}
        error={plansQuery.error}
        onRetry={() => void plansQuery.refetch()}
        featureDisabledKey="admin.billing.disabled"
      >
        <div className="space-y-3">
          {plans.length === 0 ? (
            <p className="text-[11.5px] text-fg-3">{t('admin.billing.checkoutNoPaidPlans')}</p>
          ) : (
            <div className="grid items-end gap-3 sm:grid-cols-3">
              <SelectInput
                label={t('admin.billing.subscribePlan')}
                value={selectedPlanId}
                onChange={setPlanId}
                options={plans.map((plan) => ({ value: plan.plan_id, label: `${plan.name} (${plan.plan_id})` }))}
              />
              <Button
                variant="primary"
                disabled={checkout.isPending || selectedPlanId === ''}
                onClick={() => {
                  if (selectedPlanId !== '') checkout.mutate()
                }}
              >
                {t('admin.billing.action.checkout')}
              </Button>
            </div>
          )}
          {checkout.error != null && <ErrorState error={checkout.error} />}
          {session !== null && (
            <KeyValue
              items={[
                [t('admin.billing.field.provider'), session.provider],
                [t('admin.billing.field.sessionId'), session.session_id],
                [t('admin.billing.field.amount'), format.currency(session.amount_micros, sessionCurrency)],
                [t('admin.billing.field.topupId'), session.topup_id],
                [
                  t('admin.billing.field.checkoutUrl'),
                  <a
                    key="checkout-url"
                    className="break-all text-accent"
                    href={session.checkout_url}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {session.checkout_url}
                  </a>,
                ],
              ]}
            />
          )}
        </div>
      </AdminQueryGate>
    </Card>
  )
}

function LedgerCard() {
  const { t, format } = useI18n()
  const [limit, setLimit] = useState(LEDGER_PAGE)
  const query = useQuery({
    queryKey: [...adminKeys.billing, 'ledger', limit],
    queryFn: () => listBillingLedger(limit),
  })
  const entries = query.data?.entries ?? []

  return (
    <Card title={t('admin.billing.section.ledger')} description={t('admin.billing.ledgerHint')}>
      <AdminQueryGate
        pending={query.isPending}
        error={query.error}
        onRetry={() => void query.refetch()}
        featureDisabledKey="admin.billing.disabled"
      >
        {entries.length === 0 ? (
          <EmptyState title={t('admin.billing.emptyLedger')} />
        ) : (
          <>
            <TableWrap>
              <Table>
                <THead>
                  <TR>
                    <TH>{t('admin.billing.field.createdAt')}</TH>
                    <TH>{t('admin.billing.field.kind')}</TH>
                    <TH>{t('admin.billing.field.account')}</TH>
                    <TH>{t('admin.billing.field.direction')}</TH>
                    <TH>{t('admin.billing.field.amount')}</TH>
                    <TH>{t('admin.billing.field.idempotencyKey')}</TH>
                  </TR>
                </THead>
                <TBody>
                  {entries.map((entry) => {
                    const kind = entryKind(entry.metadata_json)
                    return (
                      <TR key={entry.entry_id}>
                        <TD>{format.dateTime(entry.created_at)}</TD>
                        <TD>{kind === null ? '—' : KIND_KEYS[kind] ? t(KIND_KEYS[kind]) : kind}</TD>
                        <TD>{ACCOUNT_KEYS[entry.account] ? t(ACCOUNT_KEYS[entry.account]) : entry.account}</TD>
                        <TD>
                          <Badge tone={entry.direction === 'credit' ? 'accent' : 'warn'}>
                            {DIRECTION_KEYS[entry.direction] ? t(DIRECTION_KEYS[entry.direction]) : entry.direction}
                          </Badge>
                        </TD>
                        <TD className="font-mono">{format.currency(entry.amount_micros, entry.currency)}</TD>
                        <TD className="font-mono text-[10.5px]">{entry.idempotency_key}</TD>
                      </TR>
                    )
                  })}
                </TBody>
              </Table>
            </TableWrap>
            {entries.length >= limit && (
              <div className="pt-2">
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={query.isFetching}
                  onClick={() => setLimit((current) => current + LEDGER_PAGE)}
                >
                  {t('paging.more')}
                </Button>
              </div>
            )}
          </>
        )}
      </AdminQueryGate>
    </Card>
  )
}

function InvoicesCard() {
  const { t, format } = useI18n()
  const query = useQuery({
    queryKey: [...adminKeys.billing, 'invoices'],
    queryFn: () => listBillingInvoices(LEDGER_PAGE),
  })
  const invoices = query.data?.invoices ?? []

  return (
    <Card title={t('admin.billing.section.invoices')}>
      <AdminQueryGate
        pending={query.isPending}
        error={query.error}
        onRetry={() => void query.refetch()}
        featureDisabledKey="admin.billing.disabled"
      >
        {invoices.length === 0 ? (
          <EmptyState title={t('admin.billing.emptyInvoices')} />
        ) : (
          <TableWrap>
            <Table>
              <THead>
                <TR>
                  <TH>{t('admin.billing.field.invoiceId')}</TH>
                  <TH>{t('admin.billing.field.period')}</TH>
                  <TH>{t('admin.billing.field.amount')}</TH>
                  <TH>{t('admin.billing.field.status')}</TH>
                  <TH>{t('admin.billing.field.createdAt')}</TH>
                </TR>
              </THead>
              <TBody>
                {invoices.map((invoice) => (
                  <TR key={invoice.invoice_id}>
                    <TD className="font-mono text-[11px]">{invoice.invoice_id}</TD>
                    <TD>
                      {format.dateTime(invoice.period_start)} → {format.dateTime(invoice.period_end)}
                    </TD>
                    <TD className="font-mono">{format.currency(invoice.amount_micros, invoice.currency)}</TD>
                    <TD>
                      <Badge tone={statusTone(invoice.status)}>{invoice.status}</Badge>
                    </TD>
                    <TD>{format.dateTime(invoice.created_at)}</TD>
                  </TR>
                ))}
              </TBody>
            </Table>
          </TableWrap>
        )}
      </AdminQueryGate>
    </Card>
  )
}
