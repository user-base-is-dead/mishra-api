import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'

const { copyToClipboardMock } = vi.hoisted(() => ({
  copyToClipboardMock: vi.fn().mockResolvedValue(true)
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key
  })
}))

vi.mock('@/composables/useClipboard', () => ({
  useClipboard: () => ({
    copyToClipboard: copyToClipboardMock
  })
}))

import ClaudeCodeInstaller from '../ClaudeCodeInstaller.vue'

const mountInstaller = () =>
  mount(ClaudeCodeInstaller, {
    props: {
      apiKey: 'sk-installer-test',
      baseRoot: 'https://gateway.example.com/v1'
    }
  })

describe('ClaudeCodeInstaller', () => {
  it('renders the POSIX one-command installer with the key inlined', async () => {
    const wrapper = mountInstaller()
    await wrapper.get('[data-testid="installer-os-unix"]').trigger('click')
    await nextTick()

    const command = wrapper.get('[data-testid="installer-command"]').text()
    expect(command).toBe(
      '$ curl -fsSL "https://gateway.example.com/install.sh?key=sk-installer-test" | bash'
    )
    expect(wrapper.get('[data-testid="installer-view-source"]').attributes('href')).toBe(
      'https://gateway.example.com/install.sh?key=sk-installer-test'
    )
    // /v1 must be stripped: the installer scripts are served from the gateway root.
    expect(command).not.toContain('/v1/install.sh')
  })

  it('switches to the PowerShell installer for Windows', async () => {
    const wrapper = mountInstaller()

    await wrapper.get('[data-testid="installer-os-windows"]').trigger('click')
    await nextTick()

    expect(wrapper.get('[data-testid="installer-os-windows"]').attributes('aria-pressed')).toBe('true')
    expect(wrapper.get('[data-testid="installer-command"]').text()).toBe(
      '$ irm "https://gateway.example.com/install.ps1?key=sk-installer-test" | iex'
    )
    expect(wrapper.get('[data-testid="installer-view-source"]').attributes('href')).toContain(
      '/install.ps1?key=sk-installer-test'
    )
  })

  it('copies the install and uninstall commands', async () => {
    copyToClipboardMock.mockClear()
    const wrapper = mountInstaller()
    await wrapper.get('[data-testid="installer-os-unix"]').trigger('click')
    await nextTick()

    await wrapper.get('[data-testid="installer-copy-install"]').trigger('click')
    expect(copyToClipboardMock).toHaveBeenLastCalledWith(
      'curl -fsSL "https://gateway.example.com/install.sh?key=sk-installer-test" | bash',
      'installer.copied'
    )

    await wrapper.get('[data-testid="installer-copy-uninstall"]').trigger('click')
    expect(copyToClipboardMock).toHaveBeenLastCalledWith(
      'curl -fsSL "https://gateway.example.com/install.sh?key=sk-installer-test" | bash -s -- --uninstall',
      'installer.copied'
    )
  })
})
