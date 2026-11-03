<script setup lang="ts">
// 关闭窗口确认：仅退出窗口（后端托盘常驻）或完全退出（结束进程）。
// 关闭 / ESC / 遮罩 = 取消，回到应用不关窗。
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "../api";
import { store } from "../store";
import { t } from "../i18n";
import Modal from "./Modal.vue";

const win = getCurrentWindow();

function cancel() {
  store.closeConfirm = false;
}

function closeWindow() {
  store.closeConfirm = false;
  // 直接销毁窗口：后端保留在托盘继续运行
  void win.destroy();
}

function quitApp() {
  store.closeConfirm = false;
  void api.exitApp().catch(() => {});
}
</script>

<template>
  <Modal :title="t('close.title')" @close="cancel">
    <p class="close-desc">{{ t("close.desc") }}</p>
    <template #footer>
      <div class="foot-actions">
        <button class="btn" @click="quitApp">{{ t("close.quit") }}</button>
        <button class="btn btn-primary" @click="closeWindow">{{ t("close.window") }}</button>
      </div>
    </template>
  </Modal>
</template>

<style scoped>
.close-desc {
  font-size: calc(13px * var(--font-scale, 1));
  line-height: 1.6;
  color: var(--text-2);
  padding: 2px 0 6px;
}
.foot-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}
</style>
