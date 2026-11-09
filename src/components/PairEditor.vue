<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref } from "vue";
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

/** 目标设备是否在线（未选设备视为不在线） */
const deviceOnline = computed(
  () => !!form.deviceLabel && store.volumes.some((v) => v.label === form.deviceLabel),
);

const deviceOptions = computed<SelectOption[]>(() => {
  const opts: SelectOption[] = [
    { value: "", label: t("editor.device_hint"), disabled: true },
    ...store.volumes.map((v) => ({ value: v.label, label: `${v.label}（${v.mount}）` })),
  ];
  // 任务目标设备离线：仅显示设备名称（无盘符可选），置灰不可选
  if (form.deviceLabel && !deviceOnline.value) {
    opts.push({ value: form.deviceLabel, label: form.deviceLabel, disabled: true });
  }
  return opts;
});

onMounted(() => {
  if (!hasVolumes.value) void refreshVolumes();
});

async function pickLocal() {
  const path = await open({ directory: true, multiple: false, title: t("editor.local_pick") });
  if (typeof path === "string") form.localPath = path;
}

/* ---- 传输方向交换动画 ----
   交换的是"卡片"而不是卡片里的字：方向条上两张端点卡片整体换位——本地
   文件夹那张滑到右边、设备那张滑到左边，角色徽标跟着卡片走（它属于这张
   卡片，不属于这个位置）。

   实现是 FLIP：先记下两张卡片的当前视觉矩形，改完方向后让 Vue 重排 DOM
   （:key 稳定，节点整体搬家），再用 WAAPI 从旧位置补间到新位置。因此动
   的是真卡片，不是叠在上面的幽灵；飞行中再次点击会以"当前视觉位"作为新
   起点（可中断），不会跳回原位。

   两张卡片要穿过彼此，纯水平对撞会糊成一团：向右去的抬 9px、向左去的沉
   9px 错开半个身位，中段再把不透明度压到 45%——读起来是"擦肩而过"。
   reduced-motion 下直接换位，不补间。 */
const swapRot = ref(0);
const cardEls: Partial<Record<"local" | "device", HTMLElement>> = {};
let flights: Animation[] = [];
const reduceMotion =
  typeof window !== "undefined" &&
  !!window.matchMedia &&
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function zoomOf(el: HTMLElement): number {
  return Number(getComputedStyle(el).zoom) || 1;
}

function setCardRef(key: string, el: unknown) {
  const node = el as HTMLElement | null;
  if (node) cardEls[key as "local" | "device"] = node;
  else delete cardEls[key as "local" | "device"];
}

/** 方向条上的两张卡片：顺序随方向翻转，角色随卡片（= 端点）走 */
const dirEnds = computed(() => {
  const push = form.direction === "push";
  const local = {
    key: "local",
    icon: "folder",
    name: t("editor.local"),
    role: t(push ? "editor.side_send" : "editor.side_recv"),
  };
  const device = {
    key: "device",
    icon: "drive",
    name: form.deviceLabel || t("editor.device_hint"),
    role: t(push ? "editor.side_recv" : "editor.side_send"),
  };
  return push ? [local, device] : [device, local];
});

/** 交换发送端 / 接收端（本机 → 设备 或 设备 → 本机） */
function swapDirection() {
  // 起点：当前视觉位置（含上一段补间的中途姿态），保证可中断
  const before = new Map<string, DOMRect>();
  for (const key of ["local", "device"] as const) {
    const el = cardEls[key];
    if (el) before.set(key, el.getBoundingClientRect());
  }
  flights.forEach((a) => a.cancel());
  flights = [];

  form.direction = form.direction === "push" ? "pull" : "push";
  swapRot.value += 180;
  if (reduceMotion) return;

  void nextTick().then(() => {
    for (const [key, from] of before) {
      const el = cardEls[key as "local" | "device"];
      if (!el) continue;
      const to = el.getBoundingClientRect();
      const zoom = zoomOf(el);
      const dx = (from.left - to.left) / zoom;
      const dy = (from.top - to.top) / zoom;
      if (!dx && !dy) continue;
      const vert = (dx < 0 ? -9 : 9) / zoom;
      const anim = el.animate(
        [
          { transform: `translate(${dx}px, ${dy}px)`, opacity: 1, offset: 0 },
          {
            transform: `translate(${dx * 0.5}px, ${dy * 0.5 + vert}px) scale(0.98)`,
            opacity: 0.45,
            offset: 0.5,
          },
          { transform: "translate(0, 0) scale(1)", opacity: 1, offset: 1 },
        ],
        { duration: 380, easing: "cubic-bezier(0.32, 0.72, 0, 1)" },
      );
      anim.onfinish = () => {
        flights = flights.filter((a) => a !== anim);
      };
      flights.push(anim);
    }
  });
}

onBeforeUnmount(() => {
  flights.forEach((a) => a.cancel());
  flights = [];
});

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
          <div
            v-for="end in dirEnds"
            :key="end.key"
            :ref="(el) => setCardRef(end.key, el)"
            class="dir-end"
          >
            <Icon :name="end.icon" :size="16" class="dir-icon" />
            <div class="dir-text">
              <span class="dir-side">{{ end.role }}</span>
              <span class="dir-name">{{ end.name }}</span>
            </div>
          </div>
          <button
            type="button"
            class="dir-swap"
            :title="t('editor.swap')"
            :aria-label="t('editor.swap')"
            @click="swapDirection"
          >
            <svg
              viewBox="0 0 20 20"
              width="16"
              height="16"
              aria-hidden="true"
              :style="{ transform: `rotate(${swapRot}deg)` }"
            >
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
          <button class="btn" :disabled="!deviceOnline" @click="pickRemote">
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

/* 方向条：两张端点卡片 + 悬浮在正中的交换按钮。
   按钮用绝对定位而不是占一格：卡片换位时两节点必须是相邻兄弟，
   中间夹一个按钮会让它们无法整块搬家。46px = 30px 按钮 + 两侧各 8px */
.direction-bar {
  position: relative;
  display: flex;
  align-items: stretch;
  gap: 46px;
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
/* 角色徽标跟着卡片走：它说明的是"这一端在传输里扮演什么角色"，
   而不是"这个位置上是什么" */
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
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
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
}
/* 交换图标随方向同向累计旋转（与卡片换位同曲线同时长） */
.dir-swap svg {
  transition: transform 380ms cubic-bezier(0.32, 0.72, 0, 1);
}
@media (prefers-reduced-motion: reduce) {
  .dir-swap svg {
    transition: none;
  }
}
.dir-swap {
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
  transform: translate(-50%, -50%) scale(0.92);
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
