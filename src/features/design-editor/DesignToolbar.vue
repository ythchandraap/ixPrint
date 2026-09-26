<script setup lang="ts">
import { Barcode, ChevronDown, HelpCircle, Image as ImageIcon, Minus, PanelRight, QrCode, Redo2, Square, Type, Undo2 } from "lucide-vue-next";
import { nextTick, ref } from "vue";

type ElementType = Exclude<import("@/shared/api").TemplateElement["type"], "image">;
defineProps<{
  pageLabel: string;
  list: boolean;
  canUndo: boolean;
  canRedo: boolean;
  showGrid: boolean;
  showRulers: boolean;
  showBands: boolean;
  showObjectBounds: boolean;
  snapEnabled: boolean;
  zoom: string;
  inspectorOpen: boolean;
}>();
const addMenu = ref<HTMLDetailsElement | null>(null);
const emit = defineEmits<{
  add: [type: ElementType | "image"];
  undo: [];
  redo: [];
  "open-page": [];
  "update:showGrid": [value: boolean];
  "update:showRulers": [value: boolean];
  "update:showBands": [value: boolean];
  "update:showObjectBounds": [value: boolean];
  "update:snapEnabled": [value: boolean];
  "update:zoom": [value: string];
  "toggle-inspector": [];
  help: [];
}>();
function checkbox(event: Event, key: "showGrid" | "showRulers" | "showBands" | "showObjectBounds") {
  const value = (event.target as HTMLInputElement).checked;
  if (key === "showGrid") emit("update:showGrid", value);
  else if (key === "showRulers") emit("update:showRulers", value);
  else if (key === "showBands") emit("update:showBands", value);
  else emit("update:showObjectBounds", value);
}
const addItems: { type: ElementType | "image"; label: string; icon: typeof Type }[] = [
  { type: "text", label: "Text", icon: Type }, { type: "line", label: "Line", icon: Minus },
  { type: "rect", label: "Rectangle", icon: Square }, { type: "barcode", label: "Barcode", icon: Barcode },
  { type: "qr", label: "QR", icon: QrCode }, { type: "image", label: "Image", icon: ImageIcon },
];
function addItem(type: ElementType | "image") {
  if (addMenu.value) addMenu.value.open = false;
  emit("add", type);
  if (type !== "image") void nextTick(() => addMenu.value?.querySelector<HTMLElement>("summary")?.focus());
}
function menuKeys(event: KeyboardEvent) {
  const menu = addMenu.value;
  if (!menu) return;
  const items = [...menu.querySelectorAll<HTMLButtonElement>("[role=menuitem]")];
  if (event.key === "Escape") {
    event.preventDefault();
    menu.open = false;
    menu.querySelector<HTMLElement>("summary")?.focus();
    return;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    if (!menu.open) menu.open = true;
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === "ArrowDown" ? (current + 1) % items.length : (current <= 0 ? items.length - 1 : current - 1);
    items[next]?.focus();
  }
}
</script>

<template>
  <div class="design-toolbar" aria-label="Canvas tools">
    <div class="toolbar-primary">
      <details ref="addMenu" class="toolbar-menu add-menu" @keydown="menuKeys">
        <summary class="ghost"><span><strong>+</strong> Add</span><ChevronDown :size="14" aria-hidden="true" /></summary>
        <div class="toolbar-popover" role="menu" aria-label="Add object">
          <button v-for="item in addItems" :key="item.type" type="button" role="menuitem" @click="addItem(item.type)">
            <component :is="item.icon" :size="16" aria-hidden="true" /> {{ item.label }}
          </button>
        </div>
      </details>
      <button type="button" class="icon" title="Undo" aria-label="Undo" :disabled="!canUndo" @click="emit('undo')"><Undo2 :size="17" /></button>
      <button type="button" class="icon" title="Redo" aria-label="Redo" :disabled="!canRedo" @click="emit('redo')"><Redo2 :size="17" /></button>
      <button type="button" class="ghost small page-summary" @click="emit('open-page')">{{ pageLabel }}</button>
      <button type="button" class="icon inspector-toggle" :aria-pressed="inspectorOpen" aria-label="Inspector" title="Inspector" @click="emit('toggle-inspector')"><PanelRight :size="16" /></button>
    </div>
    <div class="toolbar-secondary">
      <details class="toolbar-menu guides-menu">
        <summary class="ghost small"><span>Guides</span><ChevronDown :size="14" aria-hidden="true" /></summary>
        <div class="toolbar-popover guide-options" aria-label="Canvas guides">
          <label><input type="checkbox" :checked="showGrid" @change="checkbox($event, 'showGrid')" /> Grid</label>
          <label><input type="checkbox" :checked="showRulers" @change="checkbox($event, 'showRulers')" /> Rulers</label>
          <label v-if="list"><input type="checkbox" :checked="showBands" @change="checkbox($event, 'showBands')" /> Bands</label>
          <label><input type="checkbox" :checked="showObjectBounds" @change="checkbox($event, 'showObjectBounds')" /> Object bounds</label>
        </div>
      </details>
      <label class="snap-control"><input type="checkbox" :checked="snapEnabled" @change="emit('update:snapEnabled', ($event.target as HTMLInputElement).checked)" /> Snap</label>
      <label class="zoom-control"><span class="sr-only">Zoom</span>
        <select :value="zoom" aria-label="Canvas zoom" @change="emit('update:zoom', ($event.target as HTMLSelectElement).value)">
          <option value="fit">Fit</option>
          <option v-for="level in [50, 75, 100, 125, 150, 200]" :key="level" :value="String(level)">{{ level }}%</option>
        </select>
      </label>
      <button type="button" class="icon help-button" title="Keyboard and group help" aria-label="Help" @click="emit('help')"><HelpCircle :size="16" /></button>
    </div>
  </div>
</template>
