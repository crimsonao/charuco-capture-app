<script setup lang="ts">
import { ref } from 'vue';
import CapturePage from './pages/CapturePage.vue';
import SetupPage from './pages/SetupPage.vue';

interface CameraMode {
  device_name: string;
  dshow_index: number;
  width: number;
  height: number;
  fourcc: string;
}

const page = ref<'setup' | 'capture'>('setup');
const selectedMode = ref<CameraMode | null>(null);

function handleStartCapture(mode: CameraMode): void {
  selectedMode.value = mode;
  page.value = 'capture';
}

function handleStopCapture(): void {
  page.value = 'setup';
  selectedMode.value = null;
}
</script>

<template>
  <SetupPage v-if="page === 'setup'" @start-capture="handleStartCapture" />
  <CapturePage
    v-else-if="selectedMode !== null"
    :mode="selectedMode"
    @stop="handleStopCapture"
  />
</template>
