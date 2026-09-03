<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

const props = defineProps<{
  initialDir?: string;
}>();

const emit = defineEmits<{
  begin: [outputDir: string];
}>();

const outputDir = ref(props.initialDir ?? '');
const errorMessage = ref('');

async function loadDefaultDir(): Promise<string> {
  try {
    return await invoke<string>('default_output_dir');
  } catch (err) {
    errorMessage.value = err instanceof Error ? err.message : String(err);
    return '';
  }
}

async function handleBegin(): Promise<void> {
  let dir = outputDir.value.trim();
  if (!dir) {
    dir = await loadDefaultDir();
    if (dir) {
      outputDir.value = dir;
    }
  }
  emit('begin', dir);
}

onMounted(async () => {
  if (outputDir.value.trim()) {
    return;
  }
  const dir = await loadDefaultDir();
  if (dir) {
    outputDir.value = dir;
  }
});
</script>

<template>
  <main class="start">
    <header class="start__header">
      <h1>ChArUco Capture</h1>
      <p>选择输出目录后开始。默认桌面 ChArUcoCapture。</p>
    </header>

    <label class="start__dir">
      输出目录
      <input
        v-model="outputDir"
        type="text"
        spellcheck="false"
        aria-label="输出目录"
        placeholder="Desktop/ChArUcoCapture"
      />
    </label>

    <p v-if="errorMessage" class="start__status start__status--error">{{ errorMessage }}</p>

    <div class="start__actions">
      <button type="button" class="btn btn--primary" aria-label="开始" @click="handleBegin">
        开始
      </button>
    </div>
  </main>
</template>

<style scoped>
.start {
  font-family: system-ui, sans-serif;
  padding: 1.5rem 2rem 2.5rem;
  max-width: 720px;
  margin: 0 auto;
  color: #1a1a1a;
}

.start__header h1 {
  margin: 0 0 0.35rem;
  font-size: 1.4rem;
}

.start__header p {
  margin: 0;
  color: #555;
}

.start__dir {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  margin: 1.5rem 0 1rem;
  font-size: 0.9rem;
  color: #444;
}

.start__dir input {
  padding: 0.45rem 0.6rem;
  border: 1px solid #ccc;
  border-radius: 6px;
  font-size: 0.95rem;
}

.start__status {
  margin: 0 0 1rem;
}

.start__status--error {
  color: #b00020;
}

.start__actions {
  display: flex;
  gap: 0.75rem;
}

.btn {
  border: 1px solid #ccc;
  background: #fff;
  padding: 0.45rem 1rem;
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.95rem;
}

.btn--primary {
  background: #1b6bff;
  border-color: #1b6bff;
  color: #fff;
}
</style>
