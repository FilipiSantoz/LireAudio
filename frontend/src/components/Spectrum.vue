<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue'
import { getLatestBands, NUM_BANDS } from './spectrumState'

const MIRROR_COUNT = NUM_BANDS * 2 - 1

const containerRef = ref<HTMLDivElement | null>(null)
const canvasRef = ref<HTMLCanvasElement | null>(null)

let cssWidth = 0
let cssHeight = 0
let frameId = 0
let resizeObserver: ResizeObserver | undefined

function syncCanvasSize(ctx: CanvasRenderingContext2D) {
  const canvas = canvasRef.value
  if (!canvas) return

  const dpr = window.devicePixelRatio || 1
  const pixelWidth = Math.round(cssWidth * dpr)
  const pixelHeight = Math.round(cssHeight * dpr)

  if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
    canvas.width = pixelWidth
    canvas.height = pixelHeight
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  }
}

function drawBars(ctx: CanvasRenderingContext2D, bands: number[], width: number, height: number) {
  const marginX = Math.min(10, width * 0.02)
  const availableWidth = Math.max(0, width - marginX * 1.5)
  const gap = Math.max(1, Math.min(1.4, (availableWidth / MIRROR_COUNT) * 0.16))
  const barWidth = Math.max(
    1,
    (availableWidth - gap * (MIRROR_COUNT - 1)) / MIRROR_COUNT,
  )

  const centerY = height / 2
  // Bar Height
  const maxHeight = height * 0.42

  for (let i = 0; i < MIRROR_COUNT; i++) {
    const bandIndex = i < NUM_BANDS ? i : MIRROR_COUNT - 1 - i
    const raw = Math.max(0, Math.min(1, bands[bandIndex] ?? 0))

    const newHeight = maxHeight * raw
    if (newHeight <= 0.5) continue

    const x = marginX + i * (barWidth + gap)
    const y = centerY - newHeight
    const fullHeight = newHeight * 2

    // Higher Contrast
    ctx.fillStyle = 'rgb(249, 49, 94)'
    ctx.fillRect(x + 4, y + 4, barWidth + 3, fullHeight + 1)

    // Soft Contrast
    ctx.fillStyle = 'rgba(249, 49, 94, 0.40)'
    ctx.fillRect(x + 5, y + 4, barWidth + 4, fullHeight + 2)

    // Shadow
    ctx.fillStyle = 'rgb(15, 8, 20)'
    ctx.fillRect(x + 6, y + 3, barWidth, fullHeight)

    // Principal Bar
    ctx.fillStyle = 'rgb(237, 204, 254)'
    ctx.fillRect(x, y, barWidth, fullHeight)

    // Contrast Line
    ctx.fillStyle = 'rgb(33, 210, 151)'
    ctx.fillRect(x, y, Math.min(2.2, barWidth), fullHeight)
  }
}


function draw() {
  const canvas = canvasRef.value
  const ctx = canvas?.getContext('2d')
  if (!canvas || !ctx || cssWidth <= 0 || cssHeight <= 0) return

  syncCanvasSize(ctx)
  ctx.clearRect(0, 0, cssWidth, cssHeight)

  drawBars(ctx, getLatestBands(), cssWidth, cssHeight)
}

function loop() {

  draw()
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
