<script>
    import { toasts } from '../lib/stores.js';
</script>

<div class="toast-stack">
    {#each $toasts as t (t.id)}
        <div class="toast" class:info={t.level === 'info'} class:warn={t.level === 'warn'} class:error={t.level === 'error'} class:success={t.level === 'success'}>
            <span class="icon" aria-hidden="true">
                {#if t.level === 'error'}
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
                {:else if t.level === 'warn'}
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
                {:else if t.level === 'success'}
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
                {:else}
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
                {/if}
            </span>
            <span class="msg">{t.message}</span>
        </div>
    {/each}
</div>

<style>
    .toast-stack {
        position: fixed;
        bottom: 20px;
        right: 20px;
        display: flex;
        flex-direction: column;
        gap: 8px;
        z-index: 2000;
        pointer-events: none;
        max-width: 400px;
    }
    .toast {
        display: flex;
        align-items: center;
        gap: 10px;
        padding: 10px 14px;
        border-radius: 10px;
        background: rgba(22, 27, 34, 0.95);
        border: 1px solid rgba(255,255,255,0.08);
        color: var(--text, #c9d1d9);
        font-size: 0.85em;
        backdrop-filter: blur(10px);
        box-shadow: 0 6px 18px rgba(0,0,0,0.35);
        animation: toastIn 180ms ease-out;
        pointer-events: auto;
    }
    .toast.info    { border-color: rgba(88,166,255,0.3);  color: #c9d1d9; }
    .toast.success { border-color: rgba(152,195,121,0.35); color: #c9d1d9; }
    .toast.warn    { border-color: rgba(229,192,123,0.35); color: #e5c07b; }
    .toast.error   { border-color: rgba(255,99,99,0.4);    color: #ff8080; }
    .icon { display: inline-flex; align-items: center; flex-shrink: 0; }
    .msg { flex: 1; line-height: 1.35; }
    @keyframes toastIn {
        from { opacity: 0; transform: translateY(8px); }
        to   { opacity: 1; transform: translateY(0); }
    }
</style>
