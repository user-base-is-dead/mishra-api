<template>
  <div class="space-y-3" data-testid="claude-code-installer">
    <div
      class="overflow-hidden rounded-2xl border border-black/[0.08] bg-white/80 shadow-[0_2px_20px_-2px_rgba(0,0,0,0.1)] backdrop-blur-2xl backdrop-saturate-150 dark:border-white/10 dark:bg-gray-900/60 dark:shadow-[0_2px_20px_-2px_rgba(0,0,0,0.3)]"
    >
      <!-- OS switch -->
      <div class="flex items-center justify-end border-b border-black/[0.08] px-4 py-3 dark:border-white/10">
        <div class="flex gap-2" role="group" :aria-label="t('installer.osGroupLabel')">
          <button
            type="button"
            data-testid="installer-os-unix"
            :aria-pressed="os === 'unix'"
            class="cursor-pointer rounded-md px-2.5 py-1 text-[11px] transition-all"
            :class="os === 'unix'
              ? 'bg-gray-200/50 text-gray-900 dark:bg-white/[0.06] dark:text-white/70'
              : 'text-gray-400 hover:text-gray-600 dark:text-white/25 dark:hover:text-white/40'"
            @click="os = 'unix'"
          >
            {{ t('installer.osUnix') }}
          </button>
          <button
            type="button"
            data-testid="installer-os-windows"
            :aria-pressed="os === 'windows'"
            class="cursor-pointer rounded-md px-2.5 py-1 text-[11px] transition-all"
            :class="os === 'windows'
              ? 'bg-gray-200/50 text-gray-900 dark:bg-white/[0.06] dark:text-white/70'
              : 'text-gray-400 hover:text-gray-600 dark:text-white/25 dark:hover:text-white/40'"
            @click="os = 'windows'"
          >
            {{ t('installer.osWindows') }}
          </button>
        </div>
      </div>

      <!-- Command -->
      <div class="p-4 sm:p-5">
        <div class="flex items-start gap-3">
          <div class="min-w-0 flex-1 overflow-x-auto">
            <div class="whitespace-pre font-mono text-[13px] leading-6" data-testid="installer-command">
              <span
                v-for="(token, index) in commandTokens"
                :key="index"
                :class="token.class"
              >{{ token.text }}</span>
            </div>
          </div>
          <button
            type="button"
            data-testid="installer-copy-install"
            :title="copiedId === 'install' ? t('installer.copied') : t('installer.copy')"
            :aria-label="copiedId === 'install' ? t('installer.copied') : t('installer.copy')"
            class="m-0 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-md border-0 bg-transparent p-0 leading-6 transition-all hover:bg-gray-100 dark:hover:bg-white/10"
            :class="copiedId === 'install'
              ? 'text-green-600 dark:text-green-400'
              : 'text-gray-400 hover:text-gray-600 dark:text-white/30 dark:hover:text-white/50'"
            @click="copy(command, 'install')"
          >
            <svg v-if="copiedId === 'install'" class="block h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M20 6 9 17l-5-5" />
            </svg>
            <svg v-else class="block h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
              <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
            </svg>
          </button>
        </div>
        <p class="mt-3 font-mono text-[11px] text-gray-400 dark:text-white/20">
          {{ t('installer.hint') }}
        </p>
      </div>
    </div>

    <!-- Next steps -->
    <div class="flex flex-wrap items-center gap-x-4 gap-y-1.5 px-1 text-[11px] text-gray-500 dark:text-white/30">
      <span class="flex items-center gap-1.5">
        <span class="text-gray-400 dark:text-white/20">1.</span>
        {{ os === 'windows' ? t('installer.step1NewTerminal') : t('installer.step1Reload') }}
        <code
          v-if="os !== 'windows'"
          class="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-gray-600 dark:bg-white/[0.06] dark:text-white/40"
        >source ~/.zshrc</code>
      </span>
      <span class="flex items-center gap-1.5">
        <span class="text-gray-400 dark:text-white/20">2.</span>
        {{ t('installer.step2Start') }}
        <code class="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-gray-600 dark:bg-white/[0.06] dark:text-white/40">claude</code>
      </span>
    </div>

    <!-- Source + uninstall -->
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1 px-1 text-[11px] text-gray-400 dark:text-white/25">
      <a
        :href="scriptUrl"
        target="_blank"
        rel="noopener noreferrer"
        data-testid="installer-view-source"
        class="text-primary-600 hover:underline dark:text-primary-400"
      >{{ t('installer.viewSource') }}</a>
      <button
        type="button"
        data-testid="installer-copy-uninstall"
        class="cursor-pointer border-0 bg-transparent p-0 text-left text-gray-400 hover:text-gray-600 hover:underline dark:text-white/25 dark:hover:text-white/40"
        @click="copy(uninstallCommand, 'uninstall')"
      >
        {{ copiedId === 'uninstall' ? t('installer.copied') : t('installer.uninstallHint') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useClipboard } from '@/composables/useClipboard'

interface Props {
  /** API key embedded into the generated install command. */
  apiKey: string
  /** Gateway root (no trailing /v1). Falls back to the current origin. */
  baseRoot?: string
}

type InstallOs = 'unix' | 'windows'
interface Token {
  text: string
  class: string
}

const props = defineProps<Props>()

const { t } = useI18n()
const { copyToClipboard } = useClipboard()

const os = ref<InstallOs>(detectOs())
const copiedId = ref<string | null>(null)

function detectOs(): InstallOs {
  if (typeof navigator === 'undefined') return 'unix'
  const platform = `${navigator.platform || ''} ${navigator.userAgent || ''}`.toLowerCase()
  return platform.includes('win') ? 'windows' : 'unix'
}

const root = computed(() => {
  const raw = props.baseRoot || (typeof window !== 'undefined' ? window.location.origin : '')
  return raw.replace(/\/v1(beta)?\/?$/, '').replace(/\/+$/, '')
})

const scriptPath = computed(() => (os.value === 'windows' ? '/install.ps1' : '/install.sh'))
const scriptUrl = computed(() => `${root.value}${scriptPath.value}?key=${props.apiKey}`)

const command = computed(() =>
  os.value === 'windows'
    ? `irm "${scriptUrl.value}" | iex`
    : `curl -fsSL "${scriptUrl.value}" | bash`
)

const uninstallCommand = computed(() =>
  os.value === 'windows'
    ? `& ([scriptblock]::Create((irm "${scriptUrl.value}"))) -Uninstall`
    : `curl -fsSL "${scriptUrl.value}" | bash -s -- --uninstall`
)

// Token classes mirror a terminal prompt: command words stand out, the key is highlighted.
const CMD = 'text-violet-600 dark:text-violet-400'
const MUTED = 'text-gray-400 dark:text-white/20'
const DIM = 'text-gray-500 dark:text-white/40'
const URL_TOKEN = 'text-sky-600 dark:text-sky-400/70'
const KEY_TOKEN = 'text-amber-600 dark:text-amber-400/70'

const commandTokens = computed((): Token[] => {
  const query = `${scriptPath.value}?key=`
  if (os.value === 'windows') {
    return [
      { text: '$ ', class: MUTED },
      { text: 'irm', class: CMD },
      { text: ' "', class: MUTED },
      { text: root.value, class: URL_TOKEN },
      { text: query, class: DIM },
      { text: props.apiKey, class: KEY_TOKEN },
      { text: '" | ', class: MUTED },
      { text: 'iex', class: CMD }
    ]
  }
  return [
    { text: '$ ', class: MUTED },
    { text: 'curl', class: CMD },
    { text: ' -fsSL', class: DIM },
    { text: ' "', class: MUTED },
    { text: root.value, class: URL_TOKEN },
    { text: query, class: DIM },
    { text: props.apiKey, class: KEY_TOKEN },
    { text: '" | ', class: MUTED },
    { text: 'bash', class: CMD }
  ]
})

async function copy(content: string, id: string) {
  const success = await copyToClipboard(content, t('installer.copied'))
  if (!success) return
  copiedId.value = id
  setTimeout(() => {
    if (copiedId.value === id) copiedId.value = null
  }, 2000)
}
</script>
