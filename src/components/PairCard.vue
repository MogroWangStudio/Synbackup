<script setup lang="ts">
import { computed } from "vue";
import { api } from "../api";
import { refreshConfig, refreshPending, store, toast } from "../store";
import { fmtTime, t } from "../i18n";
import { useConfirmAction } from "../composables/useConfirmAction";
import type { SyncPair } from "../types";
import Icon from "./Icon.vue";

const props = defineProps<{ pair: SyncPair }>();

const connected = computed(() =>
  store.volumes.some((v) => v.matchedPairIds.includes(props.pair.id)),
);
const pendingCount = computed(
  () => store.pending.find((p) => p.pairId === props.pair.id)?.items.length ?? 0,
);
const syncing = computed(() => store.syncingPairIds.includes(props.pair.id));

const statusText = computed(() => {
  if (syncing.value) return t("pairs.syncing");
  if (!props.pair.enabled) return t("pairs.disabled");
  if (pendingCount.value > 0) return t("pairs.pending_count", { n: pendingCount.value });
  return connected.value ? t("pairs.device_online") : t("pairs.device_offline");
});
const statusClass = computed(() => {
  if (syncing.value) return "chip-accent";
  if (!props.pair.enabled) return "chip-gray";
  if (pendingCount.value > 0) return "chip-orange";
  return connected.value ? "chip-green" : "chip-gray";
});

const remoteLabel = computed(() =>
  props.pair.remotePath
    ? `${props.pair.deviceLabel} / ${props.pair.remotePath}`
    : `${props.pair.deviceLabel} / ${t("editor.remote_root")}`,
);

function removePair() {
  api
    .deletePair(props.pair.id)
    .then(async () => {
      await Promise.all([refreshConfig(), refreshPending()]);
      toast("success", t("toast.pair_deleted"));
    })
    .catch((e) => toast("error", String(e)));
}

const { confirming: confirmingDelete, confirm: onDelete } = useConfirmAction(removePair);

async function onCheck() {
  try {
    const review = await api.scanPair(props.pair.id);
    await refreshPending();
    if (review.items.length === 0) {
      toast("success", t("toast.uptodate"));
      return;
    }
    store.diffDialog = { ...review, source: "manual" };
  } catch (e) {
    toast("error", String(e));
  }
}

function onReview() {
  const review = store.pending.find((p) => p.pairId === props.pair.id);
  if (review) store.diffDialog = { ...review, source: "pending" };
}

function onEdit() {
  store.editor = { pair: props.pair };
}

function onToggleEnabled() {
  api
    .setPairEnabled(props.pair.id, !props.pair.enabled)
    .then(refreshConfig)
    .catch((e) => toast("error", String(e)));
}
</script>

<template>
  <div class="card pair-card">
    <div class="pair-main">
      <div class="pair-name-row">
        <span class="pair-name">{{ pair.name }}</span>
        <span class="chip" :class="statusClass">{{ statusText }}</span>
      </div>
      <div class="pair-route" :title="pair.localPath">
        <Icon name="folder" :size="14" class="route-icon" />
        <span class="route-text">{{ pair.localPath }}</span>
        <Icon
          :name="pair.direction === 'pull' ? 'arrow_l' : 'arrow_r'"
          :size="12"
          class="route-arrow"
        />
        <Icon name="drive" :size="14" class="route-icon" />
        <span class="route-text">{{ remoteLabel }}</span>
      </div>
      <div v-if="pair.lastSyncAt" class="pair-meta">
        {{ t("pairs.last_sync") }} · {{ fmtTime(pair.lastSyncAt) }}
      </div>
    </div>

    <div class="pair-actions">
      <button
        v-if="pendingCount > 0"
        class="btn btn-sm btn-primary"
        :title="t('pairs.review')"
        :aria-label="t('pairs.review')"
        @click="onReview"
      >
        <Icon name="alert" :size="14" />
        <span class="btn-label">{{ t("pairs.review") }}</span>
      </button>
      <button
        class="btn btn-sm btn-primary"
        :disabled="syncing"
        :title="t('pairs.check')"
        :aria-label="t('pairs.check')"
        @click="onCheck"
      >
        <Icon name="sync" :size="14" />
        <span class="btn-label">{{ t("pairs.check") }}</span>
      </button>
      <button
        class="btn btn-sm"
        :title="pair.enabled ? t('pairs.disable') : t('pairs.enable')"
        :aria-label="pair.enabled ? t('pairs.disable') : t('pairs.enable')"
        @click="onToggleEnabled"
      >
        <Icon :name="pair.enabled ? 'close' : 'check'" :size="14" />
        <span class="btn-label">{{ pair.enabled ? t("pairs.disable") : t("pairs.enable") }}</span>
      </button>
      <button
        class="btn btn-sm"
        :title="t('pairs.edit')"
        :aria-label="t('pairs.edit')"
        @click="onEdit"
      >
        <Icon name="edit" :size="14" />
        <span class="btn-label">{{ t("pairs.edit") }}</span>
      </button>
      <button
        class="btn btn-sm btn-danger"
        :title="t('pairs.delete')"
        :aria-label="t('pairs.delete')"
        @click="onDelete"
      >
        <Icon name="trash" :size="14" />
        <span class="btn-label" :class="{ 'keep-text': confirmingDelete }">
          {{ confirmingDelete ? t("pairs.delete_confirm") : t("pairs.delete") }}
        </span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.pair-card {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 14px 16px;
  transition: border-color 140ms ease, box-shadow 140ms ease;
}
.pair-card:hover {
  border-color: var(--hairline-strong);
}

.pair-main {
  flex: 1;
  min-width: 0;
}
.pair-name-row {
  display: flex;
  align-items: center;
  gap: 9px;
}
.pair-name {
  font-size: calc(14px * var(--font-scale, 1));
  font-weight: 600;
  letter-spacing: -0.01em;
}

.pair-route {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 5px;
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-2);
  min-width: 0;
}
.route-icon {
  flex: none;
  color: var(--text-3);
}
.route-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.route-arrow {
  flex: none;
  color: var(--text-3);
}

.pair-meta {
  margin-top: 5px;
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
}

.pair-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: none;
}
.pair-actions .btn svg {
  flex: none;
}

/* 按钮显示模式（data-task-btn 由 theme.ts 写入）：
   仅图标隐藏文字，仅文字隐藏图标；删除确认态始终保留文字 */
:root[data-task-btn="icon"] .pair-actions .btn-label {
  display: none;
}
:root[data-task-btn="icon"] .pair-actions .btn-label.keep-text {
  display: inline;
}
:root[data-task-btn="text"] .pair-actions .btn svg {
  display: none;
}
</style>
