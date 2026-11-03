<script setup lang="ts">
// 自绘标题栏：整条即拖拽区，窗口按钮按平台分工。
// macOS 保留系统红绿灯（透明叠加标题栏），只负责让出左侧空间；
// Windows / Linux 无边框窗口，窗口按钮由这里绘制。
import { onMounted, onUnmounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";
import Icon from "./Icon.vue";
import Logo from "./Logo.vue";

const isMac = document.documentElement.dataset.os === "mac";
const win = getCurrentWindow();
const maximized = ref(false);
let stopResize: (() => void) | undefined;
let syncTimer: number | undefined;

// 权限缺失或窗口已销毁时静默失败，不让标题栏抛错打断交互
function fire(action: () => Promise<unknown>) {
  void action().catch(() => {});
}

async function syncMaximized() {
  try {
    maximized.value = await win.isMaximized();
  } catch {
    /* 忽略：仅图标状态不准确 */
  }
}

onMounted(async () => {
  if (isMac) return;
  await syncMaximized();
  try {
    // 拖拽改尺寸时事件密集，合并成一次查询即可
    stopResize = await win.onResized(() => {
      window.clearTimeout(syncTimer);
      syncTimer = window.setTimeout(syncMaximized, 120);
    });
  } catch {
    /* 忽略：图标退回默认的“最大化”形态 */
  }
});

onUnmounted(() => {
  window.clearTimeout(syncTimer);
  stopResize?.();
});
</script>

<template>
  <header class="titlebar" data-tauri-drag-region>
    <div class="identity" data-tauri-drag-region>
      <span class="mark" data-tauri-drag-region><Logo variant="mark" :size="18" /></span>
      <span class="name" data-tauri-drag-region>同备</span>
      <span class="sub" data-tauri-drag-region>Synbackup</span>
    </div>

    <div v-if="!isMac" class="controls">
      <button
        type="button"
        class="ctl"
        :title="t('win.minimize')"
        :aria-label="t('win.minimize')"
        @click="fire(() => win.minimize())"
      >
        <Icon name="win-min" :size="16" />
      </button>
      <button
        type="button"
        class="ctl"
        :title="maximized ? t('win.restore') : t('win.maximize')"
        :aria-label="maximized ? t('win.restore') : t('win.maximize')"
        @click="fire(() => win.toggleMaximize())"
      >
        <Icon :name="maximized ? 'win-restore' : 'win-max'" :size="16" />
      </button>
      <button
        type="button"
        class="ctl ctl-close"
        :title="t('win.close')"
        :aria-label="t('win.close')"
        @click="fire(() => win.close())"
      >
        <Icon name="win-close" :size="16" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  display: flex;
  flex: none;
  align-items: center;
  justify-content: space-between;
  height: var(--titlebar-h);
  padding-left: var(--titlebar-inset);
  /* 与侧栏同一层材质：结构层，不是内容层 */
  background: var(--bg-sidebar);
  /* 弹窗遮罩（z-index 100）之上：关窗与拖动始终可用，与原生标题栏一致 */
  position: relative;
  z-index: 150;
  user-select: none;
}

.identity {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding-right: 14px;
}
.mark {
  display: flex;
  flex: none;
}
.name {
  font-size: 12.5px;
  font-weight: 600;
  letter-spacing: -0.01em;
  line-height: 1;
}
.sub {
  font-size: 11px;
  line-height: 1;
  letter-spacing: 0.02em;
  color: var(--text-3);
}

.controls {
  display: flex;
  height: 100%;
}
.ctl {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 46px;
  height: 100%;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--text-2);
  cursor: default;
  transition:
    background-color 110ms ease,
    color 110ms ease;
}
.ctl:hover {
  background: var(--fill-soft);
  color: var(--text);
}
.ctl:active {
  background: var(--bg-active);
  transition-duration: 40ms;
}
.ctl-close:hover {
  background: var(--win-close);
  color: #fff;
}
.ctl-close:active {
  background: var(--win-close-active);
  color: #fff;
}
.ctl:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -3px;
}
</style>
