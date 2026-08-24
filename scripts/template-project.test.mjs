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
const PRODUCT_CARGO_TARGET = join(
  testing.PROJECT_ROOT,
  'target/template-product-check',
)
const RETAINED_TEMPLATE_NOTICE =
  'This repository starts as a project template.'
const GENERATED_PROJECT_RESIDUALS = [
  'application generated from the template',
  'This template deliberately',
  'This template has not published',
  'Rust + Vue template',
  'The template keeps separate schema modules',
  'the template works without Diesel CLI',
  'The initializer uses an equivalent path spelling',
  'file:template?mode=memory&cache=shared',
  'Project identity or initialization guidance',
]

function run(commandName, args, options = {}) {
  const environment = { ...process.env, ...options.env }
  delete environment.NODE_TEST_CONTEXT
  const result = spawnSync(commandName, args, {
    cwd: options.cwd,
    encoding: 'utf8',
    stdio: options.inherit ? 'inherit' : ['ignore', 'pipe', 'pipe'],
    env: environment,
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

function repositoryFiles(root = testing.PROJECT_ROOT) {
  return run(
    'git',
    [
      '-C',
      root,
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

function commandOutput(result) {
  const output = [result.stdout, result.stderr].filter(Boolean).join('\n')
  return output || JSON.stringify({ status: result.status, signal: result.signal })
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
  assert.equal(existsSync(join(fixture.repository, testing.STATE_PATH)), false)
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
  assert.equal(workflow.includes('DEV_TEMPLATE_INIT_ANSWERS'), false)
  assert.equal(workflow.includes('Test template initialization contract'), false)
  assert.equal(
    readFileSync(join(fixture.repository, 'front/index.html'), 'utf8').includes(
      `<title>${expected.displayName}</title>`,
    ),
    true,
  )

  const app = readFileSync(join(fixture.repository, 'server/src/app.rs'), 'utf8')
  assert.equal(app.includes('template-example:'), false)
  assert.equal(app.includes('/api/items'), false)
  assert.equal(existsSync(join(fixture.repository, 'front/src/pages/Items.vue')), false)
  assert.equal(existsSync(join(fixture.repository, 'front/tests/Items.test.ts')), false)
  assert.equal(existsSync(join(fixture.repository, 'front/tests/services.test.ts')), false)
  assert.equal(existsSync(join(fixture.repository, 'front/tests/Users.test.ts')), false)
  assert.equal(existsSync(join(fixture.repository, 'front/e2e/items.spec.ts')), false)
  assert.equal(existsSync(join(fixture.repository, 'front/e2e/dashboard.spec.ts')), true)
  assert.equal(
    existsSync(join(fixture.repository, 'server/migrations/sqlite/.gitkeep')),
    true,
  )
  assert.equal(
    existsSync(join(fixture.repository, 'server/migrations/postgres/.gitkeep')),
    true,
  )

  const justfile = readFileSync(join(fixture.repository, 'justfile'), 'utf8')
  assert.equal(justfile.includes('\ninit '), false)
  assert.equal(justfile.includes('test-template-init'), false)

  const readme = readFileSync(join(fixture.repository, 'README.md'), 'utf8')
  assert.equal(readme.includes(RETAINED_TEMPLATE_NOTICE), true)
  assert.equal(readme.includes('run `just init`'), true)
  assert.equal(readme.includes('you may delete this notice'), true)
  assert.deepEqual(
    trackedTextResiduals(fixture.repository, GENERATED_PROJECT_RESIDUALS),
    [],
  )

  for (const path of repositoryFiles(fixture.repository)) {
    const absolutePath = join(fixture.repository, path)
    if (!existsSync(absolutePath)) {
      continue
    }
    const buffer = readFileSync(absolutePath)
    if (buffer.includes(0)) {
      continue
    }
    const text = buffer.toString('utf8')
    assert.equal(text.includes('template-init:'), false, path)
    assert.equal(text.includes('template-example:'), false, path)
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

test('keeps the source manifest identity-only', () => {
  assert.deepEqual(
    JSON.parse(readFileSync(join(testing.PROJECT_ROOT, testing.STATE_PATH), 'utf8')),
    { identity: SOURCE_IDENTITY },
  )
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

test('initializes one clean project and removes all template lifecycle files', () => {
  const fixture = createFixture('sample-service')
  const productImage = `cyder-template:c3-product-${process.pid}`
  try {
    const answers = {
      projectSlug: 'sample-service',
      displayName: 'Sample Service',
      githubRepository: 'acme/sample-service',
      runCheck: false,
    }
    const result = initializeFixture(fixture, answers)
    assert.equal(result.status, 0, result.stderr)
    assertInitializedFixture(fixture, answers)

    if (
      process.env.DEV_TEMPLATE_PRODUCT_CHECK === '1' ||
      process.env.DEV_TEMPLATE_CONTAINER_CHECK === '1'
    ) {
      run('just', ['check'], {
        cwd: fixture.repository,
        env: { CARGO_TARGET_DIR: PRODUCT_CARGO_TARGET },
      })
      run('docker', ['compose', '-f', 'docker-compose.yml', 'config'], {
        cwd: fixture.repository,
      })
    }

    if (process.env.DEV_TEMPLATE_CONTAINER_CHECK === '1') {
      run('docker', ['build', '--quiet', '-t', productImage, '-f', 'Dockerfile', '.'], {
        cwd: fixture.repository,
        inherit: true,
      })
      run('bash', ['scripts/test-container-e2e.sh', productImage], {
        cwd: fixture.repository,
        inherit: true,
      })
    }

    const repeat = run(process.execPath, ['scripts/init-project.mjs'], {
      cwd: fixture.repository,
      allowFailure: true,
    })
    assert.notEqual(repeat.status, 0)
    assert.match(repeat.stderr, /MODULE_NOT_FOUND/)
  } finally {
    if (process.env.DEV_TEMPLATE_CONTAINER_CHECK === '1') {
      run('docker', ['image', 'rm', '--force', productImage], {
        cwd: fixture.repository,
        allowFailure: true,
      })
    }
    fixture.cleanup()
  }
})

test('rejects unsupported non-interactive answer fields', () => {
  for (const unsupported of ['unexpectedOption', 'futureOption']) {
    const fixture = createFixture(`unsupported-${unsupported.toLowerCase()}`)
    try {
      const result = initializeFixture(fixture, {
        projectSlug: 'strict-service',
        displayName: 'Strict Service',
        githubRepository: 'acme/strict-service',
        runCheck: false,
        [unsupported]: true,
      })
      assert.notEqual(result.status, 0)
      assert.match(commandOutput(result), new RegExp(unsupported))
      assert.equal(run('git', ['status', '--porcelain'], { cwd: fixture.repository }).stdout, '')
    } finally {
      fixture.cleanup()
    }
  }
})

test('rejects invalid answers without changing the worktree', () => {
  const fixture = createFixture('invalid-service')
  try {
    const result = initializeFixture(fixture, {
      projectSlug: 'Invalid Service',
      displayName: 'Invalid Service',
      githubRepository: 'acme/invalid-service',
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
      runCheck: false,
    })
    assert.notEqual(result.status, 0)
    assert.match(commandOutput(result), /worktree must be clean/)
    assert.equal(existsSync(join(fixture.repository, testing.STATE_PATH)), true)
  } finally {
    fixture.cleanup()
  }
})

test('rejects every local artifact path before initialization', () => {
  for (const path of testing.BLOCKING_LOCAL_PATHS) {
    const fixture = createFixture(`artifact-${path.replaceAll('/', '-')}`)
    try {
      mkdirSync(join(fixture.repository, path), { recursive: true })
      const result = initializeFixture(fixture, {
        projectSlug: 'artifact-service',
        displayName: 'Artifact Service',
        githubRepository: 'acme/artifact-service',
        runCheck: false,
      })
      assert.notEqual(result.status, 0)
      assert.match(commandOutput(result), /clean template checkout/)
      assert.match(commandOutput(result), new RegExp(path.replaceAll('/', '\\/')))
      assert.equal(existsSync(join(fixture.repository, testing.STATE_PATH)), true)
    } finally {
      fixture.cleanup()
    }
  }
})

test('rejects drifted marker structure without partial writes', () => {
  const fixture = createFixture('drifted-service')
  try {
    const readmePath = join(fixture.repository, 'README.md')
    const readme = readFileSync(readmePath, 'utf8')
    const marker = '<!-- template-init:end -->'
    const markerIndex = readme.lastIndexOf(marker)
    assert.notEqual(markerIndex, -1)
    writeFileSync(
      readmePath,
      `${readme.slice(0, markerIndex)}${readme.slice(markerIndex + marker.length)}`,
    )
    run('git', ['add', 'README.md'], { cwd: fixture.repository })
    run('git', ['commit', '-m', 'drift marker'], { cwd: fixture.repository })

    const result = initializeFixture(fixture, {
      projectSlug: 'drifted-service',
      displayName: 'Drifted Service',
      githubRepository: 'acme/drifted-service',
      runCheck: false,
    })
    assert.notEqual(result.status, 0)
    assert.match(commandOutput(result), /matching end/)
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
      runCheck: false,
    })
    assert.notEqual(result.status, 0)
    assert.match(commandOutput(result), /all project files were restored/)
    assert.equal(run('git', ['status', '--porcelain'], { cwd: fixture.repository }).stdout, '')
    assert.deepEqual(
      JSON.parse(readFileSync(join(fixture.repository, testing.STATE_PATH), 'utf8')),
      { identity: SOURCE_IDENTITY },
    )
  } finally {
    fixture.cleanup()
  }
})
