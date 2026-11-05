<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { refreshConfig, refreshVolumes, store, toast } from "../store";
import { t } from "../i18n";
import type { SyncDirection, SyncPair } from "../types";
import Icon from "./Icon.vue";
import Modal from "./Modal.vue";
import Select from "./Select.vue";
import type { SelectOption } from "./Select.vue";

const modalRef = ref<InstanceType<typeof Modal>>();

const editing = store.editor?.pair ?? null;

const form = reactive({
  id: editing?.id ?? "",
  name: editing?.name ?? "",
  localPath: editing?.localPath ?? "",
  deviceLabel: editing?.deviceLabel ?? "",
  remotePath: editing?.remotePath ?? "",
  direction: (editing?.direction ?? "push") as SyncDirection,
  enabled: editing?.enabled ?? true,
});

const error = ref("");
const saving = ref(false);

const hasVolumes = computed(() => store.volumes.length > 0);

const deviceOptions = computed<SelectOption[]>(() => [
  { value: "", label: t("editor.device_hint"), disabled: true },
  ...store.volumes.map((v) => ({ value: v.label, label: `${v.label}（${v.mount}）` })),
]);

onMounted(() => {
  if (!hasVolumes.value) void refreshVolumes();
});

async function pickLocal() {
  const path = await open({ directory: true, multiple: false, title: t("editor.local_pick") });
  if (typeof path === "string") form.localPath = path;
}

/** 交换发送端 / 接收端（本机 → 设备 或 设备 → 本机） */
function swapDirection() {
  form.direction = form.direction === "push" ? "pull" : "push";
}

/**
 * 浏览设备上的文件夹：系统目录对话框里选中设备卷内的文件夹，
 * 自动换算成相对设备根目录的路径填入（设备路径本身不是绝对定位的）。
 */
async function pickRemote() {
  if (!form.deviceLabel) return void (error.value = t("editor.err_device"));
  const vol = store.volumes.find((v) => v.label === form.deviceLabel);
  if (!vol) return;
  const path = await open({ directory: true, multiple: false, title: t("editor.remote_pick") });
  if (typeof path !== "string") return;
  if (path !== vol.mount && !path.startsWith(vol.mount + "/")) {
    error.value = t("editor.err_remote_volume");
    return;
  }
  error.value = "";
  form.remotePath = path === vol.mount ? "" : path.slice(vol.mount.length + 1);
}

async function save() {
  if (saving.value) return;
  error.value = "";
  if (!form.name.trim()) return void (error.value = t("editor.err_name"));
  if (!form.localPath) return void (error.value = t("editor.err_local"));
  if (!form.deviceLabel) return void (error.value = t("editor.err_device"));

  const pair: SyncPair = {
    id: form.id,
    name: form.name,
    localPath: form.localPath,
    deviceLabel: form.deviceLabel,
    deviceMarker: editing?.deviceMarker ?? null,
    remotePath: form.remotePath,
    direction: form.direction,
    enabled: form.enabled,
    createdAt: editing?.createdAt ?? 0,
    lastSyncAt: editing?.lastSyncAt ?? null,
  };

  saving.value = true;
  try {
    await api.upsertPair(pair);
    await Promise.all([refreshConfig(), refreshVolumes()]);
    toast("success", t("toast.pair_saved"));
    saving.value = false; // 先解除 busy，Modal 才接受关闭
    // 走 Modal 的关闭流程：先播离场动画，结束后由 @close 卸载
    modalRef.value?.requestClose();
  } catch (e) {
    error.value = String(e);
    saving.value = false;
  }
}

function close() {
  modalRef.value?.requestClose();
}

function handleClose() {
  store.editor = null;
}
</script>

<template>
  <Modal
    ref="modalRef"
    :title="editing ? t('editor.edit') : t('editor.new')"
    :busy="saving"
    @close="handleClose"
  >
    <div class="form">
      <label class="field">
        <span class="field-label">{{ t("editor.name") }}</span>
        <input
          v-model="form.name"
          class="input"
          :placeholder="t('editor.name_placeholder')"
          spellcheck="false"
        />
      </label>

      <div class="field">
        <span class="field-label">{{ t("editor.direction") }}</span>
        <div class="direction-bar">
          <div class="dir-end">
            <Icon name="folder" :size="16" class="dir-icon" />
            <div class="dir-text">
              <span class="dir-side">
                {{ t(form.direction === "push" ? "editor.side_send" : "editor.side_recv") }}
              </span>
              <span class="dir-name">{{ t("editor.local") }}</span>
            </div>
          </div>
          <button
            type="button"
            class="dir-swap"
            :title="t('editor.swap')"
            :aria-label="t('editor.swap')"
            @click="swapDirection"
          >
            <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true">
              <path
                d="M4 7h11m0 0-3.2-3.2M15 7l-3.2 3.2M16 13H5m0 0 3.2 3.2M5 13l3.2-3.2"
                fill="none"
                stroke="currentColor"
                stroke-width="1.6"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>
          <div class="dir-end">
            <Icon name="drive" :size="16" class="dir-icon" />
            <div class="dir-text">
              <span class="dir-side">
                {{ t(form.direction === "push" ? "editor.side_recv" : "editor.side_send") }}
              </span>
              <span class="dir-name">
                {{ form.deviceLabel || t("editor.device_hint") }}
              </span>
            </div>
          </div>
        </div>
        <p class="field-hint">{{ t("editor.direction_hint") }}</p>
      </div>

      <div class="field">
        <span class="field-label">{{ t("editor.local") }}</span>
        <div class="field-row">
          <input v-model="form.localPath" class="input grow" spellcheck="false" />
          <button class="btn" @click="pickLocal">{{ t("editor.local_pick") }}</button>
        </div>
      </div>

      <div class="field">
        <span class="field-label">{{ t("editor.device") }}</span>
        <Select v-model="form.deviceLabel" :options="deviceOptions" fill :aria-label="t('editor.device')" />
        <p v-if="!hasVolumes" class="field-hint warn">{{ t("editor.device_none") }}</p>
      </div>

      <div class="field">
        <span class="field-label">{{ t("editor.remote") }}</span>
        <div class="field-row">
          <input
            v-model="form.remotePath"
            class="input grow"
            :placeholder="t('editor.remote_placeholder')"
            spellcheck="false"
          />
          <button class="btn" :disabled="!form.deviceLabel" @click="pickRemote">
            {{ t("editor.remote_pick") }}
          </button>
        </div>
        <p class="field-hint">{{ t("editor.remote_hint") }}</p>
      </div>
    </div>

    <p v-if="error" class="form-error">{{ error }}</p>

    <template #footer>
      <span />
      <div class="foot-actions">
        <button class="btn" @click="close">{{ t("common.cancel") }}</button>
        <button class="btn btn-primary" :disabled="saving" @click="save">
          {{ t("common.save") }}
        </button>
      </div>
    </template>
  </Modal>
</template>

<style scoped>
.form {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding-top: 2px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.field-label {
  font-size: calc(12.5px * var(--font-scale, 1));
  font-weight: 600;
  color: var(--text-2);
}
.field-row {
  display: flex;
  gap: 8px;
}
.grow {
  flex: 1;
  min-width: 0;
}
.field-hint {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
}

/* 方向条：发送端 ⇆ 接收端，中间为交换按钮 */
.direction-bar {
  display: flex;
  align-items: stretch;
  gap: 8px;
}
.dir-end {
  display: flex;
  align-items: center;
  gap: 9px;
  flex: 1;
  min-width: 0;
  padding: 9px 12px;
  border: 1px solid var(--hairline);
  border-radius: var(--radius-m);
  background: var(--fill-softer);
}
.dir-icon {
  color: var(--accent);
  flex: none;
}
.dir-text {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}
.dir-side {
  font-size: calc(10.5px * var(--font-scale, 1));
  font-weight: 600;
  color: var(--accent-text);
}
.dir-name {
  font-size: calc(12.5px * var(--font-scale, 1));
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dir-swap {
  align-self: center;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  flex: none;
  padding: 0;
  border: 1px solid var(--hairline-strong);
  border-radius: 50%;
  background: var(--bg-content);
  color: var(--text-2);
  cursor: pointer;
  transition:
    background-color 110ms ease,
    color 110ms ease,
    transform 120ms var(--ease);
}
.dir-swap:hover {
  background: var(--bg-hover);
  color: var(--text);
}
.dir-swap:active {
  transform: scale(0.92);
}
.dir-swap:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.field-hint.warn {
  color: var(--orange-text);
}
.form-error {
  margin-top: 12px;
  font-size: calc(12.5px * var(--font-scale, 1));
  color: var(--red-text);
}
</style>
