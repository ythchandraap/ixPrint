<script setup lang="ts">
import { ArrowDown, ArrowUp, ChevronDown, ChevronRight, ClipboardPaste, Copy, Folder, MoreHorizontal, Trash2 } from "lucide-vue-next";
import { computed, nextTick, ref, watch } from "vue";
import type { TemplateElement } from "@/shared/api";
import type { DesignNode } from "./useDesignLayers";

interface StructureRow {
  node: DesignNode;
  depth: number;
  label: string;
  group?: { id: string; name: string };
  hasChildren: boolean;
  sortIndex: number;
}
interface LayerRow { element: TemplateElement; index: number; label: string; groupPath: string }
const props = defineProps<{
  structureRows: StructureRow[];
  rows: LayerRow[];
  selected: DesignNode[];
  hasCopied: boolean;
  moveTargets: { id: string | null; label: string }[];
  canGroup: boolean;
  canUngroup: boolean;
}>();
const emit = defineEmits<{
  copy: [];
  paste: [];
  selectNode: [node: DesignNode];
  group: [];
  ungroup: [];
  move: [targetId: string | null];
  rename: [id: string, name: string];
  remove: [];
  reorder: [index: number, direction: -1 | 1];
}>();
const tab = ref<"structure" | "order">("structure");
const expanded = ref(new Set<string>());
const knownGroupIds = ref(new Set<string>());
const moveTarget = ref("canvas");
const moveDialogOpen = ref(false);
const actionsMenu = ref<HTMLDetailsElement | null>(null);
const editingGroup = ref<string | null>(null);
const nameDraft = ref("");
const selectedGroup = computed(() => {
  const node = props.selected.length === 1 && props.selected[0].kind === "group" ? props.selected[0] : null;
  return node ? props.structureRows.find((row) => row.node.kind === "group" && row.node.id === node.id) : undefined;
});
watch(() => props.structureRows.map((row) => row.group?.id).filter(Boolean), (ids) => {
  const current = new Set(ids.filter((id): id is string => !!id));
  const nextExpanded = new Set([...expanded.value].filter((id) => current.has(id)));
  for (const id of current) if (!knownGroupIds.value.has(id)) nextExpanded.add(id);
  knownGroupIds.value = current;
  expanded.value = nextExpanded;
}, { immediate: true });
watch(() => props.moveTargets.map((target) => target.id === null ? "canvas" : `group:${target.id}`), (ids) => {
  if (!ids.includes(moveTarget.value)) moveTarget.value = "canvas";
}, { immediate: true });
const visibleStructure = computed(() => {
  const visible: StructureRow[] = [];
  for (const row of props.structureRows) {
    const parentIds = props.structureRows.filter((item) => item.group && item.depth < row.depth).slice(-1).map((item) => item.group!.id);
    if (parentIds.every((id) => expanded.value.has(id))) visible.push(row);
  }
  return visible;
});
const isSelected = (node: DesignNode) => props.selected.some((item) => item.kind === node.kind && item.id === node.id);
const moveTargetId = computed(() => moveTarget.value === "canvas" ? null : moveTarget.value.slice("group:".length));
const canRename = computed(() => !!selectedGroup.value?.group);
function closeActions() {
  if (actionsMenu.value) actionsMenu.value.open = false;
}
function menuKeys(event: KeyboardEvent) {
  const menu = actionsMenu.value;
  if (!menu) return;
  const items = [...menu.querySelectorAll<HTMLButtonElement>("[role=menuitem]:not(:disabled)")];
  if (event.key === "Escape") {
    event.preventDefault();
    closeActions();
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
function toggleExpand(id: string) {
  const next = new Set(expanded.value);
  if (next.has(id)) next.delete(id); else next.add(id);
  expanded.value = next;
}
function beginRename() {
  const row = selectedGroup.value;
  if (!row?.group) return;
  tab.value = "structure";
  editingGroup.value = row.group.id;
  nameDraft.value = row.group.name;
  closeActions();
  void nextTick(() => document.querySelector<HTMLInputElement>(".group-name-input")?.focus());
}
function finishRename() {
  if (editingGroup.value) emit("rename", editingGroup.value, nameDraft.value);
  editingGroup.value = null;
}
function openMoveDialog() {
  moveDialogOpen.value = true;
  closeActions();
  void nextTick(() => document.querySelector<HTMLSelectElement>(".move-dialog select")?.focus());
}
function cancelMoveDialog() {
  moveDialogOpen.value = false;
  void nextTick(() => actionsMenu.value?.querySelector<HTMLElement>("summary")?.focus());
}
function confirmMove() {
  emit("move", moveTargetId.value);
  moveDialogOpen.value = false;
  void nextTick(() => actionsMenu.value?.querySelector<HTMLElement>("summary")?.focus());
}
function doAction(action: "group" | "ungroup" | "delete") {
  closeActions();
  if (action === "group") emit("group");
  else if (action === "ungroup") emit("ungroup");
  else emit("remove");
  void nextTick(() => actionsMenu.value?.querySelector<HTMLElement>("summary")?.focus());
}
</script>

<template>
  <section class="layers-panel" data-inspector-tab="layers" aria-label="Layers">
    <header class="layers-panel-heading">
      <h3>Layers · {{ rows.length }}</h3>
      <div class="row tight">
        <button type="button" class="icon" title="Copy selected objects or groups" aria-label="Copy selected objects or groups" :disabled="!selected.length" @click="emit('copy')"><Copy :size="15" /></button>
        <button type="button" class="icon" title="Paste into the focused level" aria-label="Paste into the focused level" :disabled="!hasCopied" @click="emit('paste')"><ClipboardPaste :size="15" /></button>
        <details ref="actionsMenu" class="layers-actions-menu" @keydown="menuKeys">
          <summary class="icon" aria-label="Actions" title="Actions"><MoreHorizontal :size="17" /></summary>
          <div class="layers-action-list" role="menu" aria-label="Layer actions">
            <button type="button" role="menuitem" :disabled="!canGroup" @click="doAction('group')">Group</button>
            <button type="button" role="menuitem" :disabled="!canUngroup" @click="doAction('ungroup')">Ungroup</button>
            <button type="button" role="menuitem" :disabled="!canRename" @click="beginRename">Rename</button>
            <button type="button" role="menuitem" :disabled="!selected.length" @click="openMoveDialog">Move to group…</button>
            <button type="button" role="menuitem" class="danger-text" :disabled="!selected.length" @click="doAction('delete')"><Trash2 :size="14" /> Delete</button>
          </div>
        </details>
      </div>
    </header>

    <div class="layers-view-tabs" role="tablist" aria-label="Layer view">
      <button type="button" role="tab" :aria-selected="tab === 'structure'" :class="{ on: tab === 'structure' }" @click="tab = 'structure'">Structure</button>
      <button type="button" role="tab" :aria-selected="tab === 'order'" :class="{ on: tab === 'order' }" @click="tab = 'order'">Order</button>
    </div>

    <div v-if="tab === 'structure'" class="layers-list structure-list" aria-label="Template structure">
      <div v-for="row in visibleStructure" :key="`${row.node.kind}:${row.node.id}`" class="structure-row" :style="{ paddingInlineStart: `${row.depth * 12}px` }">
        <button v-if="row.group" type="button" class="icon structure-toggle" :aria-label="`${expanded.has(row.group.id) ? 'Collapse' : 'Expand'} ${row.group.name}`" @click="toggleExpand(row.group.id)">
          <ChevronDown v-if="expanded.has(row.group.id)" :size="14" /><ChevronRight v-else :size="14" />
        </button>
        <span v-else class="structure-toggle-spacer" aria-hidden="true"></span>
        <button type="button" :class="['layer-select', { on: isSelected(row.node) }]" :aria-pressed="isSelected(row.node)" :title="row.label" @click="emit('selectNode', row.node)">
          <Folder v-if="row.node.kind === 'group'" :size="15" />
          <input v-if="editingGroup === row.group?.id" v-model="nameDraft" class="group-name-input" maxlength="60" aria-label="Group name" @click.stop @keydown.enter.prevent="finishRename" @keydown.esc.stop="editingGroup = null" @blur="finishRename" />
          <span v-else class="layer-label">{{ row.label }}</span>
        </button>
      </div>
      <p v-if="!visibleStructure.length" class="muted">No objects yet. Add one to start the design.</p>
    </div>

    <div v-else-if="rows.length" class="layers-list order-list" aria-label="Layer order">
      <div v-for="layer in rows" :key="layer.element.id" class="order-row">
        <button type="button" :class="['layer-select', { on: isSelected({ kind: 'element', id: layer.element.id }) }]" :aria-pressed="isSelected({ kind: 'element', id: layer.element.id })" :title="layer.label" @click="emit('selectNode', { kind: 'element', id: layer.element.id })">
          <span class="layer-index">{{ rows.length - layer.index }}</span>
          <span class="layer-content"><span class="layer-label">{{ layer.label }}</span><span class="layer-path">{{ layer.groupPath || 'Canvas' }}</span></span>
        </button>
        <div class="row tight layer-order-actions">
          <button type="button" class="icon" title="Move forward" :aria-label="`Move ${layer.label} forward`" :disabled="layer.index === rows.length - 1" @click="emit('reorder', layer.index, 1)"><ArrowUp :size="14" /></button>
          <button type="button" class="icon" title="Move backward" :aria-label="`Move ${layer.label} backward`" :disabled="layer.index === 0" @click="emit('reorder', layer.index, -1)"><ArrowDown :size="14" /></button>
        </div>
      </div>
    </div>
    <p v-else class="muted">No objects yet. Add one to start the design.</p>

    <p class="layers-help">Structure follows each group’s earliest object. Change print stacking in Order. Paste offsets by 2 mm.</p>

    <div v-if="moveDialogOpen" class="move-dialog" role="dialog" aria-label="Move to group" @keydown.esc.prevent="cancelMoveDialog">
      <h4>Move to group</h4>
      <label class="field"><span>Destination</span>
        <select v-model="moveTarget" aria-label="Destination group">
          <option v-for="target in moveTargets" :key="target.id === null ? 'canvas' : `group:${target.id}`" :value="target.id === null ? 'canvas' : `group:${target.id}`">{{ target.label }}</option>
        </select>
      </label>
      <div class="row end"><button type="button" class="ghost small" @click="cancelMoveDialog">Cancel</button><button type="button" class="primary small" @click="confirmMove">Move</button></div>
    </div>
  </section>
</template>
