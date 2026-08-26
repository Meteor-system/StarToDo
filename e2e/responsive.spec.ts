import { expect, test, type Page } from '@playwright/test';

const browserErrors = new WeakMap<Page, string[]>();

interface PlannerMeasurements {
  viewport: { width: number; height: number };
  container: { width: number; height: number };
  document: { width: number; height: number };
}

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  browserErrors.set(page, errors);

  page.on('pageerror', (error) => errors.push(`pageerror: ${error.message}`));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(`console.error: ${message.text()}`);
  });
});

test.afterEach(async ({ page }, testInfo) => {
  const errors = browserErrors.get(page) ?? [];
  await testInfo.attach('browser-errors', {
    body: JSON.stringify(errors),
    contentType: 'application/json'
  });
  expect(errors, `Unexpected browser errors:\n${errors.join('\n')}`).toEqual([]);
});

async function openPlanner(page: Page): Promise<void> {
  await page.goto('/');
  await page.getByRole('button', { name: '周计划' }).click();
  await expect(page.getByRole('dialog', { name: '周计划' })).toBeVisible();
}

async function plannerMeasurements(page: Page): Promise<PlannerMeasurements> {
  return page.evaluate(() => {
    const planner = document.querySelector<HTMLElement>('.planner');
    if (!planner) throw new Error('Planner container was not found');
    const bounds = planner.getBoundingClientRect();
    return {
      viewport: { width: window.innerWidth, height: window.innerHeight },
      container: { width: bounds.width, height: bounds.height },
      document: {
        width: document.documentElement.scrollWidth,
        height: document.documentElement.scrollHeight
      }
    };
  });
}

async function expectDocumentInsideViewport(page: Page): Promise<PlannerMeasurements> {
  const measurements = await plannerMeasurements(page);
  expect(measurements.document.width).toBeLessThanOrEqual(measurements.viewport.width);
  expect(measurements.document.height).toBeLessThanOrEqual(measurements.viewport.height);
  return measurements;
}

test('planner stays inside a 520x420 viewport with a narrow selected-day surface', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 520, height: 420 });
  await openPlanner(page);

  const tablist = page.getByRole('tablist', { name: '周计划日期' });
  await expect(tablist).toBeVisible();
  const tabs = tablist.getByRole('tab');
  await expect(tabs).toHaveCount(8);
  await expect(tablist.getByRole('tab', { selected: true })).toHaveCount(1);
  for (let index = 0; index < 8; index += 1) {
    await tabs.nth(index).focus();
    await expect(tabs.nth(index)).toBeFocused();
    await expect(tabs.nth(index)).not.toHaveAttribute('tabindex', '-1');
  }
  await tabs.nth(1).press('Enter');
  await expect(tabs.nth(1)).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', await tabs.nth(1).getAttribute('id') ?? '');
  await expect(page.getByRole('tabpanel')).toBeVisible();
  await expect(page.getByRole('region', { name: '周计划看板，可横向滚动' })).toBeHidden();

  const measurements = await expectDocumentInsideViewport(page);
  expect(measurements.container.width).toBeLessThan(760);
  console.log(`planner measurements 520x420 ${JSON.stringify(measurements)}`);
  await testInfo.attach('planner-measurements', {
    body: JSON.stringify(measurements),
    contentType: 'application/json'
  });
});

test('planner layout follows a sub-760 drawer container at 760x560', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 760, height: 560 });
  await openPlanner(page);

  const measurements = await expectDocumentInsideViewport(page);
  expect(measurements.container.width).toBeLessThan(760);
  console.log(`planner measurements 760x560 ${JSON.stringify(measurements)}`);
  await expect(page.getByRole('tablist', { name: '周计划日期' })).toBeVisible();
  await expect(page.getByRole('tabpanel')).toBeVisible();
  await expect(page.getByRole('region', { name: '周计划看板，可横向滚动' })).toBeHidden();
  await testInfo.attach('planner-measurements', {
    body: JSON.stringify(measurements),
    contentType: 'application/json'
  });
});

test('planner layout shows the wide board in a wide drawer at 1180x760', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1180, height: 760 });
  await openPlanner(page);

  const measurements = await expectDocumentInsideViewport(page);
  expect(measurements.container.width).toBeGreaterThanOrEqual(760);
  console.log(`planner measurements 1180x760 ${JSON.stringify(measurements)}`);
  await expect(page.getByRole('region', { name: '周计划看板，可横向滚动' })).toBeVisible();
  await expect(page.getByRole('tablist', { name: '周计划日期' })).toBeHidden();
  await expect(page.getByRole('tabpanel')).toBeHidden();
  await testInfo.attach('planner-measurements', {
    body: JSON.stringify(measurements),
    contentType: 'application/json'
  });
});
