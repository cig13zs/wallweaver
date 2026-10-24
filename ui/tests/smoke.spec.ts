import { test, expect, type Page } from "@playwright/test";

const OUT = "tests/screens";

/** Collects console errors, page errors and CSP violations for the whole test. */
function watchErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (m) => { if (m.type() === "error") errors.push(`console: ${m.text()}`); });
  page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
  page.on("requestfailed", (r) => errors.push(`requestfailed: ${r.url()} ${r.failure()?.errorText ?? ""}`));
  page.on("request", (r) => {
    const url = new URL(r.url());
    if (url.hostname !== "localhost") errors.push(`external request: ${r.url()}`);
  });
  return errors;
}

/** Waits until every on-screen artwork canvas has been drawn and fonts are ready. */
async function settle(page: Page) {
  await page.evaluate(() => document.fonts.ready);
  await page.waitForFunction(() => {
    const visible = [...document.querySelectorAll<HTMLCanvasElement>("canvas.art")].filter((c) => {
      const r = c.getBoundingClientRect();
      return r.width > 0 && r.height > 0 && r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth;
    });
    return visible.length > 0 && visible.every((c) => c.dataset.drawn === "1");
  });
}

async function open(page: Page, hash: string) {
  await page.goto(`/?static${hash}`);
  await expect(page.locator(".sidebar")).toBeVisible();
}

test("discover", async ({ page }) => {
  const errors = watchErrors(page);
  await open(page, "#/discover");
  await expect(page.getByRole("heading", { name: "Aurora Loom" })).toBeVisible();
  await expect(page.locator(".card-w")).toHaveCount(16);
  await settle(page);
  await page.locator(".card-w").nth(2).hover();
  await page.waitForTimeout(250);
  await page.screenshot({ path: `${OUT}/01-discover.png` });
  expect(errors).toEqual([]);
});

test("discover filters and keyboard", async ({ page }) => {
  const errors = watchErrors(page);
  await open(page, "#/discover");
  await page.getByRole("button", { name: "Space", exact: true }).click();
  await expect(page.locator(".card-w")).toHaveCount(3);
  await page.getByRole("switch", { name: "Low impact only" }).click();
  await expect(page.locator(".card-w")).toHaveCount(0);
  await page.getByRole("button", { name: "All", exact: true }).click();
  await expect(page.locator(".card-w")).toHaveCount(10);
  await page.keyboard.press("Control+K");
  await expect(page.locator("[data-search]")).toBeFocused();
  await page.keyboard.type("petal");
  await expect(page.locator(".card-w")).toHaveCount(1);
  expect(errors).toEqual([]);
});

test("wallpaper detail", async ({ page }) => {
  const errors = watchErrors(page);
  await open(page, "#/wallpaper/aurora-loom");
  await expect(page.getByRole("heading", { name: "Aurora Loom", level: 1 })).toBeVisible();
  await expect(page.getByRole("button", { name: /Apply to displays 2 and 3/ })).toBeVisible();
  await settle(page);
  await page.screenshot({ path: `${OUT}/02-detail.png` });
  await page.getByRole("button", { name: /Display 1/ }).click();
  await expect(page.getByRole("button", { name: /Apply to displays 1, 2 and 3/ })).toBeVisible();
  await page.getByRole("button", { name: /Apply to displays/ }).click();
  await expect(page.getByRole("status").filter({ hasText: "Aurora Loom is on displays 1, 2 and 3" })).toBeVisible();
  expect(errors).toEqual([]);
});

test("permission consent", async ({ page }) => {
  const errors = watchErrors(page);
  await open(page, "#/discover?install=weather-glass");
  const dialog = page.getByRole("dialog", { name: /Weather Glass wants to/ });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("radiogroup", { name: "Read CPU and GPU usage" }).getByRole("radio", { name: "Deny" }).click();
  await expect(dialog.getByRole("button", { name: "Install with 3 of 4 allowed" })).toBeVisible();
  await settle(page);
  await page.mouse.move(0, 0);
  await page.screenshot({ path: `${OUT}/03-consent.png` });
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  expect(errors).toEqual([]);
});

test("install flow grants only what was allowed", async ({ page }) => {
  const errors = watchErrors(page);
  await open(page, "#/discover?install=weather-glass");
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("radiogroup", { name: /Follow your mouse/ }).getByRole("radio", { name: "Deny" }).click();
  await dialog.getByRole("button", { name: "Install with 3 of 4 allowed" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Weather Glass installed with 3 of 4 permissions allowed" })).toBeVisible();
  expect(errors).toEqual([]);
});

test("performance and power", async ({ page }) => {
  const errors = watchErrors(page);
  await open(page, "#/performance");
  await expect(page.getByRole("heading", { name: "Performance & power" })).toBeVisible();
  await expect(page.getByRole("radio", { name: /Balanced/ })).toHaveAttribute("aria-checked", "true");
  await settle(page);
  await page.screenshot({ path: `${OUT}/04-performance.png` });
  await page.getByRole("radio", { name: /Battery saver/ }).click();
  await expect(page.getByRole("radio", { name: /Battery saver/ })).toHaveAttribute("aria-checked", "true");
  expect(errors).toEqual([]);
});

test("creator studio", async ({ page }) => {
  const errors = watchErrors(page);
  await open(page, "#/creator");
  await expect(page.getByRole("heading", { name: "aurora-loom" })).toBeVisible();
  await settle(page);
  await page.screenshot({ path: `${OUT}/05-creator.png` });
  await page.getByRole("button", { name: "Declare this origin" }).click();
  await expect(page.getByRole("button", { name: "Declare this origin" })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("supporter", async ({ page }) => {
  const errors = watchErrors(page);
  await open(page, "#/supporter");
  await expect(page.getByRole("heading", { name: /free and open source/ })).toBeVisible();
  await settle(page);
  await page.screenshot({ path: `${OUT}/07-supporter.png` });
  expect(errors).toEqual([]);
});

test("settings and light theme", async ({ page }) => {
  const errors = watchErrors(page);
  await open(page, "#/settings");
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await page.screenshot({ path: `${OUT}/08-settings.png` });
  await page.getByRole("radio", { name: "Light" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.screenshot({ path: `${OUT}/08-settings-light.png` });
  await page.goto("/?static#/discover");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(page.locator(".card-w")).toHaveCount(16);
  await settle(page);
  await page.locator(".card-w").nth(2).hover();
  await page.waitForTimeout(250);
  await page.screenshot({ path: `${OUT}/01-discover-light.png` });
  await page.goto("/?static#/supporter");
  await settle(page);
  await page.screenshot({ path: `${OUT}/07-supporter-light.png` });
  expect(errors).toEqual([]);
});

test("sidebar keyboard navigation reaches every screen", async ({ page }) => {
  const errors = watchErrors(page);
  await open(page, "#/discover");
  for (const name of ["Performance", "Creator Studio", "Settings", "Support Wallweaver", "Discover"]) {
    const link = page.getByRole("navigation", { name: "Main" }).getByRole("link", { name });
    await link.focus();
    await expect(link).toBeFocused();
    const outline = await link.evaluate((el) => getComputedStyle(el).outlineStyle);
    expect(outline).not.toBe("none");
    await page.keyboard.press("Enter");
    await expect(link).toHaveAttribute("aria-current", "page");
  }
  expect(errors).toEqual([]);
});

test("reduced motion disables transitions", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await open(page, "#/discover");
  const dur = await page.locator(".chip").first().evaluate((el) => getComputedStyle(el).transitionDuration);
  expect(parseFloat(dur)).toBeLessThan(0.01);
});
