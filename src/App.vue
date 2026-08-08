<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import iconUrl from './assets/colorify-icon.png'
import applyIcon from './assets/icon-apply.png'
import behaviorIcon from './assets/icon-behavior.png'
import deleteIcon from './assets/icon-delete.png'
import exportIcon from './assets/icon-export.png'
import importIcon from './assets/icon-import.png'
import lightIcon from './assets/icon-light.png'
import profilesIcon from './assets/icon-profiles.png'
import resetIcon from './assets/icon-reset.png'
import saveIcon from './assets/icon-save.png'

type ProfileKey = 'brightness' | 'contrast' | 'saturation' | 'hue' | 'gamma' | 'temperature'

type Profile = {
  name: string
  brightness: number
  contrast: number
  saturation: number
  hue: number
  gamma: number
  temperature: number
}

type Control = {
  key: ProfileKey
  label: string
  min: number
  max: number
  step: number
}

type ControlGroup = {
  title: string
  tone: 'light' | 'color' | 'advanced'
  keys: ProfileKey[]
}

type WallpaperSelection = {
  path: string
  dataUrl: string
}

type StartupSettings = {
  enabled: boolean
  startMinimized: boolean
}

const appWindow = getCurrentWindow()
const appVersion = 'V1.5.1'
const profiles = ref<Profile[]>([])
const selectedName = ref('Default')
const status = ref('Current profile applied.')
const isOn = ref(true)
const autoApply = ref(true)
const startWithWindows = ref(false)
const startMinimized = ref(false)
const profileName = ref('')
const editingValue = ref<ProfileKey | null>(null)
const editText = ref('')
const wallpaperUrl = ref('')
const pendingDeleteProfile = ref<Profile | null>(null)
const showLegal = ref(false)
const behaviorReady = ref(false)

const current = ref<Profile>({
  name: 'Default',
  brightness: 0,
  contrast: 1,
  saturation: 1,
  hue: 0,
  gamma: 1,
  temperature: 6500,
})

const controls: Control[] = [
  { key: 'brightness', label: 'Brightness', min: -0.25, max: 0.25, step: 0.01 },
  { key: 'contrast', label: 'Contrast', min: 0.5, max: 2, step: 0.01 },
  { key: 'saturation', label: 'Saturation', min: 0, max: 5, step: 0.01 },
  { key: 'hue', label: 'Hue', min: -180, max: 180, step: 1 },
  { key: 'gamma', label: 'Gamma', min: 0.5, max: 2.5, step: 0.01 },
  { key: 'temperature', label: 'Temperature', min: 3000, max: 10000, step: 100 },
]

const controlGroups: ControlGroup[] = [
  { title: 'Light & Contrast', tone: 'light', keys: ['brightness', 'contrast'] },
  { title: 'Color & Saturation', tone: 'color', keys: ['saturation', 'hue'] },
  { title: 'Advanced', tone: 'advanced', keys: ['gamma', 'temperature'] },
]

const groupIcons: Partial<Record<ControlGroup['tone'], string>> = {
  light: lightIcon,
}

const selectedProfile = computed(() => profiles.value.find((profile) => profile.name === selectedName.value))
const wallpaperStyle = computed(() => ({
  backgroundImage: wallpaperUrl.value ? `url("${wallpaperUrl.value}")` : 'none',
}))
const builtInProfileNames = new Set([
  'Custom',
  'Default',
  'FPS Visibility',
  'FPS Clarity',
  'Competitive Clarity',
  'Black Equalizer',
  'Balanced Gaming',
  'Digital Vibrance',
  'Vibrant Color',
  'Vivid Colors',
  'True Color',
  'Dark Scene Lift',
  'Night Visibility',
  'Cinema Warm',
  'Warm Media',
])
const controlMap = new Map(controls.map((control) => [control.key, control]))

function cloneProfile(profile: Profile): Profile {
  return { ...profile }
}

function isProfile(value: unknown): value is Profile {
  if (!value || typeof value !== 'object') return false
  const profile = value as Record<string, unknown>
  return (
    typeof profile.name === 'string' &&
    typeof profile.brightness === 'number' &&
    typeof profile.contrast === 'number' &&
    typeof profile.saturation === 'number' &&
    typeof profile.hue === 'number' &&
    typeof profile.gamma === 'number' &&
    typeof profile.temperature === 'number'
  )
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value))
}

function rangePercent(value: number, min: number, max: number) {
  return clamp(((value - min) / (max - min)) * 100, 0, 100)
}

function displayPercent(value: number, min: number, max: number) {
  return Math.round(rangePercent(value, min, max))
}

function valueText(control: Control) {
  const value = current.value[control.key]
  if (control.key === 'saturation') return `${Math.round(value * 100)}%`
  if (control.key === 'hue') return `${Math.round(value)}°`
  if (control.key === 'temperature') return `${Math.round(value)}K`
  return `${displayPercent(value, control.min, control.max)}%`
}

function controlsForGroup(group: ControlGroup) {
  return group.keys.map((key) => controlMap.get(key)).filter((control): control is Control => Boolean(control))
}

function textToValue(control: Control, text: string) {
  const clean = text.trim().replace('%', '').replace('°', '').replace('K', '').replace('k', '').replace(',', '.')
  const parsed = Number(clean)
  if (!Number.isFinite(parsed)) return current.value[control.key]
  if (control.key === 'saturation') return clamp(parsed / 100, control.min, control.max)
  if (control.key === 'hue') return clamp(parsed, control.min, control.max)
  if (control.key === 'temperature') return clamp(parsed, control.min, control.max)
  return clamp(control.min + (control.max - control.min) * (parsed / 100), control.min, control.max)
}

function selectProfile(name: string) {
  selectedName.value = name
  const profile = selectedProfile.value
  if (!profile) return
  current.value = cloneProfile(profile)
  void apply()
}

function syncCustomProfile() {
  current.value.name = 'Custom'
  selectedName.value = 'Custom'

  const customIndex = profiles.value.findIndex((profile) => profile.name === 'Custom')
  const customProfile = cloneProfile(current.value)
  if (customIndex >= 0) {
    profiles.value.splice(customIndex, 1, customProfile)
  } else {
    profiles.value.unshift(customProfile)
  }

  persistProfiles()
}

function persistProfiles() {
  localStorage.setItem('colorify.profiles', JSON.stringify(profiles.value))
  localStorage.setItem('colorify.selectedName', selectedName.value)
}

function persistCurrentProfile() {
  localStorage.setItem('colorify.currentProfile', JSON.stringify(current.value))
  localStorage.setItem('colorify.selectedName', selectedName.value)
}

async function apply() {
  if (!isOn.value) {
    status.value = 'Effect is OFF.'
    return
  }

  try {
    status.value = await invoke<string>('apply_profile', { profile: current.value })
    persistCurrentProfile()
  } catch (error) {
    status.value = String(error)
  }
}

async function reset() {
  const selected = selectedProfile.value
  const defaults = profiles.value.find((profile) => profile.name === 'Default')

  if (selectedName.value === 'Custom') {
    if (!defaults) return
    current.value = { ...cloneProfile(defaults), name: 'Custom' }
    syncCustomProfile()
  } else if (selected) {
    current.value = cloneProfile(selected)
  } else if (defaults) {
    selectedName.value = defaults.name
    current.value = cloneProfile(defaults)
  }

  persistCurrentProfile()
  await apply()
}

function resetControl(key: ProfileKey) {
  const defaults = profiles.value.find((profile) => profile.name === 'Default')
  if (!defaults) return
  current.value[key] = defaults[key] as never
  syncCustomProfile()
  persistCurrentProfile()
  void apply()
}

async function togglePower() {
  isOn.value = !isOn.value
  if (isOn.value) {
    await apply()
  } else {
    await invoke<string>('reset_display').catch(() => undefined)
    status.value = 'Effect is OFF.'
  }
}

function setValue(key: ProfileKey, event: Event) {
  const input = event.target as HTMLInputElement
  current.value[key] = Number(input.value) as never
  syncCustomProfile()
  persistCurrentProfile()
  if (autoApply.value) void apply()
}

function startEdit(control: Control) {
  editingValue.value = control.key
  editText.value = valueText(control)
  nextTick(() => {
    const input = document.querySelector<HTMLInputElement>('.value-input')
    input?.focus()
    input?.select()
  })
}

function commitEdit(control: Control) {
  current.value[control.key] = textToValue(control, editText.value) as never
  editingValue.value = null
  syncCustomProfile()
  persistCurrentProfile()
  void apply()
}

function selectValueText(event: FocusEvent | MouseEvent) {
  const input = event.target as HTMLInputElement
  input.select()
}

function saveProfile() {
  const name = profileName.value.trim()
  if (!name) {
    status.value = 'Enter a profile name first.'
    return
  }

  const saved = { ...current.value, name }
  const existing = profiles.value.findIndex((profile) => profile.name === name)
  if (existing >= 0) profiles.value.splice(existing, 1, saved)
  else profiles.value.push(saved)
  selectedName.value = name
  current.value = cloneProfile(saved)
  profileName.value = ''
  status.value = `Saved profile: ${name}`
  persistProfiles()
  persistCurrentProfile()
}

function deleteProfile() {
  const index = profiles.value.findIndex((profile) => profile.name === selectedName.value)
  if (index < 0 || profiles.value.length <= 1) {
    status.value = 'Keep at least one profile.'
    return
  }

  pendingDeleteProfile.value = cloneProfile(profiles.value[index])
}

function cancelDeleteProfile() {
  pendingDeleteProfile.value = null
  status.value = 'Delete cancelled.'
}

function confirmDeleteProfile() {
  const profile = pendingDeleteProfile.value
  if (!profile) return

  const index = profiles.value.findIndex((item) => item.name === profile.name)
  if (index < 0) {
    pendingDeleteProfile.value = null
    status.value = 'Profile was already removed.'
    return
  }

  profiles.value.splice(index, 1)
  const defaults = profiles.value.find((item) => item.name === 'Default') ?? profiles.value[0]
  if (defaults) selectProfile(defaults.name)
  pendingDeleteProfile.value = null
  status.value = `Deleted profile: ${profile.name}`
  persistProfiles()
}

async function runCommand(command: string) {
  try {
    status.value = await invoke<string>(command)
  } catch (error) {
    status.value = String(error)
  }
}

function clearWallpaper() {
  wallpaperUrl.value = ''
  localStorage.removeItem('colorify.wallpaperPath')
  status.value = 'Wallpaper cleared.'
}

async function chooseWallpaper() {
  try {
    const wallpaper = await invoke<WallpaperSelection | null>('choose_wallpaper')
    if (!wallpaper) {
      status.value = 'Image selection cancelled.'
      return
    }

    wallpaperUrl.value = wallpaper.dataUrl
    localStorage.setItem('colorify.wallpaperPath', wallpaper.path)
    status.value = 'Background image selected.'
  } catch (error) {
    status.value = String(error)
  }
}

async function importProfiles() {
  try {
    const imported = await invoke<Profile[]>('import_profiles')
    if (imported.length === 0) {
      status.value = 'Import cancelled.'
      return
    }

    for (const importedProfile of imported) {
      const existing = profiles.value.findIndex((profile) => profile.name === importedProfile.name)
      if (existing >= 0) profiles.value.splice(existing, 1, importedProfile)
      else profiles.value.push(importedProfile)
    }

  selectedName.value = imported[0].name
  current.value = cloneProfile(imported[0])
  persistProfiles()
  persistCurrentProfile()
  status.value = `Imported ${imported.length} profile${imported.length === 1 ? '' : 's'}.`
  void apply()
  } catch (error) {
    status.value = String(error)
  }
}

async function exportProfile() {
  try {
    status.value = await invoke<string>('export_profile', { profile: current.value })
  } catch (error) {
    status.value = String(error)
  }
}

function titlebarDrag(event: MouseEvent) {
  if (event.detail > 1) return
  if ((event.target as HTMLElement).closest('button, input')) return
  void appWindow.startDragging()
}

function titlebarDoubleClick(event: MouseEvent) {
  if ((event.target as HTMLElement).closest('button, input')) return
  void appWindow.toggleMaximize()
}

onMounted(async () => {
  const storedWallpaperPath = localStorage.getItem('colorify.wallpaperPath')
  if (storedWallpaperPath) {
    try {
      wallpaperUrl.value = await invoke<string>('load_wallpaper', { path: storedWallpaperPath })
    } catch {
      localStorage.removeItem('colorify.wallpaperPath')
    }
  }

  const storedBehavior = localStorage.getItem('colorify.behavior')
  if (storedBehavior) {
    try {
      const behavior = JSON.parse(storedBehavior) as {
        autoApply?: boolean
        startWithWindows?: boolean
        startMinimized?: boolean
      }
      autoApply.value = behavior.autoApply ?? autoApply.value
      startWithWindows.value = behavior.startWithWindows ?? startWithWindows.value
      startMinimized.value = behavior.startMinimized ?? startMinimized.value
    } catch {
      localStorage.removeItem('colorify.behavior')
    }
  }

  try {
    const startup = await invoke<StartupSettings>('get_startup_settings')
    startWithWindows.value = startup.enabled
    startMinimized.value = startup.startMinimized
  } catch (error) {
    status.value = String(error)
  } finally {
    behaviorReady.value = true
  }

  const builtIns = await invoke<Profile[]>('get_profiles')
  const stored = localStorage.getItem('colorify.profiles')
  const storedProfiles = stored ? (JSON.parse(stored) as Profile[]) : []
  const customProfiles = storedProfiles.filter((profile) => !builtInProfileNames.has(profile.name))
  const storedCustom = storedProfiles.find((profile) => profile.name === 'Custom' && isProfile(profile))
  profiles.value = [...builtIns, ...customProfiles]
  if (storedCustom) {
    const customIndex = profiles.value.findIndex((profile) => profile.name === 'Custom')
    if (customIndex >= 0) profiles.value.splice(customIndex, 1, cloneProfile(storedCustom))
  }

  const storedSelected = localStorage.getItem('colorify.selectedName')
  const storedCurrent = localStorage.getItem('colorify.currentProfile')
  const first = profiles.value.find((profile) => profile.name === storedSelected) ?? profiles.value[0]

  if (storedCurrent) {
    try {
      const parsed = JSON.parse(storedCurrent)
      if (isProfile(parsed)) {
        selectedName.value = parsed.name
        current.value = cloneProfile(parsed)
        void apply()
        return
      }
    } catch {
      localStorage.removeItem('colorify.currentProfile')
    }
  }

  if (first) {
    selectedName.value = first.name
    current.value = cloneProfile(first)
    void apply()
  }
})

watch([autoApply, startWithWindows, startMinimized], async () => {
  if (!startWithWindows.value && startMinimized.value) startMinimized.value = false

  localStorage.setItem(
    'colorify.behavior',
    JSON.stringify({
      autoApply: autoApply.value,
      startWithWindows: startWithWindows.value,
      startMinimized: startMinimized.value,
    }),
  )

  if (!behaviorReady.value) return

  try {
    status.value = await invoke<string>('set_startup_settings', {
      enabled: startWithWindows.value,
      startMinimized: startMinimized.value,
    })
  } catch (error) {
    status.value = String(error)
  }
})
</script>

<template>
  <main class="app-shell">
    <div class="wallpaper-layer" :class="{ visible: wallpaperUrl }" :style="wallpaperStyle"></div>
    <header class="titlebar" data-tauri-drag-region @pointerdown="titlebarDrag" @dblclick="titlebarDoubleClick">
      <div class="brand" data-tauri-drag-region>
        <img :src="iconUrl" alt="" data-tauri-drag-region />
        <span data-tauri-drag-region>Colorify</span>
        <small data-tauri-drag-region>{{ appVersion }}</small>
      </div>

      <div class="top-actions" data-tauri-drag-region>
        <button class="icon-button" @click.stop="apply">
          <img :src="applyIcon" alt="" />
          <span>Apply</span>
        </button>
        <button class="icon-button reset-button" aria-label="Reset" @click.stop="reset">
          <img :src="resetIcon" alt="" />
          <span>Reset</span>
        </button>
        <button class="power-switch" :class="{ off: !isOn }" type="button" role="switch" :aria-checked="isOn" @click.stop="togglePower">
          <span>{{ isOn ? 'ON' : 'OFF' }}</span>
        </button>
      </div>

      <div class="window-actions">
        <button class="window-button minimize" aria-label="Minimize" @click.stop="appWindow.minimize()"><span></span></button>
        <button class="window-button maximize" aria-label="Maximize" @click.stop="appWindow.toggleMaximize()"><span></span></button>
        <button class="window-button close" aria-label="Close" @click.stop="appWindow.close()"><span></span></button>
      </div>
    </header>

    <section class="workspace">
      <aside class="profiles">
        <p class="eyebrow with-icon"><img :src="profilesIcon" alt="" /> Profiles</p>
        <button
          v-for="profile in profiles"
          :key="profile.name"
          class="profile-button"
          :class="{ active: profile.name === selectedName }"
          @click="selectProfile(profile.name)"
        >
          {{ profile.name }}
        </button>
        <div class="profile-actions">
          <button class="icon-button danger" @click="deleteProfile">
            <img :src="deleteIcon" alt="" />
            <span>Delete profile</span>
          </button>
          <button @click="runCommand('open_data_folder')">Open data folder</button>
        </div>
      </aside>

      <section class="panel controls">
        <div class="section-head">
          <h1>Color controls</h1>
          <p>{{ status }}</p>
        </div>

        <div class="sliders">
          <section v-for="group in controlGroups" :key="group.title" class="control-group">
            <div class="group-head">
              <div class="group-title">
                <img v-if="groupIcons[group.tone]" class="group-image-icon" :src="groupIcons[group.tone]" alt="" />
                <span v-else-if="group.tone === 'color'" class="group-icon color"></span>
                <span>{{ group.title }}</span>
              </div>
            </div>

            <div v-for="control in controlsForGroup(group)" :key="control.key" class="slider-row">
              <span>{{ control.label }}</span>
              <input
                v-if="editingValue === control.key"
                class="value-input"
                v-model="editText"
                autofocus
                @focus="selectValueText"
                @click="selectValueText"
                @keydown.enter.prevent="commitEdit(control)"
                @blur="commitEdit(control)"
              />
              <button v-else class="value-pill" type="button" @click="startEdit(control)">
                {{ valueText(control) }}
              </button>
              <input
                type="range"
                :aria-label="control.label"
                :class="{ hue: control.key === 'hue' }"
                :style="{ '--range-percent': `${rangePercent(current[control.key], control.min, control.max)}%` }"
                :min="control.min"
                :max="control.max"
                :step="control.step"
                :value="current[control.key]"
                @input="setValue(control.key, $event)"
              />
              <button class="mini-reset reset-button" type="button" @click="resetControl(control.key)">
                <img :src="resetIcon" alt="" />
              </button>
            </div>
          </section>
        </div>
      </section>

      <aside class="panel side">
        <section>
          <h2 class="with-icon"><img :src="saveIcon" alt="" /> Create profile</h2>
          <p>Save the current color settings.</p>
          <input v-model="profileName" placeholder="Profile name" />
          <button class="icon-button" @click="saveProfile">
            <img :src="saveIcon" alt="" />
            <span>Save profile</span>
          </button>
        </section>

        <section>
          <h2 class="with-icon"><img :src="behaviorIcon" alt="" /> Behavior</h2>
          <label class="tooltip-wrap">
            <input v-model="autoApply" type="checkbox" />
            Auto apply while sliding
            <span class="tooltip">Applies color changes instantly as you move a slider.</span>
          </label>
          <label class="tooltip-wrap">
            <input v-model="startWithWindows" type="checkbox" />
            Start with Windows
            <span class="tooltip">Launches Colorify automatically after you sign in to Windows.</span>
          </label>
          <label v-if="startWithWindows" class="tooltip-wrap">
            <input v-model="startMinimized" type="checkbox" />
            Start minimized
            <span class="tooltip">Starts Colorify in the tray/background instead of opening the window.</span>
          </label>
        </section>

        <section>
          <h2>Background image</h2>
          <button @click="chooseWallpaper">Choose image</button>
          <button @click="clearWallpaper">Clear wallpaper</button>
        </section>

        <div class="side-grid">
          <button class="icon-button" @click="importProfiles">
            <img :src="importIcon" alt="" />
            <span>Import</span>
          </button>
          <button class="icon-button" @click="exportProfile">
            <img :src="exportIcon" alt="" />
            <span>Export</span>
          </button>
        </div>
        <button class="credits-button" type="button" @click="showLegal = true">Legal & Credits</button>
      </aside>
    </section>

    <div v-if="pendingDeleteProfile" class="modal-backdrop" @click.self="cancelDeleteProfile">
      <section class="confirm-modal" role="dialog" aria-modal="true" aria-labelledby="delete-profile-title">
        <div class="modal-icon">
          <img :src="deleteIcon" alt="" />
        </div>
        <div class="modal-copy">
          <h2 id="delete-profile-title">Delete profile?</h2>
          <p>
            <strong>{{ pendingDeleteProfile.name }}</strong> will be removed from your profile list.
            This action cannot be undone.
          </p>
        </div>
        <div class="modal-actions">
          <button type="button" class="modal-cancel" @click="cancelDeleteProfile">Cancel</button>
          <button type="button" class="modal-delete" @click="confirmDeleteProfile">Delete</button>
        </div>
      </section>
    </div>

    <div v-if="showLegal" class="modal-backdrop" @click.self="showLegal = false">
      <section class="confirm-modal credits-modal legal-modal" role="dialog" aria-modal="true" aria-labelledby="legal-title">
        <div class="modal-copy">
          <h2 id="legal-title">Legal & Credits</h2>
          <h3>License</h3>
          <p>
            Colorify is proprietary software. You may use it, but you may not redistribute,
            resell, repackage, or claim it as your own.
          </p>
          <h3>Privacy</h3>
          <p>
            Colorify stores profiles, behavior settings, current slider values, and optional
            wallpaper paths locally on your device. This build does not include accounts,
            analytics, advertising trackers, or telemetry.
          </p>
          <h3>Disclaimer</h3>
          <p>
            Colorify adjusts display colors using operating-system level display APIs.
            It does not read game memory, inject into games, or modify game files.
            Compatibility and rule acceptance are not guaranteed.
          </p>
          <h3>Credits</h3>
          <p>
            Icons made by Flaticon contributors from
            <a href="https://www.flaticon.com" target="_blank" rel="noreferrer">www.flaticon.com</a>.
          </p>
        </div>
        <div class="modal-actions single">
          <button type="button" class="modal-cancel" @click="showLegal = false">Done</button>
        </div>
      </section>
    </div>
  </main>
</template>
