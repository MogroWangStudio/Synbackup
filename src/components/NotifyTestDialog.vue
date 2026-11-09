<script setup lang="ts">
// 测试通知窗口：标题 / 内容 / 按钮全部可自定义后发送。
// 按钮即系统通知的动作：macOS 走 UNNotificationAction，Windows 走 toast
// 的 <action>（经 tauri-winrt-notification）；Linux 的 Desktop Notifications
// 规范在 notify-rust 下没有动作能力——仅 Linux 隐藏按钮编辑，只发标题与内容。
import { nextTick, ref } from "vue";
import Modal from "./Modal.vue";
import Icon from "./Icon.vue";
import { api } from "../api";
import { notifyError, t } from "../i18n";
import { toast } from "../store";

const emit = defineEmits<{ (e: "close"): void }>();

const os = document.documentElement.dataset.os;
const supportsActions = os === "mac" || os === "windows";
const MAX_BUTTONS = 3;

const modalRef = ref<InstanceType<typeof Modal>>();
const title = ref("");
const body = ref("");
const rows = ref<string[]>([]);
const sending = ref(false);

function close() {
  modalRef.value?.requestClose();
}

function addRow() {
  if (rows.value.length < MAX_BUTTONS) rows.value.push("");
}

async function send() {
  sending.value = true;
  let ok = false;
  try {
    await api.testNotification({
      title: title.value.trim() || undefined,
      body: body.value.trim() || undefined,
      buttons: rows.value.map((s) => s.trim()).filter((s) => s !== ""),
    });
    toast("info", t("ntest.sent"));
    ok = true;
  } catch (e) {
    toast("error", notifyError(String(e)));
  } finally {
    // 先解除 busy；busy prop 要等本次重渲染 flush 后才到达 Modal，
    // 所以关闭前先 nextTick 等它落地（与 PairEditor 同一坑）
    sending.value = false;
    await nextTick();
  }
  if (ok) close();
}
</script>

<template>
  <Modal ref="modalRef" :title="t('ntest.title')" :busy="sending" @close="emit('close')">
    <div class="form">
      <label class="field">
        <span class="field-label">{{ t("ntest.subject") }}</span>
        <input
          v-model="title"
          class="input"
          maxlength="50"
          :placeholder="t('ntest.subject_placeholder')"
          spellcheck="false"
        />
      </label>

      <label class="field">
        <span class="field-label">{{ t("ntest.body") }}</span>
        <textarea
          v-model="body"
          class="input area"
          rows="3"
          maxlength="300"
          :placeholder="t('ntest.body_placeholder')"
          spellcheck="false"
        ></textarea>
      </label>

      <div v-if="supportsActions" class="field">
        <span class="field-label">{{ t("ntest.buttons") }}</span>
        <div class="btn-rows">
          <div v-for="(_, i) in rows" :key="i" class="btn-row">
            <input
              v-model="rows[i]"
              class="input"
              maxlength="30"
              :placeholder="t('ntest.button_placeholder')"
              spellcheck="false"
            />
            <button
              type="button"
              class="icon-btn"
              :aria-label="t('ntest.remove')"
              :title="t('ntest.remove')"
              @click="rows.splice(i, 1)"
            >
              <Icon name="close" :size="13" />
            </button>
          </div>
          <button
            v-if="rows.length < MAX_BUTTONS"
            type="button"
            class="btn btn-sm"
            @click="addRow"
          >
            {{ t("ntest.add_button") }}
          </button>
        </div>
        <p class="field-hint">{{ t("ntest.buttons_hint") }}</p>
      </div>
      <p v-else class="field-hint">{{ t("ntest.unsupported") }}</p>
    </div>

    <template #footer>
      <span />
      <div class="foot-actions">
        <button class="btn" @click="close">{{ t("common.cancel") }}</button>
        <button class="btn btn-primary" :disabled="sending" @click="send">
          {{ t("ntest.send") }}
        </button>
      </div>
    </template>
  </Modal>
</template>

<style scoped>
/* 表单骨架与 PairEditor 同一套（field 系列在各组件 scoped，不跨文件） */
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
.field-hint {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
}
/* 输入占满字段宽度（.input 本身不定宽） */
.field .input {
  width: 100%;
}
.area {
  resize: none;
  line-height: 1.5;
}
/* 按钮编辑行：输入占满 + 行尾移除按钮；“添加按钮”不随列拉伸 */
.btn-rows {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.btn-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.btn-row .input {
  flex: 1;
  min-width: 0;
}
.btn-rows > .btn {
  align-self: flex-start;
}
</style>
