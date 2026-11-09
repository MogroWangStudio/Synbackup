<script setup lang="ts">
/* 总览显示项：勾掉不要的卡片、给需要轮询的项配刷新频率。
   改动即时保存（没有"确定/取消"——这里没有可丢失的东西，关掉就是完成） */
import { computed, ref } from "vue";
import { api } from "../api";
import { store, toast } from "../store";
import { t } from "../i18n";
import type { AppSettings } from "../types";
import {
  OVERVIEW_INTERVAL_FIELD,
  OVERVIEW_STAT_KEYS,
  OVERVIEW_STAT_LABEL,
  isPolled,
  overviewIntervals,
  type OverviewPolledKey,
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

/** 频率下拉：0 档的措辞按项区分——设备仍会被系统事件推送，其余是真的停更；
    项目总大小的档位是分钟级，别读成"300 秒" */
function intervalOptions(key: OverviewPolledKey): SelectOption[] {
  return overviewIntervals(key).map((ms) => ({
    value: String(ms),
    label:
      ms === 0
        ? t(key === "devices" ? "overview.refresh_event" : "overview.refresh_off")
        : ms >= 60_000
          ? t("overview.minutes", { n: ms / 60_000 })
          : t("overview.seconds", { n: ms / 1000 }),
  }));
}

function intervalOf(key: OverviewPolledKey): string {
  return String(store.settings[OVERVIEW_INTERVAL_FIELD[key]]);
}

/* 模板里按规范顺序遍历全部键，只有可轮询项才渲染下拉；
   下面三个函数把类型收窄放在函数体内，模板不必关心哪一项可轮询 */
function optionsFor(key: OverviewStatKey): SelectOption[] {
  return isPolled(key) ? intervalOptions(key) : [];
}

function valueFor(key: OverviewStatKey): string {
  return isPolled(key) ? intervalOf(key) : "";
}

function setIntervalFor(key: OverviewStatKey, value: string) {
  if (!isPolled(key)) return;
  const patch: Partial<AppSettings> = {};
  patch[OVERVIEW_INTERVAL_FIELD[key]] = Number(value) || 0;
  void save(patch);
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
          :model-value="valueFor(key)"
          :options="optionsFor(key)"
          :disabled="!shown.has(key)"
          :aria-label="`${t(OVERVIEW_STAT_LABEL[key])} · ${t('overview.refresh')}`"
          @update:model-value="(v) => setIntervalFor(key, v)"
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
