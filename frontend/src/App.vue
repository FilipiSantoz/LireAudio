<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, reactive, ref } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import Spectrum from './components/Spectrum.vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import { setLatestBands, NUM_BANDS } from './components/spectrumState'


interface AudioDevice {
  index: number
  name: string
}

interface AudioDeviceList {
  inputs: AudioDevice[]
  outputs: AudioDevice[]
  preferredInputIndex: number | null
  defaultOutputIndex: number | null
  routingStatus: string
}

type AudioStartStatus = 'running' | 'stopped'

interface AudioStartInfo {
  sampleRate: number
  status: AudioStartStatus
}

type VinylParameter = 'amount' | 'crackle' | 'hiss' | 'room' | 'warmth'
type Panel = 'settings' | 'fx' | null

const vinylParameters: { key: VinylParameter; label: string }[] = [
  { key: 'amount', label: 'Amount' },
  { key: 'crackle', label: 'Crackle' },
  { key: 'hiss', label: 'Hiss' },
  { key: 'room', label: 'RoomNoise' },
  { key: 'warmth', label: 'Warmth' },
]

const devices = ref<AudioDeviceList | null>(null)
const selectedInput = ref<number | null>(null)
const selectedOutput = ref<number | null>(null)
const activePanel = ref<Panel>(null)
const audioRunning = ref(false)
const routingStatus = ref('Init Audio…')
const statusMessage = ref('Init Analyzer…')
const statusIsError = ref(false)
const isGlassMode = ref(false)
const SIGNAL_THRESHOLD = 0.02
const SIGNAL_HOLD_MS = 1500
const isPlaying = ref(false)
let lastSignalAt = 0
let playingTimer: number | undefined

const indicatorState = computed(() => {
  if (!audioRunning.value) return 'off'
  return isPlaying.value ? 'playing' : 'silent'
})


const vinyl = reactive({
  enabled: false,
  amount: 0.50,
  crackle: 0.35,
  hiss: 0.25,
  room: 0.25,
  wow: 0.45,
  warmth: 0.50,
  needle: 0.40,
})

function openGithub(event: MouseEvent) {
  event.preventDefault();
  void openUrl("https://github.com/FilipiSantoz");
}

function setStatus(message: string, isError = false) {
  statusMessage.value = message
  statusIsError.value = isError
}

async function refreshDevices() {
  try {
    const previousInputName = devices.value?.inputs.find(
      (device) => device.index === selectedInput.value,
    )?.name
    const previousOutputName = devices.value?.outputs.find(
      (device) => device.index === selectedOutput.value,
    )?.name

    const listed = await invoke<AudioDeviceList>('list_audio_devices')
    devices.value = listed
    routingStatus.value = listed.routingStatus

    selectedInput.value = listed.inputs.find(
      (device) => device.name === previousInputName,
    )?.index ?? listed.preferredInputIndex ?? listed.inputs[0]?.index ?? null

    if (selectedOutput.value !== null && previousOutputName) {
      selectedOutput.value = listed.outputs.find(
        (device) => device.name === previousOutputName,
      )?.index ?? null;
    } else {
      selectedOutput.value = null;
    }

  } catch (error) {
    setStatus(`Fail to list Audio Devices: ${String(error)}`, true)
  }
}

async function applyAudioSettings() {
  setStatus('Init Spectrum')

  const channel = new Channel<number[]>()

  channel.onmessage = (incoming) => {
    const sanitized = Array.from({ length: NUM_BANDS }, (_, index) => {
      const value = incoming[index] ?? 0
      return Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : 0
    })

    setLatestBands(sanitized)

    if (sanitized.some((value) => value > SIGNAL_THRESHOLD)) {
      lastSignalAt = performance.now()
    }
  }


  try {
    const info = await invoke<AudioStartInfo>('start_audio', {
      spectrumChannel: channel,
    })

    audioRunning.value = info.status === 'running'
    statusIsError.value = false
  } catch (error) {
    audioRunning.value = false
    setStatus(`Fail to Init Audio: ${String(error)}`, true)
  }
}

async function togglePanel(panel: Exclude<Panel, null>) {
  activePanel.value = activePanel.value === panel ? null : panel
  if (panel === 'settings' && activePanel.value === 'settings') {
    await refreshDevices()
  }
}

async function setVinylEnabled() {
  try {
    await invoke('set_vinyl_enabled', { enabled: vinyl.enabled })
  } catch (error) {
    setStatus(`Failt to Update Vinyl: ${String(error)}`, true)
  }
}

function setVinylParameter(name: VinylParameter, event: Event) {
  const input = event.target
  if (!(input instanceof HTMLInputElement)) return

  const value = Number(input.value)
  vinyl[name] = value

  void invoke('set_vinyl_parameter', { name, value }).catch((error) => {
    setStatus(`Fail to update ${name}: ${String(error)}`, true)
  })
}

onMounted(async () => {
  void applyAudioSettings()

  playingTimer = window.setInterval(() => {
    isPlaying.value = performance.now() - lastSignalAt < SIGNAL_HOLD_MS
  }, 200)
})

onBeforeUnmount(() => {
  // Also restores the virtual routing when the frontend is reloaded in dev.
  if (playingTimer !== undefined) clearInterval(playingTimer)
  void invoke('stop_audio').catch(() => undefined)
})


</script>

<template>
  <main class="app">
    <header class="header" data-tauri-drag-region>
      <div class="brand">
        <h1><a href="https://github.com/FilipiSantoz" @click="openGithub">LUNA ANALYZER</a></h1>
      </div>
      <nav class="actions" aria-label="Main controls">
        <span class="live-indicator" :class="indicatorState">
          <span class="dot"></span>
        </span>
        <button class="icon" :class="{ selected: isGlassMode }" @click="isGlassMode = !isGlassMode"
          title="Aurora">𜱇</button>
        <button class="icon" :class="{ selected: activePanel === 'fx' }" @click="togglePanel('fx')"></button>
        <button class="icon" :class="{ selected: activePanel === 'settings' }" @click="togglePanel('settings')"
          aria-label="Configurations">⚙</button>
      </nav>
    </header>

    <div class="spectrum-stage">
      <section class="spectrum-panel" aria-label="Audio spectrum">
        <Spectrum />
        <div class="aurora" :class="{ active: isGlassMode }"></div>
        <div v-if="!audioRunning" class="spectrum-empty">Waiting Audio</div>
      </section>
    </div>
    <Teleport to="body">
      <Transition name="fade-slide">
        <section v-if="activePanel === 'settings'" class="floating-panel">
          <div class="panel-heading">
            <div>
              <p class="eyebrow">AUDIO ROUTING</p>
              <h2>Configs</h2>
            </div>
            <button class="close-button" @click="activePanel = null" aria-label="Close Config">×</button>
          </div>

          <label class="field">
            <span>Input Devices</span>
            <select v-model.number="selectedInput">
              <option v-if="!devices?.inputs.length" :value="null" disabled>No Input Devices Available</option>
              <option v-for="device in devices?.inputs ?? []" :key="`${device.index}-${device.name}`"
                :value="device.index">
                {{ device.name }}
              </option>
            </select>
          </label>

          <label class="field">
            <span>Output Devices</span>
            <select v-model="selectedOutput">
              <option :value="null">Analisys Only(No Processing)</option>
              <option v-for="device in devices?.outputs ?? []" :key="`${device.index}-${device.name}`"
                :value="device.index">
                {{ device.name }}
              </option>
            </select>
          </label>

          <div class="panel-actions">
            <button class="select-button" @click="refreshDevices">Update List</button>
            <button class="select-button" @click="applyAudioSettings">Apply Devices</button>
          </div>
        </section>

        <section v-else-if="activePanel === 'fx'" class="floating-panel">
          <div class="panel-heading">
            <div>
              <p class="eyebrow">EFFECTS</p>
              <h2>Vinyl Simulation</h2>
            </div>
            <button class="close-button" @click="activePanel = null" aria-label="Close Effects">×</button>
          </div>

          <label class="toggle-row">
            <span>Vinyl enabled</span>
            <input v-model="vinyl.enabled" type="checkbox" @change="setVinylEnabled" />
          </label>

          <div class="sliders" :class="{ disabled: !vinyl.enabled }">
            <label v-for="parameter in vinylParameters" :key="parameter.key" class="slider-row">
              <span>{{ parameter.label }}</span>
              <input type="range" min="0" max="1" step="0.01" :value="vinyl[parameter.key]" :disabled="!vinyl.enabled"
                @input="setVinylParameter(parameter.key, $event)" />
              <output>{{ Math.round(vinyl[parameter.key] * 100) }}%</output>
            </label>
          </div>
        </section>
      </Transition>
    </Teleport>

  </main>
</template>

<style>
button,
select,
input {
  font: inherit;
}

button {
  cursor: pointer;
}

a {
  text-decoration: none;
  color: #edd6fe;
}

.app {
  position: relative;
  width: 100%;
  min-height: 100%;
  padding: 12px 12px 0 12px;
  background: var(--primary-color);
  overflow: hidden;
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding-bottom: 4px;
  border-bottom: 0.1px solid var(--border-color);
  user-select: none;
  margin: 0;
  z-index: 2;
}

.header:hover {
  cursor: grab;
}

.icon {
  font-family: 'NerdFont';
}

.brand {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
  font-size: 12px;
  font-family: 'NerdFont';
  font-weight: bold;
}

.brand h1 {
  margin: 0;
  font-size: 14px;
  font-weight: 750;
  letter-spacing: 0.16em;
}

.eyebrow {
  margin: 0;
  font-size: 10px;
  letter-spacing: 0.10em;
  opacity: 0.76;
}

.actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.actions button,
.select-button {
  padding: 7px 11px;
  color: #fff;
  background: var(--spectrum-color);
  border: 0.1px solid var(--border-color);
  border-radius: 6px;
}

.actions button:hover,
.actions button.selected,
.select-button:hover {
  border: 0.1px solid rgba(244, 224, 252, 0.75);
  background: rgba(230, 69, 255, 0.30);
}


.live-indicator {
  display: flex;
  align-items: center;
  gap: 6px;
  --dot-color: #778197;
  --dot-glow: rgba(119, 129, 151, 0.3);
}

.live-indicator.playing {
  --dot-color: #50d890;
  --dot-glow: rgba(80, 216, 144, 0.6);
}

.live-indicator.silent {
  --dot-color: #ff5c6c;
  --dot-glow: rgba(255, 92, 108, 0.6);
}



.dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--dot-color);
  box-shadow: 0 0 8px rgba(80, 216, 144, 0.5);
}

.live-indicator.playing .dot,
.live-indicator.silent .dot {
  animation: dot-blink 1.4s ease-in-out infinite;
}

@keyframes dot-blink {

  0%,
  100% {
    opacity: 1;
  }

  50% {
    opacity: 0.25;
  }
}

.status-line.error {
  color: #ff9caa;
}

.route-status {
  opacity: 0.7;
}

.spectrum-panel {
  position: relative;
  width: 100%;
  height: 30vh;
  min-height: 204px;
  overflow: hidden;
  background: var(--spectrum-color);
  z-index: 2;
}

.spectrum-stage {
  position: relative;
  isolation: isolate;
}

.spectrum-stage::before {
  content: "";
  position: absolute;
  inset: -40% -15%;
  z-index: -1;
  pointer-events: none;

  background: radial-gradient(ellipse at 50% 50%,
      rgba(255, 255, 255, 0.12) 0%,
      rgba(255, 255, 255, 0.10) 8%,
      rgba(247, 69, 255, 0.06) 28%,
      rgba(247, 69, 255, 0.02) 40%,
      transparent 80%);
}

.aurora {
  position: absolute;
  inset: 0;
  pointer-events: none;
  background: var(--spectrum-color);
  border-radius: 8px;
  transition: all 0.35s cubic-bezier(0.16, 1, 0.3, 1);
}

.aurora.active {
  z-index: 999;
  backdrop-filter: blur(17px) brightness(1.2);
  -webkit-backdrop-filter: blur(17px) brightness(1.2);
  border-radius: 8px;
  background:
    linear-gradient(in hsl shorter hue to bottom,
      rgba(215, 70, 255, 0.08) 0%,
      rgba(250, 134, 13, 0.05) 20%,
      rgba(215, 70, 255, 0.02) 40%),
    var(--spectrum-color);

}

.spectrum-panel .spectrum {
  height: 100%;
  min-height: 0;
}

.spectrum-empty {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  color: #edd6fe;
  font-size: 12px;
  pointer-events: none;
}

.floating-panel {
  position: fixed;
  top: 55px;
  right: 18px;
  width: min(92vw, 460px);
  max-height: calc(100vh - 75px);
  overflow-y: auto;
  z-index: 1000;
  padding: 18px;
  border-radius: 12px;
  background: var(--spectrum-color);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border: 0.1px solid var(--border-color);
}


.panel-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 18px;
}

.panel-heading h2 {
  margin: 1px 0 0 0;
  font-size: 12px;
  font-weight: 750;
}

.close-button {
  color: #c7cce0;
  border: 0;
  border-radius: 6px;
  background: transparent;
  font-size: 22px;
}

.close-button:hover {
  background: rgba(255, 255, 255, 0.08);
}

.field {
  display: grid;
  gap: 4px;
  margin: 8px 0;
  color: #fff;
  font-size: 12px;
}

.field select {
  width: 100%;
  min-width: 0;
  padding: 4px;
  color: #edd6fe;
  border: 0.1px solid var(--border-color);
  border-radius: 4px;
  background: var(--primary-color);
}

.hint {
  color: #e0b99a;
  font-size: 11px;
  line-height: 1.5;
}

.panel-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 12px;
}

.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 0 14px;
  color: #c7cce0;
  font-size: 13px;
}

.toggle-row input {
  accent-color: #a58ce9;
}

.sliders {
  display: grid;
  gap: 4px;
}

.sliders.disabled {
  opacity: 0.55;
}

.slider-row {
  display: grid;
  grid-template-columns: 82px minmax(80px, 1fr) 42px;
  align-items: center;
  gap: 10px;
  color: #fff;
  font-size: 12px;
}

.slider-row input {
  width: 100%;
  accent-color: rgba(247, 69, 255, 0.8);
}

.slider-row output {
  color: #fff;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

@media (max-width: 560px) {
  .app {
    padding: 12px;
  }

  .header {
    align-items: flex-start;
  }

  .brand {
    display: grid;
    gap: 5px;
  }

  .actions {
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .live-indicator.playing .dot,
  .live-indicator.silent .dot {
    animation: none;
  }

  .spectrum-panel {
    height: 220px;
  }
}
</style>
