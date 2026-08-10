<script lang="ts">
  import {
    errorMessage,
    parseTaskDrafts,
    todayLocalDate,
    type BatchCreateResult,
    type TaskDraft
  } from '$lib/tasks';

  interface Props {
    tauriAvailable: boolean;
    disabled: boolean;
    onCreate: (drafts: TaskDraft[]) => Promise<BatchCreateResult>;
  }

  let { tauriAvailable, disabled, onCreate }: Props = $props();
  let text = $state('');
  let drafts = $state<TaskDraft[]>([]);
  let parseBusy = $state(false);
  let submitBusy = $state(false);
  let error = $state<string | null>(null);
  let announcement = $state('');

  async function parse(): Promise<TaskDraft[]> {
    if (!text.trim()) {
      drafts = [];
      error = '请输入至少一项任务。';
      return [];
    }
    if (!tauriAvailable) {
      error = '浏览器预览无法调用任务解析器。';
      return [];
    }

    parseBusy = true;
    error = null;
    try {
      const parsed = await parseTaskDrafts(text, todayLocalDate());
      drafts = parsed;
      if (parsed.length === 0) error = '没有解析出可添加的任务。';
      return parsed;
    } catch (cause) {
      drafts = [];
      error = errorMessage(cause);
      return [];
    } finally {
      parseBusy = false;
    }
  }

  async function submit(): Promise<void> {
    const parsed = drafts.length > 0 ? drafts : await parse();
    if (parsed.length === 0) return;

    submitBusy = true;
    error = null;
    try {
      const result = await onCreate(parsed);
      if (result.remainingDrafts.length > 0) {
        drafts = result.remainingDrafts;
        const createdMessage = result.createdCount > 0 ? `已添加 ${result.createdCount} 项；` : '';
        error = `${createdMessage}${result.error ?? `还有 ${result.remainingDrafts.length} 项未添加。`}`;
        announcement = result.createdCount > 0 ? `已添加 ${result.createdCount} 项任务，剩余 ${result.remainingDrafts.length} 项待处理。` : '';
        return;
      }
      text = '';
      drafts = [];
      error = null;
      announcement = `已添加 ${result.createdCount} 项任务。`;
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      submitBusy = false;
    }
  }

  function handleInput(): void {
    drafts = [];
    error = null;
    announcement = '';
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void submit();
    }
  }
</script>

<section class="composer" aria-labelledby="composer-heading">
  <div class="composer-heading">
    <div>
      <p class="eyebrow">快速输入</p>
      <h3 id="composer-heading">按行写下要推进的事</h3>
    </div>
    <span class="hint">Ctrl/⌘ + Enter</span>
  </div>
  <textarea
    bind:value={text}
    placeholder="今天 整理发布清单\nTomorrow review the launch notes\nMonday 给设计提反馈"
    aria-describedby="composer-help"
    oninput={handleInput}
    onkeydown={handleKeydown}
    disabled={disabled || parseBusy || submitBusy}
  ></textarea>
  <p id="composer-help" class="help">支持今天、明天、周一至周日，以及 today、tomorrow、Monday–Sunday；每个关键词会成为后续任务的计划日期。</p>
  <div class="composer-actions">
    <button type="button" onclick={() => void parse()} disabled={disabled || !tauriAvailable || parseBusy || submitBusy}>{parseBusy ? '解析中…' : '解析预览'}</button>
    <button type="button" class="primary" onclick={() => void submit()} disabled={disabled || !tauriAvailable || parseBusy || submitBusy}>{submitBusy ? '添加中…' : '解析并添加'}</button>
  </div>
  {#if drafts.length}
    <ol class="drafts" aria-label="解析结果">
      {#each drafts as draft}
        <li><span>{draft.title}</span>{#if draft.plannedDate}<time datetime={draft.plannedDate}>{draft.plannedDate}</time>{:else}<span class="muted">无计划日期</span>{/if}</li>
      {/each}
    </ol>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <p class="sr-only" aria-live="polite">{announcement}</p>
</section>

<style>
  .composer { display:grid; gap:8px; padding:18px 0; border-bottom:1px solid var(--line); }.composer-heading { display:flex; align-items:baseline; justify-content:space-between; gap:12px; }.eyebrow { margin:0; color:var(--muted); font-size:10px; letter-spacing:.14em; text-transform:uppercase; }.composer h3 { margin:4px 0 0; font-size:13px; font-weight:600; }.hint,.help,.muted { color:var(--muted); font-size:11px; }.help { margin:0; line-height:1.5; }.composer textarea { width:100%; min-height:82px; resize:vertical; border:1px solid var(--line); border-radius:4px; padding:9px; color:inherit; background:rgba(0,0,0,.14); }.composer-actions { display:flex; gap:8px; justify-content:flex-end; flex-wrap:wrap; }.composer button { border:1px solid var(--line); border-radius:4px; padding:6px 9px; color:#e5edf5; background:rgba(255,255,255,.05); font-size:12px; }.composer button.primary { border-color:rgba(91,169,255,.65); background:rgba(91,169,255,.18); }.drafts { display:grid; gap:5px; margin:2px 0 0; padding:10px 0 0 20px; border-top:1px solid rgba(255,255,255,.06); color:#d9e2eb; font-size:12px; }.drafts li { display:flex; justify-content:space-between; gap:12px; }.drafts time { color:var(--blue); font-variant-numeric:tabular-nums; }.error { margin:0; color:#ffaeae; font-size:12px; }.sr-only { position:absolute; width:1px; height:1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; } button:focus-visible,textarea:focus-visible { outline:2px solid var(--blue); outline-offset:2px; }
</style>
