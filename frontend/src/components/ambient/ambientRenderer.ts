/*---------------------------------------------------------------------------------------------
 *  Adapted from Miron Code's ambient background renderer.
 *  Licensed under the MIT License.
 *--------------------------------------------------------------------------------------------*/

import { FluidSimulation, type IFluidPalette } from './fluidSimulation'

export interface IAmbientOptions {
  /** 0 flattens the field to the base tone; 1 is full strength. */
  intensity: number
  /** Whether pointer movement disturbs the smoke. */
  followPointer: boolean
  /** When false, build the smoke once and then hold it still. */
  animated: boolean
}

const TARGET_FPS = 60
const FRAME_INTERVAL_MS = 1000 / TARGET_FPS

const POINTER_FORCE = 5200
const MAX_POINTER_FORCE = 230
const POINTER_DENSITY_PER_SECOND = 2.1

const EMITTER_COUNT = 5
const EMITTER_DENSITY_PER_SECOND = 0.57
const EMITTER_FORCE = 26
const EMITTER_SPEED = 0.085
const EMITTER_TURN_RATE = 0.55

const SEED_SPLATS = 8
const SEED_DENSITY = 0.85
const SEED_FORCE = 70
const WARMUP_STEPS = 100

interface IEmitter {
  x: number
  y: number
  angle: number
}

/**
 * Owns the full-viewport canvas and drives the exact stable-fluid simulation
 * used by Miron IDE. The solver itself lives in fluidSimulation.ts.
 */
export class AmbientRenderer {
  private readonly simulation: FluidSimulation | undefined
  private readonly cleanups: Array<() => void> = []

  private palette: IFluidPalette
  private options: IAmbientOptions

  /** Pointer position in texture space, with the origin at the bottom left. */
  private pointer: { x: number; y: number } | undefined
  private lastPointer: { x: number; y: number } | undefined
  private readonly emitters: IEmitter[] = []

  private animationFrame: number | undefined
  private lastFrameAt = 0
  private hidden: boolean
  private disposed = false

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly targetWindow: Window & typeof globalThis,
    palette: IFluidPalette,
    options: IAmbientOptions
  ) {
    this.palette = palette
    this.options = options
    this.hidden = targetWindow.document.visibilityState === 'hidden'

    this.canvas.width = this.backingWidth()
    this.canvas.height = this.backingHeight()

    this.simulation = FluidSimulation.create(this.canvas, palette, {
      intensity: options.intensity,
      followPointer: options.followPointer
    })

    if (!this.simulation) return

    this.warmUp()
    this.registerListeners()
    this.start()
  }

  get isSupported(): boolean {
    return Boolean(this.simulation)
  }

  /*
   * Use the visual viewport rather than canvas.clientWidth. The landing loader
   * temporarily locks document overflow; clientWidth changes by the scrollbar
   * width when that lock is removed even though the physical viewport did not
   * resize. Keeping the backing store on innerWidth/innerHeight prevents that
   * cosmetic layout change from rebuilding every framebuffer and restarting the
   * smoke immediately after reveal.
   */
  private backingWidth(): number {
    return Math.max(1, Math.round(this.targetWindow.innerWidth))
  }

  private backingHeight(): number {
    return Math.max(1, Math.round(this.targetWindow.innerHeight))
  }

  /** Seed and develop the field before its first visible frame. */
  private warmUp(): void {
    for (let index = 0; index < SEED_SPLATS; index++) {
      const angle = Math.random() * Math.PI * 2
      this.simulation?.splat(
        Math.random(),
        Math.random(),
        Math.cos(angle) * SEED_FORCE,
        Math.sin(angle) * SEED_FORCE,
        SEED_DENSITY
      )
    }

    for (let index = 0; index < EMITTER_COUNT; index++) {
      this.emitters.push({
        x: Math.random(),
        y: Math.random(),
        angle: Math.random() * Math.PI * 2
      })
    }

    for (let index = 0; index < WARMUP_STEPS; index++) {
      this.simulation?.step(1 / TARGET_FPS)
    }
  }

  private registerListeners(): void {
    const onPointerMove = (event: PointerEvent) => {
      const rect = this.canvas.getBoundingClientRect()
      if (rect.width === 0 || rect.height === 0) return

      this.pointer = {
        x: (event.clientX - rect.left) / rect.width,
        y: 1 - (event.clientY - rect.top) / rect.height
      }
    }

    const onPointerLeave = () => {
      this.pointer = undefined
      this.lastPointer = undefined
    }

    // The fixed canvas sits behind the application and deliberately has
    // pointer-events:none, so listen at window level instead of on the canvas.
    this.targetWindow.addEventListener('pointermove', onPointerMove, { passive: true })
    this.targetWindow.addEventListener('pointerleave', onPointerLeave, { passive: true })
    this.cleanups.push(() => {
      this.targetWindow.removeEventListener('pointermove', onPointerMove)
      this.targetWindow.removeEventListener('pointerleave', onPointerLeave)
    })

    const onVisibilityChange = () => {
      this.setHidden(this.targetWindow.document.visibilityState === 'hidden')
    }
    this.targetWindow.document.addEventListener('visibilitychange', onVisibilityChange)
    this.cleanups.push(() => {
      this.targetWindow.document.removeEventListener('visibilitychange', onVisibilityChange)
    })

    const resize = () => {
      this.simulation?.resize(this.backingWidth(), this.backingHeight())
      if (this.animationFrame === undefined) this.simulation?.render()
    }

    if (typeof ResizeObserver !== 'undefined') {
      const observer = new ResizeObserver(resize)
      observer.observe(this.canvas)
      this.cleanups.push(() => observer.disconnect())
    } else {
      this.targetWindow.addEventListener('resize', resize, { passive: true })
      this.cleanups.push(() => this.targetWindow.removeEventListener('resize', resize))
    }
  }

  /** Convert pointer movement since the last frame into a clamped fluid push. */
  private applyPointer(): void {
    if (!this.options.followPointer || !this.pointer) {
      this.lastPointer = undefined
      return
    }

    const previous = this.lastPointer
    this.lastPointer = { ...this.pointer }
    if (!previous) return

    const dx = this.pointer.x - previous.x
    const dy = this.pointer.y - previous.y
    if (Math.hypot(dx, dy) < 0.0008) return

    let forceX = dx * POINTER_FORCE
    let forceY = dy * POINTER_FORCE
    const magnitude = Math.hypot(forceX, forceY)

    if (magnitude > MAX_POINTER_FORCE) {
      const scale = MAX_POINTER_FORCE / magnitude
      forceX *= scale
      forceY *= scale
    }

    this.simulation?.splat(
      this.pointer.x,
      this.pointer.y,
      forceX,
      forceY,
      POINTER_DENSITY_PER_SECOND / TARGET_FPS
    )
  }

  /** Advance the five drifting sources and let each lay down a smoke ribbon. */
  private applyAmbient(): void {
    const step = 1 / TARGET_FPS

    for (const emitter of this.emitters) {
      emitter.angle += (Math.random() - 0.5) * 2 * EMITTER_TURN_RATE * step

      const dx = Math.cos(emitter.angle)
      const dy = Math.sin(emitter.angle)
      emitter.x += dx * EMITTER_SPEED * step
      emitter.y += dy * EMITTER_SPEED * step

      if (emitter.x < 0.05 || emitter.x > 0.95) {
        emitter.angle = Math.PI - emitter.angle
        emitter.x = Math.min(0.95, Math.max(0.05, emitter.x))
      }
      if (emitter.y < 0.05 || emitter.y > 0.95) {
        emitter.angle = -emitter.angle
        emitter.y = Math.min(0.95, Math.max(0.05, emitter.y))
      }

      this.simulation?.splat(
        emitter.x,
        emitter.y,
        dx * EMITTER_FORCE,
        dy * EMITTER_FORCE,
        EMITTER_DENSITY_PER_SECOND / TARGET_FPS
      )
    }
  }

  private setHidden(hidden: boolean): void {
    if (hidden === this.hidden) return

    this.hidden = hidden
    if (hidden) this.stop()
    else this.start()
  }

  private start(): void {
    if (this.animationFrame !== undefined || this.hidden || !this.simulation || this.disposed) return

    if (!this.options.animated) {
      this.simulation.render()
      return
    }

    const animate = () => {
      if (this.disposed || this.hidden) return
      this.animationFrame = this.targetWindow.requestAnimationFrame(animate)

      const now = Date.now()
      if (now - this.lastFrameAt < FRAME_INTERVAL_MS) return
      this.lastFrameAt = now

      this.applyPointer()
      this.applyAmbient()
      this.simulation?.step(1 / TARGET_FPS)
      this.simulation?.render()
    }

    this.animationFrame = this.targetWindow.requestAnimationFrame(animate)
  }

  private stop(): void {
    if (this.animationFrame !== undefined) {
      this.targetWindow.cancelAnimationFrame(this.animationFrame)
      this.animationFrame = undefined
    }
  }

  updatePalette(palette: IFluidPalette): void {
    this.palette = palette
    this.simulation?.updatePalette(this.palette)
    if (this.animationFrame === undefined) this.simulation?.render()
  }

  updateOptions(options: IAmbientOptions): void {
    const wasAnimated = this.options.animated
    this.options = options
    this.simulation?.updateOptions({
      intensity: options.intensity,
      followPointer: options.followPointer
    })

    if (options.animated !== wasAnimated) {
      this.stop()
      this.start()
    } else if (this.animationFrame === undefined) {
      this.simulation?.render()
    }
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true

    this.stop()
    for (const cleanup of this.cleanups.splice(0)) cleanup()
    this.simulation?.dispose()
  }
}
