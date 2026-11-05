<script setup lang="ts">
// 关闭窗口确认：仅退出窗口（后端托盘常驻）或完全退出（结束进程）。
// 关闭 / ESC / 遮罩 = 取消，回到应用不关窗。
import { ref } from "vue";
import { getCurrentWindow, Window } from "@tauri-apps/api/window";
import { api } from "../api";
import { store } from "../store";
import { t } from "../i18n";
import Modal from "./Modal.vue";

const busy = ref(false);

function cancel() {
  if (busy.value) return;
  store.closeConfirm = false;
}

function closeWindow() {
  if (busy.value) return;
  busy.value = true;
  store.closeConfirm = false;
  // 直接销毁窗口：后端保留在托盘继续运行。
  // 优先走前端 destroy IPC；被 capability 拒绝或句柄失效时，
  // 逐级回退：重取句柄 → 后端 destroy_main_window 命令（无权限限制）。
  const fallbackToBackend = (e: unknown) => {
    console.error("[synbackup] 前端销毁窗口失败，改用后端命令", e);
    void api.destroyMainWindow().catch((e2) => {
      console.error("[synbackup] 后端销毁窗口也失败", e2);
      busy.value = false;
    });
  };
  getCurrentWindow()
    .destroy()
    .catch((e) =>
      Window.getByLabel("main")
        .then((w2) => w2?.destroy())
        .catch((e2) => {
          void e2;
          fallbackToBackend(e);
        }),
    );
}

function quitApp() {
  if (busy.value) return;
  busy.value = true;
  store.closeConfirm = false;
  void api.exitApp().catch((e) => {
    console.error("[synbackup] 退出应用失败", e);
    busy.value = false;
  });
}
</script>

<template>
  <Modal :title="t('close.title')" @close="cancel">
    <p class="close-desc">{{ t("close.desc") }}</p>
    <template #footer>
      <div class="foot-actions">
        <button class="btn" :disabled="busy" @click="quitApp">{{ t("close.quit") }}</button>
        <button class="btn btn-primary" :disabled="busy" @click="closeWindow">
          {{ t("close.window") }}
        </button>
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
/* 按钮固定在窗口右下角：Modal 的 footer 默认 space-between（左侧留空给批量操作），
   确认框没有左侧内容，改为整体右对齐（display / gap 在 base.css 的 .foot-actions） */
.foot-actions {
  justify-content: flex-end;
  flex: 1;
}
</style>
