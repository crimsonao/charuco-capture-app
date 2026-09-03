<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

interface CameraMode {
  device_name: string;
  dshow_index: number;
  width: number;
  height: number;
  fourcc: string;
}

const props = defineProps<{
  mode: CameraMode;
}>();

const emit = defineEmits<{
  stop: [];
}>();

const jpegSrc = ref('');
const hint = ref('请把标定板放进画面');
const nCorners = ref(0);
const imageCount = ref(0);
const countTarget = ref(15);
const corners = ref<[number, number][]>([]);
const actualWidth = ref<number | null>(null);
const actualHeight = ref<number | null>(null);
const backend = ref('');
const errorMessage = ref('');
const isStarting = ref(true);

let unlisten: UnlistenFn | undefined;

onMounted(async () => {
  unlisten = await listen<{
    jpeg_base64: string;
    hint: string;
    n_corners: number;
    corners: [number, number][];
    image_count?: number;
    count_target?: number;
  }>('frame', (event) => {
    jpegSrc.value = `data:image/jpeg;base64,${event.payload.jpeg_base64}`;
    hint.value = event.payload.hint;
    nCorners.value = event.payload.n_corners;
    corners.value = event.payload.corners ?? [];
    imageCount.value = event.payload.image_count ?? 0;
    countTarget.value = event.payload.count_target ?? 15;
  });
  try {
    const [width, height, openedBackend] = await invoke<[number, number, string]>(
      'start_preview',
      { req: props.mode },
    );
    actualWidth.value = width;
    actualHeight.value = height;
    backend.value = openedBackend;
  } catch (err) {
    errorMessage.value = err instanceof Error ? err.message : String(err);
  } finally {
    isStarting.value = false;
  }
});

onUnmounted(() => {
  if (unlisten) {
    unlisten();
  }
  void invoke('stop_session');
});

async function handleStop(): Promise<void> {
  try {
    await invoke('stop_session');
  } finally {
    emit('stop');
  }
}
</script>

<template>
  <main class="capture">
    <header class="capture__header">
      <div>
        <h1>预览</h1>
        <p>
          {{ mode.device_name }} · 请求 {{ mode.width }}x{{ mode.height }} {{ mode.fourcc }}
          <template v-if="actualWidth !== null && actualHeight !== null">
            · 实际 {{ actualWidth }}x{{ actualHeight }} {{ backend }}
          </template>
        </p>
      </div>
      <button type="button" class="btn" aria-label="停止预览" @click="handleStop">停止</button>
    </header>

    <p v-if="isStarting" class="capture__status">正在打开相机...</p>
    <p v-else-if="errorMessage" class="capture__status capture__status--error">{{ errorMessage }}</p>
    <p v-else class="capture__status">
      {{ hint }} · 角点 {{ nCorners }} · 已存 {{ imageCount }}/{{ countTarget }}
    </p>

    <div class="capture__frame">
      <div v-if="jpegSrc" class="capture__stage">
        <img
          :src="jpegSrc"
          :alt="`${mode.device_name} preview`"
          class="capture__image"
        />
        <svg
          v-if="corners.length && actualWidth && actualHeight"
          class="capture__overlay"
          :viewBox="`0 0 ${actualWidth} ${actualHeight}`"
          preserveAspectRatio="xMidYMid meet"
          aria-hidden="true"
        >
          <rect
            v-for="(pt, index) in corners"
            :key="index"
            :x="pt[0] - 6"
            :y="pt[1] - 6"
            width="12"
            height="12"
            fill="none"
            :stroke="nCorners >= 6 ? '#00dc00' : '#00c8ff'"
            stroke-width="2"
          />
        </svg>
      </div>
      <p v-else-if="!isStarting && !errorMessage" class="capture__status">等待画面...</p>
    </div>
  </main>
</template>

<style scoped>
.capture {
  font-family: system-ui, sans-serif;
  padding: 1.5rem 2rem 2.5rem;
  max-width: 1100px;
  margin: 0 auto;
  color: #1a1a1a;
}

.capture__header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1rem;
}

.capture__header h1 {
  margin: 0 0 0.35rem;
  font-size: 1.4rem;
}

.capture__header p {
  margin: 0;
  color: #555;
}

.btn {
  border: 1px solid #ccc;
  background: #fff;
  padding: 0.45rem 1rem;
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.95rem;
}

.capture__status {
  margin: 1rem 0;
}

.capture__status--error {
  color: #b00020;
}

.capture__frame {
  border: 1px solid #ddd;
  border-radius: 8px;
  min-height: 240px;
  background: #111;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}

.capture__stage {
  position: relative;
  display: inline-block;
  max-width: 100%;
}

.capture__image {
  max-width: 100%;
  max-height: 70vh;
  display: block;
}

.capture__overlay {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}
</style>
