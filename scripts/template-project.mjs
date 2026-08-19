import { spawnSync } from 'node:child_process'
import {
  chmodSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  rmdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path'
import { createInterface } from 'node:readline/promises'
import { fileURLToPath } from 'node:url'

const SCRIPT_DIRECTORY = dirname(fileURLToPath(import.meta.url))
const PROJECT_ROOT = resolve(SCRIPT_DIRECTORY, '..')
const STATE_PATH = '.template-state.json'
const STATE_SCHEMA_VERSION = 1
const TEMPLATE_MARKER = 'template-init'
const EXAMPLE_MARKER = 'template-example'

const TEMPLATE_MARKER_FILES = new Map([
  ['README.md', 1],
  ['CONTRIBUTING.md', 2],
  ['justfile', 1],
  ['.github/workflows/ci.yml', 3],
])

const EXAMPLE_MARKER_FILES = new Map([
  ['README.md', 3],
  ['server/src/app.rs', 4],
  ['server/src/controller/mod.rs', 1],
  ['server/src/database/mod.rs', 6],
  ['server/src/error.rs', 3],
  ['server/src/main.rs', 1],
  ['server/src/schema/postgres.rs', 1],
  ['server/src/schema/sqlite.rs', 1],
  ['front/src/App.vue', 1],
  ['front/src/pages/Dashboard.vue', 2],
  ['front/src/router/index.ts', 2],
  ['front/src/style.css', 5],
])

const TEMPLATE_ONLY_FILES = [
  '.github/workflows/template-integration.yml',
  'scripts/template-project.test.mjs',
]

const EXAMPLE_FILES = [
  'front/src/pages/Items.vue',
  'front/src/pages/Users.vue',
  'front/src/services/items.ts',
  'front/src/services/users.ts',
  'server/migrations/postgres/2026-05-15-000000_create_items_and_users/down.sql',
  'server/migrations/postgres/2026-05-15-000000_create_items_and_users/up.sql',
  'server/migrations/sqlite/2026-05-15-000000_create_items_and_users/down.sql',
  'server/migrations/sqlite/2026-05-15-000000_create_items_and_users/up.sql',
  'server/src/controller/api_id.rs',
  'server/src/controller/items.rs',
  'server/src/controller/users.rs',
  'server/src/database/items.rs',
  'server/src/database/users.rs',
  'server/src/service/items.rs',
  'server/src/service/mod.rs',
  'server/src/service/users.rs',
]

const WINDOWS_RESERVED_NAMES = new Set([
  'con',
  'prn',
  'aux',
  'nul',
  'com1',
  'com2',
  'com3',
  'com4',
  'com5',
  'com6',
  'com7',
  'com8',
  'com9',
  'lpt1',
  'lpt2',
  'lpt3',
  'lpt4',
  'lpt5',
  'lpt6',
  'lpt7',
  'lpt8',
  'lpt9',
])

const SLUG_PATTERN = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/
const GITHUB_OWNER_PATTERN = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?$/
const GITHUB_REPOSITORY_PATTERN = /^[A-Za-z0-9._-]{1,100}$/

function fail(message) {
  throw new Error(message)
}

function git(root, args, options = {}) {
  const result = spawnSync('git', ['-C', root, ...args], {
    encoding: 'utf8',
    stdio: options.inherit ? 'inherit' : ['ignore', 'pipe', 'pipe'],
  })
  if (result.error) {
    fail(`unable to run git: ${result.error.message}`)
  }
  if (result.status !== 0 && !options.allowFailure) {
    const detail = String(result.stderr || result.stdout || '').trim()
    fail(`git ${args.join(' ')} failed${detail ? `: ${detail}` : ''}`)
  }
  return result
}

function command(commandName, args, root, options = {}) {
  const result = spawnSync(commandName, args, {
    cwd: root,
    encoding: options.inherit ? undefined : 'utf8',
    stdio: options.inherit ? 'inherit' : ['ignore', 'pipe', 'pipe'],
    env: process.env,
  })
  if (result.error) {
    fail(`unable to run ${commandName}: ${result.error.message}`)
  }
  if (result.status !== 0) {
    const detail = options.inherit
      ? ''
      : String(result.stderr || result.stdout || '').trim()
    fail(`${commandName} ${args.join(' ')} failed${detail ? `: ${detail}` : ''}`)
  }
  return result
}

function repositoryRoot(root = PROJECT_ROOT) {
  const result = git(root, ['rev-parse', '--show-toplevel'])
  const gitRoot = realpathSync(String(result.stdout).trim())
  const expectedRoot = realpathSync(root)
  if (gitRoot !== expectedRoot) {
    fail(`run the command from the repository root '${gitRoot}'`)
  }
  return gitRoot
}

function requireCleanWorktree(root) {
  const status = git(root, ['status', '--porcelain=v1', '--untracked-files=all'])
  if (String(status.stdout).trim()) {
    fail('the Git worktree must be clean, including staged and untracked files')
  }
}

function readState(root) {
  const path = join(root, STATE_PATH)
  if (!existsSync(path)) {
    fail(`${STATE_PATH} is missing; this repository is not a supported template state`)
  }

  let state
  try {
    state = JSON.parse(readFileSync(path, 'utf8'))
  } catch (error) {
    fail(`${STATE_PATH} is invalid JSON: ${error.message}`)
  }

  if (state.schemaVersion !== STATE_SCHEMA_VERSION) {
    fail(`unsupported template state schema version '${state.schemaVersion}'`)
  }
  if (!['template', 'initialized'].includes(state.status)) {
    fail(`unsupported template status '${state.status}'`)
  }
  if (!['present', 'stripped'].includes(state.examples)) {
    fail(`unsupported example status '${state.examples}'`)
  }

  const requiredIdentityFields = [
    'projectSlug',
    'rustIdentifier',
    'frontendPackage',
    'displayName',
    'githubRepository',
  ]
  for (const field of requiredIdentityFields) {
    if (typeof state.identity?.[field] !== 'string' || !state.identity[field]) {
      fail(`${STATE_PATH} is missing identity.${field}`)
    }
  }

  return state
}

export function deriveIdentity(projectSlug, displayName, githubRepository) {
  return {
    projectSlug,
    rustIdentifier: projectSlug.replaceAll('-', '_'),
    frontendPackage: `${projectSlug}-front`,
    displayName,
    githubRepository,
  }
}

export function defaultDisplayName(projectSlug) {
  return projectSlug
    .split('-')
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join(' ')
}

export function validateProjectSlug(projectSlug, sourceIdentity = undefined) {
  if (typeof projectSlug !== 'string') {
    fail('project slug is required')
  }
  if (!SLUG_PATTERN.test(projectSlug)) {
    fail('project slug must be lowercase ASCII kebab-case and start with a letter')
  }
  if (projectSlug.length > 58) {
    fail('project slug must be at most 58 characters so the PostgreSQL test database name fits')
  }
  if (WINDOWS_RESERVED_NAMES.has(projectSlug)) {
    fail(`project slug '${projectSlug}' is reserved on Windows`)
  }

  if (sourceIdentity) {
    const sourceTokens = [
      sourceIdentity.projectSlug,
      sourceIdentity.rustIdentifier,
      sourceIdentity.frontendPackage,
    ]
    if (sourceTokens.some((token) => projectSlug.includes(token))) {
      fail('project slug must not contain a template source identifier')
    }
  }
}

export function validateDisplayName(displayName, sourceIdentity = undefined) {
  if (typeof displayName !== 'string') {
    fail('display name is required')
  }
  if (displayName !== displayName.trim()) {
    fail('display name must not have leading or trailing whitespace')
  }
  const characters = Array.from(displayName)
  if (characters.length < 1 || characters.length > 80) {
    fail('display name must contain between 1 and 80 characters')
  }
  if (/\p{Cc}/u.test(displayName)) {
    fail('display name must not contain newlines or control characters')
  }
  if (sourceIdentity && sourceTokens(sourceIdentity).some((token) => displayName.includes(token))) {
    fail('display name must not contain a template source identifier')
  }
}

export function parseGitHubRepository(value) {
  if (typeof value !== 'string') {
    fail('GitHub repository is required')
  }
  const input = value.trim()
  let repository = input

  const sshMatch = input.match(/^git@github\.com:([^/]+)\/(.+?)(?:\.git)?$/i)
  const sshUrlMatch = input.match(/^ssh:\/\/git@github\.com\/([^/]+)\/(.+?)(?:\.git)?$/i)
  if (sshMatch || sshUrlMatch) {
    const match = sshMatch || sshUrlMatch
    repository = `${match[1]}/${match[2]}`
  } else if (/^https?:\/\//i.test(input)) {
    let url
    try {
      url = new URL(input)
    } catch {
      fail('GitHub repository URL is invalid')
    }
    if (url.hostname.toLowerCase() !== 'github.com') {
      fail('GitHub repository URL must use github.com')
    }
    repository = url.pathname.replace(/^\/+|\/+$/g, '').replace(/\.git$/i, '')
  }

  repository = repository.replace(/\.git$/i, '')
  const parts = repository.split('/')
  if (parts.length !== 2) {
    fail('GitHub repository must be owner/repository or a standard github.com URL')
  }
  const [owner, name] = parts
  if (!GITHUB_OWNER_PATTERN.test(owner) || owner.includes('--')) {
    fail(`GitHub owner '${owner}' is invalid`)
  }
  if (!GITHUB_REPOSITORY_PATTERN.test(name) || name === '.' || name === '..') {
    fail(`GitHub repository name '${name}' is invalid`)
  }
  return `${owner}/${name}`
}

function sourceTokens(identity) {
  return [identity.frontendPackage, identity.projectSlug, identity.rustIdentifier]
}

function validateTargetIdentity(target, source) {
  validateProjectSlug(target.projectSlug, source)
  validateDisplayName(target.displayName, source)
  const canonicalRepository = parseGitHubRepository(target.githubRepository)
  if (canonicalRepository === source.githubRepository) {
    fail('GitHub repository must not remain pointed at the template source')
  }
  if (sourceTokens(source).some((token) => canonicalRepository.includes(token))) {
    fail('GitHub repository must not contain a template source identifier')
  }
}

function detectedGitHubRepository(root, source) {
  const result = git(root, ['remote', 'get-url', 'origin'], { allowFailure: true })
  if (result.status !== 0) {
    return undefined
  }
  try {
    const detected = parseGitHubRepository(String(result.stdout).trim())
    return detected === source.githubRepository ? undefined : detected
  } catch {
    return undefined
  }
}

function trackedFiles(root) {
  const output = git(root, ['ls-files', '-z']).stdout
  return String(output)
    .split('\0')
    .filter(Boolean)
}

function isText(buffer) {
  if (buffer.includes(0)) {
    return false
  }
  try {
    new TextDecoder('utf-8', { fatal: true }).decode(buffer)
    return true
  } catch {
    return false
  }
}

function snapshotTrackedTree(root) {
  const tree = new Map()
  for (const path of trackedFiles(root)) {
    const absolutePath = join(root, path)
    if (!existsSync(absolutePath) || !lstatSync(absolutePath).isFile()) {
      continue
    }
    tree.set(path, {
      buffer: readFileSync(absolutePath),
      mode: statSync(absolutePath).mode,
    })
  }
  return tree
}

function replaceIdentity(text, source, target) {
  const replacements = new Map([
    [source.githubRepository, target.githubRepository],
    [source.frontendPackage, target.frontendPackage],
    [source.projectSlug, target.projectSlug],
    [source.rustIdentifier, target.rustIdentifier],
  ])
  const patterns = [...replacements.keys()].sort((left, right) => right.length - left.length)
  const expression = new RegExp(patterns.map(escapeRegExp).join('|'), 'g')
  return text.replace(expression, (match) => replacements.get(match))
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function replaceExact(text, from, to, path, expectedCount = 1) {
  const count = text.split(from).length - 1
  if (count !== expectedCount) {
    fail(`${path} expected ${expectedCount} occurrence(s) of a required template value; found ${count}`)
  }
  return text.replaceAll(from, to)
}

function escapeHtml(value) {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')
}

function escapeMarkdownHeading(value) {
  return value.replaceAll('\\', '\\\\').replace(/([`*_[\]<>#|])/g, '\\$1')
}

function applyDisplayIdentity(path, text, source, target) {
  if (path === 'README.md') {
    return replaceExact(
      text,
      `# ${source.projectSlug}`,
      `# ${escapeMarkdownHeading(target.displayName)}`,
      path,
    )
  }
  if (path === 'front/index.html') {
    return replaceExact(
      text,
      `<title>${source.projectSlug}</title>`,
      `<title>${escapeHtml(target.displayName)}</title>`,
      path,
    )
  }
  if (path === 'front/src/App.vue') {
    text = replaceExact(
      text,
      `<span>${source.projectSlug}</span>`,
      `<span>${escapeHtml(target.displayName)}</span>`,
      path,
      2,
    )
    const brandMark = target.projectSlug[0].toUpperCase()
    return replaceExact(
      text,
      '<span class="brand-mark">A</span>',
      `<span class="brand-mark">${brandMark}</span>`,
      path,
      2,
    )
  }
  if (path === 'LICENSE') {
    return replaceExact(
      text,
      `${source.projectSlug} contributors`,
      `${target.displayName} contributors`,
      path,
    )
  }
  return text
}

export function removeMarkedBlocks(text, marker) {
  const newline = text.includes('\r\n') ? '\r\n' : '\n'
  const hadFinalNewline = text.endsWith('\n')
  const lines = text.split(/\r?\n/)
  const startMarker = `${marker}:start`
  const endMarker = `${marker}:end`
  const output = []
  let inside = false
  let count = 0
  const startsWithRemovedBlock = lines[0]?.includes(startMarker) === true
  const lastContentLine = [...lines].reverse().find((line) => line !== '')
  const endsWithRemovedBlock = lastContentLine?.includes(endMarker) === true

  for (const line of lines) {
    if (line.includes(startMarker)) {
      if (inside) {
        fail(`nested ${startMarker} markers are not supported`)
      }
      inside = true
      count += 1
      continue
    }
    if (line.includes(endMarker)) {
      if (!inside) {
        fail(`found ${endMarker} without a matching start marker`)
      }
      inside = false
      continue
    }
    if (!inside) {
      output.push(line)
    }
  }

  if (inside) {
    fail(`found ${startMarker} without a matching end marker`)
  }

  if (startsWithRemovedBlock) {
    while (output[0] === '') {
      output.shift()
    }
  }
  if (endsWithRemovedBlock) {
    while (output.at(-1) === '') {
      output.pop()
    }
  }

  let result = output.join(newline)
  result = result.replace(/(\r?\n){3,}/g, `${newline}${newline}`)
  if (hadFinalNewline && !result.endsWith(newline)) {
    result += newline
  }
  return { text: result, count }
}

function transformMarkers(tree, marker, expectedFiles) {
  for (const [path, expectedCount] of expectedFiles) {
    const entry = tree.get(path)
    if (!entry) {
      fail(`${path} is required for ${marker} transformation`)
    }
    const original = entry.buffer.toString('utf8')
    const transformed = removeMarkedBlocks(original, marker)
    if (transformed.count !== expectedCount) {
      fail(`${path} expected ${expectedCount} ${marker} block(s); found ${transformed.count}`)
    }
    entry.buffer = Buffer.from(transformed.text)
  }
}

function requireFiles(tree, paths, purpose) {
  for (const path of paths) {
    if (!tree.has(path)) {
      fail(`${path} is required to ${purpose}`)
    }
  }
}

function removeFiles(tree, paths) {
  for (const path of paths) {
    tree.delete(path)
  }
}

function updateStateEntry(tree, state) {
  const entry = tree.get(STATE_PATH)
  if (!entry) {
    fail(`${STATE_PATH} must be tracked`)
  }
  entry.buffer = Buffer.from(`${JSON.stringify(state, null, 2)}\n`)
}

function refreshEmptyMigrationEmbeds(tree) {
  const path = 'server/src/database/mod.rs'
  const entry = tree.get(path)
  if (!entry) {
    fail(`${path} is required to refresh embedded migrations`)
  }
  let text = entry.buffer.toString('utf8')
  text = replaceExact(
    text,
    'embed_migrations!("migrations/sqlite")',
    'embed_migrations!("migrations/./sqlite")',
    path,
  )
  text = replaceExact(
    text,
    'embed_migrations!("migrations/postgres")',
    'embed_migrations!("migrations/./postgres")',
    path,
  )
  entry.buffer = Buffer.from(text)
}

function planInitialization(snapshot, state, target, keepExamples) {
  requireFiles(snapshot, TEMPLATE_ONLY_FILES, 'initialize the project')
  if (!keepExamples) {
    requireFiles(snapshot, EXAMPLE_FILES, 'strip example resources')
  }

  const tree = cloneTree(snapshot)
  removeFiles(tree, TEMPLATE_ONLY_FILES)

  for (const [path, entry] of tree) {
    if (!isText(entry.buffer) || path === STATE_PATH) {
      continue
    }
    let text = entry.buffer.toString('utf8')
    text = applyDisplayIdentity(path, text, state.identity, target)
    text = replaceIdentity(text, state.identity, target)
    entry.buffer = Buffer.from(text)
  }

  transformMarkers(tree, TEMPLATE_MARKER, TEMPLATE_MARKER_FILES)
  if (!keepExamples) {
    removeFiles(tree, EXAMPLE_FILES)
    transformMarkers(tree, EXAMPLE_MARKER, EXAMPLE_MARKER_FILES)
    refreshEmptyMigrationEmbeds(tree)
  }

  updateStateEntry(tree, {
    schemaVersion: STATE_SCHEMA_VERSION,
    status: 'initialized',
    identity: target,
    examples: keepExamples ? 'present' : 'stripped',
  })
  verifyNoSourceIdentity(tree, state.identity)
  return tree
}

function planExampleCleanup(snapshot, state) {
  requireFiles(snapshot, EXAMPLE_FILES, 'strip example resources')
  const tree = cloneTree(snapshot)
  removeFiles(tree, EXAMPLE_FILES)
  transformMarkers(tree, EXAMPLE_MARKER, EXAMPLE_MARKER_FILES)
  refreshEmptyMigrationEmbeds(tree)
  updateStateEntry(tree, { ...state, examples: 'stripped' })
  return tree
}

function cloneTree(tree) {
  return new Map(
    [...tree].map(([path, entry]) => [
      path,
      { buffer: Buffer.from(entry.buffer), mode: entry.mode },
    ]),
  )
}

function verifyNoSourceIdentity(tree, source) {
  const residuals = []
  for (const [path, entry] of tree) {
    if (!isText(entry.buffer)) {
      continue
    }
    const text = entry.buffer.toString('utf8')
    for (const token of sourceTokens(source)) {
      if (text.includes(token)) {
        residuals.push(`${path}: ${token}`)
      }
    }
  }
  if (residuals.length) {
    fail(`template source identifiers remain:\n${residuals.join('\n')}`)
  }
}

function changedPaths(before, after) {
  const paths = new Set([...before.keys(), ...after.keys()])
  return [...paths]
    .filter((path) => {
      const left = before.get(path)
      const right = after.get(path)
      return !left || !right || !left.buffer.equals(right.buffer)
    })
    .sort()
}

function writeTreeTransaction(root, before, after, validate) {
  const changes = changedPaths(before, after)
  const deletedDirectories = [
    ...new Set(
      changes
        .filter((path) => before.has(path) && !after.has(path))
        .map((path) => dirname(path))
        .filter((path) => path !== '.'),
    ),
  ].sort((left, right) => right.split('/').length - left.split('/').length)
  let applying = true
  let restored = false

  const restore = () => {
    if (restored) {
      return
    }
    for (const path of changes) {
      const absolutePath = join(root, path)
      const original = before.get(path)
      if (!original) {
        rmSync(absolutePath, { force: true })
        continue
      }
      mkdirSync(dirname(absolutePath), { recursive: true })
      writeFileSync(absolutePath, original.buffer)
      chmodSync(absolutePath, original.mode)
    }
    restored = true
  }

  const interrupt = () => {
    if (applying) {
      restore()
    }
    process.exit(130)
  }
  process.once('SIGINT', interrupt)
  process.once('SIGTERM', interrupt)

  try {
    for (const path of changes) {
      const absolutePath = join(root, path)
      const next = after.get(path)
      if (!next) {
        rmSync(absolutePath, { force: true })
        continue
      }
      mkdirSync(dirname(absolutePath), { recursive: true })
      writeFileSync(absolutePath, next.buffer)
      chmodSync(absolutePath, next.mode)
    }
    for (const path of deletedDirectories) {
      const absolutePath = join(root, path)
      if (!existsSync(absolutePath)) {
        continue
      }
      try {
        rmdirSync(absolutePath)
      } catch (error) {
        if (error.code !== 'ENOTEMPTY' && error.code !== 'EEXIST') {
          throw error
        }
      }
    }
    validate()
    applying = false
    return changes
  } catch (error) {
    restore()
    fail(`${error.message}; all project files were restored`)
  } finally {
    process.removeListener('SIGINT', interrupt)
    process.removeListener('SIGTERM', interrupt)
  }
}

function validateWrittenProject(root, expectedState) {
  const state = readState(root)
  if (JSON.stringify(state) !== JSON.stringify(expectedState)) {
    fail('written template state does not match the planned state')
  }

  const cargoManifest = readFileSync(join(root, 'server/Cargo.toml'), 'utf8')
  const expectedCargoName = `name = "${state.identity.projectSlug}"`
  if (
    (cargoManifest.split(expectedCargoName).length - 1) !== 2 ||
    !cargoManifest.includes(`default-run = "${state.identity.projectSlug}"`)
  ) {
    fail('server/Cargo.toml package, default-run, and binary names are inconsistent')
  }
  const cargoLock = readFileSync(join(root, 'Cargo.lock'), 'utf8')
  if (!cargoLock.includes(`name = "${state.identity.projectSlug}"`)) {
    fail('Cargo.lock does not contain the initialized Rust package name')
  }

  const frontendPackage = JSON.parse(readFileSync(join(root, 'front/package.json'), 'utf8'))
  const frontendLock = JSON.parse(readFileSync(join(root, 'front/package-lock.json'), 'utf8'))
  if (
    frontendPackage.name !== state.identity.frontendPackage ||
    frontendLock.name !== state.identity.frontendPackage ||
    frontendLock.packages?.['']?.name !== state.identity.frontendPackage
  ) {
    fail('frontend package.json and package-lock.json names are inconsistent')
  }

  const tracked = trackedFiles(root)
  if (state.status === 'initialized') {
    for (const path of TEMPLATE_ONLY_FILES) {
      if (tracked.includes(path) && existsSync(join(root, path))) {
        fail(`template-only file was not removed: ${path}`)
      }
    }
  }
  if (state.examples === 'stripped') {
    for (const path of EXAMPLE_FILES) {
      if (existsSync(join(root, path))) {
        fail(`example file remains after cleanup: ${path}`)
      }
    }
    for (const path of tracked) {
      const absolutePath = join(root, path)
      if (!existsSync(absolutePath)) {
        continue
      }
      const buffer = readFileSync(absolutePath)
      if (isText(buffer) && buffer.toString('utf8').includes(`${EXAMPLE_MARKER}:`)) {
        fail(`example marker remains after cleanup: ${path}`)
      }
    }
  }

  command('cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1'], root)
}

function ignoredArtifactWarnings(root) {
  const candidates = [
    ['.app', 'local application data'],
    ['target', 'Rust build artifacts'],
    ['front/dist', 'frontend build artifacts'],
  ]
  return candidates
    .filter(([path]) => existsSync(join(root, path)))
    .map(([path, description]) => `${description} at ${path}/ is ignored and will not be renamed or removed`)
}

function printSummary(state, target, keepExamples, runCheck, root) {
  console.log('\nInitialization summary')
  console.log(`  project slug:       ${target.projectSlug}`)
  console.log(`  Rust package/bin:   ${target.projectSlug}`)
  console.log(`  Rust/database name: ${target.rustIdentifier}`)
  console.log(`  frontend package:   ${target.frontendPackage}`)
  console.log(`  display name:       ${target.displayName}`)
  console.log(`  GitHub repository:  ${target.githubRepository}`)
  console.log(`  examples:           ${keepExamples ? 'keep' : 'strip'}`)
  console.log(`  run just check:     ${runCheck ? 'yes' : 'no'}`)
  if (basename(root) !== target.projectSlug) {
    console.log(`  warning: directory '${basename(root)}' does not match the project slug`)
  }
  if (detectedGitHubRepository(root, state.identity) !== target.githubRepository) {
    console.log('  warning: the selected GitHub repository does not match the detected origin')
  }
  for (const warning of ignoredArtifactWarnings(root)) {
    console.log(`  warning: ${warning}`)
  }
}

function printStripSummary(runCheck, root) {
  console.log('\nExample cleanup summary')
  console.log(`  delete files:       ${EXAMPLE_FILES.length}`)
  console.log('  remove resources:   items, users')
  console.log('  database data:      unchanged')
  console.log(`  run just check:     ${runCheck ? 'yes' : 'no'}`)
  if (existsSync(join(root, '.app'))) {
    console.log('  warning: existing local databases are ignored and will not be changed')
  }
}

async function askText(interface_, question, defaultValue = undefined) {
  const suffix = defaultValue ? ` [${defaultValue}]` : ''
  const answer = (await interface_.question(`${question}${suffix}: `)).trim()
  return answer || defaultValue || ''
}

async function askYesNo(interface_, question, defaultValue) {
  const suffix = defaultValue ? '[Y/n]' : '[y/N]'
  while (true) {
    const answer = (await interface_.question(`${question} ${suffix}: `)).trim().toLowerCase()
    if (!answer) {
      return defaultValue
    }
    if (['y', 'yes'].includes(answer)) {
      return true
    }
    if (['n', 'no'].includes(answer)) {
      return false
    }
    console.log('Please answer yes or no.')
  }
}

function parseCliArguments(args) {
  let positional
  let answersFile
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index]
    if (!argument) {
      continue
    }
    if (argument === '--answers-file') {
      answersFile = args[index + 1]
      if (!answersFile) {
        fail('--answers-file requires a path')
      }
      index += 1
      continue
    }
    if (argument.startsWith('-')) {
      fail(`unknown option '${argument}'`)
    }
    if (positional) {
      fail('only one optional project slug may be provided')
    }
    positional = argument
  }
  return { positional, answersFile }
}

function readAnswersFile(path, root) {
  const absolutePath = isAbsolute(path) ? path : resolve(root, path)
  try {
    return JSON.parse(readFileSync(absolutePath, 'utf8'))
  } catch (error) {
    fail(`unable to read answers file '${path}': ${error.message}`)
  }
}

async function collectInitAnswers(args, root, state) {
  const { positional, answersFile } = parseCliArguments(args)
  if (answersFile) {
    if (positional) {
      fail('a positional slug cannot be combined with --answers-file')
    }
    const answers = readAnswersFile(answersFile, root)
    if (answers.confirm !== true) {
      fail('non-interactive answers must contain "confirm": true')
    }
    return {
      projectSlug: answers.projectSlug,
      displayName: answers.displayName,
      githubRepository: parseGitHubRepository(answers.githubRepository),
      keepExamples: answers.keepExamples !== false,
      runCheck: answers.runCheck === true,
      confirmed: true,
    }
  }

  if (!process.stdin.isTTY || !process.stdout.isTTY) {
    fail('interactive initialization requires a TTY; automation must use --answers-file')
  }

  const interface_ = createInterface({ input: process.stdin, output: process.stdout })
  try {
    const directoryDefault = basename(root)
    let slugDefault = positional
    if (!slugDefault) {
      try {
        validateProjectSlug(directoryDefault, state.identity)
        slugDefault = directoryDefault
      } catch {
        slugDefault = undefined
      }
    }

    const projectSlug = await askText(interface_, 'Project slug', slugDefault)
    validateProjectSlug(projectSlug, state.identity)
    const displayName = await askText(
      interface_,
      'Display name',
      defaultDisplayName(projectSlug),
    )
    validateDisplayName(displayName, state.identity)
    const repositoryDefault = detectedGitHubRepository(root, state.identity)
    const githubRepository = parseGitHubRepository(
      await askText(interface_, 'GitHub owner/repository', repositoryDefault),
    )
    const keepExamples = await askYesNo(interface_, 'Keep items/users example resources?', true)
    const runCheck = await askYesNo(interface_, 'Run just check after initialization?', true)
    const target = deriveIdentity(projectSlug, displayName, githubRepository)
    validateTargetIdentity(target, state.identity)
    printSummary(state, target, keepExamples, runCheck, root)
    const confirmed = await askYesNo(interface_, 'Proceed?', false)
    return { projectSlug, displayName, githubRepository, keepExamples, runCheck, confirmed }
  } finally {
    interface_.close()
  }
}

async function collectStripAnswers(args, root) {
  const { positional, answersFile } = parseCliArguments(args)
  if (positional) {
    fail('strip-examples does not accept a positional argument')
  }
  if (answersFile) {
    const answers = readAnswersFile(answersFile, root)
    if (answers.confirm !== true) {
      fail('non-interactive answers must contain "confirm": true')
    }
    return { confirmed: true, runCheck: answers.runCheck === true }
  }
  if (!process.stdin.isTTY || !process.stdout.isTTY) {
    fail('interactive cleanup requires a TTY; automation must use --answers-file')
  }

  const interface_ = createInterface({ input: process.stdin, output: process.stdout })
  try {
    const runCheck = await askYesNo(interface_, 'Run just check after cleanup?', true)
    printStripSummary(runCheck, root)
    const confirmed = await askYesNo(interface_, 'Proceed?', false)
    return { confirmed, runCheck }
  } finally {
    interface_.close()
  }
}

function runCheck(root) {
  console.log('\nRunning just check...')
  command('just', ['check'], root, { inherit: true })
}

export async function runInitCli(args, root = PROJECT_ROOT) {
  root = repositoryRoot(root)
  const state = readState(root)
  if (state.status === 'initialized') {
    console.log(`Project '${state.identity.displayName}' is already initialized.`)
    return
  }
  requireCleanWorktree(root)

  const answers = await collectInitAnswers(args, root, state)
  if (!answers.confirmed) {
    console.log('Initialization cancelled; no files were changed.')
    return
  }
  const target = deriveIdentity(
    answers.projectSlug,
    answers.displayName,
    answers.githubRepository,
  )
  validateTargetIdentity(target, state.identity)

  const before = snapshotTrackedTree(root)
  const after = planInitialization(before, state, target, answers.keepExamples)
  const expectedState = {
    schemaVersion: STATE_SCHEMA_VERSION,
    status: 'initialized',
    identity: target,
    examples: answers.keepExamples ? 'present' : 'stripped',
  }
  const changes = writeTreeTransaction(root, before, after, () =>
    validateWrittenProject(root, expectedState),
  )
  console.log(`Initialized '${target.displayName}' across ${changes.length} tracked files.`)
  console.log('No Git remote, commit, or ignored local data was changed.')
  if (answers.runCheck) {
    runCheck(root)
  }
  console.log('Docker verification: just docker-build')
}

export async function runStripExamplesCli(args, root = PROJECT_ROOT) {
  root = repositoryRoot(root)
  const state = readState(root)
  if (state.status !== 'initialized') {
    fail('initialize the project first and choose example cleanup in the initialization wizard')
  }
  if (state.examples === 'stripped') {
    console.log('Example resources are already stripped; no files were changed.')
    return
  }
  requireCleanWorktree(root)

  const answers = await collectStripAnswers(args, root)
  if (!answers.confirmed) {
    console.log('Example cleanup cancelled; no files were changed.')
    return
  }

  const before = snapshotTrackedTree(root)
  const after = planExampleCleanup(before, state)
  const expectedState = { ...state, examples: 'stripped' }
  const changes = writeTreeTransaction(root, before, after, () =>
    validateWrittenProject(root, expectedState),
  )
  console.log(`Removed example resources across ${changes.length} tracked files.`)
  console.log('Existing SQLite and PostgreSQL data was not changed.')
  if (answers.runCheck) {
    runCheck(root)
  }
}

export const testing = {
  EXAMPLE_FILES,
  PROJECT_ROOT,
  STATE_PATH,
  TEMPLATE_ONLY_FILES,
}
