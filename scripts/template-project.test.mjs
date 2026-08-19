import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import test from 'node:test'

import {
  defaultDisplayName,
  deriveIdentity,
  parseGitHubRepository,
  removeMarkedBlocks,
  testing,
  validateDisplayName,
  validateProjectSlug,
} from './template-project.mjs'

const SOURCE_IDENTITY = {
  projectSlug: 'cyder-template',
  rustIdentifier: 'cyder_template',
  frontendPackage: 'cyder-template-front',
  displayName: 'cyder-template',
  githubRepository: 'cyder-hub/cyder-template',
}
const INTEGRATION_CARGO_TARGET = join(
  testing.PROJECT_ROOT,
  'target/template-project-integration/shared',
)

function run(commandName, args, options = {}) {
  const result = spawnSync(commandName, args, {
    cwd: options.cwd,
    encoding: 'utf8',
    stdio: options.inherit ? 'inherit' : ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, ...options.env },
  })
  if (result.error) {
    throw result.error
  }
  if (!options.allowFailure && result.status !== 0) {
    const output = [result.stdout, result.stderr].filter(Boolean).join('\n').trim()
    assert.fail(
      `${commandName} ${args.join(' ')} failed: ${output}`,
    )
  }
  return result
}

function repositoryFiles() {
  return run(
    'git',
    [
      '-C',
      testing.PROJECT_ROOT,
      'ls-files',
      '--cached',
      '--others',
      '--exclude-standard',
      '-z',
    ],
  )
    .stdout.split('\0')
    .filter(Boolean)
}

function createFixture(directoryName) {
  const temporaryDirectory = mkdtempSync(join(tmpdir(), 'template-project-test-'))
  const repository = join(temporaryDirectory, directoryName)
  mkdirSync(repository, { recursive: true })

  for (const path of repositoryFiles()) {
    const source = join(testing.PROJECT_ROOT, path)
    if (!existsSync(source) || !statSync(source).isFile()) {
      continue
    }
    const destination = join(repository, path)
    mkdirSync(dirname(destination), { recursive: true })
    copyFileSync(source, destination)
    chmodSync(destination, statSync(source).mode)
  }

  run('git', ['init', '--initial-branch=main'], { cwd: repository })
  run('git', ['config', 'user.name', 'Template Test'], { cwd: repository })
  run('git', ['config', 'user.email', 'template-test@example.test'], { cwd: repository })
  run('git', ['add', '--all'], { cwd: repository })
  run('git', ['commit', '-m', 'test fixture'], { cwd: repository })
  run('git', ['remote', 'add', 'origin', `https://github.com/acme/${directoryName}.git`], {
    cwd: repository,
  })

  return {
    repository,
    temporaryDirectory,
    cleanup() {
      rmSync(temporaryDirectory, { recursive: true, force: true })
    },
  }
}

function initializeFixture(fixture, answers) {
  const answersPath = join(fixture.temporaryDirectory, 'answers.json')
  writeFileSync(answersPath, `${JSON.stringify({ ...answers, confirm: true }, null, 2)}\n`)
  return run(
    process.execPath,
    ['scripts/init-project.mjs', '--answers-file', answersPath],
    { cwd: fixture.repository, allowFailure: true },
  )
}

function stripFixture(fixture, answers = {}) {
  const answersPath = join(fixture.temporaryDirectory, 'strip-answers.json')
  writeFileSync(
    answersPath,
    `${JSON.stringify({ runCheck: false, ...answers, confirm: true }, null, 2)}\n`,
  )
  return run(
    process.execPath,
    ['scripts/strip-examples.mjs', '--answers-file', answersPath],
    { cwd: fixture.repository, allowFailure: true },
  )
}

function trackedTextResiduals(repository, tokens) {
  const paths = run('git', ['ls-files', '-z'], { cwd: repository })
    .stdout.split('\0')
    .filter(Boolean)
  const residuals = []
  for (const path of paths) {
    const absolutePath = join(repository, path)
    if (!existsSync(absolutePath)) {
      continue
    }
    const buffer = readFileSync(absolutePath)
    if (buffer.includes(0)) {
      continue
    }
    const text = buffer.toString('utf8')
    for (const token of tokens) {
      if (text.includes(token)) {
        residuals.push(`${path}: ${token}`)
      }
    }
  }
  return residuals
}

function assertInitializedFixture(fixture, expected) {
  const state = JSON.parse(readFileSync(join(fixture.repository, testing.STATE_PATH), 'utf8'))
  assert.equal(state.status, 'initialized')
  assert.equal(state.examples, expected.keepExamples ? 'present' : 'stripped')
  assert.deepEqual(
    state.identity,
    deriveIdentity(expected.projectSlug, expected.displayName, expected.githubRepository),
  )
  assert.deepEqual(
    trackedTextResiduals(fixture.repository, [
      SOURCE_IDENTITY.projectSlug,
      SOURCE_IDENTITY.rustIdentifier,
      SOURCE_IDENTITY.frontendPackage,
    ]),
    [],
  )
  for (const path of testing.TEMPLATE_ONLY_FILES) {
    assert.equal(existsSync(join(fixture.repository, path)), false, path)
  }
  const workflow = readFileSync(
    join(fixture.repository, '.github/workflows/ci.yml'),
    'utf8',
  )
  assert.equal(workflow.includes('template-init:'), false)
  assert.equal(workflow.includes('TEMPLATE_INIT_ANSWERS'), false)
  assert.equal(workflow.includes('Test template initialization contract'), false)
  assert.equal(
    readFileSync(join(fixture.repository, 'front/index.html'), 'utf8').includes(
      `<title>${expected.displayName}</title>`,
    ),
    true,
  )

  const app = readFileSync(join(fixture.repository, 'server/src/app.rs'), 'utf8')
  if (expected.keepExamples) {
    assert.equal(app.includes('template-example:start'), true)
    assert.equal(existsSync(join(fixture.repository, 'front/src/pages/Items.vue')), true)
  } else {
    assert.equal(app.includes('template-example:'), false)
    assert.equal(app.includes('/api/items'), false)
    assert.equal(existsSync(join(fixture.repository, 'front/src/pages/Items.vue')), false)
    assert.equal(
      existsSync(join(fixture.repository, 'server/migrations/sqlite/.gitkeep')),
      true,
    )
  }

  const diffCheck = run('git', ['diff', '--check'], {
    cwd: fixture.repository,
    allowFailure: true,
  })
  assert.equal(diffCheck.status, 0, `${diffCheck.stderr}\n${diffCheck.stdout}`)
}

test('derives every machine identity from one slug', () => {
  assert.deepEqual(deriveIdentity('my-api', 'My API', 'acme/my-api'), {
    projectSlug: 'my-api',
    rustIdentifier: 'my_api',
    frontendPackage: 'my-api-front',
    displayName: 'My API',
    githubRepository: 'acme/my-api',
  })
  assert.equal(defaultDisplayName('my-api'), 'My Api')
})

test('validates slugs and display names conservatively', () => {
  assert.doesNotThrow(() => validateProjectSlug('my-api', SOURCE_IDENTITY))
  assert.throws(() => validateProjectSlug('My API', SOURCE_IDENTITY), /kebab-case/)
  assert.throws(() => validateProjectSlug('cyder-template-api', SOURCE_IDENTITY), /source/)
  assert.throws(() => validateProjectSlug('con', SOURCE_IDENTITY), /reserved/)
  assert.doesNotThrow(() => validateDisplayName('我的 API', SOURCE_IDENTITY))
  assert.throws(() => validateDisplayName('Bad\nName', SOURCE_IDENTITY), /control/)
})

test('normalizes supported GitHub repository forms', () => {
  assert.equal(parseGitHubRepository('acme/my-api'), 'acme/my-api')
  assert.equal(parseGitHubRepository('https://github.com/acme/my-api.git'), 'acme/my-api')
  assert.equal(parseGitHubRepository('git@github.com:acme/my-api.git'), 'acme/my-api')
  assert.throws(() => parseGitHubRepository('https://example.com/acme/my-api'), /github.com/)
})

test('removes complete marker blocks and preserves CRLF', () => {
  const result = removeMarkedBlocks(
    'before\r\n// template-example:start\r\nremove\r\n// template-example:end\r\nafter\r\n',
    'template-example',
  )
  assert.equal(result.count, 1)
  assert.equal(result.text, 'before\r\nafter\r\n')
  assert.throws(
    () => removeMarkedBlocks('// template-example:start\nmissing end\n', 'template-example'),
    /matching end/,
  )
})

test('initializes a renamed project while keeping examples', () => {
  const fixture = createFixture('sample-service')
  try {
    const answers = {
      projectSlug: 'sample-service',
      displayName: 'Sample Service',
      githubRepository: 'acme/sample-service',
      keepExamples: true,
      runCheck: false,
    }
    const result = initializeFixture(fixture, answers)
    assert.equal(result.status, 0, result.stderr)
    assertInitializedFixture(fixture, answers)

    const repeat = run(process.execPath, ['scripts/init-project.mjs'], {
      cwd: fixture.repository,
      allowFailure: true,
    })
    assert.equal(repeat.status, 0, repeat.stderr)
    assert.match(repeat.stdout, /already initialized/)

    if (process.env.TEMPLATE_PROJECT_INTEGRATION === '1') {
      run('just', ['bootstrap'], { cwd: fixture.repository })
      run('just', ['check'], {
        cwd: fixture.repository,
        env: { CARGO_TARGET_DIR: INTEGRATION_CARGO_TARGET },
      })
      run('docker', ['compose', '-f', 'docker-compose.yml', 'config'], {
        cwd: fixture.repository,
      })
    }
  } finally {
    fixture.cleanup()
  }
})

test('initializes a renamed project and strips examples', () => {
  const fixture = createFixture('bare-service')
  const image = `template-init-stripped:${process.pid}`
  try {
    const answers = {
      projectSlug: 'bare-service',
      displayName: 'Bare Service',
      githubRepository: 'acme/bare-service',
      keepExamples: false,
      runCheck: false,
    }
    const result = initializeFixture(fixture, answers)
    assert.equal(result.status, 0, result.stderr)
    assertInitializedFixture(fixture, answers)

    if (process.env.TEMPLATE_PROJECT_INTEGRATION === '1') {
      run('just', ['bootstrap'], { cwd: fixture.repository })
      run('just', ['check'], {
        cwd: fixture.repository,
        env: { CARGO_TARGET_DIR: INTEGRATION_CARGO_TARGET },
      })
      run('docker', ['compose', '-f', 'docker-compose.yml', 'config'], {
        cwd: fixture.repository,
      })
      run('docker', ['build', '-t', image, '-f', 'Dockerfile', '.'], {
        cwd: fixture.repository,
      })
      run('bash', ['scripts/test-container-shutdown.sh', image], {
        cwd: fixture.repository,
      })
    }
  } finally {
    if (process.env.TEMPLATE_PROJECT_INTEGRATION === '1') {
      run('docker', ['image', 'rm', '--force', image], { allowFailure: true })
    }
    fixture.cleanup()
  }
})

test('strips examples later from a clean initialized project', () => {
  const fixture = createFixture('later-service')
  try {
    const initialized = initializeFixture(fixture, {
      projectSlug: 'later-service',
      displayName: 'Later Service',
      githubRepository: 'acme/later-service',
      keepExamples: true,
      runCheck: false,
    })
    assert.equal(initialized.status, 0, initialized.stderr)
    run('git', ['add', '--all'], { cwd: fixture.repository })
    run('git', ['commit', '-m', 'initialize project'], { cwd: fixture.repository })

    const stripped = stripFixture(fixture)
    assert.equal(stripped.status, 0, stripped.stderr)
    const state = JSON.parse(readFileSync(join(fixture.repository, testing.STATE_PATH), 'utf8'))
    assert.equal(state.examples, 'stripped')
    assert.equal(existsSync(join(fixture.repository, 'front/src/pages/Items.vue')), false)

    const repeat = run(process.execPath, ['scripts/strip-examples.mjs'], {
      cwd: fixture.repository,
      allowFailure: true,
    })
    assert.equal(repeat.status, 0, repeat.stderr)
    assert.match(repeat.stdout, /already stripped/)
  } finally {
    fixture.cleanup()
  }
})

test('rejects invalid answers without changing the worktree', () => {
  const fixture = createFixture('invalid-service')
  try {
    const result = initializeFixture(fixture, {
      projectSlug: 'Invalid Service',
      displayName: 'Invalid Service',
      githubRepository: 'acme/invalid-service',
      keepExamples: true,
      runCheck: false,
    })
    assert.notEqual(result.status, 0)
    assert.equal(run('git', ['status', '--porcelain'], { cwd: fixture.repository }).stdout, '')
  } finally {
    fixture.cleanup()
  }
})

test('rejects a dirty worktree before reading initialization answers', () => {
  const fixture = createFixture('dirty-service')
  try {
    writeFileSync(join(fixture.repository, 'untracked.txt'), 'do not overwrite\n')
    const result = initializeFixture(fixture, {
      projectSlug: 'dirty-service',
      displayName: 'Dirty Service',
      githubRepository: 'acme/dirty-service',
      keepExamples: true,
      runCheck: false,
    })
    assert.notEqual(result.status, 0)
    assert.match(result.stderr, /worktree must be clean/)
    assert.equal(
      JSON.parse(readFileSync(join(fixture.repository, testing.STATE_PATH), 'utf8')).status,
      'template',
    )
  } finally {
    fixture.cleanup()
  }
})

test('rejects drifted marker structure without partial writes', () => {
  const fixture = createFixture('drifted-service')
  try {
    const readmePath = join(fixture.repository, 'README.md')
    const readme = readFileSync(readmePath, 'utf8').replace('<!-- template-init:end -->', '')
    writeFileSync(readmePath, readme)
    run('git', ['add', 'README.md'], { cwd: fixture.repository })
    run('git', ['commit', '-m', 'drift marker'], { cwd: fixture.repository })

    const result = initializeFixture(fixture, {
      projectSlug: 'drifted-service',
      displayName: 'Drifted Service',
      githubRepository: 'acme/drifted-service',
      keepExamples: true,
      runCheck: false,
    })
    assert.notEqual(result.status, 0)
    assert.match(result.stderr, /matching end/)
    assert.equal(run('git', ['status', '--porcelain'], { cwd: fixture.repository }).stdout, '')
  } finally {
    fixture.cleanup()
  }
})

test('rolls back every write when post-write validation fails', () => {
  const fixture = createFixture('rollback-service')
  try {
    const manifestPath = join(fixture.repository, 'server/Cargo.toml')
    writeFileSync(manifestPath, `${readFileSync(manifestPath, 'utf8')}\ninvalid cargo syntax\n`)
    run('git', ['add', 'server/Cargo.toml'], { cwd: fixture.repository })
    run('git', ['commit', '-m', 'break manifest for rollback test'], { cwd: fixture.repository })

    const result = initializeFixture(fixture, {
      projectSlug: 'rollback-service',
      displayName: 'Rollback Service',
      githubRepository: 'acme/rollback-service',
      keepExamples: true,
      runCheck: false,
    })
    assert.notEqual(result.status, 0)
    assert.match(result.stderr, /all project files were restored/)
    assert.equal(run('git', ['status', '--porcelain'], { cwd: fixture.repository }).stdout, '')
    assert.equal(
      JSON.parse(readFileSync(join(fixture.repository, testing.STATE_PATH), 'utf8')).status,
      'template',
    )
  } finally {
    fixture.cleanup()
  }
})
