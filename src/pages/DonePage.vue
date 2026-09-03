<script setup lang="ts">
import { computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export interface SessionSummary {
  image_count: number;
  average_percent: number;
  mean_reprojection_error: number | null;
  camera_matrix: number[][];
  session_dir: string;
}

const props = defineProps<{
  summary: SessionSummary;
}>();

const emit = defineEmits<{
  close: [];
}>();

const matrixRows = computed(() => {
  const matrix = props.summary.camera_matrix ?? [];
  return [0, 1, 2].map((row) => {
    const cells = matrix[row] ?? [];
    return [0, 1, 2].map((col) => Number(cells[col] ?? 0));
  });
});

async function handleOpenFolder(): Promise<void> {
  if (!props.summary.session_dir) {
    return;
  }
  await invoke('open_session_folder', { path: props.summary.session_dir });
}

function handleClose(): void {
  emit('close');
}
</script>

<template>
  <main class="done">
    <header class="done__header">
      <h1>采集完成</h1>
      <p>{{ summary.session_dir }}</p>
    </header>

    <dl class="done__stats">
      <div>
        <dt>张数</dt>
        <dd>{{ summary.image_count }}</dd>
      </div>
      <div>
        <dt>平均分</dt>
        <dd>{{ summary.average_percent.toFixed(1) }}</dd>
      </div>
      <div>
        <dt>平均重投影</dt>
        <dd>
          {{
            summary.mean_reprojection_error === null
              ? 'n/a'
              : `${summary.mean_reprojection_error.toFixed(3)} px`
          }}
        </dd>
      </div>
    </dl>

    <section aria-label="内参矩阵">
      <h2>内参 3×3</h2>
      <table class="done__matrix">
        <tbody>
          <tr v-for="(row, index) in matrixRows" :key="index">
            <td v-for="(cell, col) in row" :key="col">{{ cell.toFixed(2) }}</td>
          </tr>
        </tbody>
      </table>
    </section>

    <div class="done__actions">
      <button type="button" class="btn btn--primary" aria-label="打开文件夹" @click="handleOpenFolder">
        打开文件夹
      </button>
      <button type="button" class="btn" aria-label="返回设置" @click="handleClose">返回</button>
    </div>
  </main>
</template>

<style scoped>
.done {
  font-family: system-ui, sans-serif;
  padding: 1.5rem 2rem 2.5rem;
  max-width: 720px;
  margin: 0 auto;
  color: #1a1a1a;
}

.done__header h1 {
  margin: 0 0 0.35rem;
  font-size: 1.4rem;
}

.done__header p {
  margin: 0;
  color: #555;
  word-break: break-all;
}

.done__stats {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 1rem;
  margin: 1.5rem 0;
}

.done__stats dt {
  color: #666;
  font-size: 0.9rem;
}

.done__stats dd {
  margin: 0.25rem 0 0;
  font-size: 1.4rem;
  font-weight: 600;
}

.done h2 {
  margin: 0 0 0.6rem;
  font-size: 1.05rem;
}

.done__matrix {
  border-collapse: collapse;
  font-variant-numeric: tabular-nums;
}

.done__matrix td {
  border: 1px solid #ddd;
  padding: 0.45rem 0.7rem;
  text-align: right;
}

.done__actions {
  display: flex;
  gap: 0.75rem;
  margin-top: 1.5rem;
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
