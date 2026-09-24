<script setup lang="ts">
import { Copy, Printer, Settings } from "lucide-vue-next";
import type { Config } from "@/shared/api";

defineProps<{
  product: string;
  inApp: boolean;
  printPageOpen: boolean;
  settingsOpen: boolean;
  bridgeEnabled: boolean;
  bridgeChanging: boolean;
  serverState: "loading" | "ok" | "error" | "off";
  ipState: "loading" | "ok" | "none";
  config: Config | null;
  address: string;
}>();
const emit = defineEmits<{
  "open-print": [];
  "toggle-settings": [];
  "toggle-bridge": [];
  copy: [text: string];
}>();
</script>

<template>
  <header class="appbar">
    <div class="bar">
      <span class="wordmark" :aria-label="product">{{ product }}</span>
      <span class="grow"></span>
      <button
        :class="['bar-btn', { on: printPageOpen }]"
        :aria-pressed="printPageOpen"
        :disabled="!inApp"
        title="Open test print options"
        @click="emit('open-print')"
      >
        <Printer :size="14" /> Test print
      </button>
      <button
        :class="['bar-icon', { on: settingsOpen }]"
        :aria-pressed="settingsOpen"
        aria-label="Settings: API & documentation"
        title="Settings: API & documentation"
        :disabled="!inApp"
        @click="emit('toggle-settings')"
      >
        <Settings :size="16" />
      </button>
    </div>

    <div class="statusline">
      <template v-if="!inApp">
        <span class="status"><i class="dot idle"></i>NOT IN APP</span>
      </template>
      <template v-else>
        <button
          class="bridge-toggle"
          :aria-pressed="bridgeEnabled"
          :disabled="bridgeChanging"
          :title="bridgeEnabled ? 'Disable the bridge for web apps' : 'Enable the bridge for web apps'"
          @click="emit('toggle-bridge')"
        >
          <i
            :class="[
              'dot',
              !bridgeEnabled ? 'idle' : serverState === 'ok' ? 'on' : serverState === 'error' ? 'fail' : 'idle',
            ]"
          ></i>
          {{
            !bridgeEnabled
              ? "BRIDGE OFF"
              : serverState === "ok"
                ? "BRIDGE ON"
                : serverState === "error"
                  ? "BRIDGE FAILED"
                  : "STARTING"
          }}
        </button>
        <span class="sep" aria-hidden="true">/</span>
        <span v-if="ipState === 'loading' && config?.lan_access !== false" class="skeleton">LOADING ADDRESS</span>
        <button
          v-else
          class="addr"
          :title="
            config?.lan_access === false
              ? 'Copy the local bridge address'
              : ipState === 'ok'
                ? 'Copy the address for a web app'
                : 'Not connected to a network: only accessible from this computer'
          "
          @click="emit('copy', address)"
        >
          {{ address }}<Copy :size="11" />
        </button>
        <span v-if="ipState === 'none' && config?.lan_access !== false" class="warn-note">OFFLINE</span>
        <template v-if="config">
          <span class="sep" aria-hidden="true">/</span>
          <span :class="{ 'warn-note': !config.require_token }">
            {{ config.require_token ? "TOKEN REQUIRED" : "TOKEN OPTIONAL" }}
          </span>
        </template>
      </template>
    </div>
  </header>
</template>
