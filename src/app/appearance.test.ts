import { afterEach, describe, expect, it, vi } from "vitest";

const STORAGE_KEY = "ixprint.appearance.v1";
const originalMatchMedia = Object.getOwnPropertyDescriptor(window, "matchMedia");

function createMediaQuery(initialMatches: boolean) {
  let matches = initialMatches;
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  const addEventListener = vi.fn((type: string, listener: EventListenerOrEventListenerObject | null) => {
    if (type === "change" && typeof listener === "function") {
      listeners.add(listener as (event: MediaQueryListEvent) => void);
    }
  });
  const media = {
    get matches() {
      return matches;
    },
    media: "(prefers-color-scheme: dark)",
    onchange: null,
    addEventListener,
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(() => true),
    addListener: vi.fn(),
    removeListener: vi.fn(),
  } as unknown as MediaQueryList;

  return {
    media,
    addEventListener,
    listenerCount: () => listeners.size,
    setMatches(next: boolean) {
      matches = next;
      const event = { matches, media: media.media } as MediaQueryListEvent;
      for (const listener of listeners) listener(event);
    },
  };
}

function mockMatchMedia(media: MediaQueryList) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn(() => media),
  });
}

async function loadAppearance() {
  await vi.resetModules();
  return import("./appearance");
}

function restoreMatchMedia() {
  if (originalMatchMedia) Object.defineProperty(window, "matchMedia", originalMatchMedia);
  else Reflect.deleteProperty(window, "matchMedia");
}

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  restoreMatchMedia();
});

describe("appearance", () => {
  it("defaults to System and uses Light when the OS is light without writing storage", async () => {
    const query = createMediaQuery(false);
    mockMatchMedia(query.media);
    const { appearance, initializeAppearance } = await loadAppearance();

    initializeAppearance();

    expect(appearance.value).toBe("system");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.length).toBe(0);
  });

  it("defaults to System and uses Dark when the OS is dark", async () => {
    const query = createMediaQuery(true);
    mockMatchMedia(query.media);
    const { appearance, initializeAppearance } = await loadAppearance();

    initializeAppearance();

    expect(appearance.value).toBe("system");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.length).toBe(0);
  });

  it.each([
    { stored: "light", osIsDark: true, expected: "light" },
    { stored: "dark", osIsDark: false, expected: "dark" },
  ] as const)("honors stored $stored preference over the OS", async ({ stored, osIsDark, expected }) => {
    localStorage.setItem(STORAGE_KEY, stored);
    mockMatchMedia(createMediaQuery(osIsDark).media);
    const { appearance, initializeAppearance } = await loadAppearance();

    initializeAppearance();

    expect(appearance.value).toBe(stored);
    expect(document.documentElement.dataset.theme).toBe(expected);
  });

  it("treats an unknown stored value as System without cleaning it up", async () => {
    localStorage.setItem(STORAGE_KEY, "sepia");
    mockMatchMedia(createMediaQuery(true).media);
    const { appearance, initializeAppearance } = await loadAppearance();

    initializeAppearance();

    expect(appearance.value).toBe("system");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("sepia");
  });

  it("falls back to System and follows the OS when reading storage throws", async () => {
    mockMatchMedia(createMediaQuery(true).media);
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("storage unavailable");
    });
    const { appearance, initializeAppearance } = await loadAppearance();

    initializeAppearance();

    expect(appearance.value).toBe("system");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("follows OS changes only while System is selected", async () => {
    const query = createMediaQuery(false);
    mockMatchMedia(query.media);
    const { appearance, initializeAppearance, setAppearance } = await loadAppearance();
    initializeAppearance();

    query.setMatches(true);
    expect(document.documentElement.dataset.theme).toBe("dark");

    expect(setAppearance("light")).toBe(true);
    query.setMatches(false);
    query.setMatches(true);

    expect(appearance.value).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("uses the latest OS state when switching back to System", async () => {
    const query = createMediaQuery(false);
    mockMatchMedia(query.media);
    const { initializeAppearance, setAppearance } = await loadAppearance();
    initializeAppearance();
    setAppearance("dark");
    query.setMatches(true);
    query.setMatches(false);

    setAppearance("system");

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("writes the selected value and applies it immediately", async () => {
    mockMatchMedia(createMediaQuery(false).media);
    const { appearance, initializeAppearance, setAppearance } = await loadAppearance();
    initializeAppearance();

    expect(setAppearance("dark")).toBe(true);

    expect(appearance.value).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("dark");
    expect(localStorage.length).toBe(1);
  });

  it("keeps the new session preference when saving fails", async () => {
    mockMatchMedia(createMediaQuery(false).media);
    const { appearance, initializeAppearance, setAppearance } = await loadAppearance();
    initializeAppearance();
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("storage unavailable");
    });

    expect(setAppearance("dark")).toBe(false);

    expect(appearance.value).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it.each(["missing", "throws"] as const)("uses Light for System when matchMedia is %s", async (mode) => {
    if (mode === "missing") {
      Reflect.deleteProperty(window, "matchMedia");
    } else {
      Object.defineProperty(window, "matchMedia", {
        configurable: true,
        value: vi.fn(() => {
          throw new Error("matchMedia unavailable");
        }),
      });
    }
    const { appearance, initializeAppearance, setAppearance } = await loadAppearance();

    initializeAppearance();
    expect(appearance.value).toBe("system");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(setAppearance("dark")).toBe(true);
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("initializes only once without rereading storage or adding another listener", async () => {
    localStorage.setItem(STORAGE_KEY, "dark");
    const query = createMediaQuery(false);
    mockMatchMedia(query.media);
    const getItem = vi.spyOn(Storage.prototype, "getItem");
    const { appearance, initializeAppearance, setAppearance } = await loadAppearance();

    initializeAppearance();
    setAppearance("light");
    initializeAppearance();

    expect(appearance.value).toBe("light");
    expect(getItem).toHaveBeenCalledTimes(1);
    expect(query.addEventListener).toHaveBeenCalledTimes(1);
    expect(query.listenerCount()).toBe(1);
  });
});
