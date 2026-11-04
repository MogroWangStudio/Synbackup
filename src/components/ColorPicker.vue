<script setup lang="ts">
// 自绘调色板：SV 面板（饱和度 × 明度）+ 色相滑条 + hex 输入。
// 全部指针事件 1:1 跟踪（Pointer Events + capture），无原生 input[type=color]。
// 出入弹层自锚点方向展开，Esc / 点外部关闭（与 Select 一致的交互语言）。
import { computed, onBeforeUnmount, ref, watch } from "vue";

const props = withDefaults(
  defineProps<{
    /** 初始颜色 #RRGGBB */
    modelValue: string;
    ariaLabel?: string;
  }>(),
  {},
);

const emit = defineEmits<{
  (e: "update:modelValue", v: string): void;
  (e: "close"): void;
}>();

// ---- HSV 状态（h: 0-360, s/v: 0-1）----
const hsv = ref({ h: 260, s: 0.51, v: 0.81 });
const hexInput = ref(props.modelValue);
const panelRef = ref<HTMLElement | null>(null);
const hueRef = ref<HTMLElement | null>(null);
const hexValid = ref(true);

function hexToHsv(hex: string) {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  const r = ((n >> 16) & 255) / 255;
  const g = ((n >> 8) & 255) / 255;
  const b = (n & 255) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  let h = 0;
  if (d > 0) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  return { h, s: max === 0 ? 0 : d / max, v: max };
}

function hsvToHex(h: number, s: number, v: number): string {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  let r = 0, g = 0, b = 0;
  if (h < 60) [r, g, b] = [c, x, 0];
  else if (h < 120) [r, g, b] = [x, c, 0];
  else if (h < 180) [r, g, b] = [0, c, x];
  else if (h < 240) [r, g, b] = [0, x, c];
  else if (h < 300) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  const to = (f: number) =>
    Math.round((f + m) * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${to(r)}${to(g)}${to(b)}`;
}

const currentHex = computed(() =>
  hsvToHex(hsv.value.h, hsv.value.s, hsv.value.v),
);

// 初始化：从传入颜色解析
const init = hexToHsv(props.modelValue);
if (init) hsv.value = init;

let emitHex = currentHex.value;
watch(currentHex, (hex) => {
  emitHex = hex;
  hexInput.value = hex;
  hexValid.value = true;
  emit("update:modelValue", hex);
});

// ---- SV 面板拖拽 ----
let svCleanup: (() => void) | null = null;
let hueCleanup: (() => void) | null = null;

function svFromEvent(e: PointerEvent) {
  const el = panelRef.value;
  if (!el) return;
  const r = el.getBoundingClientRect();
  hsv.value = {
    ...hsv.value,
    s: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)),
    v: Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height)),
  };
}

function onSvDown(e: PointerEvent) {
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  svFromEvent(e);
  const move = (ev: PointerEvent) => svFromEvent(ev);
  const up = () => {
    svCleanup?.();
    svCleanup = null;
  };
  svCleanup = up;
  (e.currentTarget as HTMLElement).addEventListener("pointermove", move);
  (e.currentTarget as HTMLElement).addEventListener("pointerup", up, { once: true });
}

function hueFromEvent(e: PointerEvent) {
  const el = hueRef.value;
  if (!el) return;
  const r = el.getBoundingClientRect();
  hsv.value = {
    ...hsv.value,
    h: Math.min(360, Math.max(0, ((e.clientX - r.left) / r.width) * 360)),
  };
}

function onHueDown(e: PointerEvent) {
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  hueFromEvent(e);
  const move = (ev: PointerEvent) => hueFromEvent(ev);
  const up = () => {
    hueCleanup?.();
    hueCleanup = null;
  };
  hueCleanup = up;
  (e.currentTarget as HTMLElement).addEventListener("pointermove", move);
  (e.currentTarget as HTMLElement).addEventListener("pointerup", up, { once: true });
}

function onHexInput(e: Event) {
  const v = (e.target as HTMLInputElement).value.trim();
  hexInput.value = v;
  const parsed = hexToHsv(v);
  if (parsed) {
    hexValid.value = true;
    hsv.value = parsed;
  } else {
    hexValid.value = false;
  }
}

function confirm() {
  emit("update:modelValue", emitHex);
  emit("close");
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.stopPropagation();
    emit("close");
  }
}

function onOutside(e: PointerEvent) {
  const t = e.target as Node;
  // 自身弹层内任意位置（含 SV 面板、滑条、hex 输入）都不算外部
  if (panelRef.value?.contains(t)) return;
  // 触发色块也不算外部：否则点色块关闭后，其 click 又会重新打开
  if (document.querySelector(".swatch.custom")?.contains(t)) return;
  emit("close");
}

// 关闭监听挂 document,延迟一拍避免打开用的同一次点击立刻关闭
setTimeout(() => document.addEventListener("pointerdown", onOutside), 0);
onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onOutside);
  svCleanup?.();
  hueCleanup?.();
});
</script>

<template>
  <div ref="panelRef" class="cpop" role="dialog" :aria-label="ariaLabel" tabindex="-1" @keydown="onKey">
    <!-- SV 面板：底色为当前色相,横向饱和度纵向明度 -->
    <div
      class="cp-sv"
      :style="{ '--hue': `${hsv.h}deg` }"
      @pointerdown="onSvDown"
    >
      <div
        class="cp-knob"
        :style="{
          left: `${hsv.s * 100}%`,
          top: `${(1 - hsv.v) * 100}%`,
          background: currentHex,
        }"
      />
    </div>

    <!-- 色相滑条 -->
    <div class="cp-hue" @pointerdown="onHueDown">
      <div class="cp-hue-knob" :style="{ left: `${(hsv.h / 360) * 100}%` }" />
    </div>

    <!-- 当前色 + hex 输入 -->
    <div class="cp-row">
      <span class="cp-preview" :style="{ background: currentHex }" />
      <input
        v-model="hexInput"
        class="cp-hex"
        :class="{ invalid: !hexValid }"
        spellcheck="false"
        maxlength="7"
        aria-label="hex"
        @input="onHexInput"
        @keydown.enter="confirm"
      />
      <button type="button" class="btn btn-sm btn-primary cp-ok" @click="confirm">
        确定
      </button>
    </div>
  </div>
</template>

<style scoped>
.cpop {
  width: 240px;
  padding: 12px;
  border-radius: var(--radius-m);
  border: 1px solid var(--hairline);
  background: var(--bg-content);
  box-shadow: var(--shadow-modal);
  outline: none;
}
.cp-sv {
  position: relative;
  height: 140px;
  border-radius: var(--radius-s);
  /* 底色 = 纯色相;两层渐变 = 饱和度(右) × 明度(上) */
  background:
    linear-gradient(to top, #000, transparent),
    linear-gradient(to right, #fff, hsl(var(--hue), 100%, 50%));
  cursor: crosshair;
  touch-action: none;
}
.cp-knob {
  position: absolute;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  transform: translate(-50%, -50%);
  border: 2px solid #fff;
  box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.35);
  pointer-events: none;
}
.cp-hue {
  position: relative;
  height: 12px;
  margin-top: 12px;
  border-radius: 6px;
  background: linear-gradient(
    to right,
    #f00,
    #ff0,
    #0f0,
    #0ff,
    #00f,
    #f0f,
    #f00
  );
  cursor: ew-resize;
  touch-action: none;
}
.cp-hue-knob {
  position: absolute;
  top: 50%;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  transform: translate(-50%, -50%);
  background: #fff;
  border: 2px solid #fff;
  box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.35);
  pointer-events: none;
}
.cp-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
}
.cp-preview {
  width: 26px;
  height: 26px;
  flex: none;
  border-radius: var(--radius-s);
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
}
.cp-hex {
  flex: 1;
  min-width: 0;
  height: 28px;
  padding: 0 8px;
  border-radius: var(--radius-s);
  border: 1px solid var(--hairline-strong);
  background: var(--bg-content);
  color: var(--text);
  font-family: var(--mono);
  font-size: calc(12px * var(--font-scale, 1));
  outline: none;
  transition:
    border-color 120ms ease,
    box-shadow 120ms ease;
}
.cp-hex:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}
.cp-hex.invalid {
  border-color: var(--red);
}
.cp-ok {
  flex: none;
}
</style>
