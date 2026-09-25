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
        aria-label="Settings"
        title="Settings"
        :disabled="!inApp"
        @click="emit('toggle-settings')"
      >
        <Settings :size="16" />
      </button>
    </div>

    <div class="statusline">
      <template v-if="!inApp">
        <span class="status status-not-in-app"><i class="dot idle"></i>Not in app</span>
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
              !bridgeEnabled ? 'idle' : serverState === 'ok' ? 'ok' : serverState === 'error' ? 'fail' : 'idle',
            ]"
          ></i>
          <span>{{
            !bridgeEnabled
              ? "BRIDGE OFF"
              : serverState === "ok"
                ? "BRIDGE ON"
                : serverState === "error"
                  ? "BRIDGE FAILED"
                  : "STARTING"
          }}</span>
        </button>
        <div class="address-status">
          <span v-if="ipState === 'loading' && config?.lan_access !== false" class="address-loading skeleton">Loading address…</span>
          <button
            v-else
            class="addr"
            :aria-label="`Copy bridge address: ${address}`"
            :title="
              config?.lan_access === false
                ? 'Copy the local bridge address'
                : ipState === 'ok'
                  ? 'Copy the address for a web app'
                  : 'Not connected to a network: only accessible from this computer'
            "
            @click="emit('copy', address)"
          >
            <span class="addr-value">{{ address }}</span><Copy :size="12" aria-hidden="true" />
          </button>
          <span v-if="ipState === 'none' && config?.lan_access !== false" class="warn-note">Offline</span>
        </div>
        <span v-if="config" :class="['token-status', { 'warn-note': !config.require_token }]">
          {{ config.require_token ? "Token required" : "Token optional" }}
        </span>
      </template>
    </div>
  </header>
</template>
