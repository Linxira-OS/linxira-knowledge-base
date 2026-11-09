/**
 * Plugin registry v0.
 *
 * Registration is in-process and synchronous. Nothing here downloads, imports
 * or evaluates remote code: a plugin must already be part of the bundle. The
 * registry validates manifests, version ranges, permission gates and the
 * manifest/implementation correspondence before exposing contributions.
 */
import DOMPurify from 'dompurify'
import { renderMarkdown } from '../lib/markdown'
import {
  PLUGIN_API_VERSION,
  type BlockRenderer,
  type BlockRendererContribution,
  type BlockSnapshot,
  type Command,
  type CommandInvocation,
  type Disposable,
  type PluginContext,
  type PluginHost,
  type PluginManifest,
  type PluginModule,
  type PluginPermission,
} from './types'

/* ------------------------------------------------------------------ semver */

export interface Semver {
  major: number
  minor: number
  patch: number
}

const SEMVER = /^(\d+)\.(\d+)\.(\d+)(?:[-+][0-9A-Za-z.-]+)?$/
const COMPARATOR = /^(?:\^|~|>=|<=|>|<|=)?\d+\.\d+\.\d+$/

export function parseSemver(value: string): Semver | null {
  const match = SEMVER.exec(value.trim())
  if (!match) return null
  return { major: Number(match[1]), minor: Number(match[2]), patch: Number(match[3]) }
}

function compareSemver(left: Semver, right: Semver): number {
  if (left.major !== right.major) return left.major < right.major ? -1 : 1
  if (left.minor !== right.minor) return left.minor < right.minor ? -1 : 1
  if (left.patch !== right.patch) return left.patch < right.patch ? -1 : 1
  return 0
}

/** Supported subset: `*`, `^`, `~`, `>=`, `>`, `<=`, `<`, `=`, exact, space-joined. */
export function isSupportedRange(range: string): boolean {
  const trimmed = range.trim()
  if (trimmed === '*') return true
  if (trimmed === '') return false
  return trimmed.split(/\s+/).every((token) => COMPARATOR.test(token))
}

function satisfiesComparator(version: Semver, token: string): boolean {
  const match = /^(\^|~|>=|<=|>|<|=)?(\d+\.\d+\.\d+)$/.exec(token)
  if (!match) return false
  const operator = match[1] ?? '='
  const target = parseSemver(match[2])
  if (!target) return false
  const order = compareSemver(version, target)

  switch (operator) {
    case '^':
      if (target.major > 0) return order >= 0 && version.major === target.major
      if (target.minor > 0) return order >= 0 && version.major === 0 && version.minor === target.minor
      return order >= 0 && version.major === 0 && version.minor === 0 && version.patch === target.patch
    case '~':
      return order >= 0 && version.major === target.major && version.minor === target.minor
    case '>=':
      return order >= 0
    case '>':
      return order > 0
    case '<=':
      return order <= 0
    case '<':
      return order < 0
    case '=':
    default:
      return order === 0
  }
}

/** Checks a host version against the semver-range subset the registry accepts. */
export function satisfiesSemverRange(version: string, range: string): boolean {
  const parsed = parseSemver(version)
  if (!parsed || !isSupportedRange(range)) return false
  const trimmed = range.trim()
  if (trimmed === '*') return true
  return trimmed.split(/\s+/).every((token) => satisfiesComparator(parsed, token))
}

/* --------------------------------------------------------------- validation */

const ID_PATTERN = /^[a-z0-9][a-z0-9._-]{1,63}$/
const KNOWN_PERMISSIONS: readonly PluginPermission[] = [
  'note:read',
  'note:write',
  'block:render',
  'command:register',
  'settings:read',
  'settings:write',
  'network:fetch',
]
const GRANTABLE_IN_V0: readonly PluginPermission[] = [
  'note:read',
  'note:write',
  'block:render',
  'command:register',
  'settings:read',
  'settings:write',
]

export class PluginRegistrationError extends Error {
  readonly pluginId: string
  readonly problems: readonly string[]

  constructor(pluginId: string, problems: readonly string[]) {
    super(`plugin ${pluginId}: ${problems.join('; ')}`)
    this.name = 'PluginRegistrationError'
    this.pluginId = pluginId
    this.problems = problems
  }
}

function validateContribution(
  description: BlockRendererContribution | { id: string; title: string },
  kind: 'block renderer' | 'command',
  problems: string[],
): void {
  if (!description || typeof description !== 'object') {
    problems.push(`contributes.${kind}s must contain objects`)
    return
  }
  if (!ID_PATTERN.test(description.id)) problems.push(`${kind} id "${description.id}" is invalid`)
  const label = 'name' in description ? description.name : description.title
  if (typeof label !== 'string' || label.trim() === '') problems.push(`${kind} ${description.id} needs a label`)
}

export interface PluginValidationOptions {
  /** Reject a manifest whose executable contributions are absent (default true). */
  requireImplementation?: boolean
}

/** Runtime validation for manifests that may come from JSON or a future loader. */
export function validatePlugin(
  module: PluginModule,
  options: PluginValidationOptions = {},
): string[] {
  const problems: string[] = []
  if (!module || typeof module !== 'object') return ['plugin module must be an object']
  const manifest = module.manifest
  if (!manifest || typeof manifest !== 'object') return ['manifest is required']

  if (typeof manifest.id !== 'string' || !ID_PATTERN.test(manifest.id)) {
    problems.push('manifest.id must match [a-z0-9][a-z0-9._-]{1,63}')
  }
  if (typeof manifest.name !== 'string' || manifest.name.trim() === '') problems.push('manifest.name is required')
  if (typeof manifest.version !== 'string' || parseSemver(manifest.version) === null) {
    problems.push('manifest.version must be semver MAJOR.MINOR.PATCH')
  }
  if (!manifest.engines || typeof manifest.engines.api !== 'string') {
    problems.push('manifest.engines.api is required')
  } else {
    if (!isSupportedRange(manifest.engines.api)) {
      problems.push(`engines.api "${manifest.engines.api}" is not a supported semver range`)
    } else if (!satisfiesSemverRange(PLUGIN_API_VERSION, manifest.engines.api)) {
      problems.push(`engines.api "${manifest.engines.api}" does not accept host API ${PLUGIN_API_VERSION}`)
    }
  }

  const permissions = Array.isArray(manifest.permissions) ? manifest.permissions : []
  if (!Array.isArray(manifest.permissions)) problems.push('manifest.permissions must be an array')
  for (const permission of permissions) {
    if (!KNOWN_PERMISSIONS.includes(permission)) {
      problems.push(`unknown permission "${String(permission)}"`)
    } else if (!GRANTABLE_IN_V0.includes(permission)) {
      problems.push(`permission "${permission}" is not grantable in plugin API v${PLUGIN_API_VERSION}`)
    }
  }
  if (new Set(permissions).size !== permissions.length) problems.push('permissions must be unique')

  const contributes = manifest.contributes
  if (!contributes || typeof contributes !== 'object') {
    problems.push('manifest.contributes is required')
  } else {
    const rendererIds = new Set<string>()
    const commandIds = new Set<string>()
    if (contributes.blockRenderers !== undefined && !Array.isArray(contributes.blockRenderers)) {
      problems.push('contributes.blockRenderers must be an array')
    }
    if (contributes.commands !== undefined && !Array.isArray(contributes.commands)) {
      problems.push('contributes.commands must be an array')
    }
    for (const descriptor of Array.isArray(contributes.blockRenderers) ? contributes.blockRenderers : []) {
      validateContribution(descriptor, 'block renderer', problems)
      if (rendererIds.has(descriptor.id)) problems.push(`duplicate contributed block renderer "${descriptor.id}"`)
      rendererIds.add(descriptor.id)
    }
    for (const descriptor of Array.isArray(contributes.commands) ? contributes.commands : []) {
      validateContribution(descriptor, 'command', problems)
      if (commandIds.has(descriptor.id)) problems.push(`duplicate contributed command "${descriptor.id}"`)
      commandIds.add(descriptor.id)
    }

    const renderers = module.blockRenderers ?? []
    const commands = module.commands ?? []
    if (options.requireImplementation !== false) {
      for (const renderer of renderers) {
        if (!rendererIds.has(renderer.id)) problems.push(`block renderer "${renderer.id}" is missing from contributes`)
      }
      for (const command of commands) {
        if (!commandIds.has(command.id)) problems.push(`command "${command.id}" is missing from contributes`)
      }
      for (const id of rendererIds) {
        if (!renderers.some((renderer) => renderer.id === id)) problems.push(`contributed block renderer "${id}" has no implementation`)
      }
      for (const id of commandIds) {
        if (!commands.some((command) => command.id === id)) problems.push(`contributed command "${id}" has no implementation`)
      }
    }
  }

  for (const renderer of module.blockRenderers ?? []) {
    if (!ID_PATTERN.test(renderer.id)) problems.push(`block renderer id "${renderer.id}" is invalid`)
    if (typeof renderer.match !== 'function' || typeof renderer.render !== 'function') {
      problems.push(`block renderer "${renderer.id}" must implement match() and render()`)
    }
  }
  const slashNames = new Set<string>()
  for (const command of module.commands ?? []) {
    if (!ID_PATTERN.test(command.id)) problems.push(`command id "${command.id}" is invalid`)
    if (typeof command.run !== 'function') problems.push(`command "${command.id}" must implement run()`)
    const slash = command.slash?.trim().toLowerCase()
    if (slash) {
      if (slashNames.has(slash)) problems.push(`duplicate slash command "/${slash}"`)
      slashNames.add(slash)
    }
  }

  const hooks = module.hooks
  if ((module.blockRenderers?.length ?? 0) > 0 && !permissions.includes('block:render')) {
    problems.push('block renderers require the "block:render" permission')
  }
  if ((module.commands?.length ?? 0) > 0 && !permissions.includes('command:register')) {
    problems.push('commands require the "command:register" permission')
  }
  if (hooks?.onNoteOpen && !permissions.includes('note:read')) {
    problems.push('onNoteOpen requires the "note:read" permission')
  }
  if (hooks?.onNoteSave && (!permissions.includes('note:read') || !permissions.includes('note:write'))) {
    problems.push('onNoteSave requires the "note:read" and "note:write" permissions')
  }
  if (hooks?.onBlockRender && !permissions.includes('block:render')) {
    problems.push('onBlockRender requires the "block:render" permission')
  }
  if (hooks?.onCommand && !permissions.includes('command:register')) {
    problems.push('onCommand requires the "command:register" permission')
  }
  if (hooks?.onSettings && !permissions.includes('settings:read')) {
    problems.push('onSettings requires the "settings:read" permission')
  }

  return problems
}

/* ----------------------------------------------------------------- registry */

export type PluginStatus = 'registered' | 'active' | 'failed'

export interface PluginInfo {
  manifest: PluginManifest
  status: PluginStatus
  error: string | null
}

export interface RegisteredBlockRenderer {
  pluginId: string
  pluginName: string
  renderer: BlockRenderer
  order: number
}

export interface RegisteredCommand {
  pluginId: string
  pluginName: string
  command: Command
}

export interface ResolvedBlockRender {
  html: string
  className?: string
  rendererId: string
  rendererName: string
  pluginId: string
  pluginName: string
}

export interface CommandOutcome {
  ok: boolean
  error?: unknown
}

interface RegisteredPlugin extends PluginInfo {
  module: PluginModule
}

const HTML_SANITIZE = { USE_PROFILES: { html: true } } as const

function createContext(manifest: PluginManifest, host: PluginHost): PluginContext {
  return {
    manifest,
    host,
    log(level, message, detail) {
      const line = `[plugin:${manifest.id}] ${message}`
      if (detail === undefined) console[level](line)
      else console[level](line, detail)
    },
  }
}

export class PluginRegistry {
  readonly apiVersion = PLUGIN_API_VERSION

  private readonly plugins = new Map<string, RegisteredPlugin>()
  private renderers: RegisteredBlockRenderer[] = []
  private readonly commands = new Map<string, RegisteredCommand>()
  private readonly listeners = new Set<() => void>()
  private revision = 0
  private rendererOrder = 0

  /** Registers a compiled-in plugin, or throws `PluginRegistrationError`. */
  register(module: PluginModule): Disposable {
    const problems = validatePlugin(module)
    if (problems.length > 0) {
      throw new PluginRegistrationError(module?.manifest?.id ?? '<unknown>', problems)
    }
    const { manifest } = module
    if (this.plugins.has(manifest.id)) {
      throw new PluginRegistrationError(manifest.id, ['a plugin with this id is already registered'])
    }

    for (const renderer of module.blockRenderers ?? []) {
      if (this.renderers.some((entry) => entry.renderer.id === renderer.id)) {
        throw new PluginRegistrationError(manifest.id, [`block renderer id "${renderer.id}" is already registered`])
      }
    }
    const existingSlashes = new Set(
      [...this.commands.values()]
        .map((entry) => entry.command.slash?.trim().toLowerCase())
        .filter((slash): slash is string => Boolean(slash)),
    )
    for (const command of module.commands ?? []) {
      if (this.commands.has(command.id)) {
        throw new PluginRegistrationError(manifest.id, [`command id "${command.id}" is already registered`])
      }
      const slash = command.slash?.trim().toLowerCase()
      if (slash && existingSlashes.has(slash)) {
        throw new PluginRegistrationError(manifest.id, [`slash command "/${slash}" is already registered`])
      }
    }

    this.plugins.set(manifest.id, { manifest, module, status: 'registered', error: null })
    for (const renderer of module.blockRenderers ?? []) {
      this.rendererOrder += 1
      this.renderers.push({
        pluginId: manifest.id,
        pluginName: manifest.name,
        renderer,
        order: this.rendererOrder,
      })
    }
    for (const command of module.commands ?? []) {
      this.commands.set(command.id, { pluginId: manifest.id, pluginName: manifest.name, command })
    }
    this.emit()

    let disposed = false
    return {
      dispose: () => {
        if (disposed) return
        disposed = true
        this.unregister(manifest.id)
      },
    }
  }

  unregister(pluginId: string): void {
    const entry = this.plugins.get(pluginId)
    if (!entry) return
    if (entry.status === 'active') {
      try {
        const result = entry.module.deactivate?.()
        if (result instanceof Promise) void result.catch((error) => console.warn(`[plugin:${pluginId}] deactivate failed`, error))
      } catch (error) {
        console.warn(`[plugin:${pluginId}] deactivate failed`, error)
      }
    }
    this.plugins.delete(pluginId)
    this.renderers = this.renderers.filter((renderer) => renderer.pluginId !== pluginId)
    for (const [id, command] of this.commands) {
      if (command.pluginId === pluginId) this.commands.delete(id)
    }
    this.emit()
  }

  getPlugin(pluginId: string): PluginInfo | undefined {
    const entry = this.plugins.get(pluginId)
    return entry ? { manifest: entry.manifest, status: entry.status, error: entry.error } : undefined
  }

  listPlugins(): PluginInfo[] {
    return [...this.plugins.values()].map(({ manifest, status, error }) => ({ manifest, status, error }))
  }

  listBlockRenderers(): readonly RegisteredBlockRenderer[] {
    return this.renderers
  }

  listCommands(): readonly RegisteredCommand[] {
    return [...this.commands.values()]
  }

  getCommand(commandId: string): Command | undefined {
    return this.commands.get(commandId)?.command
  }

  /**
   * First matching renderer wins. A throwing matcher/renderer is skipped, so a
   * broken plugin can never blank the note preview.
   */
  resolveBlockRenderer(block: BlockSnapshot): ResolvedBlockRender | null {
    const ordered = [...this.renderers].sort(
      (left, right) => (right.renderer.priority ?? 0) - (left.renderer.priority ?? 0) || left.order - right.order,
    )
    for (const entry of ordered) {
      try {
        if (!entry.renderer.match(block)) continue
        const result = entry.renderer.render(block, {
          pluginId: entry.pluginId,
          apiVersion: this.apiVersion,
          renderMarkdown,
        })
        if (!result || typeof result.html !== 'string') continue
        return {
          html: DOMPurify.sanitize(result.html, HTML_SANITIZE),
          className: result.className?.replace(/[^\w -]+/g, '') || undefined,
          rendererId: entry.renderer.id,
          rendererName: entry.renderer.name,
          pluginId: entry.pluginId,
          pluginName: entry.pluginName,
        }
      } catch (error) {
        console.warn(`[plugin:${entry.pluginId}] renderer ${entry.renderer.id} failed`, error)
      }
    }
    return null
  }

  /** Activates a registered plugin; activation failure is isolated. */
  async activate(pluginId: string, host: PluginHost): Promise<boolean> {
    const entry = this.plugins.get(pluginId)
    if (!entry) return false
    if (entry.status === 'active') return true
    try {
      await entry.module.activate?.(createContext(entry.manifest, host))
      entry.status = 'active'
      entry.error = null
      this.emit()
      return true
    } catch (error) {
      entry.status = 'failed'
      entry.error = error instanceof Error ? error.message : String(error)
      console.error(`[plugin:${pluginId}] activation failed`, error)
      this.emit()
      return false
    }
  }

  async deactivate(pluginId: string): Promise<void> {
    const entry = this.plugins.get(pluginId)
    if (!entry || entry.status !== 'active') return
    try {
      await entry.module.deactivate?.()
    } catch (error) {
      entry.error = error instanceof Error ? error.message : String(error)
      console.warn(`[plugin:${pluginId}] deactivate failed`, error)
    }
    entry.status = 'registered'
    this.emit()
  }

  /** Runs a command with failure isolation; the host supplies its capabilities. */
  async runCommand(commandId: string, invocation: CommandInvocation, host: PluginHost): Promise<CommandOutcome> {
    const entry = this.commands.get(commandId)
    if (!entry) return { ok: false, error: new Error(`unknown command ${commandId}`) }
    const plugin = this.plugins.get(entry.pluginId)
    if (!plugin) return { ok: false, error: new Error(`plugin ${entry.pluginId} is not registered`) }
    try {
      await entry.command.run(invocation, createContext(plugin.manifest, host))
      return { ok: true }
    } catch (error) {
      console.warn(`[plugin:${entry.pluginId}] command ${commandId} failed`, error)
      return { ok: false, error }
    }
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  getRevision = (): number => this.revision

  private emit(): void {
    this.revision += 1
    for (const listener of this.listeners) {
      try {
        listener()
      } catch (error) {
        console.warn('[plugins] registry listener failed', error)
      }
    }
  }
}

/* ---------------------------------------------------------- built-in callout */

const CALLOUT_HEADER = /^ {0,3}>[ \t]*\[!([a-zA-Z]+)\](?:[ \t]+(.*))?$/

const CALLOUT_TONES: Record<string, { tone: string; title: string }> = {
  note: { tone: 'info', title: 'Note' },
  info: { tone: 'info', title: 'Info' },
  tip: { tone: 'success', title: 'Tip' },
  success: { tone: 'success', title: 'Success' },
  warning: { tone: 'warning', title: 'Warning' },
  question: { tone: 'warning', title: 'Question' },
  danger: { tone: 'danger', title: 'Danger' },
  quote: { tone: 'quote', title: 'Quote' },
  example: { tone: 'example', title: 'Example' },
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => {
    switch (character) {
      case '&':
        return '&amp;'
      case '<':
        return '&lt;'
      case '>':
        return '&gt;'
      case '"':
        return '&quot;'
      default:
        return '&#39;'
    }
  })
}

const calloutRenderer: BlockRenderer = {
  id: 'lkb.callout.renderer',
  name: 'Callout',
  priority: 100,
  match(block) {
    return CALLOUT_HEADER.test(block.source.split('\n', 1)[0] ?? '')
  },
  render(block, context) {
    const lines = block.source.split('\n')
    const header = lines[0]?.match(CALLOUT_HEADER)
    if (!header) return null
    const type = header[1].toLowerCase()
    const preset = CALLOUT_TONES[type] ?? { tone: 'info', title: type.charAt(0).toUpperCase() + type.slice(1) }
    const title = header[2]?.trim() || preset.title
    const body = lines
      .slice(1)
      .map((line) => line.replace(/^ {0,3}>[ \t]?/, ''))
      .join('\n')
      .trim()
    const inner = body === '' ? '' : context.renderMarkdown(body)
    return {
      html:
        `<div class="lkb-callout lkb-callout-${preset.tone}" data-callout="${escapeHtml(type)}">` +
        `<div class="lkb-callout-title">${escapeHtml(title)}</div>` +
        (inner === '' ? '' : `<div class="lkb-callout-body">${inner}</div>`) +
        '</div>',
    }
  },
}

const insertCalloutCommand: Command = {
  id: 'lkb.callout.insert',
  title: 'Insert callout',
  description: 'Append an Obsidian-style callout block to the active note.',
  slash: 'callout',
  run(invocation, context) {
    const note = invocation.note ?? context.host.getNote()
    if (!note) return
    const snippet = '> [!NOTE]\n> Callout text.'
    const base = note.markdown.replace(/\s+$/, '')
    context.host.updateNote({ markdown: base === '' ? `${snippet}\n` : `${base}\n\n${snippet}\n` })
  },
}

export const builtinCalloutPlugin: PluginModule = {
  manifest: {
    id: 'lkb.callout',
    name: 'Callout blocks',
    version: '0.1.0',
    description: 'Obsidian-style callouts rendered through the v0 block renderer API.',
    author: 'LKB',
    engines: { api: '^0.1.0' },
    permissions: ['block:render', 'command:register'],
    contributes: {
      blockRenderers: [
        {
          id: 'lkb.callout.renderer',
          name: 'Callout',
          description: 'Renders `> [!NOTE]` style blocks with tone-coloured chrome.',
          priority: 100,
        },
      ],
      commands: [
        {
          id: 'lkb.callout.insert',
          title: 'Insert callout',
          description: 'Append a callout skeleton to the note.',
          slash: 'callout',
        },
      ],
    },
  },
  blockRenderers: [calloutRenderer],
  commands: [insertCalloutCommand],
  hooks: {
    onBlockRender(block) {
      return calloutRenderer.match(block) ? calloutRenderer.render(block, { pluginId: 'lkb.callout', apiVersion: PLUGIN_API_VERSION, renderMarkdown }) : null
    },
  },
}

/** The process-wide registry. Built-ins are registered at bundle load. */
export const pluginRegistry = new PluginRegistry()
pluginRegistry.register(builtinCalloutPlugin)
