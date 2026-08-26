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

test('does not horizontally overflow at 320px wide', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 720 });
  await page.goto('/');

  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

  await page.getByRole('button', { name: '专注' }).click();
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
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


test('renders the adaptive shell and diagnostics drawer', async ({ page }) => {
  await page.goto('/');

  await expect(page.getByRole('main', { name: '任务场景' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: '主导航' })).toBeVisible();

  const trigger = page.getByRole('button', { name: '打开设置与诊断' });
  await trigger.click();

  await expect(page.getByRole('dialog', { name: '设置与诊断' })).toBeVisible();
  await page.keyboard.press('Escape');

  await expect(page.getByRole('dialog', { name: '设置与诊断' })).not.toBeAttached();
  await expect(trigger).toBeFocused();
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
