<script setup lang="ts">
import { computed, reactive, ref } from "vue";
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

/** 文件类型 = 路径末段扩展名（差异与同步明细都只含文件） */
function kindOf(path: string): string {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toUpperCase() : "—";
}

/* ---- 明细列宽：表格式布局，除文件列外均可拖拽列头手柄调整 ----
   scan（状态/类型/大小）与 sync（动作/类型/大小/错误）的列语义不同，
   列宽各自独立成组，互不牵连；同组内跨记录共享。列宽是纯界面偏好，
   存 localStorage（旧版共享单组的键直接废弃，回落默认）。 */
type DetailKind = "scan" | "sync";
type ColKey = "act" | "type" | "size" | "err";
const COLS_KEY = "historyColWidths2";
const DEFAULT_W: Record<DetailKind, Partial<Record<ColKey, number>>> = {
  scan: { act: 96, type: 56, size: 72 },
  sync: { act: 96, type: 56, size: 72, err: 220 },
};
const MIN_W: Record<ColKey, number> = { act: 72, type: 44, size: 52, err: 120 };
const MAX_W = 480;

function loadWidths(): Record<DetailKind, Record<ColKey, number>> {
  const make = (kind: DetailKind) => {
    const merged = { ...DEFAULT_W[kind] } as Record<ColKey, number>;
    for (const k of Object.keys(DEFAULT_W[kind]) as ColKey[]) {
      if (merged[k] === undefined) merged[k] = DEFAULT_W.sync[k] ?? 96;
    }
    return merged;
  };
  const out = { scan: make("scan"), sync: make("sync") };
  try {
    const raw = JSON.parse(localStorage.getItem(COLS_KEY) ?? "{}");
    for (const kind of ["scan", "sync"] as DetailKind[]) {
      for (const k of Object.keys(out[kind]) as ColKey[]) {
        const v = raw?.[kind]?.[k];
        if (typeof v === "number" && Number.isFinite(v)) {
          out[kind][k] = Math.max(MIN_W[k], Math.min(MAX_W, v));
        }
      }
    }
  } catch {
    /* 损坏的偏好直接用默认 */
  }
  return out;
}

const widths = reactive(loadWidths());

/* 两组 CSS 变量（scan: --hs-*，sync: --hy-*）挂在每个 details 上，
   明细各自引用自己那组 */
const gridStyle = computed(() => ({
  "--hs-act": `${widths.scan.act}px`,
  "--hs-type": `${widths.scan.type}px`,
  "--hs-size": `${widths.scan.size}px`,
  "--hy-act": `${widths.sync.act}px`,
  "--hy-type": `${widths.sync.type}px`,
  "--hy-size": `${widths.sync.size}px`,
  "--hy-err": `${widths.sync.err}px`,
}));

function onGripDown(kind: DetailKind, key: ColKey, e: PointerEvent) {
  if (e.pointerType === "mouse" && e.button !== 0) return;
  const el = e.currentTarget as HTMLElement;
  try {
    el.setPointerCapture(e.pointerId);
  } catch {
    /* 合成指针等场景 */
  }
  const startX = e.clientX;
  const startW = widths[kind][key];
  const move = (ev: PointerEvent) => {
    widths[kind][key] = Math.max(MIN_W[key], Math.min(MAX_W, startW + ev.clientX - startX));
  };
  const stop = () => {
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", stop);
    el.removeEventListener("pointercancel", stop);
    try {
      localStorage.setItem(COLS_KEY, JSON.stringify(widths));
    } catch {
      /* 存不了就仅本次生效 */
    }
  };
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", stop);
  el.addEventListener("pointercancel", stop);
}

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
          :style="gridStyle"
        >
          <summary v-if="h.kind === 'scan'">
            {{ t("history.scan_detail") }} · {{ h.scanDiffs?.length }}
          </summary>
          <summary v-else>{{ t("history.detail") }} · {{ h.items.length }}</summary>

          <template v-if="h.kind === 'scan'">
            <div class="hd-head">
              <span class="col-resizable">
                {{ t("diff.col_status") }}
                <span class="col-grip" :aria-hidden="true" @pointerdown="onGripDown('scan', 'act', $event)" />
              </span>
              <span>{{ t("diff.col_path") }}</span>
              <span class="col-resizable">
                {{ t("diff.col_type") }}
                <span class="col-grip" :aria-hidden="true" @pointerdown="onGripDown('scan', 'type', $event)" />
              </span>
              <span class="cell-num col-resizable">
                {{ t("history.col_size") }}
                <span class="col-grip" :aria-hidden="true" @pointerdown="onGripDown('scan', 'size', $event)" />
              </span>
            </div>
            <div v-for="(d, k) in h.scanDiffs" :key="k" class="history-item hd-row">
              <span class="history-item-act">
                <span class="chip" :class="statusChipClass(d.status)" :title="t('st.' + d.status)">{{ t("st." + d.status) }}</span>
              </span>
              <span class="history-item-path" :title="d.path">{{ d.path }}</span>
              <span class="history-item-kind" :title="kindOf(d.path)">{{ kindOf(d.path) }}</span>
              <span class="history-item-size">{{ d.size ? fmtSize(d.size) : "—" }}</span>
            </div>
            <p v-if="h.scanTotal > (h.scanDiffs?.length ?? 0)" class="history-more">
              {{ t("history.items_more", { n: h.scanTotal - (h.scanDiffs?.length ?? 0) }) }}
            </p>
          </template>
          <template v-else>
            <div class="hd-head has-err-col">
              <span class="col-resizable">
                {{ t("history.col_act") }}
                <span class="col-grip" :aria-hidden="true" @pointerdown="onGripDown('sync', 'act', $event)" />
              </span>
              <span>{{ t("diff.col_path") }}</span>
              <span class="col-resizable">
                {{ t("diff.col_type") }}
                <span class="col-grip" :aria-hidden="true" @pointerdown="onGripDown('sync', 'type', $event)" />
              </span>
              <span class="cell-num col-resizable">
                {{ t("history.col_size") }}
                <span class="col-grip" :aria-hidden="true" @pointerdown="onGripDown('sync', 'size', $event)" />
              </span>
              <span class="col-resizable">
                {{ t("history.col_err") }}
                <span class="col-grip" :aria-hidden="true" @pointerdown="onGripDown('sync', 'err', $event)" />
              </span>
            </div>
            <div v-for="(it, k) in h.items" :key="k" class="history-item hd-row has-err-col">
              <span class="history-item-act" :class="{ failed: !it.ok }">{{ t("act." + it.action) }}</span>
              <span class="history-item-path" :title="it.path">{{ it.path }}</span>
              <span class="history-item-kind" :title="kindOf(it.path)">{{ kindOf(it.path) }}</span>
              <span class="history-item-size">{{ it.size ? fmtSize(it.size) : "—" }}</span>
              <span class="history-item-err" :class="{ failed: !it.ok && it.error }">{{ it.ok ? "—" : it.error }}</span>
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
  /* 列拖宽超过卡片宽度时明细自身横向滚动：内容不被裁掉，
     手柄永远可达（此前溢出被裁，手柄点不到，列宽无法恢复） */
  overflow-x: auto;
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
/* 明细表格式：列宽由 details 上的 CSS 变量给出（列头手柄可拖拽调整），
   文件列弹性吸收；scan 明细 4 列、sync 明细多一列错误（末列可调） */
.hd-head {
  display: grid;
  grid-template-columns:
    var(--hs-act, 96px)
    minmax(180px, 1fr)
    var(--hs-type, 56px)
    var(--hs-size, 72px);
  gap: 10px;
  align-items: center;
  padding: 4px 10px 0 12px;
  font-size: calc(10.5px * var(--font-scale, 1));
  font-weight: 600;
  color: var(--text-3);
}
.hd-head > span {
  position: relative;
  min-width: 0;
}
/* sync 明细多一列错误：同样的前四列 + 错误列（可调） */
.hd-head.has-err-col {
  grid-template-columns:
    var(--hy-act, 96px)
    minmax(180px, 1fr)
    var(--hy-type, 56px)
    var(--hy-size, 72px)
    var(--hy-err, 220px);
}
.col-grip {
  position: absolute;
  right: -9px;
  top: -2px;
  bottom: -2px;
  width: 13px;
  cursor: col-resize;
  touch-action: none;
}
.col-grip::after {
  content: "";
  position: absolute;
  right: 6px;
  top: 3px;
  bottom: 3px;
  width: 1.5px;
  border-radius: 1px;
  background: var(--accent);
  opacity: 0;
  transition: opacity 120ms ease;
}
.col-grip:hover::after {
  opacity: 0.55;
}
.cell-num.col-resizable {
  text-align: right;
}
.history-item {
  display: grid;
  grid-template-columns:
    var(--hs-act, 96px)
    minmax(180px, 1fr)
    var(--hs-type, 56px)
    var(--hs-size, 72px);
  gap: 10px;
  align-items: baseline;
  padding: 3px 10px 0 12px;
  min-width: 0;
}
.history-item-act {
  color: var(--text-3);
  min-width: 0;
  overflow: hidden;
}
/* 状态 chip 按内容宽，列拖宽后不跟着拉满 */
.history-item-act .chip {
  max-width: 100%;
  overflow: hidden;
}
.history-item-act.failed {
  color: var(--red-text);
}
/* 文件名跟随界面字体设置（此前固定等宽，改字体设置对它无效） */
.history-item-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.history-item-kind {
  font-size: calc(10.5px * var(--font-scale, 1));
  color: var(--text-3);
  letter-spacing: 0.04em;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  user-select: none;
}
.history-item-size {
  font-variant-numeric: tabular-nums;
  color: var(--text-3);
  text-align: right;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.history-item-err {
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.history-item-err.failed {
  color: var(--red-text);
}
.history-item.hd-row.has-err-col {
  grid-template-columns:
    var(--hy-act, 96px)
    minmax(180px, 1fr)
    var(--hy-type, 56px)
    var(--hy-size, 72px)
    var(--hy-err, 220px);
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
