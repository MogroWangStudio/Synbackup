<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import Icon from "./Icon.vue";

const props = defineProps<{ title: string; wide?: boolean; busy?: boolean }>();
const emit = defineEmits<{ (e: "close"): void }>();

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape" && !props.busy) emit("close");
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <Teleport to="body">
    <Transition name="modal">
      <div class="modal-overlay" @click.self="!busy && emit('close')">
        <div class="modal-panel" :class="{ wide }" role="dialog" aria-modal="true">
          <header class="modal-head">
            <h2>{{ title }}</h2>
            <button class="icon-btn" :disabled="busy" :aria-label="'关闭'" @click="emit('close')">
              <Icon name="close" :size="15" />
            </button>
          </header>
          <div class="modal-body">
            <slot />
          </div>
          <footer v-if="$slots.footer" class="modal-foot">
            <slot name="footer" />
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
