<script setup lang="ts">
// 自定义下拉列表：替代原生 <select>（原生弹层样式不可控、与整体视觉脱节）。
// 弹层用 fixed 定位锚定触发器，避免被 Modal / 滚动容器裁剪；
// 键盘可达（上下选择、回车确认、Esc 关闭），可访问性与原生一致。
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import Icon from "./Icon.vue";

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

const open = ref(false);
const triggerRef = ref<HTMLElement | null>(null);
const listRef = ref<HTMLElement | null>(null);
const activeIdx = ref(-1);
const dropUp = ref(false);
const popStyle = ref<{ left: string; top: string; minWidth: string } | null>(null);

const current = computed(() => props.options.find((o) => o.value === props.modelValue));
const currentLabel = computed(() => current.value?.label ?? props.placeholder ?? "");

let removeOutside: (() => void) | null = null;

function selectableOptions() {
  return props.options.map((o, i) => ({ o, i })).filter(({ o }) => !o.disabled);
}

function updatePosition() {
  const el = triggerRef.value;
  if (!el) return;
  const r = el.getBoundingClientRect();
  const spaceBelow = window.innerHeight - r.bottom;
  dropUp.value = spaceBelow < 220 && r.top > 220;
  popStyle.value = {
    left: `${Math.round(r.left)}px`,
    top: dropUp.value ? `${Math.round(r.top - 6)}px` : `${Math.round(r.bottom + 6)}px`,
    minWidth: `${Math.round(r.width)}px`,
  };
}

async function setOpen(v: boolean) {
  if (props.disabled || v === open.value) return;
  open.value = v;
  if (v) {
    updatePosition();
    const sel = selectableOptions();
    const curIdx = props.options.findIndex((o) => o.value === props.modelValue);
    activeIdx.value = sel.find(({ i }) => i === curIdx)?.i ?? sel[0]?.i ?? -1;
    removeOutside = listenOutside();
    await nextTick();
    listRef.value?.querySelector('[data-active="true"]')?.scrollIntoView({ block: "nearest" });
  } else {
    removeOutside?.();
    removeOutside = null;
  }
}

function listenOutside() {
  const onDown = (e: MouseEvent) => {
    const t = e.target as Node;
    if (!triggerRef.value?.contains(t) && !listRef.value?.contains(t)) setOpen(false);
  };
  window.addEventListener("pointerdown", onDown, true);
  return () => window.removeEventListener("pointerdown", onDown, true);
}

function choose(opt: SelectOption) {
  if (opt.disabled) return;
  emit("update:modelValue", opt.value);
  setOpen(false);
  triggerRef.value?.focus();
}

function move(dir: 1 | -1) {
  const sel = selectableOptions();
  if (!sel.length) return;
  const pos = sel.findIndex(({ i }) => i === activeIdx.value);
  const next = sel[(pos + dir + sel.length) % sel.length];
  activeIdx.value = next.i;
  listRef.value?.querySelector('[data-active="true"]')?.scrollIntoView({ block: "nearest" });
}

function onTriggerKey(e: KeyboardEvent) {
  if (props.disabled) return;
  switch (e.key) {
    case "Enter":
    case " ":
      e.preventDefault();
      setOpen(!open.value);
      break;
    case "ArrowDown":
      e.preventDefault();
      open.value ? move(1) : setOpen(true);
      break;
    case "ArrowUp":
      e.preventDefault();
      open.value ? move(-1) : setOpen(true);
      break;
    case "Escape":
      if (open.value) {
        e.stopPropagation();
        setOpen(false);
      }
      break;
  }
}

function onListKey(e: KeyboardEvent) {
  switch (e.key) {
    case "ArrowDown":
      e.preventDefault();
      move(1);
      break;
    case "ArrowUp":
      e.preventDefault();
      move(-1);
      break;
    case "Enter":
    case " ":
      e.preventDefault();
      if (activeIdx.value >= 0) choose(props.options[activeIdx.value]);
      break;
    case "Escape":
      e.stopPropagation();
      setOpen(false);
      triggerRef.value?.focus();
      break;
  }
}

function onScrollOrResize() {
  if (open.value) updatePosition();
}

onMounted(() => {
  window.addEventListener("scroll", onScrollOrResize, true);
  window.addEventListener("resize", onScrollOrResize);
});
onBeforeUnmount(() => {
  window.removeEventListener("scroll", onScrollOrResize, true);
  window.removeEventListener("resize", onScrollOrResize);
  removeOutside?.();
});
// 值被外部改变时保持弹层内高亮同步
watch(() => props.modelValue, () => {
  if (open.value) {
    const curIdx = props.options.findIndex((o) => o.value === props.modelValue);
    activeIdx.value = curIdx;
  }
});
</script>

<template>
  <button
    ref="triggerRef"
    type="button"
    class="cselect"
    :class="{ open, fill, disabled }"
    role="combobox"
    :aria-expanded="open"
    aria-haspopup="listbox"
    :aria-label="ariaLabel"
    :disabled="disabled"
    @click="setOpen(!open)"
    @keydown="onTriggerKey"
  >
    <span class="cselect-label" :class="{ placeholder: !current }">{{ currentLabel }}</span>
    <Icon name="arrow_down" :size="13" class="cselect-chevron" />
  </button>

  <Teleport to="body">
    <Transition name="cselect-pop">
      <div
        v-if="open"
        ref="listRef"
        class="cselect-pop"
        :class="{ up: dropUp }"
        :style="popStyle"
        role="listbox"
        tabindex="-1"
        @keydown="onListKey"
      >
        <div
          v-for="(o, i) in options"
          :key="o.value"
          class="cselect-item"
          :class="{ active: i === activeIdx, selected: o.value === modelValue, disabled: o.disabled }"
          :data-active="i === activeIdx"
          role="option"
          :aria-selected="o.value === modelValue"
          :aria-disabled="o.disabled"
          @click="choose(o)"
          @mousemove="!o.disabled && (activeIdx = i)"
        >
          <span class="cselect-item-label">{{ o.label }}</span>
          <Icon v-if="o.value === modelValue" name="check" :size="13" class="cselect-check" />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/* 触发器：与 .input 同规格（30px 高、同圆角描边），保证替换后视觉一致 */
.cselect {
  display: inline-flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  height: 30px;
  min-width: 120px;
  padding: 0 10px;
  border-radius: var(--radius-s);
  border: 1px solid var(--hairline-strong);
  background: var(--bg-content);
  color: var(--text);
  font-size: calc(13px * var(--font-scale, 1));
  font-family: inherit;
  cursor: pointer;
  text-align: left;
  transition:
    border-color 120ms ease,
    box-shadow 120ms ease;
}
.cselect.fill {
  width: 100%;
}
/* 悬停：描边突出（柔和光圈由内向外晕开），不改变触发器宽度 */
.cselect:not(.fill) {
  transition:
    border-color 120ms ease,
    box-shadow 120ms ease;
}
.cselect:hover {
  border-color: var(--accent-2);
  box-shadow: 0 0 0 3px var(--accent-soft);
}
.cselect.open,
.cselect:focus-visible {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
  outline: none;
}
.cselect.disabled {
  opacity: 0.55;
  cursor: default;
}
.cselect-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cselect-label.placeholder {
  color: var(--text-3);
}
.cselect-chevron {
  flex: none;
  color: var(--text-3);
  transition: transform 160ms var(--ease);
}
.cselect.open .cselect-chevron {
  transform: rotate(180deg);
}

/* 弹层：fixed 锚定触发器，不被 Modal / 滚动容器裁剪 */
.cselect-pop {
  position: fixed;
  z-index: 400; /* 弹窗遮罩 100 / 标题栏 150 之上 */
  max-height: 220px;
  overflow-y: auto;
  padding: 4px;
  border-radius: var(--radius-m);
  border: 1px solid var(--hairline);
  background: var(--bg-content);
  box-shadow: var(--shadow-modal);
  transform-origin: top;
  outline: none;
}
.cselect-pop.up {
  transform-origin: bottom;
}
.cselect-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 6px 10px;
  border-radius: calc(var(--radius-s) - 2px);
  font-size: calc(12.5px * var(--font-scale, 1));
  color: var(--text);
  cursor: pointer;
}
.cselect-item.active {
  background: var(--bg-hover);
}
.cselect-item.selected {
  color: var(--accent-text);
  font-weight: 500;
}
.cselect-item.disabled {
  color: var(--text-3);
  cursor: default;
}
.cselect-check {
  flex: none;
  color: var(--accent);
}

/* 弹层出入：自触发器方向展开（Apple: 空间一致性），快而克制 */
.cselect-pop-enter-active {
  transition:
    opacity 140ms var(--ease),
    transform 160ms var(--ease);
}
.cselect-pop-leave-active {
  transition:
    opacity 110ms var(--ease),
    transform 120ms var(--ease);
}
.cselect-pop-enter-from,
.cselect-pop-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.98);
}
.cselect-pop.up.cselect-pop-enter-from,
.cselect-pop.up.cselect-pop-leave-to {
  transform: translateY(4px) scale(0.98);
}
@media (prefers-reduced-motion: reduce) {
  .cselect-pop-enter-active,
  .cselect-pop-leave-active {
    transition: opacity 120ms ease;
  }
  .cselect-pop-enter-from,
  .cselect-pop-leave-to {
    transform: none;
  }
}
</style>
