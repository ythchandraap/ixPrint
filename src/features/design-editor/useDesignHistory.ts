import { nextTick, onBeforeUnmount, ref, watch } from "vue";
import type { Template } from "@/shared/api";

/** Bounded snapshots keep undo usable while dragging and typing without retaining unlimited embedded images. */
export function useDesignHistory(template: Template) {
  let captureView: () => unknown = () => undefined;
  let restoreView: (state: unknown) => void = () => {};
  const encode = () => JSON.stringify({ template, view: captureView() });
  const snapshots = [encode()];
  const index = ref(0);
  const canUndo = ref(false);
  const canRedo = ref(false);
  let timer: number | undefined;
  let restoring = false;
  let transaction = false;

  function updateAvailability() {
    canUndo.value = index.value > 0;
    canRedo.value = index.value < snapshots.length - 1;
  }
  function commit() {
    clearTimeout(timer);
    timer = undefined;
    if (restoring) return;
    const state = encode();
    const prior = JSON.parse(snapshots[index.value]) as { template: Template };
    const current = JSON.parse(state) as { template: Template };
    if (JSON.stringify(prior.template) === JSON.stringify(current.template)) {
      snapshots[index.value] = state;
      return;
    }
    snapshots.splice(index.value + 1);
    snapshots.push(state);
    index.value = snapshots.length - 1;
    while (snapshots.length > 30 || snapshots.reduce((bytes, item) => bytes + item.length, 0) > 32_000_000) {
      if (snapshots.length <= 2) break;
      snapshots.shift();
      index.value--;
    }
    updateAvailability();
  }
  watch(() => JSON.stringify(template), () => {
    if (restoring) return;
    if (transaction) return;
    clearTimeout(timer);
    timer = window.setTimeout(commit, 300);
  });
  async function restore(next: number) {
    restoring = true;
    index.value = next;
    const snapshot = JSON.parse(snapshots[next]) as { template: Template; view: unknown };
    const value = snapshot.template;
    const elements = template.elements;
    for (const key of Object.keys(template)) delete (template as unknown as Record<string, unknown>)[key];
    Object.assign(template, value);
    elements.splice(0, elements.length, ...value.elements);
    template.elements = elements;
    updateAvailability();
    await nextTick();
    restoreView(snapshot.view);
    restoring = false;
  }
  function undo() {
    commit();
    if (index.value > 0) void restore(index.value - 1);
  }
  function redo() {
    if (timer !== undefined) commit();
    if (index.value < snapshots.length - 1) void restore(index.value + 1);
  }
  function bindView(capture: () => unknown, restore: (state: unknown) => void) {
    captureView = capture;
    restoreView = restore;
    snapshots[index.value] = encode();
  }
  function markView() {
    commit();
    snapshots[index.value] = encode();
  }
  function beginTransaction() {
    commit();
    transaction = true;
  }
  function endTransaction() {
    transaction = false;
    commit();
  }
  onBeforeUnmount(() => clearTimeout(timer));
  return { undo, redo, canUndo, canRedo, commit, bindView, markView, beginTransaction, endTransaction };
}
