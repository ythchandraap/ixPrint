import { ref, type Ref } from "vue";
import type { Template, TemplateElement } from "@/shared/api";

export function useDesignClipboard(
	template: Template,
	selected: Ref<number | null>,
	moveBy: (element: TemplateElement, dx: number, dy: number) => void,
) {
	const copiedElement = ref<TemplateElement | null>(null);
	let pasteCount = 0;

	function copySelected() {
		if (selected.value === null) return;
		copiedElement.value = JSON.parse(JSON.stringify(template.elements[selected.value])) as TemplateElement;
		pasteCount = 0;
	}

	function pasteCopied() {
		if (!copiedElement.value) return;
		const copy = JSON.parse(JSON.stringify(copiedElement.value)) as TemplateElement;
		copy.id = Math.random().toString(36).slice(2, 10);
		pasteCount += 1;
		moveBy(copy, 2 * pasteCount, 2 * pasteCount);
		template.elements.push(copy);
		selected.value = template.elements.length - 1;
	}

	function duplicateSelected() {
		copySelected();
		pasteCopied();
	}

	return { copiedElement, copySelected, pasteCopied, duplicateSelected };
}
