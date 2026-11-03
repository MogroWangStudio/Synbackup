<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";
import { refreshHistory, store, toast } from "../store";
import { fmtDateTime, t } from "../i18n";

const confirmingClear = ref(false);
let confirmTimer = 0;

function onClear() {
  if (!confirmingClear.value) {
    confirmingClear.value = true;
    confirmTimer = window.setTimeout(() => (confirmingClear.value = false), 3000);
    return;
  }
  window.clearTimeout(confirmTimer);
  api
    .clearHistory()
    .then(async () => {
      await refreshHistory();
      toast("success", t("toast.history_cleared"));
    })
    .catch((e) => toast("error", String(e)));
}
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t("history.title") }}</h1>
      </div>
      <button
        v-if="store.history.length"
        class="btn btn-sm btn-danger"
        @click="onClear"
      >
        {{ confirmingClear ? t("history.clear_confirm") : t("history.clear") }}
      </button>
    </div>

    <div v-if="store.history.length" class="card history-list">
      <div v-for="(h, i) in store.history" :key="i" class="history-row">
        <span class="history-time">{{ fmtDateTime(h.ts) }}</span>
        <span class="history-name">{{ h.pairName }}</span>
        <span class="chip" :class="h.trigger === 'auto' ? 'chip-accent' : 'chip-gray'">
          {{ h.trigger === "auto" ? t("history.trigger_auto") : t("history.trigger_manual") }}
        </span>
        <span class="history-device">{{ h.deviceLabel }}</span>
        <span class="history-chips">
          <span v-if="h.copiedToRemote" class="chip chip-accent">
            {{ t("history.to_remote", { n: h.copiedToRemote }) }}
          </span>
          <span v-if="h.copiedToLocal" class="chip chip-accent">
            {{ t("history.to_local", { n: h.copiedToLocal }) }}
          </span>
          <span v-if="h.deletedRemote + h.deletedLocal" class="chip chip-gray">
            {{ t("history.deleted", { n: h.deletedRemote + h.deletedLocal }) }}
          </span>
          <span v-if="h.skipped" class="chip chip-gray">
            {{ t("diff.summary_skipped", { n: h.skipped }) }}
          </span>
          <span v-if="h.errors.length" class="chip chip-red">
            {{ t("diff.summary_failed", { n: h.errors.length }) }}
          </span>
        </span>

        <!-- 逐项明细：做了什么、结果如何。失败项直接标红，不必再单独开一处错误列表 -->
        <details v-if="h.items.length" class="history-detail">
          <summary>{{ t("history.detail") }} · {{ h.items.length }}</summary>
          <div v-for="(it, k) in h.items" :key="k" class="history-item">
            <span class="history-item-act" :class="{ failed: !it.ok }">{{ t("act." + it.action) }}</span>
            <span class="history-item-path">{{ it.path }}</span>
            <span v-if="!it.ok && it.error" class="history-item-err">{{ it.error }}</span>
          </div>
          <p v-if="h.itemsTruncated" class="history-more">
            {{ t("history.items_more", { n: h.itemsTruncated }) }}
          </p>
        </details>

        <!-- 明细被截断时，错误清单仍完整保留，避免关键失败被挤掉 -->
        <details v-if="h.errors.length && h.itemsTruncated" class="history-detail errors">
          <summary>{{ t("history.errors", { n: h.errors.length }) }}</summary>
          <div v-for="e in h.errors" :key="e.path" class="history-item">
            <span class="history-item-act failed">{{ e.path }}</span>
            <span class="history-item-err">{{ e.message }}</span>
          </div>
        </details>
      </div>
    </div>

    <div v-else class="card empty">
      <p class="empty-title">{{ t("history.none") }}</p>
    </div>
  </div>
</template>

<style scoped>
.history-list {
  padding: 2px 16px;
}
.history-row {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  min-height: 44px;
  padding: 7px 0;
  border-bottom: 1px solid var(--hairline);
  font-size: calc(12.5px * var(--font-scale, 1));
}
.history-row:last-child {
  border-bottom: none;
}
.history-time {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
  flex: none;
  width: 128px;
}
.history-name {
  font-weight: 600;
  flex: none;
}
.history-device {
  color: var(--text-2);
  font-size: calc(12px * var(--font-scale, 1));
}
.history-chips {
  display: flex;
  gap: 5px;
  flex-wrap: wrap;
  margin-left: auto;
}
.history-detail {
  width: 100%;
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-2);
}
.history-detail summary {
  cursor: pointer;
  width: fit-content;
  color: var(--text-2);
  padding: 2px 0;
}
.history-detail summary:hover {
  color: var(--text);
}
.history-detail.errors summary {
  color: var(--red-text);
}
.history-item {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 3px 0 0 12px;
  min-width: 0;
}
.history-item-act {
  flex: none;
  width: 78px;
  color: var(--text-3);
}
.history-item-act.failed {
  color: var(--red-text);
}
.history-item-path {
  font-family: var(--mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.history-item-err {
  color: var(--red-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: none;
  max-width: 45%;
}
.history-more {
  padding: 4px 0 0 12px;
  color: var(--text-3);
}
.empty {
  padding: 40px 20px;
  text-align: center;
}
.empty-title {
  font-size: calc(13px * var(--font-scale, 1));
  color: var(--text-2);
}
</style>
