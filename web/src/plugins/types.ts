/**
 * Plugin API v0 — types only.
 *
 * v0 is deliberately a front-end registration skeleton: plugins are compiled
 * into the bundle and registered in-process. The host never fetches, evaluates
 * or imports remote JavaScript, and plugin output is sanitised before it reaches
 * the DOM. See `document/plugin-api.md` for the full contract.
 */

export const PLUGIN_API_VERSION = '0.1.0'

export interface PluginEngines {
  /** Semver range of the host plugin API this plugin supports, e.g. `^0.1.0`. */
  api: string
  /** Optional minimum application version, informational in v0. */
  app?: string
}

export type PluginPermission =
  | 'note:read'
  | 'note:write'
  | 'block:render'
  | 'command:register'
  | 'settings:read'
  | 'settings:write'
  | 'network:fetch'

export interface BlockRendererContribution {
  id: string
  name: string
  description?: string
  /** Higher priority runs first; ties keep registration order. */
  priority?: number
}

export interface CommandContribution {
  id: string
  title: string
  description?: string
  /** Name without the leading slash; a future slash menu discovers it. */
  slash?: string
}

export interface PluginContributions {
  blockRenderers?: BlockRendererContribution[]
  commands?: CommandContribution[]
}

export interface PluginManifest {
  /** Stable id, e.g. `lkb.callout`. Must be unique in the host. */
  id: string
  name: string
  version: string
  description?: string
  author?: string
  engines: PluginEngines
  permissions: PluginPermission[]
  contributes: PluginContributions
}

export interface BlockSnapshot {
  /** Raw Markdown source of one top-level block. */
  source: string
  /** Best-effort kind detected by the host (`paragraph`, `heading1`, …). */
  kind: string
  /** Zero-based position in the note. */
  index: number
}

export interface BlockRenderResult {
  /**
   * HTML produced by the renderer. The host sanitises it again with the same
   * profile as the Markdown preview before inserting it.
   */
  html: string
  /** Extra class names for the preview wrapper. */
  className?: string
}

export interface BlockRendererContext {
  pluginId: string
  apiVersion: string
  /** Host renderer for nested Markdown (callouts, quotes, tables…). */
  renderMarkdown(markdown: string): string
}

export interface BlockRenderer {
  id: string
  name: string
  priority?: number
  /** Pure and fast; throws are caught and the next renderer is tried. */
  match(block: BlockSnapshot): boolean
  /** Return `null` to decline. Throws fall back to plain Markdown rendering. */
  render(block: BlockSnapshot, context: BlockRendererContext): BlockRenderResult | null
}

export interface NoteSnapshot {
  space: string
  slug: string
  lang: string
  title: string
  markdown: string
  revisionId: string | null
  dirty: boolean
}

export interface NotePatch {
  title?: string
  markdown?: string
}

export interface CommandInvocation {
  note: NoteSnapshot | null
  /** Currently selected text, empty when there is no selection. */
  selection: string
  /** Markdown of the block that triggered the command. */
  blockSource: string
}

/**
 * Capabilities handed to a plugin. The host owns the note; plugins may request
 * a patch, never mutate storage directly. Every method is synchronous so a
 * renderer cannot stall a keystroke with an un-awaited promise.
 */
export interface PluginHost {
  readonly apiVersion: string
  /** Aborted when the plugin is deactivated or the editor unmounts. */
  readonly signal: AbortSignal
  renderMarkdown(markdown: string): string
  getNote(): NoteSnapshot | null
  updateNote(patch: NotePatch): void
  toast(message: string, tone?: 'info' | 'warn' | 'error'): void
  getSettings<T extends Record<string, unknown> = Record<string, unknown>>(): Readonly<T>
  updateSettings(patch: Record<string, unknown>): void
  openSettings(): void
  registerBlockRenderer(renderer: BlockRenderer): Disposable
  registerCommand(command: Command): Disposable
}

export interface PluginContext {
  manifest: PluginManifest
  host: PluginHost
  log(level: 'debug' | 'info' | 'warn' | 'error', message: string, detail?: unknown): void
}

export interface Command {
  id: string
  title: string
  description?: string
  /** Slash trigger without the leading `/`, e.g. `callout`. */
  slash?: string
  run(invocation: CommandInvocation, context: PluginContext): void | Promise<void>
}

/**
 * Lifecycle hooks. v0 stores and type-checks them; only in-bundle modules can
 * supply them because there is no remote loader. A future sandboxed host will
 * dispatch the same names over `postMessage`.
 */
export interface PluginHooks {
  onNoteOpen?(note: NoteSnapshot, context: PluginContext): void | Promise<void>
  onNoteSave?(note: NoteSnapshot, context: PluginContext): void | Promise<void>
  onBlockRender?(block: BlockSnapshot, context: PluginContext): BlockRenderResult | null
  onCommand?(command: Command, context: PluginContext): void | Promise<void>
  onSettings?(settings: Readonly<Record<string, unknown>>, context: PluginContext): void | Promise<void>
}

/**
 * A compiled-in plugin. `manifest.contributes` is declarative metadata; the
 * executable `blockRenderers` / `commands` arrays must match those ids.
 */
export interface PluginModule {
  manifest: PluginManifest
  activate?(context: PluginContext): void | Promise<void>
  deactivate?(): void | Promise<void>
  blockRenderers?: BlockRenderer[]
  commands?: Command[]
  hooks?: PluginHooks
}

export interface Disposable {
  dispose(): void
}
