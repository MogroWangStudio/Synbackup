<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { refreshVolumes, store } from "../store";
import { api } from "../api";
import { fmtSize, fmtTime, t } from "../i18n";
import { MIN_POLL_MS, OVERVIEW_STAT_LABEL, type OverviewStatKey } from "../overview";
import Icon from "../components/Icon.vue";
import EmptyState from "../components/EmptyState.vue";
import OverviewEditDialog from "../components/OverviewEditDialog.vue";

const pendingTotal = computed(() =>
  store.pending.reduce((n, p) => n + p.items.length, 0),
);

/* ---- 轮询：只给"可轮询且正在显示"的数据项开定时器 ----
   内存走本地 ref（后端命令），设备直接刷新 store.volumes；
   频率取自设置（0 / <3s 视为关闭），改设置即时重启定时器。 */
const memoryText = ref("");
let memTimer: number | undefined;
let devTimer: number | undefined;

async function refreshMemory() {
  try {
    memoryText.value = fmtSize(await api.getAppMemory());
  } catch {
    /* 拉取失败保持上一次显示 */
  }
}

function restartPolling() {
  window.clearInterval(memTimer);
  window.clearInterval(devTimer);
  memTimer = undefined;
  devTimer = undefined;

  const shownKeys = new Set(store.settings.overviewStats);
  const memMs = store.settings.overviewMemoryMs;
  if (shownKeys.has("memory") && memMs >= MIN_POLL_MS) {
    void refreshMemory();
    memTimer = window.setInterval(refreshMemory, memMs);
  }
  const devMs = store.settings.overviewDevicesMs;
  if (shownKeys.has("devices") && devMs >= MIN_POLL_MS) {
    devTimer = window.setInterval(() => {
      void refreshVolumes().catch(() => {});
    }, devMs);
  }
}

onMounted(restartPolling);
onBeforeUnmount(() => {
  window.clearInterval(memTimer);
  window.clearInterval(devTimer);
});
watch(
  () => [
    store.settings.overviewStats.join(","),
    store.settings.overviewMemoryMs,
    store.settings.overviewDevicesMs,
  ],
  restartPolling,
);

/** 卡片内容：按设置里的顺序取显示项；未知键（旧配置 / 手改）忽略 */
const stats = computed(() => {
  const valueOf: Record<OverviewStatKey, string | number> = {
    pairs: store.pairs.length,
    devices: store.volumes.length,
    pending: pendingTotal.value,
    memory: memoryText.value || "–",
  };
  return store.settings.overviewStats
    .filter((k): k is OverviewStatKey => k in OVERVIEW_STAT_LABEL)
    .map((k) => ({ key: k, n: valueOf[k], label: t(OVERVIEW_STAT_LABEL[k]) }));
});

const editing = ref(false);

function goPairs() {
  store.view = "pairs";
}
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t("overview.title") }}</h1>
      </div>
      <button type="button" class="btn btn-sm" @click="editing = true">
        <Icon name="edit" :size="13" />
        {{ t("overview.edit") }}
      </button>
    </div>

    <!-- 待处理差异横幅 -->
    <button v-if="store.pending.length" class="card banner" @click="goPairs">
      <Icon name="alert" :size="17" class="banner-icon" />
      <span class="banner-text">{{ t("overview.pending_banner", { n: store.pending.length }) }}</span>
      <span class="banner-cta">{{ t("overview.pending_cta") }}</span>
    </button>

    <!-- 统计 -->
    <div v-if="stats.length" class="stats">
      <div v-for="s in stats" :key="s.key" class="stat">
        <span class="stat-num">{{ s.n }}</span>
        <span class="stat-label">{{ s.label }}</span>
      </div>
    </div>
    <!-- 全部隐藏时不占位，只留一句说明与回到编辑的入口 -->
    <p v-else class="stats-empty">
      {{ t("overview.all_hidden") }}
      <button type="button" class="link-btn" @click="editing = true">
        {{ t("overview.edit") }}
      </button>
    </p>

    <!-- 空状态 -->
    <EmptyState
      v-if="!store.pairs.length"
      icon="sync"
      :title="t('overview.empty_pairs_title')"
      :desc="t('overview.empty_pairs_desc')"
    >
      <button class="btn btn-primary" @click="store.editor = { pair: null }">
        <Icon name="plus" :size="14" />
        {{ t("overview.empty_pairs_cta") }}
      </button>
    </EmptyState>

    <!-- 最近同步 -->
    <section v-if="store.history.length" class="recent">
      <h2 class="section-title">{{ t("overview.recent") }}</h2>
      <div class="card">
        <div v-for="(h, i) in store.history.slice(0, 6)" :key="i" class="recent-row">
          <Icon name="drive" :size="14" class="recent-icon" />
          <span class="recent-name">{{ h.pairName }}</span>
          <span class="recent-chip" :class="h.errors.length ? 'chip-red-text' : 'chip-green-text'">
            <template v-if="h.errors.length">{{ t("diff.summary_failed", { n: h.errors.length }) }}</template>
            <template v-else>
              {{ t("history.moved", { n: h.copiedToRemote + h.copiedToLocal }) }}
            </template>
          </span>
          <span class="recent-time">{{ fmtTime(h.ts) }}</span>
        </div>
      </div>
    </section>

    <OverviewEditDialog v-if="editing" @close="editing = false" />
  </div>
</template>

<style scoped>
.banner {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 12px 14px;
  margin-bottom: 18px;
  cursor: pointer;
  text-align: left;
  border-color: var(--orange-soft);
  background: var(--orange-soft);
  color: var(--orange-text);
  transition: filter 120ms ease;
}
.banner:hover {
  filter: brightness(0.98);
}
.banner-icon {
  flex: none;
}
.banner-text {
  flex: 1;
  font-weight: 500;
}
.banner-cta {
  font-weight: 600;
  font-size: calc(12.5px * var(--font-scale, 1));
}

.stats {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 12px;
  margin-bottom: 22px;
}
.stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 14px 16px;
}
.stat-num {
  font-size: calc(26px * var(--font-scale, 1));
  font-weight: 700;
  letter-spacing: -0.02em;
  line-height: 1.1;
  font-variant-numeric: tabular-nums;
}
.stat-label {
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-2);
}
/* 统计项全部隐藏后的一句说明：不下沉成空白，也不抢版面 */
.stats-empty {
  display: flex;
  align-items: baseline;
  gap: 6px;
  margin-bottom: 22px;
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-3);
}
.link-btn {
  border: none;
  background: none;
  padding: 0;
  font-size: inherit;
  color: var(--accent-text);
  font-weight: 500;
  cursor: pointer;
}
.link-btn:hover {
  text-decoration: underline;
}

.section-title {
  font-size: calc(13px * var(--font-scale, 1));
  font-weight: 600;
  color: var(--text-2);
  margin-bottom: 8px;
}
.recent {
  margin-top: 4px;
}
.recent .card {
  padding: 2px 14px;
}
.recent-row {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 40px;
  border-bottom: 1px solid var(--hairline);
  font-size: calc(12.5px * var(--font-scale, 1));
}
.recent-row:last-child {
  border-bottom: none;
}
.recent-icon {
  color: var(--text-3);
  flex: none;
}
.recent-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}
.recent-chip {
  font-size: calc(11.5px * var(--font-scale, 1));
  font-variant-numeric: tabular-nums;
}
.chip-green-text {
  color: var(--green-text);
}
.chip-red-text {
  color: var(--red-text);
}
.recent-time {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
</style>
