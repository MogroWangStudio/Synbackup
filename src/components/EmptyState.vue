<script setup lang="ts">
import Icon from "./Icon.vue";

// 列表为空时的占位卡片：图标 + 标题 + 可选说明 + 可选动作（插槽）。
// Overview / Pairs / Devices 共用，此前是各自复制模板与样式。
defineProps<{ icon?: string; title: string; desc?: string }>();
</script>

<template>
  <div class="card empty">
    <Icon v-if="icon" :name="icon" :size="26" class="empty-icon" />
    <p class="empty-title">{{ title }}</p>
    <p v-if="desc" class="empty-desc">{{ desc }}</p>
    <slot />
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 40px 20px;
  text-align: center;
}
.empty-icon {
  color: var(--text-3);
}
.empty-title {
  font-size: calc(14px * var(--font-scale, 1));
  font-weight: 600;
}
.empty-desc {
  font-size: calc(12.5px * var(--font-scale, 1));
  color: var(--text-2);
  max-width: 340px;
}
/* 动作按钮来自父组件插槽，需穿透 scoped 才能命中 */
.empty :deep(.btn) {
  margin-top: 6px;
}
</style>
