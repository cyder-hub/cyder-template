import { readdir, readFile, rm, writeFile } from 'node:fs/promises'
import { extname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'
import {
  brotliCompress as brotliCompressCallback,
  constants,
  gzip as gzipCallback,
} from 'node:zlib'

const brotliCompress = promisify(brotliCompressCallback)
const gzip = promisify(gzipCallback)

export const MINIMUM_SOURCE_BYTES = 1024
export const COMPRESSIBLE_EXTENSIONS = new Set([
  '.css',
  '.html',
  '.js',
  '.json',
  '.mjs',
  '.svg',
  '.txt',
  '.wasm',
  '.xml',
])

export async function precompressDirectory(directory) {
  const root = resolve(directory)
  const files = await collectFiles(root)
  let brotliFiles = 0
  let gzipFiles = 0

  for (const file of files) {
    const extension = extname(file).toLowerCase()
    if (!COMPRESSIBLE_EXTENSIONS.has(extension)) {
      continue
    }

    const brotliPath = `${file}.br`
    const gzipPath = `${file}.gz`
    await Promise.all([rm(brotliPath, { force: true }), rm(gzipPath, { force: true })])

    const source = await readFile(file)
    if (source.byteLength < MINIMUM_SOURCE_BYTES) {
      continue
    }

    const [brotli, gzipped] = await Promise.all([
      brotliCompress(source, {
        params: {
          [constants.BROTLI_PARAM_QUALITY]: 11,
        },
      }),
      gzip(source, { level: 9 }),
    ])

    if (brotli.byteLength < source.byteLength) {
      await writeFile(brotliPath, brotli)
      brotliFiles += 1
    }
    if (gzipped.byteLength < source.byteLength) {
      await writeFile(gzipPath, gzipped)
      gzipFiles += 1
    }
  }

  return { brotliFiles, gzipFiles }
}

async function collectFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true })
  const files = []
  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
    const path = resolve(directory, entry.name)
    if (entry.isDirectory()) {
      files.push(...(await collectFiles(path)))
    } else if (entry.isFile() && !entry.name.endsWith('.br') && !entry.name.endsWith('.gz')) {
      files.push(path)
    }
  }
  return files
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : null
if (invokedPath === fileURLToPath(import.meta.url)) {
  const directory = process.argv[2]
  if (!directory) {
    console.error('usage: node scripts/precompress.mjs <directory>')
    process.exitCode = 2
  } else {
    const result = await precompressDirectory(directory)
    console.log(
      `precompressed frontend assets: brotli=${result.brotliFiles} gzip=${result.gzipFiles}`,
    )
  }
}
