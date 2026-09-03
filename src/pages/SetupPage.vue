<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

interface CameraMode {
  device_name: string;
  dshow_index: number;
  width: number;
  height: number;
  fourcc: string;
}

const modes = ref<CameraMode[]>([]);
const selectedKey = ref<string | null>(null);
const isLoading = ref(false);
const errorMessage = ref('');

function modeKey(mode: CameraMode): string {
  return `${mode.dshow_index}|${mode.device_name}|${mode.width}|${mode.height}|${mode.fourcc}`;
}

const selectedMode = computed(() => {
  if (selectedKey.value === null) {
    return null;
  }
  return modes.value.find((mode) => modeKey(mode) === selectedKey.value) ?? null;
});

const canStartCapture = computed(() => selectedMode.value !== null);

async function loadCameras(): Promise<void> {
  isLoading.value = true;
  errorMessage.value = '';
  selectedKey.value = null;
  try {
    modes.value = await invoke<CameraMode[]>('list_cameras');
  } catch (err) {
    modes.value = [];
    errorMessage.value = err instanceof Error ? err.message : String(err);
  } finally {
    isLoading.value = false;
  }
}

function handleSelectRow(mode: CameraMode): void {
  selectedKey.value = modeKey(mode);
}

function handleRowKeydown(event: KeyboardEvent, mode: CameraMode): void {
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault();
    handleSelectRow(mode);
  }
}

function handleStartCapture(): void {
  if (!canStartCapture.value) {
    return;
  }
}

onMounted(() => {
  void loadCameras();
});
</script>

<template>
  <main class="setup">
    <header class="setup__header">
      <h1>相机与分辨率</h1>
      <p>必须点选一行后再开始采集。不会预选任何相机。</p>
    </header>

    <div class="setup__toolbar">
      <button type="button" class="btn btn--ghost" :disabled="isLoading" @click="loadCameras">
        刷新列表
      </button>
      <button
        type="button"
        class="btn btn--primary"
        :disabled="!canStartCapture"
        aria-label="开始采集"
        @click="handleStartCapture"
      >
        开始采集
      </button>
    </div>

    <p v-if="isLoading" class="setup__status">正在枚举 DirectShow 相机...</p>
    <p v-else-if="errorMessage" class="setup__status setup__status--error">{{ errorMessage }}</p>
    <p v-else-if="modes.length === 0" class="setup__status">
      未检测到相机，请关闭 Windows 相机应用后重试。
    </p>

    <div v-if="modes.length > 0" class="setup__table-wrap">
      <table class="mode-table">
        <thead>
          <tr>
            <th scope="col">设备名</th>
            <th scope="col">DirectShow 索引</th>
            <th scope="col">分辨率</th>
            <th scope="col">格式</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="mode in modes"
            :key="modeKey(mode)"
            :class="{ 'is-selected': selectedKey === modeKey(mode) }"
            :aria-selected="selectedKey === modeKey(mode)"
            tabindex="0"
            role="button"
            :aria-label="`${mode.device_name} ${mode.width}x${mode.height} ${mode.fourcc}`"
            @click="handleSelectRow(mode)"
            @keydown="handleRowKeydown($event, mode)"
          >
            <td>{{ mode.device_name }}</td>
            <td>{{ mode.dshow_index }}</td>
            <td>{{ mode.width }}x{{ mode.height }}</td>
            <td>{{ mode.fourcc }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </main>
</template>

<style scoped>
.setup {
  font-family: system-ui, sans-serif;
  padding: 1.5rem 2rem 2.5rem;
  max-width: 960px;
  margin: 0 auto;
  color: #1a1a1a;
}

.setup__header h1 {
  margin: 0 0 0.35rem;
  font-size: 1.4rem;
}

.setup__header p {
  margin: 0;
  color: #555;
}

.setup__toolbar {
  display: flex;
  gap: 0.75rem;
  margin: 1.25rem 0 1rem;
}

.btn {
  border: 1px solid #ccc;
  background: #fff;
  padding: 0.45rem 1rem;
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.95rem;
}

.btn:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.btn--primary {
  background: #1b6bff;
  border-color: #1b6bff;
  color: #fff;
}

.btn--ghost:not(:disabled):hover {
  background: #f3f3f3;
}

.setup__status {
  margin: 0.5rem 0 1rem;
}

.setup__status--error {
  color: #b00020;
}

.setup__table-wrap {
  overflow: auto;
  border: 1px solid #ddd;
  border-radius: 8px;
}

.mode-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.95rem;
}

.mode-table th,
.mode-table td {
  text-align: left;
  padding: 0.55rem 0.75rem;
  border-bottom: 1px solid #eee;
}

.mode-table tbody tr {
  cursor: pointer;
}

.mode-table tbody tr:hover {
  background: #f5f8ff;
}

.mode-table tbody tr.is-selected {
  background: #dce8ff;
}

.mode-table tbody tr:focus {
  outline: 2px solid #1b6bff;
  outline-offset: -2px;
}
</style>
