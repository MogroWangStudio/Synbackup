<script setup lang="ts">
// 数值步进器：− / 当前值 / + 三个区域，边界自动禁用。
// 设置页的字体大小与界面大小都用它，交互一致、免输入。
const props = withDefaults(
  defineProps<{
    modelValue: number;
    step: number;
    min: number;
    max: number;
    /** 展示格式（如百分比）；缺省直接显示数值 */
    format?: (v: number) => string;
    decreaseLabel: string;
    increaseLabel: string;
  }>(),
  {},
);

const emit = defineEmits<{ (e: "update:modelValue", v: number): void }>();

function bump(dir: 1 | -1) {
  const raw = props.modelValue + dir * props.step;
  const v = Math.min(props.max, Math.max(props.min, Number(raw.toFixed(2))));
  if (v !== props.modelValue) emit("update:modelValue", v);
}
</script>

<template>
  <div class="stepper">
    <button
      type="button"
      class="st-btn"
      :disabled="modelValue <= min"
      :aria-label="decreaseLabel"
      @click="bump(-1)"
    >
      <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
        <path d="M2.5 6h7" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
    </button>
    <span class="st-value" aria-live="polite">{{ format ? format(modelValue) : modelValue }}</span>
    <button
      type="button"
      class="st-btn"
      :disabled="modelValue >= max"
      :aria-label="increaseLabel"
      @click="bump(1)"
    >
      <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
        <path
          d="M2.5 6h7M6 2.5v7"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
        />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.stepper {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--hairline);
  border-radius: var(--radius-s);
  background: var(--bg-content);
}
.st-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: calc(var(--radius-s) - 2px);
  background: transparent;
  color: var(--text-2);
  cursor: pointer;
  transition:
    background-color 110ms ease,
    color 110ms ease;
}
.st-btn:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text);
}
.st-btn:active:not(:disabled) {
  background: var(--bg-active);
}
.st-btn:disabled {
  color: var(--text-3);
  opacity: 0.55;
  cursor: default;
}
.st-btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
.st-value {
  min-width: 44px;
  text-align: center;
  font-size: calc(12px * var(--font-scale, 1));
  font-variant-numeric: tabular-nums;
  color: var(--text);
  user-select: none;
}
</style>
