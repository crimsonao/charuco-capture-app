<script setup lang="ts">
import { ref } from 'vue';
import CapturePage, { type SessionSummary } from './pages/CapturePage.vue';
import DonePage from './pages/DonePage.vue';
import SetupPage from './pages/SetupPage.vue';
import StartPage from './pages/StartPage.vue';

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
  out_root: string;
}

const page = ref<'start' | 'setup' | 'capture' | 'done'>('start');
const outputDir = ref('');
const selectedMode = ref<CameraMode | null>(null);
const sessionParams = ref<SessionParams | null>(null);
const doneSummary = ref<SessionSummary | null>(null);

function handleBegin(dir: string): void {
  outputDir.value = dir.trim();
  selectedMode.value = null;
  sessionParams.value = null;
  doneSummary.value = null;
  page.value = 'setup';
}

function handleBackToStart(): void {
  selectedMode.value = null;
  sessionParams.value = null;
  page.value = 'start';
}

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
  <StartPage v-if="page === 'start'" :initial-dir="outputDir" @begin="handleBegin" />
  <SetupPage
    v-else-if="page === 'setup'"
    :output-dir="outputDir"
    @back="handleBackToStart"
    @start-capture="handleStartCapture"
  />
  <CapturePage
    v-else-if="page === 'capture' && selectedMode !== null && sessionParams !== null"
    :mode="selectedMode"
    :session="sessionParams"
    @stop="handleStopCapture"
    @done="handleSessionDone"
  />
  <DonePage v-else-if="page === 'done' && doneSummary !== null" :summary="doneSummary" @close="handleCloseDone" />
</template>
