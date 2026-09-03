<script setup lang="ts">
import { ref } from 'vue';
import CapturePage, { type SessionSummary } from './pages/CapturePage.vue';
import DonePage from './pages/DonePage.vue';
import SetupPage from './pages/SetupPage.vue';

interface CameraMode {
  device_name: string;
  dshow_index: number;
  width: number;
  height: number;
  fourcc: string;
}

interface SessionParams {
  count_target: number;
  score_target: number;
  square_mm: number;
  marker_mm: number;
}

const page = ref<'setup' | 'capture' | 'done'>('setup');
const selectedMode = ref<CameraMode | null>(null);
const sessionParams = ref<SessionParams | null>(null);
const doneSummary = ref<SessionSummary | null>(null);

function handleStartCapture(payload: { mode: CameraMode; session: SessionParams }): void {
  selectedMode.value = payload.mode;
  sessionParams.value = payload.session;
  page.value = 'capture';
}

function handleStopCapture(): void {
  page.value = 'setup';
  selectedMode.value = null;
  sessionParams.value = null;
}

function handleSessionDone(summary: SessionSummary): void {
  doneSummary.value = summary;
  page.value = 'done';
}

function handleCloseDone(): void {
  page.value = 'setup';
  selectedMode.value = null;
  sessionParams.value = null;
  doneSummary.value = null;
}
</script>

<template>
  <SetupPage v-if="page === 'setup'" @start-capture="handleStartCapture" />
  <CapturePage
    v-else-if="page === 'capture' && selectedMode !== null && sessionParams !== null"
    :mode="selectedMode"
    :session="sessionParams"
    @stop="handleStopCapture"
    @done="handleSessionDone"
  />
  <DonePage v-else-if="page === 'done' && doneSummary !== null" :summary="doneSummary" @close="handleCloseDone" />
</template>
