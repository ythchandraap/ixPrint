import { ref } from "vue";

export type Appearance = "system" | "light" | "dark";

const STORAGE_KEY = "ixprint.appearance.v1";
const SYSTEM_QUERY = "(prefers-color-scheme: dark)";

export const appearance = ref<Appearance>("system");

let initialized = false;
let systemPreference: MediaQueryList | undefined;

function isAppearance(value: string | null): value is Appearance {
  return value === "system" || value === "light" || value === "dark";
}

function applyEffectiveTheme() {
  const effectiveTheme =
    appearance.value === "system"
      ? systemPreference?.matches
        ? "dark"
        : "light"
      : appearance.value;
  document.documentElement.dataset.theme = effectiveTheme;
}

function onSystemAppearanceChange() {
  if (appearance.value === "system") applyEffectiveTheme();
}

export function initializeAppearance(): void {
  if (initialized) return;
  initialized = true;

  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    appearance.value = isAppearance(stored) ? stored : "system";
  } catch {
    appearance.value = "system";
  }

  try {
    if (typeof window.matchMedia === "function") {
      systemPreference = window.matchMedia(SYSTEM_QUERY);
      systemPreference.addEventListener("change", onSystemAppearanceChange);
    }
  } catch {
    systemPreference = undefined;
  }

  applyEffectiveTheme();
}

export function setAppearance(next: Appearance): boolean {
  appearance.value = next;
  applyEffectiveTheme();

  try {
    window.localStorage.setItem(STORAGE_KEY, next);
    return true;
  } catch {
    return false;
  }
}
