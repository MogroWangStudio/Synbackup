<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from "vue";
import { store } from "../store";
import { api } from "../api";
import { toast } from "../store";
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

const enabledPairs = computed(() => store.pairs.filter((p) => p.enabled).length);
// 状态区各项均可在设置中开关；全部关闭时整个状态区不渲染
const s = computed(() => store.settings);

/** 各状态项的可见开关（key 与设置字段对应） */
const visibleMap = computed<Record<string, boolean>>(() => ({
  devices: s.value.statusDevices,
  pairs: s.value.statusPairs,
  sync: s.value.statusSyncCount,
  version: s.value.statusVersion,
}));

/** 显示顺序：设置的 statusOrder 过滤可见项，遗漏的可见键按默认次序补尾 */
const statusItems = computed<string[]>(() => {
  const visible = Object.keys(visibleMap.value).filter((k) => visibleMap.value[k]);
  const ordered = (s.value.statusOrder ?? []).filter((k) => visible.includes(k));
  return [...ordered, ...visible.filter((k) => !ordered.includes(k))];
});

const hasFoot = computed(() => statusItems.value.length > 0);

/* ---- 状态项拖拽排序 ----
   变换式 FLIP：拖拽项 1:1 跟随指针（无过渡），兄弟项以过渡位移让位
   （无插入线）；松手提交顺序——兄弟项的位移与自然重排恰好抵消
   （视觉零跳变），拖拽项以反向位移过渡落位。 */
const footEl = ref<HTMLElement | null>(null);
const dragging = ref<string | null>(null);
const dragOffset = ref(0);
const shifts = ref<Record<string, number>>({});
const noAnim = ref(false);

const GAP = 5; // 与 .sidebar-foot 的 gap 保持一致

function lineStyle(key: string) {
  if (dragging.value === key) return { transform: `translateY(${dragOffset.value}px)` };
  const shift = shifts.value[key];
  if (shift) return { transform: `translateY(${shift}px)` };
  return {};
}

function onFootDown(key: string, e: PointerEvent) {
  if (e.pointerType === "mouse" && e.button !== 0) return;
  const foot = footEl.value;
  if (!foot) return;
  const lines = [...foot.querySelectorAll<HTMLElement>(".foot-line")];
  if (lines.length < 2) return;
  const rects = lines.map((el) => el.getBoundingClientRect());
  const keys = lines.map((el) => el.dataset.key ?? "");
  const idx = keys.indexOf(key);
  const H = rects[0].height;
  const base = rects[0].top;
  const startY = e.clientY;
  const grab = e.clientY - rects[idx].top;
  let started = false;
  let target = idx;

  const el = e.currentTarget as HTMLElement;
  try {
    el.setPointerCapture(e.pointerId);
  } catch {
    /* 合成指针等场景：继续无捕获跟踪 */
  }

  /** 虚拟排列（去掉拖拽项、在 target 处留空位）下兄弟项的位移 */
  const applyShifts = () => {
    const next: Record<string, number> = {};
    let r = 0;
    for (let i = 0; i < keys.length; i++) {
      if (i === idx) continue;
      const pos = r < target ? r : r + 1;
      next[keys[i]] = base + pos * (H + GAP) - rects[i].top;
      r++;
    }
    shifts.value = next;
  };

  const move = (ev: PointerEvent) => {
    const dy = ev.clientY - startY;
    if (!started) {
      if (Math.abs(dy) < 6) return; // 拖拽阈值：避免误触
      started = true;
      dragging.value = key;
    }
    dragOffset.value = dy;
    // 拖拽项视觉中心落在哪个槽位 → 目标插入位（0..n-1）
    const center = rects[idx].top + grab + dy + H / 2;
    const slot = (center - base - H / 2) / (H + GAP);
    const t = Math.max(0, Math.min(keys.length - 1, Math.round(slot)));
    if (t !== target) {
      target = t;
      applyShifts();
    }
  };

  const stop = () => {
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", stop);
    el.removeEventListener("pointercancel", stop);
    if (!started) return;

    // 提交新顺序（隐藏项保留原位）；顺序未变则只收尾
    const visibleSet = new Set(Object.keys(visibleMap.value).filter((k) => visibleMap.value[k]));
    const full = [...(s.value.statusOrder ?? [])];
    for (const k of keys) if (!full.includes(k)) full.push(k);
    const visIdx = full.filter((k) => visibleSet.has(k));
    const orderChanged = target !== visIdx.indexOf(key);
    if (orderChanged) {
      const without = visIdx.filter((k) => k !== key);
      without.splice(target, 0, key);
      const queue = [...without];
      const newOrder = full.map((k) => (visibleSet.has(k) ? (queue.shift() as string) : k));
      store.settings = { ...store.settings, statusOrder: newOrder };
      void api.saveSettings(store.settings).catch((err) => toast("error", String(err)));
    }

    // 落位：提交后拖拽项的自然位 = 让位空位；反向位移挂上后再过渡归零
    const delta = rects[idx].top + dragOffset.value - (base + target * (H + GAP));
    noAnim.value = true;
    dragging.value = null;
    shifts.value = {};
    dragOffset.value = 0;
    void nextTick().then(() => {
      const lineEl = foot.querySelector<HTMLElement>(`.foot-line[data-key="${key}"]`);
      if (lineEl && orderChanged && Math.abs(delta) > 0.5) {
        lineEl.style.transform = `translateY(${delta}px)`;
        void lineEl.offsetHeight; // reflow：反向位移就位后再开过渡
        noAnim.value = false;
        lineEl.style.transform = "";
      } else {
        noAnim.value = false;
      }
    });
  };

  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", stop);
  el.addEventListener("pointercancel", stop);
}

onBeforeUnmount(() => {
  dragging.value = null;
  shifts.value = {};
});
</script>

<template>
  <aside class="sidebar">
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

    <div
      v-if="hasFoot"
      ref="footEl"
      class="sidebar-foot"
      :class="{ 'no-anim': noAnim }"
    >
      <div
        v-for="key in statusItems"
        :key="key"
        class="foot-line"
        :class="{ dragging: dragging === key, dimmed: !!dragging && dragging !== key }"
        :style="lineStyle(key)"
        :data-key="key"
        @pointerdown="onFootDown(key, $event)"
      >
        <template v-if="key === 'devices'">
          <span class="dot" :class="store.volumes.length ? 'ok' : 'idle'" />
          <span class="foot-text">
            {{
              store.volumes.length
                ? t("status.devices_connected", { n: store.volumes.length })
                : t("status.no_devices")
            }}
          </span>
        </template>
        <template v-else-if="key === 'pairs'">
          <span class="foot-text">
            {{ t("status.pairs_enabled", { on: enabledPairs, total: store.pairs.length }) }}
          </span>
        </template>
        <template v-else-if="key === 'sync'">
          <span class="foot-text">{{ t("status.sync_count", { n: store.syncCount }) }}</span>
        </template>
        <template v-else>
          <span class="foot-text">v{{ store.appInfo?.version ?? "–" }}</span>
        </template>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  width: 212px;
  flex: none;
  /* 品牌标识已上移至标题栏，侧栏只承载导航与设备状态 */
  padding: 10px 10px 14px;
  background: var(--bg-sidebar);
  backdrop-filter: blur(24px) saturate(1.6);
  border-right: 1px solid var(--hairline);
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
  font-size: calc(13px * var(--font-scale, 1));
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
  font-size: calc(10.5px * var(--font-scale, 1));
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
  flex-direction: column;
  gap: 5px; /* 与脚本里的 GAP 常量保持一致 */
  padding: 10px 8px 2px;
  border-top: 1px solid var(--hairline);
}
.foot-line {
  display: flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
  position: relative;
  padding: 2px 6px;
  margin: 0 -6px;
  user-select: none;
  touch-action: none;
  cursor: grab;
  /* 让位位移与落位动画共用同一条过渡 */
  transition: transform 180ms var(--ease);
}
.foot-line.dragging {
  transition: none; /* 拖拽项 1:1 跟随指针，不吃过渡 */
  z-index: 2;
  cursor: grabbing;
  background: var(--fill-soft);
  border-radius: var(--radius-s);
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.14);
}
.foot-line.dimmed {
  opacity: 0.7;
}
/* 提交顺序的瞬间禁用过渡：位移与重排恰好抵消，不能让过渡掺进来 */
.sidebar-foot.no-anim .foot-line {
  transition: none !important;
}
@media (prefers-reduced-motion: reduce) {
  .foot-line {
    transition: none;
  }
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
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-2);
}
</style>
