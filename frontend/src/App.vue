<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue'
import Spectrum from './components/Spectrum.vue'

const bands = ref<number[]>(
  Array.from({ length: 26 }, (_, i) =>
    Math.max(0.05, 0.25 * Math.exp(-i / 9)),
  ),
)

let timer: number | undefined

onMounted(() => {
  timer = window.setInterval(() => {
    bands.value = Array.from({ length: 26 }, (_, i) => {
      const bassShape = 0.35 * Math.exp(-i / 8)
      const peaks = 0.35 * Math.abs(
        Math.sin(Date.now() / 240 + i * 0.75),
      )
      const variation = Math.random() * 0.12

      return Math.min(1, bassShape + peaks + variation)
    })
  }, 20)
})

onBeforeUnmount(() => {
  if (timer !== undefined) {
    window.clearInterval(timer)
  }
})
</script>

<template>
  <main class="app">
    <header class="header">
      <h1>LUNA ANALYZER</h1>
      <span> SPECTRUM </span>
    </header>

    <section class="spectrum-panel">
      <Spectrum :bands="bands" />
    </section>
  </main>
</template>

<style>
html,
body,
#app {
  width: 100%;
  height: 100%;
  margin: 0;
}

* {
  box-sizing: border-box;
}

body {
  font-family: Inter, sans-serif;
  background: #10182b;
  color: #edd6fe;
}

.app {
  min-height: 100%;
  padding: 24px;
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.header h1 {
  font-size: 16px;
  letter-spacing: 0.18em;
}

.header span {
  font-size: 11px;
  letter-spacing: 0.14em;
  opacity: 0.7;
}

.spectrum-panel {
  width: 100%;
  height: 240px;
  background: rgba(26, 36, 120, 0.18);
  border-radius: 8px;
  overflow: hidden;
}
</style>
