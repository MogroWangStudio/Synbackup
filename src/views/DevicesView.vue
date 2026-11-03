<script setup lang="ts">
import { computed } from "vue";
import { store } from "../store";
import { fmtSize, t } from "../i18n";
import Icon from "../components/Icon.vue";
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
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t("devices.title") }}</h1>
        <p class="page-desc">{{ t("devices.desc") }}</p>
      </div>
    </div>

    <div v-if="store.volumes.length" class="device-list">
      <div v-for="v in store.volumes" :key="v.mount" class="card device-row">
        <Icon name="drive" :size="20" class="device-icon" />
        <div class="device-main">
          <div class="device-name">{{ v.label || v.mount }}</div>
          <div class="device-mount">{{ v.mount }} · {{ v.fs }}</div>
          <div class="usage">
            <div class="usage-fill" :style="{ width: usagePct(v) + '%' }" />
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

    <div v-else class="card empty">
      <Icon name="drive" :size="26" class="empty-icon" />
      <p class="empty-title">{{ t("devices.none") }}</p>
      <p class="empty-desc">{{ t("devices.desc") }}</p>
    </div>
  </div>
</template>

<style scoped>
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
  font-size: 13.5px;
  font-weight: 600;
}
.device-mount {
  font-size: 11.5px;
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
  background: var(--text-3);
}
.device-cap {
  margin-top: 4px;
  font-size: 11px;
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
  font-size: 11px;
  color: var(--text-3);
}
.device-nomatch {
  font-size: 11.5px;
  color: var(--text-3);
}
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 40px 20px;
  text-align: center;
}
.empty-icon {
  color: var(--text-3);
}
.empty-title {
  font-size: 14px;
  font-weight: 600;
}
.empty-desc {
  font-size: 12.5px;
  color: var(--text-2);
  max-width: 340px;
}
</style>
