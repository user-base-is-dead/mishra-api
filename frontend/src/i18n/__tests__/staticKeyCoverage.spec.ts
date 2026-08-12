import { describe, expect, it } from 'vitest'
import { readFileSync, globSync } from 'node:fs'
import { resolve } from 'node:path'

import en from '../locales/en'
import zh from '../locales/zh'

// Missing i18n keys do not throw — vue-i18n renders the raw key path, which
// shipped visible strings like "landing.features.billingTitle" to production.
// This test fails instead, for every static t('a.b') literal in the app.
const projectRoot = resolve(__dirname, '../../..')

function lookup(messages: Record<string, unknown>, path: string): unknown {
  return path
    .split('.')
    .reduce<any>((node, part) => (node == null ? undefined : node[part]), messages)
}

function staticKeysByFile(): Map<string, string[]> {
  const files = globSync('src/**/*.vue', { cwd: projectRoot }) as string[]
  const out = new Map<string, string[]>()
  for (const file of files) {
    const source = readFileSync(resolve(projectRoot, file), 'utf8')
    const keys = new Set(
      (source.match(/t\('([a-zA-Z0-9_]+(?:\.[a-zA-Z0-9_]+)+)'\)/g) ?? []).map((m) => m.slice(3, -2))
    )
    if (keys.size) out.set(file.replace(/\\/g, '/'), [...keys])
  }
  return out
}

describe('i18n static key coverage', () => {
  const byFile = staticKeysByFile()

  it('scans the component tree', () => {
    expect(byFile.size).toBeGreaterThan(50)
  })

  it.each([
    ['en', en],
    ['zh', zh]
  ])('%s resolves every static t() key used in .vue files', (_locale, messages) => {
    const missing: string[] = []
    for (const [file, keys] of byFile) {
      for (const key of keys) {
        if (typeof lookup(messages as Record<string, unknown>, key) !== 'string') {
          missing.push(`${file} -> ${key}`)
        }
      }
    }
    expect(missing.sort()).toEqual([])
  })
})
