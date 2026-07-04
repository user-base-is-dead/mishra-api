<template>
  <div class="relative flex min-h-screen flex-col overflow-hidden bg-primary-50 bg-mesh-gradient dark:bg-dark-950">
    <!-- Background Decorations -->
    <div class="pointer-events-none absolute inset-0 overflow-hidden">
      <div class="absolute -right-40 -top-40 h-96 w-96 rounded-full bg-primary-400/20 blur-3xl"></div>
      <div class="absolute -bottom-40 -left-40 h-96 w-96 rounded-full bg-primary-500/15 blur-3xl"></div>
      <div
        class="absolute inset-0 bg-[linear-gradient(rgba(212,160,23,0.04)_1px,transparent_1px),linear-gradient(90deg,rgba(212,160,23,0.04)_1px,transparent_1px)] bg-[size:64px_64px]"
      ></div>
    </div>

    <!-- Header -->
    <header
      class="sticky top-0 z-30 border-b border-primary-200/40 bg-white/70 backdrop-blur-xl dark:border-dark-800/60 dark:bg-dark-950/70"
    >
      <nav class="mx-auto flex max-w-4xl items-center justify-between px-6 py-3">
        <router-link to="/home" class="flex items-center gap-3">
          <div class="h-10 w-10 overflow-hidden rounded-xl shadow-glow ring-1 ring-primary-400/30">
            <img :src="siteLogo || '/logo.png'" alt="Logo" class="h-full w-full object-contain" />
          </div>
          <div class="leading-tight">
            <span class="block text-lg font-bold text-gray-900 dark:text-white">{{ siteName }}</span>
            <span class="block text-[10px] font-medium uppercase tracking-wider text-primary-600 dark:text-primary-400">
              A {{ PARENT_COMPANY }} Company
            </span>
          </div>
        </router-link>

        <router-link
          to="/home"
          class="inline-flex items-center gap-2 rounded-xl border border-primary-200/70 bg-white/70 px-4 py-2 text-sm font-semibold text-gray-800 backdrop-blur-sm transition-all hover:border-primary-300 hover:shadow-md dark:border-dark-700 dark:bg-dark-800/70 dark:text-white"
        >
          <Icon name="arrowLeft" size="sm" />
          Back to Home
        </router-link>
      </nav>
    </header>

    <!-- Content -->
    <main class="relative z-10 flex-1 px-6 py-16">
      <article class="mx-auto max-w-4xl">
        <div class="mb-12 text-center">
          <span class="mb-4 inline-flex items-center gap-2 rounded-full border border-primary-200/60 bg-white/70 px-4 py-1.5 text-xs font-semibold uppercase tracking-wider text-primary-700 dark:border-primary-800/60 dark:bg-dark-800/70 dark:text-primary-300">
            Legal
          </span>
          <h1 class="mb-3 text-4xl font-extrabold text-gradient md:text-5xl">Privacy Policy</h1>
          <p class="text-sm text-gray-500 dark:text-dark-400">Last updated: {{ lastUpdated }}</p>
        </div>

        <div class="card space-y-10 p-8 md:p-12">
          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">1. Introduction</h2>
            <p class="leading-relaxed text-gray-600 dark:text-dark-300">
              This Privacy Policy explains how {{ siteName }}, operated by {{ PARENT_COMPANY }} ("we", "us", or "our"),
              collects, uses, and protects your information when you use our AI API gateway platform and related
              services (the "Service"). By using the Service, you agree to the practices described in this policy.
            </p>
          </section>

          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">2. Information We Collect</h2>
            <p class="mb-3 leading-relaxed text-gray-600 dark:text-dark-300">We collect the following categories of information:</p>
            <ul class="list-disc space-y-2 pl-6 text-gray-600 dark:text-dark-300">
              <li><span class="font-medium text-gray-800 dark:text-dark-100">Account information</span> — such as your email address and authentication credentials.</li>
              <li><span class="font-medium text-gray-800 dark:text-dark-100">Usage data</span> — API requests, model selections, token counts, timestamps, and billing records.</li>
              <li><span class="font-medium text-gray-800 dark:text-dark-100">Technical data</span> — IP address, browser type, and device information collected automatically.</li>
              <li><span class="font-medium text-gray-800 dark:text-dark-100">Request content</span> — prompts and responses routed through the gateway, processed to deliver the Service.</li>
            </ul>
          </section>

          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">3. How We Use Your Information</h2>
            <ul class="list-disc space-y-2 pl-6 text-gray-600 dark:text-dark-300">
              <li>To provide, operate, and maintain the Service.</li>
              <li>To route requests to third-party AI providers and return responses.</li>
              <li>To calculate usage, process billing, and manage your account balance.</li>
              <li>To monitor performance, prevent abuse, and improve reliability and security.</li>
              <li>To communicate with you about your account, updates, and support requests.</li>
            </ul>
          </section>

          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">4. Data Storage &amp; Security</h2>
            <p class="leading-relaxed text-gray-600 dark:text-dark-300">
              We implement industry-standard technical and organizational measures to protect your data, including
              encryption in transit, access controls, and hardened infrastructure. While no method of transmission or
              storage is completely secure, we work continuously to safeguard your information against unauthorized
              access, alteration, or disclosure.
            </p>
          </section>

          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">5. Third-Party AI Providers</h2>
            <p class="leading-relaxed text-gray-600 dark:text-dark-300">
              {{ siteName }} acts as a gateway that forwards your requests to third-party AI providers (such as
              Anthropic, OpenAI, and Google). When you send a request, the relevant content is transmitted to the
              selected provider to generate a response. Those providers process data according to their own privacy
              policies and terms, and we encourage you to review them.
            </p>
          </section>

          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">6. Cookies</h2>
            <p class="leading-relaxed text-gray-600 dark:text-dark-300">
              We use cookies and similar technologies to keep you signed in, remember your preferences, and understand
              how the Service is used. You can control cookies through your browser settings, though disabling them may
              affect certain features.
            </p>
          </section>

          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">7. Data Retention</h2>
            <p class="leading-relaxed text-gray-600 dark:text-dark-300">
              We retain your information for as long as your account is active or as needed to provide the Service,
              comply with legal obligations, resolve disputes, and enforce our agreements. When data is no longer
              required, we delete or anonymize it.
            </p>
          </section>

          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">8. Your Rights</h2>
            <p class="leading-relaxed text-gray-600 dark:text-dark-300">
              Depending on your jurisdiction, you may have the right to access, correct, export, or delete your personal
              data, and to object to or restrict certain processing. To exercise these rights, contact us using the
              details below.
            </p>
          </section>

          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">9. Children's Privacy</h2>
            <p class="leading-relaxed text-gray-600 dark:text-dark-300">
              The Service is not intended for individuals under the age of 16. We do not knowingly collect personal
              information from children. If you believe a child has provided us with personal data, please contact us so
              we can remove it.
            </p>
          </section>

          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">10. Changes to This Policy</h2>
            <p class="leading-relaxed text-gray-600 dark:text-dark-300">
              We may update this Privacy Policy from time to time. When we make material changes, we will update the
              "Last updated" date above and, where appropriate, notify you through the Service. Your continued use of
              the Service after changes take effect constitutes acceptance of the updated policy.
            </p>
          </section>

          <section>
            <h2 class="mb-3 text-xl font-semibold text-gray-900 dark:text-white">11. Contact Us</h2>
            <p class="leading-relaxed text-gray-600 dark:text-dark-300">
              If you have questions about this Privacy Policy or how we handle your data, contact us at
              <a href="mailto:support@mironapi.com" class="font-medium text-primary-600 hover:underline dark:text-primary-400">support@mironapi.com</a>.
              {{ siteName }} is managed and operated by {{ PARENT_COMPANY }}.
            </p>
          </section>
        </div>

        <div class="mt-10 text-center">
          <router-link
            to="/home"
            class="btn btn-primary px-8 py-3 text-base shadow-lg shadow-primary-500/30"
          >
            <Icon name="arrowLeft" size="md" class="mr-2" :stroke-width="2" />
            Back to Home
          </router-link>
        </div>
      </article>
    </main>

    <!-- Footer -->
    <footer class="relative z-10 border-t border-primary-200/40 bg-white/60 px-6 py-8 backdrop-blur-xl dark:border-dark-800/60 dark:bg-dark-950/70">
      <p class="mx-auto max-w-4xl text-center text-sm text-gray-500 dark:text-dark-400">
        &copy; {{ currentYear }} {{ siteName }}. Managed &amp; operated by {{ PARENT_COMPANY }}. All rights reserved.
      </p>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useAppStore } from '@/stores'
import Icon from '@/components/icons/Icon.vue'
import { PARENT_COMPANY } from '@/config/brand'

const appStore = useAppStore()

const siteName = computed(() => appStore.cachedPublicSettings?.site_name || appStore.siteName || 'Mishra Miron API')
const siteLogo = computed(() => appStore.cachedPublicSettings?.site_logo || appStore.siteLogo || '')

const currentYear = computed(() => new Date().getFullYear())
const lastUpdated = 'February 2025'
</script>
