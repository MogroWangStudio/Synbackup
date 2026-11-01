<script setup lang="ts">
import { store } from "../store";
import { t } from "../i18n";
import Icon from "../components/Icon.vue";
import PairCard from "../components/PairCard.vue";
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t("pairs.title") }}</h1>
        <p class="page-desc">{{ t("pairs.desc") }}</p>
      </div>
      <button class="btn btn-primary" @click="store.editor = { pair: null }">
        <Icon name="plus" :size="14" />
        {{ t("pairs.add") }}
      </button>
    </div>

    <div v-if="store.pairs.length" class="pair-list">
      <PairCard v-for="pair in store.pairs" :key="pair.id" :pair="pair" />
    </div>

    <div v-else class="card empty">
      <Icon name="sync" :size="26" class="empty-icon" />
      <p class="empty-title">{{ t("overview.empty_pairs_title") }}</p>
      <p class="empty-desc">{{ t("overview.empty_pairs_desc") }}</p>
      <button class="btn btn-primary" @click="store.editor = { pair: null }">
        <Icon name="plus" :size="14" />
        {{ t("overview.empty_pairs_cta") }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.pair-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
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
  font-size: 14px;
  font-weight: 600;
}
.empty-desc {
  font-size: 12.5px;
  color: var(--text-2);
  max-width: 340px;
}
.empty .btn {
  margin-top: 6px;
}
</style>
