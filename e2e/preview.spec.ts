import { expect, test, type Page } from '@playwright/test';

const browserErrors = new WeakMap<Page, string[]>();

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

test('opens task tools as contextual drawers', async ({ page }) => {
  await page.goto('/');

  await expect(
    page.getByRole('heading', { name: '今天要推进什么？' })
  ).toBeVisible();

  await page.getByRole('button', { name: '项目' }).click();
  await expect(
    page.getByRole('dialog', { name: '项目' })
  ).toBeVisible();
  await page.keyboard.press('Escape');

  await page.getByRole('button', { name: '周计划' }).click();
  await expect(
    page.getByRole('dialog', { name: '周计划' })
  ).toBeVisible();

  await expect(
    page.getByRole('heading', { name: '今天要推进什么？' })
  ).toBeVisible();
});

test('keeps quick capture resident outside the detailed task drawer', async ({ page }) => {
  await page.goto('/');

  const taskScene = page.getByRole('main', { name: '任务场景' });
  const quickCapture = taskScene.getByRole('region', { name: '快速捕获' });
  await expect(quickCapture).toBeVisible();
  await expect(quickCapture.getByText('浏览器预览已禁用任务持久化；请在桌面应用中管理任务。', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: '详细新建' }).click();
  const drawer = page.getByRole('dialog', { name: '详细新建' });
  await expect(drawer).toBeVisible();
  await expect(quickCapture).toBeVisible();
  await expect(drawer.getByRole('heading', { name: '按行写下要推进的事' })).not.toBeAttached();
});

test('renders the browser preview page shell without runtime errors', async ({ page }) => {
  await page.goto('/');

  await expect(page.getByText('STAR TODO', { exact: true })).toBeVisible();
  await expect(page.getByRole('main', { name: '任务场景' })).toBeVisible();
  await expect(page.getByText('浏览器预览', { exact: true })).toBeVisible();
});

test('disables task persistence and does not expose task creation controls in browser preview', async ({ page }) => {
  await page.goto('/');

  await expect(page.getByText('浏览器预览已禁用任务持久化；请在桌面应用中管理任务。', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '解析预览' })).not.toBeAttached();
  await expect(page.getByRole('button', { name: '解析并添加' })).not.toBeAttached();
  await expect(page.getByRole('button', { name: '添加任务' })).not.toBeAttached();
});

test('navigates between tasks and focus with keyboard-reachable, visibly focused controls', async ({ page }) => {
  await page.goto('/');

  const navigation = page.getByRole('navigation', { name: '主导航' });
  const tasks = navigation.getByRole('button', { name: '任务' });
  const focus = navigation.getByRole('button', { name: '专注' });

  await expect(tasks).toBeVisible();
  await expect(focus).toBeVisible();
  await focus.focus();
  await expect(focus).toBeFocused();
  await expect(focus).toHaveCSS('outline-style', /^(?!none$).+/);
  await expect(focus).toHaveCSS('outline-width', /^(?!0px$).+/);
  await page.keyboard.press('Enter');

  await expect(page.getByRole('main', { name: '专注场景' })).toBeVisible();
  await expect(page.getByText('浏览器预览不具备桌面持久化计时和阶段通知能力；专注控制已禁用。', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: /开始/ })).toBeDisabled();

  await tasks.focus();
  await expect(tasks).toBeFocused();
  await expect(tasks).toHaveCSS('outline-style', /^(?!none$).+/);
  await expect(tasks).toHaveCSS('outline-width', /^(?!0px$).+/);
  await page.keyboard.press('Enter');
  await expect(page.getByRole('main', { name: '任务场景' })).toBeVisible();
});

test('preserves resident task state and focus across a focus round trip', async ({ page }) => {
  await page.goto('/');

  const navigation = page.getByRole('navigation', { name: '主导航' });
  const search = page.getByRole('textbox', { name: '搜索任务' });
  const uniqueSearch = 'resident-task-search-7f31';
  await search.fill(uniqueSearch);
  await search.focus();
  await expect(search).toBeFocused();

  await search.evaluate((element) => {
    const input = element as HTMLInputElement & { __sceneIdentity?: string };
    input.__sceneIdentity = 'original-task-search';
    input.scrollLeft = input.scrollWidth;
  });
  const originalScrollLeft = await search.evaluate((element) => element.scrollLeft);

  await navigation.getByRole('button', { name: '专注', exact: true }).click();
  await expect(page.getByRole('main', { name: '专注场景' })).toBeVisible();
  await page.getByRole('button', { name: '返回任务' }).click();

  await expect(page.getByRole('main', { name: '任务场景' })).toBeVisible();
  await expect(search).toHaveValue(uniqueSearch);
  await expect(search).toBeFocused();
  expect(await search.evaluate((element) =>
    (element as HTMLInputElement & { __sceneIdentity?: string }).__sceneIdentity
  )).toBe('original-task-search');
  expect(await search.evaluate((element) => element.scrollLeft)).toBe(originalScrollLeft);
});

test('shows a centered focus stage and exits visual immersive mode', async ({
  page
}) => {
  await page.goto('/');

  await page.getByRole('button', { name: '专注' }).click();
  await expect(page.getByRole('timer')).toBeVisible();

  await page.getByRole('button', {
    name: '进入沉浸'
  }).click();

  await expect(page.locator('.app-shell')).toHaveAttribute(
    'data-immersive',
    'visual-fallback'
  );

  await page.keyboard.press('Escape');

  await expect(page.locator('.app-shell')).toHaveAttribute(
    'data-immersive',
    'off'
  );
});

test('returns from browser visual immersive mode before showing tasks', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: '专注' }).click();
  await page.getByRole('button', { name: '进入沉浸' }).click();
  await expect(page.locator('.app-shell')).toHaveAttribute('data-immersive', 'visual-fallback');

  await page.getByRole('button', { name: '返回任务' }).click();

  await expect(page.locator('.app-shell')).toHaveAttribute('data-immersive', 'off');
  await expect(page.getByRole('main', { name: '任务场景' })).toBeVisible();
});

const viewportMatrix = [
  { width: 320, height: 720 },
  { width: 520, height: 420 },
  { width: 760, height: 560 },
  { width: 1180, height: 760 },
  { width: 1440, height: 900 }
];

for (const viewport of viewportMatrix) {
  test(`task and focus scenes fit ${viewport.width}x${viewport.height}`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await page.goto('/');

    const fitsViewport = () => page.evaluate(() => ({
      width: document.documentElement.scrollWidth <= window.innerWidth,
      height: document.documentElement.scrollHeight <= window.innerHeight
    }));

    await expect.poll(fitsViewport).toEqual({ width: true, height: true });
    await page.getByRole('button', { name: '专注' }).click();
    await expect(page.getByRole('timer')).toBeVisible();
    await expect.poll(fitsViewport).toEqual({ width: true, height: true });
  });
}

test('scrolling is confined to visible declared local regions at 520x420', async ({ page }) => {
  await page.setViewportSize({ width: 520, height: 420 });
  await page.goto('/');
  await page.getByRole('button', { name: '周计划' }).click();

  const planner = page.getByRole('dialog', { name: '周计划' });
  await expect(planner).toBeVisible();
  const localRegion = planner.getByRole('tabpanel');
  await expect(localRegion).toBeVisible();
  await expect(localRegion).toHaveCSS('overflow-y', /auto|scroll/);
  await expect.poll(() => page.evaluate(() => ({
    top: document.scrollingElement?.scrollTop ?? -1,
    left: document.scrollingElement?.scrollLeft ?? -1
  }))).toEqual({ top: 0, left: 0 });

  const visibleRegions = page.locator('[data-scroll-region]:visible');
  const visibleRegionCount = await visibleRegions.count();
  for (let index = 0; index < visibleRegionCount; index += 1) {
    await expect(visibleRegions.nth(index)).toHaveCSS('overflow-y', /auto|scroll/);
  }

  const localScroll = await localRegion.evaluate((region) => {
    const overflowContent = document.createElement('div');
    overflowContent.setAttribute('aria-hidden', 'true');
    overflowContent.style.height = `${region.clientHeight + 1}px`;
    overflowContent.style.flex = 'none';
    region.append(overflowContent);
    region.scrollTop = 1;
    const measurements = {
      clientHeight: region.clientHeight,
      scrollHeight: region.scrollHeight,
      scrollTop: region.scrollTop
    };
    overflowContent.remove();
    return measurements;
  });
  expect(localScroll.scrollHeight).toBeGreaterThan(localScroll.clientHeight);
  expect(localScroll.scrollTop).toBeGreaterThan(0);
  expect(await page.evaluate(() => document.scrollingElement?.scrollTop ?? -1)).toBe(0);
});

test('dismisses the current Pomodoro warning without swallowing a different warning', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('startodo.auto-immersive', 'disabled');
    const callbacks = new Map<number, (payload: unknown) => void>();
    let callbackId = 0;
    let warning = '通知主机不可用：warning A';
    const snapshot = {
      settings: { focusMinutes: 25, shortBreakMinutes: 5, longBreakMinutes: 15, longBreakInterval: 4, updatedAtUnixMs: Date.now() },
      currentSession: null,
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
        unregisterCallback(id: number) { callbacks.delete(id); },
        async invoke(command: string) {
          if (command === 'get_pomodoro_view') return { snapshot, taskSummaries: [] };
          if (command === 'get_pomodoro') return snapshot;
          if (command === 'start_pomodoro') {
            const currentWarning = warning;
            warning = '通知主机不可用：warning B';
            return { snapshot, notificationWarning: currentWarning };
          }
          if (command === 'list_tasks' || command === 'list_projects' || command === 'list_reminders') return [];
          if (command === 'list_deleted_tasks' || command === 'list_missed_reminders' || command === 'list_reliability_incidents') return [];
          if (command === 'get_window_preferences') return { maximized: true, normalBounds: { x: null, y: null, width: 960, height: 680 }, alwaysOnTop: false, autoImmersive: 'ask', lastImmersive: false };
          if (command === 'get_window_state') return { maximized: true, fullscreen: false, alwaysOnTop: false, normalBounds: { x: null, y: null, width: 960, height: 680 } };
          if (command === 'claim_pending_pomodoro_activations' || command === 'claim_pending_activations') return { claimId: null, items: [] };
          if (command === 'reconcile_reminders') return { warning: null };
          if (command === 'take_pending_floating_intent') return null;
          if (command === 'plugin:event|listen') return 1;
          if (command === 'plugin:event|unlisten' || command === 'record_ui_ready' || command === 'record_ui_not_ready') return null;
          console.error(`Unexpected mocked Tauri command: ${command}`);
          throw new Error(`Unexpected mocked Tauri command: ${command}`);
        }
      }
    });
  });
  await page.goto('/');
  await page.getByRole('navigation', { name: '主导航' }).getByRole('button', { name: '专注', exact: true }).click();
  const start = page.getByRole('button', { name: '开始专注' });

  await start.click();
  const warningA = page.getByLabel('通知').getByRole('alert').filter({ hasText: 'warning A' });
  await expect(warningA).toBeVisible();
  await warningA.getByRole('button', { name: '关闭通知' }).click();
  await expect(warningA).not.toBeAttached();
  await expect(page.getByLabel('通知').getByText('warning A')).not.toBeAttached();

  await start.click();
  await expect(page.getByLabel('通知').getByRole('alert').filter({ hasText: 'warning B' })).toBeVisible();
});

test('shows an explicit diagnostics error when a Tauri-only action is requested', async ({ page }) => {
  await page.goto('/');

  const trigger = page.getByRole('button', { name: '打开设置与诊断' });
  await trigger.click();
  await expect(page.getByRole('dialog', { name: '设置与诊断' })).toBeVisible();
  await page.getByRole('button', { name: '刷新运行时快照' }).click();

  await expect(page.getByRole('alert')).toHaveText('当前在浏览器预览环境，Tauri 后端不可用。');
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: '设置与诊断' })).not.toBeAttached();
  await expect(trigger).toBeFocused();
});


test.describe('reduced motion', () => {
  test('keeps drawer and scene operations keyboard usable', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/');
    expect(await page.evaluate(() => matchMedia('(prefers-reduced-motion: reduce)').matches)).toBe(true);

    await expect(page.getByRole('main', { name: '任务场景' })).toBeVisible();
    await expect(page.getByRole('navigation', { name: '主导航' })).toBeVisible();

    const trigger = page.getByRole('button', { name: '打开设置与诊断' });
    await trigger.focus();
    await expect(trigger).toBeFocused();
    await trigger.click();
    await expect(page.getByRole('dialog', { name: '设置与诊断' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.getByRole('dialog', { name: '设置与诊断' })).not.toBeAttached();
    await expect(trigger).toBeFocused();

    await page.getByRole('button', { name: '专注' }).click();
    await expect(page.getByRole('main', { name: '专注场景' })).toBeVisible();
    await expect(page.getByRole('timer')).toBeVisible();
  });
});

test('keeps the document inside the viewport', async ({ page }) => {
  await page.setViewportSize({ width: 760, height: 560 });
  await page.goto('/');

  await expect.poll(() => page.evaluate(() => ({
    width: document.documentElement.scrollWidth <= window.innerWidth,
    height: document.documentElement.scrollHeight <= window.innerHeight
  }))).toEqual({ width: true, height: true });
});

test('keeps the primary workspace controls keyboard reachable with visible focus', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('浏览器预览已禁用任务持久化；请在桌面应用中管理任务。', { exact: true })).toBeVisible();

  const search = page.getByRole('textbox', { name: '搜索任务' });
  const planner = page.getByRole('button', { name: '周计划' });
  const filters = page.getByRole('button', { name: '筛选' });

  await search.focus();
  await expect(search).toBeFocused();
  await expect(search).toHaveCSS('outline-style', /^(?!none$).+/);
  await expect(search).toHaveCSS('outline-width', /^(?!0px$).+/);

  await filters.focus();
  await expect(filters).toBeFocused();
  await expect(filters).toHaveCSS('outline-style', /^(?!none$).+/);
  await expect(filters).toHaveCSS('outline-width', /^(?!0px$).+/);

  await planner.focus();
  await expect(planner).toBeFocused();
  await expect(planner).toHaveCSS('outline-style', /^(?!none$).+/);
  await expect(planner).toHaveCSS('outline-width', /^(?!0px$).+/);
});
