---
title: Built-in Wiki
slug: 04-wiki
lang: en
order: 4
summary: Page creation, editing, revision history, concurrent-save behavior, permissions, and the language boundary of the built-in wiki.
---

# Built-in Wiki

The built-in wiki does four things: spaces, pages, revisions, and per-language pages. It has no rich-text collaboration, comments, approvals, or plugins. Saved content is compiled into searchable documents, so "write it down" and "it can be found" are one pipeline.

## 1. Entry Points

Two front ends read and write the same data: server-rendered HTML pages and the JSON API used by the SPA.

| Entry | Path | Role/permission |
| --- | --- | --- |
| HTML page list | `GET /` | Signed in (no auth in solo) |
| HTML reader | `GET /w/{space}/{slug}` | Space `viewer` |
| HTML edit form | `GET /w/{space}/{slug}/edit` | Space `editor`; space must be `active` |
| HTML save | `POST /w/{space}/{slug}` (form `csrf_token`) | Space `editor`; **no If-Match** |
| HTML history | `GET /w/{space}/{slug}/history` | Space `viewer` |
| JSON read/save/delete | `GET|PUT|POST|DELETE /api/v1/pages/{space}/{slug}` | `editor` (`space_owner` for delete) |
| JSON history/single revision | `GET .../history?include_markdown=true`, `GET .../revisions/{revision_id}` | Read access |
| JSON restore | `POST .../restore` | `space_owner` |
| Page list | `GET /api/v1/pages?space&lang&q&limit&cursor` | Read access |
| Static summary | `GET .../summary?lang=` | Space `viewer` |
| SPA routes | `/w/:space/:slug`, `/edit`, `/history`, `/create/:space` | Guarded by the frontend `RequireAuth` |

Page identity is `(tenant, space, lang, slug)`: the same slug under Chinese and English is two pages, and citations resolve per language.

## 2. Creating a Page

In the SPA open `/create/{space}?slug=<slug>&lang=<zh|en>`, or go directly to `/w/{space}/new-page/edit`. Slug rules: non-empty, at most 200 bytes, no `/` or `\`, and not `.` or `..`. The title must not be empty, and JSON saves also require non-empty `markdown`.

The first save creates a Page and a revision at version 1. Built-in documents in `space=product` are written by `lkb seed` with author `system:seed`; once a user edits a page, the seed never overwrites it.

## 3. Editing and Saving

Saving increments the version, writes a new immutable Revision, points the page at it, and recompiles Document/Chunk under `wiki/{space}/{lang}/{slug}.md`. Index or quota failures return an error; quota is reserved before the save, recomputed after, and released on failure. Writing to an archived or soft-deleted space returns 409 `space_not_active`.

- **HTML form path**: only `csrf_token`, `lang`, `title`, and `markdown`; no version precondition, so the later writer wins (last-write-wins).
- **JSON API**: `PUT`/`POST` must carry `If-Match` whose value is the current `current_revision_id`.

## 4. History, Single Revisions, and Restore

- `GET .../history` returns the version list (version, author, time, `content_hash`, language); `include_markdown=true` also returns each revision's Markdown.
- `GET .../revisions/{revision_id}` returns one revision's full body.
- `DELETE /api/v1/pages/{space}/{slug}?lang=` archives the page (sets `valid_to` and hides the indexed document) and requires `space_owner`.
- `POST .../restore` restores an archived page and also requires `space_owner`.

History is read-only: restore changes the visible state without rewriting old revisions.

## 5. Concurrent Saves and If-Match

The JSON write protocol:

1. `GET /api/v1/pages/{space}/{slug}` to obtain `current_revision_id` (`rev_` plus 32 hex characters) and `version`.
2. Send `PUT` (or `POST`) with `If-Match: <current_revision_id>`.
3. Missing `If-Match` → **400 `bad_request`** ("If-Match header is required"); `If-Match: *` explicitly skips the check.
4. Mismatch → **409 `revision_conflict`**; the client should re-read and retry.

No `ETag` header is emitted; the version identifier travels in the body as `current_revision_id`. The HTML form path has no such check and remains last-write-wins, so prefer the JSON API or the SPA editor when several people edit one page.

## 6. Permissions and Visibility

- Reads need at least space `viewer`; writes need `editor`; delete/archive/restore need `space_owner`.
- Space membership and document ACL intersect: both must pass.
- A page's document ACL is currently an explicit `allow_all`; the effective permission is the space role (at least `viewer` to read, `editor` to write). Imported documents may still carry real ACLs, and the two gates intersect.
- An `archived` space is read-only; a soft-deleted space disappears from lists.
- Unreadable pages return 404; if a page exists but is outside your ACL, a write returns 403, avoiding existence disclosure.

## 7. Languages and Switching

Content `lang` is part of page identity and a recall-SQL filter (`pages.lang`, `documents.lang`, `chunks.lang`, `events.lang` are all indexed). Interface language is separate from content language:

- **Interface language**: the SPA LocaleToggle switches between `zh-CN` and `en`, stored in `localStorage['lkb-locale']`, and drives the API `Accept-Language` header.
- **Content language**: the TopBar language selector stores `localStorage['lkb-content-lang']` and sends it as the `lang` parameter for search/ask; wiki reader/edit/history pages take `?lang=`.
- Seed content is imported per language from `docs/zh` and `docs/en` with aligned slugs; open the English page or use `?lang=en`.
- CLI: `lkb page list --lang zh`, `lkb page show <space> <slug> --lang en`.

## 8. CLI Operations

```bash
./target/release/lkb page list --space product --lang zh
./target/release/lkb page show product 01-overview --lang zh
```

On Windows PowerShell, use `.\target\release\lkb.exe`. team and enterprise require `--token` or `LKB_API_TOKEN`.

## 9. Related Documents

- [02 Core Concepts](02-concepts.md)
- [05 Search and AI Answers](05-search-ai.md)
- [06 Authentication and Security](06-auth-security.md)
