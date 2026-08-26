import { expect, test, type Page } from '@playwright/test';

const browserErrors = new WeakMap<Page, string[]>();

interface FocusMeasurements {
  viewport: { width: number; height: number };
  document: { width: number; height: number };
  canvas: { x: number; y: number; width: number; height: number };
  stage: { x: number; y: number; width: number; height: number };
  timer: { x: number; y: number; width: number; height: number };
}

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

async function openFocus(page: Page): Promise<void> {
  await page.goto('/');
  await page.getByRole('button', { name: '专注' }).click();
  await expect(page.getByRole('timer')).toBeVisible();
}

async function focusMeasurements(page: Page): Promise<FocusMeasurements> {
  return page.evaluate(() => {
    const canvas = document.querySelector<HTMLElement>('.stage-canvas');
    const stage = document.querySelector<HTMLElement>('[data-focus-stage]');
    const timer = document.querySelector<HTMLElement>('[role="timer"]');
    if (!canvas || !stage || !timer) throw new Error('Focus layout was not found');
    const toRect = (element: HTMLElement) => {
      const bounds = element.getBoundingClientRect();
      return { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height };
    };
    return {
      viewport: { width: window.innerWidth, height: window.innerHeight },
      document: {
        width: document.documentElement.scrollWidth,
        height: document.documentElement.scrollHeight
      },
      canvas: toRect(canvas),
      stage: toRect(stage),
      timer: toRect(timer)
    };
  });
}

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

async function installActiveFloatingTauriMock(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const callbacks = new Map<number, (payload: unknown) => void>();
    let callbackId = 0;
    const preferences = {
      visible: true,
      x: null,
      y: null,
      width: 340,
      height: 64,
      alwaysOnTop: true,
      userResized: false,
      displayMode: 'capsule'
    };
    const snapshot = {
      settings: {
        focusMinutes: 25,
        shortBreakMinutes: 5,
        longBreakMinutes: 15,
        longBreakInterval: 4,
        updatedAtUnixMs: Date.now()
      },
      currentSession: {
        id: 1,
        taskId: null,
        taskTitleSnapshot: '测试专注任务',
        phase: 'focus',
        status: 'running',
        plannedDurationSeconds: 1500,
        pausedRemainingSeconds: null,
        startedAtUnixMs: Date.now(),
        targetEndsAtUnixMs: Date.now() + 1_500_000,
        pausedAtUnixMs: null,
        endedAtUnixMs: null,
        notificationTag: null,
        createdAtUnixMs: Date.now(),
        updatedAtUnixMs: Date.now()
      },
      completedFocusesInCycle: 0,
      recommendedPhase: 'focus',
      completedFocusTodayCount: 0
    };

    Object.assign(window, {
      __TAURI_INTERNALS__: {
        transformCallback(callback: (payload: unknown) => void) {
          const id = ++callbackId;
          callbacks.set(id, callback);
          return id;
        },
        unregisterCallback(id: number) {
          callbacks.delete(id);
        },
        async invoke(command: string, args: Record<string, unknown> = {}) {
          if (command === 'get_pomodoro_view') return { snapshot, taskSummaries: [] };
          if (command === 'list_tasks' || command === 'list_projects' || command === 'list_reminders') return [];
          if (command === 'get_floating_window_preferences') return preferences;
          if (command === 'set_floating_display_mode') {
            preferences.displayMode = args.mode as 'capsule' | 'expanded';
            return { ...preferences };
          }
          if (command === 'plugin:event|listen') return 1;
          if (command === 'plugin:event|unlisten') return null;
          if (command === 'open_focus_from_floating' || command === 'hide_floating_window') return null;
          throw new Error(`Unexpected mocked Tauri command: ${command}`);
        }
      }
    });
  });
}

test('active capsule preserves logical focus while expanding', async ({ page }) => {
  await installActiveFloatingTauriMock(page);
  await page.setViewportSize({ width: 340, height: 64 });
  await page.goto('/?window=floating');

  const floating = page.getByRole('region', { name: 'StarToDo 悬浮窗' });
  await expect(floating).toHaveAttribute('data-display-mode', 'capsule');

  await page.keyboard.press('Tab');
  await expect(floating).toHaveAttribute('data-display-mode', 'interaction-expanded');
  await expect(page.getByRole('button', { name: '打开专注工作区' })).toBeFocused();

  await page.reload();
  await expect(floating).toHaveAttribute('data-display-mode', 'capsule');
  const expand = page.getByRole('button', { name: '展开悬浮窗' });
  await expand.focus();
  await expect(floating).toHaveAttribute('data-display-mode', 'interaction-expanded');
  await expect(page.getByRole('button', { name: '隐藏悬浮窗' })).toBeFocused();
});

test('keyboard focus inside keeps the active companion expanded past its deadline', async ({ page }) => {
  await page.clock.install({ time: new Date('2026-08-27T12:00:00+08:00') });
  await installActiveFloatingTauriMock(page);
  await page.setViewportSize({ width: 340, height: 64 });
  await page.goto('/?window=floating');

  const floating = page.getByRole('region', { name: 'StarToDo 悬浮窗' });
  await expect(floating).toHaveAttribute('data-display-mode', 'capsule');
  await page.keyboard.press('Tab');
  await expect(floating).toHaveAttribute('data-display-mode', 'interaction-expanded');
  const focusedTarget = page.getByRole('button', { name: '打开专注工作区' });
  await expect(focusedTarget).toBeFocused();

  await page.clock.fastForward(5_700);
  await expect(floating).toHaveAttribute('data-display-mode', 'interaction-expanded');
  await expect(focusedTarget).toBeFocused();

  await page.evaluate(() => {
    const outside = document.createElement('button');
    outside.textContent = 'outside floating companion';
    document.body.append(outside);
    outside.focus();
  });
  expect(await floating.evaluate((region) => !region.contains(document.activeElement))).toBe(true);
  await page.clock.fastForward(700);
  await expect(floating).toHaveAttribute('data-display-mode', 'capsule');
});

test('failed logical focus restoration arms bounded collapse', async ({ page }) => {
  await page.clock.install({ time: new Date('2026-08-25T12:00:00Z') });
  await installActiveFloatingTauriMock(page);
  await page.addInitScript(() => {
    window.addEventListener('DOMContentLoaded', () => {
      new MutationObserver(() => {
        const region = document.querySelector<HTMLElement>('[aria-label="StarToDo 悬浮窗"]');
        if (region?.dataset.displayMode !== 'interaction-expanded') return;
        const targets = region.querySelectorAll<HTMLButtonElement>('.expanded-panel [data-floating-focus-target]');
        for (const target of targets) target.disabled = true;
      }).observe(document.documentElement, { subtree: true, childList: true });
    }, { once: true });
  });
  await page.setViewportSize({ width: 340, height: 64 });
  await page.goto('/?window=floating');

  const floating = page.getByRole('region', { name: 'StarToDo 悬浮窗' });
  await expect(floating).toHaveAttribute('data-display-mode', 'capsule');
  await page.getByRole('button', { name: '暂停' }).focus();
  await expect(floating).toHaveAttribute('data-display-mode', 'interaction-expanded');
  expect(await floating.evaluate((region) => !region.contains(document.activeElement))).toBe(true);
  await page.clock.fastForward(5_000);
  await expect(floating).toHaveAttribute('data-display-mode', 'capsule');
});

test.describe('touch activation', () => {
  test.use({ hasTouch: true, viewport: { width: 340, height: 64 } });

  test('active capsule explicit activation expands without focus ownership', async ({ page }) => {
    await installActiveFloatingTauriMock(page);
    await page.goto('/?window=floating');

    const floating = page.getByRole('region', { name: 'StarToDo 悬浮窗' });
    await expect(floating).toHaveAttribute('data-display-mode', 'capsule');
    const expand = page.getByRole('button', { name: '展开悬浮窗' });
    const box = await expand.boundingBox();
    expect(box).not.toBeNull();
    await page.touchscreen.tap(box!.x + box!.width / 2, box!.y + box!.height / 2);
    await expect(floating).toHaveAttribute('data-display-mode', 'interaction-expanded');
  });
});

test('ownership-free activation collapses exactly at its deadline', async ({ page }) => {
  await page.clock.install({ time: new Date('2026-08-25T12:00:00Z') });
  await installActiveFloatingTauriMock(page);
  await page.setViewportSize({ width: 340, height: 64 });
  await page.goto('/?window=floating');

  const floating = page.getByRole('region', { name: 'StarToDo 悬浮窗' });
  await expect(floating).toHaveAttribute('data-display-mode', 'capsule');
  await page.clock.pauseAt(new Date('2026-08-25T12:00:10Z'));
  await page.getByRole('button', { name: '展开悬浮窗' }).evaluate((button: HTMLElement) => button.click());
  expect(await floating.getAttribute('data-display-mode')).toBe('interaction-expanded');
  expect(await floating.evaluate((region) => !region.contains(document.activeElement))).toBe(true);

  await page.clock.fastForward(4_999);
  expect(await floating.getAttribute('data-display-mode')).toBe('interaction-expanded');
  await page.clock.fastForward(1);
  expect(await floating.getAttribute('data-display-mode')).toBe('capsule');
});

test('floating route renders an expanded idle companion', async ({
  page
}) => {
  await page.setViewportSize({
    width: 360,
    height: 260
  });
  await page.goto('/?window=floating');

  const floating = page.getByRole('region', {
    name: 'StarToDo 悬浮窗'
  });

  await expect(floating).toHaveAttribute(
    'data-display-mode',
    'expanded'
  );
  await expect(
    page.getByRole('button', {
      name: '打开专注工作区'
    })
  ).toBeVisible();

  await expect
    .poll(() =>
      page.evaluate(
        () =>
          document.documentElement.scrollWidth <=
            window.innerWidth &&
          document.documentElement.scrollHeight <=
            window.innerHeight
      )
    )
    .toBe(true);

  const measurements = await page.evaluate(() => {
    const body = document.querySelector<HTMLElement>('.content-body');
    return {
      viewport: { width: window.innerWidth, height: window.innerHeight },
      document: {
        width: document.documentElement.scrollWidth,
        height: document.documentElement.scrollHeight
      },
      localBody: body ? {
        clientHeight: body.clientHeight,
        scrollHeight: body.scrollHeight,
        overflowY: getComputedStyle(body).overflowY
      } : null
    };
  });
  expect(measurements.localBody).not.toBeNull();
  expect(measurements.localBody?.overflowY).toBe('auto');
  console.log(`floating measurements 360x260 ${JSON.stringify(measurements)}`);
});

for (const viewport of [
  { width: 320, height: 720 },
  { width: 520, height: 420 },
  { width: 1180, height: 760 }
]) {
  test(`focus layout stays centered and resident at ${viewport.width}x${viewport.height}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openFocus(page);

    await expect(page.getByRole('button', { name: '返回任务' })).toBeVisible();
    await expect(page.getByRole('button', { name: '专注上下文' })).toBeVisible();
    await expect(page.getByRole('button', { name: '进入沉浸' })).toBeVisible();
    await expect(page.getByRole('button', { name: /开始/ })).toBeVisible();

    const measurements = await focusMeasurements(page);
    expect(measurements.document.width).toBeLessThanOrEqual(measurements.viewport.width);
    expect(measurements.document.height).toBeLessThanOrEqual(measurements.viewport.height);

    const canvasCenterX = measurements.canvas.x + measurements.canvas.width / 2;
    const canvasCenterY = measurements.canvas.y + measurements.canvas.height / 2;
    const timerCenterX = measurements.timer.x + measurements.timer.width / 2;
    const timerCenterY = measurements.timer.y + measurements.timer.height / 2;
    expect(Math.abs(timerCenterX - canvasCenterX)).toBeLessThanOrEqual(3);
    expect(Math.abs(timerCenterY - canvasCenterY)).toBeLessThanOrEqual(Math.max(90, measurements.canvas.height * 0.24));
    expect(measurements.stage.width).toBeLessThanOrEqual(measurements.canvas.width + 1);
    expect(measurements.stage.height).toBeLessThanOrEqual(measurements.canvas.height + 1);

    console.log(`focus measurements ${viewport.width}x${viewport.height} ${JSON.stringify(measurements)}`);
    await testInfo.attach('focus-measurements', {
      body: JSON.stringify(measurements),
      contentType: 'application/json'
    });
  });
}

test.describe('focus reduced motion', () => {
  test('focus layout remains compatible without breathing animation', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.setViewportSize({ width: 520, height: 420 });
    await openFocus(page);
    expect(await page.evaluate(() => matchMedia('(prefers-reduced-motion: reduce)').matches)).toBe(true);
    await page.locator('.focus-ring').evaluate((element) => {
      element.classList.add('running');
      element.setAttribute('data-force-running', 'true');
    });
    await expect(page.locator('.focus-ring')).toHaveClass(/running/);
    await expect(page.locator('.focus-ring > .focus-ring-core')).toHaveCSS('animation-name', 'none');
    const measurements = await focusMeasurements(page);
    expect(measurements.document.width).toBeLessThanOrEqual(measurements.viewport.width);
    expect(measurements.document.height).toBeLessThanOrEqual(measurements.viewport.height);
  });
});

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
