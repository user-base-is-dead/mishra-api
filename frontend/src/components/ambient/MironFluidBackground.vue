<template>
  <div
    class="miron-fluid-background"
    :class="[
      isDark ? 'miron-fluid-background-dark' : 'miron-fluid-background-light',
      { 'miron-fluid-background-pending': !isRevealed }
    ]"
    aria-hidden="true"
  >
    <canvas
      ref="canvas"
      class="miron-fluid-canvas"
      :class="{ 'miron-fluid-canvas-hidden': isFallback }"
    ></canvas>
    <div v-if="isFallback" class="miron-fluid-fallback"></div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { AmbientRenderer } from './ambientRenderer'
import type { IFluidPalette } from './fluidSimulation'

const props = withDefaults(defineProps<{
  /** Keep the warmed renderer hidden until the landing loader signals completion. */
  deferReveal?: boolean
}>(), {
  deferReveal: false
})

const FLUID_REVEAL_EVENT = 'miron-fluid-reveal'

const DARK_PALETTE: IFluidPalette = {
  // Exact values from Miron Silk's miron.ambient* theme colors.
  base: { r: 21, g: 13, b: 7 },       // #150D07
  mid: { r: 110, g: 71, b: 19 },      // #6E4713
  high: { r: 142, g: 95, b: 26 }      // #8E5F1A
}

const LIGHT_PALETTE: IFluidPalette = {
  // Miron renderer's light fallback: gold blended over #FFFDF8.
  base: { r: 255, g: 253, b: 248 },   // #FFFDF8
  mid: { r: 244, g: 229, b: 190 },    // 26% gold
  high: { r: 233, g: 205, b: 131 }    // 52% gold
}

const canvas = ref<HTMLCanvasElement | null>(null)
const isDark = ref(document.documentElement.classList.contains('dark'))
const isFallback = ref(false)
const isRevealed = ref(!props.deferReveal)

let renderer: AmbientRenderer | undefined
let themeObserver: MutationObserver | undefined
let motionQuery: MediaQueryList | undefined

function currentPalette(): IFluidPalette {
  return isDark.value ? DARK_PALETTE : LIGHT_PALETTE
}

function rendererOptions() {
  return {
    // Miron IDE's default "balanced" intensity.
    intensity: 0.75,
    followPointer: true,
    // Reduced motion keeps the developed field but freezes it.
    animated: !(motionQuery?.matches ?? false)
  }
}

function syncTheme() {
  isDark.value = document.documentElement.classList.contains('dark')
  renderer?.updatePalette(currentPalette())
}

function syncMotionPreference() {
  renderer?.updateOptions(rendererOptions())
}

function revealFluid() {
  isRevealed.value = true
}

watch(
  () => props.deferReveal,
  (deferReveal) => {
    // Navigating back to the landing page gets a fresh loader gate. Navigating
    // anywhere else reveals the persistent renderer immediately.
    isRevealed.value = !deferReveal
  },
  { immediate: true }
)

onMounted(() => {
  window.addEventListener(FLUID_REVEAL_EVENT, revealFluid)
  if (!canvas.value) return

  motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)')
  syncTheme()

  try {
    // Warm up immediately behind the opaque loader. Only visibility is gated,
    // so the first revealed frame is already developed and moving.
    renderer = new AmbientRenderer(canvas.value, window, currentPalette(), rendererOptions())
    isFallback.value = !renderer.isSupported
  } catch (error) {
    // A failed/blocked WebGL context should never prevent the site from loading.
    console.warn('Miron fluid background is unavailable; using the static fallback.', error)
    isFallback.value = true
  }

  themeObserver = new MutationObserver(syncTheme)
  themeObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class']
  })

  motionQuery.addEventListener('change', syncMotionPreference)
})

onBeforeUnmount(() => {
  window.removeEventListener(FLUID_REVEAL_EVENT, revealFluid)
  motionQuery?.removeEventListener('change', syncMotionPreference)
  themeObserver?.disconnect()
  renderer?.dispose()
  renderer = undefined
})
</script>

<style scoped>
.miron-fluid-background {
  position: fixed;
  inset: 0;
  z-index: 0;
  overflow: hidden;
  pointer-events: none;
  contain: strict;
}

/* Do not use display:none: the canvas must keep its final viewport dimensions
 * and continue warming while the opaque loader is on top. */
.miron-fluid-background-pending {
  visibility: hidden;
  opacity: 0;
}

.miron-fluid-background-dark {
  background: #150d07;
}

.miron-fluid-background-light {
  background: #fffdf8;
}

.miron-fluid-canvas,
.miron-fluid-fallback {
  position: absolute;
  inset: 0;
  display: block;
  width: 100%;
  height: 100%;
}

.miron-fluid-canvas-hidden {
  visibility: hidden;
}

/* Only used when float render targets/WebGL are unavailable. */
.miron-fluid-background-dark .miron-fluid-fallback {
  background:
    radial-gradient(ellipse at 18% 28%, rgba(142, 95, 26, 0.58), transparent 42%),
    radial-gradient(ellipse at 78% 72%, rgba(110, 71, 19, 0.52), transparent 46%),
    #150d07;
}

.miron-fluid-background-light .miron-fluid-fallback {
  background:
    radial-gradient(ellipse at 18% 28%, rgba(233, 205, 131, 0.62), transparent 42%),
    radial-gradient(ellipse at 78% 72%, rgba(244, 229, 190, 0.72), transparent 46%),
    #fffdf8;
}
</style>