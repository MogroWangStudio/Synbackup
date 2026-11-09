<script setup lang="ts">
/* 总览显示项：勾掉不要的卡片、给需要轮询的项配刷新频率。
   改动即时保存（没有"确定/取消"——这里没有可丢失的东西，关掉就是完成） */
import { computed, ref } from "vue";
import { api } from "../api";
import { store, toast } from "../store";
import { t } from "../i18n";
import type { AppSettings } from "../types";
import {
  OVERVIEW_INTERVALS,
  OVERVIEW_POLLED,
  OVERVIEW_STAT_KEYS,
  OVERVIEW_STAT_LABEL,
  type OverviewStatKey,
} from "../overview";
import Modal from "./Modal.vue";
import Select from "./Select.vue";
import Toggle from "./Toggle.vue";
import type { SelectOption } from "./Select.vue";

const emit = defineEmits<{ (e: "close"): void }>();
const modalRef = ref<InstanceType<typeof Modal>>();

/** 当前显示项集合（顺序按规范顺序落地，关掉再打开不会跑到最后） */
const shown = computed(() => new Set(store.settings.overviewStats));
const isPolled = (key: OverviewStatKey) => OVERVIEW_POLLED.includes(key);

/** 频率下拉：0 档的措辞按项区分——设备仍会被系统事件推送，内存则是真的停更 */
function intervalOptions(key: OverviewStatKey): SelectOption[] {
  return OVERVIEW_INTERVALS.map((ms) => ({
    value: String(ms),
    label:
      ms === 0
        ? t(key === "devices" ? "overview.refresh_event" : "overview.refresh_off")
        : t("overview.seconds", { n: ms / 1000 }),
  }));
}

function intervalOf(key: OverviewStatKey): string {
  return String(key === "devices" ? store.settings.overviewDevicesMs : store.settings.overviewMemoryMs);
}

async function save(patch: Partial<AppSettings>) {
  const next = { ...store.settings, ...patch };
  store.settings = next;
  try {
    await api.saveSettings(next);
  } catch (e) {
    toast("error", String(e));
  }
}

function toggle(key: OverviewStatKey, on: boolean) {
  const set = new Set(store.settings.overviewStats);
  if (on) set.add(key);
  else set.delete(key);
  // 只保留已知项、按规范顺序排列
  const next = OVERVIEW_STAT_KEYS.filter((k) => set.has(k));
  void save({ overviewStats: [...next] });
}

function onInterval(key: OverviewStatKey, value: string) {
  const ms = Number(value) || 0;
  void save(key === "devices" ? { overviewDevicesMs: ms } : { overviewMemoryMs: ms });
}

function close() {
  modalRef.value?.requestClose();
}
</script>

<template>
  <Modal ref="modalRef" :title="t('overview.edit_title')" @close="emit('close')">
    <p class="edit-hint">{{ t("overview.edit_hint") }}</p>
    <div class="edit-list">
      <div v-for="key in OVERVIEW_STAT_KEYS" :key="key" class="edit-row">
        <div class="edit-main">
          <span class="edit-name">{{ t(OVERVIEW_STAT_LABEL[key]) }}</span>
          <span class="edit-sub">
            {{ isPolled(key) ? t("overview.refresh") : t("overview.refresh_event") }}
          </span>
        </div>
        <Select
          v-if="isPolled(key)"
          class="edit-interval"
          :model-value="intervalOf(key)"
          :options="intervalOptions(key)"
          :disabled="!shown.has(key)"
          :aria-label="`${t(OVERVIEW_STAT_LABEL[key])} · ${t('overview.refresh')}`"
          @update:model-value="(v) => onInterval(key, v)"
        />
        <Toggle
          :model-value="shown.has(key)"
          :aria-label="t(OVERVIEW_STAT_LABEL[key])"
          @update:model-value="(v) => toggle(key, v)"
        />
      </div>
    </div>

    <template #footer>
      <span />
      <div class="foot-actions">
        <button class="btn btn-primary" @click="close">{{ t("common.close") }}</button>
      </div>
    </template>
  </Modal>
</template>

<style scoped>
.edit-hint {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
  line-height: 1.55;
  padding-bottom: 4px;
}
.edit-list {
  display: flex;
  flex-direction: column;
}
/* 行高与设置页一致：一眼看出"每行一个可开关的项" */
.edit-row {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 46px;
  padding: 7px 0;
  border-bottom: 1px solid var(--hairline);
}
.edit-row:last-child {
  border-bottom: none;
}
.edit-main {
  display: flex;
  flex-direction: column;
  gap: 1px;
  flex: 1;
  min-width: 0;
}
.edit-name {
  font-size: calc(13px * var(--font-scale, 1));
  font-weight: 500;
}
.edit-sub {
  font-size: calc(11px * var(--font-scale, 1));
  color: var(--text-3);
}
/* 频率下拉固定宽度：四行里只有两行有它，宽度不固定会让右侧开关错位 */
.edit-interval {
  width: 126px;
  flex: none;
}
</style>
