import { readFile } from 'node:fs/promises'
import { pathToFileURL } from 'node:url'

export const FILE_COVERAGE_MINIMUM = 60

const METRICS = ['branches', 'functions', 'lines', 'statements']

export function checkFileCoverage(summary, minimum = FILE_COVERAGE_MINIMUM) {
  if (!summary || typeof summary !== 'object' || Array.isArray(summary)) {
    throw new Error('coverage summary must be a JSON object')
  }
  if (!Number.isFinite(minimum) || minimum < 0 || minimum > 100) {
    throw new Error('per-file coverage minimum must be between 0 and 100')
  }

  const files = Object.entries(summary).filter(([name]) => name !== 'total')
  if (files.length === 0) {
    throw new Error('coverage summary does not contain any files')
  }

  const failures = []
  for (const [file, report] of files) {
    if (!report || typeof report !== 'object' || Array.isArray(report)) {
      throw new Error(`coverage summary for ${file} must be a JSON object`)
    }
    for (const metric of METRICS) {
      const result = report[metric]
      if (!result || typeof result !== 'object' || typeof result.pct !== 'number') {
        throw new Error(`coverage summary for ${file} is missing ${metric}.pct`)
      }
      if (result.pct < minimum) {
        failures.push(`${file}: ${metric} ${String(result.pct)}%`)
      }
    }
  }

  if (failures.length > 0) {
    throw new Error(
      `per-file coverage must be at least ${String(minimum)}%:\n${failures.join('\n')}`,
    )
  }

  return files.length
}

export async function checkCoverageFile(path) {
  const summary = JSON.parse(await readFile(path, 'utf8'))
  return checkFileCoverage(summary)
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const path = process.argv[2]
  if (!path) {
    console.error('usage: node scripts/check-coverage.mjs <coverage-summary.json>')
    process.exitCode = 2
  } else {
    try {
      const fileCount = await checkCoverageFile(path)
      console.log(
        `per-file coverage passed for ${String(fileCount)} files at ${String(FILE_COVERAGE_MINIMUM)}%`,
      )
    } catch (source) {
      console.error(source instanceof Error ? source.message : String(source))
      process.exitCode = 1
    }
  }
}
