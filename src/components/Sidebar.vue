<script setup lang="ts">
import { computed } from "vue";
import { store } from "../store";
import { t } from "../i18n";
import Icon from "./Icon.vue";

const items = computed(() => [
  { key: "overview", icon: "overview", label: t("nav.overview"), badge: 0 },
  {
    key: "pairs",
    icon: "sync",
    label: t("nav.pairs"),
    badge: store.pending.reduce((n, p) => n + p.items.length, 0),
  },
  { key: "devices", icon: "drive", label: t("nav.devices"), badge: 0 },
  { key: "history", icon: "history", label: t("nav.history"), badge: 0 },
  { key: "settings", icon: "settings", label: t("nav.settings"), badge: 0 },
]);
</script>

<template>
  <aside class="sidebar">
    <div class="brand">
      <img src="../assets/app-icon.png" alt="" class="brand-icon" draggable="false" />
      <div class="brand-text">
        <div class="brand-name">同备</div>
        <div class="brand-sub">Synbackup</div>
      </div>
    </div>

    <nav class="nav">
      <button
        v-for="item in items"
        :key="item.key"
        class="nav-item"
        :class="{ active: store.view === item.key }"
        @click="store.view = item.key as any"
      >
        <Icon :name="item.icon" :size="17" />
        <span class="nav-label">{{ item.label }}</span>
        <span v-if="item.badge > 0" class="nav-badge">{{ item.badge }}</span>
      </button>
    </nav>

    <div class="sidebar-foot">
      <span class="dot" :class="store.volumes.length ? 'ok' : 'idle'" />
      <span class="foot-text">
        {{
          store.volumes.length
            ? t("status.devices_connected", { n: store.volumes.length })
            : t("status.no_devices")
        }}
      </span>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  width: 212px;
  flex: none;
  padding: 16px 10px 14px;
  background: var(--bg-sidebar);
  backdrop-filter: blur(24px) saturate(1.6);
  border-right: 1px solid var(--hairline);
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 2px 8px 14px;
}
.brand-icon {
  width: 34px;
  height: 34px;
  border-radius: 8px;
}
.brand-name {
  font-size: 15px;
  font-weight: 700;
  letter-spacing: -0.01em;
  line-height: 1.2;
}
.brand-sub {
  font-size: 11px;
  color: var(--text-2);
  letter-spacing: 0.02em;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 9px;
  height: 32px;
  padding: 0 9px;
  border: none;
  border-radius: var(--radius-s);
  background: transparent;
  color: var(--text);
  font-size: 13px;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}
.nav-item:hover {
  background: var(--bg-hover);
}
.nav-item.active {
  background: var(--accent);
  color: var(--text-on-accent);
}
.nav-label {
  flex: 1;
}
.nav-badge {
  min-width: 18px;
  height: 17px;
  padding: 0 5px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 9px;
  font-size: 10.5px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  background: var(--orange-soft);
  color: var(--orange-text);
}
.nav-item.active .nav-badge {
  background: rgba(255, 255, 255, 0.25);
  color: #fff;
}

.sidebar-foot {
  margin-top: auto;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 10px 8px 2px;
  border-top: 1px solid var(--hairline);
}
.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex: none;
}
.dot.ok {
  background: var(--green);
}
.dot.idle {
  background: var(--text-3);
}
.foot-text {
  font-size: 11.5px;
  color: var(--text-2);
}
</style>
