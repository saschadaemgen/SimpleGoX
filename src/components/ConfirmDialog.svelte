<script>
    import { confirmDialog } from '../lib/stores.js';
    import { onMount } from 'svelte';

    let visible = false;
    onMount(() => requestAnimationFrame(() => visible = true));

    function close() {
        visible = false;
        setTimeout(() => confirmDialog.update(d => ({ ...d, visible: false, onConfirm: null })), 200);
    }

    async function confirm() {
        if ($confirmDialog.onConfirm) await $confirmDialog.onConfirm();
        close();
    }

    function backdrop(e) { if (e.target === e.currentTarget) close(); }
</script>

<svelte:window on:keydown={e => $confirmDialog.visible && e.key === 'Escape' && close()} />

{#if $confirmDialog.visible}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="bg" class:visible on:click={backdrop}>
        <div class="dlg" class:visible>
            <h3>{$confirmDialog.title}</h3>
            <p>{$confirmDialog.message}</p>
            <div class="acts">
                <button class="sec" on:click={close}>Cancel</button>
                <button class="pri" class:danger={$confirmDialog.danger} on:click={confirm}>{$confirmDialog.confirmText}</button>
            </div>
        </div>
    </div>
{/if}

<style>
    .bg { position: fixed; inset: 0; background: rgba(0,0,0,0); z-index: 300; display: flex; align-items: center; justify-content: center; transition: background 150ms; }
    .bg.visible { background: rgba(0,0,0,0.5); }
    .dlg { background: var(--bg-card); border: 1px solid var(--border-2); border-radius: 12px; padding: 24px; width: 340px; box-shadow: 0 8px 24px rgba(0,0,0,0.4); transform: scale(0.95); opacity: 0; transition: all 150ms var(--ease); }
    .dlg.visible { transform: scale(1); opacity: 1; }
    h3 { font-size: 1em; font-weight: 700; margin-bottom: 8px; }
    p { font-size: 0.86em; color: var(--text-2); line-height: 1.5; margin-bottom: 20px; }
    .acts { display: flex; justify-content: flex-end; gap: 8px; }
    .sec { padding: 8px 16px; border-radius: 8px; border: 1px solid var(--border-2); background: transparent; color: var(--text-2); font-size: 0.86em; font-weight: 600; cursor: pointer; font-family: inherit; }
    .sec:hover { background: var(--bg-hover); }
    .pri { padding: 8px 16px; border-radius: 8px; border: none; background: var(--ac); color: white; font-size: 0.86em; font-weight: 600; cursor: pointer; font-family: inherit; }
    .pri.danger { background: var(--red); }
    .pri:hover { opacity: 0.9; }
</style>
