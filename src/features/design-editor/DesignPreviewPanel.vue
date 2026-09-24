<script setup lang="ts">
import { ref } from "vue";
import { api, errorText, type Template } from "@/shared/api";
type ListLayout = Extract<NonNullable<Template["layout"]>, { mode: "list" }>;
const props = defineProps<{
	template: Template;
	list: ListLayout | null;
	fields: string[];
	autoFields: string[];
	autoHint: Record<string, string>;
	exampleBody: string;
	localIp: string;
	serverPort: number;
}>();
const previewRows = ref(12);
const printPages = ref<string[]>([]);
const printTotal = ref(0);
const printError = ref("");
const printBusy = ref(false);
async function showPrintPreview() {
	printBusy.value = true;
	printError.value = "";
	try {
		const rows = props.list ? previewRows.value : Math.min(previewRows.value, 6);
		const [pages, total] = await api.previewPrint(JSON.parse(JSON.stringify(props.template)), rows, 3);
		[printPages.value, printTotal.value] = [pages, total];
	} catch (error) {
		printError.value = errorText(error);
	} finally {
		printBusy.value = false;
	}
}
</script>
<template>
	<fieldset>
		<legend>Sample data</legend>
		<p class="hint">Enter example values to see dynamic text in the design preview and test print. New fields start with “Contoh”.</p>
		<p v-if="!fields.length" class="hint">No fields yet. Write {{ "{" + "{field}" + "}" }} in text, barcode or QR content.</p>
		<template v-if="autoFields.length">
			<p class="hint">Automatic fields — leave blank for ix.print to fill, or enter your own value (the web app may also send it):</p>
			<div class="grid2">
				<label v-for="field in autoFields" :key="field" class="field">
					<span>{{ field }}</span>
					<input v-model="template.sample[field]" :placeholder="`automatic: ${autoHint[field]}`" />
				</label>
			</div>
		</template>
		<p v-else class="hint">
			Available automatic fields: <code>{{ "{" + "{no}" + "}" }}</code>, <code>{{ "{" + "{halaman}" + "}" }}</code>,
			<code>{{ "{" + "{total_halaman}" + "}" }}</code>, <code>{{ "{" + "{jumlah_data}" + "}" }}</code> — their values can also be provided manually.
		</p>
		<div class="grid2">
			<label v-for="field in fields" :key="field" class="field">
				<span>{{ field }}</span>
				<input v-model="template.sample[field]" placeholder="e.g. Product name" />
			</label>
		</div>
	</fieldset>

	<fieldset>
		<legend>Call from a web app</legend>
		<p class="hint">Send a JSON body (plus the token header when required):</p>
		<pre class="settings">POST http://{{ localIp }}:{{ serverPort }}/print
Content-Type: application/json
{{ exampleBody }}</pre>
		<p class="hint">
			<code>"data"</code> is an array: one object = {{ list ? "one row" : "one label" }}.
			<code>"header"</code> (optional) contains values shared by every page{{ list ? " — fields in the Header/Footer bands" : "" }}.
			Other settings go in <code>"settings"</code>, e.g. <code>{"copies": 2}</code>.
		</p>
	</fieldset>

	<fieldset>
		<legend>Print preview</legend>
		<div class="row">
			<label class="row tight grow">
				<input v-model.number="previewRows" class="rows-input" type="number" min="1" max="500" />
				<span class="meta">sample items</span>
			</label>
			<button type="button" class="ghost small" :disabled="printBusy" @click="showPrintPreview">
				{{ printBusy ? "Rendering…" : "Show" }}
			</button>
		</div>
		<p v-if="printError" class="error">{{ printError }}</p>
		<template v-if="printPages.length">
			<p class="meta">{{ printTotal }} page{{ printTotal === 1 ? "" : "s" }}{{ printTotal > printPages.length ? ` (${printPages.length} first shown)` : "" }}</p>
			<div class="pages">
				<figure v-for="(src, index) in printPages" :key="index">
					<img :src="src" alt="" />
					<figcaption class="meta">p. {{ index + 1 }}</figcaption>
				</figure>
			</div>
		</template>
	</fieldset>
</template>
