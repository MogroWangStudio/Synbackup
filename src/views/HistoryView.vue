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
        <p class="page-desc">{{ t("history.desc") }}</p>
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
        <span class="history-device">{{ h.deviceLabel }}</span>
        <span class="history-chips">
          <span v-if="h.copiedToRemote" class="chip chip-blue">
            {{ t("history.to_remote", { n: h.copiedToRemote }) }}
          </span>
          <span v-if="h.copiedToLocal" class="chip chip-blue">
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
        <details v-if="h.errors.length" class="history-errors">
          <summary>{{ t("history.errors", { n: h.errors.length }) }}</summary>
          <div v-for="e in h.errors" :key="e.path" class="history-error">
            <span class="history-error-path">{{ e.path }}</span>
            <span class="history-error-msg">{{ e.message }}</span>
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
  font-size: 12.5px;
}
.history-row:last-child {
  border-bottom: none;
}
.history-time {
  font-size: 11.5px;
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
  font-size: 12px;
}
.history-chips {
  display: flex;
  gap: 5px;
  flex-wrap: wrap;
  margin-left: auto;
}
.history-errors {
  width: 100%;
  font-size: 11.5px;
}
.history-errors summary {
  cursor: pointer;
  color: var(--red-text);
  width: fit-content;
}
.history-error {
  display: flex;
  gap: 10px;
  padding: 3px 0 0;
}
.history-error-path {
  font-family: var(--mono);
  max-width: 45%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.history-error-msg {
  color: var(--red-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.empty {
  padding: 40px 20px;
  text-align: center;
}
.empty-title {
  font-size: 13px;
  color: var(--text-2);
}
</style>
