<script setup lang="ts">
import { api } from "../api";
import { store, toast } from "../store";
import { t } from "../i18n";
import type { AppSettings } from "../types";
import Icon from "../components/Icon.vue";
import Logo from "../components/Logo.vue";
import Toggle from "../components/Toggle.vue";

async function update(patch: Partial<AppSettings>) {
  const next = { ...store.settings, ...patch };
  store.settings = next;
  try {
    await api.saveSettings(next);
  } catch (e) {
    toast("error", String(e));
    return;
  }
  if (patch.language) {
    document.documentElement.lang = next.language === "en-US" ? "en" : "zh-CN";
    toast("info", t("settings.tray_hint"));
  }
}

function onLanguage(ev: Event) {
  void update({ language: (ev.target as HTMLSelectElement).value });
}

function onPollInterval(ev: Event) {
  const raw = Number((ev.target as HTMLInputElement).value);
  const clamped = Number.isFinite(raw) ? Math.max(1, Math.min(3600, Math.round(raw))) : 3;
  (ev.target as HTMLInputElement).value = String(clamped);
  void update({ pollIntervalSecs: clamped });
}
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t("settings.title") }}</h1>
      </div>
    </div>

    <section class="card group">
      <h3 class="group-title">{{ t("settings.general") }}</h3>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.language") }}</span>
        </div>
        <select class="input" :value="store.settings.language" @change="onLanguage">
          <option value="zh-CN">简体中文</option>
          <option value="en-US">English</option>
        </select>
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.launch") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.launchAtLogin"
          @update:model-value="(v) => update({ launchAtLogin: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.poll") }}</span>
          <span class="row-hint">{{ t("settings.poll_hint") }}</span>
        </div>
        <input
          type="number"
          class="input num"
          min="1"
          max="3600"
          :value="store.settings.pollIntervalSecs"
          @change="onPollInterval"
        />
      </div>
    </section>

    <section class="card group">
      <h3 class="group-title">{{ t("settings.sync_section") }}</h3>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.auto_scan") }}</span>
          <span class="row-hint">{{ t("settings.auto_scan_hint") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.autoScanOnInsert"
          @update:model-value="(v) => update({ autoScanOnInsert: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.auto_sync") }}</span>
          <span class="row-hint">{{ t("settings.auto_sync_hint") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.autoSyncNoConflict"
          @update:model-value="(v) => update({ autoSyncNoConflict: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.notify") }}</span>
          <span class="row-hint">{{ t("settings.notify_hint") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.notifyOnInsert"
          @update:model-value="(v) => update({ notifyOnInsert: v })"
        />
      </div>
    </section>

    <section class="card group">
      <h3 class="group-title">{{ t("settings.about") }}</h3>
      <div class="brand-lockup">
        <Logo variant="wordmark" :size="28" />
        <span class="brand-ver">
          {{ t("settings.version") }} {{ store.appInfo?.version ?? "0.2.0" }}
        </span>
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.data") }}</span>
          <span class="row-hint">{{ t("settings.data_hint") }}</span>
        </div>
        <span class="row-value mono" :title="store.appInfo?.dataDir">
          <Icon name="folder" :size="13" />
          {{ store.appInfo?.dataDir }}
        </span>
      </div>
    </section>
  </div>
</template>

<style scoped>
.group {
  padding: 4px 16px;
  margin-bottom: 14px;
}
.group-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-2);
  padding: 10px 0 4px;
}
.brand-lockup {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 8px 0 12px;
  border-bottom: 1px solid var(--hairline);
}
.brand-ver {
  font-size: 12px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  min-height: 44px;
  padding: 7px 0;
  border-bottom: 1px solid var(--hairline);
}
.group .row:last-child {
  border-bottom: none;
}
.row-label {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.row-name {
  font-size: 13px;
  font-weight: 500;
}
.row-hint {
  font-size: 11.5px;
  color: var(--text-3);
}
.row-value {
  font-size: 12.5px;
  color: var(--text-2);
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 55%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-value.mono {
  font-family: var(--mono);
  font-size: 11px;
}
.num {
  width: 76px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}
</style>
