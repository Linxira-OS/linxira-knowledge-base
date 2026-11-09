import { useMemo, useState } from 'react'
import { NavLink } from 'react-router-dom'
import { useInfiniteQuery } from '@tanstack/react-query'
import { encodePathSegment } from '../../api/client'
import { listPages, type PageListEntry } from '../../api/wiki'
import { Button, ErrorState, cx } from '../../components/ui'
import { useQueryState } from '../../features/query/QueryProvider'
import { useDebouncedValue } from '../../lib/hooks'
import { useI18n } from '../../lib/i18n'

/**
 * Sidebar page tree backed by `GET /api/v1/pages` (space-scoped, cursor
 * paginated, `q` filters slug/title on the server). Entries are grouped by
 * content language; the space picker lives in the top bar.
 */
export function PageTree() {
  const { t, format } = useI18n()
  const { spaceId, currentSpace } = useQueryState()
  const [filter, setFilter] = useState('')
  const debouncedFilter = useDebouncedValue(filter.trim(), 300)

  const pagesQuery = useInfiniteQuery({
    queryKey: ['pages', spaceId, debouncedFilter],
    queryFn: ({ pageParam }) =>
      listPages({
        space: spaceId as string,
        q: debouncedFilter.length >= 2 ? debouncedFilter : undefined,
        limit: 100,
        cursor: pageParam ?? null,
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.next_cursor,
    enabled: spaceId !== null,
  })

  const entries = useMemo<PageListEntry[]>(
    () => (pagesQuery.data?.pages ?? []).flatMap((page) => page.pages),
    [pagesQuery.data],
  )

  const byLang = useMemo(() => {
    const grouped = new Map<string, PageListEntry[]>()
    for (const entry of entries) {
      const bucket = grouped.get(entry.lang)
      if (bucket) bucket.push(entry)
      else grouped.set(entry.lang, [entry])
    }
    return [...grouped.entries()].sort(([a], [b]) => a.localeCompare(b))
  }, [entries])

  return (
    <div>
      <div className="flex items-center justify-between px-2 pb-1">
        <span className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-fg-3">{t('wiki.tree.title')}</span>
        {spaceId && (
          <NavLink
            to={`/create/${encodePathSegment(spaceId)}`}
            className="rounded-md border border-line px-1.5 py-[2px] text-[10px] text-fg-3 transition-colors hover:text-fg"
          >
            {t('wiki.tree.new')}
          </NavLink>
        )}
      </div>

      <div className="px-2 pb-1.5">
        <input
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
          placeholder={t('wiki.tree.filter')}
          aria-label={t('wiki.tree.filter')}
          className="py-[3px] text-[11.5px]"
        />
      </div>

      {currentSpace && (
        <div className="px-2 pb-1 text-[10.5px] text-fg-3">
          {currentSpace.name} · {t('wiki.tree.count', { count: format.number(entries.length) })}
        </div>
      )}

      {pagesQuery.isError && (
        <div className="px-2 pb-2">
          <ErrorState error={pagesQuery.error} compact onRetry={() => void pagesQuery.refetch()} />
        </div>
      )}

      {!pagesQuery.isLoading && !pagesQuery.isError && entries.length === 0 && (
        <p className="px-2 py-1 text-[11px] leading-5 text-fg-3">
          {debouncedFilter.length >= 2 ? t('state.empty') : t('wiki.tree.empty')}
        </p>
      )}

      <div className="space-y-3">
        {byLang.map(([lang, pages]) => (
          <div key={lang}>
            <div className="px-2 pb-0.5 text-[10px] uppercase tracking-[0.1em] text-fg-3">{lang}</div>
            <ul className="space-y-0.5">
              {pages.map((page) => (
                <li key={`${page.lang}-${page.slug}`}>
                  <NavLink
                    to={`/w/${encodePathSegment(spaceId ?? '')}/${encodePathSegment(page.slug)}?lang=${encodePathSegment(page.lang)}`}
                    className={({ isActive }) =>
                      cx(
                        'block truncate rounded-lg px-2.5 py-[6px] text-[12.5px] transition-colors',
                        isActive ? 'bg-accent-weak font-medium text-accent' : 'text-fg-2 hover:bg-raise hover:text-fg',
                      )
                    }
                    title={page.summary || page.title}
                  >
                    {page.title || page.slug}
                    <span className="ml-1.5 align-middle font-mono text-[9.5px] text-fg-3">v{page.version}</span>
                  </NavLink>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>

      {pagesQuery.hasNextPage && (
        <div className="px-2 pt-2">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => void pagesQuery.fetchNextPage()}
            disabled={pagesQuery.isFetchingNextPage}
          >
            {t('paging.more')}
          </Button>
        </div>
      )}
    </div>
  )
}
