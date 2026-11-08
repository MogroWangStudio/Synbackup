<script setup lang="ts">
// 字体浏览弹窗：枚举系统字体（后端 list_system_fonts），按语言分组展示，
// 支持按名称搜索与语言筛选；条目以该字体实时渲染预览；点击「使用」应用。
import { computed, onMounted, ref } from "vue";
import Modal from "./Modal.vue";
import Select from "./Select.vue";
import { api } from "../api";
import { t } from "../i18n";
import type { SelectOption } from "./Select.vue";

const emit = defineEmits<{
  (e: "close"): void;
  (e: "use", family: string): void;
}>();

const props = defineProps<{ current?: string }>();

interface FontFamilyInfo {
  family: string;
  languages: string[];
  localizedNames: string[];
  faces: number;
}

const loading = ref(true);
const fonts = ref<FontFamilyInfo[]>([]);
const query = ref("");
const langFilter = ref("all");

onMounted(async () => {
  try {
    fonts.value = (await api.listSystemFonts()) ?? [];
  } catch {
    fonts.value = [];
  } finally {
    loading.value = false;
  }
});

const LANG_ORDER = ["zh-Hans", "zh-Hant", "ja", "ko", "latin", "other"];
const langFilterOptions: SelectOption[] = [
  { value: "all", label: t("fontb.all") },
  ...LANG_ORDER.map((l) => ({ value: l, label: t(`lang.${l}`) })),
];

/** 字族的展示分组：取语言标签中的最高优先项（CJK 优先），不重复出现 */
function groupOf(f: FontFamilyInfo): string {
  return LANG_ORDER.find((l) => f.languages.includes(l)) ?? "other";
}

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  return fonts.value.filter((f) => {
    if (langFilter.value !== "all" && !f.languages.includes(langFilter.value)) return false;
    if (!q) return true;
    return (
      f.family.toLowerCase().includes(q) ||
      f.localizedNames.some((n) => n.toLowerCase().includes(q))
    );
  });
});

/** 分组展示：按语言优先序分组，组内按名称排序（"other" 已在 LANG_ORDER 内，无需补组） */
const grouped = computed(() => {
  const groups: { lang: string; fonts: FontFamilyInfo[] }[] = [];
  for (const lang of LANG_ORDER) {
    if (langFilter.value !== "all" && langFilter.value !== lang) continue;
    const list = filtered.value.filter((f) => groupOf(f) === lang);
    if (list.length) groups.push({ lang, fonts: list });
  }
  return groups;
});

const totalShown = computed(() => grouped.value.reduce((n, g) => n + g.fonts.length, 0));

function use(family: string) {
  emit("use", family);
}

const modalRef = ref<InstanceType<typeof Modal>>();

function previewStyle(f: FontFamilyInfo) {
  return { fontFamily: `"${f.family.replace(/"/g, '\\"')}", sans-serif` };
}
</script>

<template>
  <Modal ref="modalRef" :title="t('fontb.title')" wide @close="emit('close')">
    <div class="fb-toolbar">
      <input v-model="query" class="input grow" :placeholder="t('fontb.search')" spellcheck="false" />
      <Select
        class="fb-lang"
        :model-value="langFilter"
        :options="langFilterOptions"
        :aria-label="t('fontb.filter')"
        @update:model-value="(v) => (langFilter = v)"
      />
    </div>

    <div class="fb-list">
      <div v-if="loading" class="fb-empty">{{ t("fontb.loading") }}</div>
      <template v-else-if="totalShown">
        <template v-for="g in grouped" :key="g.lang">
          <div class="fb-group">{{ t(`lang.${g.lang}`) }} · {{ g.fonts.length }}</div>
          <button
            v-for="f in g.fonts"
            :key="f.family"
            type="button"
            class="fb-item"
            :class="{ current: f.family === props.current }"
            @click="use(f.family)"
          >
            <span class="fb-preview" :style="previewStyle(f)">
              {{ f.family }} Aa 字体示例 123
            </span>
            <span class="fb-meta">
              <span v-if="f.localizedNames.length" class="fb-localized">
                {{ f.localizedNames.join(" / ") }}
              </span>
              <span class="fb-faces">{{ t("fontb.faces", { n: f.faces }) }}</span>
              <span v-if="f.family === current" class="chip chip-accent fb-using">
                {{ t("fontb.using") }}
              </span>
            </span>
          </button>
        </template>
      </template>
      <div v-else class="fb-empty">{{ t("fontb.empty") }}</div>
    </div>
  </Modal>
</template>

<style scoped>
.fb-toolbar {
  display: flex;
  gap: 8px;
  margin-bottom: 10px;
}
.fb-lang {
  width: 150px;
  flex: none;
}
.fb-list {
  max-height: 380px;
  overflow-y: auto;
  border: 1px solid var(--hairline);
  border-radius: var(--radius-m);
}
.fb-group {
  padding: 8px 12px 4px;
  font-size: calc(11px * var(--font-scale, 1));
  font-weight: 600;
  color: var(--text-3);
  background: var(--fill-softer);
  position: sticky;
  top: 0;
}
.fb-item {
  display: block;
  width: 100%;
  padding: 8px 12px;
  border: none;
  background: transparent;
  color: var(--text);
  text-align: left;
  cursor: pointer;
  transition: background 120ms ease;
}
.fb-item:hover {
  background: var(--bg-hover);
}
.fb-item.current {
  background: var(--accent-soft);
}
.fb-preview {
  display: block;
  font-size: calc(15px * var(--font-scale, 1));
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fb-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 2px;
  font-size: calc(11px * var(--font-scale, 1));
  color: var(--text-3);
  overflow: hidden;
  white-space: nowrap;
}
.fb-localized {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
.fb-faces {
  flex: none;
}
.fb-using {
  flex: none;
}
.fb-empty {
  padding: 24px 12px;
  text-align: center;
  font-size: calc(12.5px * var(--font-scale, 1));
  color: var(--text-3);
}
</style>
