<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import Icon from "./Icon.vue";
import { t } from "../i18n";

const props = defineProps<{ title: string; wide?: boolean; busy?: boolean }>();
const emit = defineEmits<{ (e: "close"): void }>();

// 弹窗显隐由组件内部驱动，<Transition> 据此播放开/关动画；
// 关闭 = 先播离场动画，结束后再 emit("close") 让宿主卸载本组件。
const show = ref(false);
const CLOSE_MS = 260; // 与离场过渡时长一致并留余量，见 base.css 的 modal-leave
let closeTimer: number | undefined;

onMounted(() => {
  window.addEventListener("keydown", onKey);
  // 双 rAF 确保初始态先上屏，入场过渡从当前值出发
  requestAnimationFrame(() => requestAnimationFrame(() => (show.value = true)));
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey);
  window.clearTimeout(closeTimer);
});

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape" && !props.busy) requestClose();
}

/** 关闭入口：ESC / 遮罩 / 标题栏按钮 / 宿主的程序化关闭，统一走这里 */
function requestClose() {
  if (props.busy) return;
  if (!show.value) {
    // 尚未完成入场（快速操作），无需离场动画
    window.clearTimeout(closeTimer);
    emit("close");
    return;
  }
  show.value = false;
  window.clearTimeout(closeTimer);
  closeTimer = window.setTimeout(() => emit("close"), CLOSE_MS);
}

defineExpose({ requestClose });
</script>

<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="show" class="modal-overlay" @click.self="requestClose">
        <div class="modal-panel" :class="{ wide }" role="dialog" aria-modal="true">
          <header class="modal-head">
            <h2>{{ title }}</h2>
            <button class="icon-btn" :disabled="busy" :aria-label="t('common.close')" @click="requestClose">
              <Icon name="close" :size="15" />
            </button>
          </header>
          <div class="modal-body">
            <slot />
          </div>
          <footer v-if="$slots.footer" class="modal-foot">
            <slot name="footer" />
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
