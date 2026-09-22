<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { Check, Clipboard, FileText, FolderOpen, Globe, Network, Printer, Zap } from "lucide-vue-next";

const emit = defineEmits<{
	(e: "complete"): void;
}>();

const notice = ref<HTMLElement | null>(null);
const hasRead = ref(false);
const acknowledged = ref(false);

function updateReadState() {
	const element = notice.value;
	if (!element) return;
	hasRead.value = element.scrollTop + element.clientHeight >= element.scrollHeight - 8;
}

function complete() {
	if (hasRead.value && acknowledged.value) emit("complete");
}

onMounted(async () => {
	document.body.style.overflow = "hidden";
	await nextTick();
	updateReadState();
});

onBeforeUnmount(() => {
	document.body.style.overflow = "";
});
</script>

<template>
	<div class="onboarding-backdrop">
		<section class="onboarding-dialog" role="dialog" aria-modal="true" aria-labelledby="onboarding-title" aria-describedby="onboarding-summary">
			<header class="onboarding-header">
				<div class="onboarding-mark">
					<img src="/ix-print-logo.svg" alt="" />
				</div>
				<div>
					<p class="eyebrow">Before you start</p>
					<h1 id="onboarding-title">Welcome to ix.print</h1>
				</div>
			</header>

			<div ref="notice" class="onboarding-notice" tabindex="0" @scroll="updateReadState">
				<p id="onboarding-summary" class="onboarding-lead">
					ix.print is a local desktop print bridge. It connects web apps to printers on this computer through a local API,
					and can optionally forward a print job to another ix.print computer that you add.
				</p>

				<div class="onboarding-section">
					<h2>What ix.print uses</h2>
					<div class="onboarding-list">
						<div class="onboarding-item">
							<Printer :size="17" />
							<div><strong>System printers</strong><span>Reads printer names, status, capabilities and queues; sends print jobs; and can manage printers when you request it.</span></div>
						</div>
						<div class="onboarding-item">
							<FolderOpen :size="17" />
							<div><strong>Local app data</strong><span>Stores presets, printer settings, token hashes, remote ix.print settings, print history and temporary job files in the application data folder.</span></div>
						</div>
						<div class="onboarding-item">
							<Network :size="17" />
							<div><strong>Local network</strong><span>Runs an HTTP bridge on port 18032 by default, detects this computer’s LAN address, and connects to remote ix.print addresses only when you configure them.</span></div>
						</div>
						<div class="onboarding-item">
							<FileText :size="17" />
							<div><strong>PDF and preset data</strong><span>Uses the bundled PDF engine locally to render designed labels and receipts. File import and export happen only when you choose a file.</span></div>
						</div>
						<div class="onboarding-item">
							<Globe :size="17" />
							<div><strong>Web app requests</strong><span>Allowed websites can call the local API. API tokens protect access when token protection is enabled.</span></div>
						</div>
						<div class="onboarding-item">
							<Clipboard :size="17" />
							<div><strong>Optional system actions</strong><span>Copy buttons use the clipboard, documentation can open in your browser, and “Run at login” only takes effect when you enable it.</span></div>
						</div>
					</div>
				</div>

				<div class="onboarding-section">
					<h2>What leaves this computer</h2>
					<p>
						A local web app may send a print job and API token to ix.print. If you configure another ix.print computer,
						its address, token and the selected print job/settings are sent to that computer. Callback requests go only to a callback URL supplied by the API client.
					</p>
					<p class="onboarding-safe"><Zap :size="16" /> No cloud account, analytics or background data collection is part of ix.print.</p>
				</div>

				<div class="onboarding-section">
					<h2>Your controls</h2>
					<p>
						You can turn the bridge off, require API tokens, restrict allowed websites, remove remote computers,
						change the server port, set history retention and disable autostart from ix.print.
					</p>
					<p class="onboarding-warning">
						Keep the bridge and API tokens protected. Anyone who can reach an unsecured API address may be able to read printer information or send print jobs.
					</p>
				</div>

				<p class="onboarding-source">
					Source transparency: ix.print is a local desktop application. Its bundled PDF engine and dependencies are used to perform printing locally;
					remote network access is limited to the local API and remote addresses you configure.
				</p>
			</div>

			<footer class="onboarding-footer">
				<label class="onboarding-ack">
					<input v-model="acknowledged" type="checkbox" :disabled="!hasRead" />
					<span>I have read and understand what ix.print accesses and sends.</span>
				</label>
				<p v-if="!hasRead" class="onboarding-hint">Scroll to the end to enable confirmation.</p>
				<button class="primary onboarding-submit" :disabled="!hasRead || !acknowledged" @click="complete">
					<Check :size="16" /> Continue to ix.print
				</button>
			</footer>
		</section>
	</div>
</template>

<style scoped>
.onboarding-backdrop {
	position: fixed;
	inset: 0;
	z-index: 100;
	display: grid;
	place-items: center;
	padding: 16px;
	background: rgb(11 12 14 / 0.72);
}

.onboarding-dialog {
	width: min(560px, 100%);
	max-height: min(760px, calc(100vh - 32px));
	display: flex;
	flex-direction: column;
	background: var(--surface);
	color: var(--text);
	border: 1px solid var(--line);
	border-radius: var(--r-lg);
	box-shadow: 0 24px 60px rgb(0 0 0 / 0.28);
	overflow: hidden;
}

.onboarding-header {
	display: flex;
	align-items: center;
	gap: 12px;
	padding: 20px 20px 16px;
	border-bottom: 1px solid var(--line);
}

.onboarding-mark {
	width: 56px;
	height: 56px;
	display: grid;
	place-items: center;
	flex: none;
	background: var(--obsidian);
	border: 1px solid var(--border-dark);
	border-radius: var(--r-md);
	overflow: hidden;
}

.onboarding-mark img {
	display: block;
	width: 100%;
	height: 100%;
	object-fit: contain;
}

.eyebrow {
	margin: 0 0 2px;
	font-family: var(--font-mono);
	font-size: 10px;
	letter-spacing: 0.08em;
	text-transform: uppercase;
	color: var(--muted);
}

h1,
h2,
p {
	margin-top: 0;
}

h1 {
	margin-bottom: 0;
	font-size: 21px;
	letter-spacing: -0.02em;
}

h2 {
	margin-bottom: 8px;
	font-size: 14px;
	letter-spacing: -0.01em;
}

.onboarding-notice {
	min-height: 0;
	overflow-y: auto;
	padding: 18px 20px 20px;
	overscroll-behavior: contain;
}

.onboarding-lead {
	margin-bottom: 18px;
	font-size: 15px;
	line-height: 1.55;
}

.onboarding-section {
	padding-top: 14px;
	margin-top: 14px;
	border-top: 1px solid var(--line);
}

.onboarding-section p {
	margin-bottom: 8px;
	font-size: 13px;
	line-height: 1.55;
	color: var(--muted);
}

.onboarding-list {
	display: grid;
	gap: 8px;
}

.onboarding-item {
	display: flex;
	align-items: flex-start;
	gap: 9px;
	padding: 9px 10px;
	background: var(--bg);
	border: 1px solid var(--line);
	border-radius: var(--r-md);
	color: var(--brand);
}

.onboarding-item strong,
.onboarding-item span {
	display: block;
}

.onboarding-item strong {
	font-size: 13px;
	color: var(--text);
}

.onboarding-item span {
	margin-top: 2px;
	font-size: 12px;
	line-height: 1.45;
	color: var(--muted);
}

.onboarding-safe,
.onboarding-warning {
	display: flex;
	align-items: flex-start;
	gap: 7px;
	padding: 9px 10px;
	border-radius: var(--r-md);
}

.onboarding-safe {
	color: var(--ok) !important;
	background: var(--ok-soft);
}

.onboarding-warning {
	color: var(--warn) !important;
	background: var(--brand-pale);
}

.onboarding-source {
	padding-top: 14px;
	margin: 14px 0 0;
	border-top: 1px solid var(--line);
	font-size: 11px !important;
	line-height: 1.5;
	color: var(--muted);
}

.onboarding-footer {
	display: grid;
	gap: 8px;
	padding: 14px 20px 18px;
	border-top: 1px solid var(--line);
	background: var(--surface);
}

.onboarding-ack {
	display: flex;
	align-items: flex-start;
	gap: 8px;
	font-size: 13px;
	cursor: pointer;
}

.onboarding-ack input {
	margin-top: 3px;
	flex: none;
}

.onboarding-hint {
	margin: 0;
	font-size: 11px;
	color: var(--muted);
}

.onboarding-submit {
	justify-content: center;
	width: 100%;
}

@media (max-width: 460px) {
	.onboarding-backdrop {
		padding: 0;
		place-items: stretch;
	}

	.onboarding-dialog {
		max-height: 100vh;
		border-radius: 0;
	}
}
</style>
