import { Fragment, useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  bindExternalSession,
  decodeMessage,
  deleteConversation,
  getConversation,
  listConversations,
  restoreConversation,
} from '../api/query'
import {
  Badge,
  Banner,
  Button,
  EmptyState,
  ErrorState,
  Field,
  LoadingState,
  Pager,
  SectionTitle,
  Table,
  TableWrap,
  TBody,
  TD,
  TH,
  THead,
  TR,
  statusTone,
} from '../components/ui'
import { modeLabelKey, useQueryState } from '../features/query/QueryProvider'
import { useDebouncedValue } from '../lib/hooks'
import { useI18n } from '../lib/i18n'

const inputClass =
  'rounded-lg border border-line bg-canvas px-2.5 py-[5px] text-[12px] text-fg placeholder:text-fg-3'

const COLUMN_COUNT = 7

type Notice = { kind: 'deleted' } | { kind: 'restored' } | { kind: 'bound'; id: string }

/**
 * Conversation history backed by `/api/v1/conversations`: cursor pagination
 * via `next_cursor`, an external_session filter, and the only write
 * operations the backend exposes (tombstone DELETE, external-session bind).
 */
export function ConversationsPage() {
  const { t, format } = useI18n()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { openConversation, readableSpaces, spaces, spacesError, spacesLoading } = useQueryState()

  const [filter, setFilter] = useState('')
  const externalFilter = useDebouncedValue(filter, 350).trim()

  const [notice, setNotice] = useState<Notice | null>(null)
  const [confirmingId, setConfirmingId] = useState<string | null>(null)
  const [bindingId, setBindingId] = useState<string | null>(null)
  const [bindValue, setBindValue] = useState('')
  const [expandedId, setExpandedId] = useState<string | null>(null)

  const listQuery = useInfiniteQuery({
    queryKey: ['conversations', 'list', { external_session_id: externalFilter || null }],
    queryFn: ({ pageParam }) =>
      listConversations({
        limit: 20,
        ...(pageParam ? { cursor: pageParam } : {}),
        ...(externalFilter ? { external_session_id: externalFilter } : {}),
      }),
    initialPageParam: null as string | null,
    getNextPageParam: (lastPage) => lastPage.next_cursor || undefined,
  })

  const conversations = listQuery.data?.pages.flatMap((page) => page.conversations) ?? []

  const deleteMutation = useMutation({
    mutationFn: (conversationId: string) => deleteConversation(conversationId),
    onSuccess: (_result, conversationId) => {
      setConfirmingId(null)
      setNotice({ kind: 'deleted' })
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
      void queryClient.invalidateQueries({ queryKey: ['conversation', conversationId] })
    },
  })

  const restoreMutation = useMutation({
    mutationFn: (conversationId: string) => restoreConversation(conversationId),
    onSuccess: (_result, conversationId) => {
      setNotice({ kind: 'restored' })
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
      void queryClient.invalidateQueries({ queryKey: ['conversation', conversationId] })
    },
  })

  const bindMutation = useMutation({
    mutationFn: (input: { conversationId: string; externalSessionId: string }) =>
      bindExternalSession(input.conversationId, input.externalSessionId),
    onSuccess: (result) => {
      setBindingId(null)
      setBindValue('')
      setNotice({ kind: 'bound', id: result.external_session_id })
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
    },
  })

  const resume = (conversationId: string) => {
    void openConversation(conversationId)
    navigate(`/search?conversation=${encodeURIComponent(conversationId)}`)
  }

  const openInSearch = (conversationId: string) => {
    navigate(`/search?conversation=${encodeURIComponent(conversationId)}`)
  }

  const submitBind = (event: FormEvent<HTMLFormElement>, conversationId: string) => {
    event.preventDefault()
    const value = bindValue.trim()
    if (!value) return
    bindMutation.mutate({ conversationId, externalSessionId: value })
  }

  const spacesReady = !spacesLoading && !spacesError && spaces.length > 0

  return (
    <div className="mx-auto w-full max-w-[900px] px-6 py-5">
      <SectionTitle
        actions={
          <input
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder={t('conversations.filterExternal')}
            aria-label={t('conversations.filterExternal')}
            className={`${inputClass} w-[240px]`}
          />
        }
      >
        {t('conversations.title')}
      </SectionTitle>
      <p className="mt-1 text-[12px] text-fg-3">{t('conversations.subtitle')}</p>

      <div className="mt-4 space-y-3">
        {notice !== null && (
          <Banner
            tone="accent"
            actions={
              <Button size="sm" variant="ghost" onClick={() => setNotice(null)}>
                {t('action.close')}
              </Button>
            }
          >
            {notice.kind === 'deleted'
              ? t('conversations.deleted')
              : notice.kind === 'restored'
                ? t('conversations.restored')
                : t('conversations.bound', { id: notice.id })}
          </Banner>
        )}

        {listQuery.isLoading ? (
          <LoadingState />
        ) : conversations.length === 0 && listQuery.error ? (
          <ErrorState error={listQuery.error} onRetry={() => void listQuery.refetch()} />
        ) : conversations.length === 0 ? (
          <EmptyState title={t('conversations.empty')} />
        ) : (
          <>
            <TableWrap>
              <Table>
                <THead>
                  <TR>
                    <TH>{t('conversations.titleColumn')}</TH>
                    <TH>{t('conversations.space')}</TH>
                    <TH>{t('conversations.status')}</TH>
                    <TH>{t('conversations.externalSession')}</TH>
                    <TH>{t('conversations.updatedAt')}</TH>
                    <TH>{t('conversations.createdAt')}</TH>
                    <TH>{t('label.actions')}</TH>
                  </TR>
                </THead>
                <TBody>
                  {conversations.map((conversation) => {
                    const id = conversation.conversation_id
                    const title = conversation.title || id
                    const space = readableSpaces.find((entry) => entry.space_id === conversation.space_id)
                    const spaceMissing = spacesReady && !space
                    const deleting = deleteMutation.isPending && deleteMutation.variables === id
                    const binding = bindMutation.isPending && bindMutation.variables?.conversationId === id
                    return (
                      <Fragment key={id}>
                        <TR>
                          <TD className="px-3 py-2">
                            <Link
                              to={`/search?conversation=${encodeURIComponent(id)}`}
                              title={title}
                              className="block max-w-[240px] truncate font-medium text-fg hover:text-accent"
                            >
                              {title}
                            </Link>
                            <span className="mt-0.5 block truncate font-mono text-[10px] text-fg-3">{id}</span>
                          </TD>
                          <TD className="px-3 py-2">
                            {spaceMissing ? (
                              <Badge tone="warn">{t('conversations.noSpace')}</Badge>
                            ) : space ? (
                              <span className="text-[11.5px]" title={conversation.space_id}>
                                {space.name}
                              </span>
                            ) : (
                              <span className="font-mono text-[11px]">{conversation.space_id}</span>
                            )}
                          </TD>
                          <TD className="px-3 py-2">
                            <Badge tone={statusTone(conversation.status)}>
                              {conversation.status === 'deleted'
                                ? t('conversations.status.deleted')
                                : t('conversations.status.active')}
                            </Badge>
                          </TD>
                          <TD className="px-3 py-2 font-mono text-[11px]">
                            {conversation.external_session_id ?? (
                              <span className="font-sans text-fg-3">{t('conversations.externalSessionNone')}</span>
                            )}
                          </TD>
                          <TD className="px-3 py-2 text-[11.5px]" title={format.dateTime(conversation.updated_at)}>
                            {format.relative(conversation.updated_at)}
                          </TD>
                          <TD className="px-3 py-2 text-[11.5px]">{format.dateTime(conversation.created_at)}</TD>
                          <TD className="px-3 py-2">
                            <div className="flex flex-wrap items-center gap-1 whitespace-nowrap">
                              <Button size="sm" variant="primary" onClick={() => resume(id)}>
                                {t('conversations.resume')}
                              </Button>
                              <Button size="sm" onClick={() => openInSearch(id)}>
                                {t('conversations.open')}
                              </Button>
                              <Button
                                size="sm"
                                variant="ghost"
                                aria-expanded={expandedId === id}
                                onClick={() => setExpandedId(expandedId === id ? null : id)}
                              >
                                {t('conversations.messages')}
                              </Button>
                              <Button
                                size="sm"
                                variant="ghost"
                                onClick={() => {
                                  setBindingId(bindingId === id ? null : id)
                                  setBindValue('')
                                }}
                              >
                                {t('conversations.bindExternal')}
                              </Button>
                              {conversation.status === 'deleted' ? (
                                <Button
                                  size="sm"
                                  onClick={() => restoreMutation.mutate(id)}
                                  disabled={restoreMutation.isPending && restoreMutation.variables === id}
                                >
                                  {t('conversations.restore')}
                                </Button>
                              ) : (
                                <Button
                                  size="sm"
                                  variant="danger"
                                  onClick={() => setConfirmingId(confirmingId === id ? null : id)}
                                >
                                  {t('conversations.delete')}
                                </Button>
                              )}
                            </div>
                          </TD>
                        </TR>

                        {confirmingId === id && (
                          <TR className="bg-panel">
                            <TD colSpan={COLUMN_COUNT}>
                              <div className="flex flex-wrap items-center gap-2">
                                <span className="text-[11.5px] text-fg-2">
                                  {t('conversations.deleteConfirm', { title })}
                                </span>
                                <Button size="sm" variant="danger" disabled={deleting} onClick={() => deleteMutation.mutate(id)}>
                                  {t('action.confirm')}
                                </Button>
                                <Button size="sm" variant="ghost" onClick={() => setConfirmingId(null)}>
                                  {t('action.cancel')}
                                </Button>
                              </div>
                              {deleteMutation.isError && deleteMutation.variables === id && (
                                <div className="mt-2">
                                  <ErrorState error={deleteMutation.error} compact />
                                </div>
                              )}
                            </TD>
                          </TR>
                        )}

                        {bindingId === id && (
                          <TR className="bg-panel">
                            <TD colSpan={COLUMN_COUNT}>
                              <form className="flex flex-wrap items-end gap-2" onSubmit={(event) => submitBind(event, id)}>
                                <Field
                                  label={t('conversations.bindExternal')}
                                  hint={t('conversations.bindExternalHint')}
                                  htmlFor={`bind-${id}`}
                                >
                                  <input
                                    id={`bind-${id}`}
                                    value={bindValue}
                                    onChange={(event) => setBindValue(event.target.value)}
                                    placeholder={t('conversations.externalSession')}
                                    className={`${inputClass} w-[280px]`}
                                  />
                                </Field>
                                <Button
                                  size="sm"
                                  variant="primary"
                                  type="submit"
                                  disabled={bindValue.trim().length === 0 || binding}
                                >
                                  {t('conversations.bind')}
                                </Button>
                                <Button size="sm" variant="ghost" onClick={() => setBindingId(null)}>
                                  {t('action.cancel')}
                                </Button>
                              </form>
                              {bindMutation.isError && bindMutation.variables?.conversationId === id && (
                                <div className="mt-2">
                                  <ErrorState error={bindMutation.error} compact />
                                </div>
                              )}
                            </TD>
                          </TR>
                        )}

                        {expandedId === id && (
                          <TR className="bg-panel">
                            <TD colSpan={COLUMN_COUNT}>
                              <ConversationTranscript conversationId={id} />
                            </TD>
                          </TR>
                        )}
                      </Fragment>
                    )
                  })}
                </TBody>
              </Table>
            </TableWrap>

            {listQuery.error ? (
              <ErrorState
                error={listQuery.error}
                compact
                onRetry={() =>
                  listQuery.isFetchNextPageError ? void listQuery.fetchNextPage() : void listQuery.refetch()
                }
              />
            ) : null}

            <Pager
              hasMore={listQuery.hasNextPage}
              loading={listQuery.isFetchingNextPage}
              onNext={() => void listQuery.fetchNextPage()}
              loaded={conversations.length}
            />
          </>
        )}
      </div>
    </div>
  )
}

/** Inline transcript from `GET /api/v1/conversations/{id}`. */
function ConversationTranscript({ conversationId }: { conversationId: string }) {
  const { t, format } = useI18n()
  const transcript = useQuery({
    queryKey: ['conversation', conversationId],
    queryFn: () => getConversation(conversationId),
    retry: false,
  })

  if (transcript.isLoading) return <LoadingState />
  if (transcript.error) return <ErrorState error={transcript.error} onRetry={() => void transcript.refetch()} />

  const messages = (transcript.data?.messages ?? []).map(decodeMessage)
  if (messages.length === 0) return <p className="text-[11.5px] text-fg-3">{t('conversations.messagesEmpty')}</p>

  return (
    <ul className="divide-y divide-line">
      {messages.map(({ message, citations }) => (
        <li key={message.message_id} className="py-2 first:pt-0 last:pb-0">
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone={message.role === 'assistant' ? 'accent' : 'neutral'}>
              {message.role === 'user'
                ? t('conversations.role.user')
                : message.role === 'assistant'
                  ? t('conversations.role.assistant')
                  : message.role}
            </Badge>
            <span className="text-[10.5px] text-fg-3">{format.dateTime(message.created_at)}</span>
            <span className="text-[10.5px] text-fg-3">
              {t('conversations.mode')}:{' '}
              {message.mode === 'search' || message.mode === 'answer' || message.mode === 'both'
                ? t(modeLabelKey(message.mode))
                : message.mode}
            </span>
            {citations.length > 0 && (
              <Badge tone="neutral">{t('query.citationsCount', { count: citations.length })}</Badge>
            )}
          </div>
          <p className="mt-1 whitespace-pre-wrap text-[12px] leading-5 text-fg-2">{message.content}</p>
          <p className="mt-0.5 font-mono text-[10px] text-fg-3">
            {t('conversations.requestId')}: {message.request_id}
          </p>
          {message.rewritten_query && (
            <p className="mt-0.5 text-[10.5px] text-fg-3">
              {t('conversations.rewritten')}: <span className="text-fg-2">{message.rewritten_query}</span>
            </p>
          )}
        </li>
      ))}
    </ul>
  )
}
