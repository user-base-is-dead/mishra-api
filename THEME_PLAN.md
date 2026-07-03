# Mishra API — Premium Gold Theme Plan

> Goal: Give the entire app (web + admin) a cohesive, premium "liquid gold" finish
> that matches the logo (gold `M` + `</>` on cream, gold bezel). Gradient-rich,
> luxury feel. Light + dark mode both polished.
> Execute this plan file top-to-bottom. After each phase, run the verify step.

---

## 0. Brand Palette (source of truth)

Derived from the logo:

| Token | Hex | Use |
|-------|-----|-----|
| Gold light | `#f4e0a0` | highlights, hover glows |
| Gold (primary) | `#d4a017` | main brand, buttons, active states |
| Gold deep | `#b8860b` | gradient end, pressed states |
| Bronze | `#6b4a1e` | dark accents, deep text on gold |
| Cream | `#fdf9ed` | light backgrounds, cards |
| Ink (warm black) | `#1a1510` | dark-mode base (warm, not cold slate) |

Gradient signature (use everywhere for the "finish"):
- **Primary gradient**: `linear-gradient(135deg, #f4e0a0 0%, #d4a017 45%, #b8860b 100%)`
- **Metallic sheen** (buttons/headers): `linear-gradient(135deg, #ecc968 0%, #d4a017 50%, #96690c 100%)`
- **Subtle bg mesh**: warm gold radial tints on cream/ink.

---

## Phase 1 — Design tokens (`frontend/tailwind.config.js`)

Already done: `primary` scale is gold. Now extend for premium finish.

1. **Keep** the gold `primary` scale (done).
2. **Add a warm dark scale** — replace the cold slate `dark` scale with warm charcoal:
   ```
   dark: {
     50:'#faf7f2',100:'#f0e9df',200:'#ded3c3',300:'#b8a890',
     400:'#8a7a63',500:'#5f5240',600:'#443a2c',700:'#2e2820',
     800:'#211c16',900:'#17130e',950:'#0d0a07'
   }
   ```
   (Warm brown-black instead of blue-black → matches gold.)
3. **Add gradient tokens** under `backgroundImage`:
   ```
   'gradient-gold': 'linear-gradient(135deg,#f4e0a0 0%,#d4a017 45%,#b8860b 100%)',
   'gradient-gold-sheen': 'linear-gradient(135deg,#ecc968 0%,#d4a017 50%,#96690c 100%)',
   'gradient-gold-text': 'linear-gradient(90deg,#b8860b 0%,#d4a017 50%,#ecc968 100%)',
   ```
   Update existing `mesh-gradient` to warm gold tints (already gold — verify).
4. **Add gold glow shadows** (already partially done). Add:
   ```
   'gold-sm':'0 2px 8px rgba(212,160,23,0.20)',
   'gold-md':'0 6px 24px rgba(212,160,23,0.28)',
   'gold-lg':'0 12px 48px rgba(184,134,11,0.35)',
   ```

**Verify:** `pnpm --dir frontend exec vue-tsc --noEmit` → 0 errors; restart `pnpm dev`.

---

## Phase 2 — Global components (`frontend/src/style.css`)

These classes are used app-wide (web + admin). Changing here reskins everything at once.

1. **`.btn-primary`** → metallic gold sheen:
   ```
   @apply bg-gradient-to-br from-primary-300 via-primary-500 to-primary-700;
   @apply text-white shadow-gold-md;
   @apply hover:from-primary-400 hover:via-primary-600 hover:to-primary-800 hover:shadow-gold-lg;
   ```
2. **`.text-gradient`** → use `from-primary-700 via-primary-500 to-primary-300` (gold text gradient) instead of primary→accent.
3. **`.progress-bar`** → `from-primary-600 via-primary-500 to-primary-300` for a gold fill.
4. **`.card` / `.glass-card`** → add a faint warm tint in light mode: `bg-primary-50/40` overlay option; keep white base. In dark mode use `dark-800/60`.
5. **`.sidebar-link-active`** → gold left-accent bar:
   - keep `bg-primary-50 dark:bg-primary-900/20 text-primary-600`
   - add a `before:` pseudo gold bar (or a `border-l-2 border-primary-500`).
6. **`.stat-icon-primary`** → gold tint (already via primary). Add subtle `shadow-gold-sm`.
7. **`.badge-primary`, `.code`, `.toast-info`** → already primary/gold; verify contrast.
8. **`::selection`** → already primary; fine.

**Verify:** Load `/` and `/admin` — buttons, sidebar active item, progress bars, badges all gold with gradient sheen.

---

## Phase 3 — Auth / Landing polish (high-visibility pages)

Files:
- `frontend/src/components/layout/AuthLayout.vue`
- `frontend/src/views/HomeView.vue`
- `frontend/src/views/auth/LoginView.vue`, `RegisterView.vue`

Changes:
1. Background: apply `bg-mesh-gradient` over cream (`bg-primary-50`) in light, warm ink in dark.
2. Logo container: add gold ring/glow — `ring-1 ring-primary-300 shadow-gold-md rounded-2xl`.
3. Site title: apply `.text-gradient` (gold) to the brand name.
4. Sign-in card: `glass-card` + `shadow-gold-md`, subtle gold border `border-primary-200/60`.
5. Primary CTA buttons already `.btn-primary` (gold from Phase 2).

**Verify:** Login page looks premium — gold gradient title, glowing logo, glassy card.

---

## Phase 4 — Admin shell polish

Files:
- `frontend/src/components/layout/AppHeader.vue`
- `frontend/src/components/layout/` sidebar component
- Dashboard cards in `frontend/src/views/admin/DashboardView.vue`

Changes:
1. Header: thin gold bottom-border or `bg-gradient-gold-sheen` accent line under header.
2. Sidebar header (logo area): warm dark in dark-mode, cream in light; gold divider.
3. Dashboard stat cards: use `stat-icon-primary` gold + `shadow-gold-sm` on hover (`card-hover`).
4. Charts: keep multi-color data palettes BUT set the **primary series** color to `#d4a017` and gridlines warm. Files: `DashboardView.vue`, `UserDashboardCharts.vue`, chart components in `frontend/src/components/charts/`.

**Verify:** Admin dashboard — header accent, gold sidebar active, gold primary chart series.

---

## Phase 5 — Dark mode warmth pass

Because we swapped `dark` scale to warm charcoal (Phase 1), audit for hardcoded cold colors:
- Search: `slate-`, `#0f172a`, `#1e293b`, `#020617`, `zinc-9`, `gray-950` in `frontend/src`.
- Replace structural dark backgrounds with `dark-*` tokens (now warm).
- Keep semantic colors (red/green/amber for status) unchanged.

**Verify:** Toggle Dark Mode (sidebar) — everything warm charcoal + gold, no cold blue patches.

---

## Phase 6 — Favicon / logo assets

- `frontend/public/logo.png` already the new logo (done).
- Optional: generate a smaller `favicon` variant if the 1.4MB PNG is heavy — export a 64×64 and 32×32 and reference in `index.html` (optional perf polish).

---

## Phase 7 — Final verification

1. `pnpm --dir frontend exec vue-tsc --noEmit` → 0 errors.
2. `pnpm --dir frontend build` → succeeds.
3. Manual pass (light + dark):
   - Landing `/`, Login, Register
   - Admin: Dashboard, Users, Accounts, Groups, Settings, Subscriptions, Usage, Ops
   - Buttons, inputs (focus ring gold), tables, badges, toasts, modals, tabs, switches, progress bars
4. Confirm no leftover teal: grep `#14b8a6|#0d9488|#2dd4bf|rgba(20, 184, 166` in `frontend/src` → 0 (except intentional chart palettes if any).

---

## Execution notes for the agent (Sonnet)

- Work phase by phase; after each phase run the verify step and fix errors before moving on.
- Prefer editing `tailwind.config.js` + `style.css` (global) over per-component hardcoding — that reskins 90% at once.
- Only touch individual `.vue` files for structural/gradient accents (Phases 3–5).
- Do NOT change semantic status colors (success=emerald, danger=red, warning=amber).
- Do NOT change payment brand button colors (Stripe/Alipay/WeChat/Airwallex) — those must stay their brand colors.
- Keep accessibility: gold text on white needs `primary-700`+ for contrast; gold on dark use `primary-300/400`.
- After all phases: commit and push.
