<script setup lang="ts">
import { computed } from "vue";
import { store } from "../store";
import { fmtSize, t } from "../i18n";
import Icon from "../components/Icon.vue";
import EmptyState from "../components/EmptyState.vue";
import type { VolumeInfo } from "../types";

const pairNameById = computed(() => {
  const m = new Map<string, string>();
  for (const p of store.pairs) m.set(p.id, p.name);
  return m;
});

function matchedNames(v: VolumeInfo): string[] {
  return v.matchedPairIds.map((id) => pairNameById.value.get(id) ?? id);
}

function usagePct(v: VolumeInfo): number {
  if (!v.total) return 0;
  return Math.min(100, Math.round(((v.total - v.available) / v.total) * 100));
}

/** 存储用量等级：默认紫，80% 起橙黄，90% 起红 */
function usageLevel(v: VolumeInfo): "ok" | "warn" | "high" {
  const pct = usagePct(v);
  if (pct >= 90) return "high";
  if (pct >= 80) return "warn";
  return "ok";
}

/** 同一块物理磁盘上的分区归成一组（后端给出磁盘序号，见 devices.rs） */
interface DiskGroup {
  disk: string;
  volumes: VolumeInfo[];
  /** 组内含有内置盘的分区 */
  fixed: boolean;
  /** 组内含系统盘 */
  system: boolean;
}

const groups = computed<DiskGroup[]>(() => {
  const map = new Map<string, VolumeInfo[]>();
  for (const v of store.volumes) {
    const list = map.get(v.disk) ?? [];
    list.push(v);
    map.set(v.disk, list);
  }
  const out: DiskGroup[] = [];
  for (const [disk, volumes] of map) {
    const sorted = [...volumes].sort((a, b) => a.mount.localeCompare(b.mount));
    out.push({
      disk,
      volumes: sorted,
      fixed: sorted.some((v) => v.kind === "fixed"),
      system: sorted.some((v) => v.system),
    });
  }
  // 系统盘所在的磁盘排最前，其余按磁盘序号（"10" 排在 "9" 之后）
  return out.sort((a, b) => {
    if (a.system !== b.system) return a.system ? -1 : 1;
    return a.disk.localeCompare(b.disk, undefined, { numeric: true });
  });
});

/** 组标题：拿到序号用「磁盘 N」，拿不到时后端退回盘符 / 挂载点，原样显示 */
function diskTitle(g: DiskGroup): string {
  return /^\d+$/.test(g.disk) ? t("devices.disk_n", { n: g.disk }) : g.disk;
}

function kindLabel(g: DiskGroup): string {
  return g.fixed ? t("devices.kind_fixed") : t("devices.kind_removable");
}

/** 断线但存在于同步任务中的设备（按卷标去重），无挂载点信息 */
const offlineDevices = computed(() => {
  const online = new Set(store.volumes.map((v) => v.label));
  const map = new Map<string, string[]>();
  for (const p of store.pairs) {
    if (!p.deviceLabel || online.has(p.deviceLabel)) continue;
    const list = map.get(p.deviceLabel) ?? [];
    list.push(p.name);
    map.set(p.deviceLabel, list);
  }
  return [...map].map(([label, pairs]) => ({ label, pairs }));
});
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t("devices.title") }}</h1>
      </div>
    </div>

    <div v-if="groups.length || offlineDevices.length" class="disk-list">
      <section v-for="g in groups" :key="g.disk" class="disk-group">
        <header class="disk-head">
          <Icon name="drive" :size="14" class="disk-icon" />
          <span class="disk-title">{{ diskTitle(g) }}</span>
          <span class="disk-kind">{{ kindLabel(g) }}</span>
          <span class="disk-count">{{ t("devices.volumes", { n: g.volumes.length }) }}</span>
        </header>

        <div class="device-list">
          <div v-for="v in g.volumes" :key="v.mount" class="card device-row">
            <Icon name="drive" :size="20" class="device-icon" />
            <div class="device-main">
              <div class="device-name">
                {{ v.label || v.mount }}
                <span v-if="v.system" class="chip chip-gray vol-badge">
                  {{ t("devices.system") }}
                </span>
              </div>
              <div class="device-mount">{{ v.mount }} · {{ v.fs }}</div>
              <div class="usage">
                <div class="usage-fill" :class="usageLevel(v)" :style="{ width: usagePct(v) + '%' }" />
              </div>
              <div class="device-cap">
                {{ fmtSize(v.total - v.available) }} / {{ fmtSize(v.total) }}
              </div>
            </div>
            <div class="device-pairs">
              <template v-if="matchedNames(v).length">
                <span class="device-pairs-label">{{ t("devices.matched_pairs") }}</span>
                <span v-for="name in matchedNames(v)" :key="name" class="chip chip-accent">
                  {{ name }}
                </span>
              </template>
              <span v-else class="device-nomatch">{{ t("devices.no_pairs") }}</span>
            </div>
          </div>
        </div>
      </section>

      <!-- 断线但已配对的设备：无盘符与容量，仅标识与关联任务 -->
      <section v-if="offlineDevices.length" class="disk-group">
        <header class="disk-head">
          <Icon name="drive" :size="14" class="disk-icon" />
          <span class="disk-title">{{ t("devices.offline") }}</span>
        </header>

        <div class="device-list">
          <div v-for="d in offlineDevices" :key="d.label" class="card device-row offline">
            <Icon name="drive" :size="20" class="device-icon" />
            <div class="device-main">
              <div class="device-name">{{ d.label }}</div>
            </div>
            <div class="device-pairs">
              <span class="device-pairs-label">{{ t("devices.matched_pairs") }}</span>
              <span v-for="name in d.pairs" :key="name" class="chip chip-gray">
                {{ name }}
              </span>
            </div>
          </div>
        </div>
      </section>
    </div>

    <EmptyState
      v-else
      icon="drive"
      :title="t('devices.none')"
      :desc="t('devices.desc')"
    />
  </div>
</template>

<style scoped>
/* 一块磁盘一组：组间留出明显间隔，组内分区紧挨着——分组靠留白，
   不靠分割线，免得页面上横线密得像表格 */
.disk-list {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.disk-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.disk-head {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0 2px;
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-3);
}
.disk-icon {
  flex: none;
  color: var(--text-3);
}
.disk-title {
  font-weight: 600;
  color: var(--text-2);
  letter-spacing: 0.01em;
}
.disk-kind::before {
  content: "·";
  margin-right: 6px;
}
.disk-count {
  margin-left: auto;
  font-variant-numeric: tabular-nums;
}
.device-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.device-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 14px 16px;
}
.device-icon {
  color: var(--text-3);
  flex: none;
}
.device-main {
  flex: 1;
  min-width: 0;
}
.device-name {
  font-size: calc(13.5px * var(--font-scale, 1));
  font-weight: 600;
}
.vol-badge {
  margin-left: 6px;
  vertical-align: 1px;
}
.device-mount {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
  margin-top: 1px;
}
.usage {
  height: 4px;
  border-radius: 2px;
  background: var(--fill-soft);
  margin-top: 8px;
  max-width: 320px;
}
.usage-fill {
  height: 100%;
  border-radius: 2px;
  /* 按用量比例变色：默认主题紫，80% 起橙黄，90% 起红 */
  background: var(--accent);
  transition: background-color 200ms var(--ease);
}
.usage-fill.warn {
  background: var(--orange);
}
.usage-fill.high {
  background: var(--red);
}
.device-cap {
  margin-top: 4px;
  font-size: calc(11px * var(--font-scale, 1));
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.device-pairs {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 5px;
  flex: none;
  max-width: 220px;
}
.device-pairs-label {
  font-size: calc(11px * var(--font-scale, 1));
  color: var(--text-3);
}
.device-nomatch {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
}
/* 断线设备：整体降不透明度，盘符与容量信息一并不显示 */
.device-row.offline {
  opacity: 0.62;
}
</style>
