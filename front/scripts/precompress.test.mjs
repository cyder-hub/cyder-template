import assert from 'node:assert/strict'
import { randomBytes } from 'node:crypto'
import { mkdtemp, readFile, rm, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { promisify } from 'node:util'
import { brotliDecompress as brotliDecompressCallback, gunzip as gunzipCallback } from 'node:zlib'
import test from 'node:test'

import { precompressDirectory } from './precompress.mjs'

const brotliDecompress = promisify(brotliDecompressCallback)
const gunzip = promisify(gunzipCallback)

test('precompresses eligible assets deterministically and removes stale sidecars', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'cyder-precompress-'))
  try {
    const source = Buffer.from('export const value = "compressible frontend content";\n'.repeat(80))
    const script = join(directory, 'application.js')
    const small = join(directory, 'small.css')
    const image = join(directory, 'image.png')
    const random = join(directory, 'random.txt')
    await Promise.all([
      writeFile(script, source),
      writeFile(small, 'body {}'),
      writeFile(`${small}.br`, 'stale'),
      writeFile(`${small}.gz`, 'stale'),
      writeFile(image, source),
      writeFile(random, randomBytes(4096)),
    ])

    const first = await precompressDirectory(directory)
    assert.deepEqual(first, { brotliFiles: 1, gzipFiles: 1 })
    await assert.rejects(stat(`${small}.br`), { code: 'ENOENT' })
    await assert.rejects(stat(`${small}.gz`), { code: 'ENOENT' })
    await assert.rejects(stat(`${image}.br`), { code: 'ENOENT' })
    await assert.rejects(stat(`${random}.br`), { code: 'ENOENT' })

    const firstBrotli = await readFile(`${script}.br`)
    const firstGzip = await readFile(`${script}.gz`)
    assert.deepEqual(await brotliDecompress(firstBrotli), source)
    assert.deepEqual(await gunzip(firstGzip), source)

    const second = await precompressDirectory(directory)
    assert.deepEqual(second, first)
    assert.deepEqual(await readFile(`${script}.br`), firstBrotli)
    assert.deepEqual(await readFile(`${script}.gz`), firstGzip)
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
})
