<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import type { Template, TemplateElement } from "@/shared/api";
type ListLayout = Extract<NonNullable<Template["layout"]>, { mode: "list" }>;
defineProps<{
  elements: TemplateElement[];
  list: ListLayout | null;
  widthMm: number;
  heightMm: number;
  scale: number;
  preview: string;
  selected: number | null;
  bbox: (element: TemplateElement) => { x: number; y: number; w: number; h: number };
  startDrag: (event: PointerEvent, index: number, kind: "move" | "resize" | "end1" | "end2") => void;
}>();
const emit = defineEmits<{ width: [value: number]; "clear-selection": [] }>();
const holder = ref<HTMLElement | null>(null);
const typeLabel: Record<TemplateElement["type"], string> = {
  text: "Text",
  line: "Line",
  rect: "Rectangle",
  barcode: "Barcode",
  qr: "QR",
  image: "Image",
};
let observer: ResizeObserver | undefined;
let resizeFrame: number | undefined;
onMounted(() => {
  observer = new ResizeObserver(([entry]) => {
    const width = entry.contentRect.width;
    if (resizeFrame !== undefined) cancelAnimationFrame(resizeFrame);
    resizeFrame = requestAnimationFrame(() => {
      resizeFrame = undefined;
      emit("width", width);
    });
  });
  if (holder.value) observer.observe(holder.value);
});
onBeforeUnmount(() => {
  observer?.disconnect();
  if (resizeFrame !== undefined) cancelAnimationFrame(resizeFrame);
});
</script>
<template>
  <div ref="holder" class="holder">
    <div
      class="stage"
      tabindex="0"
      :style="{ width: `${widthMm * scale}px`, height: `${heightMm * scale}px` }"
      @pointerdown.self="emit('clear-selection')"
    >
      <img v-if="preview" :src="preview" class="render" alt="" draggable="false" />
      <template v-if="list">
        <div class="band-label" :style="{ top: '0px' }">HEADER</div>
        <div class="band-line" :style="{ top: `${list.header_mm * scale}px` }"></div>
        <div class="band-label" :style="{ top: `${list.header_mm * scale}px` }">ROW (repeated)</div>
        <div class="band-line" :style="{ top: `${(list.header_mm + list.row_mm) * scale}px` }"></div>
        <div class="band-label" :style="{ top: `${(list.header_mm + list.row_mm) * scale}px` }">FOOTER</div>
      </template>
      <template v-for="(el, i) in elements" :key="el.id || i">
        <div
          :class="['box', { on: selected === i }]"
          :style="{
            left: `${bbox(el).x * scale}px`,
            top: `${bbox(el).y * scale}px`,
            width: `${bbox(el).w * scale}px`,
            height: `${bbox(el).h * scale}px`,
          }"
          :title="typeLabel[el.type]"
          @pointerdown="startDrag($event, i, 'move')"
        >
          <span
            v-if="selected === i && el.type !== 'line'"
            class="handle br"
            @pointerdown="startDrag($event, i, 'resize')"
          ></span>
        </div>
        <template v-if="selected === i && el.type === 'line'">
          <span
            class="handle end"
            :style="{ left: `${el.x1 * scale}px`, top: `${el.y1 * scale}px` }"
            @pointerdown="startDrag($event, i, 'end1')"
          ></span>
          <span
            class="handle end"
            :style="{ left: `${el.x2 * scale}px`, top: `${el.y2 * scale}px` }"
            @pointerdown="startDrag($event, i, 'end2')"
          ></span>
        </template>
      </template>
    </div>
  </div>
</template>
