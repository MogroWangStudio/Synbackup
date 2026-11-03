<script setup lang="ts">
import { api } from "../api";
import { store, toast } from "../store";
import { applyUiPrefs, persistUiPrefs } from "../theme";
import { t } from "../i18n";
import type { AppSettings } from "../types";
import Icon from "../components/Icon.vue";
import Logo from "../components/Logo.vue";
import Toggle from "../components/Toggle.vue";
import Stepper from "../components/Stepper.vue";

/** 强调色预设：key 与 tokens.css 的 data-accent 对应，color 用于色块展示 */
const accents = [
  { key: "brand", color: "#9b62cf" },
  { key: "blue", color: "#4a7dd8" },
  { key: "green", color: "#3e9b6f" },
  { key: "orange", color: "#d97b3e" },
  { key: "pink", color: "#d05b8c" },
] as const;

const appearances = ["follow", "light", "dark"] as const;
const fonts = ["system", "serif", "rounded"] as const;

function openStudioSite() {
  void api.openExternal("https://www.mogrowangstudio.top").catch((e) => toast("error", String(e)));
}

async function update(patch: Partial<AppSettings>) {
  const next = { ...store.settings, ...patch };
  store.settings = next;
  // 外观 / 字体 / 缩放先落 DOM 再存后端，切换即时可见
  applyUiPrefs(next);
  persistUiPrefs(next);
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
          <span class="row-name">{{ t("settings.update_channel") }}</span>
          <span class="row-hint">{{ t("settings.update_channel_hint") }}</span>
        </div>
        <select
          class="input"
          :value="store.settings.updateChannel"
          @change="update({ updateChannel: ($event.target as HTMLSelectElement).value })"
        >
          <option value="stable">{{ t("channel.stable") }}</option>
          <option value="beta">{{ t("channel.beta") }}</option>
        </select>
      </div>
    </section>

    <section class="card group">
      <h3 class="group-title">{{ t("settings.personalize") }}</h3>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.accent") }}</span>
        </div>
        <div class="swatches" role="radiogroup" :aria-label="t('settings.accent')">
          <button
            v-for="sw in accents"
            :key="sw.key"
            type="button"
            class="swatch"
            :class="{ active: store.settings.accent === sw.key }"
            :style="{ '--swatch': sw.color }"
            :aria-checked="store.settings.accent === sw.key"
            role="radio"
            :title="t(`accent.${sw.key}`)"
            :aria-label="t(`accent.${sw.key}`)"
            @click="update({ accent: sw.key })"
          />
        </div>
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.appearance") }}</span>
        </div>
        <select
          class="input"
          :value="store.settings.appearance"
          @change="update({ appearance: ($event.target as HTMLSelectElement).value })"
        >
          <option v-for="a in appearances" :key="a" :value="a">{{ t(`appearance.${a}`) }}</option>
        </select>
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.font") }}</span>
        </div>
        <select
          class="input"
          :value="store.settings.fontFamily"
          @change="update({ fontFamily: ($event.target as HTMLSelectElement).value })"
        >
          <option v-for="f in fonts" :key="f" :value="f">{{ t(`font.${f}`) }}</option>
        </select>
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.font_size") }}</span>
        </div>
        <Stepper
          :model-value="store.settings.fontScale"
          :step="0.05"
          :min="0.9"
          :max="1.15"
          :format="(v) => `${Math.round(v * 100)}%`"
          :decrease-label="t('settings.size_decrease')"
          :increase-label="t('settings.size_increase')"
          @update:model-value="(v) => update({ fontScale: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.ui_size") }}</span>
        </div>
        <Stepper
          :model-value="store.settings.uiScale"
          :step="0.05"
          :min="0.85"
          :max="1.25"
          :format="(v) => `${Math.round(v * 100)}%`"
          :decrease-label="t('settings.size_decrease')"
          :increase-label="t('settings.size_increase')"
          @update:model-value="(v) => update({ uiScale: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.launch_anim") }}</span>
          <span class="row-hint">{{ t("settings.launch_anim_hint") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.launchAnimation"
          @update:model-value="(v) => update({ launchAnimation: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.status_bar") }}</span>
          <span class="row-hint">{{ t("settings.status_bar_hint") }}</span>
        </div>
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.status_devices") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.statusDevices"
          @update:model-value="(v) => update({ statusDevices: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.status_pairs") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.statusPairs"
          @update:model-value="(v) => update({ statusPairs: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.status_sync") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.statusSyncCount"
          @update:model-value="(v) => update({ statusSyncCount: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.status_version") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.statusVersion"
          @update:model-value="(v) => update({ statusVersion: v })"
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
      <div class="brand-mark">
        <Logo variant="wordmark" :size="32" />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.version") }}</span>
        </div>
        <span class="row-value">{{ store.appInfo?.version ?? "0.3.0" }}</span>
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
      <div class="dev-section">
        <h3 class="group-title">{{ t("settings.developer") }}</h3>
        <a
          href="https://www.mogrowangstudio.top"
          class="studio-link"
          @click.prevent="openStudioSite"
        >
          <img src="/studio-logo.svg" alt="MogroWang Studio" class="studio-logo" draggable="false" />
          <span class="studio-hint">{{ t("settings.studio_hint") }}</span>
        </a>
        <p class="dev-copyright">© 2026 MogroWang Studio</p>
      </div>
    </section>
  </div>
</template>

<style scoped>
/* 强调色色块：单色圆点，选中态用同色外环标记 */
.swatches {
  display: flex;
  gap: 8px;
}
.swatch {
  width: 22px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: var(--swatch);
  cursor: pointer;
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.08);
  transition:
    transform 120ms var(--ease),
    box-shadow 120ms var(--ease);
}
.swatch:hover {
  transform: scale(1.1);
}
.swatch:active {
  transform: scale(0.96);
}
.swatch.active {
  box-shadow:
    inset 0 0 0 1px rgba(0, 0, 0, 0.08),
    0 0 0 2px var(--bg-content),
    0 0 0 4px var(--swatch);
}
.swatch:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.group {
  padding: 4px 16px;
  margin-bottom: 14px;
}
.group-title {
  font-size: calc(12px * var(--font-scale, 1));
  font-weight: 600;
  color: var(--text-2);
  padding: 10px 0 4px;
}
.brand-mark {
  display: flex;
  justify-content: center;
  padding: 16px 0;
  border-bottom: 1px solid var(--hairline);
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
  font-size: calc(13px * var(--font-scale, 1));
  font-weight: 500;
}
.row-hint {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
}
.row-value {
  font-size: calc(12.5px * var(--font-scale, 1));
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
  font-size: calc(11px * var(--font-scale, 1));
}
.num {
  width: 76px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

/* ---- 开发者 ---- */
.dev-section {
  padding: 8px 0 2px;
}
/* 悬停时字标模糊变淡、浮现引导文字；点击经系统浏览器打开工作室网站 */
.studio-link {
  position: relative;
  display: block;
  width: fit-content;
  margin: 14px auto 0;
  cursor: pointer;
}
.studio-logo {
  display: block;
  height: 24px;
  max-width: 70vw;
  user-select: none;
  transition:
    filter 200ms var(--ease),
    opacity 200ms var(--ease);
}
.studio-link:hover .studio-logo,
.studio-link:focus-visible .studio-logo {
  opacity: 0.7;
  filter: blur(3px);
}
.studio-hint {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: calc(12px * var(--font-scale, 1));
  font-weight: 500;
  color: var(--text);
  white-space: nowrap;
  opacity: 0;
  transition: opacity 200ms var(--ease);
  pointer-events: none;
}
.studio-link:hover .studio-hint,
.studio-link:focus-visible .studio-hint {
  opacity: 1;
}
.studio-link:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 4px;
  border-radius: 4px;
}
.dev-copyright {
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--hairline);
  text-align: center;
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-3);
}
</style>
