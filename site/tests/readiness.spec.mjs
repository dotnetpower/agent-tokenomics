import { test, expect } from "@playwright/test";
import { createSiteServer } from "../scripts/serve.mjs";
import { base } from "../scripts/site-contract.mjs";

let server;
let origin;
test.beforeAll(async () => {
  server = await createSiteServer();
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  origin = `http://127.0.0.1:${server.address().port}`;
});
test.afterAll(async () => {
  if (server) await new Promise((resolve) => server.close(resolve));
});

async function expectDocumentDates(page) {
  const dates = page.locator(".document-dates");
  await expect(dates).toBeVisible();
  for (const [index, key] of ["created", "updated"].entries()) {
    const meta = page.locator(`meta[name="date.${key}"]`);
    await expect(meta).toHaveAttribute("content", /^\d{4}-\d{2}-\d{2}$/);
    const value = await meta.getAttribute("content");
    await expect(dates.locator("time").nth(index)).toHaveAttribute("datetime", value);
    await expect(dates.locator("time").nth(index)).toHaveText(value);
  }
  const layout = await dates.locator("dl > div").evaluateAll((groups) => groups.map((group) => {
    const label = group.querySelector("dt");
    const value = group.querySelector("dd");
    return {
      top: group.getBoundingClientRect().top,
      labelTop: label.getBoundingClientRect().top,
      valueTop: value.getBoundingClientRect().top,
      valueMargin: getComputedStyle(value).marginTop,
      valueIndent: getComputedStyle(value).paddingInlineStart,
    };
  }));
  for (const group of layout) {
    expect(group.valueMargin).toBe("0px");
    expect(group.valueIndent).toBe("0px");
    expect(Math.abs(group.labelTop - group.valueTop)).toBeLessThan(1);
  }
  if (page.viewportSize().width >= 1280) {
    expect(Math.max(...layout.map((group) => group.top)) - Math.min(...layout.map((group) => group.top))).toBeLessThan(1);
  }
}

for (const locale of ["en", "ko"]) {
  for (const viewport of [{ width: 1280, height: 900 }, { width: 390, height: 844 }]) {
    test(`${locale} ${viewport.width}px: coding agent anatomy belongs to Copilot with aligned dates`, async ({ page }, testInfo) => {
      await page.setViewportSize(viewport);
      const prefix = `${base}/${locale === "ko" ? "ko/" : ""}`;
      const path = "copilot-tokenomics/coding-agent-quotas-and-caching/";
      await page.goto(`${origin}${prefix}${path}`, { waitUntil: "networkidle" });
      await expect(page.locator("h1")).toContainText(locale === "ko" ? "코딩 에이전트" : "Coding Agent");
      await expectDocumentDates(page);
      await expect(page.locator(".answer-capsule")).toHaveCount(0);
      const currentLink = page.locator(`nav.sidebar a[href="${prefix}${path}"]`);
      await expect(currentLink).toHaveCount(1);
      await expect(currentLink).toHaveAttribute("aria-current", "page");
      const group = currentLink.locator("xpath=ancestor::details[last()]");
      await expect(group.locator("summary").first()).toHaveText(locale === "ko" ? "Copilot 토큰 절약" : "Copilot Tokenomics");
      expect(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)).toBe(false);
      await page.screenshot({ path: testInfo.outputPath("document-dates.png") });
      await page.goto(`${origin}${prefix}agent-tokenomics/coding-agent-quotas-and-caching/`);
      await expect(page).toHaveURL(`${origin}${prefix}${path}`);
    });

    test(`${locale} ${viewport.width}px: visible summary, budget math, and CLS < 0.10`, async ({ page }) => {
      await page.setViewportSize(viewport);
      await page.addInitScript(() => {
        window.__layoutShifts = [];
        new PerformanceObserver((list) => {
          for (const entry of list.getEntries()) {
            if (!entry.hadRecentInput) window.__layoutShifts.push({ value: entry.value, time: entry.startTime });
          }
        }).observe({ type: "layout-shift", buffered: true });
      });
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto(`${origin}${base}/${locale === "ko" ? "ko/" : ""}`, { waitUntil: "networkidle" });
      await page.evaluate(() => document.fonts.ready);
      await expect(page.locator("h1")).toHaveCount(1);
      const summary = page.locator(".at-hero .answer-capsule");
      await expect(summary).toBeVisible();
      const summaryBox = await summary.boundingBox();
      expect(summaryBox.y).toBeGreaterThanOrEqual(0);
      expect(summaryBox.y + summaryBox.height).toBeLessThan(viewport.height);
      const form = page.locator("[data-token-budget]");
      await form.scrollIntoViewIfNeeded();
      const initialHeight = (await form.boundingBox()).height;
      const result = form.locator("output");
      await expect(result).toHaveText(locale === "ko" ? "남은 예산: 16,000 토큰" : "Remaining budget: 16,000 tokens");
      await form.locator("[name=input]").fill("31000");
      await form.locator("button").click();
      await expect(result).toHaveText(locale === "ko" ? "예산 초과: 3,000 토큰" : "Over budget: 3,000 tokens");
      expect((await form.boundingBox()).height).toBe(initialHeight);
      await form.locator("[name=input]").fill("-1");
      await form.locator("button").click();
      await expect(result).toHaveText(locale === "ko" ? "예산 초과: 3,000 토큰" : "Over budget: 3,000 tokens");
      expect(await form.locator("[name=input]").evaluate((input) => input.validity.rangeUnderflow)).toBe(true);
      const measurements = await page.evaluate(() => {
        let max = 0, sum = 0, first = 0, previous = 0;
        for (const shift of window.__layoutShifts) {
          if (shift.time - previous > 1000 || shift.time - first > 5000) {
            sum = 0;
            first = shift.time;
          }
          sum += shift.value;
          max = Math.max(max, sum);
          previous = shift.time;
        }
        return { cls: max, overflow: document.documentElement.scrollWidth > innerWidth };
      });
      expect(measurements.cls).toBeLessThan(0.10);
      expect(measurements.overflow).toBe(false);
      await expectDocumentDates(page);
      await page.goto(`${origin}${base}/${locale === "ko" ? "ko/" : ""}copilot-tokenomics/02-measure-usage/`,
        { waitUntil: "networkidle" });
      await expectDocumentDates(page);
      expect(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)).toBe(false);
      expect(errors).toEqual([]);
      console.log(`${locale} ${viewport.width}px: CLS=${measurements.cls}; widget height unchanged`);
    });
  }
}
