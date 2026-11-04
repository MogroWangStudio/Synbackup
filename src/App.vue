<script setup lang="ts">
import { onMounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
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
import CloseConfirm from "./components/CloseConfirm.vue";
import ToastHost from "./components/ToastHost.vue";
import TitleBar from "./components/TitleBar.vue";

onMounted(async () => {
  // 先挂监听再拉数据：首次拉取失败时也要保持实时更新能力，
  // 否则设备插入、扫描完成这些事件从此再也收不到。
  await registerListeners();

  try {
    await refreshAll();
    applyLang();
    // 前端就绪：此后后端收到关闭请求会询问退出方式
    await api.markUiReady();
  } catch (e) {
    toast("error", String(e));
  }
});

async function registerListeners() {
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
  // 关闭窗口：后端拦截并转交前端询问退出方式
  await listen("close-requested", () => {
    store.closeConfirm = true;
  });
}

function applyLang() {
  document.documentElement.lang = store.settings.language === "en-US" ? "en" : "zh-CN";
}
</script>

<template>
  <div class="window">
    <TitleBar />

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
    </div>

    <PairEditor v-if="store.editor" />
    <DiffDialog v-if="store.diffDialog" />
    <CloseConfirm v-if="store.closeConfirm" />
    <ToastHost />
  </div>
</template>

<style scoped>
.window {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.app-shell {
  display: flex;
  flex: 1;
  min-height: 0;
  /* 界面缩放只作用于侧栏与内容区；标题栏保持系统 chrome 尺寸，
     与系统红绿灯的对位关系不受缩放影响 */
  zoom: var(--ui-scale, 1);
}

/* 启动入场：预设 / 时长 / 曲线均可设置（theme.ts 写入 data-boot-anim 与
   --boot-ms / --boot-ease，首帧前已就位）。reduced-motion 下不播。 */
html[data-boot-anim="fade"] .window {
  animation: boot-fade var(--boot-ms, 340ms) var(--boot-ease, var(--ease)) both;
}
html[data-boot-anim="rise"] .window {
  animation: boot-rise var(--boot-ms, 340ms) var(--boot-ease, var(--ease)) both;
}
html[data-boot-anim="zoom"] .window {
  animation: boot-zoom var(--boot-ms, 340ms) var(--boot-ease, var(--ease)) both;
}
html[data-boot-anim="slide"] .window {
  animation: boot-slide var(--boot-ms, 340ms) var(--boot-ease, var(--ease)) both;
}
@keyframes boot-fade {
  from { opacity: 0; }
}
@keyframes boot-rise {
  from {
    opacity: 0;
    transform: translateY(10px) scale(0.99);
  }
}
@keyframes boot-zoom {
  from {
    opacity: 0;
    transform: scale(0.94);
  }
}
@keyframes boot-slide {
  from {
    opacity: 0;
    transform: translateX(-14px);
  }
}
@media (prefers-reduced-motion: reduce) {
  html[data-boot-anim="fade"] .window,
  html[data-boot-anim="rise"] .window,
  html[data-boot-anim="zoom"] .window,
  html[data-boot-anim="slide"] .window {
    animation: none;
  }
}
.content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}
</style>
