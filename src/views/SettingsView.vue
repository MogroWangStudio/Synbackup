<script setup lang="ts">
import { computed, ref } from "vue";
import { api } from "../api";
import { refreshLogoImage, store, toast } from "../store";
import { open } from "@tauri-apps/plugin-dialog";
import { applyUiPrefs, DEFAULT_ACCENT, persistUiPrefs, replayBootAnim, UI_THEMES, isValidHex } from "../theme";
import { titlebarHeight } from "../ui";
import { LOCALES, t } from "../i18n";
import type { AppSettings, UpdateInfo } from "../types";
import Icon from "../components/Icon.vue";
import Logo from "../components/Logo.vue";
import Toggle from "../components/Toggle.vue";
import Stepper from "../components/Stepper.vue";
import Select from "../components/Select.vue";
import ColorPicker from "../components/ColorPicker.vue";
import NotifyTestDialog from "../components/NotifyTestDialog.vue";
import FontBrowser from "../components/FontBrowser.vue";
import type { SelectOption } from "../components/Select.vue";

/** 强调色预设：key 与 tokens.css 的 data-accent 对应，color 用于色块展示 */
const accents = [
  { key: "brand", color: "#9b62cf" },
  { key: "blue", color: "#4a7dd8" },
  { key: "green", color: "#3e9b6f" },
  { key: "orange", color: "#d97b3e" },
  { key: "pink", color: "#d05b8c" },
] as const;

const appearances = ["follow", "light", "dark"] as const;
const fonts = ["system", "serif", "rounded"] as const;

// 语言清单来自 i18n 注册表（LOCALES），标签一律母语自称
const languageOptions: SelectOption[] = LOCALES.map((l) => ({
  value: l.code,
  label: l.label,
}));
// 空白处点击关闭行为（顺序即用户口中的双击 / 单击 / 从不）
const blankClickOptions = computed<SelectOption[]>(() =>
  ["double", "single", "never"].map((k) => ({ value: k, label: t(`blankclick.${k}`) })),
);
// 选项标签经 t() 求值，必须包成 computed 跟随语言重算——
// setup 期一次性求值会在切换语言后让下拉标签停留在旧语言
const channelOptions = computed<SelectOption[]>(() => [
  { value: "stable", label: t("channel.stable") },
  { value: "beta", label: t("channel.beta") },
]);
const appearanceOptions = computed<SelectOption[]>(() =>
  appearances.map((a) => ({ value: a, label: t(`appearance.${a}`) })),
);
// 特殊主题选项：键集来自 theme.ts 的注册表（与 tokens.css 的 data-theme 块对应）
const themeOptions = computed<SelectOption[]>(() =>
  UI_THEMES.map((k) => ({ value: k, label: t(`theme.${k}`) })),
);
// 任务卡片按钮显示模式
const taskBtnOptions = computed<SelectOption[]>(() =>
  ["both", "icon", "text"].map((k) => ({ value: k, label: t(`taskbtn.${k}`) })),
);
// 品牌 logo 形态：默认（完整字标）/ 仅图案 / 自定义文字 / 自定义图片。
// "仅文字"与"默认完整"合并为一档——默认即完整字标（含文字），
// 少一个看起来差不多、实际只裁掉图案的选项。
const logoStyleOptions = computed<SelectOption[]>(() =>
  ["full", "mark", "custom_text", "custom_image"].map((k) => ({
    value: k,
    label: t(`logostyle.${k}`),
  })),
);
const logoFileName = computed(() => {
  const p = store.settings.logoImagePath;
  return p ? p.split(/[\\/]/).pop() ?? "" : "";
});

function onLogoTextInput(e: Event) {
  // 输入过程实时预览（标题栏经 store 响应式刷新）；落盘在 change 时一次
  store.settings = {
    ...store.settings,
    logoCustomText: (e.target as HTMLInputElement).value,
  };
}
function onLogoTextCommit(e: Event) {
  void update({ logoCustomText: (e.target as HTMLInputElement).value });
}
async function pickLogoImage() {
  const path = await open({
    multiple: false,
    title: t("settings.logo_pick"),
    filters: [{ name: t("settings.logo_pick"), extensions: ["png", "jpg", "jpeg", "webp", "gif", "svg"] }],
  });
  if (typeof path !== "string") return;
  await update({ logoImagePath: path });
  await refreshLogoImage();
}
// 界面字体：预设选项 + 自定义字族动态项（自定义字族由字体浏览弹窗选择）
const fontOptions = computed<SelectOption[]>(() => {
  const base: SelectOption[] = fonts.map((f) => ({ value: f, label: t(`font.${f}`) }));
  const cur = store.settings.fontFamily;
  if (cur && !fonts.includes(cur as (typeof fonts)[number])) {
    base.push({ value: cur, label: `${t("font.custom")} · ${cur}` });
  } else {
    base.push({ value: "custom", label: t("font.custom") });
  }
  return base;
});
const fontBrowserOpen = ref(false);
const fontHint = computed(() => {
  const cur = store.settings.fontFamily;
  if (cur && !fonts.includes(cur as (typeof fonts)[number])) {
    return t("settings.font_custom_hint", { family: cur });
  }
  return t("settings.font_hint");
});
function onFontSelect(v: string) {
  if (v === "custom") {
    fontBrowserOpen.value = true;
    return;
  }
  void update({ fontFamily: v });
}
function onFontBrowserUse(family: string) {
  void update({ fontFamily: family });
  fontBrowserOpen.value = false;
}
/** Select 的显示值：自定义字族时即字族名本身（与动态选项 value 对齐） */
const fontSelectValue = computed(() => {
  const cur = store.settings.fontFamily;
  return cur && !fonts.includes(cur as (typeof fonts)[number]) ? cur : cur;
});

// 启动动画预设与曲线选项
const animPresetOptions = computed<SelectOption[]>(() =>
  ["fade", "rise", "zoom", "slide", "blur", "pop"].map((p) => ({
    value: p,
    label: t(`anim.${p}`),
  })),
);
const animCurveOptions = computed<SelectOption[]>(() =>
  ["smooth", "spring", "linear", "swift"].map((c) => ({ value: c, label: t(`curve.${c}`) })),
);

// ---- 自定义主题色 ----
const pickerOpen = ref(false);
const customHex = ref(store.settings.accentCustom || DEFAULT_ACCENT);
const customSwatchEl = ref<HTMLElement | null>(null);
const pickerPos = ref<{ left: string; top: string } | null>(null);
// 调色板拖拽 / hex 输入期间的 saveSettings 防抖：此前每个 pointermove
// 发一次 IPC、后端高频重写 config.json——拖拽掉帧且偶发连环报错 toast
let saveTimer: number | undefined;

function togglePicker() {
  if (pickerOpen.value) {
    pickerOpen.value = false;
    return;
  }
  const el = customSwatchEl.value;
  if (el) {
    // 色块在 zoom（--ui-scale）容器内：getBoundingClientRect 是缩放后的
    // 视口坐标，除以 zoom 换算回未缩放坐标系，Teleport 到 body 的弹层
    // 才能对准（弹层自身不在 zoom 容器内）。
    const zoom = Number(getComputedStyle(el).zoom) || 1;
    const r = el.getBoundingClientRect();
    const PICKER_W = 240 + 16; // cpop 宽 + 边距余量
    const PICKER_H = 240;
    // 先按缩放系换算，再做视口钳制；顶部让出标题栏（与 ComboBox 弹层同一规则）
    let left = r.left / zoom;
    const top = below(r) ? r.bottom / zoom + 8 : r.top / zoom - PICKER_H - 8;
    left = Math.min(left, window.innerWidth / zoom - PICKER_W);
    left = Math.max(8, left);
    pickerPos.value = {
      left: `${Math.round(left)}px`,
      top: `${Math.round(Math.max(titlebarHeight() / zoom + 8, top))}px`,
    };
  }
  customHex.value = isValidHex(store.settings.accentCustom)
    ? store.settings.accentCustom
    : DEFAULT_ACCENT;
  pickerOpen.value = true;
}

/** 色块下方空间是否放得下调色板（zoom 无关，都用缩放后的视口坐标判断） */
function below(r: DOMRect): boolean {
  return window.innerHeight - r.bottom > 260;
}

function onCustomHex(v: string) {
  if (!isValidHex(v)) return;
  customHex.value = v;
  // 拖拽 / 输入期间只应用界面与本地偏好（1:1 反馈不落盘），
  // saveSettings 防抖到停止后写一次
  const next = { ...store.settings, accent: "custom", accentCustom: v };
  store.settings = next;
  applyUiPrefs(next);
  persistUiPrefs(next);
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    saveTimer = undefined;
    void api.saveSettings(store.settings).catch((e) => toast("error", String(e)));
  }, 300);
}

function onCustomClose() {
  pickerOpen.value = false;
  // 若关闭时用户改动过颜色但没有点确定，保留最近有效值（update 已实时应用）
  void update({ accent: "custom", accentCustom: customHex.value });
}

function openStudioSite() {
  void api.openExternal("https://www.mogrowangstudio.top").catch((e) => toast("error", String(e)));
}

function openUpdatePage() {
  if (pendingUrl) void api.openExternal(pendingUrl).catch((e) => toast("error", String(e)));
}

/** 测试通知窗口：标题 / 内容 / 按钮均可自定义后发送 */
const notifyTestOpen = ref(false);

// ---- 检查更新 ----
const checking = ref(false);
let pendingUrl = "";
const updateHint = ref("");

async function onCheckUpdate() {
  if (checking.value) return;
  checking.value = true;
  try {
    const info: UpdateInfo = await api.checkUpdate();
    if (info.available) {
      updateHint.value = t("update.available", { v: info.version });
      toast("success", t("update.available_toast", { v: info.version }));
      // 更新说明写入提示行；点击版本行可打开下载页
      pendingUrl = info.url;
    } else {
      updateHint.value = t("update.up_to_date");
      toast("info", t("update.up_to_date_toast"));
    }
  } catch (e) {
    updateHint.value = "";
    toast("error", String(e));
  } finally {
    checking.value = false;
  }
}

async function update(patch: Partial<AppSettings>) {
  // 显式保存取代挂起的防抖保存，避免重复写盘
  window.clearTimeout(saveTimer);
  saveTimer = undefined;
  const next = { ...store.settings, ...patch };
  store.settings = next;
  // 外观 / 字体 / 缩放先落 DOM 再存后端，切换即时可见
  applyUiPrefs(next);
  persistUiPrefs(next);
  try {
    await api.saveSettings(next);
  } catch (e) {
    toast("error", String(e));
    return;
  }
  if (patch.language) {
    // 语言已由 applyUiPrefs 同步到根元素；托盘文案要重启才生效，提示用户
    toast("info", t("settings.tray_hint"));
  }
}

function onLanguageSelect(v: string) {
  void update({ language: v });
}
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t("settings.title") }}</h1>
      </div>
    </div>

    <section class="card group">
      <h3 class="group-title">{{ t("settings.general") }}</h3>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.language") }}</span>
        </div>
        <Select
          :model-value="store.settings.language"
          :options="languageOptions"
          :aria-label="t('settings.language')"
          @update:model-value="onLanguageSelect"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.launch") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.launchAtLogin"
          @update:model-value="(v) => update({ launchAtLogin: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.blank_click") }}</span>
          <span class="row-hint">{{ t("settings.blank_click_hint") }}</span>
        </div>
        <Select
          :model-value="store.settings.blankClick"
          :options="blankClickOptions"
          :aria-label="t('settings.blank_click')"
          @update:model-value="(v) => update({ blankClick: v })"
        />
      </div>
    </section>

    <section class="card group">
      <h3 class="group-title">{{ t("settings.personalize") }}</h3>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.accent") }}</span>
        </div>
        <div class="swatches" role="radiogroup" :aria-label="t('settings.accent')">
          <button
            v-for="sw in accents"
            :key="sw.key"
            type="button"
            class="swatch"
            :class="{ active: store.settings.accent === sw.key }"
            :style="{ '--swatch': sw.color }"
            :aria-checked="store.settings.accent === sw.key"
            role="radio"
            :title="t(`accent.${sw.key}`)"
            :aria-label="t(`accent.${sw.key}`)"
            @click="update({ accent: sw.key })"
          />
          <!-- 自定义：多色环形渐变色块，点击弹出自绘调色板 -->
          <button
            ref="customSwatchEl"
            type="button"
            class="swatch custom"
            :class="{ active: store.settings.accent === 'custom' }"
            :aria-checked="store.settings.accent === 'custom'"
            role="radio"
            :title="t('accent.custom')"
            :aria-label="t('accent.custom')"
            @click="togglePicker"
          />
        </div>
        <Teleport to="body">
          <Transition name="picker-pop">
            <div
              v-if="pickerOpen"
              class="picker-anchor"
              :style="pickerPos"
            >
              <ColorPicker
                :model-value="customHex"
                :aria-label="t('accent.custom')"
                @update:model-value="onCustomHex"
                @close="onCustomClose"
              />
            </div>
          </Transition>
        </Teleport>
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.appearance") }}</span>
        </div>
        <Select
          :model-value="store.settings.appearance"
          :options="appearanceOptions"
          :aria-label="t('settings.appearance')"
          @update:model-value="(v) => update({ appearance: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.theme_label") }}</span>
          <span class="row-hint">{{ t("settings.theme_hint") }}</span>
        </div>
        <Select
          :model-value="store.settings.theme"
          :options="themeOptions"
          :aria-label="t('settings.theme_label')"
          @update:model-value="(v) => update({ theme: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.font") }}</span>
          <span class="row-hint">{{ fontHint }}</span>
        </div>
        <div class="font-controls">
          <Select
            :model-value="fontSelectValue"
            :options="fontOptions"
            :aria-label="t('settings.font')"
            @update:model-value="onFontSelect"
          />
          <button type="button" class="btn btn-sm" @click="fontBrowserOpen = true">
            {{ t("settings.font_browse") }}
          </button>
        </div>
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.font_size") }}</span>
        </div>
        <Stepper
          :model-value="store.settings.fontScale"
          :step="0.05"
          :min="0.9"
          :max="1.15"
          :format="(v) => `${Math.round(v * 100)}%`"
          :decrease-label="t('settings.size_decrease')"
          :increase-label="t('settings.size_increase')"
          @update:model-value="(v) => update({ fontScale: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.ui_size") }}</span>
        </div>
        <Stepper
          :model-value="store.settings.uiScale"
          :step="0.05"
          :min="0.85"
          :max="1.25"
          :format="(v) => `${Math.round(v * 100)}%`"
          :decrease-label="t('settings.size_decrease')"
          :increase-label="t('settings.size_increase')"
          @update:model-value="(v) => update({ uiScale: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.launch_anim") }}</span>
          <span class="row-hint">{{ t("settings.launch_anim_hint") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.launchAnimation"
          @update:model-value="(v) => update({ launchAnimation: v })"
        />
      </div>
      <template v-if="store.settings.launchAnimation">
        <div class="row">
          <div class="row-label">
            <span class="row-name">{{ t("settings.launch_anim_preset") }}</span>
          </div>
          <Select
            :model-value="store.settings.launchAnimPreset"
            :options="animPresetOptions"
            :aria-label="t('settings.launch_anim_preset')"
            @update:model-value="(v) => update({ launchAnimPreset: v })"
          />
        </div>
        <div class="row">
          <div class="row-label">
            <span class="row-name">{{ t("settings.launch_anim_duration") }}</span>
          </div>
          <Stepper
            :model-value="store.settings.launchAnimMs"
            :step="100"
            :min="200"
            :max="1200"
            :format="(v) => `${v} ms`"
            :decrease-label="t('settings.size_decrease')"
            :increase-label="t('settings.size_increase')"
            @update:model-value="(v) => update({ launchAnimMs: v })"
          />
        </div>
        <div class="row">
          <div class="row-label">
            <span class="row-name">{{ t("settings.launch_anim_curve") }}</span>
          </div>
          <Select
            :model-value="store.settings.launchAnimCurve"
            :options="animCurveOptions"
            :aria-label="t('settings.launch_anim_curve')"
            @update:model-value="(v) => update({ launchAnimCurve: v })"
          />
        </div>
        <div class="row">
          <div class="row-label">
            <span class="row-name">{{ t("settings.launch_anim_preview") }}</span>
            <span class="row-hint">{{ t("settings.launch_anim_preview_hint") }}</span>
          </div>
          <button type="button" class="btn btn-sm" @click="replayBootAnim(store.settings)">
            {{ t("settings.launch_anim_preview_btn") }}
          </button>
        </div>
      </template>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.task_btn") }}</span>
          <span class="row-hint">{{ t("settings.task_btn_hint") }}</span>
        </div>
        <Select
          :model-value="store.settings.taskButtonStyle"
          :options="taskBtnOptions"
          :aria-label="t('settings.task_btn')"
          @update:model-value="(v) => update({ taskButtonStyle: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.logo_label") }}</span>
          <span class="row-hint">{{ t("settings.logo_hint") }}</span>
        </div>
        <Select
          :model-value="store.settings.logoStyle"
          :options="logoStyleOptions"
          :aria-label="t('settings.logo_label')"
          @update:model-value="(v) => update({ logoStyle: v })"
        />
      </div>
      <div v-if="store.settings.logoStyle === 'custom_text'" class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.logo_text_label") }}</span>
        </div>
        <input
          class="input"
          :value="store.settings.logoCustomText"
          maxlength="24"
          :placeholder="t('settings.logo_text_placeholder')"
          spellcheck="false"
          @input="onLogoTextInput"
          @change="onLogoTextCommit"
        />
      </div>
      <div v-else-if="store.settings.logoStyle === 'custom_image'" class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.logo_image_label") }}</span>
          <span class="row-hint">{{ logoFileName || t("settings.logo_image_none") }}</span>
        </div>
        <button type="button" class="btn btn-sm" @click="pickLogoImage">
          {{ t("settings.logo_pick") }}
        </button>
      </div>
      <FontBrowser
        v-if="fontBrowserOpen"
        :current="store.settings.fontFamily"
        @use="onFontBrowserUse"
        @close="fontBrowserOpen = false"
      />
    </section>

    <section class="card group">
      <h3 class="group-title">{{ t("settings.status_bar") }}</h3>
      <p class="group-hint">{{ t("settings.status_bar_hint") }}</p>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.status_devices") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.statusDevices"
          @update:model-value="(v) => update({ statusDevices: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.status_pairs") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.statusPairs"
          @update:model-value="(v) => update({ statusPairs: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.status_sync") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.statusSyncCount"
          @update:model-value="(v) => update({ statusSyncCount: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.status_version") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.statusVersion"
          @update:model-value="(v) => update({ statusVersion: v })"
        />
      </div>
    </section>

    <section class="card group">
      <h3 class="group-title">{{ t("settings.sync_section") }}</h3>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.auto_scan") }}</span>
          <span class="row-hint">{{ t("settings.auto_scan_hint") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.autoScanOnInsert"
          @update:model-value="(v) => update({ autoScanOnInsert: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.ask_before_sync") }}</span>
          <span class="row-hint">{{ t("settings.ask_before_sync_hint") }}</span>
        </div>
        <Toggle
          :model-value="store.settings.askBeforeSync"
          @update:model-value="(v) => update({ askBeforeSync: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.auto_sync") }}</span>
          <span class="row-hint">{{
            store.settings.askBeforeSync
              ? t("settings.auto_sync_hint_disabled")
              : t("settings.auto_sync_hint")
          }}</span>
        </div>
        <Toggle
          :model-value="store.settings.autoSyncNoConflict"
          :disabled="store.settings.askBeforeSync"
          @update:model-value="(v) => update({ autoSyncNoConflict: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.notify") }}</span>
          <span class="row-hint">{{ t("settings.notify_hint") }}</span>
        </div>
        <div class="notify-controls">
          <button
            type="button"
            class="btn btn-sm"
            :title="t('settings.notify_test_title')"
            @click="notifyTestOpen = true"
          >
            {{ t("settings.notify_test") }}
          </button>
          <Toggle
            :model-value="store.settings.notifyOnInsert"
            @update:model-value="(v) => update({ notifyOnInsert: v })"
          />
        </div>
      </div>
    </section>

    <section class="card group">
      <h3 class="group-title">{{ t("settings.about") }}</h3>
      <div class="brand-mark">
        <Logo
          :size="32"
          :variant="store.settings.logoStyle === 'mark' ? 'mark' : 'full'"
          :custom-text="store.settings.logoCustomText"
          :custom-image="store.logoImageUri"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.version") }}</span>
          <span class="row-hint" :class="{ clickable: !!pendingUrl }" @click="openUpdatePage">
            {{ updateHint }}
          </span>
        </div>
        <span class="row-value">{{ store.appInfo?.version ?? "0.4.0-beta3" }}</span>
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.update_channel") }}</span>
          <span class="row-hint">{{ t("settings.update_channel_hint") }}</span>
        </div>
        <Select
          :model-value="store.settings.updateChannel"
          :options="channelOptions"
          :aria-label="t('settings.update_channel')"
          @update:model-value="(v) => update({ updateChannel: v })"
        />
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.check_update") }}</span>
        </div>
        <button class="btn btn-sm" :disabled="checking" @click="onCheckUpdate">
          {{ checking ? t("update.checking") : t("update.check") }}
        </button>
      </div>
      <div class="row">
        <div class="row-label">
          <span class="row-name">{{ t("settings.data") }}</span>
          <span class="row-hint">{{ t("settings.data_hint") }}</span>
        </div>
        <span class="row-value mono" :title="store.appInfo?.dataDir">
          <Icon name="folder" :size="13" />
          {{ store.appInfo?.dataDir }}
        </span>
      </div>
      <div class="dev-section">
        <h3 class="group-title">{{ t("settings.developer") }}</h3>
        <a
          href="https://www.mogrowangstudio.top"
          class="studio-link"
          @click.prevent="openStudioSite"
        >
          <img src="/studio-logo.svg" alt="MogroWang Studio" class="studio-logo" draggable="false" />
          <span class="studio-hint">{{ t("settings.studio_hint") }}</span>
        </a>
        <p class="dev-copyright">© 2026 MogroWang Studio</p>
      </div>
    </section>

    <NotifyTestDialog v-if="notifyTestOpen" @close="notifyTestOpen = false" />
  </div>
</template>

<style scoped>
/* 强调色色块：单色圆点，选中态用同色外环标记 */
.swatches {
  display: flex;
  gap: 8px;
}
.swatch {
  width: 22px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: var(--swatch);
  cursor: pointer;
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.08);
  transition:
    transform 120ms var(--ease),
    box-shadow 120ms var(--ease);
}
.swatch:hover {
  transform: scale(1.1);
}
.swatch:active {
  transform: scale(0.96);
}
.swatch.active {
  box-shadow:
    inset 0 0 0 1px rgba(0, 0, 0, 0.08),
    0 0 0 2px var(--bg-content),
    0 0 0 4px var(--swatch);
}
.swatch:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
/* 自定义色块：多色环形渐变，与单色预设区分 */
.swatch.custom {
  background: conic-gradient(#f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00);
}
.swatch.custom.active {
  box-shadow:
    inset 0 0 0 1px rgba(0, 0, 0, 0.08),
    0 0 0 2px var(--bg-content),
    0 0 0 4px var(--accent);
}
/* 调色板锚点：fixed 定位（坐标由 JS 依据色块计算），层级与 ComboBox 弹层同档 */
.picker-anchor {
  position: fixed;
  z-index: var(--z-popup);
  transform: translateY(0);
}
.picker-anchor > * {
  position: static;
}
/* 调色板出入：自色块方向浮出，与弹窗家族同一语言，快而克制 */
.picker-pop-enter-active {
  transition:
    opacity 140ms var(--ease),
    transform 160ms var(--ease);
}
.picker-pop-leave-active {
  transition:
    opacity 110ms var(--ease),
    transform 120ms var(--ease);
}
.picker-pop-enter-from,
.picker-pop-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.98);
}
@media (prefers-reduced-motion: reduce) {
  .picker-pop-enter-active,
  .picker-pop-leave-active {
    transition: opacity 120ms ease;
  }
  .picker-pop-enter-from,
  .picker-pop-leave-to {
    transform: none;
  }
}

.group {
  padding: 4px 16px;
  margin-bottom: 14px;
}
.group-title {
  font-size: calc(12px * var(--font-scale, 1));
  font-weight: 600;
  color: var(--text-2);
  padding: 10px 0 4px;
}
/* 板块级说明：紧跟板块标题之下、首行设置项之上 */
.group-hint {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
  padding: 0 0 4px;
}
/* 系统通知行的测试按钮 + 开关组合 */
.notify-controls {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: none;
}
.brand-mark {
  display: flex;
  justify-content: center;
  padding: 16px 0;
  border-bottom: 1px solid var(--hairline);
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  min-height: 44px;
  padding: 7px 0;
  border-bottom: 1px solid var(--hairline);
}
.group .row:last-child {
  border-bottom: none;
}
.row-label {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.row-name {
  font-size: calc(13px * var(--font-scale, 1));
  font-weight: 500;
}
.row-hint {
  font-size: calc(11.5px * var(--font-scale, 1));
  color: var(--text-3);
}
.row-value {
  font-size: calc(12.5px * var(--font-scale, 1));
  color: var(--text-2);
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 55%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-value.mono {
  font-family: var(--mono);
  font-size: calc(11px * var(--font-scale, 1));
}

/* ---- 开发者 ---- */
.dev-section {
  padding: 8px 0 2px;
}
/* 悬停时字标模糊变淡、浮现引导文字；点击经系统浏览器打开工作室网站 */
.studio-link {
  position: relative;
  display: block;
  width: fit-content;
  margin: 14px auto 0;
  cursor: pointer;
}
.studio-logo {
  display: block;
  height: 24px;
  max-width: 70vw;
  user-select: none;
  transition:
    filter 200ms var(--ease),
    opacity 200ms var(--ease);
}
.studio-link:hover .studio-logo,
.studio-link:focus-visible .studio-logo {
  opacity: 0.7;
  filter: blur(3px);
}
.studio-hint {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: calc(12px * var(--font-scale, 1));
  font-weight: 500;
  color: var(--text);
  white-space: nowrap;
  opacity: 0;
  transition: opacity 200ms var(--ease);
  pointer-events: none;
}
.studio-link:hover .studio-hint,
.studio-link:focus-visible .studio-hint {
  opacity: 1;
}
.studio-link:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 4px;
  border-radius: 4px;
}
.dev-copyright {
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--hairline);
  text-align: center;
  font-size: calc(12px * var(--font-scale, 1));
  color: var(--text-3);
}
</style>
