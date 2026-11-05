<script setup lang="ts">
import { computed, ref } from "vue";
import { api } from "../api";
import { refreshHistory, store, toast } from "../store";
import { fmtDateTime, fmtSize, t } from "../i18n";
import { statusChipClass } from "../ui";
import { useConfirmAction } from "../composables/useConfirmAction";
import type { HistoryEntry } from "../types";
import Select from "../components/Select.vue";
import type { SelectOption } from "../components/Select.vue";

// ---- 筛选与查找 ----
const kindFilter = ref<"all" | "sync" | "scan">("all");
const pairFilter = ref("");
const query = ref("");

/** 任务下拉选项：从历史记录提取（覆盖已删除的任务），按最近出现排序 */
const pairOptions = computed(() => {
  const seen = new Map<string, string>();
  for (const h of store.history) {
    if (!seen.has(h.pairId)) seen.set(h.pairId, h.pairName);
  }
  return [...seen].map(([id, name]) => ({ id, name }));
});

const pairSelectOptions = computed<SelectOption[]>(() => [
  { value: "", label: t("history.pair_all") },
  ...pairOptions.value.map(({ id, name }) => ({ value: id, label: name })),
]);

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  return store.history.filter((h) => {
    const kind = h.kind;
    if (kindFilter.value !== "all" && kind !== kindFilter.value) return false;
    if (pairFilter.value && h.pairId !== pairFilter.value) return false;
    if (q) {
      const hit =
        h.pairName.toLowerCase().includes(q) ||
        h.deviceLabel.toLowerCase().includes(q) ||
        h.items.some((it) => it.path.toLowerCase().includes(q)) ||
        (h.scanDiffs ?? []).some((d) => d.path.toLowerCase().includes(q));
      if (!hit) return false;
    }
    return true;
  });
});

/** 该条记录的数据量（后端累计，旧记录由后端缺省补 0；为 0 时不显示） */
function totalBytes(h: HistoryEntry): number {
  return h.totalBytes;
}

/** 差异状态配色（与差异处理对话框一致，见 ui.ts） */

/** scan 记录中的冲突数（both_changed / type_conflict） */
function conflictCount(h: HistoryEntry): number {
  return (h.scanDiffs ?? []).filter((d) => d.status === "both_changed" || d.status === "type_conflict").length;
}

function clearHistory() {
  api
    .clearHistory()
    .then(async () => {
      await refreshHistory();
      toast("success", t("toast.history_cleared"));
    })
    .catch((e) => toast("error", String(e)));
}

const { confirming: confirmingClear, confirm: onClear } = useConfirmAction(clearHistory);
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

    <!-- 筛选与查找 -->
    <div v-if="store.history.length" class="card filter-bar">
      <div class="seg" role="group" :aria-label="t('history.filter_kind')">
        <button
          v-for="k in ['all', 'sync', 'scan'] as const"
          :key="k"
          type="button"
          class="seg-btn"
          :class="{ active: kindFilter === k }"
          @click="kindFilter = k"
        >
          {{ t("history.filter_" + k) }}
        </button>
      </div>
      <Select
        v-model="pairFilter"
        :options="pairSelectOptions"
        class="filter-pair"
        :aria-label="t('history.filter_pair')"
      />
      <input
        v-model="query"
        class="input filter-search"
        :placeholder="t('history.search_placeholder')"
        spellcheck="false"
      />
    </div>

    <div v-if="store.history.length" class="history-meta">
      {{ t("history.matched_count", { n: filtered.length }) }}
    </div>

    <div v-if="store.history.length && filtered.length" class="card history-list">
      <div v-for="(h, i) in filtered" :key="i" class="history-row">
        <span class="history-time">{{ fmtDateTime(h.ts) }}</span>
        <span class="history-name">{{ h.pairName }}</span>
        <span class="chip" :class="h.trigger === 'auto' ? 'chip-accent' : 'chip-gray'">
          {{ h.trigger === "auto" ? t("history.trigger_auto") : t("history.trigger_manual") }}
        </span>
        <span class="history-device">{{ h.deviceLabel }}</span>
        <span class="history-chips">
          <template v-if="h.kind === 'scan'">
            <span class="chip chip-orange">{{ t("history.scan_found") }}</span>
            <span v-if="conflictCount(h)" class="chip chip-red">
              {{ t("diff.conflict_hint", { n: conflictCount(h) }) }}
            </span>
            <span v-if="totalBytes(h)" class="chip chip-gray" :title="t('history.total_bytes')">
              {{ fmtSize(totalBytes(h)) }}
            </span>
          </template>
          <template v-else>
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
            <span v-if="totalBytes(h)" class="chip chip-gray" :title="t('history.total_bytes')">
              {{ fmtSize(totalBytes(h)) }}
            </span>
            <span v-if="h.errors.length" class="chip chip-red">
              {{ t("diff.summary_failed", { n: h.errors.length }) }}
            </span>
          </template>
        </span>

        <!-- 逐项明细：同步记录显示做了什么，检查记录显示发现的差异。失败项直接标红 -->
        <details
          v-if="h.kind === 'scan' ? (h.scanDiffs?.length ?? 0) > 0 : h.items.length > 0"
          class="history-detail"
        >
          <summary v-if="h.kind === 'scan'">
            {{ t("history.scan_detail") }} · {{ h.scanDiffs?.length }}
          </summary>
          <summary v-else>{{ t("history.detail") }} · {{ h.items.length }}</summary>

          <template v-if="h.kind === 'scan'">
            <div v-for="(d, k) in h.scanDiffs" :key="k" class="history-item">
              <span class="history-item-act">
                <span class="chip" :class="statusChipClass(d.status)">{{ t("st." + d.status) }}</span>
              </span>
              <span class="history-item-path">{{ d.path }}</span>
              <span v-if="d.size" class="history-item-size">{{ fmtSize(d.size) }}</span>
            </div>
            <p v-if="h.scanTotal > (h.scanDiffs?.length ?? 0)" class="history-more">
              {{ t("history.items_more", { n: h.scanTotal - (h.scanDiffs?.length ?? 0) }) }}
            </p>
          </template>
          <template v-else>
            <div v-for="(it, k) in h.items" :key="k" class="history-item">
              <span class="history-item-act" :class="{ failed: !it.ok }">{{ t("act." + it.action) }}</span>
              <span class="history-item-path">{{ it.path }}</span>
              <span v-if="it.size" class="history-item-size">{{ fmtSize(it.size) }}</span>
              <span v-if="!it.ok && it.error" class="history-item-err">{{ it.error }}</span>
            </div>
            <p v-if="h.itemsTruncated" class="history-more">
              {{ t("history.items_more", { n: h.itemsTruncated }) }}
            </p>
          </template>
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

    <!-- 有历史但筛选无结果 -->
    <div v-else-if="store.history.length" class="card empty">
      <p class="empty-title">{{ t("history.no_match") }}</p>
    </div>

    <div v-else class="card empty">
      <p class="empty-title">{{ t("history.none") }}</p>
    </div>
  </div>
</template>

<style scoped>
/* ---- 筛选条 ---- */
.filter-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  padding: 8px 12px;
  margin-bottom: 10px;
}
.seg {
  display: inline-flex;
  padding: 2px;
  border: 1px solid var(--hairline);
  border-radius: var(--radius-s);
  background: var(--fill-softer);
}
.seg-btn {
  padding: 4px 12px;
  border: none;
  border-radius: calc(var(--radius-s) - 2px);
  background: transparent;
  color: var(--text-2);
  font-size: calc(12px * var(--font-scale, 1));
  cursor: pointer;
  transition:
    background-color 110ms ease,
    color 110ms ease;
}
.seg-btn:hover {
  color: var(--text);
}
.seg-btn.active {
  background: var(--accent);
  color: var(--text-on-accent);
  font-weight: 500;
}
.seg-btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
.filter-pair {
  width: 150px;
  flex: none;
}
.filter-search {
  flex: 1;
  min-width: 140px;
}
.history-meta {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
  margin: 0 2px 8px;
  font-variant-numeric: tabular-nums;
}

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
.history-item-size {
  flex: none;
  font-variant-numeric: tabular-nums;
  color: var(--text-3);
  min-width: 52px;
  text-align: right;
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
