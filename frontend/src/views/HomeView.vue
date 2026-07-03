<template>
  <!-- Custom Home Content: Full Page Mode -->
  <div v-if="homeContent" class="min-h-screen">
    <!-- iframe mode -->
    <iframe
      v-if="isHomeContentUrl"
      :src="homeContent.trim()"
      class="h-screen w-full border-0"
      allowfullscreen
    ></iframe>
    <!-- HTML mode - SECURITY: homeContent is admin-only setting, XSS risk is acceptable -->
    <div v-else v-html="homeContent"></div>
  </div>

  <!-- Default Home Page -->
  <div
    v-else
    class="relative flex min-h-screen flex-col overflow-hidden bg-primary-50 bg-mesh-gradient dark:bg-dark-950"
  >
    <!-- Background Decorations -->
    <div class="pointer-events-none absolute inset-0 overflow-hidden">
      <div class="absolute -right-40 -top-40 h-96 w-96 rounded-full bg-primary-400/20 blur-3xl"></div>
      <div class="absolute -bottom-40 -left-40 h-96 w-96 rounded-full bg-primary-500/15 blur-3xl"></div>
      <div class="absolute left-1/3 top-1/4 h-72 w-72 rounded-full bg-primary-300/10 blur-3xl"></div>
      <div class="absolute bottom-1/4 right-1/4 h-64 w-64 rounded-full bg-primary-400/10 blur-3xl"></div>
      <div
        class="absolute inset-0 bg-[linear-gradient(rgba(212,160,23,0.04)_1px,transparent_1px),linear-gradient(90deg,rgba(212,160,23,0.04)_1px,transparent_1px)] bg-[size:64px_64px]"
      ></div>
    </div>

    <!-- ============ Sticky Header ============ -->
    <header
      class="sticky top-0 z-30 border-b border-primary-200/40 bg-white/70 backdrop-blur-xl dark:border-dark-800/60 dark:bg-dark-950/70"
    >
      <nav class="mx-auto flex max-w-7xl items-center justify-between px-6 py-3">
        <!-- Logo + Name -->
        <div class="flex items-center gap-3">
          <div class="h-10 w-10 overflow-hidden rounded-xl shadow-glow ring-1 ring-primary-400/30">
            <img :src="siteLogo || '/logo.png'" alt="Logo" class="h-full w-full object-contain" />
          </div>
          <div class="leading-tight">
            <span class="block text-lg font-bold text-gray-900 dark:text-white">{{ siteName }}</span>
            <span class="block text-[10px] font-medium uppercase tracking-wider text-primary-600 dark:text-primary-400">
              A {{ PARENT_COMPANY }} Company
            </span>
          </div>
        </div>

        <!-- Center Nav -->
        <div class="hidden items-center gap-8 lg:flex">
          <a href="#features" class="text-sm font-medium text-gray-600 transition-colors hover:text-primary-600 dark:text-dark-300 dark:hover:text-primary-400">Features</a>
          <a href="#how" class="text-sm font-medium text-gray-600 transition-colors hover:text-primary-600 dark:text-dark-300 dark:hover:text-primary-400">How it works</a>
          <a href="#community" class="text-sm font-medium text-gray-600 transition-colors hover:text-primary-600 dark:text-dark-300 dark:hover:text-primary-400">Community</a>
        </div>

        <!-- Actions -->
        <div class="flex items-center gap-2">
          <!-- Social Icons -->
          <div class="hidden items-center gap-1 md:flex">
            <a
              v-for="social in socialLinks"
              :key="social.label"
              :href="social.url"
              target="_blank"
              rel="noopener noreferrer"
              :title="social.label"
              :aria-label="social.label"
              class="rounded-lg p-2 text-gray-500 transition-colors hover:bg-primary-100 hover:text-primary-600 dark:text-dark-400 dark:hover:bg-dark-800 dark:hover:text-primary-400"
            >
              <svg v-if="social.icon === 'telegram'" class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                <path d="M9.78 18.65l.28-4.23 7.68-6.92c.34-.31-.07-.46-.52-.19L7.74 13.3 3.64 12c-.88-.25-.89-.86.2-1.3l15.97-6.16c.73-.33 1.43.18 1.15 1.3l-2.72 12.81c-.19.91-.74 1.13-1.5.71L12.6 16.3l-1.99 1.93c-.23.23-.42.42-.83.42z"/>
              </svg>
              <svg v-else-if="social.icon === 'telegramBot'" class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                <path d="M12 2a1 1 0 011 1v1h4a2 2 0 012 2v3h1a1 1 0 011 1v3a1 1 0 01-1 1h-1v2a2 2 0 01-2 2H7a2 2 0 01-2-2v-2H4a1 1 0 01-1-1v-3a1 1 0 011-1h1V6a2 2 0 012-2h4V3a1 1 0 011-1zM8.5 11a1.5 1.5 0 100 3 1.5 1.5 0 000-3zm7 0a1.5 1.5 0 100 3 1.5 1.5 0 000-3z"/>
              </svg>
              <svg v-else class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                <path d="M20.317 4.369a19.79 19.79 0 00-4.885-1.515.074.074 0 00-.079.037c-.211.375-.444.865-.608 1.25a18.27 18.27 0 00-5.487 0 12.64 12.64 0 00-.617-1.25.077.077 0 00-.079-.037A19.736 19.736 0 003.677 4.37a.07.07 0 00-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 00.031.057 19.9 19.9 0 005.993 3.03.078.078 0 00.084-.028 14.09 14.09 0 001.226-1.994.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128c.126-.094.252-.192.372-.291a.074.074 0 01.077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 01.078.009c.12.099.246.198.373.292a.077.077 0 01-.006.127 12.3 12.3 0 01-1.873.892.077.077 0 00-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.839 19.839 0 006.002-3.03.077.077 0 00.032-.056c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 00-.031-.028zM8.02 15.331c-1.183 0-2.157-1.086-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.332-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.086-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.332-.946 2.418-2.157 2.418z"/>
              </svg>
            </a>
          </div>

          <span class="mx-1 hidden h-6 w-px bg-gray-200 dark:bg-dark-700 md:block"></span>

          <!-- Language Switcher -->
          <LocaleSwitcher />

          <!-- Doc Link -->
          <a
            v-if="docUrl"
            :href="docUrl"
            target="_blank"
            rel="noopener noreferrer"
            class="rounded-lg p-2 text-gray-500 transition-colors hover:bg-gray-100 hover:text-gray-700 dark:text-dark-400 dark:hover:bg-dark-800 dark:hover:text-white"
            :title="t('home.viewDocs')"
          >
            <Icon name="book" size="md" />
          </a>

          <!-- Theme Toggle -->
          <button
            @click="toggleTheme"
            class="rounded-lg p-2 text-gray-500 transition-colors hover:bg-gray-100 hover:text-gray-700 dark:text-dark-400 dark:hover:bg-dark-800 dark:hover:text-white"
            :title="isDark ? t('home.switchToLight') : t('home.switchToDark')"
          >
            <Icon v-if="isDark" name="sun" size="md" />
            <Icon v-else name="moon" size="md" />
          </button>

          <!-- Login / Dashboard Button -->
          <router-link
            v-if="isAuthenticated"
            :to="dashboardPath"
            class="inline-flex items-center gap-1.5 rounded-full bg-gray-900 py-1 pl-1 pr-2.5 transition-colors hover:bg-gray-800 dark:bg-gray-800 dark:hover:bg-gray-700"
          >
            <span
              class="flex h-5 w-5 items-center justify-center rounded-full bg-gradient-to-br from-primary-400 to-primary-600 text-[10px] font-semibold text-white"
            >
              {{ userInitial }}
            </span>
            <span class="text-xs font-medium text-white">{{ t('home.dashboard') }}</span>
            <svg class="h-3 w-3 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M4.5 19.5l15-15m0 0H8.25m11.25 0v11.25" />
            </svg>
          </router-link>
          <router-link
            v-else
            to="/login"
            class="inline-flex items-center rounded-full bg-gradient-to-br from-primary-400 to-primary-600 px-4 py-1.5 text-xs font-semibold text-white shadow-lg shadow-primary-500/30 transition-transform hover:scale-105"
          >
            {{ t('home.login') }}
          </router-link>
        </div>
      </nav>
    </header>

    <main class="relative z-10 flex-1">
      <!-- ============ Hero ============ -->
      <section class="px-6 pb-16 pt-20">
        <div class="mx-auto max-w-7xl">
          <div class="flex flex-col items-center justify-between gap-14 lg:flex-row lg:gap-16">
            <!-- Left: Text -->
            <div class="flex-1 text-center lg:text-left">
              <div
                class="mb-6 inline-flex items-center gap-2 rounded-full border border-primary-200/60 bg-white/70 px-4 py-1.5 text-xs font-medium text-primary-700 shadow-sm backdrop-blur-sm dark:border-primary-800/60 dark:bg-dark-800/70 dark:text-primary-300"
              >
                <span class="h-2 w-2 animate-pulse rounded-full bg-primary-500"></span>
                {{ COMPANY_TAGLINE }}
              </div>
              <h1 class="mb-5 text-4xl font-extrabold leading-tight text-gradient md:text-6xl lg:text-7xl">
                One Key.<br />Every AI Model.
              </h1>
              <p class="mx-auto mb-8 max-w-xl text-lg text-gray-600 dark:text-dark-300 md:text-xl lg:mx-0">
                {{ siteSubtitle }} — a unified, enterprise-grade gateway to the world's leading AI providers.
                Ship faster with one API, transparent billing, and reliability you can trust.
              </p>

              <div class="flex flex-col items-center gap-4 sm:flex-row lg:justify-start">
                <router-link
                  :to="isAuthenticated ? dashboardPath : '/login'"
                  class="btn btn-primary px-8 py-3 text-base shadow-lg shadow-primary-500/30"
                >
                  {{ isAuthenticated ? t('home.goToDashboard') : 'Get Started' }}
                  <Icon name="arrowRight" size="md" class="ml-2" :stroke-width="2" />
                </router-link>
                <a
                  :href="SOCIAL_LINKS.telegramChannel"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="inline-flex items-center gap-2 rounded-xl border border-primary-200/70 bg-white/70 px-8 py-3 text-base font-semibold text-gray-800 backdrop-blur-sm transition-all hover:border-primary-300 hover:shadow-md dark:border-dark-700 dark:bg-dark-800/70 dark:text-white"
                >
                  <svg class="h-5 w-5 text-primary-500" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                    <path d="M9.78 18.65l.28-4.23 7.68-6.92c.34-.31-.07-.46-.52-.19L7.74 13.3 3.64 12c-.88-.25-.89-.86.2-1.3l15.97-6.16c.73-.33 1.43.18 1.15 1.3l-2.72 12.81c-.19.91-.74 1.13-1.5.71L12.6 16.3l-1.99 1.93c-.23.23-.42.42-.83.42z"/>
                  </svg>
                  Join Community
                </a>
              </div>

              <!-- Trust line -->
              <div class="mt-8 flex flex-wrap items-center justify-center gap-x-6 gap-y-2 text-sm text-gray-500 dark:text-dark-400 lg:justify-start">
                <span class="inline-flex items-center gap-1.5"><Icon name="shield" size="sm" class="text-primary-500" /> Enterprise security</span>
                <span class="inline-flex items-center gap-1.5"><Icon name="bolt" size="sm" class="text-primary-500" /> 99.9% uptime</span>
                <span class="inline-flex items-center gap-1.5"><Icon name="dollar" size="sm" class="text-primary-500" /> Pay-as-you-go</span>
              </div>
            </div>

            <!-- Right: Terminal Animation -->
            <div class="flex flex-1 justify-center lg:justify-end">
              <div class="terminal-container">
                <div class="terminal-window">
                  <div class="terminal-header">
                    <div class="terminal-buttons">
                      <span class="btn-close"></span>
                      <span class="btn-minimize"></span>
                      <span class="btn-maximize"></span>
                    </div>
                    <span class="terminal-title">terminal</span>
                  </div>
                  <div class="terminal-body">
                    <div class="code-line line-1">
                      <span class="code-prompt">$</span>
                      <span class="code-cmd">curl</span>
                      <span class="code-flag">-X POST</span>
                      <span class="code-url">/v1/messages</span>
                    </div>
                    <div class="code-line line-2">
                      <span class="code-comment"># Routing to upstream...</span>
                    </div>
                    <div class="code-line line-3">
                      <span class="code-success">200 OK</span>
                      <span class="code-response">{ "content": "Hello!" }</span>
                    </div>
                    <div class="code-line line-4">
                      <span class="code-prompt">$</span>
                      <span class="cursor"></span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- ============ Trust / Providers Bar ============ -->
      <section class="border-y border-primary-200/30 bg-white/40 px-6 py-12 backdrop-blur-sm dark:border-dark-800/50 dark:bg-dark-900/40">
        <div class="mx-auto max-w-7xl">
          <p class="mb-8 text-center text-sm font-semibold uppercase tracking-widest text-gray-500 dark:text-dark-400">
            Powered by the world's leading AI models
          </p>
          <div class="flex flex-wrap items-center justify-center gap-4">
            <div
              v-for="p in providers"
              :key="p.name"
              class="flex items-center gap-2 rounded-xl border px-5 py-3 backdrop-blur-sm"
              :class="p.soon
                ? 'border-gray-200/50 bg-white/40 opacity-60 dark:border-dark-700/50 dark:bg-dark-800/40'
                : 'border-primary-200 bg-white/60 ring-1 ring-primary-500/20 dark:border-primary-800 dark:bg-dark-800/60'"
            >
              <div class="flex h-8 w-8 items-center justify-center rounded-lg bg-gradient-to-br" :class="p.color">
                <span class="text-xs font-bold text-white">{{ p.badge }}</span>
              </div>
              <span class="text-sm font-medium text-gray-700 dark:text-dark-200">{{ p.name }}</span>
              <span
                v-if="!p.soon"
                class="rounded bg-primary-100 px-1.5 py-0.5 text-[10px] font-medium text-primary-600 dark:bg-primary-900/30 dark:text-primary-400"
              >Supported</span>
              <span
                v-else
                class="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] font-medium text-gray-500 dark:bg-dark-700 dark:text-dark-400"
              >Soon</span>
            </div>
          </div>
        </div>
      </section>

      <!-- ============ Features Grid ============ -->
      <section id="features" class="px-6 py-20">
        <div class="mx-auto max-w-7xl">
          <div class="mb-14 text-center">
            <h2 class="mb-3 text-3xl font-bold text-gray-900 dark:text-white md:text-4xl">
              Everything you need to build with AI
            </h2>
            <p class="mx-auto max-w-2xl text-gray-600 dark:text-dark-400">
              A production-ready platform engineered for teams that ship. One integration, infinite possibilities.
            </p>
          </div>

          <div class="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
            <div v-for="f in features" :key="f.title" class="card card-hover p-7">
              <div
                class="mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-primary-300 via-primary-500 to-primary-700 shadow-lg shadow-primary-500/30"
              >
                <Icon :name="f.icon" size="lg" class="text-white" />
              </div>
              <h3 class="mb-2 text-lg font-semibold text-gray-900 dark:text-white">{{ f.title }}</h3>
              <p class="text-sm leading-relaxed text-gray-600 dark:text-dark-400">{{ f.desc }}</p>
            </div>
          </div>
        </div>
      </section>

      <!-- ============ How it works ============ -->
      <section id="how" class="border-y border-primary-200/30 bg-white/40 px-6 py-20 backdrop-blur-sm dark:border-dark-800/50 dark:bg-dark-900/40">
        <div class="mx-auto max-w-7xl">
          <div class="mb-14 text-center">
            <h2 class="mb-3 text-3xl font-bold text-gray-900 dark:text-white md:text-4xl">Get started in minutes</h2>
            <p class="mx-auto max-w-2xl text-gray-600 dark:text-dark-400">Three simple steps from sign-up to your first request.</p>
          </div>

          <div class="grid gap-8 md:grid-cols-3">
            <div v-for="(step, i) in steps" :key="step.title" class="relative text-center">
              <div
                class="mx-auto mb-5 flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-primary-300 via-primary-500 to-primary-700 text-2xl font-bold text-white shadow-glow"
              >
                {{ i + 1 }}
              </div>
              <h3 class="mb-2 text-lg font-semibold text-gray-900 dark:text-white">{{ step.title }}</h3>
              <p class="text-sm leading-relaxed text-gray-600 dark:text-dark-400">{{ step.desc }}</p>
            </div>
          </div>
        </div>
      </section>

      <!-- ============ Community Strip ============ -->
      <section id="community" class="px-6 py-20">
        <div class="mx-auto max-w-5xl text-center">
          <h2 class="mb-3 text-3xl font-bold text-gray-900 dark:text-white md:text-4xl">Join our community</h2>
          <p class="mx-auto mb-10 max-w-2xl text-gray-600 dark:text-dark-400">
            Get support, share ideas, and stay up to date with the latest from {{ siteName }}.
          </p>
          <div class="grid gap-5 sm:grid-cols-3">
            <a
              v-for="social in socialLinks"
              :key="social.label"
              :href="social.url"
              target="_blank"
              rel="noopener noreferrer"
              class="card card-hover flex flex-col items-center gap-3 p-8"
            >
              <div class="flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-primary-300 via-primary-500 to-primary-700 text-white shadow-lg shadow-primary-500/30">
                <svg v-if="social.icon === 'telegram'" class="h-7 w-7" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M9.78 18.65l.28-4.23 7.68-6.92c.34-.31-.07-.46-.52-.19L7.74 13.3 3.64 12c-.88-.25-.89-.86.2-1.3l15.97-6.16c.73-.33 1.43.18 1.15 1.3l-2.72 12.81c-.19.91-.74 1.13-1.5.71L12.6 16.3l-1.99 1.93c-.23.23-.42.42-.83.42z"/>
                </svg>
                <svg v-else-if="social.icon === 'telegramBot'" class="h-7 w-7" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M12 2a1 1 0 011 1v1h4a2 2 0 012 2v3h1a1 1 0 011 1v3a1 1 0 01-1 1h-1v2a2 2 0 01-2 2H7a2 2 0 01-2-2v-2H4a1 1 0 01-1-1v-3a1 1 0 011-1h1V6a2 2 0 012-2h4V3a1 1 0 011-1zM8.5 11a1.5 1.5 0 100 3 1.5 1.5 0 000-3zm7 0a1.5 1.5 0 100 3 1.5 1.5 0 000-3z"/>
                </svg>
                <svg v-else class="h-7 w-7" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M20.317 4.369a19.79 19.79 0 00-4.885-1.515.074.074 0 00-.079.037c-.211.375-.444.865-.608 1.25a18.27 18.27 0 00-5.487 0 12.64 12.64 0 00-.617-1.25.077.077 0 00-.079-.037A19.736 19.736 0 003.677 4.37a.07.07 0 00-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 00.031.057 19.9 19.9 0 005.993 3.03.078.078 0 00.084-.028 14.09 14.09 0 001.226-1.994.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128c.126-.094.252-.192.372-.291a.074.074 0 01.077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 01.078.009c.12.099.246.198.373.292a.077.077 0 01-.006.127 12.3 12.3 0 01-1.873.892.077.077 0 00-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.839 19.839 0 006.002-3.03.077.077 0 00.032-.056c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 00-.031-.028zM8.02 15.331c-1.183 0-2.157-1.086-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.332-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.086-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.332-.946 2.418-2.157 2.418z"/>
                </svg>
              </div>
              <span class="text-base font-semibold text-gray-900 dark:text-white">{{ social.label }}</span>
              <span class="inline-flex items-center gap-1 text-sm font-medium text-primary-600 dark:text-primary-400">
                Open <Icon name="arrowRight" size="sm" />
              </span>
            </a>
          </div>
        </div>
      </section>

      <!-- ============ Company / About Strip ============ -->
      <section class="border-y border-primary-200/30 bg-white/40 px-6 py-16 backdrop-blur-sm dark:border-dark-800/50 dark:bg-dark-900/40">
        <div class="mx-auto flex max-w-5xl flex-col items-center gap-6 text-center">
          <span class="inline-flex items-center gap-2 rounded-full border border-primary-200/60 bg-white/70 px-4 py-1.5 text-xs font-semibold uppercase tracking-wider text-primary-700 dark:border-primary-800/60 dark:bg-dark-800/70 dark:text-primary-300">
            Backed by {{ PARENT_COMPANY }}
          </span>
          <h2 class="text-2xl font-bold text-gray-900 dark:text-white md:text-3xl">
            Enterprise-grade AI infrastructure, delivered by {{ PARENT_COMPANY }}
          </h2>
          <p class="max-w-3xl text-gray-600 dark:text-dark-400">
            {{ siteName }} is built, managed, and operated by {{ PARENT_COMPANY }} — combining deep infrastructure
            expertise with a relentless focus on reliability, security, and developer experience. When you build on
            {{ siteName }}, you're backed by the {{ PARENT_COMPANY }} team.
          </p>
        </div>
      </section>

      <!-- ============ Final CTA Band ============ -->
      <section class="px-6 py-20">
        <div class="mx-auto max-w-6xl overflow-hidden rounded-3xl bg-gradient-to-br from-primary-400 via-primary-500 to-primary-700 px-8 py-16 text-center shadow-glow">
          <h2 class="mb-4 text-3xl font-extrabold text-white md:text-4xl">
            Start building with {{ siteName }} today
          </h2>
          <p class="mx-auto mb-8 max-w-2xl text-primary-50/90">
            One key, every AI model. Join developers and teams shipping faster on enterprise-grade infrastructure.
          </p>
          <router-link
            :to="isAuthenticated ? dashboardPath : '/login'"
            class="inline-flex items-center gap-2 rounded-xl bg-white px-8 py-3 text-base font-semibold text-primary-700 shadow-lg transition-transform hover:scale-105"
          >
            {{ isAuthenticated ? t('home.goToDashboard') : 'Get Started' }}
            <Icon name="arrowRight" size="md" :stroke-width="2" />
          </router-link>
        </div>
      </section>
    </main>

    <!-- ============ Footer ============ -->
    <footer class="relative z-10 border-t border-primary-200/40 bg-white/60 px-6 py-14 backdrop-blur-xl dark:border-dark-800/60 dark:bg-dark-950/70">
      <div class="mx-auto max-w-7xl">
        <div class="grid gap-10 md:grid-cols-2 lg:grid-cols-4">
          <!-- Brand -->
          <div class="lg:col-span-1">
            <div class="mb-4 flex items-center gap-3">
              <div class="h-10 w-10 overflow-hidden rounded-xl shadow-glow ring-1 ring-primary-400/30">
                <img :src="siteLogo || '/logo.png'" alt="Logo" class="h-full w-full object-contain" />
              </div>
              <div class="leading-tight">
                <span class="block text-base font-bold text-gray-900 dark:text-white">{{ siteName }}</span>
                <span class="block text-[10px] font-medium uppercase tracking-wider text-primary-600 dark:text-primary-400">
                  A {{ PARENT_COMPANY }} Company
                </span>
              </div>
            </div>
            <p class="mb-4 max-w-xs text-sm text-gray-500 dark:text-dark-400">{{ COMPANY_TAGLINE }}</p>
            <!-- Social row -->
            <div class="flex items-center gap-2">
              <a
                v-for="social in socialLinks"
                :key="social.label"
                :href="social.url"
                target="_blank"
                rel="noopener noreferrer"
                :title="social.label"
                :aria-label="social.label"
                class="rounded-lg border border-gray-200/60 p-2 text-gray-500 transition-colors hover:border-primary-300 hover:bg-primary-100 hover:text-primary-600 dark:border-dark-700 dark:text-dark-400 dark:hover:bg-dark-800 dark:hover:text-primary-400"
              >
                <svg v-if="social.icon === 'telegram'" class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M9.78 18.65l.28-4.23 7.68-6.92c.34-.31-.07-.46-.52-.19L7.74 13.3 3.64 12c-.88-.25-.89-.86.2-1.3l15.97-6.16c.73-.33 1.43.18 1.15 1.3l-2.72 12.81c-.19.91-.74 1.13-1.5.71L12.6 16.3l-1.99 1.93c-.23.23-.42.42-.83.42z"/>
                </svg>
                <svg v-else-if="social.icon === 'telegramBot'" class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M12 2a1 1 0 011 1v1h4a2 2 0 012 2v3h1a1 1 0 011 1v3a1 1 0 01-1 1h-1v2a2 2 0 01-2 2H7a2 2 0 01-2-2v-2H4a1 1 0 01-1-1v-3a1 1 0 011-1h1V6a2 2 0 012-2h4V3a1 1 0 011-1zM8.5 11a1.5 1.5 0 100 3 1.5 1.5 0 000-3zm7 0a1.5 1.5 0 100 3 1.5 1.5 0 000-3z"/>
                </svg>
                <svg v-else class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M20.317 4.369a19.79 19.79 0 00-4.885-1.515.074.074 0 00-.079.037c-.211.375-.444.865-.608 1.25a18.27 18.27 0 00-5.487 0 12.64 12.64 0 00-.617-1.25.077.077 0 00-.079-.037A19.736 19.736 0 003.677 4.37a.07.07 0 00-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 00.031.057 19.9 19.9 0 005.993 3.03.078.078 0 00.084-.028 14.09 14.09 0 001.226-1.994.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128c.126-.094.252-.192.372-.291a.074.074 0 01.077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 01.078.009c.12.099.246.198.373.292a.077.077 0 01-.006.127 12.3 12.3 0 01-1.873.892.077.077 0 00-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.839 19.839 0 006.002-3.03.077.077 0 00.032-.056c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 00-.031-.028zM8.02 15.331c-1.183 0-2.157-1.086-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.332-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.086-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.332-.946 2.418-2.157 2.418z"/>
                </svg>
              </a>
            </div>
          </div>

          <!-- Product -->
          <div>
            <h4 class="mb-4 text-sm font-semibold uppercase tracking-wider text-gray-900 dark:text-white">Product</h4>
            <ul class="space-y-3">
              <li><a href="#features" class="text-sm text-gray-500 transition-colors hover:text-primary-600 dark:text-dark-400 dark:hover:text-primary-400">Features</a></li>
              <li><a href="#how" class="text-sm text-gray-500 transition-colors hover:text-primary-600 dark:text-dark-400 dark:hover:text-primary-400">How it works</a></li>
              <li><router-link to="/login" class="text-sm text-gray-500 transition-colors hover:text-primary-600 dark:text-dark-400 dark:hover:text-primary-400">Login</router-link></li>
              <li v-if="docUrl"><a :href="docUrl" target="_blank" rel="noopener noreferrer" class="text-sm text-gray-500 transition-colors hover:text-primary-600 dark:text-dark-400 dark:hover:text-primary-400">Documentation</a></li>
            </ul>
          </div>

          <!-- Community -->
          <div>
            <h4 class="mb-4 text-sm font-semibold uppercase tracking-wider text-gray-900 dark:text-white">Community</h4>
            <ul class="space-y-3">
              <li v-for="social in socialLinks" :key="social.label">
                <a :href="social.url" target="_blank" rel="noopener noreferrer" class="text-sm text-gray-500 transition-colors hover:text-primary-600 dark:text-dark-400 dark:hover:text-primary-400">
                  {{ social.label }}
                </a>
              </li>
            </ul>
          </div>

          <!-- Legal -->
          <div>
            <h4 class="mb-4 text-sm font-semibold uppercase tracking-wider text-gray-900 dark:text-white">Legal</h4>
            <ul class="space-y-3">
              <li><router-link to="/privacy" class="text-sm text-gray-500 transition-colors hover:text-primary-600 dark:text-dark-400 dark:hover:text-primary-400">Privacy Policy</router-link></li>
            </ul>
          </div>
        </div>

        <!-- Bottom bar -->
        <div class="mt-12 border-t border-gray-200/50 pt-8 dark:border-dark-800/50">
          <p class="text-center text-sm text-gray-500 dark:text-dark-400">
            &copy; {{ currentYear }} {{ siteName }}. Managed &amp; operated by {{ PARENT_COMPANY }}. All rights reserved.
          </p>
        </div>
      </div>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAuthStore, useAppStore } from '@/stores'
import LocaleSwitcher from '@/components/common/LocaleSwitcher.vue'
import Icon from '@/components/icons/Icon.vue'
import { PARENT_COMPANY, SOCIAL_LINKS, COMPANY_TAGLINE } from '@/config/brand'

const { t } = useI18n()

const authStore = useAuthStore()
const appStore = useAppStore()

// Site settings - directly from appStore (already initialized from injected config)
const siteName = computed(() => appStore.cachedPublicSettings?.site_name || appStore.siteName || 'Mishra API')
const siteLogo = computed(() => appStore.cachedPublicSettings?.site_logo || appStore.siteLogo || '')
const siteSubtitle = computed(() => appStore.cachedPublicSettings?.site_subtitle || 'AI API Gateway Platform')
const docUrl = computed(() => appStore.cachedPublicSettings?.doc_url || appStore.docUrl || '')
const homeContent = computed(() => appStore.cachedPublicSettings?.home_content || '')

// Check if homeContent is a URL (for iframe display)
const isHomeContentUrl = computed(() => {
  const content = homeContent.value.trim()
  return content.startsWith('http://') || content.startsWith('https://')
})

// Theme
const isDark = ref(document.documentElement.classList.contains('dark'))

// Auth state
const isAuthenticated = computed(() => authStore.isAuthenticated)
const isAdmin = computed(() => authStore.isAdmin)
const dashboardPath = computed(() => isAdmin.value ? '/admin/dashboard' : '/dashboard')
const userInitial = computed(() => {
  const user = authStore.user
  if (!user || !user.email) return ''
  return user.email.charAt(0).toUpperCase()
})

// Current year for footer
const currentYear = computed(() => new Date().getFullYear())

// Social / community links (shown in header, community strip, and footer)
const socialLinks = [
  { label: 'Telegram Bot', url: SOCIAL_LINKS.telegramBot, icon: 'telegramBot' },
  { label: 'Telegram Channel', url: SOCIAL_LINKS.telegramChannel, icon: 'telegram' },
  { label: 'Discord', url: SOCIAL_LINKS.discord, icon: 'discord' }
]

// Supported AI providers
const providers = [
  { name: 'Claude', badge: 'C', color: 'from-orange-400 to-orange-500', soon: false },
  { name: 'GPT', badge: 'G', color: 'from-green-500 to-green-600', soon: false },
  { name: 'Gemini', badge: 'G', color: 'from-blue-500 to-blue-600', soon: false },
  { name: 'Antigravity', badge: 'A', color: 'from-rose-500 to-pink-600', soon: false },
  { name: 'More', badge: '+', color: 'from-gray-500 to-gray-600', soon: true }
]

// Feature cards
const features = [
  { icon: 'server', title: 'Unified Gateway', desc: 'A single, OpenAI-compatible endpoint that routes to Claude, GPT, Gemini and more. Integrate once, switch models freely.' },
  { icon: 'users', title: 'Multi-Account Pool', desc: 'Pool multiple upstream accounts with sticky sessions and automatic load balancing for higher throughput and resilience.' },
  { icon: 'dollar', title: 'Pay-as-you-go Billing', desc: 'Transparent, usage-based pricing with real-time balance tracking. No surprises, no lock-in — pay only for what you use.' },
  { icon: 'sync', title: 'Smart Routing & Failover', desc: 'Intelligent request routing with automatic retries and failover so your applications keep running even when an upstream hiccups.' },
  { icon: 'chart', title: 'Real-time Analytics', desc: 'Monitor usage, latency, and spend across every model and key with live dashboards and detailed request logs.' },
  { icon: 'shield', title: 'Enterprise Security', desc: 'Scoped API keys, granular quotas, and hardened infrastructure built to meet the demands of production workloads.' }
] as const

// How it works steps
const steps = [
  { title: 'Create your account', desc: 'Sign up in seconds and access your dashboard to manage keys, usage, and billing in one place.' },
  { title: 'Add a key or pick a plan', desc: 'Generate an API key and choose a pay-as-you-go plan that fits your workload and budget.' },
  { title: 'Start building', desc: 'Point your existing SDK at our endpoint and start shipping. One key unlocks every supported model.' }
]

// Toggle theme
function toggleTheme() {
  isDark.value = !isDark.value
  document.documentElement.classList.toggle('dark', isDark.value)
  localStorage.setItem('theme', isDark.value ? 'dark' : 'light')
}

// Initialize theme
function initTheme() {
  const savedTheme = localStorage.getItem('theme')
  if (
    savedTheme === 'dark' ||
    (!savedTheme && window.matchMedia('(prefers-color-scheme: dark)').matches)
  ) {
    isDark.value = true
    document.documentElement.classList.add('dark')
  }
}

onMounted(() => {
  initTheme()

  // Check auth state
  authStore.checkAuth()

  // Ensure public settings are loaded (will use cache if already loaded from injected config)
  if (!appStore.publicSettingsLoaded) {
    appStore.fetchPublicSettings()
  }
})
</script>

<style scoped>
/* Terminal Container */
.terminal-container {
  position: relative;
  display: inline-block;
}

/* Terminal Window */
.terminal-window {
  width: 420px;
  background: linear-gradient(145deg, #2e2820 0%, #17130e 100%);
  border-radius: 14px;
  box-shadow:
    0 25px 50px -12px rgba(0, 0, 0, 0.4),
    0 0 0 1px rgba(255, 255, 255, 0.1),
    inset 0 1px 0 rgba(255, 255, 255, 0.1);
  overflow: hidden;
  transform: perspective(1000px) rotateX(2deg) rotateY(-2deg);
  transition: transform 0.3s ease;
}

.terminal-window:hover {
  transform: perspective(1000px) rotateX(0deg) rotateY(0deg) translateY(-4px);
}

/* Terminal Header */
.terminal-header {
  display: flex;
  align-items: center;
  padding: 12px 16px;
  background: rgba(46, 40, 32, 0.8);
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.terminal-buttons {
  display: flex;
  gap: 8px;
}

.terminal-buttons span {
  width: 12px;
  height: 12px;
  border-radius: 50%;
}

.btn-close {
  background: #ef4444;
}
.btn-minimize {
  background: #eab308;
}
.btn-maximize {
  background: #22c55e;
}

.terminal-title {
  flex: 1;
  text-align: center;
  font-size: 12px;
  font-family: ui-monospace, monospace;
  color: #64748b;
  margin-right: 52px;
}

/* Terminal Body */
.terminal-body {
  padding: 20px 24px;
  font-family: ui-monospace, 'Fira Code', monospace;
  font-size: 14px;
  line-height: 2;
}

.code-line {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  opacity: 0;
  animation: line-appear 0.5s ease forwards;
}

.line-1 {
  animation-delay: 0.3s;
}
.line-2 {
  animation-delay: 1s;
}
.line-3 {
  animation-delay: 1.8s;
}
.line-4 {
  animation-delay: 2.5s;
}

@keyframes line-appear {
  from {
    opacity: 0;
    transform: translateY(5px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.code-prompt {
  color: #22c55e;
  font-weight: bold;
}
.code-cmd {
  color: #38bdf8;
}
.code-flag {
  color: #a78bfa;
}
.code-url {
  color: #d4a017;
}
.code-comment {
  color: #64748b;
  font-style: italic;
}
.code-success {
  color: #22c55e;
  background: rgba(34, 197, 94, 0.15);
  padding: 2px 8px;
  border-radius: 4px;
  font-weight: 600;
}
.code-response {
  color: #fbbf24;
}

/* Blinking Cursor */
.cursor {
  display: inline-block;
  width: 8px;
  height: 16px;
  background: #22c55e;
  animation: blink 1s step-end infinite;
}

@keyframes blink {
  0%,
  50% {
    opacity: 1;
  }
  51%,
  100% {
    opacity: 0;
  }
}

/* Dark mode adjustments */
:deep(.dark) .terminal-window {
  box-shadow:
    0 25px 50px -12px rgba(0, 0, 0, 0.6),
    0 0 0 1px rgba(212, 160, 23, 0.2),
    0 0 40px rgba(212, 160, 23, 0.1),
    inset 0 1px 0 rgba(255, 255, 255, 0.1);
}
</style>
