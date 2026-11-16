// 跨组件共用的小型展示工具（不依赖组件树，纯函数）。
import type { DiffStatus } from "./types";

/** 标题栏真实高度（tokens.css 的 --titlebar-h；标题栏不在 zoom 容器内，不随界面缩放）。
    Teleport 到 body 的弹层坐标系 = 真实视口 / zoom，边界换算时用它做分子 */
export function titlebarHeight(): number {
  const v = parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue("--titlebar-h"),
  );
  return Number.isFinite(v) ? v : 40;
}

/** 差异状态 → chip 配色类（差异处理对话框与历史记录共用同一套口径） */
export function statusChipClass(status: DiffStatus): string {
  switch (status) {
    case "both_changed":
    case "type_conflict":
      return "chip-red";
    case "changed_local":
    case "changed_remote":
      return "chip-orange";
    case "new_local":
    case "new_remote":
      return "chip-green";
    default:
      return "chip-gray";
  }
}

/* ---- 列宽拖拽 ----
   表头手柄与列边界必须与指针 1:1 同步：界面缩放（.app-shell 的 zoom）
   放大后，指针位移是视觉像素、列宽是未缩放的 CSS 像素，不换算就会
   拖得越快越跑偏。拖拽期间给根元素挂 is-col-resizing，全局换成
   列宽光标并禁止选中，拖到文字上也不会拉出一片蓝。 */

/** 界面缩放系数（tokens.css 的 --ui-scale，作用在 .app-shell 的 zoom 上） */
function uiScale(): number {
  const v = parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue("--ui-scale"),
  );
  return Number.isFinite(v) && v > 0 ? v : 1;
}

export interface ColumnResizeOptions {
  /** 起拖时该列的宽度（CSS 像素，未缩放） */
  width: number;
  min: number;
  max: number;
  /** 宽度变化回调：拖拽过程持续调用，由调用方写入响应式状态 */
  apply: (width: number) => void;
  /** 拖拽结束（含取消）：调用方在此落盘偏好 */
  onEnd?: () => void;
}

const clampWidth = (w: number, min: number, max: number) =>
  Math.round(Math.max(min, Math.min(max, w)));

/** 表头手柄 pointerdown：调用即开始跟手拖拽，直到指针抬起 */
export function startColumnDrag(e: PointerEvent, opts: ColumnResizeOptions): void {
  if (e.pointerType === "mouse" && e.button !== 0) return;
  // 按下动作不冒泡：列表的框选 / 空白处点击关闭都不该被拖列宽触发
  e.stopPropagation();
  const startX = e.clientX;
  const scale = uiScale();
  const root = document.documentElement;
  root.classList.add("is-col-resizing");

  const move = (ev: PointerEvent) => {
    opts.apply(
      clampWidth(opts.width + (ev.clientX - startX) / scale, opts.min, opts.max),
    );
  };
  const stop = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", stop);
    window.removeEventListener("pointercancel", stop);
    root.classList.remove("is-col-resizing");
    opts.onEnd?.();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", stop);
  window.addEventListener("pointercancel", stop);
}

/** 手柄获得键盘焦点后的方向键微调（Shift 加速），让列宽不只能靠鼠标 */
export function nudgeColumn(e: KeyboardEvent, opts: ColumnResizeOptions): void {
  const step = e.shiftKey ? 24 : 8;
  let next = opts.width;
  if (e.key === "ArrowLeft") next -= step;
  else if (e.key === "ArrowRight") next += step;
  else return;
  e.preventDefault();
  const w = clampWidth(next, opts.min, opts.max);
  if (w === opts.width) return;
  opts.apply(w);
  opts.onEnd?.();
}

