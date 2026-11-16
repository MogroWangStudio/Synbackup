<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { api } from "../api";
import { refreshHistory, store, toast } from "../store";
import { fmtDateTime, fmtSize, t } from "../i18n";
import { nudgeColumn, startColumnDrag, statusChipClass } from "../ui";
import { useConfirmAction } from "../composables/useConfirmAction";
import type { HistoryEntry } from "../types";
import Select from "../components/Select.vue";
import Icon from "../components/Icon.vue";
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

/* ---- 明细列宽：所有列都有明确宽度，末尾一条填充列吃掉剩余空间 ----
   扫描明细（状态/文件/类型/大小）与同步明细（动作/文件/类型/大小/错误）
   列语义不同，宽度各自独立成组；同组内跨记录共享。列宽是纯界面偏好，
   存 localStorage。

   为什么不用"文件列吸收剩余"：那样它右侧每一列的右边界都被钉死
   （总宽 - 固定列），拖手柄时边界不动、只有左边界在动，看起来像是
   在拖文件列，文件和类型也就分不开。填充列放在最后，所有业务列的
   边界都严格跟手。scan 组的 err 只是为了两组结构同形，并不渲染。 */
type DetailKind = "scan" | "sync";
type ColKey = "act" | "path" | "type" | "size" | "err";
type Widths = Record<DetailKind, Record<ColKey, number>>;

const COLS_KEY = "historyColWidths2";
const DEFAULT_W: Widths = {
  scan: { act: 92, path: 300, type: 64, size: 76, err: 200 },
  sync: { act: 92, path: 260, type: 64, size: 76, err: 200 },
};
const MIN_W: Record<ColKey, number> = { act: 76, path: 120, type: 48, size: 56, err: 110 };
const MAX_W = 640;

/** 各组实际渲染的列（顺序即表头顺序），err 只在同步明细里出现 */
const COLS: Record<DetailKind, ColKey[]> = {
  scan: ["act", "path", "type", "size"],
  sync: ["act", "path", "type", "size", "err"],
};

function loadWidths(): Widths {
  const out: Widths = { scan: { ...DEFAULT_W.scan }, sync: { ...DEFAULT_W.sync } };
  try {
    const raw = JSON.parse(localStorage.getItem(COLS_KEY) ?? "{}");
    for (const kind of ["scan", "sync"] as DetailKind[]) {
      for (const key of Object.keys(out[kind]) as ColKey[]) {
        const v = raw?.[kind]?.[key];
        if (typeof v === "number" && Number.isFinite(v)) {
          out[kind][key] = Math.max(MIN_W[key], Math.min(MAX_W, Math.round(v)));
        }
      }
    }
  } catch {
    /* 损坏的偏好直接用默认 */
  }
  return out;
}

const widths = reactive(loadWidths());

function saveWidths() {
  try {
    localStorage.setItem(COLS_KEY, JSON.stringify(widths));
  } catch {
    /* 存不了就仅本次生效 */
  }
}

/** 表格最小宽度：各列之和。小于它时容器横向滚动，列宽不被压缩 */
function minWidth(kind: DetailKind): string {
  const w = widths[kind];
  return `${COLS[kind].reduce((n, c) => n + w[c], 0)}px`;
}

/** 表头文案：状态列在检查记录里叫「状态」，在同步记录里叫「动作」 */
function headerKey(kind: DetailKind, col: ColKey): string {
  switch (col) {
    case "act":
      return kind === "scan" ? "diff.col_status" : "history.col_act";
    case "path":
      return "diff.col_path";
    case "type":
      return "diff.col_type";
    case "size":
      return "history.col_size";
    case "err":
      return "history.col_err";
  }
}

/** 列对齐：类型居中、大小右对齐，其余左对齐 */
function thClass(col: ColKey): string {
  if (col === "type") return "hd-center";
  if (col === "size") return "hd-num";
  return "";
}

function resizeOpts(kind: DetailKind, key: ColKey) {
  return {
    width: widths[kind][key],
    min: MIN_W[key],
    max: MAX_W,
    apply: (v: number) => {
      widths[kind][key] = v;
    },
    onEnd: saveWidths,
  };
}

function onGripDown(kind: DetailKind, key: ColKey, e: PointerEvent) {
  startColumnDrag(e, resizeOpts(kind, key));
}

function onGripKey(kind: DetailKind, key: ColKey, e: KeyboardEvent) {
  nudgeColumn(e, resizeOpts(kind, key));
}

/** 双击手柄：该列回到默认宽度（拖坏了不必去翻 localStorage） */
function resetColumn(kind: DetailKind, key: ColKey) {
  widths[kind][key] = DEFAULT_W[kind][key];
  saveWidths();
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

        <!-- 逐项明细：同步记录显示做了什么，检查记录显示发现的差异。
             表格式排布，表头手柄可拖拽调列宽（双击复位、聚焦后方向键微调） -->
        <details
          v-if="h.kind === 'scan' ? (h.scanDiffs?.length ?? 0) > 0 : h.items.length > 0"
          class="history-detail"
        >
          <summary>
            <Icon name="chevron" :size="12" class="hd-caret" />
            <span>{{ h.kind === "scan" ? t("history.scan_detail") : t("history.detail") }}</span>
            <span class="hd-count">{{ h.kind === "scan" ? h.scanDiffs?.length : h.items.length }}</span>
          </summary>

          <div class="hd-scroll">
            <table class="hd-table" :style="{ minWidth: minWidth(h.kind) }">
              <colgroup>
                <col
                  v-for="col in COLS[h.kind]"
                  :key="col"
                  :style="{ width: widths[h.kind][col] + 'px' }"
                />
                <col />
              </colgroup>
              <thead>
                <tr>
                  <th v-for="col in COLS[h.kind]" :key="col" scope="col" :class="thClass(col)">
                    {{ t(headerKey(h.kind, col)) }}
                    <span
                      class="col-grip"
                      role="separator"
                      aria-orientation="vertical"
                      :aria-label="t(headerKey(h.kind, col))"
                      :aria-valuenow="widths[h.kind][col]"
                      :aria-valuemin="MIN_W[col]"
                      :aria-valuemax="MAX_W"
                      tabindex="0"
                      @pointerdown="onGripDown(h.kind, col, $event)"
                      @keydown="onGripKey(h.kind, col, $event)"
                      @dblclick="resetColumn(h.kind, col)"
                    />
                  </th>
                  <!-- 填充列：吃掉业务列之外的剩余宽度。放在最后，前面每一列
                       的右边界都只由自己的宽度决定，拖哪个手柄哪条边界动 -->
                  <th class="hd-fill" />
                </tr>
              </thead>
              <tbody v-if="h.kind === 'scan'">
                <tr v-for="(d, k) in h.scanDiffs" :key="k">
                  <td>
                    <span class="chip" :class="statusChipClass(d.status)" :title="t('st.' + d.status)">
                      {{ t("st." + d.status) }}
                    </span>
                  </td>
                  <td class="hd-path" :title="d.path">{{ d.path }}</td>
                  <td class="hd-kind" :title="kindOf(d.path)">{{ kindOf(d.path) }}</td>
                  <td class="hd-num">{{ d.size ? fmtSize(d.size) : "—" }}</td>
                  <td class="hd-fill" />
                </tr>
              </tbody>
              <tbody v-else>
                <tr v-for="(it, k) in h.items" :key="k">
                  <td class="hd-act" :class="{ failed: !it.ok }">{{ t("act." + it.action) }}</td>
                  <td class="hd-path" :title="it.path">{{ it.path }}</td>
                  <td class="hd-kind" :title="kindOf(it.path)">{{ kindOf(it.path) }}</td>
                  <td class="hd-num">{{ it.size ? fmtSize(it.size) : "—" }}</td>
                  <td class="hd-err" :class="{ failed: !it.ok }">{{ it.ok ? "—" : it.error }}</td>
                  <td class="hd-fill" />
                </tr>
              </tbody>
            </table>
          </div>

          <p
            v-if="h.kind === 'scan' && h.scanTotal > (h.scanDiffs?.length ?? 0)"
            class="history-more"
          >
            {{ t("history.items_more", { n: h.scanTotal - (h.scanDiffs?.length ?? 0) }) }}
          </p>
          <p v-else-if="h.kind !== 'scan' && h.itemsTruncated" class="history-more">
            {{ t("history.items_more", { n: h.itemsTruncated }) }}
          </p>
        </details>

        <!-- 明细被截断时，错误清单仍完整保留，避免关键失败被挤掉 -->
        <details v-if="h.errors.length && h.itemsTruncated" class="history-detail errors">
          <summary>
            <Icon name="chevron" :size="12" class="hd-caret" />
            <span>{{ t("history.errors", { n: h.errors.length }) }}</span>
          </summary>
          <ul class="hd-error-list">
            <li v-for="e in h.errors" :key="e.path">
              <span class="hd-error-path" :title="e.path">{{ e.path }}</span>
              <span class="hd-error-msg" :title="e.message">{{ e.message }}</span>
            </li>
          </ul>
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
/* ---- 明细：折叠头 + 表格 ---- */
.history-detail {
  width: 100%;
  color: var(--text-2);
}
.history-detail summary {
  display: flex;
  align-items: center;
  gap: 6px;
  width: fit-content;
  padding: 3px 0;
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-2);
  cursor: pointer;
  transition: color 120ms var(--ease);
}
/* 原生折叠三角各平台形状不一，统一换成自绘 chevron，展开时转 90° */
.history-detail summary::-webkit-details-marker {
  display: none;
}
.history-detail summary::marker {
  content: "";
}
.hd-caret {
  flex: none;
  transition: transform 160ms var(--ease);
}
.history-detail[open] .hd-caret {
  transform: rotate(90deg);
}
.history-detail summary:hover {
  color: var(--text);
}
.history-detail.errors summary {
  color: var(--red-text);
}
.hd-count {
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.hd-count::before {
  content: "·";
  margin-right: 5px;
}

/* 列拖宽到挤不下文件名时明细横向滚动：文件名不被压没，手柄始终可达 */
.hd-scroll {
  margin: 2px 0 4px;
  overflow-x: auto;
  overflow-y: hidden;
}
/* 严格表格布局：列宽由 colgroup 定死，内容再长也只省略、不撑列 */
.hd-table {
  width: 100%;
  table-layout: fixed;
  border-collapse: collapse;
  font-size: calc(12px * var(--font-scale, 1));
}
.hd-table th {
  position: relative;
  height: 26px;
  padding: 0 8px;
  /* 表头靠一层淡填充与表体分开：只加字重不够，扫描时先认到这里是列名 */
  background: var(--fill-soft);
  border-bottom: 1px solid var(--hairline);
  font-size: calc(10.5px * var(--font-scale, 1));
  font-weight: 600;
  letter-spacing: 0.02em;
  color: var(--text-3);
  text-align: left;
  white-space: nowrap;
}
.hd-table td {
  height: 28px;
  padding: 0 8px;
  border-bottom: 1px solid var(--hairline);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  vertical-align: middle;
}
/* 表头按列序递减层级：手柄伸出自己单元格的那半截必须画在右邻表头之上，
   否则会被右邻的底色盖掉，看着像挂在错误的列边界上 */
.hd-table th:nth-child(1) {
  z-index: 6;
}
.hd-table th:nth-child(2) {
  z-index: 5;
}
.hd-table th:nth-child(3) {
  z-index: 4;
}
.hd-table th:nth-child(4) {
  z-index: 3;
}
.hd-table th:nth-child(5) {
  z-index: 2;
}
.hd-table th:nth-child(6) {
  z-index: 1;
}
/* 填充列：只保留表头底色与行分隔线，不承载内容也不参与列宽调节 */
.hd-table th.hd-fill,
.hd-table td.hd-fill {
  padding: 0;
}
.hd-table th:first-child,
.hd-table td:first-child {
  padding-left: 10px;
}
.hd-table th:nth-last-child(2),
.hd-table td:nth-last-child(2) {
  padding-right: 10px;
}
.hd-table tbody tr:last-child td {
  border-bottom: none;
}
/* 行悬停只高亮单元格，表头保持自己的底色不跟着动 */
.hd-table tbody tr:hover td {
  background: var(--bg-hover);
}
.hd-table .hd-center {
  text-align: center;
}
.hd-table .hd-num {
  text-align: right;
}
/* 文件是这条明细的主角：用主文本色，其余列退到次级灰 */
.hd-table td.hd-path {
  padding-right: 14px;
  color: var(--text);
}
/* 类型是独立一列，不是文件名的尾巴：左侧一道竖线把两者分开，
   值用等宽感的小字并居中，和路径的长文本明显不是一路。
   线比行分隔线重一档——同一个颜色会混在行线里看不出是列边界 */
.hd-table th.hd-center,
.hd-table td.hd-kind {
  border-left: 1px solid var(--hairline-strong);
}
.hd-table td.hd-kind {
  font-size: calc(10.5px * var(--font-scale, 1));
  letter-spacing: 0.04em;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
  text-align: center;
}
.hd-num {
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.hd-act,
.hd-err {
  color: var(--text-3);
}
.hd-table .failed {
  color: var(--red-text);
}
/* 状态 chip 按内容宽：列拖宽后不跟着拉满整列 */
.hd-table td .chip {
  max-width: 100%;
  overflow: hidden;
}

/* 列宽手柄：贴在列右缘，常显一道淡线说明"这里能拖"；
   悬停或键盘聚焦时变强调色，加长加粗。最后一个业务列（填充列之前）
   的手柄收进单元格内，否则那道线会被横向滚动容器裁掉一半 */
.col-grip {
  position: absolute;
  top: 0;
  bottom: 0;
  right: -7px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  cursor: col-resize;
  touch-action: none;
  outline: none;
}
.hd-table th:nth-last-child(2) .col-grip {
  right: 0;
}
.col-grip::after {
  content: "";
  width: 1px;
  height: 12px;
  border-radius: 1px;
  background: var(--hairline-strong);
  transition:
    width 120ms var(--ease),
    height 120ms var(--ease),
    background-color 120ms var(--ease);
}
.col-grip:hover::after,
.col-grip:focus-visible::after {
  width: 2px;
  height: 18px;
  background: var(--accent);
}

/* 截断后的错误清单：路径与原因两栏，不分表头 */
.hd-error-list {
  list-style: none;
  margin: 2px 0 4px;
}
.hd-error-list li {
  display: grid;
  grid-template-columns: minmax(0, 42%) minmax(0, 1fr);
  gap: 12px;
  padding: 5px 10px;
  border-bottom: 1px solid var(--hairline);
  font-size: calc(11.5px * var(--font-scale, 1));
}
.hd-error-list li:last-child {
  border-bottom: none;
}
.hd-error-path,
.hd-error-msg {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.hd-error-path {
  color: var(--text);
}
.hd-error-msg {
  color: var(--red-text);
}
.history-more {
  padding: 6px 0 4px 10px;
  font-size: calc(11.5px * var(--font-scale, 1));
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
