<script setup lang="ts">
// 内联描边图标集：1.7px 圆头线条，跟随 currentColor
const props = withDefaults(defineProps<{ name: string; size?: number }>(), { size: 18 });

const icons: Record<string, string> = {
  overview:
    '<rect x="3.5" y="3.5" width="7" height="7" rx="1.8"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.8"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.8"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.8"/>',
  sync:
    '<path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"/><path d="M21 21v-5h-5"/>',
  drive:
    '<path d="M22 12H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/><line x1="6" y1="16" x2="6.01" y2="16"/><line x1="10" y1="16" x2="10.01" y2="16"/>',
  history:
    '<circle cx="12" cy="12" r="8.5"/><polyline points="12 7.5 12 12 15.2 13.8"/>',
  settings:
    '<line x1="4" y1="7.5" x2="20" y2="7.5"/><circle cx="9.5" cy="7.5" r="2.4"/><line x1="4" y1="16.5" x2="20" y2="16.5"/><circle cx="14.5" cy="16.5" r="2.4"/>',
  plus: '<path d="M12 5v14"/><path d="M5 12h14"/>',
  close: '<path d="M6 6l12 12"/><path d="M18 6 6 18"/>',
  folder:
    '<path d="M3.5 6.8c0-1 .8-1.8 1.8-1.8h3.8c.5 0 1 .2 1.4.6l1.3 1.4h6.9c1 0 1.8.8 1.8 1.8v8.4c0 1-.8 1.8-1.8 1.8H5.3c-1 0-1.8-.8-1.8-1.8V6.8z"/>',
  check: '<path d="M5 12.5 10 17.5 19 7"/>',
  alert:
    '<path d="M12 3.8 2.9 19.4h18.2L12 3.8z"/><path d="M12 10v4.4"/><path d="M12 17.4h.01"/>',
  info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5"/><path d="M12 8h.01"/>',
  trash:
    '<path d="M4 7h16"/><path d="M9.5 7V5.6c0-.9.7-1.6 1.6-1.6h1.8c.9 0 1.6.7 1.6 1.6V7"/><path d="M6.3 7l.8 11.6c.1 1 .9 1.9 2 1.9h5.8c1.1 0 1.9-.9 2-1.9L17.7 7"/><path d="M10 11v6"/><path d="M14 11v6"/>',
  edit:
    '<path d="m14.5 5.5 4 4"/><path d="M4 20l1.2-4.8L16.4 4a2.1 2.1 0 0 1 3 0l.6.6a2.1 2.1 0 0 1 0 3L8.8 18.8 4 20z"/>',
  arrow_r: '<path d="M4.5 12h15"/><path d="m13 5.5 6.5 6.5-6.5 6.5"/>',
  arrow_l: '<path d="M19.5 12h-15"/><path d="m11 5.5-6.5 6.5 6.5 6.5"/>',
  // 窗口按钮：细线、小尺寸，视觉重量低于内容图标
  "win-min": '<path d="M6.5 12h11"/>',
  "win-max": '<rect x="6" y="6" width="12" height="12" rx="1.6"/>',
  "win-restore":
    '<rect x="5.8" y="9" width="9.2" height="9.2" rx="1.5"/><path d="M9 9V6.6A1.6 1.6 0 0 1 10.6 5h6.8A1.6 1.6 0 0 1 19 6.6v6.8a1.6 1.6 0 0 1-1.6 1.6H15"/>',
  "win-close": '<path d="m7 7 10 10"/><path d="M17 7 7 17"/>',
};

const markup = () => icons[props.name] ?? icons.info;
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.7"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    v-html="markup()"
  />
</template>
