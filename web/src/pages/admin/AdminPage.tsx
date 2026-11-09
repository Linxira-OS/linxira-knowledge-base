import { useEffect } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { Banner, SectionTitle, Tabs, type TabItem } from '../../components/ui'
import { AuditPanel } from '../../features/admin/AuditPanel'
import { BillingPanel } from '../../features/admin/BillingPanel'
import { ConnectorsPanel } from '../../features/admin/ConnectorsPanel'
import { GroupsPanel } from '../../features/admin/GroupsPanel'
import { ModelsPanel } from '../../features/admin/ModelsPanel'
import { NotProvidedCards } from '../../features/admin/NotProvidedCards'
import { OverviewPanel } from '../../features/admin/OverviewPanel'
import { QuotasPanel } from '../../features/admin/QuotasPanel'
import { RolesPanel } from '../../features/admin/RolesPanel'
import { SpacesPanel } from '../../features/admin/SpacesPanel'
import { UsagePanel } from '../../features/admin/UsagePanel'
import { UsersPanel } from '../../features/admin/UsersPanel'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'

const ADMIN_TABS = [
  'overview',
  'users',
  'roles',
  'groups',
  'spaces',
  'models',
  'usage',
  'quotas',
  'billing',
  'connectors',
  'audit',
] as const
type AdminTab = (typeof ADMIN_TABS)[number]

function isAdminTab(value: string | undefined): value is AdminTab {
  return value !== undefined && (ADMIN_TABS as readonly string[]).includes(value)
}

/** Deep-linkable admin console: `/admin/:tab?`, defaulting to the overview. */
export function AdminPage() {
  const { t } = useI18n()
  const session = useSession()
  const navigate = useNavigate()
  const params = useParams()
  const raw = params.tab ?? params['*']?.split('/')[0]
  const active: AdminTab = isAdminTab(raw) ? raw : 'overview'

  useEffect(() => {
    if (raw !== undefined && !isAdminTab(raw)) navigate('/admin/overview', { replace: true })
  }, [raw, navigate])

  // Admin endpoints require the tenant owner/admin role (solo's local principal is an owner).
  const role = session.principal?.tenant_role
  if (role !== 'owner' && role !== 'admin') {
    return (
      <div className="mx-auto w-full max-w-[900px] px-6 py-5">
        <SectionTitle>{t('admin.title')}</SectionTitle>
        <div className="mt-4">
          <Banner tone="warn" title={t('admin.roleRequired')}>
            {t('admin.roleHint')}
          </Banner>
        </div>
      </div>
    )
  }

  const items: TabItem<AdminTab>[] = [
    { id: 'overview', label: t('admin.tab.overview') },
    { id: 'users', label: t('admin.tab.users') },
    { id: 'roles', label: t('admin.tab.roles') },
    { id: 'groups', label: t('admin.tab.groups') },
    { id: 'spaces', label: t('admin.tab.spaces') },
    { id: 'models', label: t('admin.tab.models') },
    { id: 'usage', label: t('admin.tab.usage') },
    // Solo defaults quotas off. Keep the deep link working (read-only panel
    // with a tier banner) but remove the entry point from the tab strip.
    ...(session.features?.features.quotas === false ? [] : [{ id: 'quotas' as const, label: t('admin.tab.quotas') }]),
    { id: 'billing', label: t('admin.tab.billing') },
    { id: 'connectors', label: t('admin.tab.connectors') },
    { id: 'audit', label: t('admin.tab.audit') },
  ]
  const activeLabel =
    items.find((item) => item.id === active)?.label ?? (active === 'quotas' ? t('admin.tab.quotas') : active)

  return (
    <div className="mx-auto w-full max-w-[900px] px-6 py-5">
      <nav aria-label={t('admin.breadcrumb')} className="mb-1 flex items-center gap-1.5 text-[11px] text-fg-3">
        <Link to="/admin" className="transition-colors hover:text-fg">
          {t('nav.admin')}
        </Link>
        <span aria-hidden="true">/</span>
        <span className="text-fg-2">{activeLabel}</span>
      </nav>
      <SectionTitle>{t('admin.title')}</SectionTitle>
      <p className="mt-1 text-[12px] text-fg-3">{t('admin.subtitle')}</p>
      <div className="mt-4">
        <Tabs items={items} active={active} onChange={(id) => navigate(`/admin/${id}`)} />
      </div>
      <div className="mt-4 space-y-4">
        {active === 'overview' && <OverviewPanel />}
        {active === 'users' && <UsersPanel />}
        {active === 'roles' && <RolesPanel />}
        {active === 'groups' && <GroupsPanel />}
        {active === 'spaces' && <SpacesPanel />}
        {active === 'models' && <ModelsPanel />}
        {active === 'usage' && <UsagePanel />}
        {active === 'quotas' && <QuotasPanel />}
        {active === 'billing' && <BillingPanel />}
        {active === 'connectors' && <ConnectorsPanel />}
        {active === 'audit' && <AuditPanel />}
        <NotProvidedCards />
      </div>
    </div>
  )
}
