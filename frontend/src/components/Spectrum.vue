<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue'
import { getLatestBands } from './spectrumState'

const NUM_BANDS = 28
const MIRROR_COUNT = NUM_BANDS * 2 - 1

// +GAIN = +Bars go UP
// GAMMA is the GAIN expoent.
const GAIN = 1.12
const GAMMA = 0.90

const containerRef = ref<HTMLDivElement | null>(null)
const canvasRef = ref<HTMLCanvasElement | null>(null)

let cssWidth = 0
let cssHeight = 0
let frameId = 0
let resizeObserver: ResizeObserver | undefined

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

function drawBars(ctx: CanvasRenderingContext2D, bands: number[], width: number, height: number) {

  const marginX = Math.min(20, width * 0.04)
  const availableWidth = Math.max(0, width - marginX * 2)
  const gap = Math.max(1, Math.min(5.4, (availableWidth / MIRROR_COUNT) * 0.21))
  const barWidth = Math.max(
    1,
    (availableWidth - gap * (MIRROR_COUNT - 1)) / MIRROR_COUNT,
  )

  const centerY = height / 2
  // Bar Height
  const maxHeight = height * 0.45


  for (let i = 0; i < MIRROR_COUNT; i++) {
    const sourceIndex = i < NUM_BANDS ? i : MIRROR_COUNT - 1 - i
    const raw = bands[sourceIndex] ?? 0
    const safeValue = Number.isFinite(raw) ? Math.max(0, Math.min(1, raw)) : 0
    const normalized = Math.min(1, safeValue * GAIN) ** GAMMA

    const newHeight = maxHeight * normalized
    if (newHeight <= 0.5) continue

    const x = marginX + i * (barWidth + gap)
    const y = centerY - newHeight
    const fullHeight = newHeight * 2

    // Contraste externo
    ctx.fillStyle = 'rgb(247, 69, 255)'
    ctx.fillRect(x + 4, y + 4, barWidth + 3, fullHeight + 1)

    // Contraste suave
    ctx.fillStyle = 'rgba(247, 69, 255, 0.40)'
    ctx.fillRect(x + 5, y + 4, barWidth + 4, fullHeight + 2)

    // Sombra
    ctx.fillStyle = 'rgb(15, 8, 20)'
    ctx.fillRect(x + 6, y + 3, barWidth, fullHeight)

    // Barra principal
    ctx.fillStyle = 'rgb(237, 204, 254)'
    ctx.fillRect(x, y, barWidth, fullHeight)

    // Linha de contraste
    ctx.fillStyle = 'rgb(110, 255, 69)'
    ctx.fillRect(x, y, Math.min(1.6, barWidth), fullHeight)
  }
}


function draw() {
  const canvas = canvasRef.value
  const ctx = canvas?.getContext('2d')
  if (!canvas || !ctx || cssWidth <= 0 || cssHeight <= 0) return

  syncCanvasSize()

  const dpr = window.devicePixelRatio || 1
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
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
