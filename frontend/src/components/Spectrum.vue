<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue'

const props = defineProps<{
  bands: number[]
}>()

const BAND_COUNT = 26
const MIRROR_COUNT = BAND_COUNT * 2 - 1

//+RELEASE = +Faster Bars Fall 
const RELEASE = 24
// +ONSET_BOOST = +Energy 
const ONSET_BOOST = 0.84

// +GAIN = +Bars go UP
// GAMMA is the GAIN expoent.
const GAIN = 1.00
const GAMMA = 1.02

const containerRef = ref<HTMLDivElement | null>(null)
const canvasRef = ref<HTMLCanvasElement | null>(null)

let cssWidth = 0
let cssHeight = 0
let frameId = 0
let lastTime = 0
let resizeObserver: ResizeObserver | undefined

const shown = new Float32Array(MIRROR_COUNT)
const prevTarget = new Float32Array(MIRROR_COUNT)

function targetAt(i: number): number {
  const n = BAND_COUNT
  const src = i < n ? i : 2 * n - 2 - i
  const raw = props.bands[src]
  return Number.isFinite(raw) ? Math.max(0, Math.min(1, raw)) : 0
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

function updateEnvelope(dt: number) {
  const releaseK = 1 - Math.exp(-RELEASE * dt)

  for (let i = 0; i < MIRROR_COUNT; i++) {
    const target = targetAt(i)
    const onset = Math.max(0, target - prevTarget[i])
    prevTarget[i] = target

    const boosted = Math.min(1, target + onset * ONSET_BOOST)

    if (boosted > shown[i]) {
      shown[i] = boosted
    } else {
      shown[i] += (boosted - shown[i]) * releaseK
    }
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

  updateEnvelope(dt)

  const marginX = Math.min(20, cssWidth * 0.04)
  const availableWidth = Math.max(0, cssWidth - marginX * 2)
  const gap = Math.max(1, Math.min(5, (availableWidth / MIRROR_COUNT) * 0.22))
  const barWidth = Math.max(
    1,
    (availableWidth - gap * (MIRROR_COUNT - 1)) / MIRROR_COUNT,
  )

  const centerY = cssHeight / 2
  const maxHeight = cssHeight * 0.4

  for (let i = 0; i < MIRROR_COUNT; i++) {
    const normalized = Math.min(1, shown[i] * GAIN) ** GAMMA
    const height = maxHeight * normalized
    if (height <= 0.5) continue

    const x = marginX + i * (barWidth + gap)
    const y = centerY - height
    const fullHeight = height * 2

    // Constrast Bar
    ctx.fillStyle = 'rgb(247, 69, 255)'
    ctx.fillRect(x + 4, y + 4, barWidth + 3, fullHeight + 1)

    //Soft Contrast
    ctx.fillStyle = 'rgba(247, 69, 255, 0.40)'
    ctx.fillRect(x + 5, y + 4, barWidth + 4, fullHeight + 2)

    //Solid Shadow
    ctx.fillStyle = 'rgba(15, 8, 20, 0.70)'
    ctx.fillRect(x + 6, y + 3, barWidth, fullHeight)

    // Barra principal
    ctx.fillStyle = 'rgb(237, 204, 254)'
    ctx.fillRect(x, y, barWidth, fullHeight)

    // Contrast Line
    ctx.fillStyle = 'rgb(110, 255, 69)'
    ctx.fillRect(x, y, Math.min(1.4, barWidth), fullHeight)
  }
}

function loop(time: number) {
  const dt = lastTime ? Math.min(0.05, (time - lastTime) / 1000) : 1 / 60
  lastTime = time

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
