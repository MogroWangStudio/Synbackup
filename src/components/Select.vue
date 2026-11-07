<script setup lang="ts">
// MWS ComboBox —— 按 MWS UI 设计稿（mws-combobox）严格移植：
//   · 三层结构：pop（视口锚定）/ frame（圆角弹层框）/ plate（行板，靠位移对行）；
//   · 打开时选中行先对准触发器，frame 自单行高度整幅展开（420ms 回弹减速），
//     行按与锚点的距离错峰淡入；关闭反向收回到选中行（加速离场）；
//   · 打开状态下点选其他项：弹层滑移把新选中行送到触发器位置（recenter，
//     320ms）再收回——"列表移动、按钮不动"的选中手感；
//   · 弹出方向三态：优先以选中行为中心，空间不足整体向下 / 向上，再不足贴边钳制。
// 应用侧适配：弹层 Teleport 到 body 用 fixed 定位（不被 Modal / 滚动容器裁剪），
// 触发器几何按容器 zoom（--ui-scale）换算到 body 坐标系；Esc 阻断冒泡避免连带
// 关闭 Modal；prefers-reduced-motion 下所有位移动画退化为瞬时切换。
import { computed, onBeforeUnmount, onBeforeUpdate, onMounted, ref, watch } from "vue";
import type { ComponentPublicInstance } from "vue";

export interface SelectOption {
  value: string;
  label: string;
  /** 置灰不可选 */
  disabled?: boolean;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: SelectOption[];
    placeholder?: string;
    ariaLabel?: string;
    /** 触发器宽度：fill 撑满父容器，否则自适应内容并限制最小宽 */
    fill?: boolean;
    disabled?: boolean;
  }>(),
  { fill: false, disabled: false },
);

const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();

/* ---- 设计稿动效参数（原样保留） ---- */
const ROW = 38; // 单行高度（触发器与弹层行同高，center 模式对位的前提）
const MARGIN = 12; // 弹层距视口边缘的最小留白
const EO = "cubic-bezier(0.34, 1.56, 0.64, 1)"; // 打开 / 换中：回弹减速
const EC = "cubic-bezier(0.55, 0, 1, 0.45)"; // 关闭：加速离场
const DUR = 420; // 展开 / 收回时长
const MOVE_DUR = 320; // 换中滑移时长

const reduceMotion =
  typeof window !== "undefined" &&
  !!window.matchMedia &&
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const durOpen = reduceMotion ? 0 : DUR;
const durClose = reduceMotion ? 0 : Math.round(DUR * 0.6);
const durMove = reduceMotion ? 0 : MOVE_DUR;

/* ---- 状态 ---- */
const idx = ref(0); // 当前选中行（弹层几何以它为锚）
const hoverI = ref(-1); // 悬停 / 键盘高亮行
const open = ref(false);
let anchor = 0; // 展开时的锚点行（动画的参照系）
let phase: "closed" | "opening" | "open" | "closing" = "closed";
let busy = false; // 换中滑移进行中（锁定输入，Esc 除外）
let lastDir: "center" | "down" | "up" = "center";
let timer = 0;
let moveTimer = 0;
let lastEl: HTMLElement | null = null;

const rootEl = ref<HTMLElement | null>(null);
const btnEl = ref<HTMLElement | null>(null);
const chevEl = ref<SVGSVGElement | null>(null);
const popEl = ref<HTMLElement | null>(null);
const frameEl = ref<HTMLElement | null>(null);
const plateEl = ref<HTMLElement | null>(null);
const rowEls = ref<HTMLElement[]>([]);
onBeforeUpdate(() => {
  rowEls.value = [];
});
function setRowRef(el: Element | ComponentPublicInstance | null, i: number) {
  if (el instanceof HTMLElement) rowEls.value[i] = el;
}

const current = computed(() => props.options.find((o) => o.value === props.modelValue));
const currentLabel = computed(() => current.value?.label ?? props.placeholder ?? "");
const popH = computed(() => props.options.length * ROW);

function isDis(i: number): boolean {
  return !!props.options[i]?.disabled;
}

/* ---- 几何：设计稿 openOffset / layout / closedGeom 的移植，
   坐标统一换算到未缩放的 body 坐标系（--ui-scale 只作用于主内容区） ---- */

function zoomOf(): number {
  const el = rootEl.value;
  return (el ? Number(getComputedStyle(el).zoom) : 1) || 1;
}

/** 触发器在未缩放（body）坐标系中的位置 */
function rootRect(): { top: number; left: number; width: number } {
  const el = rootEl.value;
  if (!el) return { top: 0, left: 0, width: 0 };
  const z = zoomOf();
  const r = el.getBoundingClientRect();
  return { top: r.top / z, left: r.left / z, width: r.width / z };
}

/** 第 to 行对准触发器时弹层（fixed）应在的 top；同时判定弹出方向 lastDir */
function popTopFor(to: number): number {
  const z = zoomOf();
  const vh = window.innerHeight / z;
  const { top } = rootRect();
  const total = props.options.length * ROW;
  const up = Math.max(0, top - MARGIN);
  const down = Math.max(0, vh - (top + ROW) - MARGIN);
  const needUp = to * ROW;
  const needDown = (props.options.length - to) * ROW;
  const lo = MARGIN - top;
  const hi = vh - MARGIN - total - top;
  let rel: number;
  if (up >= needUp && down >= needDown) {
    lastDir = "center";
    rel = -to * ROW;
  } else if (down >= total) {
    lastDir = "down";
    rel = ROW;
  } else if (up >= total) {
    lastDir = "up";
    rel = -total;
  } else {
    lastDir = up >= down ? "down" : "up";
    rel = lastDir === "down" ? lo : hi;
  }
  return top + rel;
}

/** 锚定触发器位置（left / width / top），打开与滚动跟随共用 */
function layout() {
  const pop = popEl.value;
  if (!pop) return;
  const { left, width } = rootRect();
  pop.style.transition = "none";
  pop.style.left = `${left}px`;
  pop.style.width = `${width}px`;
  pop.style.top = `${popTopFor(anchor)}px`;
}

function popTop(): number {
  const el = popEl.value;
  if (!el) return 0;
  const v = parseFloat(el.style.top);
  return isNaN(v) ? 0 : v;
}

function curTop(el: HTMLElement): number {
  const v = parseFloat(getComputedStyle(el).top);
  if (!isNaN(v)) return v;
  const s = parseFloat(el.style.top);
  return isNaN(s) ? 0 : s;
}

/** 收回态几何：frame 对准触发器，plate 对准选中行 */
function closedGeom(): [number, number] {
  return [rootRect().top - popTop(), -idx.value * ROW];
}

function setGeom(t: number, h: number, p: number) {
  if (!frameEl.value || !plateEl.value) return;
  frameEl.value.style.top = `${t}px`;
  frameEl.value.style.height = `${h}px`;
  plateEl.value.style.top = `${p}px`;
}

/** 行的入场位移：以锚点为参照，近 0 远 8px */
function shift(i: number): number {
  return i === anchor ? 0 : i > anchor ? 8 : -8;
}

/* ---- 动画状态机：设计稿 closedFrame / show / hide / snapOpen / recenter 移植 ---- */

function closedFrame() {
  const frame = frameEl.value;
  const plate = plateEl.value;
  if (!frame || !plate) return;
  frame.style.transition = "none";
  plate.style.transition = "none";
  frame.style.boxShadow = "none";
  layout();
  const g = closedGeom();
  setGeom(g[0], ROW, g[1]);
  const rows = rowEls.value;
  for (let i = 0; i < props.options.length; i++) {
    const el = rows[i];
    if (!el) continue;
    el.style.transition = "none";
    el.style.opacity = "0";
    el.style.transform = `translateY(${shift(i)}px)`;
  }
  void frame.offsetWidth;
}

function show() {
  if (props.disabled) return;
  clearTimeout(timer);
  clearTimeout(moveTimer);
  busy = false;

  anchor = idx.value;
  hoverI.value = -1;
  open.value = true;
  phase = "opening";
  const pop = popEl.value;
  const frame = frameEl.value;
  const plate = plateEl.value;
  const chev = chevEl.value;
  if (!pop || !frame || !plate) return;
  layout();
  pop.style.visibility = "visible";
  pop.style.pointerEvents = "auto";
  closedFrame();
  frame.style.boxShadow = "var(--mws-shadow)";
  frame.style.transition = `top ${durOpen}ms ${EO}, height ${durOpen}ms ${EO}`;
  plate.style.transition = `top ${durOpen}ms ${EO}`;
  setGeom(0, popH.value, 0);
  if (chev) {
    chev.style.transition = `transform ${durOpen}ms ${EO}`;
    chev.style.transform = "rotate(180deg)";
  }
  const rows = rowEls.value;
  for (let i = 0; i < props.options.length; i++) {
    const el = rows[i];
    if (!el) continue;
    const d = reduceMotion ? 0 : Math.round(Math.abs(i - anchor) * 24);
    el.style.transition = `opacity ${Math.round(durOpen * 0.4)}ms ${EO} ${d}ms, transform ${Math.round(durOpen * 0.55)}ms ${EO} ${d}ms, background 120ms ease`;
    el.style.opacity = "1";
    el.style.transform = "translateY(0)";
  }
  timer = window.setTimeout(() => {
    if (open.value) phase = "open";
  }, durOpen + 160);
}

function hide() {
  if (!open.value) return;
  open.value = false;
  phase = "closing";
  clearTimeout(timer);
  const frame = frameEl.value;
  const plate = plateEl.value;
  const chev = chevEl.value;
  if (!frame || !plate) return;
  const g = closedGeom();
  frame.style.transition = `top ${durClose}ms ${EC}, height ${durClose}ms ${EC}`;
  plate.style.transition = `top ${durClose}ms ${EC}`;
  setGeom(g[0], ROW, g[1]);
  if (chev) {
    chev.style.transition = `transform ${durClose}ms ${EC}`;
    chev.style.transform = "rotate(0deg)";
  }
  const rows = rowEls.value;
  for (let i = 0; i < props.options.length; i++) {
    const el = rows[i];
    if (!el) continue;
    if (i === anchor) {
      el.style.transition = "none";
      el.style.opacity = "1";
    } else {
      el.style.transition = `opacity ${Math.round(durClose * 0.5)}ms ${EC}`;
      el.style.opacity = "0";
    }
  }
  timer = window.setTimeout(() => {
    if (open.value) return;
    const pop = popEl.value;
    if (pop) {
      pop.style.visibility = "hidden";
      pop.style.pointerEvents = "none";
    }
    phase = "closed";
    anchor = idx.value;

    hoverI.value = -1;
    closedFrame();
  }, durClose + 30);
}

/** 跳过动画直接呈全开态（打开途中被换中打断时使用） */
function snapOpen() {
  clearTimeout(timer);
  const frame = frameEl.value;
  const plate = plateEl.value;
  const chev = chevEl.value;
  if (!frame || !plate) return;
  frame.style.transition = "none";
  plate.style.transition = "none";
  if (chev) chev.style.transition = "none";
  setGeom(0, popH.value, 0);
  if (chev) chev.style.transform = "rotate(180deg)";
  const rows = rowEls.value;
  for (let i = 0; i < props.options.length; i++) {
    const el = rows[i];
    if (!el) continue;
    el.style.transition = "none";
    el.style.opacity = "1";
    el.style.transform = "translateY(0)";
  }
  void frame.offsetWidth;
  phase = "open";
}

/** 换中：滑移弹层把第 to 行送到触发器位置，落定后收回 */
function recenter(to: number) {
  const pop = popEl.value;
  const plate = plateEl.value;
  if (!pop || !plate) return;
  const md = durMove;
  const p = curTop(pop);
  const pl = curTop(plate);
  const delta = popTopFor(to) - p;
  clearTimeout(timer);
  clearTimeout(moveTimer);
  busy = true;
  anchor = to;
  // slide 模式：整体位移 pop（frame / plate 相对不动），行板视觉连续
  plate.style.transition = "none";
  plate.style.top = `${pl}px`;
  pop.style.transition = "none";
  pop.style.top = `${p}px`;
  void pop.offsetWidth;
  pop.style.transition = `top ${md}ms ${EO}`;
  pop.style.top = `${p + delta}px`;
  lastEl = pop;
  moveTimer = window.setTimeout(() => {
    if (!busy) return;
    busy = false;
    if (!lastEl) return;
    lastEl.style.transition = "none";
    if (!open.value) return;
    lastEl.style.top = `${p + delta}px`;
    hide();
  }, md + 30);
}

/** 把滑移定格在当前位置（随后由调用方 hide / show 接管） */
function settle() {
  if (!busy) return;
  clearTimeout(moveTimer);
  busy = false;
  if (!lastEl) return;
  const v = curTop(lastEl);
  lastEl.style.transition = "none";
  lastEl.style.top = `${v}px`;
}

function commit(k: number) {
  if (isDis(k)) return;
  const changed = k !== idx.value;
  idx.value = k;
  const val = props.options[k]?.value;
  if (val !== undefined) emit("update:modelValue", val);
  if (changed && open.value && lastDir === "center") {
    if (phase === "opening") snapOpen();
    recenter(k);
  } else {
    anchor = k;
    hide();
  }
}

/* ---- 事件 ---- */

function onBtnClick() {
  settle();
  if (open.value) hide();
  else show();
}

function onRowEnter(i: number) {
  if (!open.value || busy) return;
  hoverI.value = i;
}

function onRowLeave(i: number) {
  if (hoverI.value === i) hoverI.value = -1;
}

function onRowClick(i: number) {
  if (!open.value) return;
  commit(i);
}

/** 全局键盘：capture 阶段监听，Esc 在此消费掉，避免连带关闭所在 Modal */
function onKeyDown(ev: KeyboardEvent) {
  const k = ev.key;
  if (!open.value) {
    if (document.activeElement === btnEl.value && (k === "ArrowDown" || k === "ArrowUp" || k === "Enter" || k === " ")) {
      ev.preventDefault();
      show();
    }
    return;
  }
  if (busy && k !== "Escape") return;
  if (k === "Escape") {
    ev.preventDefault();
    ev.stopPropagation();
    settle();
    hide();
  } else if (k === "Tab") {
    hide();
  } else if (k === "ArrowDown" || k === "ArrowUp") {
    ev.preventDefault();
    let i = hoverI.value < 0 ? idx.value : hoverI.value;
    const dir = k === "ArrowDown" ? 1 : -1;
    for (let n = 0; n < props.options.length; n++) {
      i = (i + dir + props.options.length) % props.options.length;
      if (!isDis(i)) break;
    }
    hoverI.value = i;
  } else if (k === "Enter" || k === " ") {
    ev.preventDefault();
    if (hoverI.value >= 0 && hoverI.value !== idx.value) commit(hoverI.value);
    else hide();
  }
}

function onDocClick(ev: MouseEvent) {
  if (!open.value) return;
  const t = ev.target as Node;
  if (!rootEl.value?.contains(t) && !popEl.value?.contains(t)) {
    settle();
    hide();
  }
}

/** 滚动 / 改窗时弹层跟随触发器（换中滑移中不动，落定后自然对位） */
function onScrollOrResize() {
  if (open.value && !busy) layout();
}

onMounted(() => {
  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("click", onDocClick);
  window.addEventListener("scroll", onScrollOrResize, true);
  window.addEventListener("resize", onScrollOrResize);
  closedFrame();
});
onBeforeUnmount(() => {
  clearTimeout(timer);
  clearTimeout(moveTimer);
  window.removeEventListener("keydown", onKeyDown, true);
  window.removeEventListener("click", onDocClick);
  window.removeEventListener("scroll", onScrollOrResize, true);
  window.removeEventListener("resize", onScrollOrResize);
});

// 值或选项集被外部改变时保持选中行同步（弹层行样式由响应式 class 自动刷新）；
// immediate：初始 modelValue 也要算出 idx，否则选中标记落在第 0 行
watch(
  () => [props.modelValue, props.options] as const,
  () => {
    const i = props.options.findIndex((o) => o.value === props.modelValue);
    idx.value = i >= 0 ? i : 0;
  },
  { immediate: true },
);
</script>

<template>
  <div ref="rootEl" class="mws-cbx" :class="{ fill, disabled }">
    <button
      ref="btnEl"
      type="button"
      class="mws-btn"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-label="ariaLabel"
      :disabled="disabled"
      @click="onBtnClick"
    >
      <span class="mws-label" :class="{ placeholder: !current }">{{ currentLabel }}</span>
      <svg ref="chevEl" width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
        <path
          d="M2.5 4.5L6 8L9.5 4.5"
          fill="none"
          stroke="currentColor"
          stroke-width="1.3"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>

    <Teleport to="body">
      <div ref="popEl" class="mws-pop" role="listbox" :aria-label="ariaLabel">
        <div ref="frameEl" class="mws-frame">
          <div ref="plateEl" class="mws-plate">
            <div
              v-for="(o, i) in options"
              :key="o.value"
              :ref="(el) => setRowRef(el, i)"
              class="mws-row"
              :class="{
                'is-sel': i === idx,
                'is-hover': hoverI === i && !o.disabled,
                'is-dis': o.disabled,
              }"
              role="option"
              :aria-selected="i === idx"
              :aria-disabled="o.disabled || undefined"
              @pointerenter="onRowEnter(i)"
              @pointerleave="onRowLeave(i)"
              @click.stop="onRowClick(i)"
            >
              <span>{{ o.label }}</span>
              <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
                <path
                  d="M2 6.2L4.7 9L10 3.2"
                  fill="none"
                  stroke-width="1.4"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            </div>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
/* ---- 设计稿变量 → 应用主题 token。弹层 Teleport 到 body 后脱离组件子树，
   变量映射需在 .mws-cbx 与 .mws-pop 上各声明一份（值引用全局 token，
   深浅色随 :root 切换自动生效） ---- */
.mws-cbx,
.mws-pop {
  --mws-accent: var(--accent);
  --mws-accent2: var(--accent-2);
  --mws-sel: var(--accent-soft);
  --mws-text: var(--text);
  --mws-panel: var(--bg-content);
  --mws-border: var(--hairline-strong);
  --mws-hover: var(--bg-hover);
  --mws-muted: var(--text-3);
  --mws-font: var(--font);
  --mws-size: calc(13px * var(--font-scale, 1));
  --mws-weight: 400;
  --mws-shadow: 0 8px 24px rgba(0, 0, 0, 0.14);
}
:root[data-appearance="dark"] .mws-pop {
  --mws-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}
@media (prefers-color-scheme: dark) {
  :root:not([data-appearance="light"]) .mws-pop {
    --mws-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
  }
}

/* ---- 触发器 ---- */
.mws-cbx {
  position: relative;
  display: inline-block;
  min-width: 120px;
  font-family: var(--mws-font);
  font-size: var(--mws-size);
  letter-spacing: 0px;
}
.mws-cbx.fill {
  display: block;
  width: 100%;
}
.mws-cbx.disabled {
  opacity: 0.55;
}
.mws-btn {
  width: 100%;
  height: 38px;
  padding: 0 12px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  background: var(--mws-panel);
  color: var(--mws-text);
  border: 0.5px solid var(--mws-border);
  border-radius: 10px;
  font: inherit;
  font-weight: var(--mws-weight);
  letter-spacing: inherit;
  cursor: pointer;
  text-align: left;
}
/* 设计稿未定义悬停 / 按压态，按最小反馈补齐：悬停描边换虚色，按下即响应 */
.mws-btn:not(:disabled):hover {
  border-color: var(--mws-accent2);
}
.mws-btn:not(:disabled):active {
  background: var(--mws-hover);
}
.mws-btn:disabled {
  cursor: default;
}
.mws-btn:focus-visible {
  outline: 2px solid var(--mws-accent);
  outline-offset: 2px;
}
.mws-btn svg {
  flex: none;
}
.mws-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mws-label.placeholder {
  color: var(--mws-muted);
}

/* ---- 弹层：fixed 锚定触发器（坐标由 JS 换算 zoom 后写入），DOM 常驻，
   靠 visibility 切换以承载收回动画 ---- */
.mws-pop {
  position: fixed;
  z-index: 400; /* 弹窗遮罩 100 / 标题栏 150 之上 */
  visibility: hidden;
  pointer-events: none;
  font-family: var(--mws-font);
  font-size: var(--mws-size);
  letter-spacing: 0px;
}
.mws-frame {
  position: absolute;
  left: 0;
  width: 100%;
  overflow: hidden;
  background: var(--mws-panel);
  border: 0.5px solid var(--mws-border);
  border-radius: 12px;
  corner-shape: round;
}
.mws-plate {
  position: absolute;
  left: 0;
  width: 100%;
}
.mws-row {
  height: 38px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 10px;
  cursor: pointer;
  color: var(--mws-text);
  border-left: 2px solid transparent;
  opacity: 0;
  font-weight: var(--mws-weight);
}
.mws-row svg {
  flex: none;
  opacity: 0;
}
.mws-row svg path {
  stroke: var(--mws-accent);
}
.mws-row.is-sel {
  background: var(--mws-sel);
  font-weight: 500;
  border-left-color: var(--mws-accent);
}
.mws-row.is-sel svg {
  opacity: 1;
}
.mws-row.is-hover {
  background: var(--mws-hover);
}
.mws-row.is-dis {
  color: var(--mws-muted);
  cursor: not-allowed;
  background: transparent;
}
</style>
