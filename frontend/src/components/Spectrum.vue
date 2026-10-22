<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue'
import { getLatestBands } from './spectrumState'

const NUM_BANDS = 28
const MIRROR_COUNT = NUM_BANDS * 2 - 1

// Control the little values
// < NOISE_FLOOR becames 0
// Otherwise is smoothed
const NOISE_FLOOR = 0.01
const SMALL_ATTACK_TAU = 0.012
const RELEASE_TAU = 0.035
// Determines if the bar will receive the full value or the smoothing 
const STRONG_RISE = 0.06

// +GAIN = +Bars go UP
// GAMMA is the GAIN expoent.
const GAIN = 1.10
const GAMMA = 0.80

const containerRef = ref<HTMLDivElement | null>(null)
const canvasRef = ref<HTMLCanvasElement | null>(null)

let cssWidth = 0
let cssHeight = 0
let frameId = 0
let lastFrameTime = 0
let resizeObserver: ResizeObserver | undefined

const shown = new Float32Array(MIRROR_COUNT)

function updateEnvelope(bands: number[], dt: number) {
  const attackK = 1 - Math.exp(-dt / SMALL_ATTACK_TAU)
  const releaseK = 1 - Math.exp(-dt / RELEASE_TAU)

  for (let i = 0; i < MIRROR_COUNT; i++) {
    const sourceIndex =
      i < NUM_BANDS ? i : MIRROR_COUNT - 1 - i

    const raw = bands[sourceIndex] ?? 0
    const value = Number.isFinite(raw)
      ? Math.max(0, Math.min(1, raw))
      : 0

    const target =
      value <= NOISE_FLOOR
        ? 0
        : (value - NOISE_FLOOR) / (1 - NOISE_FLOOR)

    const delta = target - shown[i]

    if (delta >= STRONG_RISE) {
      shown[i] = target
    } else if (delta > 0) {
      shown[i] += delta * attackK
    } else {
      shown[i] += delta * releaseK
    }
  }
}


function syncCanvasSize() {
  const canvas = canvasRef.value
  if (!canvas) return

  const dpr = window.devicePixelRatio || 1
  const pixelWidth = Math.round(cssWidth * dpr)
  const pixelHeight = Math.round(cssHeight * dpr)

  if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
    canvas.width = pixelWidth
    canvas.height = pixelHeight
  }
}

function drawBars(ctx: CanvasRenderingContext2D, width: number, height: number) {
  const marginX = Math.min(20, width * 0.04)
  const availableWidth = Math.max(0, width - marginX * 2)
  const gap = Math.max(1, Math.min(5.4, (availableWidth / MIRROR_COUNT) * 0.21))
  const barWidth = Math.max(
    1,
    (availableWidth - gap * (MIRROR_COUNT - 1)) / MIRROR_COUNT,
  )

  const centerY = height / 2
  // Bar Height
  const maxHeight = height * 0.46


  for (let i = 0; i < MIRROR_COUNT; i++) {
    shown[i] = Math.max(0, Math.min(1, shown[i]))
    const raw = Math.min(1, shown[i] * GAIN) ** GAMMA

    const newHeight = maxHeight * raw
    if (newHeight <= 0.5) continue

    const x = marginX + i * (barWidth + gap)
    const y = centerY - newHeight
    const fullHeight = newHeight * 2

    // Higher Contrast
    ctx.fillStyle = 'rgb(247, 69, 255)'
    ctx.fillRect(x + 4, y + 4, barWidth + 3, fullHeight + 1)

    // Soft Contrast
    // ctx.fillStyle = 'rgba(247, 69, 255, 0.40)'
    // ctx.fillRect(x + 5, y + 4, barWidth + 4, fullHeight + 2)

    // Shadow
    ctx.fillStyle = 'rgb(15, 8, 20)'
    ctx.fillRect(x + 6, y + 3, barWidth, fullHeight)

    // Principal Bar
    ctx.fillStyle = 'rgb(237, 204, 254)'
    ctx.fillRect(x, y, barWidth, fullHeight)

    // Contrast Line
    ctx.fillStyle = 'rgb(110, 255, 69)'
    ctx.fillRect(x, y, Math.min(1.6, barWidth), fullHeight)
  }
}


function draw(dt: number) {
  const canvas = canvasRef.value
  const ctx = canvas?.getContext('2d')
  if (!canvas || !ctx || cssWidth <= 0 || cssHeight <= 0) return

  syncCanvasSize()

  const dpr = window.devicePixelRatio || 1
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, cssWidth, cssHeight)

  updateEnvelope(getLatestBands(), dt)

  drawBars(ctx, cssWidth, cssHeight)
}

function loop(now: number) {
  const dt =
    lastFrameTime === 0
      ? 1 / 60
      : Math.min((now - lastFrameTime) / 1000, 0.048)

  lastFrameTime = now

  draw(dt)
  frameId = requestAnimationFrame(loop)
}

onMounted(() => {
  const container = containerRef.value
  if (container) {

    resizeObserver = new ResizeObserver(([entry]) => {
      cssWidth = entry.contentRect.width
      cssHeight = entry.contentRect.height
    })
    resizeObserver.observe(container)
  }

  frameId = requestAnimationFrame(loop)
})

onBeforeUnmount(() => {
  cancelAnimationFrame(frameId)
  resizeObserver?.disconnect()
})
</script>

<template>
  <div ref="containerRef" class="spectrum">
    <canvas ref="canvasRef" aria-label="Espectro de áudio espelhado" />
  </div>
</template>

<style scoped>
.spectrum {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 180px;
  overflow: hidden;
}

canvas {
  display: block;
  width: 100%;
  height: 100%;
}
</style>
