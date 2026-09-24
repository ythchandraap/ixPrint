<script setup lang="ts">
import { ArrowDown, ArrowUp, ClipboardPaste, Copy } from "lucide-vue-next";
import type { TemplateElement } from "@/shared/api";
defineProps<{
	rows: { element: TemplateElement; index: number; label: string }[];
	selected: number | null;
	elementCount: number;
	hasCopied: boolean;
}>();
const emit = defineEmits<{
	copy: []; paste: []; select: [index: number]; reorder: [index: number, direction: -1 | 1];
}>();
</script>
<template>
<fieldset class="layers-panel">
			<legend>Layers · {{ elementCount }}</legend>
			<div class="row wrap layers-toolbar">
				<span class="muted grow">Top layer appears first. Select a row to edit it.</span>
				<button type="button" class="ghost small" :disabled="selected === null" title="Copy selected element (⌘/Ctrl+C)" @click="emit('copy')">
					<Copy :size="14" /> Copy
				</button>
				<button type="button" class="ghost small" :disabled="!hasCopied" title="Paste element (⌘/Ctrl+V)" @click="emit('paste')">
					<ClipboardPaste :size="14" /> Paste
				</button>
			</div>
			<div v-if="rows.length" class="layers-list">
				<div v-for="layer in rows" :key="layer.element.id" class="layer-row">
					<button
						type="button"
						:class="['layer-select', { on: selected === layer.index }]"
						:aria-pressed="selected === layer.index"
						:title="layer.label"
						@click="emit('select', layer.index)"
					>
						<span class="layer-index">{{ elementCount - layer.index }}</span>
						<span class="layer-label">{{ layer.label }}</span>
					</button>
					<div class="row tight layer-order-actions">
						<button type="button" class="icon" title="Move forward" :aria-label="`Move ${layer.label} forward`" :disabled="layer.index === elementCount - 1" @click="emit('reorder', layer.index, 1)">
							<ArrowUp :size="14" />
						</button>
						<button type="button" class="icon" title="Move backward" :aria-label="`Move ${layer.label} backward`" :disabled="layer.index === 0" @click="emit('reorder', layer.index, -1)">
							<ArrowDown :size="14" />
						</button>
					</div>
				</div>
			</div>
			<p v-else class="muted">No elements yet. Add an element above to start the design.</p>
			<p class="meta">Copy and paste work inside this design. Pasted copies are offset slightly.</p>
		</fieldset>
</template>
