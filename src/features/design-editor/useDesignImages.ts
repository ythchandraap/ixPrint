import { ref, type ComputedRef, type Ref } from "vue";
import { errorText, type Template, type TemplateElement } from "@/shared/api";

const MAX_IMAGE_PX = 1600;
const snap = (value: number) => Math.round(value / 0.5) * 0.5;

export function useDesignImages(template: Template, canvasHeight: ComputedRef<number>, selected: Ref<number | null>) {
  const fileInput = ref<HTMLInputElement | null>(null);
  const imageError = ref("");
  let replacing: number | null = null;

  function pickImage(index: number | null) {
    replacing = index;
    imageError.value = "";
    fileInput.value?.click();
  }

  function toDataUrl(file: File): Promise<{ url: string; width: number; height: number }> {
    return new Promise((resolve, reject) => {
      const objectUrl = URL.createObjectURL(file);
      const img = new Image();
      img.onload = () => {
        let width = img.naturalWidth || 1024;
        let height = img.naturalHeight || 1024;
        const factor = Math.min(1, MAX_IMAGE_PX / Math.max(width, height));
        width = Math.max(1, Math.round(width * factor));
        height = Math.max(1, Math.round(height * factor));
        const canvas = document.createElement("canvas");
        [canvas.width, canvas.height] = [width, height];
        const context = canvas.getContext("2d");
        if (!context) return reject("Canvas is unavailable");
        context.drawImage(img, 0, 0, width, height);
        URL.revokeObjectURL(objectUrl);
        const url = file.type === "image/jpeg" ? canvas.toDataURL("image/jpeg", 0.9) : canvas.toDataURL("image/png");
        resolve({ url, width, height });
      };
      img.onerror = () => {
        URL.revokeObjectURL(objectUrl);
        reject(
          `"${file.name}" cannot be opened as an image: the file is damaged or its format is not supported on this computer`,
        );
      };
      img.src = objectUrl;
    });
  }

  async function onImageFile(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    try {
      const { url, width, height } = await toDataUrl(file);
      const target: TemplateElement | undefined = replacing === null ? undefined : template.elements[replacing];
      if (target?.type === "image") {
        target.src = url;
        return;
      }
      const designWidth = template.width_mm;
      const designHeight = canvasHeight.value;
      let elementWidth = snap(Math.max(5, Math.min(designWidth * 0.4, designWidth - 4)));
      let elementHeight = snap((elementWidth * height) / width);
      if (elementHeight > designHeight - 4) {
        elementHeight = snap(Math.max(5, designHeight - 4));
        elementWidth = snap((elementHeight * width) / height);
      }
      template.elements.push({
        type: "image",
        id: Math.random().toString(36).slice(2, 10),
        x: 2,
        y: 2,
        w: elementWidth,
        h: elementHeight,
        src: url,
        fit: "contain",
        grayscale: false,
      });
      selected.value = template.elements.length - 1;
    } catch (error) {
      imageError.value = errorText(error);
    }
  }

  return { fileInput, imageError, pickImage, onImageFile };
}
