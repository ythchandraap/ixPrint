<script setup lang="ts">
import { ref } from "vue";
import { Copy, Trash2 } from "lucide-vue-next";
import type { TemplateElement } from "@/shared/api";
import AutocompleteField from "@/shared/ui/AutocompleteField.vue";
defineProps<{ current: TemplateElement | null; selected: number | null; fields: string[] }>();
const emit = defineEmits<{
	"insert-field": [field: string]; "replace-image": [index: number | null]; duplicate: []; remove: [];
}>();
const newField = ref("");
const alignOptions = [{ value: "left", label: "Left" }, { value: "center", label: "Centre" }, { value: "right", label: "Right" }];
const valignOptions = [{ value: "top", label: "Top" }, { value: "middle", label: "Centre" }, { value: "bottom", label: "Bottom" }];
const fitOptions = [{ value: "contain", label: "Fit (keep ratio)" }, { value: "stretch", label: "Fill box" }];
const typeLabel: Record<TemplateElement["type"], string> = { text: "Text", line: "Line", rect: "Rectangle", barcode: "Barcode", qr: "QR", image: "Image" };
function insertField(field: string) {
	emit("insert-field", field);
}
function addField() {
	const field = newField.value.trim().replace(/[^A-Za-z0-9_.-]/g, "_");
	if (!field) return;
	emit("insert-field", field);
	newField.value = "";
}
function useImageField(element: TemplateElement, field: string) {
	if (element.type !== "image") return;
	const normalized = field.trim().replace(/[^A-Za-z0-9_.-]/g, "_");
	element.src = normalized ? `{{${normalized}}}` : "";
}
</script>
<template>

		<fieldset v-if="current">
			<legend>{{ typeLabel[current.type] }}</legend>
			<template v-if="current.type === 'text'">
				<label class="field">
					<span>Content (use {{ "{" + "{field}" + "}" }} for data)</span>
					<textarea v-model="current.text" rows="2"></textarea>
				</label>
				<div class="grid4">
					<label class="field"><span>Ukuran (pt)</span><input v-model.number="current.size" type="number" min="2" max="300" step="any" /></label>
					<label class="field"><span>Alignment</span>
						<AutocompleteField v-model="current.align" :options="alignOptions" aria-label="Text alignment" />
					</label>
					<label class="field"><span>Vertical</span>
						<AutocompleteField v-model="current.valign" :options="valignOptions" aria-label="Vertical alignment" />
					</label>
					<label class="field"><span>&nbsp;</span>
						<span class="row tight"><input v-model="current.bold" type="checkbox" /> Bold</span>
					</label>
				</div>
				<div class="row wrap">
					<label class="row tight check"><input v-model="current.wrap" type="checkbox" /> Wrap text</label>
					<label class="row tight check"><input v-model="current.fit" type="checkbox" /> Shrink to fit</label>
				</div>
			</template>
			<template v-if="current.type === 'barcode' || current.type === 'qr'">
				<label class="field">
					<span>Value ({{ current.type === "barcode" ? "Code128, ASCII" : "text/URL" }})</span>
					<input v-model="current.value" />
				</label>
				<label v-if="current.type === 'barcode'" class="row tight check">
					<input v-model="current.show_text" type="checkbox" /> Show text below barcode
				</label>
			</template>
			<template v-if="current.type === 'image'">
				<div class="row wrap">
					<img v-if="current.src.startsWith('data:')" :src="current.src" class="image-thumb" alt="" />
					<span v-else class="meta">From data field: <code>{{ current.src || "—" }}</code></span>
					<button type="button" class="ghost small" @click="emit('replace-image', selected)">Replace image…</button>
				</div>
				<div class="grid2">
					<label class="field"><span>Placement</span>
						<AutocompleteField v-model="current.fit" :options="fitOptions" aria-label="Image placement" />
					</label>
					<label class="field"><span>Data field (optional)</span>
						<input
							:value="current.src.startsWith('{{') ? current.src.replace(/[{}]/g, '').trim() : ''"
							placeholder="e.g. photo — image per item"
							@change="useImageField(current, ($event.target as HTMLInputElement).value)"
						/>
					</label>
				</div>
				<label class="row tight check"><input v-model="current.grayscale" type="checkbox" /> Black and white (clearer on thermal printers)</label>
				<p class="hint">
					Any format this computer can open (PNG, JPG, SVG, WebP, GIF, BMP, AVIF…). From a data field, the web app sends
					the image as a data URL/base64 (PNG, JPEG, GIF, BMP, WebP, TIFF, ICO).
				</p>
			</template>
			<template v-if="current.type === 'rect'">
				<div class="row wrap">
					<label class="field"><span>Border (pt, 0 = none)</span><input v-model.number="current.stroke" type="number" min="0" step="any" /></label>
					<label class="row tight check"><input v-model="current.fill" type="checkbox" /> Black fill</label>
				</div>
			</template>

			<div v-if="current.type === 'line'" class="grid4">
				<label class="field"><span>X1</span><input v-model.number="current.x1" type="number" step="any" /></label>
				<label class="field"><span>Y1</span><input v-model.number="current.y1" type="number" step="any" /></label>
				<label class="field"><span>X2</span><input v-model.number="current.x2" type="number" step="any" /></label>
				<label class="field"><span>Y2</span><input v-model.number="current.y2" type="number" step="any" /></label>
				<label class="field"><span>Width (pt)</span><input v-model.number="current.width" type="number" min="0.1" step="any" /></label>
			</div>
			<div v-else-if="current.type === 'qr'" class="grid4">
				<label class="field"><span>X</span><input v-model.number="current.x" type="number" step="any" /></label>
				<label class="field"><span>Y</span><input v-model.number="current.y" type="number" step="any" /></label>
				<label class="field"><span>Ukuran</span><input v-model.number="current.size" type="number" min="2" step="any" /></label>
			</div>
			<div v-else class="grid4">
				<label class="field"><span>X</span><input v-model.number="current.x" type="number" step="any" /></label>
				<label class="field"><span>Y</span><input v-model.number="current.y" type="number" step="any" /></label>
				<label class="field"><span>Width</span><input v-model.number="current.w" type="number" min="1" step="any" /></label>
				<label class="field"><span>Height</span><input v-model.number="current.h" type="number" min="1" step="any" /></label>
			</div>

			<div v-if="current.type === 'text' || current.type === 'barcode' || current.type === 'qr'" class="row wrap">
				<span class="meta">Insert field:</span>
				<button v-for="f in fields" :key="f" type="button" class="ghost small" @click="insertField(f)">{{ f }}</button>
				<input v-model="newField" class="field-input" placeholder="new field" @keydown.enter.prevent="addField" />
				<button type="button" class="ghost small" @click="addField">+</button>
			</div>

			<div class="row wrap">
				<button type="button" class="icon" title="Duplicate" @click="emit('duplicate')"><Copy :size="16" /></button>
				<span class="grow"></span>
				<button type="button" class="icon danger" title="Remove element" @click="emit('remove')"><Trash2 :size="16" /></button>
			</div>
		</fieldset>
		<p v-else class="hint">Click an element in the preview to edit it.</p>
</template>
