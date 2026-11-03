<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import { refreshConfig, refreshHistory, refreshPending, store, toast } from "../store";
import { fmtDateTime, fmtSize, t } from "../i18n";
import type { ApplyResult, Decision, DiffStatus, SyncAction } from "../types";
import Icon from "./Icon.vue";
import Modal from "./Modal.vue";

const info = store.diffDialog!;

/** 每种差异状态允许的处理方式 */
const actionsByStatus: Record<DiffStatus, SyncAction[]> = {
  new_local: ["local_to_remote", "skip"],
  new_remote: ["remote_to_local", "skip"],
  changed_local: ["local_to_remote", "skip"],
  changed_remote: ["remote_to_local", "skip"],
  both_changed: ["local_to_remote", "remote_to_local", "skip"],
  deleted_remote: ["local_to_remote", "delete_local", "skip"],
  deleted_local: ["remote_to_local", "delete_remote", "skip"],
  type_conflict: ["local_to_remote", "remote_to_local", "skip"],
};

const statusOrder: DiffStatus[] = [
  "both_changed",
  "type_conflict",
  "changed_local",
  "changed_remote",
  "deleted_local",
  "deleted_remote",
  "new_local",
  "new_remote",
];

const decisions = reactive(new Map<string, SyncAction>());
for (const item of info.items) decisions.set(item.path, item.suggested);

const phase = ref<"review" | "syncing" | "done">("review");
const progress = reactive({ done: 0, total: 0, current: "" });
const result = ref<ApplyResult | null>(null);

const items = computed(() =>
  [...info.items].sort((a, b) => {
    const d = statusOrder.indexOf(a.status) - statusOrder.indexOf(b.status);
    return d !== 0 ? d : a.path.localeCompare(b.path);
  }),
);

const counts = computed(() => {
  const c = new Map<DiffStatus, number>();
  for (const i of info.items) c.set(i.status, (c.get(i.status) ?? 0) + 1);
  return statusOrder.filter((s) => c.has(s)).map((s) => ({ status: s, n: c.get(s)! }));
});

const conflictCount = computed(
  () =>
    info.items.filter(
      (i) => i.status === "both_changed" || i.status === "type_conflict",
    ).length,
);

function statusClass(status: DiffStatus): string {
  switch (status) {
    case "both_changed":
    case "type_conflict":
      return "chip-red";
    case "changed_local":
    case "changed_remote":
      return "chip-orange";
    case "new_local":
    case "new_remote":
      return "chip-green";
    default:
      return "chip-gray";
  }
}

function arrowFor(status: DiffStatus): string {
  switch (status) {
    case "new_local":
    case "changed_local":
    case "deleted_remote":
      return "arrow_r";
    case "new_remote":
    case "changed_remote":
    case "deleted_local":
      return "arrow_l";
    default:
      return "alert";
  }
}

function applyBulk(kind: "suggest" | "l2r" | "r2l" | "skip") {
  for (const item of info.items) {
    const allowed = actionsByStatus[item.status];
    let action: SyncAction = "skip";
    if (kind === "suggest") action = item.suggested;
    else if (kind === "l2r" && allowed.includes("local_to_remote")) action = "local_to_remote";
    else if (kind === "r2l" && allowed.includes("remote_to_local")) action = "remote_to_local";
    decisions.set(item.path, action);
  }
}

function setAction(path: string, ev: Event) {
  decisions.set(path, (ev.target as HTMLSelectElement).value as SyncAction);
}

let unlisten: (() => void) | null = null;
onMounted(async () => {
  unlisten = await listen<{ pairId: string; done: number; total: number; current: string }>(
    "sync-progress",
    (e) => {
      if (e.payload.pairId !== info.pairId) return;
      progress.done = e.payload.done;
      progress.total = e.payload.total;
      progress.current = e.payload.current;
    },
  );
});
onBeforeUnmount(() => unlisten?.());

async function startSync() {
  if (phase.value !== "review") return;
  phase.value = "syncing";
  store.syncingPairIds.push(info.pairId);
  const list: Decision[] = [...decisions.entries()].map(([path, action]) => ({ path, action }));
  try {
    result.value = await api.applyDecisions(info.pairId, list);
    // 命令返回是"处理完成"的权威信号：这里主动刷新总览数据，
    // 不依赖 sync-finished 事件投递（窗口不可见时 eval 可能延迟或丢失）。
    await Promise.all([refreshPending(), refreshHistory(), refreshConfig()]);
    phase.value = "done";
  } catch (e) {
    toast("error", String(e));
    phase.value = "review";
  } finally {
    const i = store.syncingPairIds.indexOf(info.pairId);
    if (i >= 0) store.syncingPairIds.splice(i, 1);
  }
}

async function close() {
  if (phase.value === "syncing") return;
  if (info.source === "pending") {
    try {
      await api.dismissPendingReview(info.pairId);
      await refreshPending();
    } catch {
      /* 忽略 */
    }
  }
  store.diffDialog = null;
  // 无论入口来源，关闭时兜底刷新一次（未处理就关闭 = 后端 dismiss 已移除，
  // 已处理 = startSync 已刷新；再刷一次保证总览 / 任务卡片的计数一致）
  void refreshPending();
  void refreshConfig();
  void refreshHistory();
}
</script>

<template>
  <Modal
    :title="t('diff.title')"
    wide
    :busy="phase === 'syncing'"
    @close="close"
  >
    <div class="diff-meta">
      <span class="diff-pair">{{ info.pairName }}</span>
      <span class="diff-device">
        <Icon name="drive" :size="13" />
        {{ info.deviceLabel }}
      </span>
      <span class="diff-time">{{ t("diff.scanned_at") }} {{ fmtDateTime(info.scannedAt) }}</span>
      <span class="diff-total">{{ t("diff.all_counts", { n: info.items.length }) }}</span>
    </div>

    <!-- 状态统计 -->
    <div class="diff-counts">
      <span v-for="c in counts" :key="c.status" class="chip" :class="statusClass(c.status)">
        {{ t(`st.${c.status}`) }} {{ c.n }}
      </span>
    </div>

    <!-- 审阅阶段 -->
    <template v-if="phase === 'review'">
      <div class="diff-list">
        <div class="diff-head">
          <span>{{ t("diff.col_status") }}</span>
          <span>{{ t("diff.col_path") }}</span>
          <span class="cell-num">{{ t("diff.col_local") }}</span>
          <span />
          <span class="cell-num">{{ t("diff.col_remote") }}</span>
          <span>{{ t("diff.col_action") }}</span>
        </div>
        <div v-for="item in items" :key="item.path" class="diff-row">
          <span class="chip" :class="statusClass(item.status)">
            {{ t(`st.${item.status}`) }}
          </span>
          <span class="diff-path" :title="item.path">{{ item.path }}</span>
          <span
            class="cell-num"
            :title="item.local ? fmtDateTime(item.local.mtimeMs) : ''"
          >
            {{ item.local ? fmtSize(item.local.size) : "—" }}
          </span>
          <span class="diff-arrow">
            <Icon :name="arrowFor(item.status)" :size="13" />
          </span>
          <span
            class="cell-num"
            :title="item.remote ? fmtDateTime(item.remote.mtimeMs) : ''"
          >
            {{ item.remote ? fmtSize(item.remote.size) : "—" }}
          </span>
          <select
            class="input diff-action"
            :value="decisions.get(item.path)"
            @change="setAction(item.path, $event)"
          >
            <option
              v-for="a in actionsByStatus[item.status]"
              :key="a"
              :value="a"
            >
              {{ t(`act.${a}`) }}
            </option>
          </select>
        </div>
      </div>
    </template>

    <!-- 同步进行中 -->
    <template v-else-if="phase === 'syncing'">
      <div class="syncing">
        <div class="progress-track">
          <div
            class="progress-fill"
            :style="{ transform: `scaleX(${progress.total ? progress.done / progress.total : 0})` }"
          />
        </div>
        <p class="progress-text">
          {{ t("diff.progress") }} · {{ progress.done }} / {{ progress.total }}
        </p>
        <p class="progress-file">{{ progress.current }}</p>
      </div>
    </template>

    <!-- 完成 -->
    <template v-else>
      <div class="done">
        <Icon name="check" :size="28" class="done-icon" />
        <p class="done-title">{{ t("diff.done") }}</p>
        <div class="done-chips">
          <span v-if="result?.copiedToRemote" class="chip chip-accent">
            {{ t("diff.summary_copied_r", { n: result.copiedToRemote }) }}
          </span>
          <span v-if="result?.copiedToLocal" class="chip chip-accent">
            {{ t("diff.summary_copied_l", { n: result.copiedToLocal }) }}
          </span>
          <span v-if="result?.deletedRemote" class="chip chip-gray">
            {{ t("diff.summary_deleted_r", { n: result.deletedRemote }) }}
          </span>
          <span v-if="result?.deletedLocal" class="chip chip-gray">
            {{ t("diff.summary_deleted_l", { n: result.deletedLocal }) }}
          </span>
          <span v-if="result?.skipped" class="chip chip-gray">
            {{ t("diff.summary_skipped", { n: result.skipped }) }}
          </span>
          <span v-if="result?.errors.length" class="chip chip-red">
            {{ t("diff.summary_failed", { n: result.errors.length }) }}
          </span>
        </div>
        <div v-if="result?.errors.length" class="done-errors">
          <div v-for="e in result.errors" :key="e.path" class="done-error">
            <span class="done-error-path">{{ e.path }}</span>
            <span class="done-error-msg">{{ e.message }}</span>
          </div>
        </div>
      </div>
    </template>

    <template #footer>
      <template v-if="phase === 'review'">
        <div class="bulk">
          <button class="btn btn-sm" @click="applyBulk('suggest')">
            {{ t("diff.bulk_suggest") }}
          </button>
          <button class="btn btn-sm" @click="applyBulk('l2r')">
            {{ t("diff.bulk_l2r") }}
          </button>
          <button class="btn btn-sm" @click="applyBulk('r2l')">
            {{ t("diff.bulk_r2l") }}
          </button>
          <button class="btn btn-sm" @click="applyBulk('skip')">
            {{ t("diff.bulk_skip") }}
          </button>
        </div>
        <div class="foot-actions">
          <button class="btn" @click="close">{{ t("diff.cancel") }}</button>
          <button class="btn btn-primary" @click="startSync">
            {{
              conflictCount > 0
                ? `${t("diff.start")}（${t("diff.conflict_hint", { n: conflictCount })}）`
                : t("diff.start")
            }}
          </button>
        </div>
      </template>
      <template v-else-if="phase === 'syncing'">
        <span />
        <span class="syncing-hint">{{ t("diff.syncing_hint") }}</span>
      </template>
      <template v-else>
        <span />
        <button class="btn btn-primary" @click="close">{{ t("diff.close") }}</button>
      </template>
    </template>
  </Modal>
</template>

<style scoped>
.diff-meta {
  display: flex;
  align-items: baseline;
  gap: 12px;
  flex-wrap: wrap;
}
.diff-pair {
  font-size: calc(14px * var(--font-scale, 1));
  font-weight: 600;
}
.diff-device {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-2);
}
.diff-time {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
}
.diff-total {
  margin-left: auto;
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-2);
  font-variant-numeric: tabular-nums;
}

.diff-counts {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  margin: 10px 0 12px;
}

.diff-list {
  border: 1px solid var(--hairline);
  border-radius: var(--radius-m);
  overflow: auto;
  max-height: 380px;
}
.diff-head,
.diff-row {
  display: grid;
  grid-template-columns: 96px minmax(0, 1fr) 76px 24px 76px 128px;
  gap: 10px;
  align-items: center;
  padding: 0 12px;
}
.diff-head {
  position: sticky;
  top: 0;
  z-index: 1;
  height: 32px;
  font-size: calc(11px * var(--font-scale, 1));
  font-weight: 600;
  color: var(--text-2);
  background: var(--bg-content);
  border-bottom: 1px solid var(--hairline);
}
.diff-row {
  min-height: 40px;
  padding-top: 5px;
  padding-bottom: 5px;
  border-bottom: 1px solid var(--hairline);
  font-size: calc(12.5px * var(--font-scale, 1));
}
.diff-row:last-child {
  border-bottom: none;
}
.diff-row:hover {
  background: var(--bg-hover);
}
.diff-row .chip {
  justify-content: center;
}
.diff-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--mono);
  font-size: calc(11.5px * var(--font-scale, 1));
}
.cell-num {
  text-align: right;
  color: var(--text-2);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.diff-arrow {
  color: var(--text-3);
  display: inline-flex;
  justify-content: center;
}
.diff-action {
  height: 26px;
  font-size: calc(12px * var(--font-scale, 1));
  width: 100%;
}

.syncing {
  padding: 36px 8px;
}
.progress-track {
  height: 6px;
  border-radius: 3px;
  background: var(--fill-soft);
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  border-radius: 3px;
  background: var(--accent);
  transform-origin: 0 50%;
  transition: transform 200ms var(--ease);
}
.progress-text {
  margin-top: 12px;
  font-size: calc(12.5px * var(--font-scale, 1));
  color: var(--text-2);
  font-variant-numeric: tabular-nums;
}
.progress-file {
  margin-top: 4px;
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
  font-family: var(--mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.done {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 28px 8px 20px;
}
.done-icon {
  color: var(--green);
}
.done-title {
  font-size: calc(15px * var(--font-scale, 1));
  font-weight: 600;
}
.done-chips {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  justify-content: center;
}
.done-errors {
  width: 100%;
  max-height: 140px;
  overflow-y: auto;
  border: 1px solid var(--red-soft);
  background: var(--red-soft);
  border-radius: var(--radius-m);
  padding: 8px 12px;
  margin-top: 6px;
}
.done-error {
  display: flex;
  gap: 10px;
  font-size: calc(11.5px * var(--font-scale, 1));
  padding: 3px 0;
}
.done-error-path {
  font-family: var(--mono);
  color: var(--text);
  flex: none;
  max-width: 45%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.done-error-msg {
  color: var(--red-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bulk {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.foot-actions {
  display: flex;
  gap: 8px;
  margin-left: auto;
}
.syncing-hint {
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-2);
}
</style>
