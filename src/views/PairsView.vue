<script setup lang="ts">
import { store } from "../store";
import { t } from "../i18n";
import Icon from "../components/Icon.vue";
import PairCard from "../components/PairCard.vue";
import EmptyState from "../components/EmptyState.vue";
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t("pairs.title") }}</h1>
      </div>
      <button class="btn btn-primary" @click="store.editor = { pair: null }">
        <Icon name="plus" :size="14" />
        {{ t("pairs.add") }}
      </button>
    </div>

    <div v-if="store.pairs.length" class="pair-list">
      <PairCard v-for="pair in store.pairs" :key="pair.id" :pair="pair" />
    </div>

    <EmptyState
      v-else
      icon="sync"
      :title="t('overview.empty_pairs_title')"
      :desc="t('overview.empty_pairs_desc')"
    >
      <button class="btn btn-primary" @click="store.editor = { pair: null }">
        <Icon name="plus" :size="14" />
        {{ t("overview.empty_pairs_cta") }}
      </button>
    </EmptyState>
  </div>
</template>

<style scoped>
.pair-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
</style>
