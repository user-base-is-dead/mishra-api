import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import en from '../locales/en'
import zh from '../locales/zh'

// HomeView.vue renders the marketing page purely from the `landing.*` namespace.
// A missing key shows up as raw text like "landing.features.billingTitle" on the
// live page, so keep the locales and the template in lockstep.
const homeView = readFileSync(resolve(__dirname, '../../views/HomeView.vue'), 'utf8')

function usedLandingKeys(): string[] {
  const matches = homeView.match(/landing\.[a-zA-Z0-9_]+\.[a-zA-Z0-9_]+/g) ?? []
  return [...new Set(matches)].sort()
}

function lookup(messages: Record<string, any>, path: string): unknown {
  return path.split('.').reduce<any>((node, part) => (node == null ? undefined : node[part]), messages)
}

describe('landing page locales', () => {
  it('finds landing keys in HomeView', () => {
    expect(usedLandingKeys().length).toBeGreaterThan(40)
  })

  it.each([
    ['en', en],
    ['zh', zh]
  ])('%s defines every landing key used by HomeView', (_locale, messages) => {
    const missing = usedLandingKeys().filter((key) => typeof lookup(messages as any, key) !== 'string')
    expect(missing).toEqual([])
  })
})
