<script setup lang="ts">
import { onMounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import { refreshAll, refreshConfig, refreshHistory, refreshPending, store, toast } from "./store";
import { t } from "./i18n";
import type { PendingReview, VolumeInfo } from "./types";
import Sidebar from "./components/Sidebar.vue";
import OverviewView from "./views/OverviewView.vue";
import PairsView from "./views/PairsView.vue";
import DevicesView from "./views/DevicesView.vue";
import HistoryView from "./views/HistoryView.vue";
import SettingsView from "./views/SettingsView.vue";
import PairEditor from "./components/PairEditor.vue";
import DiffDialog from "./components/DiffDialog.vue";
import ToastHost from "./components/ToastHost.vue";

onMounted(async () => {
  try {
    await refreshAll();
  } catch (e) {
    toast("error", String(e));
    return;
  }
  applyLang();

  await listen<VolumeInfo[]>("devices-changed", (e) => {
    store.volumes = e.payload;
  });
  await listen<PendingReview>("review-available", async (e) => {
    await Promise.all([refreshPending(), refreshConfig()]);
    toast("info", t("toast.review_available", { name: e.payload.pairName, n: e.payload.items.length }));
  });
  await listen("sync-finished", async () => {
    await Promise.all([refreshPending(), refreshHistory(), refreshConfig()]);
  });
});

function applyLang() {
  document.documentElement.lang = store.settings.language === "en-US" ? "en" : "zh-CN";
}
</script>

<template>
  <div class="app-shell">
    <Sidebar />
    <main class="content">
      <Transition name="view-fade" mode="out-in">
        <OverviewView v-if="store.view === 'overview'" />
        <PairsView v-else-if="store.view === 'pairs'" />
        <DevicesView v-else-if="store.view === 'devices'" />
        <HistoryView v-else-if="store.view === 'history'" />
        <SettingsView v-else />
      </Transition>
    </main>

    <PairEditor v-if="store.editor" />
    <DiffDialog v-if="store.diffDialog" />
    <ToastHost />
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  height: 100%;
}
.content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}
</style>
