<script setup lang="ts">
import { onBeforeUnmount, onMounted, Ref, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Command } from '@tauri-apps/plugin-shell';
// import { exists } from "@tauri-apps/api";
import { resolveResource } from "@tauri-apps/api/path";
import { exists } from "@tauri-apps/plugin-fs";
import { Icon } from "@iconify/vue";
import { Copy, Printer } from 'lucide-vue-next';


const local_ip = ref("");
const name = ref("");
const os = ref("");
const selectedPrinter = ref("");
let intervalId: number | null = null;
let intervalIp: number | null = null;
let intervalOs: number | null = null;
const printers = ref<string[]>([]);

const ip = ref('Some text to copy');


async function localIp() {
	local_ip.value = await invoke("this_local_ip");
}

async function localOs() {
	os.value = await invoke("os_detected");
}

async function getPrinter() {
	let result = await Command.create('exec-pwsh', ['-c', 'Get-Printer']).execute();
	const testt = result?.stdout || '';
	const printerLines = testt.trim().split('\n').filter(line =>
		line.trim() !== '' &&
		!line.startsWith('Name') &&
		!line.startsWith('----')
	);
	return printerLines.map(line => line.trim().split(/\s{2,}/)[0]);
}


function updatePrinters(existing: Ref<string[]>, next: string[]) {
	const current = existing.value;

	const toAdd = next.filter(p => !current.includes(p));
	
	const toRemove = current.filter(p => !next.includes(p));

	existing.value = current
		.filter(p => !toRemove.includes(p))
		.concat(toAdd); 
}


async function pollPrinters() {
	const newList = await getPrinter();
	updatePrinters(printers, newList);
}

onMounted(async () => {
	intervalId = setInterval(pollPrinters, 1000);
	intervalIp = setInterval(localIp, 1000);
	intervalOs = setInterval(localOs, 1000);

	await localOs
	await pollPrinters
	await localIp
});

onBeforeUnmount(() => {
	if (intervalId) clearInterval(intervalId);
	if (intervalIp) clearInterval(intervalIp);
	if (intervalOs) clearInterval(intervalOs);
});

function copyToClipboard(string:string) {
  navigator.clipboard.writeText(string ?? local_ip.value)
    // .then(() => alert('Copied!'))
    // .catch(err => console.error('Copy failed', err));
}
	
async function runPrint() {
	const bundledExe = await resolveResource("spdf.exe");
	const bundledPdf = await resolveResource("RSUD.pdf");

	let exePath = bundledExe;
	let pdfPath = bundledPdf;

	let copies = 1
	let orientation = 'landscape'

	// Development fallback
	if (!(await exists(bundledExe))) {
		exePath = "src/assets/spdf.exe"; // Adjust as needed
		pdfPath = "src/assets/RSUD.pdf";
	}

	if (os.value=='windows') {
		const result = await Command.create("exec-pwsh", [
			"-c",
			`Start-Process -FilePath '${exePath}' ` +
			`-ArgumentList '-print-to "${selectedPrinter.value}" -print-settings "${copies}x,${orientation}" "${pdfPath}"' -Wait`
		]).execute();
	}
}

function changePrinter(event) {
	event.preventDefault()
	selectedPrinter.value = event?.target?.parentElement?.parentElement?.id
}
</script>

<template>
	<div class="wrapper">
		<div class="apps">
			<header class="flex justify-between flex-column">
				<div class="flex flex-column">
					<h1 class="text-blue-900 text-left m-0"><b>fxPrint</b></h1>
					<p class="text-blue-500 text-medium mb-0 mt-4">bridging your web apps to printer</p>
				</div>
				<div class="flex flex-column bg-blue-500 pt-6 relative rounded-tl-4">
					<h3 class="text-blue-100 m-0 text-medium p-4">
						<b> 
							{{ local_ip }}
						</b>
						<p class="text-blue-100 m-0 i-pt-0  text-sm">
							({{ os }})
						</p>
					</h3>
					<div @click="copyToClipboard(ip.value)" class="absolute flex justify-center items-center  bg-white -mt-10 rounded-full w-10 ml-4 aspect-square">
						<Copy
							class="text-blue-500 cursor-pointer"
							:size="20"
						/>
					</div>
					<div @click="runPrint" class="absolute flex justify-center items-center  bg-white -mt-10 rounded-full w-10 ml-16 aspect-square">
						<Printer
							class="text-blue-500 cursor-pointer"
							:size="20"
						/>
					</div>
				</div>
			</header>
			<div class="p-4 flex flex-column">
				<h3 class="text-blue-900 m-0 text-medium">Printer List</h3>
				<div class="flex flex-column mt-2 gap-2">
					<div class="flex items-center justify-between gap-2" v-for="printer in printers" :key="printer" :id="printer">
						<div class="flex items-center gap-2" >
							<input type="radio" name="printer"  @change="changePrinter" >
							<span class="truncate" id="printer">
								{{ printer }}
							</span>
						</div>
							<div @click="copyToClipboard(printer)" class=" flex justify-center items-center  bg-white  rounded-full p-2  aspect-square gap-2">
								<Copy
									class="text-blue-500 cursor-pointer"
									:size="16"
								/>
							</div>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>

<style >
		@font-face {
		font-family: 'commissioner';
		src: url('src\assets\fonts\commissioner\commissioner-variable.ttf') format('truetype');
		}
	body {
			font-family: commissioner ,Arial, sans-serif;
			background-color: #f9f9f9;
			color: #333;
			margin: 0;
			padding: 0;
		}
	.truncate{	
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.relative{
		position: relative;
	}

	.text-sm{
		font-size: small;
	}
	.absolute{
		position: absolute;
	}
	.w-10{
	width:  40px;
	}

	.h-10{
	height: 40px;
	}

	.bg-white{
		background-color: white;
	}

	.cursor-pointer{
		cursor: pointer;
	}
	.wrapper{
		display: flex;
		flex-direction: column;
		width: 100%;
		min-height: 100vh;
		align-items: center;
		justify-content: center;
	}
	.apps{
		width: 100%;
		min-height: 100vh;
		max-width: 360px;
		background-color: whitesmoke;
	}
	.flex-column{
		flex-direction: column;
	}
	.flex{
		display: flex;
	}
	.justify-center{
		justify-content: center;
	}
	.justify-between{
		justify-content: space-between;
	}
	.items-center{
		align-items: center;
	}
	.gap-2{
		gap: .5rem;
	}
	.bg-blue-500{
		background-color : oklch(62.3% 0.214 259.815)
	}

	.text-blue-100{
		color :  oklch(93.2% 0.032 255.585)
	}
	.text-blue-500{
		color : oklch(62.3% 0.214 259.815)
	}
	.text-blue-900{
		color:  oklch(37.9% 0.146 265.522);
	}
	.text-left{
		text-align: start;
	}
	.text-medium{
		font-weight: 500;
	}
	.i-pt-0{
		padding-top : 0  !important;
	}
	.pt-0{
		padding-top : 0;
	}
	.p-2{
		padding: .5rem;
	}
	.p-3{
		padding: .75rem;
	}
	.p-4{
		padding: 1rem;
	}
	.pt-6{
		padding-top: 1.5rem; /* 24px */
	}
	.pt-10{
		padding-top: 40px;
	}
	.-mt-10{
		margin-top:  -40px;
	}
	.rounded-full{
		border-radius: 9999px;
	}
	.rounded-tl-4{
		border-radius:  1rem 0 0 0 ;
	}
	.m-0{
		margin:  0;
	}
	.mt-2{
		margin-top: .5rem;
	}
	.mt-4{
		margin-top: 1rem;
	}
	.ml-4{
		margin-left: 1rem;
	}
	.ml-10{
		margin-left: 40px;
	}
	.ml-16{
		margin-left: 4rem;
	}
	.mb-0{
		margin-bottom: 0;
	}
	header{
		
		padding: 4rem 0rem 0 1rem;
		min-height: 192px;
		background-color:  oklch(88.2% 0.059 254.128)
	}
	.logo.vite:hover {
		filter: drop-shadow(0 0 2em #747bff);
	}

.logo.vue:hover {
	filter: drop-shadow(0 0 2em #249b73);
}

:root {
	font-family: commisioner, Avenir, Helvetica, Arial, sans-serif;
	font-size: 16px;
	line-height: 24px;
	font-weight: 400;

	color: #0f0f0f;
	background-color: #f6f6f6;

	font-synthesis: none;
	text-rendering: optimizeLegibility;
	-webkit-font-smoothing: antialiased;
	-moz-osx-font-smoothing: grayscale;
	-webkit-text-size-adjust: 100%;
}

.container {
	margin: 0;
	padding-top: 10vh;
	display: flex;
	flex-direction: column;
	justify-content: center;
	text-align: center;
}

.logo {
	height: 6em;
	padding: 1.5em;
	will-change: filter;
	transition: 0.75s;
}

.logo.tauri:hover {
	filter: drop-shadow(0 0 2em #24c8db);
}

.row {
	display: flex;
	justify-content: center;
}

a {
	font-weight: 500;
	color: #646cff;
	text-decoration: inherit;
}

a:hover {
	color: #535bf2;
}

h1 {
	text-align: center;
}

input,
button {
	border-radius: 8px;
	border: 1px solid transparent;
	padding: 0.6em 1.2em;
	font-size: 1em;
	font-weight: 500;
	font-family: inherit;
	color: #0f0f0f;
	background-color: #ffffff;
	transition: border-color 0.25s;
	box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
}

.aspect-square{
	aspect-ratio: 1 / 1;
}
button {
	cursor: pointer;
}

button:hover {
	border-color: #396cd8;
}
button:active {
	border-color: #396cd8;
	background-color: #e8e8e8;
}

input,
button {
	outline: none;
}

#greet-input {
	margin-right: 5px;
}

@media (prefers-color-scheme: dark) {
	:root {
		color: #f6f6f6;
		background-color: #2f2f2f;
	}

	a:hover {
		color: #24c8db;
	}

	input,
	button {
		color: #ffffff;
		background-color: #0f0f0f98;
	}
	button:active {
		background-color: #0f0f0f69;
	}
}

</style>