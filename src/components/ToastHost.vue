<script setup lang="ts">
import { dismissToast, store } from "../store";
import Icon from "./Icon.vue";

const iconFor = (kind: string) => (kind === "success" ? "check" : kind === "error" ? "alert" : "info");
</script>

<template>
  <Teleport to="body">
    <div class="toast-host">
      <TransitionGroup name="toast">
        <div v-for="toast in store.toasts" :key="toast.id" class="toast" :class="toast.kind">
          <Icon :name="iconFor(toast.kind)" :size="14" />
          <span class="toast-text">{{ toast.text }}</span>
          <button class="toast-close" @click="dismissToast(toast.id)">
            <Icon name="close" :size="11" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-host {
  position: fixed;
  right: 16px;
  bottom: 16px;
  z-index: 200;
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-end;
}
.toast {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 380px;
  padding: 8px 10px 8px 12px;
  border-radius: var(--radius-m);
  background: rgba(28, 28, 30, 0.92);
  backdrop-filter: blur(16px);
  color: #f5f5f7;
  font-size: 12.5px;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.3);
}
.toast.success :deep(svg) {
  color: #4cd471;
}
.toast.error :deep(svg) {
  color: #ff6961;
}
.toast.info :deep(svg) {
  color: #6db2ff;
}
.toast-text {
  flex: 1;
}
.toast-close {
  display: inline-flex;
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.55);
  cursor: pointer;
  padding: 3px;
  border-radius: 5px;
}
.toast-close:hover {
  color: #fff;
}
</style>
