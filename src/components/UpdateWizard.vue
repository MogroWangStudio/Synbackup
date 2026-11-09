<script setup lang="ts">
/* 更新向导：检查到新版本后接管整条链路——下载、等待重启、安装。
   打开即开始下载（用户刚点了"检查更新"，这一步本就是他想要的），
   下载完成后给 6 秒倒计时自动重启；倒计时期间可以取消，也可以用
   "稍后"直接关掉，选择权始终在用户手上。 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../api";
import { store, toast } from "../store";
import { fmtSize, t } from "../i18n";
import type { UpdateInfo, UpdateProgress } from "../types";
import Icon from "./Icon.vue";
import Modal from "./Modal.vue";

const props = defineProps<{ info: UpdateInfo }>();
const emit = defineEmits<{ (e: "close"): void }>();

const modalRef = ref<InstanceType<typeof Modal>>();
/** 下载完成后自动重启的等待秒数 */
const AUTO_RESTART_SEC = 6;

type Phase = "downloading" | "ready" | "installing" | "error" | "manual";

// 平台没有对应产物（非 Windows x64 / macOS arm64）才落到 manual
const phase = ref<Phase>(props.info.auto ? "downloading" : "manual");
const received = ref(0);
const total = ref(0);
const error = ref("");
const countdown = ref(AUTO_RESTART_SEC);

let unlisten: UnlistenFn | undefined;
let tick: number | undefined;
/** 用户主动取消：正在飞的下载结果不再改变界面 */
let cancelled = false;

const percent = computed(() =>
  total.value > 0 ? Math.min(100, Math.round((received.value / total.value) * 100)) : 0,
);
/** 有同步在跑时暂停倒计时：替换会强杀进程，正在复制的文件会留半截 */
const syncing = computed(() => store.syncingPairIds.length > 0);
const readyText = computed(() =>
  syncing.value || countdown.value <= 0
    ? t("upd.ready_now")
    : t("upd.ready", { n: countdown.value }),
);

onMounted(async () => {
  unlisten = await listen<UpdateProgress>("update-progress", (e) => {
    received.value = e.payload.received;
    total.value = e.payload.total;
  });
  if (props.info.auto) void startDownload();
});

onBeforeUnmount(() => {
  unlisten?.();
  window.clearInterval(tick);
});

async function startDownload() {
  if (cancelled) return;
  phase.value = "downloading";
  error.value = "";
  received.value = 0;
  total.value = 0;
  try {
    await api.downloadUpdate(props.info.version);
    if (cancelled) return;
    phase.value = "ready";
    beginCountdown();
  } catch (e) {
    if (cancelled) return;
    error.value = String(e);
    phase.value = "error";
  }
}

/** 倒计时：同步进行中时原地等待，同步结束后继续 */
function beginCountdown() {
  countdown.value = AUTO_RESTART_SEC;
  window.clearInterval(tick);
  tick = window.setInterval(() => {
    if (syncing.value) return;
    countdown.value -= 1;
    if (countdown.value <= 0) {
      window.clearInterval(tick);
      tick = undefined;
      void install();
    }
  }, 1000);
}

async function install() {
  if (phase.value === "installing") return;
  window.clearInterval(tick);
  tick = undefined;
  phase.value = "installing";
  try {
    await api.installUpdate(props.info.version);
    // 正常路径：后端已派发替换脚本并退出进程，界面随之消失
  } catch (e) {
    error.value = String(e);
    phase.value = "error";
  }
}

/** 取消下载并关闭向导（下载中断，不留半截文件） */
function cancelDownload() {
  toast("info", t("upd.cancelled"));
  close();
}

/** "稍后"：保留已下载的包，关掉向导 */
function later() {
  window.clearInterval(tick);
  tick = undefined;
  close();
}

function close() {
  modalRef.value?.requestClose();
}

function openPage() {
  void api.openExternal(props.info.url).catch((e) => toast("error", String(e)));
}

/** 关闭的唯一出口（X / Esc / 遮罩 / 取消 / 稍后）：
    下载中关闭必须同时叫停后台下载——否则组件已卸载，命令还在写盘 */
function handleClose() {
  if (phase.value === "downloading") {
    cancelled = true;
    void api.cancelUpdateDownload().catch(() => {});
  }
  window.clearInterval(tick);
  tick = undefined;
  emit("close");
}
</script>

<template>
  <Modal ref="modalRef" :title="t('upd.title')" :busy="phase === 'installing'" @close="handleClose">
    <div class="upd">
      <div class="upd-head">
        <span class="upd-current">v{{ info.current }}</span>
        <span class="upd-arrow" aria-hidden="true">→</span>
        <span class="upd-version">{{ info.version }}</span>
      </div>

      <Transition name="upd-step" mode="out-in">
        <!-- 下载中：进度条是唯一的进度来源，不做不确定态的来回滚动 -->
        <div v-if="phase === 'downloading'" key="downloading" class="upd-state">
          <div
            class="bar"
            role="progressbar"
            :aria-valuenow="percent"
            aria-valuemin="0"
            aria-valuemax="100"
            :aria-label="t('upd.downloading')"
          >
            <span class="bar-fill" :style="{ transform: `scaleX(${percent / 100})` }" />
          </div>
          <div class="upd-meta">
            <span>{{ t("upd.downloading") }}</span>
            <span class="upd-bytes">
              <template v-if="total">{{ percent }}% · </template>
              {{ total ? `${fmtSize(received)} / ${fmtSize(total)}` : fmtSize(received) }}
            </span>
          </div>
        </div>

        <div v-else-if="phase === 'ready'" key="ready" class="upd-state">
          <div class="upd-line">
            <Icon name="check" :size="15" class="upd-ok" />
            <span>{{ readyText }}</span>
          </div>
          <p class="upd-hint">{{ t("upd.restart_hint") }}</p>
          <p v-if="syncing" class="upd-hint">{{ t("upd.syncing") }}</p>
        </div>

        <div v-else-if="phase === 'installing'" key="installing" class="upd-state">
          <div class="upd-line">
            <span class="upd-pulse" />
            <span>{{ t("upd.installing") }}</span>
          </div>
        </div>

        <div v-else-if="phase === 'error'" key="error" class="upd-state">
          <div class="upd-line error">
            <Icon name="alert" :size="15" />
            <span>{{ t("upd.failed") }}</span>
          </div>
          <p class="upd-error">{{ error }}</p>
        </div>

        <div v-else key="manual" class="upd-state">
          <p class="upd-hint">{{ t("upd.unsupported") }}</p>
        </div>
      </Transition>

      <section class="upd-notes">
        <h3>{{ t("upd.notes") }}</h3>
        <pre class="upd-notes-body">{{ info.notes || t("upd.no_notes") }}</pre>
      </section>
    </div>

    <template #footer>
      <span class="upd-file">{{ info.asset }}</span>
      <div class="foot-actions">
        <template v-if="phase === 'downloading'">
          <button class="btn btn-sm" @click="cancelDownload">{{ t("upd.cancel") }}</button>
        </template>
        <template v-else-if="phase === 'ready'">
          <button class="btn btn-sm" @click="later">{{ t("upd.later") }}</button>
          <button class="btn btn-sm btn-primary" @click="install">
            {{ t("upd.install_now") }}
          </button>
        </template>
        <template v-else-if="phase === 'installing'">
          <button class="btn btn-sm" disabled>{{ t("upd.installing") }}</button>
        </template>
        <template v-else-if="phase === 'error'">
          <button class="btn btn-sm" @click="openPage">{{ t("upd.manual") }}</button>
          <button class="btn btn-sm btn-primary" @click="startDownload">
            {{ t("upd.retry") }}
          </button>
        </template>
        <template v-else>
          <button class="btn btn-sm" @click="close">{{ t("common.close") }}</button>
          <button class="btn btn-sm btn-primary" @click="openPage">{{ t("upd.manual") }}</button>
        </template>
      </div>
    </template>
  </Modal>
</template>

<style scoped>
.upd {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding-top: 2px;
}
.upd-head {
  display: flex;
  align-items: baseline;
  gap: 7px;
}
/* 版本号是主角：大一号、负字距；旧版本以等宽小字在前，箭头点明方向 */
.upd-version {
  font-size: calc(19px * var(--font-scale, 1));
  font-weight: 650;
  letter-spacing: -0.02em;
  font-variant-numeric: tabular-nums;
}
.upd-current {
  font-family: var(--mono);
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
}
.upd-arrow {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
}
.upd-state {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 42px;
}
.upd-line {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: calc(13px * var(--font-scale, 1));
  color: var(--text);
}
.upd-line.error {
  color: var(--red-text);
}
.upd-ok {
  color: var(--accent);
  flex: none;
}
/* 安装中的呼吸点：低频、低对比，只表示"还活着" */
.upd-pulse {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex: none;
  background: var(--accent);
  animation: upd-pulse 1.4s var(--ease) infinite;
}
@keyframes upd-pulse {
  0%,
  100% {
    opacity: 0.35;
    transform: scale(0.85);
  }
  50% {
    opacity: 1;
    transform: scale(1);
  }
}
@media (prefers-reduced-motion: reduce) {
  .upd-pulse {
    animation: none;
    opacity: 1;
  }
}
.bar {
  height: 4px;
  border-radius: 2px;
  background: var(--fill-soft);
  overflow: hidden;
}
/* 用 scaleX 而非 width：整条走合成层，24 MB 的进度刷新不掉帧 */
.bar-fill {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: var(--accent);
  transform-origin: left center;
  transition: transform 240ms linear;
}
.upd-meta {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-2);
}
.upd-bytes {
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
  color: var(--text-3);
  white-space: nowrap;
}
.upd-hint {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
  line-height: 1.55;
}
.upd-error {
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--red-text);
  user-select: text;
  word-break: break-word;
}
.upd-notes h3 {
  font-size: calc(11.5px * var(--font-scale, 1));
  font-weight: 600;
  color: var(--text-2);
  margin-bottom: 6px;
}
.upd-notes-body {
  max-height: 180px;
  overflow-y: auto;
  padding: 10px 12px;
  border-radius: var(--radius-s);
  background: var(--fill-softer);
  border: 1px solid var(--hairline);
  font-family: inherit;
  font-size: calc(12px * var(--font-scale, 1));
  line-height: 1.6;
  color: var(--text-2);
  white-space: pre-wrap;
  word-break: break-word;
  user-select: text;
}
/* 阶段切换：只做淡入与 2px 微移，方向固定向下——内容在往下推进 */
.upd-step-enter-active {
  transition:
    opacity 160ms var(--ease),
    transform 180ms var(--ease);
}
.upd-step-leave-active {
  transition:
    opacity 110ms var(--ease-modal-out),
    transform 120ms var(--ease-modal-out);
}
.upd-step-enter-from {
  opacity: 0;
  transform: translateY(-2px);
}
.upd-step-leave-to {
  opacity: 0;
  transform: translateY(2px);
}
.upd-file {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--mono);
  font-size: calc(10.5px * var(--font-scale, 1));
  color: var(--text-3);
}
</style>
