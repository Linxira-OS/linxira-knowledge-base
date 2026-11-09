import { useCallback, useEffect, useMemo, useState } from 'react'
import { useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ApiError, encodePathSegment } from '../../api/client'
import { getPage, isRevisionConflictError, savePage } from '../../api/wiki'
import { Badge, Banner, Button, Card, EmptyState, ErrorState, Field, LoadingState, SectionTitle } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { renderMarkdown } from '../../lib/markdown'
import { useUnsavedWarning } from '../../lib/hooks'

const SLUG_PATTERN = /^[^/\\]{1,200}$/

interface Draft {
  title: string
  markdown: string
  lang: string
  /** `If-Match` value loaded with the draft; `null` means “create with *”. */
  baseRevisionId: string | null
  exists: boolean
}

const EMPTY_DRAFT: Draft = { title: '', markdown: '', lang: 'zh', baseRevisionId: null, exists: false }

function EditorForm({ space, slug, creating }: { space: string; slug: string; creating: boolean }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { t } = useI18n()
  const [searchParams] = useSearchParams()
  const langParam = searchParams.get('lang')
  const [slugValue, setSlugValue] = useState(slug)
  const [draft, setDraft] = useState<Draft>(() => ({ ...EMPTY_DRAFT, lang: langParam ?? 'zh' }))
  const [loaded, setLoaded] = useState(false)
  const [conflict, setConflict] = useState(false)
  const [saved, setSaved] = useState(false)
  const [saveError, setSaveError] = useState<unknown>(null)

  const effectiveSlug = creating ? slugValue.trim() : slug
  const slugValid = effectiveSlug !== '' && SLUG_PATTERN.test(effectiveSlug)

  const load = useCallback(async (): Promise<Draft> => {
    if (!slugValid) {
      const draft: Draft = { ...EMPTY_DRAFT, lang: langParam ?? 'zh' }
      setDraft(draft)
      setLoaded(true)
      return draft
    }
    let next: Draft
    try {
      const page = await getPage(space, effectiveSlug, langParam)
      next = {
        title: page.title,
        markdown: page.markdown,
        lang: page.lang,
        baseRevisionId: page.current_revision_id,
        exists: true,
      }
    } catch (error) {
      if (error instanceof ApiError && error.status === 404) {
        next = { ...EMPTY_DRAFT, title: effectiveSlug, lang: langParam ?? 'zh', exists: false }
      } else {
        throw error
      }
    }
    setDraft(next)
    setLoaded(true)
    return next
  }, [space, effectiveSlug, langParam, slugValid])

  const sourceQuery = useQuery({
    queryKey: ['wiki', 'editor', space, effectiveSlug, langParam],
    queryFn: load,
    enabled: slugValid,
    retry: false,
  })

  // Refill the form whenever a new revision is loaded (initial load / reload).
  useEffect(() => {
    setLoaded(false)
    setConflict(false)
  }, [space, effectiveSlug])

  const [original, setOriginal] = useState<Draft | null>(null)
  useEffect(() => {
    if (loaded && original === null) setOriginal(draft)
  }, [loaded, original, draft])

  const dirty =
    original !== null &&
    (draft.title !== original.title ||
      draft.markdown !== original.markdown ||
      draft.lang !== original.lang ||
      effectiveSlug !== slug ||
      original.baseRevisionId !== draft.baseRevisionId)
  useUnsavedWarning(dirty)

  const previewHtml = useMemo(() => renderMarkdown(draft.markdown), [draft.markdown])

  const saveMutation = useMutation({
    mutationFn: (baseRevisionId: string | null) =>
      savePage({
        space,
        slug: effectiveSlug,
        title: draft.title.trim(),
        markdown: draft.markdown,
        lang: draft.lang,
        baseRevisionId,
      }),
    onSuccess: (result) => {
      setSaved(true)
      setConflict(false)
      setSaveError(null)
      void queryClient.invalidateQueries({ queryKey: ['wiki'] })
      void queryClient.invalidateQueries({ queryKey: ['pages'] })
      navigate(`/w/${encodePathSegment(space)}/${encodePathSegment(result.slug)}?lang=${encodePathSegment(result.lang)}`)
    },
    onError: (error) => {
      if (isRevisionConflictError(error)) {
        setConflict(true)
        setSaveError(null)
      } else {
        setSaveError(error)
      }
    },
  })

  const reload = useCallback(async () => {
    setOriginal(null)
    setSaved(false)
    setConflict(false)
    setSaveError(null)
    await sourceQuery.refetch()
  }, [sourceQuery])

  const overwrite = useCallback(async () => {
    // Fetch the newest revision id, then save with it — an explicit overwrite.
    try {
      const page = await getPage(space, effectiveSlug, langParam)
      saveMutation.mutate(page.current_revision_id)
    } catch (error) {
      if (error instanceof ApiError && error.status === 404) saveMutation.mutate('*')
      else setSaveError(error)
    }
  }, [space, effectiveSlug, langParam, saveMutation])

  const submit = () => {
    if (draft.title.trim() === '' || !slugValid) return
    saveMutation.mutate(draft.exists ? draft.baseRevisionId : null)
  }

  if (sourceQuery.isLoading && !loaded) {
    return (
      <div className="mx-auto w-full max-w-[1180px] px-6 py-5">
        <LoadingState label={t('wiki.editor.loading')} />
      </div>
    )
  }

  if (sourceQuery.isError) {
    return (
      <div className="mx-auto w-full max-w-[900px] px-6 py-5">
        <ErrorState error={sourceQuery.error} onRetry={() => void sourceQuery.refetch()} />
      </div>
    )
  }

  if (!creating && loaded && !draft.exists && sourceQuery.isFetched) {
    return (
      <div className="mx-auto w-full max-w-[900px] px-6 py-5">
        <EmptyState title={t('wiki.reader.notFound')} description={`${space}/${slug}`} />
      </div>
    )
  }

  return (
    <div className="mx-auto w-full max-w-[1180px] px-6 py-5">
      <SectionTitle
        actions={
          <>
            {dirty && <Badge tone="warn">{t('wiki.editor.dirty')}</Badge>}
            {dirty && (
              <Button variant="ghost" size="sm" onClick={() => original && setDraft(original)}>
                {t('wiki.editor.discard')}
              </Button>
            )}
            <Button variant="primary" onClick={submit} disabled={saveMutation.isPending || draft.title.trim() === '' || !slugValid}>
              {saveMutation.isPending ? t('action.saving') : t('action.save')}
            </Button>
          </>
        }
      >
        {creating ? t('wiki.editor.newPage') : t('wiki.editor.title')}
      </SectionTitle>

      <div className="mt-3 space-y-3">
        <Banner tone="neutral">{t('wiki.editor.optimistic')}</Banner>

        {draft.exists && <Banner tone={creating ? 'warn' : 'neutral'}>{t('wiki.editor.existsNotice')}</Banner>}

        {conflict && (
          <Banner
            tone="warn"
            title={t('wiki.editor.conflict')}
            actions={
              <>
                <Button size="sm" onClick={() => void reload()}>
                  {t('wiki.editor.reload')}
                </Button>
                <Button size="sm" variant="danger" onClick={() => void overwrite()}>
                  {t('wiki.editor.overwrite')}
                </Button>
              </>
            }
          >
            {t('wiki.editor.conflictHint')}
          </Banner>
        )}

        {saveError !== null && (
          <ErrorState
            error={saveError}
            onRetry={() => saveMutation.mutate(draft.exists ? draft.baseRevisionId : null)}
          />
        )}

        <div className="grid gap-3 sm:grid-cols-3">
          {creating && (
            <Field label={t('wiki.editor.slug')} hint={t('wiki.editor.slugHint')} error={!slugValid && slugValue !== '' ? t('wiki.editor.slugInvalid') : undefined}>
              <input
                value={slugValue}
                onChange={(event) => setSlugValue(event.target.value)}
                autoComplete="off"
                spellCheck={false}
                className="font-mono"
              />
            </Field>
          )}
          <Field label={t('wiki.editor.pageTitle')} error={draft.title.trim() === '' ? t('wiki.editor.titleRequired') : undefined}>
            <input value={draft.title} onChange={(event) => setDraft({ ...draft, title: event.target.value })} maxLength={200} />
          </Field>
          <Field label={t('wiki.editor.lang')}>
            <select value={draft.lang} onChange={(event) => setDraft({ ...draft, lang: event.target.value })}>
              <option value="zh">zh</option>
              <option value="en">en</option>
            </select>
          </Field>
        </div>

        <div className="grid gap-3 lg:grid-cols-2">
          <Card title={t('wiki.editor.markdown')} bodyClassName="p-2">
            <textarea
              value={draft.markdown}
              onChange={(event) => setDraft({ ...draft, markdown: event.target.value })}
              className="thin-scroll min-h-[380px] w-full resize-y border-0 bg-transparent p-2 font-mono text-[12.5px] leading-6"
              spellCheck={false}
            />
          </Card>
          <Card title={t('wiki.editor.preview')} bodyClassName="p-3">
            {draft.markdown.trim() === '' ? (
              <p className="text-[11.5px] text-fg-3">{t('wiki.editor.previewEmpty')}</p>
            ) : (
              <div className="prose-lkb max-h-[420px] overflow-y-auto" dangerouslySetInnerHTML={{ __html: previewHtml }} />
            )}
          </Card>
        </div>

        {saved && <Banner tone="accent">{t('wiki.editor.saved')}</Banner>}
      </div>
    </div>
  )
}

/** `/w/:space/:slug/edit` — edit an existing page. */
export function WikiEditorPage() {
  const { space = '', slug = '' } = useParams<{ space: string; slug: string }>()
  return <EditorForm space={space} slug={slug} creating={false} />
}

/** `/create/:space` — new page; `?slug=` pre-fills the identifier. */
export function WikiCreatePage() {
  const { space = '' } = useParams<{ space: string }>()
  const [searchParams] = useSearchParams()
  const slug = searchParams.get('slug') ?? ''
  return <EditorForm space={space} slug={slug} creating />
}
