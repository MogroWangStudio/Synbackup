<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { refreshAll, refreshConfig, refreshHistory, refreshPending, store, toast } from "./store";
import { replayBootAnim } from "./theme";
import { notifyError, t } from "./i18n";
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
  // 冷启动：首帧后才重播启动动画——挂载即播的动画在真实窗口首次
  // 绘制前就已放完，双 rAF 确保从"已绘制"的状态重启
  requestAnimationFrame(() => requestAnimationFrame(() => replayBootAnim(store.settings)));

  // 先挂监听再拉数据：首次拉取失败时也要保持实时更新能力，
  // 否则设备插入、扫描完成这些事件从此再也收不到。
  await registerListeners();

  try {
    await refreshAll();
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
  // 窗口显示（托盘重开 / Dock 点击 / 重建）：重播启动动画
  await listen("window-shown", () => replayBootAnim(store.settings));
  // 通知「取消任务」：待处理差异被移除，刷新
  await listen("pending-changed", () => {
    void refreshPending();
  });
  // 测试通知的点击反馈：点了按钮 / 点了通知本体（仅测试类别会发此事件）
  await listen<{ kind: string; action: string }>("notification-action", (e) => {
    if (e.payload.kind === "open") {
      toast("info", t("toast.notify_open"));
    } else if (e.payload.kind === "button") {
      toast("success", t("toast.notify_button", { action: e.payload.action }));
    }
  });
  // 通知投递失败（权限被拒 / 投递阶段出错）：提示用户，不再静默吞掉
  await listen<string>("notification-error", (e) => {
    toast("error", notifyError(e.payload));
  });
}

/* ---- 空白处点击关闭（设置 → 通用；never / single / double）----
   关闭动作走 win.close()，与标题栏关闭键同一条链路（后端拦截后询问
   退出方式）。空白 = 目标在窗口树内、且不在任何交互 / 内容元素内；
   标题栏拖拽区单独排除，双击最大化等原生语义不受影响。 */
function isBlankArea(e: MouseEvent): boolean {
  const el = e.target as HTMLElement | null;
  if (!el?.closest) return false;
  if (!el.closest(".window")) return false; // Teleport 弹层等不在窗口树内
  if (
    el.closest(
      "button, a, input, textarea, select, label, summary, .card, .toggle, .mws-cbx, .mws-pop, " +
        ".picker-anchor, .modal-overlay, .foot-line, .sidebar, .nav-item, .swatches, .seg, " +
        ".toast-host, .dev-section, .studio-link, [data-tauri-drag-region]",
    )
  ) {
    return false;
  }
  return true;
}

function fireWindowClose() {
  void getCurrentWindow().close().catch(() => {});
}

function onWinClick(e: MouseEvent) {
  if (store.settings.blankClick === "single" && isBlankArea(e)) fireWindowClose();
}

function onWinDblClick(e: MouseEvent) {
  if (store.settings.blankClick === "double" && isBlankArea(e)) fireWindowClose();
}

document.addEventListener("click", onWinClick);
document.addEventListener("dblclick", onWinDblClick);
onBeforeUnmount(() => {
  document.removeEventListener("click", onWinClick);
  document.removeEventListener("dblclick", onWinDblClick);
});
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
html[data-boot-anim="blur"] .window {
  animation: boot-blur var(--boot-ms, 340ms) var(--boot-ease, var(--ease)) both;
}
html[data-boot-anim="pop"] .window {
  animation: boot-pop var(--boot-ms, 340ms) var(--boot-ease, var(--ease)) both;
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
/* 虚化聚焦：材质从磨砂里显影（blur → 清晰），与弹窗遮罩的 blur 同语言 */
@keyframes boot-blur {
  from {
    opacity: 0;
    filter: blur(14px);
    transform: scale(0.99);
  }
  to {
    opacity: 1;
    filter: blur(0);
    transform: none;
  }
}
/* 弹出：快速缩放登场，60% 处轻微过冲再落定（动量手感） */
@keyframes boot-pop {
  0% {
    opacity: 0;
    transform: scale(0.92);
  }
  60% {
    opacity: 1;
    transform: scale(1.015);
  }
  100% {
    opacity: 1;
    transform: none;
  }
}
@media (prefers-reduced-motion: reduce) {
  html[data-boot-anim="fade"] .window,
  html[data-boot-anim="rise"] .window,
  html[data-boot-anim="zoom"] .window,
  html[data-boot-anim="slide"] .window,
  html[data-boot-anim="blur"] .window,
  html[data-boot-anim="pop"] .window {
    animation: none;
  }
}
.content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}
</style>
