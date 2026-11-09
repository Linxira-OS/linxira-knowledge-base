import { useEffect } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { SectionTitle, Tabs, type TabItem } from '../../components/ui'
import { AuditPanel } from '../../features/admin/AuditPanel'
import { BillingPanel } from '../../features/admin/BillingPanel'
import { ConnectorsPanel } from '../../features/admin/ConnectorsPanel'
import { GroupsPanel } from '../../features/admin/GroupsPanel'
import { ModelsPanel } from '../../features/admin/ModelsPanel'
import { NotProvidedCards } from '../../features/admin/NotProvidedCards'
import { QuotasPanel } from '../../features/admin/QuotasPanel'
import { RolesPanel } from '../../features/admin/RolesPanel'
import { SpacesPanel } from '../../features/admin/SpacesPanel'
import { UsagePanel } from '../../features/admin/UsagePanel'
import { UsersPanel } from '../../features/admin/UsersPanel'
import { useI18n } from '../../lib/i18n'

const ADMIN_TABS = ['users', 'roles', 'groups', 'spaces', 'models', 'usage', 'quotas', 'billing', 'connectors', 'audit'] as const
type AdminTab = (typeof ADMIN_TABS)[number]

function isAdminTab(value: string | undefined): value is AdminTab {
  return value !== undefined && (ADMIN_TABS as readonly string[]).includes(value)
}

/** Deep-linkable admin console: `/admin/:tab?`, defaulting to users. */
export function AdminPage() {
  const { t } = useI18n()
  const navigate = useNavigate()
  const params = useParams()
  const raw = params.tab ?? params['*']?.split('/')[0]
  const active: AdminTab = isAdminTab(raw) ? raw : 'users'

  useEffect(() => {
    if (raw !== undefined && !isAdminTab(raw)) navigate('/admin/users', { replace: true })
  }, [raw, navigate])

  const items: TabItem<AdminTab>[] = [
    { id: 'users', label: t('admin.tab.users') },
    { id: 'roles', label: t('admin.tab.roles') },
    { id: 'groups', label: t('admin.tab.groups') },
    { id: 'spaces', label: t('admin.tab.spaces') },
    { id: 'models', label: t('admin.tab.models') },
    { id: 'usage', label: t('admin.tab.usage') },
    { id: 'quotas', label: t('admin.tab.quotas') },
    { id: 'billing', label: t('admin.tab.billing') },
    { id: 'connectors', label: t('admin.tab.connectors') },
    { id: 'audit', label: t('admin.tab.audit') },
  ]

  return (
    <div className="mx-auto w-full max-w-[900px] px-6 py-5">
      <SectionTitle>{t('admin.title')}</SectionTitle>
      <p className="mt-1 text-[12px] text-fg-3">{t('admin.subtitle')}</p>
      <div className="mt-4">
        <Tabs items={items} active={active} onChange={(id) => navigate(`/admin/${id}`)} />
      </div>
      <div className="mt-4 space-y-4">
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
