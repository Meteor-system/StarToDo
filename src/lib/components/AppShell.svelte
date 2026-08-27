<script lang="ts">
  import type { Snippet } from 'svelte';
  import ContextDrawer from './ContextDrawer.svelte';
  import ToastStack from './ToastStack.svelte';
  import type { AppScene, ImmersiveDisplayState, ShellDrawer, ToastMessage } from '$lib/ui-state';

  interface Props {
    activeScene: AppScene;
    openDrawer: ShellDrawer;
    immersiveDisplay: ImmersiveDisplayState;
    initialized: boolean;
    tauriAvailable: boolean;
    toasts: ToastMessage[];
    onSceneChange: (scene: AppScene) => void | Promise<void>;
    onOpenDiagnostics: () => void;
    onCloseDrawer: () => void;
    onToggleFloating: () => void | Promise<void>;
    children: Snippet;
    diagnosticsContent: Snippet;
  }

  let { activeScene, openDrawer, immersiveDisplay, initialized, tauriAvailable, toasts, onSceneChange, onOpenDiagnostics, onCloseDrawer, onToggleFloating, children, diagnosticsContent }: Props = $props();
</script>

<div class="app-shell" class:immersive={immersiveDisplay !== 'off'} data-immersive={immersiveDisplay}>
  <header class="app-header">
    <div class="identity"><span class="mark" aria-hidden="true"></span><div><p class="eyebrow">STAR TODO</p><p class="shell-title">{activeScene === 'focus' ? '专注' : '任务'}</p></div></div>
    <div class="shell-actions">
      <p class="status" aria-live="polite"><span class:offline={!tauriAvailable} class="status-dot"></span>{#if !initialized}正在初始化{:else if tauriAvailable}已连接{:else}浏览器预览{/if}</p>
      <button type="button" onclick={onToggleFloating} disabled={!tauriAvailable}>悬浮窗</button>
      <button type="button" onclick={onOpenDiagnostics} aria-controls="diagnostics-drawer">打开设置与诊断</button>
    </div>
  </header>
  <nav class="scene-navigation" aria-label="主导航">
    <button type="button" class:active={activeScene === 'tasks'} aria-current={activeScene === 'tasks' ? 'page' : undefined} onclick={() => onSceneChange('tasks')}>任务</button>
    <button type="button" class:active={activeScene === 'focus'} aria-current={activeScene === 'focus' ? 'page' : undefined} onclick={() => onSceneChange('focus')}>专注</button>
  </nav>
  <main class="scene-canvas" aria-label={activeScene === 'tasks' ? '任务场景' : '专注场景'}>
    {@render children()}
  </main>
  <ContextDrawer open={openDrawer === 'diagnostics'} drawerId="diagnostics-drawer" title="设置与诊断" size="wide" onClose={onCloseDrawer}>
    {@render diagnosticsContent()}
  </ContextDrawer>
  <ToastStack messages={toasts} />
</div>

<style>
  .app-header { display:flex; align-items:center; justify-content:space-between; gap:14px; padding:16px clamp(14px,4vw,36px); border-bottom:1px solid var(--line); }
  .identity,.shell-actions,.status { display:flex; align-items:center; }
  .identity { gap:11px; }.shell-actions { gap:12px; }.shell-actions button { border:1px solid var(--line); border-radius:var(--radius-sm); padding:6px 9px; color:var(--text-soft); background:var(--surface); font-size:12px; }
  .shell-actions button:hover:not(:disabled) { border-color:var(--text-soft); background:var(--surface-hover); color:var(--text); }.mark { width:9px; height:9px; border-radius:50%; background:var(--info); }.eyebrow { margin:0; color:var(--muted); font-size:10px; letter-spacing:.14em; }.shell-title { margin:3px 0 0; font-size:20px; font-weight:640; }.status { gap:7px; margin:0; color:var(--muted); font-size:12px; }.status-dot { width:7px; height:7px; border-radius:50%; background:var(--success); }.status-dot.offline { background:#e0ad65; }
  .scene-navigation { display:flex; flex-direction:column; gap:3px; padding:14px 8px; border-right:1px solid var(--line); background:var(--surface); }.scene-navigation button { border:0; border-radius:var(--radius-sm); padding:8px 12px; color:var(--muted); background:transparent; text-align:left; }.scene-navigation button.active { color:var(--text); background:var(--surface-hover); }.scene-canvas { padding:18px clamp(14px,4vw,36px) 26px; }
  .immersive .app-header,.immersive .scene-navigation { display:none; }.immersive { grid-template-columns:minmax(0,1fr); grid-template-rows:minmax(0,1fr); }
  @media (max-width:759px) { .scene-navigation { flex-direction:row; border-right:0; border-top:1px solid var(--line); padding:8px; } .scene-navigation button { flex:1; text-align:center; } }
</style>