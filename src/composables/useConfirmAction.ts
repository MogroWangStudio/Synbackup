import { onBeforeUnmount, ref } from "vue";

/**
 * 两步确认操作（删除任务、清空历史等破坏性动作共用）：
 * 第一次点击进入确认态，3 秒内再次点击执行；超时或组件
 * 卸载自动解除确认态。按钮文案据 confirming 切换。
 */
export function useConfirmAction(action: () => void) {
  const confirming = ref(false);
  let timer = 0;

  function confirm() {
    if (!confirming.value) {
      confirming.value = true;
      timer = window.setTimeout(() => (confirming.value = false), 3000);
      return;
    }
    window.clearTimeout(timer);
    confirming.value = false;
    action();
  }

  onBeforeUnmount(() => window.clearTimeout(timer));
  return { confirming, confirm };
}
