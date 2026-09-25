<script setup lang="ts">
import type { Component } from "vue";

defineProps<{
  icon: Component;
  title: string;
  text?: string;
  /** Red accent for problems (no network, failed jobs) instead of neutral. */
  tone?: "neutral" | "warn";
}>();
</script>

<template>
  <div :class="['empty', tone ?? 'neutral']">
    <div class="empty-icon"><component :is="icon" :size="22" /></div>
    <strong>{{ title }}</strong>
    <p v-if="text" class="hint">{{ text }}</p>
    <div v-if="$slots.default" class="row wrap empty-actions"><slot /></div>
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 6px;
  padding: 16px 12px;
  border: 1px dashed var(--line);
  border-radius: var(--r-lg);
  background: var(--surface);
}
.empty-icon {
  width: 28px;
  height: 28px;
  border-radius: var(--r-sm);
  display: grid;
  place-items: center;
  color: var(--brand);
}
.warn .empty-icon {
  color: var(--danger);
}
.empty .hint {
  max-width: 300px;
}
.empty-actions {
  justify-content: center;
  margin-top: 6px;
}
</style>
