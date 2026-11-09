import { Card, NotProvided } from '../../components/ui'
import { useI18n } from '../../lib/i18n'

/**
 * Capabilities the backend does not expose. Rendered instead of fabricating
 * panels, per ADR 0004 (explicit degradation).
 */
export function NotProvidedCards() {
  const { t } = useI18n()
  return (
    <Card title={t('admin.notProvided.title')}>
      <div className="grid gap-3 sm:grid-cols-2">
        <NotProvided title={t('admin.notProvided.sso')} body={t('admin.notProvided.ssoBody')} code="sso" />
        <NotProvided
          title={t('admin.notProvided.publicShare')}
          body={t('admin.notProvided.publicShareBody')}
          code="public-share"
        />
        <NotProvided
          title={t('admin.notProvided.tenantSettings')}
          body={t('admin.notProvided.tenantSettingsBody')}
          code="tenant-settings"
        />
      </div>
    </Card>
  )
}
