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
   一次交换是"两枚角色徽标换了位置"，所以动的是位置而不是文字：旧徽标
   离开原位、沿方向条水平滑到对侧（左右各偏 7px 错开，交叉时不会叠在
   一起），落点处旧字淡出、新字自下方 2px 淡入，两端卡片同时泛一层
   强调色——你能看到"发送端去了右边"这件事本身。

   轨迹只走水平轴：方向条是一条线，斜向飞越会把"左右关系"读成"上下
   关系"。中间略微放大 + 加速再收住，是节制的动量暗示，不做回弹。
   飞行中再次点击：从当前视觉位续走（可中断），不会跳回起点。
   reduced-motion 下只换字，不飞。 */
const sideL = ref<HTMLElement | null>(null);
const sideR = ref<HTMLElement | null>(null);
const swapRot = ref(0);
/** 刚完成换位：驱动"新字落位"与端点提示，动画结束即撤 */
const arriving = ref(false);
let arriveTimer: number | undefined;
let ghosts: { el: HTMLElement; anim: Animation }[] = [];
const reduceMotion =
  typeof window !== "undefined" &&
  !!window.matchMedia &&
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function zoomOf(el: HTMLElement): number {
  return Number(getComputedStyle(el).zoom) || 1;
}

/** 换位提示：与徽标飞行同时起、稍晚收 */
function flashRoles() {
  arriving.value = true;
  window.clearTimeout(arriveTimer);
  arriveTimer = window.setTimeout(() => (arriving.value = false), 460);
}

/** 交换发送端 / 接收端（本机 → 设备 或 设备 → 本机） */
function swapDirection() {
  const chipL = sideL.value;
  const chipR = sideR.value;
  // 起点：进行中的幽灵从当前视觉位续走（可中断），否则用真实徽标位置
  const startL = ghosts.at(0)?.el.getBoundingClientRect() ?? chipL?.getBoundingClientRect();
  const startR = ghosts.at(1)?.el.getBoundingClientRect() ?? chipR?.getBoundingClientRect();
  ghosts.forEach((g) => {
    g.anim.cancel();
    g.el.remove();
  });
  ghosts = [];
  const oldL = chipL?.textContent ?? "";
  const oldR = chipR?.textContent ?? "";

  form.direction = form.direction === "push" ? "pull" : "push";
  swapRot.value += 180;
  flashRoles();
  if (!chipL || !chipR || !startL || !startR || reduceMotion) return;

  void nextTick().then(() => {
    const endL = chipL.getBoundingClientRect();
    const endR = chipR.getBoundingClientRect();
    const zL = zoomOf(chipL);

    // 徽标在 zoom 容器内、幽灵挂在 body 上：给幽灵补上同一档 zoom，
    // 坐标系一致（否则改了"界面大小"之后落点会整体偏移）
    const mkGhost = (label: string, from: DOMRect, to: DOMRect, vert: number) => {
      const g = chipL.cloneNode(true) as HTMLElement;
      g.textContent = label;
      g.style.position = "fixed";
      g.style.left = `${from.left / zL}px`;
      g.style.top = `${from.top / zL}px`;
      g.style.width = `${from.width / zL}px`;
      g.style.margin = "0";
      g.style.zoom = String(zL);
      g.style.transformOrigin = "top left";
      g.style.pointerEvents = "none";
      g.style.zIndex = "130"; // 弹出层(120)之上、标题栏(150)之下
      document.body.appendChild(g);
      const dx = (to.left - from.left) / zL;
      const dy = (to.top - from.top) / zL;
      const anim = g.animate(
        [
          { transform: "translate(0, 0) scale(1)", opacity: 1, offset: 0 },
          {
            transform: `translate(${dx * 0.5}px, ${dy * 0.5 + vert / zL}px) scale(1.04)`,
            opacity: 1,
            offset: 0.52,
          },
          { transform: `translate(${dx}px, ${dy}px) scale(1)`, opacity: 1, offset: 0.86 },
          { transform: `translate(${dx}px, ${dy}px) scale(1)`, opacity: 0, offset: 1 },
        ],
        { duration: 320, easing: "cubic-bezier(0.32, 0.72, 0, 1)", fill: "forwards" },
      );
      anim.onfinish = () => g.remove();
      return { el: g, anim };
    };

    // 左右各偏 7px：两条轨迹在中点擦肩而过，不重叠
    ghosts = [mkGhost(oldL, startL, endR, -7), mkGhost(oldR, startR, endL, 7)];
  });
}

onBeforeUnmount(() => {
  ghosts.forEach((g) => {
    g.anim.cancel();
    g.el.remove();
  });
  ghosts = [];
  window.clearTimeout(arriveTimer);
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
          <div class="dir-end" :class="{ 'role-flip': arriving }">
            <Icon name="folder" :size="16" class="dir-icon" />
            <div class="dir-text">
              <span ref="sideL" class="dir-side" :class="{ arriving }">
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
          <div class="dir-end" :class="{ 'role-flip': arriving }">
            <Icon name="drive" :size="16" class="dir-icon" />
            <div class="dir-text">
              <span ref="sideR" class="dir-side" :class="{ arriving }">
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
/* 新角色落位：与幽灵抵达同一刻浮上来，不是瞬间换字 */
.dir-side.arriving {
  animation: dir-side-in 220ms var(--ease) 110ms both;
}
@keyframes dir-side-in {
  from {
    opacity: 0;
    transform: translateY(3px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
/* 端点提示：角色换位的那一侧泛一层强调色，随飞行结束退去 */
.dir-end.role-flip {
  animation: dir-flip 380ms var(--ease) both;
}
@keyframes dir-flip {
  from {
    background-color: var(--accent-soft);
    border-color: color-mix(in srgb, var(--accent) 34%, transparent);
  }
  to {
    background-color: var(--fill-softer);
    border-color: var(--hairline);
  }
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
}
/* 交换图标随方向同向累计旋转（与徽标飞行同曲线同时长） */
.dir-swap svg {
  transition: transform 320ms cubic-bezier(0.32, 0.72, 0, 1);
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
