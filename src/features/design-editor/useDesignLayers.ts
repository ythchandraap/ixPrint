import { computed, ref, type ComputedRef } from "vue";
import type { Template, TemplateElement } from "@/shared/api";

type ListLayout = Extract<NonNullable<Template["layout"]>, { mode: "list" }>;

const snap = (value: number) => Math.round(value / 0.5) * 0.5;
const newId = () => Math.random().toString(36).slice(2, 10);

export function useDesignLayers(
  template: Template,
  list: ComputedRef<ListLayout | null>,
  canvasHeight: ComputedRef<number>,
  ensureSample: (field: string) => void,
) {
  const selected = ref<number | null>(null);
  const current = computed(() => (selected.value === null ? null : template.elements[selected.value]));
  const layerRows = computed(() =>
    template.elements.map((element, index) => ({ element, index, label: layerLabel(element) })).reverse(),
  );

  function setMode(mode: "label" | "list") {
    if (mode === "label") template.layout = { mode: "label" };
    else if (!list.value) {
      const header = snap(Math.min(20, template.height_mm / 4));
      template.layout = {
        mode: "list",
        header_mm: header,
        row_mm: 10,
        footer_mm: 15,
        footer: "last_page",
        roll: false,
      };
    }
  }

  function bandOf(element: TemplateElement): "header" | "row" | "footer" {
    const layout = list.value;
    if (!layout) return "row";
    const top = element.type === "line" ? Math.min(element.y1, element.y2) : element.y;
    return top < layout.header_mm - 0.001
      ? "header"
      : top < layout.header_mm + layout.row_mm - 0.001
        ? "row"
        : "footer";
  }

  function add(type: Exclude<TemplateElement["type"], "image">) {
    const width = template.width_mm;
    const height = canvasHeight.value;
    const margin = Math.min(2, width / 10);
    const y0 = list.value ? list.value.header_mm + 0.5 : margin;
    const element: TemplateElement =
      type === "text"
        ? {
            type,
            id: newId(),
            x: margin,
            y: y0,
            w: width - 2 * margin,
            h: list.value ? Math.max(1, list.value.row_mm - 1) : Math.min(8, height - 2 * margin),
            text: "Text {{kolom}}",
            size: 10,
            bold: false,
            align: "left",
            valign: "top",
            wrap: true,
            fit: true,
          }
        : type === "line"
          ? {
              type,
              id: newId(),
              x1: margin,
              y1: snap(height / 2),
              x2: width - margin,
              y2: snap(height / 2),
              width: 0.75,
            }
          : type === "rect"
            ? {
                type,
                id: newId(),
                x: margin,
                y: margin,
                w: width - 2 * margin,
                h: height - 2 * margin,
                stroke: 0.75,
                fill: false,
              }
            : type === "barcode"
              ? {
                  type,
                  id: newId(),
                  x: margin,
                  y: margin,
                  w: Math.min(50, width - 2 * margin),
                  h: Math.min(12, height - 2 * margin),
                  value: "{{kode}}",
                  show_text: true,
                }
              : {
                  type,
                  id: newId(),
                  x: margin,
                  y: margin,
                  size: Math.min(20, width - 2 * margin, height - 2 * margin),
                  value: "{{kode}}",
                };
    template.elements.push(element);
    if (element.type === "text") ensureSample("kolom");
    else if (element.type === "barcode" || element.type === "qr") ensureSample("kode");
    selected.value = template.elements.length - 1;
  }

  function removeSelected() {
    if (selected.value === null) return;
    template.elements.splice(selected.value, 1);
    selected.value = null;
  }

  function reorderAt(index: number, direction: -1 | 1) {
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= template.elements.length) return;
    [template.elements[index], template.elements[nextIndex]] = [template.elements[nextIndex], template.elements[index]];
    if (selected.value === index) selected.value = nextIndex;
    else if (selected.value === nextIndex) selected.value = index;
  }

  function selectAndReorder(index: number, direction: -1 | 1) {
    selected.value = index;
    reorderAt(index, direction);
  }

  return { selected, current, layerRows, setMode, bandOf, add, removeSelected, reorderAt, selectAndReorder };
}

function layerLabel(element: TemplateElement) {
  switch (element.type) {
    case "text":
      return `Text · ${element.text.replace(/\s+/g, " ").trim() || "Empty"}`;
    case "line":
      return "Line";
    case "rect":
      return "Rectangle";
    case "barcode":
      return `Barcode · ${element.value}`;
    case "qr":
      return `QR code · ${element.value}`;
    case "image":
      return `Image · ${element.src.startsWith("{{") ? element.src.slice(2, -2) : "Embedded image"}`;
  }
  return "Element";
}
