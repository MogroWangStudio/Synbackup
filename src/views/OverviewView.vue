<script setup lang="ts">
import { computed } from "vue";
import { store } from "../store";
import { fmtTime, t } from "../i18n";
import Icon from "../components/Icon.vue";

const pendingTotal = computed(() =>
  store.pending.reduce((n, p) => n + p.items.length, 0),
);

const stats = computed(() => [
  { n: store.pairs.length, label: t("overview.stat_pairs") },
  { n: store.volumes.length, label: t("overview.stat_devices") },
  { n: pendingTotal.value, label: t("overview.stat_pending") },
]);

function goPairs() {
  store.view = "pairs";
}
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t("overview.title") }}</h1>
        <p class="page-desc">{{ t("overview.desc") }}</p>
      </div>
    </div>

    <!-- 待处理差异横幅 -->
    <button v-if="store.pending.length" class="card banner" @click="goPairs">
      <Icon name="alert" :size="17" class="banner-icon" />
      <span class="banner-text">{{ t("overview.pending_banner", { n: store.pending.length }) }}</span>
      <span class="banner-cta">{{ t("overview.pending_cta") }}</span>
    </button>

    <!-- 统计 -->
    <div class="stats">
      <div v-for="s in stats" :key="s.label" class="stat">
        <span class="stat-num">{{ s.n }}</span>
        <span class="stat-label">{{ s.label }}</span>
      </div>
    </div>

    <!-- 空状态 -->
    <div v-if="!store.pairs.length" class="card empty">
      <Icon name="sync" :size="26" class="empty-icon" />
      <p class="empty-title">{{ t("overview.empty_pairs_title") }}</p>
      <p class="empty-desc">{{ t("overview.empty_pairs_desc") }}</p>
      <button class="btn btn-primary" @click="store.editor = { pair: null }">
        <Icon name="plus" :size="14" />
        {{ t("overview.empty_pairs_cta") }}
      </button>
    </div>

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
  grid-template-columns: repeat(3, 1fr);
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

.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 40px 20px;
  text-align: center;
}
.empty-icon {
  color: var(--text-3);
}
.empty-title {
  font-size: calc(14px * var(--font-scale, 1));
  font-weight: 600;
}
.empty-desc {
  font-size: calc(12.5px * var(--font-scale, 1));
  color: var(--text-2);
  max-width: 340px;
}
.empty .btn {
  margin-top: 6px;
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
