<script setup lang="ts">
/* 版本列表：拉取全部发布，选中任意版本安装（升级或降级）。
   降级是危险动作——可能出现部分功能不可用或已知漏洞，选择旧版本时
   常驻警告条，且需要两步确认（第一次点只把按钮换成"确认降级"）。
   确认后把该版本构造成更新信息交给更新向导，下载与安装走同一条链路。 */
import { computed, onMounted, ref } from "vue";
import { api } from "../api";
import { store, toast } from "../store";
import { fmtDate, t } from "../i18n";
import type { UpdateInfo, VersionEntry } from "../types";
import Icon from "./Icon.vue";
import Modal from "./Modal.vue";

const emit = defineEmits<{ (e: "close"): void }>();
const modalRef = ref<InstanceType<typeof Modal>>();

const loading = ref(true);
const error = ref("");
const versions = ref<VersionEntry[]>([]);
const selected = ref<VersionEntry | null>(null);
/** 降级两步确认的第二步 */
const confirming = ref(false);

const currentVersion = computed(() => store.appInfo?.version ?? "");

onMounted(async () => {
  try {
    versions.value = await api.listVersions();
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
});

function pick(v: VersionEntry) {
  if (v.isCurrent) return;
  selected.value = v;
  confirming.value = false;
}

/** 选中旧版本 → 常驻警告；第一次点只换成"确认降级"，再点才执行 */
function proceed() {
  const v = selected.value;
  if (!v) return;
  if (v.isDowngrade && !confirming.value) {
    confirming.value = true;
    return;
  }
  const info: UpdateInfo = {
    available: true,
    version: v.version,
    notes: v.notes,
    url: v.url,
    current: currentVersion.value,
    asset: v.asset,
    assetUrl: v.assetUrl,
    auto: v.auto,
  };
  store.updateWizard = info;
  modalRef.value?.requestClose();
}

function openPage() {
  const v = selected.value;
  if (v) void api.openExternal(v.url).catch((e) => toast("error", String(e)));
}

function close() {
  modalRef.value?.requestClose();
}
</script>

<template>
  <Modal ref="modalRef" :title="t('vlist.title')" @close="emit('close')">
    <p class="vlist-hint">{{ t("vlist.hint") }}</p>

    <div v-if="loading" class="vlist-state">
      <span class="vlist-pulse" />{{ t("vlist.loading") }}
    </div>
    <div v-else-if="error" class="vlist-state error">
      <Icon name="alert" :size="14" />
      <span>{{ error }}</span>
    </div>
    <p v-else-if="!versions.length" class="vlist-state">{{ t("vlist.empty") }}</p>

    <div v-else class="vlist" role="listbox" :aria-label="t('vlist.title')">
      <button
        v-for="v in versions"
        :key="v.version"
        type="button"
        class="vlist-row"
        role="option"
        :aria-selected="selected?.version === v.version"
        :class="{ 'is-selected': selected?.version === v.version, 'is-current': v.isCurrent }"
        @click="pick(v)"
      >
        <span class="vlist-ver">v{{ v.version }}</span>
        <span v-if="v.isPrerelease" class="chip chip-gray">{{ t("vlist.prerelease") }}</span>
        <span v-if="v.isCurrent" class="chip chip-accent">{{ t("vlist.current") }}</span>
        <span class="vlist-date">{{ v.updated ? fmtDate(new Date(v.updated).getTime()) : "" }}</span>
      </button>
    </div>

    <!-- 降级警告：选中旧版本即常驻 -->
    <div v-if="selected?.isDowngrade" class="downgrade-warn" role="alert">
      <Icon name="alert" :size="14" class="warn-icon" />
      <span>{{ t("vlist.downgrade_hint") }}</span>
    </div>

    <template #footer>
      <span class="vlist-picked">{{
        selected ? `v${selected.version}` : ""
      }}</span>
      <div class="foot-actions">
        <button class="btn" @click="close">{{ t("common.close") }}</button>
        <button
          v-if="selected && !selected.auto"
          class="btn btn-sm"
          @click="openPage"
        >
          {{ t("upd.manual") }}
        </button>
        <button
          v-else-if="selected"
          class="btn btn-primary"
          @click="proceed"
        >
          {{
            selected.isDowngrade
              ? confirming
                ? t("vlist.confirm_downgrade")
                : t("vlist.downgrade")
              : t("vlist.install")
          }}
        </button>
      </div>
    </template>
  </Modal>
</template>

<style scoped>
.vlist-hint {
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-3);
  line-height: 1.55;
  margin-bottom: 10px;
}
.vlist {
  max-height: 320px;
  overflow-y: auto;
  border: 1px solid var(--hairline);
  border-radius: var(--radius-m);
}
.vlist-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  height: 38px;
  padding: 0 12px;
  border: none;
  border-bottom: 1px solid var(--hairline);
  background: transparent;
  color: var(--text);
  font-size: calc(12.5px * var(--font-scale, 1));
  text-align: left;
  cursor: pointer;
}
.vlist-row:last-child {
  border-bottom: none;
}
.vlist-row:hover {
  background: var(--bg-hover);
}
/* 按下即变深：选中反馈不能等到松手 */
.vlist-row:active {
  background: var(--bg-active);
}
.vlist-row.is-selected {
  background: var(--accent-soft);
}
.vlist-row.is-current {
  cursor: default;
}
.vlist-ver {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  min-width: 110px;
}
.vlist-date {
  margin-left: auto;
  font-size: calc(11px * var(--font-scale, 1));
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.vlist-state {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 96px;
  justify-content: center;
  font-size: calc(12.5px * var(--font-scale, 1));
  color: var(--text-3);
}
.vlist-state.error {
  color: var(--red-text);
  user-select: text;
}
/* 加载中的呼吸点（与更新向导的 installing 同款语言） */
.vlist-pulse {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex: none;
  background: var(--accent);
  animation: vlist-pulse 1.4s var(--ease) infinite;
}
@keyframes vlist-pulse {
  0%,
  100% {
    opacity: 0.35;
    transform: scale(0.85);
  }
  50% {
    opacity: 1;
    transform: scale(1);
  }
}
@media (prefers-reduced-motion: reduce) {
  .vlist-pulse {
    animation: none;
    opacity: 1;
  }
}
/* 降级警告：橙色警示条，选中旧版本即常驻 */
.downgrade-warn {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin-top: 10px;
  padding: 9px 11px;
  border-radius: var(--radius-s);
  background: var(--orange-soft);
  color: var(--orange-text);
  font-size: calc(11.5px * var(--font-scale, 1));
  line-height: 1.55;
}
.warn-icon {
  flex: none;
  margin-top: 2px;
}
.vlist-picked {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--mono);
  font-size: calc(10.5px * var(--font-scale, 1));
  color: var(--text-3);
}
</style>
