<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { refreshConfig, refreshVolumes, store, toast } from "../store";
import { t } from "../i18n";
import type { SyncPair } from "../types";
import Modal from "./Modal.vue";

const editing = store.editor?.pair ?? null;

const form = reactive({
  id: editing?.id ?? "",
  name: editing?.name ?? "",
  localPath: editing?.localPath ?? "",
  deviceLabel: editing?.deviceLabel ?? "",
  remotePath: editing?.remotePath ?? "",
  enabled: editing?.enabled ?? true,
});

const error = ref("");
const saving = ref(false);

const hasVolumes = computed(() => store.volumes.length > 0);

onMounted(() => {
  if (!hasVolumes.value) void refreshVolumes();
});

async function pickLocal() {
  const path = await open({ directory: true, multiple: false, title: t("editor.local_pick") });
  if (typeof path === "string") form.localPath = path;
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
    enabled: form.enabled,
    createdAt: editing?.createdAt ?? 0,
    lastSyncAt: editing?.lastSyncAt ?? null,
  };

  saving.value = true;
  try {
    await api.upsertPair(pair);
    await Promise.all([refreshConfig(), refreshVolumes()]);
    toast("success", t("toast.pair_saved"));
    store.editor = null;
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}

function close() {
  store.editor = null;
}
</script>

<template>
  <Modal :title="editing ? t('editor.edit') : t('editor.new')" :busy="saving" @close="close">
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
        <span class="field-label">{{ t("editor.local") }}</span>
        <div class="field-row">
          <input v-model="form.localPath" class="input grow" spellcheck="false" />
          <button class="btn" @click="pickLocal">{{ t("editor.local_pick") }}</button>
        </div>
      </div>

      <div class="field">
        <span class="field-label">{{ t("editor.device") }}</span>
        <select v-model="form.deviceLabel" class="input grow">
          <option value="" disabled>{{ t("editor.device_hint") }}</option>
          <option v-for="v in store.volumes" :key="v.mount" :value="v.label">
            {{ v.label }}（{{ v.mount }}）
          </option>
        </select>
        <p v-if="!hasVolumes" class="field-hint warn">{{ t("editor.device_none") }}</p>
      </div>

      <label class="field">
        <span class="field-label">{{ t("editor.remote") }}</span>
        <input
          v-model="form.remotePath"
          class="input"
          :placeholder="t('editor.remote_placeholder')"
          spellcheck="false"
        />
      </label>
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
.field-hint.warn {
  color: var(--orange-text);
}
.form-error {
  margin-top: 12px;
  font-size: calc(12.5px * var(--font-scale, 1));
  color: var(--red-text);
}
.foot-actions {
  display: flex;
  gap: 8px;
}
</style>
