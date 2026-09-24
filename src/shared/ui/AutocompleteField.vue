<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { ChevronDown } from "lucide-vue-next";

export interface AutocompleteOption {
  value: string;
  label: string;
  meta?: string;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: AutocompleteOption[];
    placeholder?: string;
    ariaLabel?: string;
    disabled?: boolean;
    limit?: number;
  }>(),
  { placeholder: "Choose…", ariaLabel: "Choose an option", disabled: false, limit: 8 },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
  (e: "search", value: string): void;
}>();
const query = ref("");
const open = ref(false);
const active = ref(-1);
const showAll = ref(false);
const input = ref<HTMLInputElement | null>(null);

function labelFor(value: string) {
  return props.options.find((option) => option.value === value)?.label ?? value;
}

const suggestions = computed(() => {
  const normalized = query.value.trim().toLowerCase();
  const options =
    showAll.value || !normalized
      ? props.options
      : props.options.filter(
          (option) =>
            option.label.toLowerCase().includes(normalized) || option.value.toLowerCase().includes(normalized),
        );
  return options.slice(0, props.limit);
});

watch(
  () => [props.modelValue, props.options] as const,
  () => {
    if (!open.value) query.value = labelFor(props.modelValue);
  },
  { immediate: true },
);

function focus() {
  open.value = true;
  showAll.value = true;
  active.value = -1;
}

function inputChanged(event: Event) {
  query.value = (event.target as HTMLInputElement).value;
  emit("search", query.value);
  showAll.value = false;
  open.value = true;
  active.value = -1;
}

function choose(option: AutocompleteOption) {
  emit("update:modelValue", option.value);
  query.value = option.label;
  open.value = false;
  showAll.value = false;
  active.value = -1;
}

function restore() {
  query.value = labelFor(props.modelValue);
  open.value = false;
  showAll.value = false;
  active.value = -1;
}

function blur() {
  // Let a suggestion button receive its click before the field closes.
  window.setTimeout(restore, 120);
}

function keydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    restore();
    return;
  }
  if (event.key === "ArrowDown") {
    event.preventDefault();
    open.value = true;
    showAll.value = false;
    active.value = suggestions.value.length ? (active.value + 1) % suggestions.value.length : -1;
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    open.value = true;
    showAll.value = false;
    active.value = suggestions.value.length
      ? active.value <= 0
        ? suggestions.value.length - 1
        : active.value - 1
      : -1;
  } else if (event.key === "Enter" && open.value && active.value >= 0) {
    event.preventDefault();
    choose(suggestions.value[active.value]);
  } else if (event.key === "Tab") {
    restore();
  }
}

async function openFromClick() {
  if (props.disabled) return;
  open.value = true;
  showAll.value = true;
  await nextTick();
  input.value?.focus();
}
</script>

<template>
  <div class="autocomplete-field">
    <input
      ref="input"
      :value="query"
      :placeholder="placeholder"
      :aria-label="ariaLabel"
      :aria-expanded="open"
      aria-autocomplete="list"
      :disabled="disabled"
      @focus="focus"
      @input="inputChanged"
      @keydown="keydown"
      @blur="blur"
    />
    <button
      type="button"
      class="autocomplete-toggle"
      :disabled="disabled"
      tabindex="-1"
      aria-label="Show options"
      @mousedown.prevent="openFromClick"
    >
      <ChevronDown :size="15" />
    </button>
    <div v-if="open && suggestions.length" class="autocomplete-menu" role="listbox">
      <button
        v-for="(option, i) in suggestions"
        :key="option.value"
        type="button"
        :class="['autocomplete-option', { active: active === i }]"
        :aria-selected="option.value === modelValue"
        @mousedown.prevent
        @click="choose(option)"
      >
        <span class="grow truncate">{{ option.label }}</span>
        <span v-if="option.meta" class="autocomplete-meta">{{ option.meta }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.autocomplete-field {
  position: relative;
  width: 100%;
}
.autocomplete-field > input {
  padding-right: 34px;
}
.autocomplete-toggle {
  position: absolute;
  right: 1px;
  top: 1px;
  bottom: 1px;
  width: 32px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: calc(var(--r-md) - 1px);
  background: transparent;
  color: var(--steel);
  cursor: pointer;
}
.autocomplete-toggle:hover:not(:disabled) {
  background: var(--brand-pale);
  color: var(--brand);
}
.autocomplete-toggle:disabled {
  cursor: default;
}
.autocomplete-menu {
  position: absolute;
  left: 0;
  right: 0;
  top: calc(100% + 4px);
  z-index: 20;
  max-height: 220px;
  overflow-y: auto;
  padding: 4px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  box-shadow: 0 8px 20px rgb(0 0 0 / 0.16);
}
.autocomplete-option {
  width: 100%;
  min-height: 32px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border: none;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}
.autocomplete-option:hover,
.autocomplete-option.active {
  background: var(--brand-pale);
}
.autocomplete-meta {
  font-family: var(--font-mono);
  font-size: 10px;
  letter-spacing: 0.04em;
  color: var(--brand);
}
</style>
