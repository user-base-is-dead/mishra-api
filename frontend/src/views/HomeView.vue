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
    ref="root"
    class="landing-root relative flex min-h-screen flex-col bg-[#fdfcf9] text-neutral-900 antialiased dark:bg-dark-950 dark:text-neutral-100"
  >
    <!-- ============ Custom Cursor ============ -->
    <div ref="cursorRing" class="cursor-ring" aria-hidden="true">
      <span ref="cursorLabel" class="cursor-label"></span>
    </div>
    <div ref="cursorDot" class="cursor-dot" aria-hidden="true"></div>
    <!-- ============ Sticky Header ============ -->
    <header
      class="sticky top-0 z-40 backdrop-blur-xl transition-colors duration-300"
      :class="scrolled
        ? 'border-b border-neutral-200/70 bg-[#fdfcf9]/80 dark:border-white/10 dark:bg-dark-950/80'
        : 'border-b border-transparent bg-transparent'"
    >
      <nav class="mx-auto flex max-w-7xl items-center justify-between px-6 py-3.5">
        <!-- Logo + Name -->
        <div class="flex items-center gap-3">
          <div class="h-9 w-9 overflow-hidden rounded-lg ring-1 ring-neutral-200/70 dark:ring-white/10">
            <img :src="siteLogo || '/logo.png'" alt="Logo" class="h-full w-full object-contain" />
          </div>
          <div class="leading-none">
            <span class="block text-[15px] font-semibold tracking-tight">{{ siteName }}</span>
            <span class="mt-0.5 block text-[10px] font-medium uppercase tracking-[0.18em] text-primary-600 dark:text-primary-400">
              A {{ PARENT_COMPANY }} Company
            </span>
          </div>
        </div>

        <!-- Center Nav -->
        <div class="absolute left-1/2 hidden -translate-x-1/2 items-center gap-8 lg:flex">
          <a
            v-for="link in navLinks"
            :key="link.href"
            :href="link.href"
            class="text-sm font-medium text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100"
          >{{ t(link.label) }}</a>
        </div>

        <!-- Actions -->
        <div class="flex items-center gap-1.5">
          <!-- Social Icons -->
          <div class="hidden items-center gap-0.5 md:flex">
            <a
              v-for="social in socialLinks"
              :key="social.label"
              :href="social.url"
              target="_blank"
              rel="noopener noreferrer"
              :title="social.label"
              :aria-label="social.label"
              class="rounded-lg p-2 text-neutral-400 transition-colors hover:text-primary-600 dark:text-neutral-500 dark:hover:text-primary-400"
            >
              <svg v-if="social.icon === 'telegram'" class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                <path d="M9.78 18.65l.28-4.23 7.68-6.92c.34-.31-.07-.46-.52-.19L7.74 13.3 3.64 12c-.88-.25-.89-.86.2-1.3l15.97-6.16c.73-.33 1.43.18 1.15 1.3l-2.72 12.81c-.19.91-.74 1.13-1.5.71L12.6 16.3l-1.99 1.93c-.23.23-.42.42-.83.42z"/>
              </svg>
              <svg v-else-if="social.icon === 'telegramBot'" class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                <path d="M12 2a1 1 0 011 1v1h4a2 2 0 012 2v3h1a1 1 0 011 1v3a1 1 0 01-1 1h-1v2a2 2 0 01-2 2H7a2 2 0 01-2-2v-2H4a1 1 0 01-1-1v-3a1 1 0 011-1h1V6a2 2 0 012-2h4V3a1 1 0 011-1zM8.5 11a1.5 1.5 0 100 3 1.5 1.5 0 000-3zm7 0a1.5 1.5 0 100 3 1.5 1.5 0 000-3z"/>
              </svg>
              <svg v-else class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                <path d="M20.317 4.369a19.79 19.79 0 00-4.885-1.515.074.074 0 00-.079.037c-.211.375-.444.865-.608 1.25a18.27 18.27 0 00-5.487 0 12.64 12.64 0 00-.617-1.25.077.077 0 00-.079-.037A19.736 19.736 0 003.677 4.37a.07.07 0 00-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 00.031.057 19.9 19.9 0 005.993 3.03.078.078 0 00.084-.028 14.09 14.09 0 001.226-1.994.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128c.126-.094.252-.192.372-.291a.074.074 0 01.077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 01.078.009c.12.099.246.198.373.292a.077.077 0 01-.006.127 12.3 12.3 0 01-1.873.892.077.077 0 00-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.839 19.839 0 006.002-3.03.077.077 0 00.032-.056c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 00-.031-.028zM8.02 15.331c-1.183 0-2.157-1.086-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.332-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.086-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.332-.946 2.418-2.157 2.418z"/>
              </svg>
            </a>
          </div>

          <span class="mx-1 hidden h-5 w-px bg-neutral-200 dark:bg-white/10 md:block"></span>

          <!-- Language Switcher -->
          <LocaleSwitcher />

          <!-- Doc Link -->
          <a
            v-if="docUrl"
            :href="docUrl"
            target="_blank"
            rel="noopener noreferrer"
            class="rounded-lg p-2 text-neutral-400 transition-colors hover:text-neutral-900 dark:text-neutral-500 dark:hover:text-neutral-100"
            :title="t('home.viewDocs')"
          >
            <Icon name="book" size="md" />
          </a>

          <!-- Theme Toggle -->
          <button
            @click="toggleTheme"
            class="rounded-lg p-2 text-neutral-400 transition-colors hover:text-neutral-900 dark:text-neutral-500 dark:hover:text-neutral-100"
            :title="isDark ? t('home.switchToLight') : t('home.switchToDark')"
          >
            <Icon v-if="isDark" name="sun" size="md" />
            <Icon v-else name="moon" size="md" />
          </button>

          <!-- Login / Dashboard Button -->
          <router-link
            v-if="isAuthenticated"
            :to="dashboardPath"
            data-cursor="enter"
            class="magnetic ml-1 inline-flex items-center gap-1.5 rounded-full border border-neutral-200/70 bg-white py-1 pl-1 pr-3 transition-colors hover:border-neutral-300 dark:border-white/10 dark:bg-white/5 dark:hover:border-white/20"
          >
            <span
              class="flex h-5 w-5 items-center justify-center rounded-full bg-primary-500 text-[10px] font-semibold text-neutral-900"
            >
              {{ userInitial }}
            </span>
            <span class="text-xs font-medium">{{ t('home.dashboard') }}</span>
          </router-link>
          <router-link
            v-else
            to="/login"
            data-cursor="enter"
            class="magnetic ml-1 inline-flex items-center rounded-full bg-primary-500 px-4 py-1.5 text-xs font-semibold text-neutral-900 transition-colors hover:bg-primary-400"
          >
            {{ t('home.login') }}
          </router-link>
        </div>
      </nav>
    </header>

    <main class="relative flex-1">
      <!-- ============ Hero ============ -->
      <section id="product" class="relative overflow-hidden px-6 pb-20 pt-20 md:pt-28">
        <!-- Parallax background element -->
        <div class="pointer-events-none absolute inset-0 overflow-hidden">
          <div
            ref="heroBlob"
            class="hero-blob absolute -right-32 -top-24 h-[32rem] w-[32rem] rounded-full bg-primary-400/10 blur-[120px] dark:bg-primary-500/10"
          ></div>
          <div
            class="absolute inset-0 bg-[linear-gradient(rgba(120,113,108,0.045)_1px,transparent_1px),linear-gradient(90deg,rgba(120,113,108,0.045)_1px,transparent_1px)] bg-[size:56px_56px] [mask-image:radial-gradient(ellipse_at_center,black,transparent_75%)] dark:bg-[linear-gradient(rgba(255,255,255,0.03)_1px,transparent_1px),linear-gradient(90deg,rgba(255,255,255,0.03)_1px,transparent_1px)]"
          ></div>
        </div>

        <div class="relative mx-auto max-w-7xl">
          <div class="grid items-center gap-16 lg:grid-cols-2">
            <!-- Left: Text -->
            <div class="text-center lg:text-left">
              <p class="hero-eyebrow mb-6 text-xs font-medium uppercase tracking-[0.2em] text-primary-600 dark:text-primary-400">
                {{ COMPANY_TAGLINE }}
              </p>

              <h1 class="text-[2.75rem] font-semibold leading-[1.05] tracking-tight md:text-6xl lg:text-[4.25rem]">
                <span class="block overflow-hidden pb-1"><span class="hero-line inline-block">{{ t('landing.hero.line1') }}</span></span>
                <span class="block overflow-hidden pb-1"><span class="hero-line inline-block">{{ t('landing.hero.line2') }}</span></span>
                <span class="block overflow-hidden pb-1"><span class="hero-line inline-block text-primary-600 dark:text-primary-400">{{ t('landing.hero.line3') }}</span></span>
              </h1>

              <p class="hero-sub mx-auto mt-7 max-w-xl text-lg leading-relaxed text-neutral-500 dark:text-neutral-400 lg:mx-0">
                {{ t('landing.hero.subtitle') }}
              </p>

              <div class="hero-cta mt-9 flex flex-col items-center gap-3 sm:flex-row lg:justify-start">
                <router-link
                  :to="isAuthenticated ? dashboardPath : '/login'"
                  data-cursor="launch"
                  class="magnetic inline-flex w-full items-center justify-center gap-2 rounded-xl bg-primary-500 px-7 py-3 text-sm font-semibold text-neutral-900 transition-colors hover:bg-primary-400 sm:w-auto"
                >
                  {{ isAuthenticated ? t('home.goToDashboard') : t('landing.hero.getStarted') }}
                  <Icon name="arrowRight" size="sm" :stroke-width="2" />
                </router-link>
                <a
                  :href="SOCIAL_LINKS.telegramChannel"
                  target="_blank"
                  rel="noopener noreferrer"
                  data-cursor="say hi"
                  class="magnetic inline-flex w-full items-center justify-center gap-2 rounded-xl border border-neutral-300 px-7 py-3 text-sm font-semibold text-neutral-700 transition-colors hover:border-primary-500/50 hover:text-neutral-900 dark:border-white/15 dark:text-neutral-200 dark:hover:border-primary-400/40 dark:hover:text-white sm:w-auto"
                >
                  {{ t('landing.hero.talkToUs') }}
                </a>
              </div>

              <!-- Trust line -->
              <div class="hero-cta mt-9 flex flex-wrap items-center justify-center gap-x-6 gap-y-2 text-sm text-neutral-500 dark:text-neutral-400 lg:justify-start">
                <span class="inline-flex items-center gap-1.5"><Icon name="shield" size="sm" class="text-primary-600 dark:text-primary-400" /> {{ t('landing.hero.trustSecurity') }}</span>
                <span class="inline-flex items-center gap-1.5"><Icon name="bolt" size="sm" class="text-primary-600 dark:text-primary-400" /> {{ t('landing.hero.trustUptime') }}</span>
                <span class="inline-flex items-center gap-1.5"><Icon name="dollar" size="sm" class="text-primary-600 dark:text-primary-400" /> {{ t('landing.hero.trustPayg') }}</span>
              </div>
            </div>

            <!-- Right: Terminal Animation -->
            <div class="hero-terminal flex justify-center lg:justify-end">
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

      <!-- ============ Trust / Providers Marquee ============ -->
      <section class="relative border-y border-neutral-200/70 py-10 dark:border-white/10">
        <p class="reveal mb-7 px-6 text-center text-xs font-medium uppercase tracking-[0.2em] text-neutral-400 dark:text-neutral-500">
          {{ t('landing.trust.heading') }}
        </p>
        <!-- edge fades -->
        <div class="pointer-events-none absolute inset-y-0 left-0 z-10 w-24 bg-gradient-to-r from-[#fdfcf9] to-transparent dark:from-dark-950"></div>
        <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-24 bg-gradient-to-l from-[#fdfcf9] to-transparent dark:from-dark-950"></div>
        <div class="marquee">
          <div ref="providerTrack" class="marquee-track flex w-max items-center gap-3">
            <span
              v-for="(p, i) in providersLoop"
              :key="p.name + i"
              class="inline-flex shrink-0 items-center gap-2 rounded-lg border border-neutral-200/70 px-4 py-2 text-sm font-medium dark:border-white/10"
              :class="p.soon ? 'text-neutral-400 dark:text-neutral-600' : 'text-neutral-600 dark:text-neutral-300'"
            >
              <span class="h-1.5 w-1.5 rounded-full" :class="p.soon ? 'bg-neutral-300 dark:bg-white/20' : 'bg-primary-500'"></span>
              {{ p.name }}
              <span v-if="p.soon" class="text-[10px] uppercase tracking-wider text-neutral-400 dark:text-neutral-600">{{ t('landing.trust.soon') }}</span>
            </span>
          </div>
        </div>
      </section>

      <!-- ============ Feature Bento Grid ============ -->
      <section id="features" class="px-6 py-24">
        <div class="mx-auto max-w-7xl">
          <div class="mb-14 max-w-2xl">
            <p class="reveal mb-4 text-xs font-medium uppercase tracking-[0.2em] text-primary-600 dark:text-primary-400">{{ t('landing.features.eyebrow') }}</p>
            <h2 class="reveal text-3xl font-semibold tracking-tight md:text-4xl">
              {{ t('landing.features.title') }}
            </h2>
            <p class="reveal mt-4 text-lg text-neutral-500 dark:text-neutral-400">
              {{ t('landing.features.subtitle') }}
            </p>
          </div>

          <div
            ref="featureGrid"
            class="grid grid-cols-1 overflow-hidden rounded-2xl border-l border-t border-neutral-200/70 dark:border-white/10 md:grid-cols-2 lg:grid-cols-3"
          >
            <div
              v-for="f in features"
              :key="f.title"
              class="feature-cell tilt group relative overflow-hidden border-b border-r border-neutral-200/70 p-8 transition-colors dark:border-white/10"
              data-cursor="explore"
            >
              <span class="feature-spot pointer-events-none absolute -inset-px opacity-0 transition-opacity duration-300 group-hover:opacity-100"></span>
              <div class="tilt-inner relative">
                <div class="mb-6 inline-flex h-11 w-11 items-center justify-center rounded-xl border border-neutral-200/70 text-primary-600 transition-transform duration-300 group-hover:-translate-y-0.5 group-hover:scale-110 dark:border-white/10 dark:text-primary-400">
                  <Icon :name="f.icon" size="md" />
                </div>
                <h3 class="mb-2 text-base font-semibold tracking-tight">{{ t(f.title) }}</h3>
                <p class="text-sm leading-relaxed text-neutral-500 dark:text-neutral-400">{{ t(f.desc) }}</p>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- ============ Stats Band ============ -->
      <section ref="statsBand" class="border-y border-neutral-200/70 px-6 py-16 dark:border-white/10">
        <div class="mx-auto grid max-w-6xl grid-cols-2 gap-y-10 lg:grid-cols-4">
          <div v-for="s in stats" :key="s.label" class="text-center">
            <div class="text-4xl font-semibold tracking-tight md:text-5xl">
              {{ s.prefix }}{{ s.display.toFixed(s.decimals) }}{{ s.suffix }}
            </div>
            <div class="mt-2 text-xs font-medium uppercase tracking-[0.18em] text-neutral-400 dark:text-neutral-500">
              {{ t(s.label) }}
            </div>
          </div>
        </div>
      </section>

      <!-- ============ How it works ============ -->
      <section id="how" class="px-6 py-24">
        <div class="mx-auto max-w-6xl">
          <div class="mb-16 max-w-2xl">
            <p class="reveal mb-4 text-xs font-medium uppercase tracking-[0.2em] text-primary-600 dark:text-primary-400">{{ t('landing.steps.eyebrow') }}</p>
            <h2 class="reveal text-3xl font-semibold tracking-tight md:text-4xl">{{ t('landing.steps.title') }}</h2>
            <p class="reveal mt-4 text-lg text-neutral-500 dark:text-neutral-400">{{ t('landing.steps.subtitle') }}</p>
          </div>

          <div ref="stepsWrap" class="relative">
            <!-- connecting line -->
            <div class="pointer-events-none absolute left-0 right-0 top-6 hidden md:block">
              <div class="step-line h-px w-full origin-left bg-neutral-200 dark:bg-white/10"></div>
            </div>

            <div class="grid gap-10 md:grid-cols-3">
              <div v-for="(step, i) in steps" :key="step.title" class="step-item relative">
                <div class="mb-5 inline-flex h-12 w-12 items-center justify-center rounded-xl border border-neutral-200/70 bg-[#fdfcf9] text-sm font-semibold text-primary-600 dark:border-white/10 dark:bg-dark-950 dark:text-primary-400">
                  {{ String(i + 1).padStart(2, '0') }}
                </div>
                <h3 class="mb-2 text-base font-semibold tracking-tight">{{ t(step.title) }}</h3>
                <p class="text-sm leading-relaxed text-neutral-500 dark:text-neutral-400">{{ t(step.desc) }}</p>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- ============ Company / Backed by Miron ============ -->
      <section id="company" class="border-t border-neutral-200/70 px-6 py-24 dark:border-white/10">
        <div class="mx-auto grid max-w-6xl items-center gap-12 lg:grid-cols-2">
          <div>
            <p class="reveal mb-4 text-xs font-medium uppercase tracking-[0.2em] text-primary-600 dark:text-primary-400">
              {{ t('landing.company.eyebrow', { company: PARENT_COMPANY }) }}
            </p>
            <h2 class="reveal scramble text-3xl font-semibold tracking-tight md:text-4xl">
              {{ t('landing.company.title', { company: PARENT_COMPANY }) }}
            </h2>
          </div>
          <div class="reveal">
            <p class="text-lg leading-relaxed text-neutral-500 dark:text-neutral-400">
              {{ t('landing.company.body', { siteName, company: PARENT_COMPANY }) }}
            </p>
            <div class="mt-8 flex flex-wrap gap-x-8 gap-y-3 text-sm text-neutral-500 dark:text-neutral-400">
              <span class="inline-flex items-center gap-2"><Icon name="checkCircle" size="sm" class="text-primary-600 dark:text-primary-400" /> {{ t('landing.company.soc') }}</span>
              <span class="inline-flex items-center gap-2"><Icon name="checkCircle" size="sm" class="text-primary-600 dark:text-primary-400" /> {{ t('landing.company.monitoring') }}</span>
              <span class="inline-flex items-center gap-2"><Icon name="checkCircle" size="sm" class="text-primary-600 dark:text-primary-400" /> {{ t('landing.company.routing') }}</span>
            </div>
          </div>
        </div>
      </section>

      <!-- ============ Giant Brand Marquee ============ -->
      <section class="marquee-hero relative overflow-hidden border-t border-neutral-200/70 py-16 dark:border-white/10 md:py-24">
        <div ref="bigMarquee" class="flex w-max items-center gap-8 whitespace-nowrap will-change-transform">
          <template v-for="n in 4" :key="'bm' + n">
            <span class="big-type text-neutral-900 dark:text-neutral-100">{{ siteName }}</span>
            <span class="big-type-outline">✳</span>
            <span class="big-type text-primary-500">{{ COMPANY_TAGLINE }}</span>
            <span class="big-type-outline">✳</span>
          </template>
        </div>
      </section>

      <!-- ============ Final CTA ============ -->
      <section class="px-6 pb-24">
        <div class="reveal mx-auto max-w-6xl overflow-hidden rounded-3xl border border-neutral-200/70 bg-white px-8 py-16 text-center dark:border-white/10 dark:bg-white/[0.02]">
          <p class="mb-4 text-xs font-medium uppercase tracking-[0.2em] text-primary-600 dark:text-primary-400">{{ t('landing.cta.eyebrow') }}</p>
          <h2 class="mx-auto max-w-2xl text-3xl font-semibold tracking-tight md:text-4xl">
            {{ t('landing.cta.title', { siteName }) }}
          </h2>
          <p class="mx-auto mt-4 max-w-xl text-lg text-neutral-500 dark:text-neutral-400">
            {{ t('landing.cta.subtitle') }}
          </p>
          <router-link
            :to="isAuthenticated ? dashboardPath : '/login'"
            class="mt-9 inline-flex items-center gap-2 rounded-xl bg-primary-500 px-8 py-3 text-sm font-semibold text-neutral-900 transition-colors hover:bg-primary-400"
          >
            {{ isAuthenticated ? t('home.goToDashboard') : t('landing.hero.getStarted') }}
            <Icon name="arrowRight" size="sm" :stroke-width="2" />
          </router-link>
        </div>
      </section>
    </main>

    <!-- ============ Footer ============ -->
    <footer class="border-t border-neutral-200/70 px-6 py-14 dark:border-white/10">
      <div class="mx-auto max-w-7xl">
        <div class="grid gap-10 md:grid-cols-2 lg:grid-cols-5">
          <!-- Brand -->
          <div class="lg:col-span-2">
            <div class="mb-4 flex items-center gap-3">
              <div class="h-9 w-9 overflow-hidden rounded-lg ring-1 ring-neutral-200/70 dark:ring-white/10">
                <img :src="siteLogo || '/logo.png'" alt="Logo" class="h-full w-full object-contain" />
              </div>
              <div class="leading-none">
                <span class="block text-[15px] font-semibold tracking-tight">{{ siteName }}</span>
                <span class="mt-0.5 block text-[10px] font-medium uppercase tracking-[0.18em] text-primary-600 dark:text-primary-400">
                  A {{ PARENT_COMPANY }} Company
                </span>
              </div>
            </div>
            <p class="mb-5 max-w-xs text-sm text-neutral-500 dark:text-neutral-400">{{ COMPANY_TAGLINE }}</p>
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
                class="rounded-lg border border-neutral-200/70 p-2 text-neutral-400 transition-colors hover:border-primary-500/40 hover:text-primary-600 dark:border-white/10 dark:text-neutral-500 dark:hover:text-primary-400"
              >
                <svg v-if="social.icon === 'telegram'" class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M9.78 18.65l.28-4.23 7.68-6.92c.34-.31-.07-.46-.52-.19L7.74 13.3 3.64 12c-.88-.25-.89-.86.2-1.3l15.97-6.16c.73-.33 1.43.18 1.15 1.3l-2.72 12.81c-.19.91-.74 1.13-1.5.71L12.6 16.3l-1.99 1.93c-.23.23-.42.42-.83.42z"/>
                </svg>
                <svg v-else-if="social.icon === 'telegramBot'" class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M12 2a1 1 0 011 1v1h4a2 2 0 012 2v3h1a1 1 0 011 1v3a1 1 0 01-1 1h-1v2a2 2 0 01-2 2H7a2 2 0 01-2-2v-2H4a1 1 0 01-1-1v-3a1 1 0 011-1h1V6a2 2 0 012-2h4V3a1 1 0 011-1zM8.5 11a1.5 1.5 0 100 3 1.5 1.5 0 000-3zm7 0a1.5 1.5 0 100 3 1.5 1.5 0 000-3z"/>
                </svg>
                <svg v-else class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                  <path d="M20.317 4.369a19.79 19.79 0 00-4.885-1.515.074.074 0 00-.079.037c-.211.375-.444.865-.608 1.25a18.27 18.27 0 00-5.487 0 12.64 12.64 0 00-.617-1.25.077.077 0 00-.079-.037A19.736 19.736 0 003.677 4.37a.07.07 0 00-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 00.031.057 19.9 19.9 0 005.993 3.03.078.078 0 00.084-.028 14.09 14.09 0 001.226-1.994.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128c.126-.094.252-.192.372-.291a.074.074 0 01.077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 01.078.009c.12.099.246.198.373.292a.077.077 0 01-.006.127 12.3 12.3 0 01-1.873.892.077.077 0 00-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.839 19.839 0 006.002-3.03.077.077 0 00.032-.056c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 00-.031-.028zM8.02 15.331c-1.183 0-2.157-1.086-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.332-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.086-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.332-.946 2.418-2.157 2.418z"/>
                </svg>
              </a>
            </div>
          </div>

          <!-- Product -->
          <div>
            <h4 class="mb-4 text-xs font-semibold uppercase tracking-[0.18em] text-neutral-400 dark:text-neutral-500">{{ t('landing.footer.product') }}</h4>
            <ul class="space-y-3">
              <li><a href="#features" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.features') }}</a></li>
              <li><a href="#how" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.howItWorks') }}</a></li>
              <li><router-link to="/login" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.login') }}</router-link></li>
              <li v-if="docUrl"><a :href="docUrl" target="_blank" rel="noopener noreferrer" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.docs') }}</a></li>
            </ul>
          </div>

          <!-- Community -->
          <div>
            <h4 class="mb-4 text-xs font-semibold uppercase tracking-[0.18em] text-neutral-400 dark:text-neutral-500">{{ t('landing.footer.community') }}</h4>
            <ul class="space-y-3">
              <li v-for="social in socialLinks" :key="social.label">
                <a :href="social.url" target="_blank" rel="noopener noreferrer" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">
                  {{ social.label }}
                </a>
              </li>
            </ul>
          </div>

          <!-- Company + Legal -->
          <div>
            <h4 class="mb-4 text-xs font-semibold uppercase tracking-[0.18em] text-neutral-400 dark:text-neutral-500">{{ t('landing.footer.company') }}</h4>
            <ul class="space-y-3">
              <li><a href="#company" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.about', { company: PARENT_COMPANY }) }}</a></li>
              <li><router-link to="/privacy" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.privacy') }}</router-link></li>
            </ul>
          </div>
        </div>

        <!-- Bottom bar -->
        <div class="mt-12 border-t border-neutral-200/70 pt-8 dark:border-white/10">
          <p class="text-center text-sm text-neutral-400 dark:text-neutral-500">
            {{ t('landing.footer.rights', { year: currentYear, siteName, company: PARENT_COMPANY }) }}
          </p>
        </div>
      </div>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'
import gsap from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'
import { useAuthStore, useAppStore } from '@/stores'
import LocaleSwitcher from '@/components/common/LocaleSwitcher.vue'
import Icon from '@/components/icons/Icon.vue'
import { PARENT_COMPANY, SOCIAL_LINKS, COMPANY_TAGLINE } from '@/config/brand'

gsap.registerPlugin(ScrollTrigger)

const { t } = useI18n()

const authStore = useAuthStore()
const appStore = useAppStore()

// Site settings - directly from appStore (already initialized from injected config)
const siteName = computed(() => appStore.cachedPublicSettings?.site_name || appStore.siteName || 'Mishra Miron API')
const siteLogo = computed(() => appStore.cachedPublicSettings?.site_logo || appStore.siteLogo || '')
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

// Header scroll state
const scrolled = ref(false)
function onScroll() {
  scrolled.value = window.scrollY > 20
}

// Center navigation anchors
const navLinks = [
  { label: 'landing.nav.product', href: '#product' },
  { label: 'landing.nav.features', href: '#features' },
  { label: 'landing.nav.howItWorks', href: '#how' },
  { label: 'landing.nav.company', href: '#company' }
]

// Social / community links (shown in header, footer)
const socialLinks = [
  { label: 'Telegram Bot', url: SOCIAL_LINKS.telegramBot, icon: 'telegramBot' },
  { label: 'Telegram Channel', url: SOCIAL_LINKS.telegramChannel, icon: 'telegram' },
  { label: 'Discord', url: SOCIAL_LINKS.discord, icon: 'discord' }
]

// Supported AI providers (trust strip)
const providers = [
  { name: 'Claude', soon: false },
  { name: 'GPT', soon: false },
  { name: 'Gemini', soon: false },
  { name: 'Antigravity', soon: false },
  { name: 'More', soon: true }
]

// Feature cards
const features = [
  { icon: 'server', title: 'landing.features.unifiedTitle', desc: 'landing.features.unifiedDesc' },
  { icon: 'users', title: 'landing.features.poolTitle', desc: 'landing.features.poolDesc' },
  { icon: 'dollar', title: 'landing.features.billingTitle', desc: 'landing.features.billingDesc' },
  { icon: 'sync', title: 'landing.features.routingTitle', desc: 'landing.features.routingDesc' },
  { icon: 'chart', title: 'landing.features.analyticsTitle', desc: 'landing.features.analyticsDesc' },
  { icon: 'shield', title: 'landing.features.securityTitle', desc: 'landing.features.securityDesc' }
] as const

// Stats band (animated count-up)
const stats = ref([
  { label: 'landing.stats.models', target: 50, prefix: '', suffix: '+', decimals: 0, display: 0 },
  { label: 'landing.stats.uptime', target: 99.9, prefix: '', suffix: '%', decimals: 1, display: 0 },
  { label: 'landing.stats.latency', target: 1, prefix: '<', suffix: 's', decimals: 0, display: 0 },
  { label: 'landing.stats.support', target: 24, prefix: '', suffix: '/7', decimals: 0, display: 0 }
])

// How it works steps
const steps = [
  { title: 'landing.steps.step1Title', desc: 'landing.steps.step1Desc' },
  { title: 'landing.steps.step2Title', desc: 'landing.steps.step2Desc' },
  { title: 'landing.steps.step3Title', desc: 'landing.steps.step3Desc' }
]

// Template refs
const root = ref<HTMLElement | null>(null)
const heroBlob = ref<HTMLElement | null>(null)
const featureGrid = ref<HTMLElement | null>(null)
const statsBand = ref<HTMLElement | null>(null)
const stepsWrap = ref<HTMLElement | null>(null)
const cursorDot = ref<HTMLElement | null>(null)
const cursorRing = ref<HTMLElement | null>(null)
const cursorLabel = ref<HTMLElement | null>(null)
const providerTrack = ref<HTMLElement | null>(null)
const bigMarquee = ref<HTMLElement | null>(null)

// Providers duplicated once for a seamless -50% marquee loop
const providersLoop = [...providers, ...providers]

// Cleanup handles for interaction listeners
const cleanupFns: Array<() => void> = []

let ctx: gsap.Context | undefined

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

function initAnimations() {
  const prefersReduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
  if (prefersReduced) {
    // Everything visible; fill in stat numbers immediately
    stats.value.forEach((s) => { s.display = s.target })
    return
  }

  ctx = gsap.context(() => {
    // Hero headline — split into words, mask-reveal with slight rotation
    const heroLines = gsap.utils.toArray<HTMLElement>('.hero-line')
    const heroWords: HTMLElement[] = []
    heroLines.forEach((line) => {
      const text = line.textContent ?? ''
      line.textContent = ''
      text.split(/(\s+)/).forEach((chunk) => {
        if (chunk.trim() === '') {
          line.appendChild(document.createTextNode(chunk))
          return
        }
        const mask = document.createElement('span')
        mask.className = 'word-mask'
        const word = document.createElement('span')
        word.className = 'hero-word'
        word.textContent = chunk
        mask.appendChild(word)
        line.appendChild(mask)
        heroWords.push(word)
      })
    })

    gsap.from(heroWords, {
      yPercent: 115,
      rotate: 6,
      opacity: 0,
      duration: 1,
      ease: 'power4.out',
      stagger: 0.045,
      delay: 0.1
    })

    // Hero eyebrow
    gsap.from('.hero-eyebrow', { opacity: 0, y: 12, duration: 0.6, ease: 'power2.out' })

    // Hero subtext + CTAs fade + y, delayed after headline
    gsap.from('.hero-sub', { opacity: 0, y: 20, duration: 0.8, delay: 0.5, ease: 'power3.out' })
    gsap.from('.hero-cta', { opacity: 0, y: 20, duration: 0.8, delay: 0.65, stagger: 0.1, ease: 'power3.out' })

    // Terminal fades/scales in
    gsap.from('.hero-terminal', { opacity: 0, y: 30, scale: 0.97, duration: 1, delay: 0.4, ease: 'power3.out' })

    // Section eyebrows / headings / generic reveals — fade-up on scroll
    gsap.utils.toArray<HTMLElement>('.reveal').forEach((el) => {
      gsap.from(el, {
        opacity: 0,
        y: 24,
        duration: 0.8,
        ease: 'power3.out',
        scrollTrigger: { trigger: el, start: 'top 80%' }
      })
    })

    // Feature grid cells — playful overshoot pop-in on scroll
    if (featureGrid.value) {
      gsap.from('.feature-cell', {
        opacity: 0,
        y: 40,
        scale: 0.94,
        duration: 0.7,
        ease: 'back.out(1.7)',
        stagger: { each: 0.07, from: 'start' },
        scrollTrigger: { trigger: featureGrid.value, start: 'top 82%' }
      })
    }

    // Stats count-up — runs once when the band enters the viewport
    if (statsBand.value) {
      ScrollTrigger.create({
        trigger: statsBand.value,
        start: 'top 80%',
        once: true,
        onEnter: () => {
          stats.value.forEach((s) => {
            gsap.to(s, {
              display: s.target,
              duration: 2,
              ease: 'power2.out'
            })
          })
        }
      })
    }

    // How-it-works: connecting line scaleX from 0
    if (stepsWrap.value) {
      gsap.from('.step-line', {
        scaleX: 0,
        duration: 1.1,
        ease: 'power2.out',
        scrollTrigger: { trigger: stepsWrap.value, start: 'top 75%' }
      })

      // Steps stagger from the left
      gsap.from('.step-item', {
        opacity: 0,
        x: -30,
        duration: 0.7,
        ease: 'power3.out',
        stagger: 0.15,
        scrollTrigger: { trigger: stepsWrap.value, start: 'top 75%' }
      })
    }

    // Subtle parallax on the hero background blob
    if (heroBlob.value) {
      gsap.to(heroBlob.value, {
        yPercent: 40,
        ease: 'none',
        scrollTrigger: { trigger: root.value, start: 'top top', end: 'bottom top', scrub: true }
      })
    }

    // Terminal idle float (gentle, endless)
    gsap.to('.terminal-window', {
      y: -10,
      duration: 2.6,
      ease: 'sine.inOut',
      repeat: -1,
      yoyo: true,
      delay: 1.6
    })

    // Provider marquee — seamless left drift (track holds 2 copies -> -50%)
    if (providerTrack.value) {
      gsap.to(providerTrack.value, {
        xPercent: -50,
        duration: 28,
        ease: 'none',
        repeat: -1
      })
    }

    // Giant brand marquee — scroll-reactive drift + base auto-scroll
    if (bigMarquee.value) {
      gsap.to(bigMarquee.value, {
        xPercent: -50,
        duration: 22,
        ease: 'none',
        repeat: -1
      })
      gsap.fromTo(
        bigMarquee.value,
        { x: 120 },
        {
          x: -120,
          ease: 'none',
          scrollTrigger: { trigger: bigMarquee.value, start: 'top bottom', end: 'bottom top', scrub: true }
        }
      )
    }

    // Scramble headline when it scrolls into view
    gsap.utils.toArray<HTMLElement>('.scramble').forEach((el) => {
      ScrollTrigger.create({
        trigger: el,
        start: 'top 80%',
        once: true,
        onEnter: () => scrambleText(el)
      })
    })
  }, root.value ?? undefined)
}

// Decode/scramble text animation (creative-dev signature)
function scrambleText(el: HTMLElement) {
  const finalText = el.textContent ?? ''
  const chars = '!<>-_\\/[]{}—=+*^?#________'
  let frame = 0
  const total = 26
  const interval = window.setInterval(() => {
    frame++
    const progress = frame / total
    const revealCount = Math.floor(finalText.length * progress)
    let out = ''
    for (let i = 0; i < finalText.length; i++) {
      if (finalText[i] === ' ') { out += ' '; continue }
      if (i < revealCount) {
        out += finalText[i]
      } else {
        out += chars[Math.floor(Math.random() * chars.length)]
      }
    }
    el.textContent = out
    if (frame >= total) {
      el.textContent = finalText
      window.clearInterval(interval)
    }
  }, 28)
  cleanupFns.push(() => { window.clearInterval(interval); el.textContent = finalText })
}

// Custom cursor + magnetic buttons + card tilt
function initInteractions() {
  const finePointer = window.matchMedia('(hover: hover) and (pointer: fine)').matches
  const prefersReduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
  if (!finePointer || prefersReduced) return

  const dot = cursorDot.value
  const ring = cursorRing.value
  const label = cursorLabel.value
  const rootEl = root.value
  if (!dot || !ring || !rootEl) return

  rootEl.classList.add('cursor-active')

  let mouseX = window.innerWidth / 2
  let mouseY = window.innerHeight / 2
  let ringX = mouseX
  let ringY = mouseY
  const setDotX = gsap.quickSetter(dot, 'x', 'px')
  const setDotY = gsap.quickSetter(dot, 'y', 'px')

  const onMove = (e: MouseEvent) => {
    mouseX = e.clientX
    mouseY = e.clientY
    setDotX(mouseX)
    setDotY(mouseY)
  }
  window.addEventListener('mousemove', onMove, { passive: true })
  cleanupFns.push(() => window.removeEventListener('mousemove', onMove))

  const tick = () => {
    ringX += (mouseX - ringX) * 0.18
    ringY += (mouseY - ringY) * 0.18
    gsap.set(ring, { x: ringX, y: ringY })
  }
  gsap.ticker.add(tick)
  cleanupFns.push(() => gsap.ticker.remove(tick))

  // Interactive hover state: grow ring + show label
  const interactive = rootEl.querySelectorAll<HTMLElement>('a, button, [data-cursor], .tilt')
  interactive.forEach((elm) => {
    const enter = () => {
      ring.classList.add('is-hover')
      const lbl = elm.getAttribute('data-cursor')
      if (label) label.textContent = lbl ?? ''
      ring.classList.toggle('has-label', !!lbl)
    }
    const leave = () => {
      ring.classList.remove('is-hover')
      ring.classList.remove('has-label')
      if (label) label.textContent = ''
    }
    elm.addEventListener('mouseenter', enter)
    elm.addEventListener('mouseleave', leave)
    cleanupFns.push(() => {
      elm.removeEventListener('mouseenter', enter)
      elm.removeEventListener('mouseleave', leave)
    })
  })

  // Magnetic buttons — translate toward pointer, spring back
  rootEl.querySelectorAll<HTMLElement>('.magnetic').forEach((el) => {
    const strength = 0.35
    const move = (e: MouseEvent) => {
      const rect = el.getBoundingClientRect()
      const relX = e.clientX - (rect.left + rect.width / 2)
      const relY = e.clientY - (rect.top + rect.height / 2)
      gsap.to(el, { x: relX * strength, y: relY * strength, duration: 0.4, ease: 'power3.out' })
    }
    const reset = () => gsap.to(el, { x: 0, y: 0, duration: 0.6, ease: 'elastic.out(1, 0.4)' })
    el.addEventListener('mousemove', move)
    el.addEventListener('mouseleave', reset)
    cleanupFns.push(() => {
      el.removeEventListener('mousemove', move)
      el.removeEventListener('mouseleave', reset)
    })
  })

  // 3D tilt + spotlight on feature cells
  rootEl.querySelectorAll<HTMLElement>('.tilt').forEach((el) => {
    const inner = el.querySelector<HTMLElement>('.tilt-inner')
    const spot = el.querySelector<HTMLElement>('.feature-spot')
    const move = (e: MouseEvent) => {
      const rect = el.getBoundingClientRect()
      const px = (e.clientX - rect.left) / rect.width
      const py = (e.clientY - rect.top) / rect.height
      gsap.to(inner, {
        rotateY: (px - 0.5) * 10,
        rotateX: (0.5 - py) * 10,
        duration: 0.4,
        ease: 'power2.out',
        transformPerspective: 800
      })
      if (spot) {
        spot.style.background = `radial-gradient(240px circle at ${px * 100}% ${py * 100}%, rgba(212,160,23,0.14), transparent 60%)`
      }
    }
    const reset = () => gsap.to(inner, { rotateY: 0, rotateX: 0, duration: 0.6, ease: 'power3.out' })
    el.addEventListener('mousemove', move)
    el.addEventListener('mouseleave', reset)
    cleanupFns.push(() => {
      el.removeEventListener('mousemove', move)
      el.removeEventListener('mouseleave', reset)
    })
  })
}

onMounted(() => {
  initTheme()

  // Check auth state
  authStore.checkAuth()

  // Ensure public settings are loaded (will use cache if already loaded from injected config)
  if (!appStore.publicSettingsLoaded) {
    appStore.fetchPublicSettings()
  }

  window.addEventListener('scroll', onScroll, { passive: true })
  onScroll()

  initAnimations()
  initInteractions()
})

onBeforeUnmount(() => {
  window.removeEventListener('scroll', onScroll)
  cleanupFns.forEach((fn) => fn())
  cleanupFns.length = 0
  ctx?.revert()
  ScrollTrigger.getAll().forEach((trigger) => trigger.kill())
})
</script>

<style scoped>
/* Terminal Container */
.terminal-container {
  position: relative;
  display: inline-block;
}

/* Terminal Window — refined: hairline border, softer shadow, minimal tilt */
.terminal-window {
  width: 420px;
  max-width: 100%;
  background: linear-gradient(160deg, #211c16 0%, #0d0a07 100%);
  border-radius: 12px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow:
    0 20px 45px -20px rgba(0, 0, 0, 0.45),
    0 1px 0 rgba(255, 255, 255, 0.04) inset;
  overflow: hidden;
  transform: perspective(1200px) rotateX(1deg) rotateY(-1deg);
  transition: transform 0.4s ease, box-shadow 0.4s ease;
}

.terminal-window:hover {
  transform: perspective(1200px) rotateX(0deg) rotateY(0deg) translateY(-3px);
  box-shadow: 0 30px 60px -24px rgba(0, 0, 0, 0.5);
}

/* Terminal Header */
.terminal-header {
  display: flex;
  align-items: center;
  padding: 11px 15px;
  background: rgba(255, 255, 255, 0.02);
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}

.terminal-buttons {
  display: flex;
  gap: 7px;
}

.terminal-buttons span {
  width: 11px;
  height: 11px;
  border-radius: 50%;
}

.btn-close {
  background: #6b6255;
}
.btn-minimize {
  background: #6b6255;
}
.btn-maximize {
  background: #6b6255;
}

.terminal-title {
  flex: 1;
  text-align: center;
  font-size: 12px;
  font-family: ui-monospace, monospace;
  color: #8a7a63;
  margin-right: 45px;
}

/* Terminal Body */
.terminal-body {
  padding: 20px 22px;
  font-family: ui-monospace, 'Fira Code', monospace;
  font-size: 13.5px;
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
  animation-delay: 0.6s;
}
.line-2 {
  animation-delay: 1.3s;
}
.line-3 {
  animation-delay: 2.1s;
}
.line-4 {
  animation-delay: 2.8s;
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
  color: #e0a82e;
  font-weight: 600;
}
.code-cmd {
  color: #ded3c3;
}
.code-flag {
  color: #b8a890;
}
.code-url {
  color: #ecc968;
}
.code-comment {
  color: #6b6255;
  font-style: italic;
}
.code-success {
  color: #d4a017;
  background: rgba(212, 160, 23, 0.12);
  padding: 1px 8px;
  border-radius: 4px;
  font-weight: 600;
}
.code-response {
  color: #b8a890;
}

/* Blinking Cursor */
.cursor {
  display: inline-block;
  width: 8px;
  height: 15px;
  background: #e0a82e;
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

@media (prefers-reduced-motion: reduce) {
  .code-line {
    opacity: 1;
    animation: none;
  }
  .cursor {
    animation: none;
  }
  .terminal-window {
    transform: none;
  }
}

/* ===================== Creative motion layer ===================== */

/* Custom cursor */
.cursor-dot,
.cursor-ring {
  position: fixed;
  top: 0;
  left: 0;
  z-index: 60;
  pointer-events: none;
  border-radius: 9999px;
  opacity: 0;
}
.cursor-active .cursor-dot,
.cursor-active .cursor-ring {
  opacity: 1;
}
.cursor-dot {
  width: 7px;
  height: 7px;
  margin: -3.5px 0 0 -3.5px;
  background: #d4a017;
}
.cursor-ring {
  width: 42px;
  height: 42px;
  margin: -21px 0 0 -21px;
  border: 1px solid rgba(212, 160, 23, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: width 0.25s ease, height 0.25s ease, margin 0.25s ease,
    background-color 0.25s ease, border-color 0.25s ease;
}
.cursor-ring.is-hover {
  width: 72px;
  height: 72px;
  margin: -36px 0 0 -36px;
  background: rgba(212, 160, 23, 0.09);
  border-color: rgba(212, 160, 23, 0.35);
}
.cursor-label {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.09em;
  color: #b7860f;
  opacity: 0;
  transition: opacity 0.2s ease;
  white-space: nowrap;
}
.dark .cursor-label {
  color: #ecc968;
}
.cursor-ring.has-label .cursor-label {
  opacity: 1;
}
.cursor-active,
.cursor-active * {
  cursor: none !important;
}

/* Hero word mask reveal */
.word-mask {
  display: inline-block;
  overflow: hidden;
  vertical-align: top;
  padding: 0.02em 0.04em 0.08em 0;
}
.hero-word {
  display: inline-block;
  will-change: transform;
}

/* Marquees */
.marquee {
  overflow: hidden;
}
.marquee-track,
.marquee-hero > div {
  will-change: transform;
}
.big-type {
  font-size: clamp(2.25rem, 9vw, 6.5rem);
  font-weight: 700;
  letter-spacing: -0.025em;
  line-height: 1;
}
.big-type-outline {
  font-size: clamp(1.5rem, 5vw, 3.5rem);
  line-height: 1;
  color: transparent;
  -webkit-text-stroke: 1px rgba(212, 160, 23, 0.55);
}

/* Feature tilt + spotlight */
.tilt {
  transform-style: preserve-3d;
}
.tilt-inner {
  transform-style: preserve-3d;
}
.feature-spot {
  z-index: 0;
}

@media (prefers-reduced-motion: reduce) {
  .marquee-track,
  .marquee-hero > div,
  .terminal-window {
    transform: none !important;
  }
  .hero-word {
    transform: none !important;
  }
}
</style>
