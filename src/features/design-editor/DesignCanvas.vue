<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { Template, TemplateElement } from "@/shared/api";
import type { PointerAction, ResizeHandle } from "./useDesignPointer";
type ListLayout = Extract<NonNullable<Template["layout"]>, { mode: "list" }>;
const props = defineProps<{
  elements: TemplateElement[];
  list: ListLayout | null;
  widthMm: number;
  heightMm: number;
  scale: number;
  preview: string;
  showGrid: boolean;
  showRulers: boolean;
  showBands: boolean;
  showObjectBounds: boolean;
  selected: number | null;
  selectedElementIds: string[];
  selectedGroups: { id: string; box: { x: number; y: number; w: number; h: number } }[];
  isElementInsideFocus: (id: string) => boolean;
  onDoubleClickElement: (id: string) => void;
  bbox: (element: TemplateElement) => { x: number; y: number; w: number; h: number };
  startDrag: (event: PointerEvent, index: number, kind: PointerAction) => void;
}>();
const resizeHandles: ResizeHandle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
const qrResizeHandles: ResizeHandle[] = ["se"];
const emit = defineEmits<{
  "viewport-size": [value: { width: number; height: number }];
  "clear-selection": [];
  "select-from-canvas": [id: string];
}>();
const holder = ref<HTMLElement | null>(null);
const rulerStep = computed(() => [5, 10, 20, 50, 100, 200, 500, 1000].find((mm) => mm * props.scale >= 48) ?? 2000);
function ticks(length: number) {
  const minor = rulerStep.value / 5;
  return Array.from({ length: Math.floor(length / minor) + 1 }, (_, index) => ({
    mm: index * minor,
    major: index % 5 === 0,
  }));
}
const xTicks = computed(() => ticks(props.widthMm));
const yTicks = computed(() => ticks(props.heightMm));
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
      emit("viewport-size", { width, height: entry.contentRect.height });
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
  <div ref="holder" class="holder" :class="{ 'has-bands': list }">
    <div class="canvas-frame" :style="{ gridTemplateColumns: `${showRulers ? 28 : 0}px ${list ? 54 : 0}px auto`, gridTemplateRows: `${showRulers ? 28 : 0}px auto` }">
      <div v-if="showRulers" class="ruler-corner" aria-hidden="true">mm</div>
      <div v-if="showRulers" class="ruler ruler-x" :style="{ width: `${widthMm * scale}px`, gridColumn: 3 }" aria-hidden="true">
        <div
          v-for="tick in xTicks"
          :key="tick.mm"
          :class="['ruler-tick', { major: tick.major }]"
          :style="{ left: `${tick.mm * scale}px` }"
        >
          <span v-if="tick.major">{{ tick.mm }}</span>
        </div>
      </div>
      <div v-if="showRulers" class="ruler ruler-y" :style="{ height: `${heightMm * scale}px`, gridRow: 2 }" aria-hidden="true">
        <div
          v-for="tick in yTicks"
          :key="tick.mm"
          :class="['ruler-tick', { major: tick.major }]"
          :style="{ top: `${tick.mm * scale}px` }"
        >
          <span v-if="tick.major">{{ tick.mm }}</span>
        </div>
      </div>
      <div v-if="list" class="band-gutter" aria-hidden="true">
        <template v-if="showBands">
          <span class="band-gutter-label" :style="{ top: '2px' }">HEADER</span>
          <span class="band-gutter-label" :style="{ top: `${list.header_mm * scale + 2}px` }">ROW</span>
          <span class="band-gutter-label" :style="{ top: `${(list.header_mm + list.row_mm) * scale + 2}px` }">FOOTER</span>
        </template>
      </div>
      <div
        class="stage"
        tabindex="0"
        :style="{ width: `${widthMm * scale}px`, height: `${heightMm * scale}px`, gridColumn: 3, gridRow: 2 }"
        @pointerdown.self="emit('clear-selection')"
      >
        <img v-if="preview" :src="preview" class="render" alt="" draggable="false" />
        <div v-if="showGrid" class="canvas-grid" :style="{ backgroundSize: `${5 * scale}px ${5 * scale}px` }" aria-hidden="true"></div>
        <template v-if="list && showBands">
          <div class="band-line" :style="{ top: `${list.header_mm * scale}px` }"></div>
          <div class="band-line" :style="{ top: `${(list.header_mm + list.row_mm) * scale}px` }"></div>
        </template>
        <div
          v-for="group in selectedGroups"
          :key="group.id"
          class="group-selection-box"
          :style="{
            left: `${group.box.x * scale}px`, top: `${group.box.y * scale}px`,
            width: `${group.box.w * scale}px`, height: `${group.box.h * scale}px`,
          }"
          aria-hidden="true"
        ></div>
        <template v-for="(el, i) in elements" :key="el.id || i">
          <div
          :class="['box', { on: selected === i || selectedElementIds.includes(el.id), bounds: showObjectBounds, dimmed: !isElementInsideFocus(el.id) }]"
            :style="{
              left: `${bbox(el).x * scale}px`,
              top: `${bbox(el).y * scale}px`,
              width: `${bbox(el).w * scale}px`,
              height: `${bbox(el).h * scale}px`,
            }"
            :title="typeLabel[el.type]"
            @pointerdown="emit('select-from-canvas', el.id); startDrag($event, i, 'move')"
            @dblclick.stop="onDoubleClickElement(el.id)"
          >
            <template v-if="selected === i && el.type !== 'line'">
              <span
                v-for="handle in el.type === 'qr' ? qrResizeHandles : resizeHandles"
                :key="handle"
                :class="['handle', handle]"
                :title="`Resize ${handle}`"
                @pointerdown="startDrag($event, i, handle)"
              ></span>
            </template>
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
  </div>
</template>
