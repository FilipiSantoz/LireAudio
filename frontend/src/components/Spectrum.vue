<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from 'vue'

const props = defineProps<{
  bands: number[]
}>()

const canvasRef = ref<HTMLCanvasElement | null>(null)

let resizeObserver: ResizeObserver | undefined

function mirroredBands(bands: readonly number[]): number[] {
  if (bands.length === 0) return []

  return [
    ...bands,
    ...bands.slice(0, -1).reverse(),
  ]
}

function drawSpectrum() {
  const canvas = canvasRef.value
  const ctx = canvas?.getContext('2d')

  if (!canvas || !ctx) return

  const bounds = canvas.getBoundingClientRect()
  if (bounds.width <= 0 || bounds.height <= 0) return

  const dpr = window.devicePixelRatio || 1
  const pixelWidth = Math.max(1, Math.round(bounds.width * dpr))
  const pixelHeight = Math.max(1, Math.round(bounds.height * dpr))

  if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
    canvas.width = pixelWidth
    canvas.height = pixelHeight
  }

  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, bounds.width, bounds.height)

  const values = mirroredBands(props.bands)
  if (values.length === 0) return

  const marginX = Math.min(20, bounds.width * 0.04)
  const availableWidth = Math.max(0, bounds.width - marginX * 2)
  const count = values.length

  const gap = Math.max(
    1,
    Math.min(5, (availableWidth / count) * 0.50),
  )

  const totalGap = gap * Math.max(0, count - 1)
  const barWidth = Math.max(1, (availableWidth - totalGap) / count)

  const centerY = bounds.height / 2
  const maxHeight = bounds.height * 0.38

  const gamma = 0.80
  const gain = 1.16

  for (let i = 0; i < count; i++) {
    const value = Number.isFinite(values[i])
      ? Math.max(0, Math.min(1, values[i]))
      : 0

    const normalized = Math.min(1, value * gain) ** gamma
    const height = maxHeight * normalized

    if (height <= 0.5) continue

    const x = marginX + i * (barWidth + gap)
    const y = centerY - height
    const fullHeight = height * 2

    // Bar Colors
    // Contrast Color.
    ctx.fillStyle = 'rgba(200, 32, 250, 0.99)'
    ctx.fillRect(x + 4, y + 4, barWidth + 1.24, fullHeight + 1.24)

    // Shadow
    // ctx.shadowColor = 'rgba(5, 12, 20, 0.9)'
    // ctx.shadowBlur = 4
    // ctx.shadowOffsetX = 6
    // ctx.shadowOffsetY = 3

    // Principal Bar.
    ctx.fillStyle = 'rgb(237, 214, 254)'
    ctx.fillRect(x, y, barWidth, fullHeight)

    ctx.shadowBlur = 0
    ctx.shadowColor = 'transparent'
    // Contrast Line.
    ctx.fillStyle = 'rgba(255, 244, 69, 0.98)'
    ctx.fillRect(x, y, Math.min(1.0, barWidth), fullHeight)
  }
}

watch(
  () => props.bands,
  drawSpectrum,
  { deep: true, flush: 'post' },
)

onMounted(() => {
  drawSpectrum()

  if (canvasRef.value) {
    resizeObserver = new ResizeObserver(drawSpectrum)
    resizeObserver.observe(canvasRef.value)
  }
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
})
</script>

<template>
  <div class="spectrum">
    <canvas ref="canvasRef" aria-label="Espectro de áudio espelhado" />
  </div>
</template>

<style scoped>
.spectrum {
  width: 100%;
  height: 100%;
  min-height: 220px;
  overflow: hidden;
}

canvas {
  display: block;
  width: 100%;
  height: 100%;
}
</style>
