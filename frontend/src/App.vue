<script setup lang="ts">
import { onMounted, onBeforeUnmount, reactive, ref } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import Spectrum from './components/Spectrum.vue'

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

interface AudioStartInfo {
  sampleRate: number
  inputName: string
  outputName: string | null
}

type VinylParameter = 'amount' | 'crackle' | 'hiss' | 'texture' | 'wow' | 'warmth' | 'needle'
type Panel = 'settings' | 'fx' | null

const vinylParameters: { key: VinylParameter; label: string }[] = [
  { key: 'amount', label: 'Amount' },
  { key: 'crackle', label: 'Crackle' },
  { key: 'hiss', label: 'Hiss' },
  { key: 'texture', label: 'Texture' },
  { key: 'wow', label: 'Wow' },
  { key: 'warmth', label: 'Warmth' },
  { key: 'needle', label: 'Needle' },
]

const bands = ref<number[]>(Array(26).fill(0))
const devices = ref<AudioDeviceList | null>(null)
const selectedInput = ref<number | null>(null)
const selectedOutput = ref<number | null>(null)
const activePanel = ref<Panel>(null)
const audioRunning = ref(false)
const routingStatus = ref('Inicializando roteamento de áudio…')
const statusMessage = ref('Inicializando Analyzer…')
const statusIsError = ref(false)

const vinyl = reactive({
  enabled: false,
  amount: 0.50,
  crackle: 0.35,
  hiss: 0.25,
  texture: 0.25,
  wow: 0.45,
  warmth: 0.50,
  needle: 0.40,
})

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
  if (selectedInput.value === null) {
    setStatus('No Input Device Available', true)
    return
  }

  setStatus('Init Spectrum')


  const channel = new Channel<number[]>()

  channel.onmessage = (incoming) => {
    bands.value = Array.from({ length: 26 }, (_, index) => {
      const value = incoming[index] ?? 0
      return Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : 0
    })
  }

  try {
    const info = await invoke<AudioStartInfo>('start_audio', {
      inputIndex: selectedInput.value,
      outputIndex: selectedOutput.value,
      spectrumChannel: channel,
    })

    audioRunning.value = true
    statusIsError.value = false
    const output = info.outputName ?? 'Analisys Only'
    setStatus(`${info.inputName} → ${output} · ${info.sampleRate} Hz`)
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
    setStatus(`Falha ao atualizar ${name}: ${String(error)}`, true)
  })
}

onMounted(async () => {
  await refreshDevices()
  if (devices.value) await applyAudioSettings()
})

onBeforeUnmount(() => {
  // Also restores the virtual routing when the frontend is reloaded in dev.
  void invoke('stop_audio').catch(() => undefined)
})


</script>

<template>
  <main class="app">
    <header class="header" data-tauri-drag-region>
      <div class="brand">
        <h1>LUNA ANALYZER</h1>
      </div>
      <nav class="actions" aria-label="Main controls">
        <span class="live-indicator" :class="{ active: audioRunning }">
          <span class="dot" />{{ audioRunning ? '' : '' }}
        </span>
        <button class="icon" :class="{ selected: activePanel === 'fx' }" @click="togglePanel('fx')"></button>
        <button class="icon" :class="{ selected: activePanel === 'settings' }" @click="togglePanel('settings')"
          aria-label="Configurations">⚙</button>
      </nav>
    </header>

    <section class="spectrum-panel" aria-label="Audio spectrum">
      <Spectrum :bands="bands" />
      <div v-if="!audioRunning" class="spectrum-empty">Waiting Audio</div>
    </section>

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

.app {
  position: relative;
  width: 100%;
  min-height: 100%;
  padding: 18px;
  background: var(--primary-color);
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  min-height: 30px;
  padding-bottom: 10px;
  border-bottom: 0.1px solid var(--border-color);
  user-select: none;
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
  gap: 16px;
  min-width: 0;
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
  letter-spacing: 0.15em;
  opacity: 0.66;
}

.actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.actions button,
.select-button {
  padding: 7px 11px;
  color: #edd6fe;
  background: var(--primary-color);
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
  margin-right: 8px;
  color: #a8afc5;
  font-size: 10px;
  letter-spacing: 0.1em;
  white-space: nowrap;
}

.live-indicator.active {
  color: #a8f0c6;
}

.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #778197;
}

.live-indicator.active .dot {
  background: #50d890;
  box-shadow: 0 0 8px rgba(80, 216, 144, 0.5);
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
  height: 40vh;
  min-height: 210px;
  overflow: hidden;
  border-radius: 4px;
  background: var(--spectrum-color);
}

.spectrum-panel>* {
  position: relative;
  z-index: 1;
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
  top: 60px;
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
  margin: 5px 0 0;
  font-size: 16px;
  font-weight: 650;
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
  gap: 7px;
  margin: 14px 0;
  color: #c7cce0;
  font-size: 12px;
}

.field select {
  width: 100%;
  min-width: 0;
  padding: 9px;
  color: #edd6fe;
  border: 1px solid #3a4661;
  border-radius: 6px;
  background: #10182b;
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
  gap: 13px;
}

.sliders.disabled {
  opacity: 0.45;
}

.slider-row {
  display: grid;
  grid-template-columns: 82px minmax(80px, 1fr) 42px;
  align-items: center;
  gap: 10px;
  color: #c7cce0;
  font-size: 12px;
}

.slider-row input {
  width: 100%;
  accent-color: #a58ce9;
}

.slider-row output {
  color: #a8afc5;
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

  .live-indicator {
    display: none;
  }

  .spectrum-panel {
    height: 220px;
  }
}
</style>
