<script setup lang="ts">
// 自绘标题栏：整条即拖拽区，窗口按钮按平台分工。
// macOS 保留系统红绿灯（透明叠加标题栏），只负责让出左侧空间；
// Windows / Linux 无边框窗口，窗口按钮由这里绘制。
// 左上角 logo 不在拖拽区内：单击有按压反馈，连按五下触发随机彩蛋。
import { onMounted, onUnmounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";
import { toast } from "../store";
import Icon from "./Icon.vue";
import Logo from "./Logo.vue";

const isMac = document.documentElement.dataset.os === "mac";
const win = getCurrentWindow();
const maximized = ref(false);
let stopResize: (() => void) | undefined;
let syncTimer: number | undefined;

// ---- logo 彩蛋 ----
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const eggClass = ref("");
let clickStreak = 0;
let streakTimer: number | undefined;
let eggTimer: number | undefined;

function onLogoClick() {
  clickStreak++;
  window.clearTimeout(streakTimer);
  streakTimer = window.setTimeout(() => (clickStreak = 0), 2000);
  if (clickStreak < 5) return;
  clickStreak = 0;
  triggerEgg();
}

function triggerEgg() {
  const roll = Math.floor(Math.random() * 4);
  // 文案彩蛋 / 动效偏好下的降级：只弹一条俏皮 toast
  if (roll === 3 || reducedMotion) {
    toast("info", t(`easter.${1 + Math.floor(Math.random() * 3)}`));
    return;
  }
  const cls = ["egg-bounce", "egg-rainbow", "egg-spin"][roll];
  eggClass.value = ""; // 先清掉，保证同一彩蛋连续触发时动画能重播
  requestAnimationFrame(() => {
    eggClass.value = cls;
    window.clearTimeout(eggTimer);
    eggTimer = window.setTimeout(() => (eggClass.value = ""), 2000);
  });
}

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
  window.clearTimeout(streakTimer);
  window.clearTimeout(eggTimer);
  stopResize?.();
});
</script>

<template>
  <header class="titlebar" data-tauri-drag-region>
    <div class="identity" data-tauri-drag-region>
      <!-- 横向文字标：16px 高、按 1144:206 原始比例推算宽度；不带拖拽区，可点按 -->
      <span class="mark" :class="eggClass" @click="onLogoClick">
        <Logo :size="16" />
      </span>
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
  min-width: 0;
}
.mark {
  display: flex;
  flex: none;
  /* 单击反馈：按下即缩（反馈在 pointer-down，不等松手），松手弹回 */
  transform-origin: 12% center;
  transition: transform 130ms var(--ease);
}
.mark:active {
  transform: scale(0.93);
}

/* ---- 连按彩蛋 ---- */
.mark.egg-bounce {
  animation: egg-bounce 900ms var(--ease) both;
}
@keyframes egg-bounce {
  0% { transform: none; }
  22% { transform: translateY(-7px) scaleY(1.05); }
  46% { transform: translateY(0) scaleY(0.96); }
  68% { transform: translateY(-4px); }
  100% { transform: none; }
}
.mark.egg-rainbow {
  animation: egg-rainbow 1.8s linear both;
}
@keyframes egg-rainbow {
  0%, 100% { filter: hue-rotate(0deg); }
  45% { filter: hue-rotate(210deg) saturate(1.35); }
}
.mark.egg-spin {
  animation: egg-spin 700ms var(--ease) both;
}
@keyframes egg-spin {
  from { transform: rotate(0deg) scale(1.05); }
  to { transform: rotate(360deg) scale(1); }
}
@media (prefers-reduced-motion: reduce) {
  .mark.egg-bounce,
  .mark.egg-rainbow,
  .mark.egg-spin {
    animation: none;
  }
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
