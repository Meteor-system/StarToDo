# StarToDo Adaptive Focus Canvas Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.


**Goal:** 移除旧的 compact/full 主窗口模式，将 StarToDo 改造成无页面级滚动的自适应任务/专注画布，支持可恢复的沉浸式专注、上下文抽屉和呼吸式双态悬浮窗，同时保持现有业务与可靠性语义不变。

**Architecture:** Rust、SQLite 与现有 Tauri 命令继续作为任务、项目、提醒、通知激活和 Pomodoro 状态的权威来源；src/routes/+page.svelte 继续协调生命周期和事件。纯 TypeScript 模块负责本地 UI 偏好与悬浮窗展示状态机；AppShell 负责视口和场景框架；TaskScene、FocusScene 负责场景组合；FloatingWindow 保留数据监听职责，并把展示委托给胶囊与展开面板。桌面端使用明确的自适应、沉浸和悬浮窗尺寸命令替代通用 compact/full 模式。

**Tech Stack:** SvelteKit、Svelte 5 runes、TypeScript 5.6、Vite 6、Vitest、Playwright、Tauri 2、Rust、serde、rusqlite、SQLite、Windows .NET 8 notification sidecar。

**Spec:** docs/superpowers/specs/2026-08-25-adaptive-focus-canvas-design.md

## Global Constraints
本计划只交付已批准的 Phase 1：自适应窗口迁移、任务/专注场景、上下文抽屉、100dvh 零页面级滚动、窄屏周计划、沉浸偏好和呼吸式悬浮窗。
Phase 2 的今日星图、拖拽排期和增强型完成动效，以及 Phase 3 的批量命令和高密度工作流，不在本计划中实施。
不修改 SQLite schema，不重写任务或 Pomodoro 后端业务规则。
必须保留：
task operation tokens 与 stale-response discard；
reminder warning leases、claim、acknowledgement；
Pomodoro SQLite authority、刷新序列与通知 warning；
pending task/Pomodoro notification activations；
pending floating intents；
recurring task next-instance 与 reminder warning 处理；
完成任务后的相邻任务键盘焦点恢复。
html、body、Svelte 根节点和应用 shell 必须固定在 100dvh 内并设置 overflow: hidden。
只允许任务列表、planner body 和 drawer body 局部滚动。
桌面主窗口最小尺寸为 520 x 420；浏览器预览仍需在 320px 宽度下可用且无横向页面溢出。
宽度层级：>=1180、760-1179、520-759、<520。
高度层级：>=760、560-759、420-559、浏览器预览专用 <420。
Orange 表示 active focus/primary action；blue 表示 navigation/selection/info；green 表示 completion/success；amber 表示 warning；red 表示 destructive/error。
星形仅用于品牌、专注节点和轻量完成反馈，不引入积分、等级或排行榜。
所有 transition/animation 必须支持 prefers-reduced-motion。
动效不得延迟或决定业务命令是否执行。
键盘焦点必须在任务完成、抽屉关闭、场景切换和悬浮窗展开/收起后保持可预测。
保留 Windows #[cfg] 边界，Rust 变更运行 rustfmt。
不提交 build/、src-tauri/target/、notification sidecar 的 bin/、obj/、publish/ 或生成的安装包。
执行时保留工作树中所有与本功能无关的用户改动，只暂存每个任务明确列出的文件。
本计划采用一个依赖有序的 Phase 1 计划，因为窗口迁移、shell、场景和悬浮窗共享状态与验收条件；Phase 2 和 Phase 3 应分别编写后续计划。

---

## File Structure
新建纯逻辑文件
src/lib/ui-preferences.ts
自动沉浸与悬浮窗始终展开偏好的类型化读写。
src/lib/ui-preferences.test.ts
缺失值、非法值、合法值和存储失败测试。
src/lib/floating-display.ts
呼吸式悬浮窗纯状态机。
src/lib/floating-display.test.ts
胶囊、交互展开、停留和超时测试。
src/lib/windowing.ts
自适应主窗口、沉浸模式和悬浮窗 Tauri command wrappers。
src/lib/ui-state.ts
AppScene、ShellDrawer、DrawerSize、ImmersiveDisplayState、toast 类型。
src/lib/task-interaction.ts
任务行键盘完成条件。
src/lib/task-interaction.test.ts
Space、modifier 和嵌套控件事件测试。
src/lib/planner-selection.ts
窄屏 planner 当前日期/未排期选择归一化。
src/lib/planner-selection.test.ts
本周、跨周和未排期选择测试。
新建组件
src/lib/components/AppShell.svelte
全视口应用 shell、场景导航、状态、诊断抽屉和 toast。
src/lib/components/ContextDrawer.svelte
可复用的 focus-managed drawer。
src/lib/components/ToastStack.svelte
不占布局空间的消息层。
src/lib/components/TaskCanvas.svelte
局部滚动的任务分组。
src/lib/components/TaskDetailsDrawer.svelte
任务字段编辑和普通删除确认。
src/lib/components/FocusStage.svelte
居中的专注阶段、进度环、计时器和控制。
src/lib/components/AutoImmersivePrompt.svelte
首次开始专注时的自动沉浸选择。
src/lib/components/FloatingCapsule.svelte
专注进行中的最小胶囊。
src/lib/components/FloatingExpandedPanel.svelte
空闲或交互展开状态。
e2e/responsive.spec.ts
viewport matrix、页面滚动、planner、drawer、immersive、reduced-motion 测试。
docs/testing/adaptive-focus-canvas-windows-smoke.md
Windows 桌面 smoke test 证据。
重命名
src/lib/components/TaskWorkspace.svelte → src/lib/components/TaskScene.svelte
src/lib/components/FocusWorkspace.svelte → src/lib/components/FocusScene.svelte
src/lib/components/DiagnosticsPanel.svelte → src/lib/components/DiagnosticsDrawer.svelte
修改
src/routes/+page.svelte
src/app.css
src/lib/tasks.ts
src/lib/components/TaskItem.svelte
src/lib/components/TaskComposer.svelte
src/lib/components/ProjectSidebar.svelte
src/lib/components/WeekPlanner.svelte
src/lib/components/FocusMiniBar.svelte
src/lib/components/FloatingWindow.svelte
src-tauri/src/lib.rs
src-tauri/tauri.conf.json
e2e/preview.spec.ts
### Task 1: 类型化 UI 偏好
Files:

Create: src/lib/ui-preferences.ts
Create: src/lib/ui-preferences.test.ts
Consume unchanged: src/lib/preferences.ts:1-end
Interfaces:

Consumes:
readPreference(key: string): string | null
writePreference(key: string, value: string): void
Produces:
ts
复制
export type AutoImmersivePreference = 'unset' | 'enabled' | 'disabled';
export type FloatingExpansionPreference = 'auto' | 'always';

export interface UiPreferences {
  autoImmersive: AutoImmersivePreference;
  floatingExpansion: FloatingExpansionPreference;
}

export const DEFAULT_UI_PREFERENCES: UiPreferences;

export function readUiPreferences(): UiPreferences;
export function writeAutoImmersivePreference(
  value: AutoImmersivePreference
): void;
export function writeFloatingExpansionPreference(
  value: FloatingExpansionPreference
): void;
 Step 1: 写 failing tests
创建 src/lib/ui-preferences.test.ts：

ts
复制
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  DEFAULT_UI_PREFERENCES,
  readUiPreferences,
  writeAutoImmersivePreference,
  writeFloatingExpansionPreference
} from './ui-preferences';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('readUiPreferences', () => {
  it('uses defaults when storage is unavailable', () => {
    vi.stubGlobal('window', undefined);

    expect(readUiPreferences()).toEqual(DEFAULT_UI_PREFERENCES);
  });

  it('uses defaults for absent or invalid values', () => {
    const getItem = vi.fn((key: string) =>
      key === 'startodo.auto-immersive' ? 'sometimes' : 'wide'
    );
    vi.stubGlobal('window', {
      localStorage: { getItem, setItem: vi.fn() }
    });

    expect(readUiPreferences()).toEqual({
      autoImmersive: 'unset',
      floatingExpansion: 'auto'
    });
  });

  it('reads valid values', () => {
    const values = new Map<string, string>([
      ['startodo.auto-immersive', 'enabled'],
      ['startodo.floating-expansion', 'always']
    ]);

    vi.stubGlobal('window', {
      localStorage: {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: vi.fn()
      }
    });

    expect(readUiPreferences()).toEqual({
      autoImmersive: 'enabled',
      floatingExpansion: 'always'
    });
  });
});

describe('UI preference writes', () => {
  it('writes the exact serialized values', () => {
    const setItem = vi.fn();
    vi.stubGlobal('window', {
      localStorage: { getItem: vi.fn(), setItem }
    });

    writeAutoImmersivePreference('disabled');
    writeFloatingExpansionPreference('auto');

    expect(setItem).toHaveBeenCalledWith(
      'startodo.auto-immersive',
      'disabled'
    );
    expect(setItem).toHaveBeenCalledWith(
      'startodo.floating-expansion',
      'auto'
    );
  });

  it('does not throw when localStorage rejects writes', () => {
    const setItem = vi.fn(() => {
      throw new DOMException('Quota exceeded', 'QuotaExceededError');
    });
    vi.stubGlobal('window', {
      localStorage: { getItem: vi.fn(), setItem }
    });

    expect(() => writeAutoImmersivePreference('enabled')).not.toThrow();
    expect(() => writeFloatingExpansionPreference('always')).not.toThrow();
  });
});
 Step 2: 运行测试并确认预期失败
Run:

powershell
复制
npm run test:unit -- src/lib/ui-preferences.test.ts
Expected: FAIL，错误指出无法解析 ./ui-preferences。

 Step 3: 实现类型化偏好模块
创建 src/lib/ui-preferences.ts：

ts
复制
import { readPreference, writePreference } from './preferences';

export type AutoImmersivePreference = 'unset' | 'enabled' | 'disabled';
export type FloatingExpansionPreference = 'auto' | 'always';

export interface UiPreferences {
  autoImmersive: AutoImmersivePreference;
  floatingExpansion: FloatingExpansionPreference;
}

const AUTO_IMMERSIVE_KEY = 'startodo.auto-immersive';
const FLOATING_EXPANSION_KEY = 'startodo.floating-expansion';

export const DEFAULT_UI_PREFERENCES: UiPreferences = {
  autoImmersive: 'unset',
  floatingExpansion: 'auto'
};

function oneOf<T extends string>(
  value: string | null,
  allowed: readonly T[],
  fallback: T
): T {
  return value !== null && allowed.includes(value as T)
    ? (value as T)
    : fallback;
}

export function readUiPreferences(): UiPreferences {
  return {
    autoImmersive: oneOf(
      readPreference(AUTO_IMMERSIVE_KEY),
      ['unset', 'enabled', 'disabled'] as const,
      DEFAULT_UI_PREFERENCES.autoImmersive
    ),
    floatingExpansion: oneOf(
      readPreference(FLOATING_EXPANSION_KEY),
      ['auto', 'always'] as const,
      DEFAULT_UI_PREFERENCES.floatingExpansion
    )
  };
}

export function writeAutoImmersivePreference(
  value: AutoImmersivePreference
): void {
  writePreference(AUTO_IMMERSIVE_KEY, value);
}

export function writeFloatingExpansionPreference(
  value: FloatingExpansionPreference
): void {
  writePreference(FLOATING_EXPANSION_KEY, value);
}
 Step 4: 验证通过
Run:

powershell
复制
npm run test:unit -- src/lib/ui-preferences.test.ts
npm run check
Expected: 两条命令均 PASS，无 TypeScript/Svelte diagnostics。

 Step 5: 提交
powershell
复制
git add src/lib/ui-preferences.ts
git add src/lib/ui-preferences.test.ts
git commit -m "Add adaptive UI preferences"
### Task 2: 呼吸式悬浮窗纯状态机
Files:

Create: src/lib/floating-display.ts
Create: src/lib/floating-display.test.ts
Interfaces:

Consumes: 组件提供的 epoch milliseconds。
Produces:
ts
复制
export type FloatingDisplayMode =
  | 'expanded'
  | 'capsule'
  | 'interaction-expanded';

export type FloatingSizeMode = 'expanded' | 'capsule';

export interface FloatingDisplayState {
  mode: FloatingDisplayMode;
  focusActive: boolean;
  alwaysExpanded: boolean;
  pointerInside: boolean;
  focusInside: boolean;
  lastInteractionAt: number | null;
  collapseAt: number | null;
}

export type FloatingDisplayEvent =
  | { type: 'snapshot'; focusActive: boolean; at: number }
  | { type: 'always-expanded'; value: boolean; at: number }
  | {
      type:
        | 'pointer-enter'
        | 'pointer-leave'
        | 'focus-in'
        | 'focus-out'
        | 'timeout';
      at: number;
    };

export const FLOATING_INTERACTION_MS: 5000;
export const FLOATING_LEAVE_BUFFER_MS: 700;

export function createFloatingDisplayState(
  focusActive: boolean,
  alwaysExpanded: boolean
): FloatingDisplayState;

export function reduceFloatingDisplay(
  state: FloatingDisplayState,
  event: FloatingDisplayEvent
): FloatingDisplayState;

export function floatingSizeMode(
  mode: FloatingDisplayMode
): FloatingSizeMode;
 Step 1: 写 failing reducer tests
创建 src/lib/floating-display.test.ts：

ts
复制
import { describe, expect, it } from 'vitest';

import {
  FLOATING_INTERACTION_MS,
  FLOATING_LEAVE_BUFFER_MS,
  createFloatingDisplayState,
  floatingSizeMode,
  reduceFloatingDisplay
} from './floating-display';

describe('floating display reducer', () => {
  it('uses expanded while idle and capsule while focus is active', () => {
    const idle = createFloatingDisplayState(false, false);
    expect(idle.mode).toBe('expanded');

    const active = reduceFloatingDisplay(idle, {
      type: 'snapshot',
      focusActive: true,
      at: 100
    });
    expect(active.mode).toBe('capsule');
  });

  it('keeps interaction expansion for five seconds', () => {
    let state = createFloatingDisplayState(true, false);
    state = reduceFloatingDisplay(state, {
      type: 'pointer-enter',
      at: 100
    });
    state = reduceFloatingDisplay(state, {
      type: 'pointer-leave',
      at: 200
    });

    expect(state.mode).toBe('interaction-expanded');
    expect(state.collapseAt).toBe(100 + FLOATING_INTERACTION_MS);

    state = reduceFloatingDisplay(state, {
      type: 'timeout',
      at: 100 + FLOATING_INTERACTION_MS - 1
    });
    expect(state.mode).toBe('interaction-expanded');

    state = reduceFloatingDisplay(state, {
      type: 'timeout',
      at: 100 + FLOATING_INTERACTION_MS
    });
    expect(state.mode).toBe('capsule');
  });

  it('adds a leave buffer after a long pointer stay', () => {
    let state = createFloatingDisplayState(true, false);
    state = reduceFloatingDisplay(state, {
      type: 'pointer-enter',
      at: 100
    });
    state = reduceFloatingDisplay(state, {
      type: 'pointer-leave',
      at: 6_000
    });

    expect(state.collapseAt).toBe(
      6_000 + FLOATING_LEAVE_BUFFER_MS
    );
  });

  it('does not collapse while keyboard focus remains inside', () => {
    let state = createFloatingDisplayState(true, false);
    state = reduceFloatingDisplay(state, {
      type: 'focus-in',
      at: 100
    });
    state = reduceFloatingDisplay(state, {
      type: 'pointer-leave',
      at: 200
    });
    state = reduceFloatingDisplay(state, {
      type: 'timeout',
      at: 100_000
    });

    expect(state.mode).toBe('interaction-expanded');
    expect(state.collapseAt).toBeNull();
  });

  it('keeps always-expanded mode independent of focus state', () => {
    let state = createFloatingDisplayState(true, true);
    expect(state.mode).toBe('expanded');

    state = reduceFloatingDisplay(state, {
      type: 'snapshot',
      focusActive: true,
      at: 100
    });
    expect(state.mode).toBe('expanded');
  });

  it('maps interaction-expanded to the expanded outer size', () => {
    expect(floatingSizeMode('capsule')).toBe('capsule');
    expect(floatingSizeMode('expanded')).toBe('expanded');
    expect(floatingSizeMode('interaction-expanded')).toBe('expanded');
  });
});
 Step 2: 运行测试并确认预期失败
Run:

powershell
复制
npm run test:unit -- src/lib/floating-display.test.ts
Expected: FAIL，模块 ./floating-display 不存在。

 Step 3: 实现 reducer 基础类型和初始状态
在 src/lib/floating-display.ts 中定义上述类型和：

ts
复制
export const FLOATING_INTERACTION_MS = 5_000;
export const FLOATING_LEAVE_BUFFER_MS = 700;

export function createFloatingDisplayState(
  focusActive: boolean,
  alwaysExpanded: boolean
): FloatingDisplayState {
  return {
    mode: alwaysExpanded || !focusActive ? 'expanded' : 'capsule',
    focusActive,
    alwaysExpanded,
    pointerInside: false,
    focusInside: false,
    lastInteractionAt: null,
    collapseAt: null
  };
}
 Step 4: 实现 interaction deadline
加入：

ts
复制
function armCollapse(
  state: FloatingDisplayState,
  at: number
): FloatingDisplayState {
  if (state.alwaysExpanded || !state.focusActive) {
    return { ...state, mode: 'expanded', collapseAt: null };
  }

  if (state.pointerInside || state.focusInside) {
    return {
      ...state,
      mode: 'interaction-expanded',
      collapseAt: null
    };
  }

  if (state.mode !== 'interaction-expanded') {
    return { ...state, mode: 'capsule', collapseAt: null };
  }

  const interactionDeadline =
    (state.lastInteractionAt ?? at) + FLOATING_INTERACTION_MS;

  return {
    ...state,
    collapseAt: Math.max(
      interactionDeadline,
      at + FLOATING_LEAVE_BUFFER_MS
    )
  };
}
 Step 5: 实现所有 reducer 事件
ts
复制
export function reduceFloatingDisplay(
  state: FloatingDisplayState,
  event: FloatingDisplayEvent
): FloatingDisplayState {
  if (event.type === 'snapshot') {
    const next = {
      ...state,
      focusActive: event.focusActive,
      collapseAt: null
    };

    if (next.alwaysExpanded || !next.focusActive) {
      return { ...next, mode: 'expanded' };
    }

    return {
      ...next,
      mode:
        next.pointerInside || next.focusInside
          ? 'interaction-expanded'
          : 'capsule'
    };
  }

  if (event.type === 'always-expanded') {
    const next = {
      ...state,
      alwaysExpanded: event.value,
      collapseAt: null
    };

    if (event.value || !next.focusActive) {
      return { ...next, mode: 'expanded' };
    }

    return {
      ...next,
      mode:
        next.pointerInside || next.focusInside
          ? 'interaction-expanded'
          : 'capsule'
    };
  }

  if (event.type === 'pointer-enter' || event.type === 'focus-in') {
    const next = {
      ...state,
      pointerInside:
        event.type === 'pointer-enter'
          ? true
          : state.pointerInside,
      focusInside:
        event.type === 'focus-in'
          ? true
          : state.focusInside,
      lastInteractionAt: event.at,
      collapseAt: null
    };

    return {
      ...next,
      mode:
        next.alwaysExpanded || !next.focusActive
          ? 'expanded'
          : 'interaction-expanded'
    };
  }

  if (event.type === 'pointer-leave' || event.type === 'focus-out') {
    return armCollapse(
      {
        ...state,
        pointerInside:
          event.type === 'pointer-leave'
            ? false
            : state.pointerInside,
        focusInside:
          event.type === 'focus-out'
            ? false
            : state.focusInside
      },
      event.at
    );
  }

  if (
    state.collapseAt === null ||
    event.at < state.collapseAt ||
    state.pointerInside ||
    state.focusInside
  ) {
    return state;
  }

  return {
    ...state,
    mode:
      state.alwaysExpanded || !state.focusActive
        ? 'expanded'
        : 'capsule',
    collapseAt: null
  };
}

export function floatingSizeMode(
  mode: FloatingDisplayMode
): FloatingSizeMode {
  return mode === 'capsule' ? 'capsule' : 'expanded';
}
 Step 6: 验证
Run:

powershell
复制
npm run test:unit -- src/lib/floating-display.test.ts
npm run check
Expected: PASS。

 Step 7: 提交
powershell
复制
git add src/lib/floating-display.ts
git add src/lib/floating-display.test.ts
git commit -m "Add floating window display state machine"
### Task 3: Rust 主窗口偏好迁移到 adaptive
Files:

Modify: src-tauri/src/lib.rs:212-230
Modify: src-tauri/src/lib.rs:1222-1282
Modify: src-tauri/src/lib.rs:1445-1485
Modify: src-tauri/src/lib.rs:3019-3071
Modify: src-tauri/src/lib.rs:3198-end
Modify: src-tauri/tauri.conf.json
Interfaces:

Consumes legacy disk JSON:
json
复制
{
  "mode": "full",
  "width": 800,
  "height": 600,
  "alwaysOnTop": false
}
Produces:
rust
复制
enum WindowLayout {
    Adaptive,
}

struct WindowBounds {
    x: Option<i32>,
    y: Option<i32>,
    width: u32,
    height: u32,
}

struct WindowPreferences {
    layout: WindowLayout,
    maximized: bool,
    normal_bounds: WindowBounds,
    always_on_top: bool,
    last_immersive: bool,
}

struct DecodedWindowPreferences {
    preferences: WindowPreferences,
    migrated: bool,
}

fn decode_window_preferences(
    content: &str,
) -> Result<DecodedWindowPreferences, String>;
 Step 1: 添加 legacy migration failing tests
在 src-tauri/src/lib.rs 的 tests module 中添加：

rust
复制
#[test]
fn compact_window_preferences_migrate_to_adaptive() {
    let decoded = decode_window_preferences(
        r#"{"mode":"compact","width":380,"height":520,"alwaysOnTop":true}"#,
    )
    .expect("legacy compact preferences should migrate");

    assert!(decoded.migrated);
    assert_eq!(decoded.preferences.layout, WindowLayout::Adaptive);
    assert!(!decoded.preferences.maximized);
    assert_eq!(decoded.preferences.normal_bounds.width, 520);
    assert_eq!(decoded.preferences.normal_bounds.height, 520);
    assert!(decoded.preferences.always_on_top);
}

#[test]
fn full_window_preferences_migrate_to_maximized_adaptive() {
    let decoded = decode_window_preferences(
        r#"{"mode":"full","width":800,"height":600,"alwaysOnTop":false}"#,
    )
    .expect("legacy full preferences should migrate");

    assert!(decoded.migrated);
    assert_eq!(decoded.preferences.layout, WindowLayout::Adaptive);
    assert!(decoded.preferences.maximized);
    assert_eq!(decoded.preferences.normal_bounds.width, 800);
    assert_eq!(decoded.preferences.normal_bounds.height, 600);
}

#[test]
fn adaptive_window_preferences_reject_too_small_bounds() {
    let preferences = WindowPreferences {
        normal_bounds: WindowBounds {
            width: 519,
            height: 420,
            ..WindowBounds::default()
        },
        ..WindowPreferences::default()
    };

    assert_eq!(
        validate_window_preferences(&preferences).unwrap_err(),
        "window width must be between 520 and 7680"
    );
}
 Step 2: 运行测试并确认编译失败
Run:

powershell
复制
cargo test --manifest-path src-tauri/Cargo.toml window_preferences -- --nocapture
Expected: FAIL，缺少 WindowLayout、WindowBounds 或 decode_window_preferences。

 Step 3: 定义 adaptive preference model
用以下模型替换旧 WindowPreferences：

rust
复制
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum WindowLayout {
    Adaptive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowBounds {
    x: Option<i32>,
    y: Option<i32>,
    width: u32,
    height: u32,
}

impl Default for WindowBounds {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            width: 960,
            height: 680,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowPreferences {
    layout: WindowLayout,
    maximized: bool,
    normal_bounds: WindowBounds,
    always_on_top: bool,
    last_immersive: bool,
}

impl Default for WindowPreferences {
    fn default() -> Self {
        Self {
            layout: WindowLayout::Adaptive,
            maximized: true,
            normal_bounds: WindowBounds::default(),
            always_on_top: false,
            last_immersive: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyWindowPreferences {
    mode: String,
    width: u32,
    height: u32,
    always_on_top: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StoredWindowPreferences {
    Current(WindowPreferences),
    Legacy(LegacyWindowPreferences),
}

struct DecodedWindowPreferences {
    preferences: WindowPreferences,
    migrated: bool,
}
 Step 4: 实现 decode 与 validation
decode_window_preferences() 必须：

当前格式：验证并返回 migrated=false。
legacy full：maximized=true。
legacy compact：maximized=false。
legacy width clamp 到 520..=7680。
legacy height clamp 到 420..=4320。
其他 legacy mode 返回明确错误。
last_immersive=false。
不读取或修改 SQLite。
validate_window_preferences() 必须验证：

rust
复制
if !(520..=7_680).contains(&preferences.normal_bounds.width) {
    return Err("window width must be between 520 and 7680".to_string());
}
if !(420..=4_320).contains(&preferences.normal_bounds.height) {
    return Err("window height must be between 420 and 4320".to_string());
}
for coordinate in [
    preferences.normal_bounds.x,
    preferences.normal_bounds.y,
]
.into_iter()
.flatten()
{
    if !(-32_768..=32_767).contains(&coordinate) {
        return Err(
            "window coordinates must be between -32768 and 32767".to_string(),
        );
    }
}
 Step 5: 在读取时持久化 migration
read_window_preferences() 使用 decode_window_preferences()。若 migrated=true，通过现有 save_window_preferences_to_disk() 将新格式写回；写回失败只记录 eprintln!，仍返回已迁移的内存值。

缺失、非法或验证失败的文件返回 WindowPreferences::default()。

 Step 6: 应用 adaptive geometry
将 apply_window_preferences() 改为：

rust
复制
fn apply_window_preferences(
    window: &WebviewWindow,
    preferences: &WindowPreferences,
) -> Result<(), String> {
    validate_window_preferences(preferences)?;

    window
        .set_min_size(Some(LogicalSize::new(520.0, 420.0)))
        .map_err(string_error)?;
    window.set_resizable(true).map_err(string_error)?;
    window
        .set_always_on_top(preferences.always_on_top)
        .map_err(string_error)?;
    window.set_fullscreen(false).map_err(string_error)?;

    if preferences.maximized {
        window.maximize().map_err(string_error)?;
    } else {
        window.unmaximize().map_err(string_error)?;
        window
            .set_size(LogicalSize::new(
                preferences.normal_bounds.width,
                preferences.normal_bounds.height,
            ))
            .map_err(string_error)?;

        if let (Some(x), Some(y)) = (
            preferences.normal_bounds.x,
            preferences.normal_bounds.y,
        ) {
            window
                .set_position(LogicalPosition::new(x as f64, y as f64))
                .map_err(string_error)?;
        }
    }

    Ok(())
}
 Step 7: 添加主窗口 geometry tracking
新增：

rust
复制
fn install_main_window_tracking(
    app: &AppHandle,
    window: &WebviewWindow,
);
监听 WindowEvent::Moved 和 WindowEvent::Resized：

is_fullscreen()==true 时直接返回。

始终更新 preferences.maximized。

只有非 maximized 时读取 outer_position() 与 inner_size()，按 scale_factor() 转换为 logical values，更新 normal_bounds。

使用 save_window_preferences_to_disk()。

不更改 reminder listener、UI release 或 close-to-tray 行为。

 Step 8: 移除 compact backend 与 tray entry

删除：

set_window_mode_internal()
set_window_mode 的 full/compact 处理
tray Compact mode
tray menu 改为：

rust
复制
let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
let focus = MenuItem::with_id(app, "focus", "Focus", true, None::<&str>)?;
let floating =
    MenuItem::with_id(app, "floating", "悬浮窗", true, None::<&str>)?;
let hide = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
let release_ui =
    MenuItem::with_id(app, "release_ui", "Release UI", true, None::<&str>)?;
let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

let menu = Menu::with_items(
    app,
    &[&show, &focus, &floating, &hide, &release_ui, &quit],
)?;
focus handler：

将现有 pending_floating_intent 设为 { view: "focus", task_id: None }。

调用 show_or_create_main_window(app)。

发出 floating-intent-available。

不直接修改 Pomodoro 状态。

 Step 9: 更新 Tauri window config

src-tauri/tauri.conf.json 主窗口使用：

json
复制
{
  "title": "StarToDo",
  "visible": false,
  "width": 960,
  "height": 680,
  "minWidth": 520,
  "minHeight": 420,
  "resizable": true,
  "maximized": true
}
 Step 10: 验证
Run:

powershell
复制
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml window_preferences -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml
Expected: 全部 PASS。

 Step 11: 提交
powershell
复制
git add src-tauri/src/lib.rs
git add src-tauri/tauri.conf.json
git commit -m "Migrate main window to adaptive layout"
### Task 4: 明确的沉浸命令与 windowing.ts
Files:

Modify: src-tauri/src/lib.rs:80-93
Modify: src-tauri/src/lib.rs:2898-2954
Modify: src-tauri/src/lib.rs:3074-3193
Modify: src-tauri/src/lib.rs:3198-end
Create: src/lib/windowing.ts
Modify: src/lib/tasks.ts:147-159
Modify: src/lib/tasks.ts:441-445
Interfaces:

Consumes:
Task 3 WindowPreferences
Task 3 WindowBounds
Produces:
rust
复制
struct WindowState {
    maximized: bool,
    fullscreen: bool,
    normal_bounds: WindowBounds,
}

struct ImmersiveRestoreState {
    maximized: bool,
    normal_bounds: WindowBounds,
}

fn get_window_state(app: AppHandle) -> Result<WindowState, String>;
fn enter_immersive_mode(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WindowState, String>;
fn exit_immersive_mode(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WindowState, String>;
fn set_main_window_maximized(
    app: AppHandle,
    maximized: bool,
) -> Result<WindowState, String>;
TypeScript:

ts
复制
export type WindowLayout = 'adaptive';

export interface WindowBounds {
  x: number | null;
  y: number | null;
  width: number;
  height: number;
}

export interface WindowPreferences {
  layout: WindowLayout;
  maximized: boolean;
  normalBounds: WindowBounds;
  alwaysOnTop: boolean;
  lastImmersive: boolean;
}

export interface WindowState {
  maximized: boolean;
  fullscreen: boolean;
  normalBounds: WindowBounds;
}

export interface FloatingWindowPreferences {
  visible: boolean;
  x: number | null;
  y: number | null;
  width: number;
  height: number;
  alwaysOnTop: boolean;
}
 Step 1: 写 immersive restore failing tests
rust
复制
#[test]
fn immersive_restore_prefers_maximized() {
    let restore = ImmersiveRestoreState {
        maximized: true,
        normal_bounds: WindowBounds::default(),
    };

    assert_eq!(
        restore.restore_target(),
        WindowRestoreTarget::Maximized
    );
}

#[test]
fn immersive_restore_uses_normal_bounds() {
    let bounds = WindowBounds {
        x: Some(30),
        y: Some(40),
        width: 900,
        height: 700,
    };
    let restore = ImmersiveRestoreState {
        maximized: false,
        normal_bounds: bounds.clone(),
    };

    assert_eq!(
        restore.restore_target(),
        WindowRestoreTarget::Normal(bounds)
    );
}
 Step 2: 运行并确认失败
Run:

powershell
复制
cargo test --manifest-path src-tauri/Cargo.toml immersive_restore -- --nocapture
Expected: FAIL，缺少 restore 类型。

 Step 3: 定义纯 restore types
rust
复制
#[derive(Debug, Clone, PartialEq, Eq)]
struct ImmersiveRestoreState {
    maximized: bool,
    normal_bounds: WindowBounds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WindowRestoreTarget {
    Maximized,
    Normal(WindowBounds),
}

impl ImmersiveRestoreState {
    fn restore_target(&self) -> WindowRestoreTarget {
        if self.maximized {
            WindowRestoreTarget::Maximized
        } else {
            WindowRestoreTarget::Normal(self.normal_bounds.clone())
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowState {
    maximized: bool,
    fullscreen: bool,
    normal_bounds: WindowBounds,
}
为 AppState 添加：

rust
复制
immersive_restore_state: Mutex<Option<ImmersiveRestoreState>>,
在 run() 初始化为 Mutex::new(None)。

 Step 4: 实现 window-state helper
新增：

rust
复制
fn current_window_state(
    app: &AppHandle,
) -> Result<WindowState, String>;
它必须：

获取 main window，否则返回 main window is not available。

读取真实 is_maximized() 与 is_fullscreen()。

normal_bounds 使用已持久化值；只有当前非最大化且非全屏时才从真实窗口刷新 bounds。

不改变窗口。

 Step 5: 实现 enter_immersive_mode

严格按以下顺序：

获取 main window。
读取并刷新 WindowPreferences。
捕获 maximized 与最后有效 normal_bounds。
调用 set_fullscreen(true)。
fullscreen 成功后才写入 immersive_restore_state。
将 last_immersive=true 写盘。
返回真实 WindowState。
任何步骤失败时，不启动或重置 Pomodoro，也不清空旧 reminder/activation 状态。

 Step 6: 实现 exit_immersive_mode
严格按以下顺序：

获取 main window。
调用 set_fullscreen(false)。
从 immersive_restore_state 取出 runtime restore state。
若 runtime state 不存在，使用持久化的 maximized 和 normal_bounds。
Maximized target 调用 maximize()。
Normal target 调用 unmaximize()，恢复 exact logical size/position。
更新 WindowPreferences.maximized、normal_bounds 和 last_immersive=false。
返回真实 WindowState。
 Step 7: 删除 generic set_window_mode
删除命令并在 generate_handler! 中注册：

rust
复制
get_window_state,
enter_immersive_mode,
exit_immersive_mode,
set_main_window_maximized,
保留 set_always_on_top。

 Step 8: 创建 src/lib/windowing.ts
至少实现：

ts
复制
import { invoke } from '@tauri-apps/api/core';

export type WindowLayout = 'adaptive';

export interface WindowBounds {
  x: number | null;
  y: number | null;
  width: number;
  height: number;
}

export interface WindowPreferences {
  layout: WindowLayout;
  maximized: boolean;
  normalBounds: WindowBounds;
  alwaysOnTop: boolean;
  lastImmersive: boolean;
}

export interface WindowState {
  maximized: boolean;
  fullscreen: boolean;
  normalBounds: WindowBounds;
}

export interface FloatingWindowPreferences {
  visible: boolean;
  x: number | null;
  y: number | null;
  width: number;
  height: number;
  alwaysOnTop: boolean;
}

export const getWindowPreferences =
  (): Promise<WindowPreferences> =>
    invoke<WindowPreferences>('get_window_preferences');

export const getWindowState = (): Promise<WindowState> =>
  invoke<WindowState>('get_window_state');

export const enterImmersiveMode = (): Promise<WindowState> =>
  invoke<WindowState>('enter_immersive_mode');

export const exitImmersiveMode = (): Promise<WindowState> =>
  invoke<WindowState>('exit_immersive_mode');

export const setMainWindowMaximized = (
  maximized: boolean
): Promise<WindowState> =>
  invoke<WindowState>('set_main_window_maximized', { maximized });

export const setAlwaysOnTop = (
  alwaysOnTop: boolean
): Promise<void> =>
  invoke<void>('set_always_on_top', { alwaysOnTop });

export const getFloatingWindowPreferences =
  (): Promise<FloatingWindowPreferences> =>
    invoke<FloatingWindowPreferences>(
      'get_floating_window_preferences'
    );

export const showFloatingWindow = (): Promise<void> =>
  invoke<void>('show_floating_window');

export const hideFloatingWindow = (): Promise<void> =>
  invoke<void>('hide_floating_window');

export const toggleFloatingWindow = (): Promise<void> =>
  invoke<void>('toggle_floating_window');
 Step 9: 清理 tasks.ts 边界
从 tasks.ts 移除：

FloatingWindowPreferences
getFloatingWindowPreferences
showFloatingWindow
hideFloatingWindow
toggleFloatingWindow
保留：

FloatingIntent

openTaskFromFloating

openFocusFromFloating

takePendingFloatingIntent

 Step 10: 验证

powershell
复制
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml immersive_restore -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml
npm run check
Expected: PASS。

 Step 11: 提交
powershell
复制
git add src-tauri/src/lib.rs
git add src/lib/windowing.ts
git add src/lib/tasks.ts
git commit -m "Add explicit immersive window commands"
### Task 5: 全视口 AppShell、抽屉与诊断迁移
Files:

Create: src/lib/ui-state.ts
Create: src/lib/components/AppShell.svelte
Create: src/lib/components/ContextDrawer.svelte
Create: src/lib/components/ToastStack.svelte
Rename: src/lib/components/DiagnosticsPanel.svelte → src/lib/components/DiagnosticsDrawer.svelte
Modify: src/app.css
Modify: src/routes/+page.svelte:1-768
Modify: e2e/preview.spec.ts
Interfaces:

Consumes:
Task 4 toggleFloatingWindow()
Task 4 setAlwaysOnTop()
existing page warnings and runtime diagnostics
Produces:
ts
复制
export type AppScene = 'tasks' | 'focus';
export type ShellDrawer = 'diagnostics' | null;
export type DrawerSize = 'normal' | 'wide';

export type ImmersiveDisplayState =
  | 'off'
  | 'system'
  | 'visual-fallback';

export type ToastTone =
  | 'info'
  | 'success'
  | 'warning'
  | 'danger';

export interface ToastMessage {
  id: string;
  tone: ToastTone;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
  onDismiss?: () => void;
}
ContextDrawer:

ts
复制
interface Props {
  open: boolean;
  drawerId: string;
  title: string;
  size?: DrawerSize;
  onClose: () => void;
  children: Snippet;
}
AppShell:

ts
复制
interface Props {
  activeScene: AppScene;
  openDrawer: ShellDrawer;
  immersiveDisplay: ImmersiveDisplayState;
  initialized: boolean;
  tauriAvailable: boolean;
  toasts: ToastMessage[];
  onSceneChange: (
    scene: AppScene
  ) => void | Promise<void>;
  onOpenDiagnostics: () => void;
  onCloseDrawer: () => void;
  onToggleFloating: () => void | Promise<void>;
  children: Snippet;
  diagnostics: Snippet;
}
 Step 1: 添加 failing shell E2E
加入 e2e/preview.spec.ts：

ts
复制
test('renders the adaptive shell and diagnostics drawer', async ({
  page
}) => {
  await page.goto('/');

  await expect(
    page.getByRole('main', { name: '任务场景' })
  ).toBeVisible();
  await expect(
    page.getByRole('navigation', { name: '主导航' })
  ).toBeVisible();

  const trigger = page.getByRole('button', {
    name: '打开设置与诊断'
  });
  await trigger.click();

  await expect(
    page.getByRole('dialog', { name: '设置与诊断' })
  ).toBeVisible();

  await page.keyboard.press('Escape');

  await expect(
    page.getByRole('dialog', { name: '设置与诊断' })
  ).not.toBeAttached();
  await expect(trigger).toBeFocused();
});

test('keeps the document inside the viewport', async ({ page }) => {
  await page.setViewportSize({ width: 760, height: 560 });
  await page.goto('/');

  await expect
    .poll(() =>
      page.evaluate(() => ({
        width:
          document.documentElement.scrollWidth <= window.innerWidth,
        height:
          document.documentElement.scrollHeight <= window.innerHeight
      }))
    )
    .toEqual({ width: true, height: true });
});
 Step 2: 运行并确认失败
powershell
复制
npm run test:e2e -- --grep "adaptive shell|inside the viewport"
Expected: FAIL，新 shell selectors 不存在。

 Step 3: 创建 ui-state.ts
写入本任务 Interfaces 中列出的全部类型。

 Step 4: 实现 ContextDrawer.svelte focus lifecycle
核心脚本：

svelte
复制
<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import type { DrawerSize } from '$lib/ui-state';

  interface Props {
    open: boolean;
    drawerId: string;
    title: string;
    size?: DrawerSize;
    onClose: () => void;
    children: Snippet;
  }

  let {
    open,
    drawerId,
    title,
    size = 'normal',
    onClose,
    children
  }: Props = $props();

  let panel = $state<HTMLElement>();
  let returnFocus: HTMLElement | null = null;

  $effect(() => {
    if (!open) return;

    returnFocus =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;

    void tick().then(() => panel?.focus());

    return () => {
      const target = returnFocus;
      void tick().then(() => {
        if (target?.isConnected) target.focus();
      });
    };
  });

  function focusableElements(): HTMLElement[] {
    if (!panel) return [];
    return [
      ...panel.querySelectorAll<HTMLElement>(
        'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), a[href], [tabindex]:not([tabindex="-1"])'
      )
    ];
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
      return;
    }

    if (event.key !== 'Tab') return;

    const controls = focusableElements();
    if (controls.length === 0) {
      event.preventDefault();
      return;
    }

    const first = controls[0];
    const last = controls[controls.length - 1];
    const active = document.activeElement;

    if (
      event.shiftKey &&
      (active === first || !controls.includes(active as HTMLElement))
    ) {
      event.preventDefault();
      last.focus();
    } else if (
      !event.shiftKey &&
      (active === last || !controls.includes(active as HTMLElement))
    ) {
      event.preventDefault();
      first.focus();
    }
  }
</script>
Markup 必须使用：

svelte
复制
{#if open}
  <div
    class="drawer-backdrop"
    onclick={(event) => {
      if (event.currentTarget === event.target) onClose();
    }}
  >
    <aside
      bind:this={panel}
      id={drawerId}
      class:wide={size === 'wide'}
      class="context-drawer"
      role="dialog"
      aria-modal="true"
      aria-labelledby={`${drawerId}-title`}
      tabindex="-1"
      onkeydown={handleKeydown}
    >
      <header>
        <h2 id={`${drawerId}-title`}>{title}</h2>
        <button
          type="button"
          aria-label={`关闭${title}`}
          onclick={onClose}
        >×</button>
      </header>
      <div class="drawer-body">
        {@render children()}
      </div>
    </aside>
  </div>
{/if}
CSS invariants：

css
复制
.drawer-backdrop {
  position: fixed;
  inset: 0;
  z-index: 40;
  display: flex;
  justify-content: flex-end;
  background: rgb(5 6 8 / 0.68);
}

.context-drawer {
  width: min(420px, 100%);
  height: 100%;
  min-height: 0;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
}

.context-drawer.wide {
  width: min(980px, calc(100% - 48px));
}

.drawer-body {
  min-height: 0;
  overflow: auto;
  overscroll-behavior: contain;
}
 Step 5: 实现 ToastStack.svelte

messages: ToastMessage[]

warning/danger 使用 role="alert"

info/success 使用 role="status"

固定在右上方，不影响 grid 尺寸

action button 调用 onAction

dismiss button 调用 onDismiss

success toast 的 icon 使用单个 ✦

 Step 6: 实现 AppShell.svelte

根节点：

svelte
复制
<div
  class="app-shell"
  class:immersive={immersiveDisplay !== 'off'}
  data-immersive={immersiveDisplay}
>
必须包含：

header 中的 StarToDo identity、runtime status、悬浮窗按钮、打开设置与诊断。

nav aria-label="主导航"。

main aria-label={activeScene === 'tasks' ? '任务场景' : '专注场景'}。

ContextDrawer 中的 diagnostics snippet。

ToastStack。

immersive 时隐藏 header/nav，但不卸载当前 scene。

 Step 7: 建立全局 viewport invariants

在 src/app.css 中加入：

css
复制
html,
body {
  width: 100%;
  height: 100%;
  overflow: hidden;
}

body {
  min-width: 320px;
}

body > div {
  height: 100%;
}

.app-shell {
  width: 100%;
  height: 100dvh;
  min-height: 0;
  overflow: hidden;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  grid-template-rows: auto minmax(0, 1fr);
}

.app-header {
  grid-column: 1 / -1;
  min-width: 0;
}

.scene-navigation {
  min-height: 0;
}

.scene-canvas {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

@media (max-width: 759px) {
  .app-shell {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto minmax(0, 1fr) auto;
  }

  .scene-navigation {
    grid-row: 3;
  }
}

@media (max-height: 559px) {
  .density-help {
    display: none !important;
  }
}

@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    scroll-behavior: auto !important;
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
保留现有 :focus-visible 样式。

 Step 8: 重命名诊断组件并删除 mode controls
powershell
复制
git mv src/lib/components/DiagnosticsPanel.svelte src/lib/components/DiagnosticsDrawer.svelte
删除：

WindowMode
mode
onModeChange
compact/full buttons
set_window_mode 调用
使用 Task 4 的 setAlwaysOnTop()，保留：

runtime/process/database diagnostics

notification diagnostics

activation error

reminder report

UI release/hide

always-on-top

 Step 9: 集成 +page.svelte

AppView 改为导入的 AppScene。

activeView 改为 activeScene。

删除 WindowMode、windowMode、handleModeChange()。

删除对 legacy mode 的 get_window_preferences 读取。

生命周期、reminder warning listener、activation queue、Pomodoro refresh sequence 和 floating intent drain 的实现保持不变。

现有 page warnings 转换为 ToastMessage[] 只改变展示，不改变 claim/ack/reconcile。

Task 6 前临时渲染 TaskWorkspace compact={false}。

使用 DiagnosticsDrawer snippet。

 Step 10: 验证

powershell
复制
npm run check
npm run test:unit
npm run test:e2e -- --grep "adaptive shell|inside the viewport|diagnostics"
Expected: PASS。

 Step 11: 提交
powershell
复制
git add src/lib/ui-state.ts
git add src/lib/components/AppShell.svelte
git add src/lib/components/ContextDrawer.svelte
git add src/lib/components/ToastStack.svelte
git add src/lib/components/DiagnosticsDrawer.svelte
git add src/app.css
git add src/routes/+page.svelte
git add e2e/preview.spec.ts
git add -u src/lib/components/DiagnosticsPanel.svelte
git commit -m "Build the adaptive application shell"
### Task 6: 将 TaskWorkspace 转换为任务场景
Files:

Rename: src/lib/components/TaskWorkspace.svelte → src/lib/components/TaskScene.svelte
Create: src/lib/components/TaskCanvas.svelte
Modify: src/routes/+page.svelte
Modify: src/lib/components/ProjectSidebar.svelte
Modify: src/lib/components/TaskComposer.svelte
Modify: e2e/preview.spec.ts
Interfaces:

Consumes:
Task 5 ContextDrawer
existing task/project/reminder command wrappers
existing operation-token types
Produces:
ts
复制
type TaskDrawer =
  | 'create'
  | 'filters'
  | 'projects'
  | 'planner'
  | 'trash'
  | null;

interface TaskSceneProps {
  tauriAvailable: boolean;
  initialized: boolean;
  activationId: number | null;
  activationNonce: number;
  onReminderReconcile: () => Promise<ReminderReport>;
  onMutationWarning: (
    context: ReminderWarningContext
  ) => void;
  onActivationResolved: (nonce: number) => void;
  pomodoroCounts?: ReadonlyMap<number, number>;
  onStartFocus?: (taskId: number) => void;
  onPomodoroTasksChanged?: (
    unavailableTaskId?: number
  ) => void;
}
TaskCanvas：

ts
复制
interface Props {
  activeTasks: Task[];
  completedTasks: Task[];
  trashTasks: Task[];
  projects: Project[];
  activationId: number | null;
  pomodoroCounts: ReadonlyMap<number, number>;
  trashMode?: boolean;
  onOpenDetails: (taskId: number) => void;
  onFocus?: (taskId: number) => void;
  onUpdate: (
    id: number,
    input: TaskInput
  ) => Promise<TaskMutationWithToken>;
  onCompleted: (
    id: number,
    completed: boolean
  ) => Promise<TaskCompletionMutationWithToken>;
  onSnooze: (
    id: number,
    untilUnixMs: number
  ) => Promise<TaskMutationWithToken>;
  onDeferToTomorrow: (
    id: number
  ) => Promise<TaskMutationWithToken>;
  onDelete: (
    id: number
  ) => Promise<MutationWarningResult>;
  onChanged: (
    task: Task,
    kind: TaskChangeKind,
    result: TaskMutationWithToken
  ) => Promise<boolean>;
  onRemoved: (
    id: number,
    result: MutationWarningResult
  ) => Promise<boolean>;
  onRestore: (
    id: number
  ) => Promise<TaskMutationWithToken>;
  onPermanentlyDelete: (
    id: number
  ) => Promise<MutationWarningResult>;
  onRestored: (
    task: Task,
    result: TaskMutationWithToken
  ) => Promise<boolean>;
  onPermanentlyRemoved: (
    id: number,
    result: MutationWarningResult
  ) => Promise<boolean>;
}
 Step 1: 添加 failing drawer E2E
ts
复制
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
 Step 2: 运行并确认失败
powershell
复制
npm run test:e2e -- --grep "task tools as contextual drawers"
Expected: FAIL，当前 week/trash 替换工作区而非 drawer。

 Step 3: 重命名组件
powershell
复制
git mv src/lib/components/TaskWorkspace.svelte src/lib/components/TaskScene.svelte
删除：

compact prop
WorkspaceView
WORKSPACE_VIEW_STORAGE_KEY
workspaceView
activeWorkspaceView
所有 compact-only copy、CSS 和分支
不得修改：

taskOperationSequence

latestTaskOperationSequence

beginOperation

isCurrentOperation

loadGeneration

activationRetryNonce

handleChanged

reminder warning reporting

activation acknowledgement sequencing

 Step 4: 引入 drawer state

ts
复制
let activeDrawer = $state<TaskDrawer>(null);

function openTaskDrawer(drawer: Exclude<TaskDrawer, null>): void {
  activeDrawer = drawer;

  if (drawer === 'trash') {
    void loadTrash().catch(() => undefined);
  }

  if (drawer === 'planner') {
    plannerWeekStart = weekStartLocalDate(currentLocalDate);
  }
}

function closeTaskDrawer(): void {
  activeDrawer = null;
}
正常任务的 visibleTasks 始终来自 tasks；新增独立：

ts
复制
let visibleDeletedTasks = $derived(
  deletedTasks.filter((task) => searchMatches(task, true))
);
不再通过 showTrash 替换 currentTasks。

 Step 5: 抽取 TaskCanvas.svelte
组件根节点：

svelte
复制
<div
  class="task-scroll"
  data-scroll-region="tasks"
  aria-busy={false}
>
普通模式渲染：

进行中
已完成
keyed TaskItem
trash mode 只渲染：

已删除
restore/permanent delete callbacks
CSS：

css
复制
.task-scroll {
  min-height: 0;
  overflow: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
}
 Step 6: 重排 TaskScene grid
根节点：

css
复制
.task-scene {
  height: 100%;
  min-height: 0;
  display: grid;
  grid-template-rows: auto auto minmax(0, 1fr);
  container-type: inline-size;
}
header 中保留一个 search input，并添加：

详细新建
筛选
项目
周计划
回收站
TaskComposer 继续使用现有 handleComposerCreate()。

现有详细创建表单 TaskScene.svelte 原 lines 892–977 原样移入 create drawer，保留：

submit()
field errors
recurrence timezone
archived project restriction
reminder fields
operation token
filters drawer 保留现有 execution views 和 status filter。

projects drawer 使用现有 ProjectSidebar。

planner drawer：

svelte
复制
<ContextDrawer
  open={activeDrawer === 'planner'}
  drawerId="planner-drawer"
  title="周计划"
  size="wide"
  onClose={closeTaskDrawer}
>
  <WeekPlanner
    tasks={plannerTasks}
    {projects}
    weekStart={plannerWeekStart}
    today={currentLocalDate}
    onWeekChange={setPlannerWeekStart}
    onReschedule={handlePlannerReschedule}
    onOpenTask={openPlannerTaskInList}
  />
</ContextDrawer>
trash drawer 使用 visibleDeletedTasks，不改变删除/恢复业务处理。

 Step 7: 修改 planner 返回任务行为
openPlannerTaskInList(id) 改为：

activeDrawer=null
executionView='all'
statusFilter='active'
searchQuery=''
await tick()
在局部 task scroll region 内 scrollIntoView
聚焦 task-action-${id}
不使用页面级滚动。

 Step 8: 添加 height-density CSS
TaskComposer.svelte：

css
复制
@media (max-height: 759px) {
  .composer {
    padding: 10px 0 12px;
  }

  .composer textarea {
    min-height: 48px;
    max-height: 76px;
  }

  .help {
    display: none;
  }
}

@media (max-height: 559px) {
  .composer-heading h3,
  .hint {
    display: none;
  }

  .composer textarea {
    min-height: 38px;
    resize: none;
  }
}
不得修改 parse()、submit()、partial batch retry 或 Ctrl/Cmd+Enter。

 Step 9: 集成页面
+page.svelte 导入并渲染 TaskScene，删除 compact prop，其他 callbacks 原样传递。

 Step 10: 验证
powershell
复制
npm run check
npm run test:unit
npm run test:e2e -- --grep "task tools as contextual drawers|persistence|workspace"
Expected: PASS。

 Step 11: 提交
powershell
复制
git add src/lib/components/TaskScene.svelte
git add src/lib/components/TaskCanvas.svelte
git add src/lib/components/ProjectSidebar.svelte
git add src/lib/components/TaskComposer.svelte
git add src/routes/+page.svelte
git add e2e/preview.spec.ts
git add -u src/lib/components/TaskWorkspace.svelte
git commit -m "Convert tasks into an adaptive scene"
### Task 7: 任务详情抽屉、键盘完成与基础星形反馈
Files:

Create: src/lib/task-interaction.ts
Create: src/lib/task-interaction.test.ts
Create: src/lib/components/TaskDetailsDrawer.svelte
Modify: src/lib/components/TaskItem.svelte:1-511
Modify: src/lib/components/TaskCanvas.svelte
Modify: src/lib/components/TaskScene.svelte
Interfaces:

Produces:
ts
复制
export function shouldToggleTaskFromKeyboard(
  key: string,
  ctrlKey: boolean,
  metaKey: boolean,
  altKey: boolean,
  targetIsRow: boolean
): boolean;
TaskItem 新增：

ts
复制
onOpenDetails: (taskId: number) => void;
TaskDetailsDrawer：

ts
复制
interface Props {
  open: boolean;
  task: Task | null;
  projects: Project[];
  onClose: () => void;
  onUpdate: (
    id: number,
    input: TaskInput
  ) => Promise<TaskMutationWithToken>;
  onDelete: (
    id: number
  ) => Promise<MutationWarningResult>;
  onChanged: (
    task: Task,
    kind: TaskChangeKind,
    result: TaskMutationWithToken
  ) => Promise<boolean>;
  onRemoved: (
    id: number,
    result: MutationWarningResult
  ) => Promise<boolean>;
}
 Step 1: 添加 failing keyboard tests
ts
复制
import { describe, expect, it } from 'vitest';

import { shouldToggleTaskFromKeyboard } from './task-interaction';

describe('task row keyboard completion', () => {
  it('accepts an unmodified Space on the row itself', () => {
    expect(
      shouldToggleTaskFromKeyboard(
        ' ',
        false,
        false,
        false,
        true
      )
    ).toBe(true);
  });

  it('rejects modified Space', () => {
    expect(
      shouldToggleTaskFromKeyboard(
        ' ',
        true,
        false,
        false,
        true
      )
    ).toBe(false);
  });

  it('rejects events from nested controls', () => {
    expect(
      shouldToggleTaskFromKeyboard(
        ' ',
        false,
        false,
        false,
        false
      )
    ).toBe(false);
  });

  it('rejects Enter', () => {
    expect(
      shouldToggleTaskFromKeyboard(
        'Enter',
        false,
        false,
        false,
        true
      )
    ).toBe(false);
  });
});
 Step 2: 运行并确认失败
powershell
复制
npm run test:unit -- src/lib/task-interaction.test.ts
Expected: FAIL，模块不存在。

 Step 3: 实现 predicate
ts
复制
export function shouldToggleTaskFromKeyboard(
  key: string,
  ctrlKey: boolean,
  metaKey: boolean,
  altKey: boolean,
  targetIsRow: boolean
): boolean {
  return (
    key === ' ' &&
    !ctrlKey &&
    !metaKey &&
    !altKey &&
    targetIsRow
  );
}
 Step 4: 将 TaskItem 改为可聚焦行
article 增加：

svelte
复制
tabindex="0"
onkeydown={(event) => {
  if (
    shouldToggleTaskFromKeyboard(
      event.key,
      event.ctrlKey,
      event.metaKey,
      event.altKey,
      event.target === event.currentTarget
    )
  ) {
    event.preventDefault();
    void toggleCompleted();
  }
}}
这确保嵌套 completion、focus、snooze、delete 等按钮不会触发行级 Space。

 Step 5: 移出 inline editor
从 TaskItem.svelte 移走现有：

lines 97–181 的 draft/edit/save 逻辑
lines 353–453 的 editor markup
normal delete confirm
保留：

complete/restore
snooze
defer
focus
trash restore
trash permanent delete
普通任务行将 编辑 改为：

svelte
复制
<button
  type="button"
  class="quiet"
  aria-label={`查看详情：${task.title}`}
  onclick={() => onOpenDetails(task.id)}
  disabled={busy}
>
  详情
</button>
 Step 6: 实现 TaskDetailsDrawer.svelte
将移出的 editor draft、validation、recurrence timezone 和 normal-delete ConfirmDialog 迁入该组件。

关键规则：

open && task !== null 时重置 draft。

打开后 focus title input。

Escape 由 ContextDrawer 关闭。

Ctrl/Cmd+Enter 保存。

validateTaskInput() 和本地时间转换逻辑保持原样。

onChanged() 返回 false 时保持 drawer 打开，防止 stale token 结果关闭新状态。

onRemoved() 返回 false 时保持打开。

command error 保留 draft，并用 role="alert" 显示。

保存或删除真正成功后才调用 onClose()。

 Step 7: 在 TaskScene 维护单一 selected task

ts
复制
let selectedTaskId = $state<number | null>(null);

let selectedTask = $derived(
  tasks.find((task) => task.id === selectedTaskId) ?? null
);

$effect(() => {
  if (selectedTaskId !== null && selectedTask === null) {
    selectedTaskId = null;
  }
});
通过 TaskCanvas 传递：

ts
复制
onOpenDetails={(taskId) => {
  selectedTaskId = taskId;
}}
只渲染一个 TaskDetailsDrawer。

 Step 8: 添加非阻塞星形完成反馈
在 TaskScene 中添加：

ts
复制
interface CompletionFeedback {
  id: number;
  title: string;
}

let completionFeedback =
  $state<CompletionFeedback | null>(null);
let completionFeedbackSequence = 0;
let completionFeedbackTimer:
  | ReturnType<typeof window.setTimeout>
  | undefined;

function showCompletionFeedback(task: Task): void {
  if (completionFeedbackTimer !== undefined) {
    window.clearTimeout(completionFeedbackTimer);
  }

  completionFeedback = {
    id: ++completionFeedbackSequence,
    title: task.title
  };

  completionFeedbackTimer = window.setTimeout(() => {
    completionFeedback = null;
    completionFeedbackTimer = undefined;
  }, 900);
}
在 handleChanged() 已确认 token current 后、kind === 'complete' 时调用；不等待 timer。

Markup：

svelte
复制
{#if completionFeedback}
  <div
    class="completion-feedback"
    role="status"
    aria-live="polite"
  >
    <span aria-hidden="true">✦</span>
    <span>已完成：{completionFeedback.title}</span>
  </div>
{/if}
reduced-motion 下只做 opacity 变化，不做 scale/translate。

 Step 9: 验证
powershell
复制
npm run test:unit -- src/lib/task-interaction.test.ts
npm run check
npm run test:unit
Expected: PASS。

 Step 10: 提交
powershell
复制
git add src/lib/task-interaction.ts
git add src/lib/task-interaction.test.ts
git add src/lib/components/TaskDetailsDrawer.svelte
git add src/lib/components/TaskItem.svelte
git add src/lib/components/TaskCanvas.svelte
git add src/lib/components/TaskScene.svelte
git commit -m "Move task editing into a details drawer"
### Task 8: 自适应 WeekPlanner
Files:

Create: src/lib/planner-selection.ts
Create: src/lib/planner-selection.test.ts
Modify: src/lib/components/WeekPlanner.svelte:1-268
Create/Modify: e2e/responsive.spec.ts
Interfaces:

Consumes:
addLocalCalendarDays(date: string, days: number): string | null
Produces:
ts
复制
export type PlannerSelection =
  | 'unscheduled'
  | string;

export function initialPlannerSelection(
  weekStart: string,
  today: string
): PlannerSelection;

export function normalizePlannerSelection(
  selection: PlannerSelection,
  weekStart: string
): PlannerSelection;
 Step 1: 添加 failing selection tests
ts
复制
import { describe, expect, it } from 'vitest';

import {
  initialPlannerSelection,
  normalizePlannerSelection
} from './planner-selection';

describe('planner selection', () => {
  it('selects today when it is inside the displayed week', () => {
    expect(
      initialPlannerSelection(
        '2026-08-24',
        '2026-08-27'
      )
    ).toBe('2026-08-27');
  });

  it('selects week start when today is outside the week', () => {
    expect(
      initialPlannerSelection(
        '2026-08-24',
        '2026-09-03'
      )
    ).toBe('2026-08-24');
  });

  it('keeps the unscheduled selection across week changes', () => {
    expect(
      normalizePlannerSelection(
        'unscheduled',
        '2026-08-31'
      )
    ).toBe('unscheduled');
  });

  it('resets a stale date after a week change', () => {
    expect(
      normalizePlannerSelection(
        '2026-08-27',
        '2026-08-31'
      )
    ).toBe('2026-08-31');
  });
});
 Step 2: 运行并确认失败
powershell
复制
npm run test:unit -- src/lib/planner-selection.test.ts
Expected: FAIL，模块不存在。

 Step 3: 实现 selection helpers
ts
复制
import { addLocalCalendarDays } from './tasks';

export type PlannerSelection =
  | 'unscheduled'
  | string;

function inDisplayedWeek(
  date: string,
  weekStart: string
): boolean {
  const weekEnd =
    addLocalCalendarDays(weekStart, 6) ?? weekStart;
  return date >= weekStart && date <= weekEnd;
}

export function initialPlannerSelection(
  weekStart: string,
  today: string
): PlannerSelection {
  return inDisplayedWeek(today, weekStart)
    ? today
    : weekStart;
}

export function normalizePlannerSelection(
  selection: PlannerSelection,
  weekStart: string
): PlannerSelection {
  if (selection === 'unscheduled') {
    return selection;
  }

  return inDisplayedWeek(selection, weekStart)
    ? selection
    : weekStart;
}
 Step 4: 在 WeekPlanner 添加选中 bucket
ts
复制
let selection = $state<PlannerSelection>(
  initialPlannerSelection(weekStart, today)
);

$effect(() => {
  selection = normalizePlannerSelection(
    selection,
    weekStart
  );
});

let narrowTasks = $derived(
  selection === 'unscheduled'
    ? unscheduledTasks
    : scheduledDays.find(
        (day) => day.date === selection
      )?.tasks ?? []
);
 Step 5: 抽取单一 planner task card snippet
将现有 planner task <article> markup 移入：

svelte
复制
{#snippet plannerTask(task: Task)}
  <!-- 使用现有 task title、project、priority、due、reminder、
       recurrence、select、busy、error 的完整 markup。 -->
{/snippet}
实施时必须原样搬移 WeekPlanner.svelte 当前 task-card 字段和 ID；不得创建第二份 reschedule/error 状态。

这里的注释不得保留在最终代码中；最终 snippet 必须包含当前 lines 139–169 及对应 scheduled-card 的完整 markup。

 Step 6: 添加窄屏 date strip
在 wide grid 前渲染：

未排期 tab
周一至周日七个 tabs
selected bucket 的 task body
同一个 plannerTask() snippet
ARIA：

容器 role="tablist"

button role="tab"

aria-selected

aria-controls

body role="tabpanel"

 Step 7: 使用 container query 而非仅 viewport query

css
复制
.planner {
  height: 100%;
  min-height: 0;
  display: grid;
  grid-template-rows: auto auto minmax(0, 1fr);
  container-type: inline-size;
}

.planner-narrow {
  display: none;
}

@container (max-width: 759px) {
  .planner-scroll {
    display: none;
  }

  .planner-narrow {
    min-height: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
  }

  .day-strip {
    display: flex;
    overflow-x: auto;
    overscroll-behavior-inline: contain;
  }

  .selected-day-body {
    min-height: 0;
    overflow: auto;
  }
}
删除现有 mobile .planner-grid { min-width:1568px; }。

wide grid 可以局部横向滚动，但不得导致 document overflow。

 Step 8: 添加 responsive E2E
至少测试：

520x420 → narrow selected-day view。

760x560 + drawer container 小于 760 → narrow selected-day view。

1180x760 + wide planner drawer → wide board。

所有尺寸 document width/height 不超过 viewport。

 Step 9: 验证

powershell
复制
npm run test:unit -- src/lib/planner-selection.test.ts
npm run check
npm run test:e2e -- --grep "planner stays inside|planner layout"
Expected: PASS。

 Step 10: 提交
powershell
复制
git add src/lib/planner-selection.ts
git add src/lib/planner-selection.test.ts
git add src/lib/components/WeekPlanner.svelte
git add e2e/responsive.spec.ts
git commit -m "Make the week planner viewport adaptive"
### Task 9: FocusScene 与首次自动沉浸
Files:

Rename: src/lib/components/FocusWorkspace.svelte → src/lib/components/FocusScene.svelte
Create: src/lib/components/FocusStage.svelte
Create: src/lib/components/AutoImmersivePrompt.svelte
Modify: src/lib/components/FocusMiniBar.svelte
Modify: src/lib/components/DiagnosticsDrawer.svelte
Modify: src/routes/+page.svelte
Modify: e2e/preview.spec.ts
Modify: e2e/responsive.spec.ts
Interfaces:

Consumes:
Task 1 AutoImmersivePreference
Task 4 enterImmersiveMode()
Task 4 exitImmersiveMode()
existing Pomodoro types/helpers
Produces:
FocusStage：

ts
复制
interface Props {
  snapshot: PomodoroSnapshot | null;
  selectedPhase: PomodoroPhase;
  selectedTaskTitle: string | null;
  nowUnixMs: number;
  busy: boolean;
  tauriAvailable: boolean;
  onChoosePhase: (
    phase: PomodoroPhase
  ) => void;
  onPrimary: () => void;
  onSkip: () => void;
  onReset: () => void;
}
AutoImmersivePrompt：

ts
复制
interface Props {
  open: boolean;
  onChoose: (
    preference: 'enabled' | 'disabled'
  ) => void;
  onCancel: () => void;
}
Page functions：

ts
复制
async function runPomodoroCommand(
  command: () => Promise<PomodoroMutationResult>,
  failedTaskId?: number | null
): Promise<boolean>;

function requestPomodoroStart(
  input: StartPomodoroInput
): void;

async function enterImmersiveDisplay(): Promise<void>;
async function exitImmersiveDisplay(): Promise<boolean>;
 Step 1: 添加 failing immersive E2E
ts
复制
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
 Step 2: 运行并确认失败
powershell
复制
npm run test:e2e -- --grep "centered focus stage"
Expected: FAIL，没有 immersive state/attribute。

 Step 3: 重命名 FocusWorkspace
powershell
复制
git mv src/lib/components/FocusWorkspace.svelte src/lib/components/FocusScene.svelte
保留现有：

selected phase

selected task reconciliation

settings draft

backend snapshot usage

remainingPomodoroSeconds

pomodoroProgress

pomodoroPrimaryAction

task unavailable behavior

 Step 4: 抽取 FocusStage.svelte

移动：

phase switch
timer
primary action
skip/reset
timer 必须为：

svelte
复制
<p
  class="timer"
  role="timer"
  aria-label={`${active ? '剩余时间' : '阶段时长'} ${formatPomodoroDuration(displayedSeconds)}`}
>
  {formatPomodoroDuration(displayedSeconds)}
</p>
进度环使用：

css
复制
.focus-ring {
  --progress-turn:
    calc(var(--progress) * 1turn);
  background:
    conic-gradient(
      var(--accent) var(--progress-turn),
      var(--line) 0
    );
}

.focus-ring.running .focus-ring-core {
  animation: focus-breathe 3.2s ease-in-out infinite;
}

@media (prefers-reduced-motion: reduce) {
  .focus-ring.running .focus-ring-core {
    animation: none;
  }
}
不得添加 Phase 2 星座图。

 Step 5: 将 focus context 放入 drawer
FocusScene 主画布保持 stage 居中；新增 专注上下文 按钮，drawer 内放：

task selector
cycle/today metrics
next phase
Pomodoro settings form
保持现有绑定和 command callbacks。

 Step 6: 实现首次自动沉浸 prompt
文案：

svelte
复制
<h2 id="auto-immersive-title">
  开始专注时进入沉浸模式？
</h2>
<p>
  启用后，每次开始新的专注阶段都会进入全屏；
  可在设置中随时修改。
</p>
<button
  type="button"
  onclick={() => onChoose('enabled')}
>
  进入并记住
</button>
<button
  bind:this={conservativeButton}
  type="button"
  onclick={() => onChoose('disabled')}
>
  保持窗口模式
</button>
要求：

role="dialog"

aria-modal="true"

初始聚焦“保持窗口模式”

Tab containment

Escape 调用 onCancel

cancel 不启动 pending session

 Step 7: 让 Pomodoro command 返回成功状态

将 runPomodoroCommand() 改为 Promise<boolean>：

成功完成 command 和 refresh 后返回 true。

command error 或 task unavailable 返回 false。

现有 warning、selection reset、refresh sequence 和 finally 保持不变。

pause/resume/skip/reset callers 可忽略返回值。

 Step 8: 协调 start 与 immersive

在 +page.svelte 添加：

ts
复制
let uiPreferences = $state(readUiPreferences());
let immersiveDisplay =
  $state<ImmersiveDisplayState>('off');
let pendingStartInput =
  $state<StartPomodoroInput | null>(null);
逻辑：

ts
复制
async function startPomodoroAndMaybeImmerse(
  input: StartPomodoroInput,
  immerse: boolean
): Promise<void> {
  const started = await runPomodoroCommand(
    () => startPomodoro(input),
    input.taskId
  );

  if (started && immerse) {
    await enterImmersiveDisplay();
  }
}

function requestPomodoroStart(
  input: StartPomodoroInput
): void {
  if (input.phase !== 'focus') {
    void startPomodoroAndMaybeImmerse(input, false);
    return;
  }

  if (uiPreferences.autoImmersive === 'unset') {
    pendingStartInput = input;
    return;
  }

  void startPomodoroAndMaybeImmerse(
    input,
    uiPreferences.autoImmersive === 'enabled'
  );
}
选择 preference：

保存到 ui-preferences.ts。

清除 pendingStartInput。

enabled 时 start 成功后进入沉浸。

disabled 时正常 start。

prompt cancel 只清除 pending input。

 Step 9: 实现 system/fallback immersive

ts
复制
async function enterImmersiveDisplay(): Promise<void> {
  if (!tauriAvailable) {
    immersiveDisplay = 'visual-fallback';
    return;
  }

  try {
    await enterImmersiveMode();
    immersiveDisplay = 'system';
  } catch (cause) {
    immersiveDisplay = 'visual-fallback';
    pomodoroCommandWarning =
      `系统全屏不可用，已改用窗口内沉浸：${errorMessage(cause)}`;
  }
}

async function exitImmersiveDisplay(): Promise<boolean> {
  if (immersiveDisplay === 'system') {
    try {
      await exitImmersiveMode();
    } catch (cause) {
      pomodoroCommandWarning =
        `退出系统全屏失败：${errorMessage(cause)}`;
      return false;
    }
  }

  immersiveDisplay = 'off';
  return true;
}
Escape 调用 exitImmersiveDisplay()。

点击任务 scene 时先 await exit；失败则留在 focus scene。

floating intent 打开 focus scene 时不自动进入沉浸。

pause/resume 不触发沉浸选择。

 Step 10: 让 preference 可在诊断设置中修改

DiagnosticsDrawer 新增：

ts
复制
autoImmersivePreference: AutoImmersivePreference;
onAutoImmersivePreferenceChange: (
  value: AutoImmersivePreference
) => void;
设置选项：

unset：下次询问
enabled：自动进入
disabled：保持窗口模式
页面回调同时更新 rune state 与 localStorage。

 Step 11: 简化 FocusMiniBar
删除 compact prop 与 compact CSS。

始终保留：

phase
remaining time
task title
pause/resume
open focus action
在 max-height:559px 通过 CSS 隐藏 task title，不用逻辑分支。

 Step 12: 验证
powershell
复制
npm run check
npm run test:unit
npm run test:e2e -- --grep "centered focus stage|navigates between tasks and focus"
Expected: PASS。

 Step 13: 提交
powershell
复制
git add src/lib/components/FocusScene.svelte
git add src/lib/components/FocusStage.svelte
git add src/lib/components/AutoImmersivePrompt.svelte
git add src/lib/components/FocusMiniBar.svelte
git add src/lib/components/DiagnosticsDrawer.svelte
git add src/routes/+page.svelte
git add e2e/preview.spec.ts
git add e2e/responsive.spec.ts
git add -u src/lib/components/FocusWorkspace.svelte
git commit -m "Build the immersive focus scene"
### Task 10: 悬浮窗 backend auto-size 与 manual-resize protection
Files:

Modify: src-tauri/src/lib.rs:80-93
Modify: src-tauri/src/lib.rs:239-260
Modify: src-tauri/src/lib.rs:1291-1422
Modify: src-tauri/src/lib.rs:2956-2974
Modify: src-tauri/src/lib.rs:3134-3193
Modify: src-tauri/src/lib.rs:3198-end
Modify: src/lib/windowing.ts
Interfaces:

Produces:
rust
复制
enum FloatingDisplayMode {
    Capsule,
    Expanded,
}

struct FloatingWindowPreferences {
    visible: bool,
    x: Option<i32>,
    y: Option<i32>,
    width: f64,
    height: f64,
    always_on_top: bool,
    user_resized: bool,
    display_mode: FloatingDisplayMode,
}

struct FloatingWindowRuntime {
    programmatic_target: Option<(u32, u32)>,
    programmatic_until_unix_ms: u128,
}

fn floating_display_size(
    mode: FloatingDisplayMode,
) -> (f64, f64);

fn set_floating_display_mode(
    app: AppHandle,
    mode: FloatingDisplayMode,
) -> Result<FloatingWindowPreferences, String>;

fn reset_floating_auto_size(
    app: AppHandle,
) -> Result<FloatingWindowPreferences, String>;
TypeScript 扩展：

ts
复制
export type FloatingSizeMode =
  | 'capsule'
  | 'expanded';

export interface FloatingWindowPreferences {
  visible: boolean;
  x: number | null;
  y: number | null;
  width: number;
  height: number;
  alwaysOnTop: boolean;
  userResized: boolean;
  displayMode: FloatingSizeMode;
}
 Step 1: 添加 exact-size failing tests
rust
复制
#[test]
fn floating_display_modes_have_recommended_sizes() {
    assert_eq!(
        floating_display_size(FloatingDisplayMode::Capsule),
        (340.0, 64.0)
    );
    assert_eq!(
        floating_display_size(FloatingDisplayMode::Expanded),
        (360.0, 260.0)
    );
}

#[test]
fn legacy_floating_preferences_enable_auto_size() {
    let preferences =
        serde_json::from_str::<FloatingWindowPreferences>(
            r#"{"visible":true,"x":10,"y":20,"width":340,"height":180,"alwaysOnTop":true}"#,
        )
        .expect("legacy floating preferences should decode");

    assert!(!preferences.user_resized);
    assert_eq!(
        preferences.display_mode,
        FloatingDisplayMode::Expanded
    );
}
 Step 2: 运行并确认失败
powershell
复制
cargo test --manifest-path src-tauri/Cargo.toml floating_display -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml legacy_floating -- --nocapture
Expected: FAIL，缺少 enum/fields/helper。

 Step 3: 扩展 preference schema
rust
复制
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum FloatingDisplayMode {
    Capsule,
    Expanded,
}

fn default_floating_display_mode() -> FloatingDisplayMode {
    FloatingDisplayMode::Expanded
}
在 preferences 中添加：

rust
复制
#[serde(default)]
user_resized: bool,

#[serde(default = "default_floating_display_mode")]
display_mode: FloatingDisplayMode,
新默认值：

width=360
height=260
display_mode=Expanded
user_resized=false
validation：

width 260..=800

height 56..=800

坐标保持现有范围

 Step 4: 添加 programmatic-resize runtime

AppState 添加：

rust
复制
floating_window_runtime:
    Mutex<FloatingWindowRuntime>,
初始化：

rust
复制
floating_window_runtime:
    Mutex::new(FloatingWindowRuntime::default()),
programmatic resize 前写入 target 和 now_unix_ms()+1500。

 Step 5: 区分 user 和 programmatic resize
在 WindowEvent::Resized 中将 physical size 转为 logical rounded tuple。

规则：

deadline 未过且 target 存在：

不设置 user_resized
actual 等于 target 时清除 target
deadline 已过：

清除 target
设置 user_resized=true
无 target：

设置 user_resized=true
所有情况继续持久化真实 width/height

Moved 只更新坐标，不设置 user_resized

 Step 6: 实现 size commands

set_floating_display_mode：

读取 preferences。
更新 display_mode。
user_resized=false 时把 width/height 改为推荐尺寸。
window 存在时设置 programmatic target，再调用 set_size()。
user_resized=true 时不覆盖 outer dimensions。
保存并返回 preferences。
不显示、隐藏或聚焦窗口。
reset_floating_auto_size：

设置 user_resized=false。
使用当前 display_mode 的推荐尺寸。
window 存在则 programmatic resize。
保存并返回 preferences。
在 generate_handler! 注册两个命令。

 Step 7: 扩展 windowing.ts
ts
复制
export const setFloatingDisplayMode = (
  mode: FloatingSizeMode
): Promise<FloatingWindowPreferences> =>
  invoke<FloatingWindowPreferences>(
    'set_floating_display_mode',
    { mode }
  );

export const resetFloatingAutoSize =
  (): Promise<FloatingWindowPreferences> =>
    invoke<FloatingWindowPreferences>(
      'reset_floating_auto_size'
    );
 Step 8: 验证
powershell
复制
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml floating_display -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml legacy_floating -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml
npm run check
Expected: PASS。

 Step 9: 提交
powershell
复制
git add src-tauri/src/lib.rs
git add src/lib/windowing.ts
git commit -m "Add breathing floating window sizing"
### Task 11: Floating capsule 与 expanded companion
Files:

Create: src/lib/components/FloatingCapsule.svelte
Create: src/lib/components/FloatingExpandedPanel.svelte
Modify: src/lib/components/FloatingWindow.svelte:1-487
Modify: src/lib/tasks.ts
Modify: src/routes/+page.svelte
Modify: e2e/responsive.spec.ts
Interfaces:

FloatingCapsule：

ts
复制
interface Props {
  phaseLabel: string;
  remainingLabel: string;
  taskTitle: string | null;
  paused: boolean;
  busy: boolean;
  onOpenFocus: () => void;
  onPrimary: () => void;
  onExpand: () => void;
}
FloatingExpandedPanel：

ts
复制
interface Props {
  focusActive: boolean;
  phaseLabel: string;
  statusLabel: string;
  remainingLabel: string;
  taskTitle: string | null;
  tasks: Task[];
  projects: Project[];
  busy: boolean;
  alwaysExpanded: boolean;
  userResized: boolean;
  onPrimary: () => void;
  onOpenFocus: () => void;
  onOpenTask: (taskId: number) => void;
  onHide: () => void;
  onAlwaysExpandedChange: (
    value: boolean
  ) => void;
  onResetAutoSize: () => void;
}
 Step 1: 添加 failing floating-route E2E
ts
复制
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
});
 Step 2: 运行并确认失败
powershell
复制
npm run test:e2e -- --grep "expanded idle companion"
Expected: FAIL，没有 data-display-mode 与分离组件。

 Step 3: 实现 FloatingCapsule.svelte
必须渲染：

单一 star marker
phase
tabular timer
task title ellipsis
pause/resume
aria-label="展开悬浮窗"
aria-label="打开专注工作区"
不渲染任务列表、设置或诊断。

 Step 4: 实现 FloatingExpandedPanel.svelte
必须：

展示当前 phase/status/timer/task

渲染当前已有 visible-task filtering 结果的前五项

保留 project、overdue、today 和 time labels

task click 调用 onOpenTask

focus click 调用 onOpenFocus

“始终展开” checkbox

只在 userResized 时显示“恢复自动尺寸”

可隐藏 floating window

content body 局部滚动

 Step 5: 在 FloatingWindow 接入 reducer

保留现有：

tasks/project loading
tasks-changed listener
pomodoro-state-changed listener
fallback polling
stale refresh protection
Tauri unavailable state
openTaskFromFloating
openFocusFromFloating
新增：

ts
复制
let uiPreferences = $state(readUiPreferences());

let display = $state(
  createFloatingDisplayState(
    false,
    uiPreferences.floatingExpansion === 'always'
  )
);

let floatingPreferences =
  $state<FloatingWindowPreferences | null>(null);

let collapseTimer:
  | ReturnType<typeof window.setTimeout>
  | undefined;

let lastSentSizeMode:
  | FloatingSizeMode
  | null = null;
dispatch：

ts
复制
function dispatchFloating(
  event: FloatingDisplayEvent
): void {
  const next = reduceFloatingDisplay(
    display,
    event
  );

  if (next === display) return;

  display = next;
  scheduleCollapse();
}
scheduleCollapse()：

清除旧 timer。

collapseAt=null 时不创建 timer。

delay 为 Math.max(0, collapseAt-Date.now())。

timeout 只 dispatch { type:'timeout', at:Date.now() }。

 Step 6: 同步 focus state 与 outer size

当 Pomodoro session 的 running/paused active 状态变化时 dispatch snapshot。

当 floatingSizeMode(display.mode) 变化时：

browser preview 不调用 Tauri。

desktop 调用 setFloatingDisplayMode()。

使用 lastSentSizeMode 防止重复命令。

成功后刷新 floatingPreferences。

command failure 显示本地 warning，不改业务 snapshot。

 Step 7: 连接 pointer/focus events

根节点：

svelte
复制
<section
  class="floating-window"
  role="region"
  aria-label="StarToDo 悬浮窗"
  data-display-mode={display.mode}
  onpointerenter={() =>
    dispatchFloating({
      type: 'pointer-enter',
      at: Date.now()
    })}
  onpointerleave={() =>
    dispatchFloating({
      type: 'pointer-leave',
      at: Date.now()
    })}
  onfocusin={() =>
    dispatchFloating({
      type: 'focus-in',
      at: Date.now()
    })}
  onfocusout={handleFocusOut}
>
handleFocusOut 在 relatedTarget 仍位于 root 内时直接返回，否则 dispatch focus-out。

 Step 8: 持久化 always-expanded
ts
复制
function changeAlwaysExpanded(
  value: boolean
): void {
  uiPreferences = {
    ...uiPreferences,
    floatingExpansion:
      value ? 'always' : 'auto'
  };

  writeFloatingExpansionPreference(
    uiPreferences.floatingExpansion
  );

  dispatchFloating({
    type: 'always-expanded',
    value,
    at: Date.now()
  });
}
 Step 9: 添加 motion

展开：180ms

收起：240ms

只 animate opacity/transform

reduced-motion：直接切换

外层尺寸变化由 backend command 控制

manual-resized window 不被 command 覆盖

 Step 10: 更新 imports

+page.svelte 的 toggle floating 从 windowing.ts 导入。

FloatingWindow.svelte 的 preference/show/hide/size commands 从 windowing.ts 导入。

pending floating intents 继续从 tasks.ts 导入。

 Step 11: 验证

powershell
复制
npm run test:unit -- src/lib/floating-display.test.ts
npm run check
npm run test:e2e -- --grep "expanded idle companion"
Expected: PASS。

 Step 12: 提交
powershell
复制
git add src/lib/components/FloatingCapsule.svelte
git add src/lib/components/FloatingExpandedPanel.svelte
git add src/lib/components/FloatingWindow.svelte
git add src/lib/tasks.ts
git add src/routes/+page.svelte
git add e2e/responsive.spec.ts
git commit -m "Redesign the floating focus companion"
### Task 12: Viewport matrix、完整回归与 Windows smoke test
Files:

Modify: e2e/preview.spec.ts
Modify: e2e/responsive.spec.ts
Create: docs/testing/adaptive-focus-canvas-windows-smoke.md
Modify only when an observed failure requires it:
files already touched by Tasks 1–11
Interfaces:

Consumes: Tasks 1–11 的全部产物。

Produces:

browser viewport regression evidence
keyboard/reduced-motion evidence
Windows desktop/installer smoke evidence
repository verification result
 Step 1: 添加完整 viewport matrix

ts
复制
const viewports = [
  { width: 320, height: 720 },
  { width: 520, height: 420 },
  { width: 760, height: 560 },
  { width: 1180, height: 760 },
  { width: 1440, height: 900 }
];

for (const viewport of viewports) {
  test(
    `task and focus scenes fit ${viewport.width}x${viewport.height}`,
    async ({ page }) => {
      await page.setViewportSize(viewport);
      await page.goto('/');

      const fitsViewport = () =>
        page.evaluate(() => ({
          width:
            document.documentElement.scrollWidth <=
            window.innerWidth,
          height:
            document.documentElement.scrollHeight <=
            window.innerHeight
        }));

      await expect.poll(fitsViewport).toEqual({
        width: true,
        height: true
      });

      await page
        .getByRole('button', { name: '专注' })
        .click();

      await expect.poll(fitsViewport).toEqual({
        width: true,
        height: true
      });
    }
  );
}
 Step 2: 添加 local-scroll test
ts
复制
test('scrolling is confined to declared local regions', async ({
  page
}) => {
  await page.setViewportSize({
    width: 520,
    height: 420
  });
  await page.goto('/');

  expect(
    await page.evaluate(
      () => document.scrollingElement?.scrollTop ?? -1
    )
  ).toBe(0);

  const regions = page.locator('[data-scroll-region]');
  const count = await regions.count();

  for (let index = 0; index < count; index += 1) {
    const region = regions.nth(index);
    if (await region.isVisible()) {
      await expect(region).toHaveCSS(
        'overflow-y',
        /auto|scroll/
      );
    }
  }
});
 Step 3: 添加 reduced-motion 与 focus restoration test
ts
复制
test.describe('reduced motion', () => {
  test.use({ reducedMotion: 'reduce' });

  test('keeps drawer and scene operations keyboard usable', async ({
    page
  }) => {
    await page.goto('/');

    const trigger = page.getByRole('button', {
      name: '打开设置与诊断'
    });

    await trigger.focus();
    await trigger.click();
    await page.keyboard.press('Escape');
    await expect(trigger).toBeFocused();

    await page
      .getByRole('button', { name: '专注' })
      .click();
    await expect(page.getByRole('timer')).toBeVisible();
  });
});
 Step 4: 运行 frontend verification
powershell
复制
npm run check
npm run test:unit
npm run build
npm run test:e2e
Expected: 全部退出码 0。

不得通过删除或放宽 viewport/accessibility assertions 来处理失败；修复实际布局或焦点问题。

 Step 5: 运行 backend 与 sidecar verification
powershell
复制
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
npm run build:notification-host
Expected: 全部退出码 0。

 Step 6: 运行 repository gate
powershell
复制
npm run verify
Expected: exit code 0，包括 check、unit、sidecar、frontend build、Rust format/tests 和 Playwright。

 Step 7: 创建 Windows smoke record
docs/testing/adaptive-focus-canvas-windows-smoke.md 必须包含以下条目，并在实际运行后填写：

markdown
复制
# Adaptive Focus Canvas Windows Smoke Test

## Environment

- Commit:
- Windows version:
- Display scale:
- Monitor layout:
- Build command:
- Test date:

## Main window

- [ ] First launch opens maximized inside the Windows work area and keeps the taskbar visible.
- [ ] Restored normal window cannot resize below 520 x 420.
- [ ] The UI remains usable at 520 x 420.
- [ ] Normal size and position survive hide-to-tray, show, UI release/rebuild, and process restart.
- [ ] Legacy compact preferences migrate without recreating compact mode.
- [ ] Always-on-top still persists and applies.

## Focus and immersive mode

- [ ] Starting a focus phase with preference unset asks once.
- [ ] “进入并记住” starts focus and enters true fullscreen.
- [ ] “保持窗口模式” starts focus without fullscreen.
- [ ] The diagnostics preference can restore “下次询问”.
- [ ] Escape exits fullscreen.
- [ ] The visible exit action exits fullscreen.
- [ ] Exiting restores the exact prior maximized or normal state.
- [ ] A fullscreen command failure uses in-window immersive fallback without changing Pomodoro state.
- [ ] Pause, resume, skip, reset, notification warning, and SQLite restoration still work.

## Task scene

- [ ] Search, execution filter, status filter, and project selection survive scene changes.
- [ ] Quick capture and partial batch retry still work.
- [ ] Detailed task creation preserves project, reminder, priority, and recurrence fields.
- [ ] Update, complete, restore, snooze, defer, delete, trash restore, and permanent delete work.
- [ ] Stale task command results do not overwrite newer UI state.
- [ ] Completing a task restores focus to an adjacent task or search.
- [ ] Completion shows the non-blocking star feedback.
- [ ] Notification activation reveals and focuses the intended task.

## Planner

- [ ] Wide planner shows unscheduled plus seven day columns.
- [ ] Narrow planner shows the date strip and one selected bucket.
- [ ] Reschedule failures restore the previous select value and show the existing error.
- [ ] Planner-to-list navigation closes the drawer and focuses the task.
- [ ] Planner scrolling never moves the document.

## Floating window

- [ ] Idle state opens expanded.
- [ ] Running or paused focus defaults to capsule.
- [ ] Pointer or keyboard interaction expands the capsule.
- [ ] Expansion collapses after the five-second interaction window and leave buffer.
- [ ] Keyboard focus inside prevents collapse.
- [ ] “始终展开” persists across floating-window recreation and process restart.
- [ ] Manual resize is not overwritten by later display-state changes.
- [ ] “恢复自动尺寸” restores the recommended size.
- [ ] Floating task and focus intents open the correct main-window scene.
- [ ] Closing the floating window hides it instead of exiting the app.

## Tray, notification, and installer

- [ ] Tray Show works.
- [ ] Tray Focus queues the focus scene intent.
- [ ] Tray 悬浮窗 toggles the floating window.
- [ ] Tray Hide works.
- [ ] Tray Release UI destroys and safely recreates the main UI.
- [ ] Tray Quit exits the process.
- [ ] Task notification activation works.
- [ ] Pomodoro notification activation works.
- [ ] Installed-app protocol activation works.
- [ ] Uninstall removes the app and protocol registration cleanly.
每个勾选项后追加一句实际观察证据；未测试的条目保持未勾选，不伪造结果。

 Step 8: 构建 installer 并验证协议/卸载
powershell
复制
npm run tauri -- build --bundles nsis
不要暂存或提交生成的 installer。

 Step 9: 检查最终 diff
powershell
复制
git status --short
git diff --stat
git diff --check
Expected:

没有 build/

没有 src-tauri/target/

没有 sidecar bin/、obj/、publish/

没有安装包

没有 whitespace error

只有本计划范围内的 source、test 和 smoke record

 Step 10: 提交验证证据

powershell
复制
git add e2e/preview.spec.ts
git add e2e/responsive.spec.ts
git add docs/testing/adaptive-focus-canvas-windows-smoke.md
git add src
git add src-tauri/src/lib.rs
git add src-tauri/tauri.conf.json
git commit -m "Verify adaptive focus canvas behavior"
Self-Review Results
Spec coverage
主窗口默认最大化、保留任务栏、自由 normal resize、最小 520 x 420：Task 3。
legacy compact migration 与 compact mode 删除：Tasks 3、5、6。
true fullscreen 与 exact restore：Tasks 4、9。
首次自动沉浸提示与持久化设置：Tasks 1、9。
AppShell、任务/专注 scene、context drawers、diagnostics relocation：Tasks 5、6、9。
100dvh、无页面级滚动、局部滚动、宽高层级：Tasks 5、6、8、12。
task operation tokens、stale discard、focus restoration：Tasks 6、7、12。
reminder warning leases/ack、pending activations、pending floating intents：所有相关页面协调代码明确保持，Task 12 smoke 验证。
Pomodoro SQLite authority、notification warnings、refresh sequence：Tasks 4、9、11 保持，Task 12 验证。
窄屏 planner date strip 与 selected bucket：Task 8。
基础 breathing focus ring 与轻量 star completion feedback：Tasks 7、9。
floating capsule、interaction-expanded、always-expanded、manual resize protection：Tasks 2、10、11。
reduced motion 与 keyboard focus：Tasks 2、5、7、9、11、12。
Phase 2 星图/拖拽排期与 Phase 3 批量高密度工作流已明确排除。
Placeholder scan
所有任务均指定了具体路径、接口、失败测试、预期错误、实现约束、验证命令和独立提交命令。
没有未命名组件、未定义函数、泛化错误处理或数据库迁移步骤。
WeekPlanner snippet 的搬移位置和必须保留的字段已经明确；最终应用代码不得保留计划说明注释。
Type and signature consistency
AutoImmersivePreference 和 FloatingExpansionPreference 只定义于 ui-preferences.ts。
AppScene、ShellDrawer、DrawerSize、ImmersiveDisplayState 和 toast types 只定义于 ui-state.ts。
UI FloatingDisplayMode 与 backend FloatingSizeMode 通过 floatingSizeMode() 显式映射。
Rust normal_bounds、always_on_top、last_immersive、user_resized、display_mode 分别映射到 TypeScript normalBounds、alwaysOnTop、lastImmersive、userResized、displayMode。
ImmersiveDisplayState 是前端展示状态；WindowState.fullscreen 是真实系统窗口状态。
runPomodoroCommand() 在 Task 9 统一改成 Promise<boolean>，已有调用者可忽略返回值，start flow 使用它决定是否进入沉浸。
Task mutation callback signatures 不因视觉组件拆分而改变。
不新增 SQLite schema 或第二份 Pomodoro/task authority。