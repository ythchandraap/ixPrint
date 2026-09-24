<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Info, X } from "lucide-vue-next";

const INFO_OPEN_EVENT = "ixprint:setting-info-open";

defineProps<{ title: string }>();
const open = ref(false);
const trigger = ref<HTMLButtonElement | null>(null);
const popoverStyle = ref<Record<string, string>>({});

function positionPopover() {
  const button = trigger.value;
  if (!button) return;
  const rect = button.getBoundingClientRect();
  const headerBottom = document.querySelector<HTMLElement>(".top")?.getBoundingClientRect().bottom ?? 0;
  const width = Math.min(360, window.innerWidth - 48);
  const left = Math.min(Math.max(24, rect.right - width), window.innerWidth - width - 24);
  const safeTop = headerBottom + 8;
  const preferredTop = rect.bottom + 8;
  const top = Math.max(safeTop, preferredTop);
  const maxHeight = Math.max(160, window.innerHeight - top - 24);
  popoverStyle.value = {
    top: `${top}px`,
    left: `${left}px`,
    maxHeight: `${maxHeight}px`,
  };
}

function startTracking() {
  document.addEventListener("scroll", positionPopover, true);
  window.addEventListener("resize", positionPopover);
}

function stopTracking() {
  document.removeEventListener("scroll", positionPopover, true);
  window.removeEventListener("resize", positionPopover);
}

function hideWhenAnotherOpens() {
  if (open.value) open.value = false;
}

function toggle() {
  if (!open.value) window.dispatchEvent(new Event(INFO_OPEN_EVENT));
  open.value = !open.value;
}

watch(open, async (value) => {
  if (!value) {
    stopTracking();
    return;
  }
  startTracking();
  await nextTick();
  positionPopover();
});

onMounted(() => window.addEventListener(INFO_OPEN_EVENT, hideWhenAnotherOpens));
onBeforeUnmount(stopTracking);
onBeforeUnmount(() => window.removeEventListener(INFO_OPEN_EVENT, hideWhenAnotherOpens));
</script>

<template>
  <span class="setting-info">
    <button
      type="button"
      class="setting-info-button"
      ref="trigger"
      :aria-label="`About ${title}`"
      :aria-expanded="open"
      aria-haspopup="dialog"
      :title="`About ${title}`"
      @click.stop="toggle"
      @keydown.esc="open = false"
    >
      <Info :size="15" />
    </button>
    <Teleport to="body">
      <span
        v-if="open"
        class="setting-info-popover"
        :style="popoverStyle"
        role="dialog"
        :aria-label="title"
        @click.stop
      >
        <span class="setting-info-title">
          <strong>{{ title }}</strong>
          <button
            type="button"
            class="setting-info-close"
            :aria-label="`Close ${title} information`"
            @click="open = false"
          >
            <X :size="14" />
          </button>
        </span>
        <span class="setting-info-content"><slot /></span>
      </span>
    </Teleport>
  </span>
</template>

<style scoped>
.setting-info {
  position: relative;
  display: inline-flex;
  flex: none;
  vertical-align: middle;
}

.setting-info-button,
.setting-info-close {
  display: grid;
  place-items: center;
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--muted);
  cursor: pointer;
}

.setting-info-button {
  width: 25px;
  height: 25px;
  padding: 0;
  border-radius: 50%;
}

.setting-info-button:hover,
.setting-info-button[aria-expanded="true"] {
  border-color: var(--accent);
  color: var(--brand);
  background: var(--brand-pale);
}

.setting-info-close {
  width: 25px;
  height: 25px;
  padding: 0;
  border: 0;
  border-radius: var(--r-sm);
}

.setting-info-close:hover {
  color: var(--text);
  background: var(--brand-pale);
}

.setting-info-popover {
  position: fixed;
  z-index: 30;
  width: min(360px, calc(100vw - 48px));
  overflow-y: auto;
  display: block;
  padding: 12px;
  background: var(--surface);
  color: var(--text);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  box-shadow: 0 10px 26px rgb(0 0 0 / 0.18);
  font-size: 12px;
  font-weight: 400;
  line-height: 1.5;
  text-align: left;
}

.setting-info-title {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
  font-size: 13px;
}

.setting-info-title strong {
  flex: 1;
}

.setting-info-content {
  display: block;
  color: var(--muted);
}

.setting-info-content :deep(p) {
  margin: 0 0 8px;
}

.setting-info-content :deep(p:last-child) {
  margin-bottom: 0;
}

.setting-info-content :deep(ol) {
  margin: 6px 0 8px;
  padding-left: 20px;
}

.setting-info-content :deep(li + li) {
  margin-top: 4px;
}

.setting-info-content :deep(code) {
  font-size: 11px;
}

.setting-info-content :deep(.info-note) {
  padding: 7px 8px;
  background: var(--brand-pale);
  border-radius: var(--r-sm);
  color: var(--brand-ink);
}
</style>
