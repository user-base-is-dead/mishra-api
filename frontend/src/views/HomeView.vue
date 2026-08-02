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
    class="landing-root relative flex min-h-screen flex-col bg-[#fbfaf6] font-sans text-neutral-900 antialiased dark:bg-dark-950 dark:text-neutral-100"
  >
    <!-- ============ Preloader ============ -->
    <div v-if="showPreloader" ref="preloaderEl" class="preloader fixed inset-0 z-[80]" aria-hidden="true">
      <div class="preloader-panel preloader-panel-top"></div>
      <div class="preloader-panel preloader-panel-bottom"></div>
      <div class="preloader-center">
        <div class="preloader-logo">
          <img :src="siteLogo || '/logo.png'" alt="" class="preloader-logo-img" />
          <span class="preloader-logo-ring"></span>
        </div>
        <div class="preloader-name font-mono">{{ siteName }}</div>
        <div class="preloader-bar"><span ref="preloaderBar" class="preloader-bar-fill"></span></div>
      </div>
      <div ref="preloaderCount" class="preloader-count font-display">00</div>
    </div>
    <!-- ============ Film grain + vignette ============ -->
    <div class="grain pointer-events-none fixed inset-0 z-[55]" aria-hidden="true"></div>

    <!-- ============ Custom Cursor ============ -->
    <div ref="cursorRing" class="cursor-ring" aria-hidden="true">
      <span ref="cursorLabel" class="cursor-label"></span>
    </div>
    <div ref="cursorDot" class="cursor-dot" aria-hidden="true"></div>

    <!-- ============ Sticky Header ============ -->
    <header
      class="fixed inset-x-0 top-0 transition-all duration-500"
      :class="[
        mobileMenuOpen ? 'z-[70]' : 'z-40',
        scrolled && !mobileMenuOpen
          ? 'border-b border-neutral-200/70 bg-[#fbfaf6]/80 backdrop-blur-xl dark:border-primary-400/15 dark:bg-dark-950/75'
          : 'border-b border-transparent bg-transparent'
      ]"
    >
      <nav class="mx-auto flex max-w-7xl items-center justify-between px-6 py-4">
        <!-- Logo + Name -->
        <router-link to="/" data-cursor="home" class="magnetic flex items-center gap-3">
          <div class="h-9 w-9 overflow-hidden rounded-xl ring-1 ring-neutral-200/70 dark:ring-white/10">
            <img :src="siteLogo || '/logo.png'" alt="Logo" class="h-full w-full object-contain" />
          </div>
          <div class="leading-none">
            <span class="block whitespace-nowrap font-display text-[15px] font-semibold tracking-tight">{{ siteName }}</span>
            <span class="mt-1 block whitespace-nowrap font-mono text-[9px] font-medium uppercase tracking-[0.22em] text-primary-600 dark:text-primary-400">
              A {{ PARENT_COMPANY }} Company
            </span>
          </div>
        </router-link>

        <!-- Center Nav -->
        <div class="absolute left-1/2 hidden -translate-x-1/2 items-center gap-1 lg:flex">
          <template v-for="link in navLinks" :key="link.href">
            <router-link
              v-if="link.home"
              :to="link.href"
              data-cursor="jump"
              class="nav-link relative rounded-full px-4 py-2 text-sm font-medium text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100"
              @click="scrollToTop"
            >{{ t(link.label) }}</router-link>
            <a
              v-else
              :href="link.href"
              :target="link.external ? '_blank' : undefined"
              :rel="link.external ? 'noopener noreferrer' : undefined"
              data-cursor="jump"
              class="nav-link relative rounded-full px-4 py-2 text-sm font-medium text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100"
            >{{ t(link.label) }}</a>
          </template>
        </div>

        <!-- Actions -->
        <div class="flex items-center gap-1.5">
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

          <LocaleSwitcher />

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

          <button
            @click="toggleTheme"
            class="rounded-lg p-2 text-neutral-400 transition-colors hover:text-neutral-900 dark:text-neutral-500 dark:hover:text-neutral-100"
            :title="isDark ? t('home.switchToLight') : t('home.switchToDark')"
          >
            <Icon v-if="isDark" name="sun" size="md" />
            <Icon v-else name="moon" size="md" />
          </button>

          <router-link
            v-if="isAuthenticated"
            :to="dashboardPath"
            data-cursor="enter"
            class="magnetic ml-1 inline-flex items-center gap-1.5 rounded-full border border-neutral-200/70 bg-white py-1 pl-1 pr-3 transition-colors hover:border-neutral-300 dark:border-white/10 dark:bg-white/5 dark:hover:border-white/20"
          >
            <span class="flex h-5 w-5 items-center justify-center rounded-full bg-primary-500 text-[10px] font-semibold text-neutral-900">
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

          <!-- Hamburger (below lg) -->
          <button
            class="burger-btn relative z-[70] ml-1 rounded-lg p-2 text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100 lg:hidden"
            :aria-expanded="mobileMenuOpen"
            aria-label="Menu"
            @click="toggleMobileMenu"
          >
            <span class="burger" :class="{ 'is-open': mobileMenuOpen }">
              <span></span><span></span>
            </span>
          </button>
        </div>
      </nav>
    </header>

    <!-- ============ Mobile / tablet nav overlay ============ -->
    <transition name="mnav">
      <div
        v-if="mobileMenuOpen"
        class="mnav fixed inset-0 z-[60] flex flex-col bg-[#fbfaf6]/95 backdrop-blur-2xl dark:bg-[#0a0603]/95 lg:hidden"
      >
        <div class="flex h-full flex-col justify-between px-8 pb-10 pt-28">
          <nav class="flex flex-col">
            <template v-for="(link, i) in navLinks" :key="'m' + link.href">
              <router-link
                v-if="link.home"
                :to="link.href"
                class="mnav-link font-display"
                :style="{ transitionDelay: `${90 + i * 55}ms` }"
                @click="closeMobileMenu(); scrollToTop()"
              >
                <span class="mnav-index font-mono">0{{ i + 1 }}</span>{{ t(link.label) }}
              </router-link>
              <a
                v-else
                :href="link.href"
                :target="link.external ? '_blank' : undefined"
                :rel="link.external ? 'noopener noreferrer' : undefined"
                class="mnav-link font-display"
                :style="{ transitionDelay: `${90 + i * 55}ms` }"
                @click="closeMobileMenu()"
              >
                <span class="mnav-index font-mono">0{{ i + 1 }}</span>{{ t(link.label) }}
              </a>
            </template>
          </nav>

          <div class="mnav-footer" :style="{ transitionDelay: '320ms' }">
            <div class="mb-6 flex items-center gap-3">
              <a
                v-for="social in socialLinks"
                :key="'mn' + social.label"
                :href="social.url"
                target="_blank"
                rel="noopener noreferrer"
                :aria-label="social.label"
                class="rounded-xl border border-neutral-200/70 p-3 text-neutral-500 transition-colors hover:border-primary-500/40 hover:text-primary-600 dark:border-white/10 dark:text-neutral-400 dark:hover:text-primary-400"
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
            <router-link
              :to="isAuthenticated ? dashboardPath : '/login'"
              class="btn-gold-sheen inline-flex w-full items-center justify-center gap-2 rounded-xl bg-primary-500 px-7 py-4 text-sm font-semibold text-neutral-900 shadow-gold-md"
              @click="closeMobileMenu()"
            >
              {{ isAuthenticated ? t('home.goToDashboard') : t('landing.hero.getStarted') }}
              <Icon name="arrowRight" size="sm" :stroke-width="2" />
            </router-link>
          </div>
        </div>
      </div>
    </transition>
    <div id="smooth-wrapper" class="relative z-[1]">
    <div id="smooth-content">
    <main class="relative flex-1">
      <!-- ============ Hero ============ -->
      <section id="top" class="relative overflow-hidden px-6 pb-24 pt-32 md:pt-40">
        <!-- Ambient background -->
        <div class="pointer-events-none absolute inset-0 overflow-hidden">
          <div
            ref="heroBlob"
            class="hero-blob absolute -right-40 -top-32 h-[36rem] w-[36rem] rounded-full bg-primary-400/20 blur-[130px] dark:bg-primary-500/15"
          ></div>
          <div class="pointer-events-none absolute -left-48 top-1/3 h-[28rem] w-[28rem] rounded-full bg-primary-300/15 blur-[140px] dark:bg-primary-600/10"></div>
          <div class="absolute inset-0 bg-[linear-gradient(rgba(224,160,8,0.045)_1px,transparent_1px),linear-gradient(90deg,rgba(224,160,8,0.045)_1px,transparent_1px)] bg-[size:64px_64px] [mask-image:radial-gradient(ellipse_at_center,black,transparent_72%)] dark:bg-[linear-gradient(rgba(238,177,17,0.05)_1px,transparent_1px),linear-gradient(90deg,rgba(238,177,17,0.05)_1px,transparent_1px)]"></div>
        </div>

        <div ref="heroInner" class="relative mx-auto max-w-7xl">
          <!-- Availability pill -->
          <div class="hero-fade mb-8 flex justify-center lg:justify-start">
            <span class="inline-flex items-center gap-2 rounded-full border border-neutral-200/80 bg-white/60 px-3.5 py-1.5 text-xs font-medium text-neutral-600 backdrop-blur dark:border-white/10 dark:bg-white/5 dark:text-neutral-300">
              <span class="relative flex h-2 w-2">
                <span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary-400 opacity-75"></span>
                <span class="relative inline-flex h-2 w-2 rounded-full bg-primary-500"></span>
              </span>
              {{ COMPANY_TAGLINE }} · {{ t('landing.hero.trustUptime') }}
            </span>
          </div>

          <div class="grid items-center gap-14 lg:grid-cols-[1.05fr_0.95fr]">
            <!-- Left: Text -->
            <div class="text-center lg:text-left">
              <h1 ref="heroHeadline" class="font-display text-[3rem] font-semibold leading-[0.98] tracking-[-0.03em] md:text-[4rem] lg:text-[4.75rem]">
                <span class="hero-line block">{{ t('landing.hero.line1') }}</span>
                <span class="hero-line block">{{ t('landing.hero.line2') }}</span>
                <span class="hero-line block text-gradient-hero">{{ t('landing.hero.line3') }}</span>
              </h1>

              <p class="hero-fade mx-auto mt-8 max-w-xl text-lg leading-relaxed text-neutral-500 dark:text-neutral-400 lg:mx-0">
                {{ t('landing.hero.subtitle') }}
              </p>

              <div class="hero-fade mt-10 flex flex-col items-center gap-3 sm:flex-row sm:justify-center lg:justify-start">
                <router-link
                  :to="isAuthenticated ? dashboardPath : '/login'"
                  data-cursor="launch"
                  class="magnetic btn-gold-sheen group inline-flex w-full items-center justify-center gap-2 rounded-xl bg-primary-500 px-7 py-3.5 text-sm font-semibold text-neutral-900 shadow-gold-md transition-all hover:bg-primary-400 hover:shadow-gold-lg sm:w-auto"
                >
                  {{ isAuthenticated ? t('home.goToDashboard') : t('landing.hero.getStarted') }}
                  <Icon name="arrowRight" size="sm" :stroke-width="2" class="transition-transform group-hover:translate-x-0.5" />
                </router-link>
                <a
                  :href="SOCIAL_LINKS.telegramChannel"
                  target="_blank"
                  rel="noopener noreferrer"
                  data-cursor="say hi"
                  class="magnetic inline-flex w-full items-center justify-center gap-2 rounded-xl border border-neutral-300 px-7 py-3.5 text-sm font-semibold text-neutral-700 transition-colors hover:border-primary-500/50 hover:text-neutral-900 dark:border-white/15 dark:text-neutral-200 dark:hover:border-primary-400/40 dark:hover:text-white sm:w-auto"
                >
                  {{ t('landing.hero.talkToUs') }}
                </a>
              </div>

              <div class="hero-fade mt-10 flex flex-wrap items-center justify-center gap-x-6 gap-y-2 text-sm text-neutral-500 dark:text-neutral-400 lg:justify-start">
                <span class="inline-flex items-center gap-1.5"><Icon name="shield" size="sm" class="text-primary-600 dark:text-primary-400" /> {{ t('landing.hero.trustSecurity') }}</span>
                <span class="inline-flex items-center gap-1.5"><Icon name="bolt" size="sm" class="text-primary-600 dark:text-primary-400" /> {{ t('landing.hero.trustUptime') }}</span>
                <span class="inline-flex items-center gap-1.5"><Icon name="dollar" size="sm" class="text-primary-600 dark:text-primary-400" /> {{ t('landing.hero.trustPayg') }}</span>
              </div>
            </div>

            <!-- Right: API Console mock -->
            <div class="hero-console flex justify-center lg:justify-end">
              <div class="console-window w-full max-w-[460px]">
                <div class="console-header">
                  <div class="console-dots">
                    <span></span><span></span><span></span>
                  </div>
                  <span class="console-title font-mono">POST /v1/messages</span>
                  <span class="console-status">200</span>
                </div>
                <div class="console-body font-mono">
                  <div class="cx-line cx-1"><span class="cx-key">model</span><span class="cx-punc">:</span> <span class="cx-str">"claude-sonnet-5"</span></div>
                  <div class="cx-line cx-2"><span class="cx-dim"># routing to best upstream…</span></div>
                  <div class="cx-line cx-3 route-row">
                    <span
                      v-for="p in consoleProviders"
                      :key="p"
                      class="route-chip"
                    >
                      <span class="route-logo" v-html="providerLogo(p)"></span>
                      {{ p }}
                    </span>
                  </div>
                  <div class="cx-line cx-4"><span class="cx-ok">✓ 200 OK</span> <span class="cx-dim">· 142 tokens · 0.9s</span></div>
                  <div class="cx-line cx-5"><span class="cx-punc">{</span> <span class="cx-key">"content"</span><span class="cx-punc">:</span> <span class="cx-str">"Hello, world"</span> <span class="cx-punc">}</span></div>
                  <div class="cx-line cx-6"><span class="cx-prompt">$</span> <span class="cx-caret"></span></div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>
      <!-- ============ Providers Marquee ============ -->
      <section class="relative border-y border-neutral-200/70 py-12 dark:border-white/10">
        <p class="reveal mb-8 px-6 text-center font-mono text-[11px] font-medium uppercase tracking-[0.25em] text-neutral-400 dark:text-neutral-500">
          {{ t('landing.trust.heading') }}
        </p>
        <div class="pointer-events-none absolute inset-y-0 left-0 z-10 w-28 bg-gradient-to-r from-[#fbfaf6] to-transparent dark:from-dark-950"></div>
        <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-28 bg-gradient-to-l from-[#fbfaf6] to-transparent dark:from-dark-950"></div>
        <div class="marquee">
          <div ref="providerTrack" class="marquee-track flex w-max items-center gap-6">
            <span
              v-for="(p, i) in providersLoop"
              :key="p.name + i"
              class="inline-flex shrink-0 items-center gap-2.5 rounded-xl border border-neutral-200/70 bg-white/50 px-5 py-3 text-sm font-medium backdrop-blur dark:border-white/10 dark:bg-white/[0.03]"
              :class="p.soon ? 'text-neutral-400 dark:text-neutral-600' : 'text-neutral-700 dark:text-neutral-200'"
            >
              <span v-if="p.logo" class="provider-logo h-4 w-4" :class="p.soon ? 'opacity-40' : ''" v-html="providerLogo(p.logo)"></span>
              <span v-else class="h-1.5 w-1.5 rounded-full bg-primary-500"></span>
              {{ p.name }}
              <span v-if="p.soon" class="font-mono text-[10px] uppercase tracking-wider text-neutral-400 dark:text-neutral-600">{{ t('landing.trust.soon') }}</span>
            </span>
          </div>
        </div>
      </section>

      <!-- ============ Stats ============ -->
      <section class="px-6 py-24 md:py-28">
        <div ref="statsWrap" class="mx-auto grid max-w-6xl grid-cols-2 gap-x-8 gap-y-14 md:grid-cols-4">
          <div v-for="s in stats" :key="s.label" class="stat-block text-center">
            <div class="stat-figure font-display">
              <span class="stat-num" :data-target="s.value" :data-decimals="s.decimals ?? 0">0</span><span class="stat-suffix text-gradient-hero">{{ s.suffix }}</span>
            </div>
            <div class="stat-tick mx-auto mt-5 h-px w-10 origin-left bg-gradient-to-r from-primary-400 to-transparent"></div>
            <p class="mt-3 font-mono text-[11px] font-medium uppercase tracking-[0.22em] text-neutral-500 dark:text-neutral-400">{{ t(s.label) }}</p>
          </div>
        </div>
      </section>

      <!-- ============ Feature Bento Grid ============ -->
      <section id="features" class="px-6 py-28">
        <div class="mx-auto max-w-7xl">
          <div class="mb-16 flex flex-col gap-6 md:flex-row md:items-end md:justify-between">
            <div class="max-w-2xl">
              <p class="reveal section-index mb-4">01 — {{ t('landing.features.eyebrow') }}</p>
              <h2 class="reveal font-display text-3xl font-semibold tracking-[-0.02em] md:text-[2.75rem] md:leading-[1.05]">
                {{ t('landing.features.title') }}
              </h2>
            </div>
            <p class="reveal max-w-md text-base text-neutral-500 dark:text-neutral-400">
              {{ t('landing.features.subtitle') }}
            </p>
          </div>

          <div ref="featureGrid" class="grid grid-cols-1 gap-4 md:grid-cols-6">
            <div
              v-for="f in features"
              :key="f.title"
              class="feature-cell tilt group relative overflow-hidden rounded-2xl border border-neutral-200/70 bg-white/40 p-8 transition-colors dark:border-white/10 dark:bg-white/[0.02]"
              :class="f.span"
              data-cursor="explore"
            >
              <span class="feature-spot pointer-events-none absolute -inset-px opacity-0 transition-opacity duration-300 group-hover:opacity-100"></span>
              <div class="tilt-inner relative flex h-full flex-col">
                <div class="mb-6 inline-flex h-11 w-11 items-center justify-center rounded-xl border border-neutral-200/70 text-primary-600 transition-transform duration-300 group-hover:-translate-y-0.5 group-hover:scale-110 dark:border-white/10 dark:text-primary-400">
                  <Icon :name="f.icon" size="md" />
                </div>
                <h3 class="mb-2 font-display text-base font-semibold tracking-tight">{{ t(f.title) }}</h3>
                <p class="text-sm leading-relaxed text-neutral-500 dark:text-neutral-400">{{ t(f.desc) }}</p>
                <!-- Feature cell mini-visual for the wide cell -->
                <div v-if="f.big" class="mt-auto pt-8">
                  <div class="flex flex-wrap gap-2">
                    <span v-for="m in bentoModels" :key="m.name" class="inline-flex items-center gap-1.5 rounded-lg border border-neutral-200/70 bg-white/60 px-2.5 py-1.5 text-xs font-medium text-neutral-600 dark:border-white/10 dark:bg-white/[0.03] dark:text-neutral-300">
                      <span class="h-3.5 w-3.5" v-html="providerLogo(m.logo)"></span>{{ m.name }}
                    </span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- ============ How it works ============ -->
      <section id="how" ref="howSection" class="px-6 py-28">
        <div class="mx-auto max-w-6xl">
          <div class="mb-16 max-w-2xl">
            <p class="reveal section-index mb-4">02 — {{ t('landing.steps.eyebrow') }}</p>
            <h2 class="reveal font-display text-3xl font-semibold tracking-[-0.02em] md:text-[2.75rem]">{{ t('landing.steps.title') }}</h2>
            <p class="reveal mt-4 text-lg text-neutral-500 dark:text-neutral-400">{{ t('landing.steps.subtitle') }}</p>
          </div>

          <div ref="stepsWrap" class="relative">
            <div class="pointer-events-none absolute left-0 right-0 top-6 hidden md:block">
              <div class="step-line h-px w-full origin-left bg-gradient-to-r from-primary-500/60 via-primary-400/30 to-transparent"></div>
            </div>
            <div class="grid gap-10 md:grid-cols-3">
              <div v-for="(step, i) in steps" :key="step.title" class="step-item relative">
                <div class="step-num mb-5 inline-flex h-12 w-12 items-center justify-center rounded-xl border border-neutral-200/70 bg-[#fbfaf6] font-mono text-sm font-semibold text-primary-600 transition-shadow duration-500 dark:border-white/10 dark:bg-dark-950 dark:text-primary-400">
                  {{ String(i + 1).padStart(2, '0') }}
                </div>
                <h3 class="mb-2 font-display text-base font-semibold tracking-tight">{{ t(step.title) }}</h3>
                <p class="text-sm leading-relaxed text-neutral-500 dark:text-neutral-400">{{ t(step.desc) }}</p>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- ============ Company ============ -->
      <section id="company" class="border-t border-neutral-200/70 px-6 py-28 dark:border-white/10">
        <div class="mx-auto grid max-w-6xl items-center gap-12 lg:grid-cols-2">
          <div>
            <p class="reveal section-index mb-4">03 — {{ t('landing.company.eyebrow', { company: PARENT_COMPANY }) }}</p>
            <h2 class="reveal scramble font-display text-3xl font-semibold tracking-[-0.02em] md:text-[2.75rem] md:leading-[1.08]">
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
            <span class="big-type font-display text-neutral-900 dark:text-neutral-100">{{ siteName }}</span>
            <span class="big-type-outline">✳</span>
            <span class="big-type font-display text-gradient">{{ COMPANY_TAGLINE }}</span>
            <span class="big-type-outline">✳</span>
          </template>
        </div>
      </section>

      <!-- ============ Final CTA ============ -->
      <section class="px-6 pb-28">
        <div class="reveal relative mx-auto max-w-6xl overflow-hidden rounded-3xl border border-neutral-200/70 bg-white px-8 py-20 text-center dark:border-white/10 dark:bg-white/[0.02]">
          <div class="pointer-events-none absolute inset-0 bg-mesh-gradient opacity-70"></div>
          <div class="relative">
            <p class="section-index mb-4 justify-center">{{ t('landing.cta.eyebrow') }}</p>
            <h2 class="mx-auto max-w-2xl font-display text-3xl font-semibold tracking-[-0.02em] md:text-[2.75rem] md:leading-[1.05]">
              {{ t('landing.cta.title', { siteName }) }}
            </h2>
            <p class="mx-auto mt-4 max-w-xl text-lg text-neutral-500 dark:text-neutral-400">
              {{ t('landing.cta.subtitle') }}
            </p>
            <router-link
              :to="isAuthenticated ? dashboardPath : '/login'"
              data-cursor="launch"
              class="magnetic btn-gold-sheen mt-10 inline-flex items-center gap-2 rounded-xl bg-primary-500 px-8 py-3.5 text-sm font-semibold text-neutral-900 shadow-gold-md transition-all hover:bg-primary-400 hover:shadow-gold-lg"
            >
              {{ isAuthenticated ? t('home.goToDashboard') : t('landing.hero.getStarted') }}
              <Icon name="arrowRight" size="sm" :stroke-width="2" />
            </router-link>
          </div>
        </div>
      </section>
    </main>
    <!-- ============ Footer ============ -->
    <footer class="border-t border-neutral-200/70 px-6 py-16 dark:border-white/10">
      <div class="mx-auto max-w-7xl">
        <div class="grid gap-10 md:grid-cols-2 lg:grid-cols-5">
          <div class="lg:col-span-2">
            <div class="mb-4 flex items-center gap-3">
              <div class="h-9 w-9 overflow-hidden rounded-xl ring-1 ring-neutral-200/70 dark:ring-white/10">
                <img :src="siteLogo || '/logo.png'" alt="Logo" class="h-full w-full object-contain" />
              </div>
              <div class="leading-none">
                <span class="block font-display text-[15px] font-semibold tracking-tight">{{ siteName }}</span>
                <span class="mt-1 block font-mono text-[9px] font-medium uppercase tracking-[0.22em] text-primary-600 dark:text-primary-400">
                  A {{ PARENT_COMPANY }} Company
                </span>
              </div>
            </div>
            <p class="mb-5 max-w-xs text-sm text-neutral-500 dark:text-neutral-400">{{ COMPANY_TAGLINE }}</p>
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

          <div>
            <h4 class="mb-4 font-mono text-[11px] font-semibold uppercase tracking-[0.2em] text-neutral-400 dark:text-neutral-500">{{ t('landing.footer.product') }}</h4>
            <ul class="space-y-3">
              <li><a href="#features" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.features') }}</a></li>
              <li><a href="#how" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.howItWorks') }}</a></li>
              <li><router-link to="/login" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.login') }}</router-link></li>
              <li v-if="docUrl"><a :href="docUrl" target="_blank" rel="noopener noreferrer" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.docs') }}</a></li>
            </ul>
          </div>

          <div>
            <h4 class="mb-4 font-mono text-[11px] font-semibold uppercase tracking-[0.2em] text-neutral-400 dark:text-neutral-500">{{ t('landing.footer.community') }}</h4>
            <ul class="space-y-3">
              <li v-for="social in socialLinks" :key="social.label">
                <a :href="social.url" target="_blank" rel="noopener noreferrer" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">
                  {{ social.label }}
                </a>
              </li>
            </ul>
          </div>

          <div>
            <h4 class="mb-4 font-mono text-[11px] font-semibold uppercase tracking-[0.2em] text-neutral-400 dark:text-neutral-500">{{ t('landing.footer.company') }}</h4>
            <ul class="space-y-3">
              <li><a href="#company" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.about', { company: PARENT_COMPANY }) }}</a></li>
              <li><router-link to="/privacy" class="text-sm text-neutral-500 transition-colors hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100">{{ t('landing.footer.privacy') }}</router-link></li>
            </ul>
          </div>
        </div>

        <div class="mt-12 border-t border-neutral-200/70 pt-8 dark:border-white/10">
          <p class="text-center font-mono text-xs text-neutral-400 dark:text-neutral-500">
            {{ t('landing.footer.rights', { year: currentYear, siteName, company: PARENT_COMPANY }) }}
          </p>
        </div>
      </div>
    </footer>
    </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'
import gsap from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'
import { SplitText } from 'gsap/SplitText'
import { useAuthStore, useAppStore } from '@/stores'
import LocaleSwitcher from '@/components/common/LocaleSwitcher.vue'
import Icon from '@/components/icons/Icon.vue'
import { PARENT_COMPANY, SOCIAL_LINKS, COMPANY_TAGLINE } from '@/config/brand'
import { sanitizeUrl } from '@/utils/url'

gsap.registerPlugin(ScrollTrigger, SplitText)

const { t } = useI18n()
const authStore = useAuthStore()
const appStore = useAppStore()

// Site settings
const siteName = computed(() => appStore.cachedPublicSettings?.site_name || appStore.siteName || 'Mishra Miron API')
const siteLogo = computed(() => sanitizeUrl(appStore.cachedPublicSettings?.site_logo || appStore.siteLogo || '', { allowRelative: true, allowDataUrl: true }))
const docUrl = computed(() => sanitizeUrl(appStore.cachedPublicSettings?.doc_url || appStore.docUrl || ''))
const homeContent = computed(() => appStore.cachedPublicSettings?.home_content || '')
const isHomeContentUrl = computed(() => {
  const content = homeContent.value.trim()
  return content.startsWith('http://') || content.startsWith('https://')
})

// Theme
const isDark = ref(document.documentElement.classList.contains('dark'))

// Auth
const isAuthenticated = computed(() => authStore.isAuthenticated)
const isAdmin = computed(() => authStore.isAdmin)
const dashboardPath = computed(() => (isAdmin.value ? '/admin/dashboard' : '/dashboard'))
const userInitial = computed(() => {
  const user = authStore.user
  if (!user || !user.email) return ''
  return user.email.charAt(0).toUpperCase()
})
const currentYear = computed(() => new Date().getFullYear())

// Header scroll state
const scrolled = ref(false)
function onScroll() {
  scrolled.value = window.scrollY > 20
}

// Mobile / tablet nav overlay
const mobileMenuOpen = ref(false)
function openMobileMenu() {
  mobileMenuOpen.value = true
  document.documentElement.classList.add('overflow-hidden')
}
function closeMobileMenu() {
  if (!mobileMenuOpen.value) return
  mobileMenuOpen.value = false
  document.documentElement.classList.remove('overflow-hidden')
}
function toggleMobileMenu() {
  if (mobileMenuOpen.value) {
    closeMobileMenu()
  } else {
    openMobileMenu()
  }
}
function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') closeMobileMenu()
}

// ---- Real provider brand logos (paths from @lobehub/icons, single-color) ----
const PROVIDER_LOGOS: Record<string, string> = {
  anthropic: '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M17.3 3.5h-3.1l5.6 17h3.2l-5.7-17ZM6.7 3.5 1 20.5h3.3l1.16-3.6h5.98l1.16 3.6h3.3L10.2 3.5H6.7Zm-.3 10.6L8.4 7.9l2 6.2H6.4Z"/></svg>',
  openai: '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M21.55 10.004a5.416 5.416 0 00-.478-4.501c-1.217-2.09-3.662-3.166-6.05-2.66A5.59 5.59 0 0010.831 1C8.39.995 6.224 2.546 5.473 4.838A5.553 5.553 0 001.76 7.496a5.487 5.487 0 00.691 6.5 5.416 5.416 0 00.477 4.502c1.217 2.09 3.662 3.165 6.05 2.66A5.586 5.586 0 0013.168 23c2.443.006 4.61-1.546 5.361-3.84a5.553 5.553 0 003.715-2.66 5.488 5.488 0 00-.693-6.497zm-8.381 11.558a4.199 4.199 0 01-2.675-.954l.132-.074 4.44-2.53a.71.71 0 00.364-.623v-6.176l1.877 1.069c.02.01.033.029.036.05v5.115c-.003 2.274-1.87 4.118-4.174 4.123zM4.192 17.78a4.059 4.059 0 01-.498-2.763l.131.078 4.44 2.53c.225.13.504.13.73 0l5.42-3.088v2.138a.068.068 0 01-.027.057L9.9 19.288c-1.999 1.136-4.552.46-5.707-1.51zM3.023 8.216A4.15 4.15 0 015.198 6.41l-.002.151v5.06a.711.711 0 00.364.624l5.42 3.087-1.876 1.07a.067.067 0 01-.063.005l-4.489-2.559c-1.995-1.14-2.679-3.658-1.53-5.63zm15.417 3.54-5.42-3.088L14.896 7.6a.067.067 0 01.063-.006l4.489 2.557c1.998 1.14 2.683 3.662 1.529 5.633a4.163 4.163 0 01-2.174 1.807V12.38a.71.71 0 00-.363-.623zm1.867-2.773a6.04 6.04 0 00-.132-.078l-4.44-2.53a.731.731 0 00-.729 0l-5.42 3.088V7.325a.068.068 0 01.027-.057L14.1 4.713c2-1.137 4.555-.46 5.707 1.513.487.833.664 1.809.499 2.757zm-11.741 3.81-1.877-1.068a.065.065 0 01-.036-.051V6.559c.001-2.277 1.873-4.122 4.181-4.12.976 0 1.92.338 2.671.954l-.131.073-4.44 2.53a.71.71 0 00-.365.623l-.003 6.173zm1.02-2.168L12 9.25l2.414 1.375v2.75L12 14.75l-2.415-1.375v-2.75z"/></svg>',
  gemini: '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z"/></svg>',
  gateway: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="6" rx="1"/><rect x="3" y="14" width="18" height="6" rx="1"/><path d="M7 7h.01M7 17h.01"/></svg>'
}
function providerLogo(key: string): string {
  return PROVIDER_LOGOS[key] ?? PROVIDER_LOGOS.gateway
}
const consoleProviders = ['anthropic', 'openai', 'gemini']
const bentoModels = [
  { name: 'Claude', logo: 'anthropic' },
  { name: 'GPT', logo: 'openai' },
  { name: 'Gemini', logo: 'gemini' }
]

// Center navigation anchors
const navLinks = computed(() => [
  { label: 'landing.nav.home', href: '/', home: true },
  { label: 'landing.nav.ide', href: '#features' },
  { label: 'landing.nav.docs', href: docUrl.value || '#', external: !!docUrl.value },
  { label: 'landing.nav.contact', href: SOCIAL_LINKS.telegramChannel, external: true }
])

function scrollToTop() {
  window.scrollTo({ top: 0 })
}

// Social / community links
const socialLinks = [
  { label: 'Telegram Bot', url: SOCIAL_LINKS.telegramBot, icon: 'telegramBot' },
  { label: 'Telegram Channel', url: SOCIAL_LINKS.telegramChannel, icon: 'telegram' },
  { label: 'Discord', url: SOCIAL_LINKS.discord, icon: 'discord' }
]

// Supported AI providers (trust strip)
const providers = [
  { name: 'Claude', soon: false, logo: 'anthropic' },
  { name: 'GPT', soon: false, logo: 'openai' },
  { name: 'Gemini', soon: false, logo: 'gemini' },
  { name: 'Antigravity', soon: false, logo: '' },
  { name: 'More', soon: true, logo: '' }
]
const providersLoop = [...providers, ...providers]

// Feature cards — bento spans (first cell wide/tall)
const features = [
  { icon: 'server', title: 'landing.features.unifiedTitle', desc: 'landing.features.unifiedDesc', span: 'md:col-span-4', big: true },
  { icon: 'users', title: 'landing.features.poolTitle', desc: 'landing.features.poolDesc', span: 'md:col-span-2', big: false },
  { icon: 'dollar', title: 'landing.features.billingTitle', desc: 'landing.features.billingDesc', span: 'md:col-span-2', big: false },
  { icon: 'sync', title: 'landing.features.routingTitle', desc: 'landing.features.routingDesc', span: 'md:col-span-2', big: false },
  { icon: 'chart', title: 'landing.features.analyticsTitle', desc: 'landing.features.analyticsDesc', span: 'md:col-span-2', big: false },
  { icon: 'shield', title: 'landing.features.securityTitle', desc: 'landing.features.securityDesc', span: 'md:col-span-6', big: false }
] as const

// How it works steps
const steps = [
  { title: 'landing.steps.step1Title', desc: 'landing.steps.step1Desc' },
  { title: 'landing.steps.step2Title', desc: 'landing.steps.step2Desc' },
  { title: 'landing.steps.step3Title', desc: 'landing.steps.step3Desc' }
]

// Headline stats (count-up on scroll)
const stats: Array<{ value: number; suffix: string; decimals?: number; label: string }> = [
  { value: 20, suffix: '+', label: 'landing.stats.models' },
  { value: 99.9, suffix: '%', decimals: 1, label: 'landing.stats.uptime' },
  { value: 0.9, suffix: 's', decimals: 1, label: 'landing.stats.latency' },
  { value: 24, suffix: '/7', label: 'landing.stats.support' }
]

// Template refs
const root = ref<HTMLElement | null>(null)
const heroBlob = ref<HTMLElement | null>(null)
const heroHeadline = ref<HTMLElement | null>(null)
const heroInner = ref<HTMLElement | null>(null)
const featureGrid = ref<HTMLElement | null>(null)
const stepsWrap = ref<HTMLElement | null>(null)
const statsWrap = ref<HTMLElement | null>(null)
const howSection = ref<HTMLElement | null>(null)
const cursorDot = ref<HTMLElement | null>(null)
const cursorRing = ref<HTMLElement | null>(null)
const cursorLabel = ref<HTMLElement | null>(null)
const providerTrack = ref<HTMLElement | null>(null)
const bigMarquee = ref<HTMLElement | null>(null)
const preloaderEl = ref<HTMLElement | null>(null)
const preloaderBar = ref<HTMLElement | null>(null)
const preloaderCount = ref<HTMLElement | null>(null)

const prefersReducedMotion =
  typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches

// Intro preloader doubles as the loading screen; skipped for reduced motion.
const showPreloader = ref(!prefersReducedMotion)
const FLUID_REVEAL_EVENT = 'miron-fluid-reveal'

const cleanupFns: Array<() => void> = []
let ctx: gsap.Context | undefined
let mm: gsap.MatchMedia | undefined
let splitInstances: SplitText[] = []
let heroTl: gsap.core.Timeline | undefined

function toggleTheme() {
  isDark.value = !isDark.value
  document.documentElement.classList.toggle('dark', isDark.value)
  localStorage.setItem('theme', isDark.value ? 'dark' : 'light')
}

function initTheme() {
  // Dark is the default brand experience; light is an explicit opt-in.
  if (localStorage.getItem('theme') !== 'light') {
    isDark.value = true
    document.documentElement.classList.add('dark')
  }
}

/** Reveal the already-warmed fluid and start the page only after the loader is gone. */
function revealLiveExperience() {
  window.dispatchEvent(new Event(FLUID_REVEAL_EVENT))
  heroTl?.play()
}

// Intro preloader: counter + progress bar, then the curtain parts. The live
// background and hero remain gated until both panels have completely exited.
function runIntro() {
  if (!preloaderEl.value) {
    showPreloader.value = false
    revealLiveExperience()
    return
  }

  document.documentElement.classList.add('overflow-hidden')
  const unlock = () => document.documentElement.classList.remove('overflow-hidden')
  cleanupFns.push(unlock)

  const counter = { v: 0 }
  const introTl = gsap.timeline({
    onComplete: () => {
      unlock()
      showPreloader.value = false
      revealLiveExperience()
    }
  })
  introTl
    .to(counter, {
      v: 100,
      duration: 1.5,
      ease: 'power2.inOut',
      onUpdate: () => {
        if (preloaderCount.value) preloaderCount.value.textContent = String(Math.round(counter.v)).padStart(2, '0')
      }
    }, 0)
    .fromTo('.preloader-bar-fill', { scaleX: 0 }, { scaleX: 1, duration: 1.5, ease: 'power2.inOut' }, 0)
    .from('.preloader-logo', { opacity: 0, y: 26, scale: 0.92, duration: 0.8, ease: 'power3.out' }, 0.05)
    .from('.preloader-name', { opacity: 0, y: 12, duration: 0.6, ease: 'power3.out' }, 0.25)
    .to('.preloader-center, .preloader-count', { opacity: 0, y: -20, duration: 0.35, ease: 'power2.in' }, 1.6)
    .to('.preloader-panel-top', { yPercent: -101, duration: 0.9, ease: 'power4.inOut' }, 1.9)
    .to('.preloader-panel-bottom', { yPercent: 101, duration: 0.9, ease: 'power4.inOut' }, 1.96)
}

function initAnimations() {
  if (prefersReducedMotion) return

  ctx = gsap.context(() => {
    // ---- Hero entrance choreography (played when the preloader curtain opens) ----
    heroTl = gsap.timeline({ paused: true })
    if (heroHeadline.value) {
      const split = new SplitText(heroHeadline.value.querySelectorAll('.hero-line'), {
        type: 'chars,lines',
        linesClass: 'hero-splitline'
      })
      splitInstances.push(split)
      // Gradient text can't clip through SplitText's transformed char divs — re-apply per char.
      split.chars.forEach((c) => {
        if ((c as HTMLElement).closest('.text-gradient-hero')) {
          (c as HTMLElement).classList.add('hero-char-gold')
        }
      })
      heroTl.from(split.chars, {
        yPercent: 120,
        opacity: 0,
        rotate: 4,
        duration: 0.9,
        ease: 'power4.out',
        stagger: 0.016
      }, 0)
    }
    heroTl.from('.hero-fade', { opacity: 0, y: 22, duration: 0.9, stagger: 0.1, ease: 'power3.out' }, 0.4)
    heroTl.from('.hero-console', {
      opacity: 0,
      y: 44,
      rotateX: 12,
      scale: 0.94,
      transformPerspective: 1200,
      duration: 1.1,
      ease: 'power3.out'
    }, 0.35)

    // Hero recedes with depth as you scroll past it
    if (heroInner.value) {
      gsap.to(heroInner.value, {
        yPercent: -12,
        opacity: 0.1,
        scale: 0.96,
        ease: 'none',
        scrollTrigger: { trigger: '#top', start: 'top top', end: 'bottom top', scrub: true }
      })
    }

    // Generic scroll reveals
    gsap.utils.toArray<HTMLElement>('.reveal').forEach((el) => {
      gsap.from(el, {
        opacity: 0,
        y: 26,
        duration: 0.85,
        ease: 'power3.out',
        scrollTrigger: { trigger: el, start: 'top 82%' }
      })
    })

    // Stats — reveal + count-up
    if (statsWrap.value) {
      gsap.from('.stat-block', {
        opacity: 0,
        y: 36,
        duration: 0.8,
        ease: 'power3.out',
        stagger: 0.09,
        scrollTrigger: { trigger: statsWrap.value, start: 'top 84%' }
      })
      gsap.from('.stat-tick', {
        scaleX: 0,
        duration: 0.9,
        ease: 'power2.out',
        stagger: 0.09,
        scrollTrigger: { trigger: statsWrap.value, start: 'top 84%' }
      })
      ScrollTrigger.create({
        trigger: statsWrap.value,
        start: 'top 80%',
        once: true,
        onEnter: () => {
          gsap.utils.toArray<HTMLElement>('.stat-num').forEach((el) => {
            const target = parseFloat(el.dataset.target ?? '0')
            const decimals = parseInt(el.dataset.decimals ?? '0', 10)
            const obj = { v: 0 }
            gsap.to(obj, {
              v: target,
              duration: 1.8,
              ease: 'power3.out',
              onUpdate: () => { el.textContent = obj.v.toFixed(decimals) }
            })
          })
        }
      })
    }

    // Feature bento cells — overshoot pop-in
    if (featureGrid.value) {
      gsap.from('.feature-cell', {
        opacity: 0,
        y: 44,
        scale: 0.95,
        duration: 0.75,
        ease: 'back.out(1.6)',
        stagger: { each: 0.08, from: 'start' },
        scrollTrigger: { trigger: featureGrid.value, start: 'top 82%' }
      })
    }

    // How-it-works: pinned scrub timeline on md+, simple stagger on small screens
    mm = gsap.matchMedia()
    mm.add('(min-width: 768px)', () => {
      if (!howSection.value || !stepsWrap.value) return
      const items = gsap.utils.toArray<HTMLElement>('.step-item')
      gsap.set(items, { autoAlpha: 0.16, y: 28 })
      const tl = gsap.timeline({
        scrollTrigger: {
          trigger: howSection.value,
          start: 'top top+=64',
          end: '+=160%',
          pin: true,
          scrub: 0.6,
          anticipatePin: 1
        }
      })
      tl.fromTo('.step-line', { scaleX: 0 }, { scaleX: 1, duration: 3, ease: 'none' }, 0)
      items.forEach((item, i) => {
        tl.to(item, { autoAlpha: 1, y: 0, duration: 0.7, ease: 'power2.out' }, i * 0.95 + 0.15)
        const num = item.querySelector('.step-num')
        if (num) {
          tl.to(num, {
            boxShadow: '0 0 26px rgba(255, 182, 28, 0.45), inset 0 1px 0 rgba(255, 233, 163, 0.4)',
            borderColor: 'rgba(255, 182, 28, 0.55)',
            duration: 0.4
          }, i * 0.95 + 0.35)
        }
      })
      tl.to({}, { duration: 0.6 })
    })
    mm.add('(max-width: 767px)', () => {
      if (!stepsWrap.value) return
      gsap.from('.step-line', {
        scaleX: 0,
        duration: 1.2,
        ease: 'power2.out',
        scrollTrigger: { trigger: stepsWrap.value, start: 'top 78%' }
      })
      gsap.from('.step-item', {
        opacity: 0,
        x: -32,
        duration: 0.7,
        ease: 'power3.out',
        stagger: 0.15,
        scrollTrigger: { trigger: stepsWrap.value, start: 'top 78%' }
      })
    })

    // Parallax hero blob
    if (heroBlob.value) {
      gsap.to(heroBlob.value, {
        yPercent: 42,
        ease: 'none',
        scrollTrigger: { trigger: root.value, start: 'top top', end: 'bottom top', scrub: true }
      })
    }

    // Console idle float
    gsap.to('.console-window', { y: -10, duration: 2.8, ease: 'sine.inOut', repeat: -1, yoyo: true, delay: 3.4 })

    // Provider marquee
    if (providerTrack.value) {
      gsap.to(providerTrack.value, { xPercent: -50, duration: 42, ease: 'none', repeat: -1 })
    }

    // Giant brand marquee: infinite loop that reacts to scroll velocity (skew + speed + direction)
    if (bigMarquee.value) {
      const loop = gsap.to(bigMarquee.value, { xPercent: -50, duration: 24, ease: 'none', repeat: -1 })
      const skewSetter = gsap.quickSetter(bigMarquee.value, 'skewX', 'deg')
      const proxy = { skew: 0 }
      ScrollTrigger.create({
        onUpdate: (self) => {
          const velocity = self.getVelocity()
          const skew = gsap.utils.clamp(-9, 9, velocity / -350)
          if (Math.abs(skew) > Math.abs(proxy.skew)) {
            proxy.skew = skew
            gsap.to(proxy, {
              skew: 0,
              duration: 0.9,
              ease: 'power3.out',
              overwrite: true,
              onUpdate: () => skewSetter(proxy.skew)
            })
          }
          gsap.to(loop, {
            timeScale: (velocity < 0 ? -1 : 1) * gsap.utils.clamp(1, 3.2, 1 + Math.abs(velocity) / 1100),
            duration: 0.5,
            overwrite: 'auto'
          })
        }
      })
      gsap.fromTo(bigMarquee.value, { x: 120 }, {
        x: -120,
        ease: 'none',
        scrollTrigger: { trigger: bigMarquee.value, start: 'top bottom', end: 'bottom top', scrub: true }
      })
    }

    // Scramble headline on scroll-in
    gsap.utils.toArray<HTMLElement>('.scramble').forEach((el) => {
      ScrollTrigger.create({ trigger: el, start: 'top 80%', once: true, onEnter: () => scrambleText(el) })
    })
  }, root.value ?? undefined)
}

// Decode/scramble text animation
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
      out += i < revealCount ? finalText[i] : chars[Math.floor(Math.random() * chars.length)]
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

  rootEl.querySelectorAll<HTMLElement>('.tilt').forEach((el) => {
    const inner = el.querySelector<HTMLElement>('.tilt-inner')
    const spot = el.querySelector<HTMLElement>('.feature-spot')
    const move = (e: MouseEvent) => {
      const rect = el.getBoundingClientRect()
      const px = (e.clientX - rect.left) / rect.width
      const py = (e.clientY - rect.top) / rect.height
      gsap.to(inner, { rotateY: (px - 0.5) * 8, rotateX: (0.5 - py) * 8, duration: 0.4, ease: 'power2.out', transformPerspective: 900 })
      if (spot) spot.style.background = `radial-gradient(260px circle at ${px * 100}% ${py * 100}%, rgba(212,160,23,0.15), transparent 62%)`
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
  authStore.checkAuth()
  if (!appStore.publicSettingsLoaded) appStore.fetchPublicSettings()

  window.addEventListener('scroll', onScroll, { passive: true })
  window.addEventListener('keydown', onKeydown)
  onScroll()

  initAnimations()
  initInteractions()

  if (showPreloader.value) {
    runIntro()
  } else {
    revealLiveExperience()
  }
})

onBeforeUnmount(() => {
  window.removeEventListener('scroll', onScroll)
  window.removeEventListener('keydown', onKeydown)
  closeMobileMenu()
  cleanupFns.forEach((fn) => fn())
  cleanupFns.length = 0
  splitInstances.forEach((s) => s.revert())
  splitInstances = []
  heroTl?.kill()
  heroTl = undefined
  mm?.revert()
  mm = undefined
  ctx?.revert()
  ScrollTrigger.getAll().forEach((trigger) => trigger.kill())
})
</script>

<style scoped>
/* ===================== Typography accents ===================== */
.text-gradient-hero {
  background: linear-gradient(120deg, #ffe9a3 0%, #ffb61c 45%, #c77b06 100%);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
  filter: drop-shadow(0 0 26px rgba(255, 182, 28, 0.28));
}
/* SplitText chars re-apply the gold clip per glyph (see initAnimations) */
:deep(.hero-char-gold) {
  background: linear-gradient(120deg, #ffe9a3 0%, #ffb61c 45%, #c77b06 100%);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}
.text-gradient {
  background: linear-gradient(90deg, #b8860b 0%, #d4a017 50%, #ecc968 100%);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}
.section-index {
  display: inline-flex;
  align-items: center;
  font-family: 'JetBrains Mono', ui-monospace, monospace;
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: #c68410;
}
.dark .section-index {
  color: #eeb111;
}

/* SplitText line masks the chars during reveal */
:deep(.hero-splitline) {
  overflow: hidden;
  /* Leave room for descenders (y, g, p, q, j) while the line is masked. */
  padding-bottom: 0.16em;
  /* SplitText hard-codes text-align: left inline on line divs; keep the
     responsive centered layout below lg. */
  text-align: inherit !important;
}

/* ===================== Hamburger + mobile nav ===================== */
.burger {
  position: relative;
  display: block;
  width: 20px;
  height: 14px;
}
.burger span {
  position: absolute;
  left: 0;
  width: 100%;
  height: 2px;
  border-radius: 999px;
  background: currentColor;
  transition: transform 0.35s cubic-bezier(0.22, 1, 0.36, 1), top 0.35s ease;
}
.burger span:first-child { top: 2px; }
.burger span:last-child { top: 10px; }
.burger.is-open span:first-child { top: 6px; transform: rotate(45deg); }
.burger.is-open span:last-child { top: 6px; transform: rotate(-45deg); }

.mnav-link {
  position: relative;
  display: flex;
  align-items: baseline;
  gap: 14px;
  padding: 18px 0;
  font-size: clamp(1.75rem, 6vw, 2.5rem);
  font-weight: 600;
  letter-spacing: -0.02em;
  border-bottom: 1px solid rgba(128, 108, 84, 0.18);
  transition: opacity 0.5s ease, transform 0.55s cubic-bezier(0.22, 1, 0.36, 1), color 0.25s ease;
}
.mnav-link:hover { color: #f0a50a; }
.mnav-index {
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.18em;
  color: #c68410;
}
.dark .mnav-index { color: #ffb61c; }
.mnav-footer {
  transition: opacity 0.5s ease, transform 0.55s cubic-bezier(0.22, 1, 0.36, 1);
}
.mnav-enter-active,
.mnav-leave-active { transition: opacity 0.35s ease; }
.mnav-enter-from,
.mnav-leave-to { opacity: 0; }
.mnav-enter-from .mnav-link,
.mnav-enter-from .mnav-footer { opacity: 0; transform: translateY(24px); }

/* Nav link hover underline */
.nav-link::after {
  content: '';
  position: absolute;
  left: 50%;
  bottom: 4px;
  height: 1.5px;
  width: 0;
  background: currentColor;
  transition: width 0.3s ease, left 0.3s ease;
}
.nav-link:hover::after {
  left: 16px;
  width: calc(100% - 32px);
}

/* ===================== Preloader ===================== */
.preloader-panel {
  position: absolute;
  left: 0;
  right: 0;
  height: 50.5%;
  background: #080502;
  will-change: transform;
}
.preloader-panel-top {
  top: 0;
  background-image: radial-gradient(90% 120% at 50% 100%, rgba(240, 165, 10, 0.07), transparent 60%);
}
.preloader-panel-bottom {
  bottom: 0;
  background-image: radial-gradient(90% 120% at 50% 0%, rgba(240, 165, 10, 0.05), transparent 60%);
}
.preloader-center {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 18px;
}
.preloader-logo {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 96px;
  height: 96px;
}
.preloader-logo-img {
  width: 64px;
  height: 64px;
  object-fit: contain;
  border-radius: 18px;
  filter: drop-shadow(0 0 28px rgba(255, 182, 28, 0.35));
}
/* Thin rotating gold arc around the logo — quiet, precise, expensive-looking */
.preloader-logo-ring {
  position: absolute;
  inset: 0;
  border-radius: 9999px;
  background: conic-gradient(
    from 0deg,
    transparent 0deg,
    transparent 288deg,
    rgba(255, 182, 28, 0.9) 342deg,
    rgba(255, 233, 163, 1) 360deg
  );
  -webkit-mask: radial-gradient(farthest-side, transparent calc(100% - 2px), black calc(100% - 1.5px));
  mask: radial-gradient(farthest-side, transparent calc(100% - 2px), black calc(100% - 1.5px));
  animation: preloader-ring-spin 1.4s linear infinite;
}
@keyframes preloader-ring-spin {
  to { transform: rotate(360deg); }
}
.preloader-name {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.34em;
  text-transform: uppercase;
  color: #b8a890;
}
.preloader-bar {
  width: 168px;
  height: 2px;
  border-radius: 999px;
  background: rgba(255, 233, 163, 0.12);
  overflow: hidden;
}
.preloader-bar-fill {
  display: block;
  width: 100%;
  height: 100%;
  transform-origin: left;
  background: linear-gradient(90deg, #9c6205, #ffb61c 60%, #ffe9a3);
  box-shadow: 0 0 12px rgba(255, 182, 28, 0.8);
}
.preloader-count {
  position: absolute;
  right: max(4vw, 24px);
  bottom: max(4vh, 20px);
  font-size: clamp(3.5rem, 9vw, 7rem);
  font-weight: 600;
  line-height: 1;
  letter-spacing: -0.04em;
  font-variant-numeric: tabular-nums;
  background: linear-gradient(120deg, #ffe9a3 0%, #ffb61c 50%, #c77b06 100%);
  -webkit-background-clip: text;
  background-clip: text;
  -webkit-text-fill-color: transparent;
}

/* ===================== Stats ===================== */
.stat-figure {
  font-size: clamp(2.6rem, 6vw, 4.4rem);
  font-weight: 600;
  line-height: 1;
  letter-spacing: -0.03em;
}
.stat-num {
  font-variant-numeric: tabular-nums;
}
.stat-suffix {
  margin-left: 2px;
}

/* ===================== Gold CTA sheen sweep ===================== */
.btn-gold-sheen {
  position: relative;
  overflow: hidden;
  box-shadow:
    inset 0 1px 0 rgba(255, 244, 200, 0.5),
    inset 0 -1px 0 rgba(62, 36, 9, 0.35),
    0 6px 24px rgba(240, 165, 10, 0.3);
}
.btn-gold-sheen::after {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(110deg, transparent 25%, rgba(255, 244, 200, 0.55) 50%, transparent 75%);
  background-size: 220% 100%;
  background-position: 130% 0;
  opacity: 0;
  transition: opacity 0.2s ease;
  pointer-events: none;
}
.btn-gold-sheen:hover::after {
  opacity: 1;
  animation: gold-sheen-sweep 1.1s ease forwards;
}
@keyframes gold-sheen-sweep {
  from { background-position: 130% 0; }
  to { background-position: -110% 0; }
}

/* ===================== Film grain ===================== */
.grain {
  opacity: 0.035;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='120' height='120'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='3' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
  mix-blend-mode: multiply;
}
.dark .grain {
  opacity: 0.05;
  mix-blend-mode: screen;
}

/* ===================== API Console ===================== */
.console-window {
  background: linear-gradient(160deg, #1d1611 0%, #080502 100%);
  border-radius: 14px;
  border: 1px solid rgba(255, 182, 28, 0.14);
  box-shadow:
    0 34px 70px -30px rgba(0, 0, 0, 0.7),
    0 0 44px -8px rgba(240, 165, 10, 0.14),
    inset 0 1px 0 rgba(255, 233, 163, 0.14);
  overflow: hidden;
  transform: perspective(1400px) rotateX(1.5deg) rotateY(-1.5deg);
  transition: transform 0.5s ease, box-shadow 0.5s ease;
}
.console-window:hover {
  transform: perspective(1400px) rotateX(0deg) rotateY(0deg) translateY(-4px);
  box-shadow:
    0 44px 90px -32px rgba(0, 0, 0, 0.75),
    0 0 64px -8px rgba(255, 182, 28, 0.22),
    inset 0 1px 0 rgba(255, 233, 163, 0.18);
}
.console-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  background: rgba(255, 255, 255, 0.02);
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}
.console-dots {
  display: flex;
  gap: 7px;
}
.console-dots span {
  width: 11px;
  height: 11px;
  border-radius: 50%;
  background: #6b6255;
}
.console-title {
  flex: 1;
  font-size: 12px;
  color: #8a7a63;
  letter-spacing: 0.02em;
}
.console-status {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  font-weight: 700;
  color: #1d1103;
  background: linear-gradient(150deg, #ffe9a3 0%, #ffb61c 55%, #d98d0c 100%);
  padding: 2px 8px;
  border-radius: 6px;
  box-shadow: inset 0 1px 0 rgba(255, 244, 200, 0.6), 0 0 14px rgba(255, 182, 28, 0.45);
}
.console-body {
  padding: 20px 22px;
  font-size: 13px;
  line-height: 2;
}
.cx-line {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  opacity: 0;
  transform: translateY(6px);
  animation: cx-appear 0.5s ease forwards;
}
.cx-1 { animation-delay: 0.7s; }
.cx-2 { animation-delay: 1.2s; }
.cx-3 { animation-delay: 1.7s; }
.cx-4 { animation-delay: 2.3s; }
.cx-5 { animation-delay: 2.8s; }
.cx-6 { animation-delay: 3.2s; }
@keyframes cx-appear {
  to { opacity: 1; transform: translateY(0); }
}
.cx-key { color: #ecc968; }
.cx-str { color: #b8a890; }
.cx-punc { color: #6b6255; }
.cx-dim { color: #6b6255; font-style: italic; }
.cx-ok {
  color: #0d0a07;
  background: rgba(212, 160, 23, 0.9);
  padding: 1px 8px;
  border-radius: 5px;
  font-weight: 700;
}
.cx-prompt { color: #e0a82e; font-weight: 600; }
.route-row { gap: 8px; }
.route-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 9px;
  border-radius: 7px;
  font-size: 11px;
  color: #ded3c3;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.07);
  text-transform: capitalize;
}
.route-logo {
  display: inline-flex;
  width: 13px;
  height: 13px;
  color: #ecc968;
}
.route-logo :deep(svg) { width: 100%; height: 100%; }
.cx-caret {
  display: inline-block;
  width: 8px;
  height: 15px;
  background: #e0a82e;
  animation: blink 1s step-end infinite;
}
@keyframes blink {
  0%, 50% { opacity: 1; }
  51%, 100% { opacity: 0; }
}

/* ===================== Provider logos in DOM ===================== */
.provider-logo :deep(svg),
.route-logo :deep(svg) {
  width: 100%;
  height: 100%;
}

/* ===================== Custom cursor ===================== */
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
  width: 8px;
  height: 8px;
  margin: -4px 0 0 -4px;
  background: #f4c542;
  border: 1px solid rgba(255, 255, 255, 0.9);
  box-shadow: 0 0 10px rgba(244, 197, 66, 0.9);
}
.cursor-ring {
  width: 42px;
  height: 42px;
  margin: -21px 0 0 -21px;
  border: 2px solid rgba(244, 197, 66, 0.95);
  background: rgba(244, 197, 66, 0.08);
  box-shadow: 0 0 0 1px rgba(16, 12, 8, 0.45), 0 0 16px rgba(244, 197, 66, 0.45);
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
  background: rgba(13, 10, 7, 0.82);
  border-color: rgba(255, 255, 255, 0.18);
}
.cursor-label {
  font-family: 'JetBrains Mono', monospace;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.09em;
  color: #f5eee0;
  opacity: 0;
  transition: opacity 0.2s ease;
  white-space: nowrap;
}
.cursor-ring.has-label .cursor-label {
  opacity: 1;
}
.cursor-active,
.cursor-active * {
  cursor: none !important;
}

/* ===================== Marquees ===================== */
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
  letter-spacing: -0.03em;
  line-height: 1;
}
.big-type-outline {
  font-size: clamp(1.5rem, 5vw, 3.5rem);
  line-height: 1;
  color: transparent;
  -webkit-text-stroke: 1px rgba(255, 182, 28, 0.8);
  filter: drop-shadow(0 0 10px rgba(255, 182, 28, 0.35));
}

/* ===================== Feature tilt ===================== */
.tilt {
  transform-style: preserve-3d;
}
.tilt-inner {
  transform-style: preserve-3d;
}
.feature-spot {
  z-index: 0;
  border-radius: inherit;
}

/* ===================== Reduced motion ===================== */
@media (prefers-reduced-motion: reduce) {
  .cx-line { opacity: 1; transform: none; animation: none; }
  .cx-caret { animation: none; }
  .console-window { transform: none; }
  .marquee-track,
  .marquee-hero > div { transform: none !important; }
  .grain { display: none; }
  .btn-gold-sheen::after { display: none; }
}
</style>
