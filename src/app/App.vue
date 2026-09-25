<script setup lang="ts">
import { ArrowLeft, MonitorX } from "lucide-vue-next";
import AppHeader from "./AppHeader.vue";
import { useAppShell } from "./useAppShell";
import "./app.css";
import PrintersTab from "@/features/printers/PrintersTab.vue";
import PresetsTab from "@/features/presets/PresetsTab.vue";
import DesignEditor from "@/features/design-editor/DesignEditor.vue";
import HistoryTab from "@/features/history/HistoryTab.vue";
import PrintTab from "@/features/print/PrintTab.vue";
import ApiTab from "@/features/api-tokens/ApiTab.vue";
import RemotesPanel from "@/features/remotes/RemotesPanel.vue";
import ErrorLogPanel from "@/features/diagnostics/ErrorLogPanel.vue";
import ApiAccessLogPanel from "@/features/diagnostics/ApiAccessLogPanel.vue";
import SettingsView from "@/features/settings/SettingsView.vue";
import EmptyState from "@/shared/ui/EmptyState.vue";
import OnboardingDialog from "@/features/settings/OnboardingDialog.vue";

const {
  PRODUCT,
  BRAND,
  inApp,
  version,
  serverState,
  serverError,
  bridgeEnabled,
  bridgeChanging,
  ipState,
  configError,
  os,
  engineError,
  printers,
  config,
  selectedPrinter,
  tab,
  printPageOpen,
  requestedPreset,
  tabLabel,
  navigationStarted,
  settingsOpen,
  settingsMenu,
  diagnosticsLog,
  settingsTitle,
  appearance,
  onboardingOpen,
  toast,
  refreshing,
  printersLoaded,
  notify,
  changeAppearance,
  completeOnboarding,
  refreshPrinters,
  openDocs,
  osLabel,
  bridgeHost,
  address,
  copy,
  toggleDefaultPrinter,
  testPages,
  setTestPages,
  openPrintPage,
  toggleSettings,
  selectSettingsMenu,
  selectDiagnosticsLog,
  backFromSettings,
  openDiagnostics,
  closePrintPage,
  selectTab,
  reload,
  setConfig,
  toggleBridge,
} = useAppShell();
</script>

<template>
  <div class="app">
    <div class="top">
      <AppHeader
        :product="PRODUCT"
        :in-app="inApp"
        :print-page-open="printPageOpen"
        :settings-open="settingsOpen"
        :bridge-enabled="bridgeEnabled"
        :bridge-changing="bridgeChanging"
        :server-state="serverState"
        :ip-state="ipState"
        :config="config"
        :address="address"
        @open-print="openPrintPage()"
        @toggle-settings="toggleSettings()"
        @toggle-bridge="toggleBridge"
        @copy="copy"
      />
      <p v-if="serverState === 'error'" class="banner">
        {{ serverError }}. Close any other application using the configured port, then restart {{ PRODUCT }}.
      </p>

      <p v-if="engineError" class="banner">PDF engine unavailable: {{ engineError }}</p>
      <nav v-if="settingsOpen" class="tabs settings-bar">
        <button
          class="icon"
          :title="settingsMenu ? 'Back to Settings' : 'Back to app'"
          :aria-label="settingsMenu ? 'Back to Settings' : 'Back to app'"
          @click="backFromSettings"
        ><ArrowLeft :size="18" /></button>
        <strong>{{ settingsTitle }}</strong>
      </nav>
      <nav v-else class="tabs">
        <button :class="{ on: !printPageOpen && tab === 'printers' }" @click="selectTab('printers')">Printers</button>
        <button :class="{ on: !printPageOpen && tab === 'presets' }" @click="selectTab('presets')">Presets</button>
        <button :class="{ on: !printPageOpen && tab === 'history' }" @click="selectTab('history')">History</button>
      </nav>
    </div>

    <div class="page-transition-host">
      <Transition name="page" :css="navigationStarted">
        <main v-if="!inApp" key="browser-page">
          <EmptyState
            :icon="MonitorX"
            title="Open this in the ix.print app"
            text="This is the ix.print desktop app and cannot be used directly in a browser. To try the REST API from a browser, open http://<computer-ip>:<server-port>/docs."
          />
        </main>
        <main v-else-if="settingsOpen" key="settings-page" class="settings-main">
          <SettingsView
            :config="config"
            :config-error="configError"
            :local-ip="bridgeHost"
            :os="os"
            :printers="printers"
            :bridge-ready="serverState === 'ok'"
            :appearance="appearance"
            :active-menu="settingsMenu"
            :diagnostics-log="diagnosticsLog"
            @config="setConfig"
            @notify="notify"
            @appearance-change="changeAppearance"
            @menu-change="selectSettingsMenu"
            @diagnostics-log-change="selectDiagnosticsLog"
            @copy="copy"
            @docs="openDocs"
            @printers-changed="refreshPrinters"
          >
            <template #diagnostics="{ showSystemLogs, activeLog }">
              <ErrorLogPanel v-if="activeLog === 'error-log'" :show-system-logs="showSystemLogs" @copy="copy" @notify="notify" />
              <ApiAccessLogPanel v-else @copy="copy" @notify="notify" />
            </template>
            <template #api="{ config: settingsConfig, localIp: settingsLocalIp }">
              <ApiTab
                :config="settingsConfig"
                :local-ip="settingsLocalIp"
                @config="setConfig"
                @notify="notify"
                @copy="copy"
              />
            </template>
            <template #remotes>
              <RemotesPanel @notify="notify" @printers-changed="refreshPrinters" />
            </template>
          </SettingsView>
        </main>
        <main v-else-if="configError" key="config-error-page">
          <EmptyState :icon="MonitorX" tone="warn" title="Settings could not be loaded" :text="configError">
            <button class="primary small" @click="openDiagnostics">Open Diagnostics</button>
            <button class="ghost small" @click="reload">Try again</button>
          </EmptyState>
        </main>
        <main v-else-if="!config" key="loading-page">
          <p class="muted center-text">Loading…</p>
        </main>
        <main v-else :key="printPageOpen ? 'test-print-page' : `tab-${tab}`">
          <PrintTab
            v-if="printPageOpen"
            :config="config"
            :printers="printers"
            :printers-loaded="printersLoaded"
            :selected-printer="selectedPrinter"
            :test-pages="testPages"
            :requested-preset="requestedPreset"
            :return-label="tabLabel"
            @back="closePrintPage"
            @presets="selectTab('presets')"
            @printers="selectTab('printers')"
            @select-printer="selectedPrinter = $event"
            @test-pages="setTestPages"
          />
          <PrintersTab
            v-else-if="tab === 'printers'"
            :printers="printers"
            :config="config"
            :selected="selectedPrinter"
            :refreshing="refreshing"
            :loaded="printersLoaded"
            :os="os"
            @refresh="refreshPrinters(true)"
            @reload="refreshPrinters()"
            @select="selectedPrinter = $event"
            @toggle-default="toggleDefaultPrinter"
            @copy="copy"
            @notify="notify"
            @config="setConfig"
          />
          <PresetsTab
            v-else-if="tab === 'presets'"
            :config="config"
            :printers="printers"
            :test-pages="testPages"
            :local-ip="bridgeHost"
            :server-port="config.server_port"
            @config="setConfig"
            @notify="notify"
            @open-print="openPrintPage"
          >
            <template #design-editor="{ presetName, template, localIp: editorLocalIp, serverPort, apply, cancel }">
              <DesignEditor
                :preset-name="presetName"
                :template="template"
                :local-ip="editorLocalIp"
                :server-port="serverPort"
                @apply="apply"
                @cancel="cancel"
              />
            </template>
          </PresetsTab>
          <HistoryTab v-else :config="config" @config="setConfig" @notify="notify" />
        </main>
      </Transition>
    </div>

    <footer class="colophon">
      <span>{{ PRODUCT }}{{ version ? ` ${version}` : "" }}{{ osLabel ? ` · ${osLabel}` : "" }}</span>
      <span class="grow"></span>
      <span>IX Systems / {{ BRAND }}</span>
    </footer>

    <OnboardingDialog v-if="onboardingOpen" @complete="completeOnboarding" />

    <div v-if="toast" :class="['toast', { error: toast.error }]" role="status" @click="toast = null">
      {{ toast.text }}
    </div>
  </div>
</template>
