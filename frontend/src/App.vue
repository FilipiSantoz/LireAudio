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

let spectrumChannel: Channel<number[]> | null = null

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

    selectedOutput.value = listed.outputs.find(
      (device) => device.name === previousOutputName,
    )?.index ?? listed.defaultOutputIndex ?? listed.outputs[0]?.index ?? null
  } catch (error) {
    setStatus(`Falha ao listar dispositivos: ${String(error)}`, true)
  }
}

async function applyAudioSettings() {
  if (selectedInput.value === null) {
    setStatus('Nenhum dispositivo de entrada disponível.', true)
    return
  }
  if (!spectrumChannel) {
    setStatus('O canal do espectro ainda não foi inicializado.', true)
    return
  }

  setStatus('Iniciando processamento de áudio…')
  try {
    const info = await invoke<AudioStartInfo>('start_audio', {
      inputIndex: selectedInput.value,
      outputIndex: selectedOutput.value,
      spectrumChannel,
    })

    audioRunning.value = true
    statusIsError.value = false
    const output = info.outputName ?? 'somente análise (sem monitoramento)'
    setStatus(`${info.inputName} → ${output} · ${info.sampleRate} Hz`)
  } catch (error) {
    audioRunning.value = false
    setStatus(`Falha ao iniciar áudio: ${String(error)}`, true)
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
    setStatus(`Falha ao atualizar Vinyl: ${String(error)}`, true)
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
  spectrumChannel = new Channel<number[]>()
  spectrumChannel.onmessage = (incoming) => {
    // The DSP emits 26 values; sanitize them before rendering.
    bands.value = Array.from({ length: 26 }, (_, index) => {
      const value = incoming[index] ?? 0
      return Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : 0
    })
  }

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
    <header class="header">
      <div class="brand">
        <h1>LUNA ANALYZER</h1>
        <span class="eyebrow">SPECTRUM</span>
      </div>
      <nav class="actions" aria-label="Main controls">
        <span class="live-indicator" :class="{ active: audioRunning }">
          <span class="dot" />{{ audioRunning ? 'AUDIO LIVE' : 'AUDIO OFF' }}
        </span>
        <button :class="{ selected: activePanel === 'fx' }" @click="togglePanel('fx')"></button>
        <button :class="{ selected: activePanel === 'settings' }" @click="togglePanel('settings')"
          aria-label="Configurations">⚙</button>
      </nav>
    </header>

    <div class="status-line" :class="{ error: statusIsError }" role="status">
      <span>{{ statusMessage }}</span>
      <span class="route-status">{{ routingStatus }}</span>
    </div>

    <section class="spectrum-panel" aria-label="Audio spectrum">
      <Spectrum :bands="bands" />
      <div v-if="!audioRunning" class="spectrum-empty">Aguardando sinal de áudio…</div>
    </section>

    <section v-if="activePanel === 'settings'" class="panel">
      <div class="panel-heading">
        <div>
          <p class="eyebrow">AUDIO ROUTING</p>
          <h2>Configurações</h2>
        </div>
        <button class="close-button" @click="activePanel = null" aria-label="Fechar configurações">×</button>
      </div>

      <label class="field">
        <span>Dispositivo de entrada</span>
        <select v-model.number="selectedInput">
          <option v-if="!devices?.inputs.length" :value="null" disabled>Nenhuma entrada disponível</option>
          <option v-for="device in devices?.inputs ?? []" :key="`${device.index}-${device.name}`" :value="device.index">
            {{ device.name }}
          </option>
        </select>
      </label>

      <label class="field">
        <span>Dispositivo de saída</span>
        <select v-model.number="selectedOutput">
          <option :value="null">Somente análise (sem monitoramento)</option>
          <option v-for="device in devices?.outputs ?? []" :key="`${device.index}-${device.name}`"
            :value="device.index">
            {{ device.name }}
          </option>
        </select>
      </label>

      <p v-if="selectedOutput === null" class="hint">
        Sem monitoramento, o espectro continua funcionando, mas o áudio processado não sai pelas caixas. Como o roteador
        envia os apps ao sink virtual, o áudio do sistema pode ficar inaudível até você selecionar uma saída.
      </p>

      <div class="panel-actions">
        <button class="secondary-button" @click="refreshDevices">Atualizar lista</button>
        <button class="primary-button" @click="applyAudioSettings">Aplicar dispositivos</button>
      </div>
    </section>

    <section v-if="activePanel === 'fx'" class="panel">
      <div class="panel-heading">
        <div>
          <p class="eyebrow">EFFECTS</p>
          <h2>Vinyl Simulation</h2>
        </div>
        <button class="close-button" @click="activePanel = null" aria-label="Fechar efeitos">×</button>
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
  </main>
</template>

<style>
:root {
  color-scheme: dark;
  font-family: Inter, ui-sans-serif, system-ui, sans-serif;
  color: #edd6fe;
  background: #10182b;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
}

html,
body,
#app {
  width: 100%;
  min-width: 0;
  height: 100%;
  min-height: 100%;
  margin: 0;
}

* {
  box-sizing: border-box;
}

body {
  overflow: auto;
}

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
  background: #10182b;
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  min-height: 38px;
  padding-bottom: 12px;
  border-bottom: 1px solid rgba(221, 208, 251, 0.14);
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
.secondary-button,
.primary-button {
  padding: 7px 11px;
  color: #edd6fe;
  background: rgba(26, 36, 120, 0.18);
  border: 1px solid rgba(221, 208, 251, 0.2);
  border-radius: 6px;
}

.actions button:hover,
.actions button.selected,
.secondary-button:hover {
  border-color: rgba(20, 207, 244, 0.75);
  background: rgba(20, 207, 244, 0.08);
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

.status-line {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 6px 16px;
  padding: 9px 1px;
  color: #a8afc5;
  font-size: 11px;
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
  height: min(42vh, 300px);
  min-height: 180px;
  overflow: hidden;
  border: 1px solid rgba(221, 208, 251, 0.12);
  border-radius: 8px;
  background: rgba(26, 36, 120, 0.14);
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
  color: rgba(237, 214, 254, 0.45);
  font-size: 12px;
  pointer-events: none;
}

.panel {
  width: min(100%, 560px);
  margin-top: 14px;
  padding: 16px;
  border: 1px solid rgba(221, 208, 251, 0.18);
  border-radius: 10px;
  background: #151e33;
  box-shadow: 0 12px 36px rgba(0, 0, 0, 0.18);
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
  width: 28px;
  height: 28px;
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
  margin-top: 18px;
}

.primary-button {
  background: #5e4a98;
  border-color: #7761b9;
}

.primary-button:hover {
  background: #6d57aa;
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
