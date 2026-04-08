<script>
    import { createDmDialogOpen } from '../lib/stores.js';
    import { createDm } from '../lib/tauri.js';
    import { onMount } from 'svelte';

    let userId = '', loading = false, error = null, visible = false;
    onMount(() => requestAnimationFrame(() => visible = true));

    function close() { visible = false; setTimeout(() => { createDmDialogOpen.set(false); userId = ''; error = null; }, 200); }

    async function submit() {
        if (!userId.trim()) return;
        loading = true; error = null;
        try { await createDm(userId.trim()); close(); }
        catch (e) { error = String(e); }
        finally { loading = false; }
    }

    function onKey(e) { if (e.key === 'Escape') close(); if (e.key === 'Enter' && userId.trim()) submit(); }
    function backdrop(e) { if (e.target === e.currentTarget) close(); }
</script>

<svelte:window on:keydown={onKey} />
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="bg" class:visible on:click={backdrop}>
    <div class="dlg" class:visible>
        <h2>Direct Message</h2>
        <div class="field"><label for="dm-uid">User ID</label><input id="dm-uid" bind:value={userId} placeholder="@user:server.com" /></div>
        {#if error}<div class="err">{error}</div>{/if}
        <div class="acts">
            <button class="sec" on:click={close}>Cancel</button>
            <button class="pri" on:click={submit} disabled={!userId.trim() || loading}>{loading ? 'Starting...' : 'Start Chat'}</button>
        </div>
    </div>
</div>

<style>
    .bg { position: fixed; inset: 0; background: rgba(0,0,0,0); backdrop-filter: blur(0); z-index: 150; display: flex; align-items: center; justify-content: center; transition: all 200ms; }
    .bg.visible { background: rgba(0,0,0,0.5); backdrop-filter: blur(4px); }
    .dlg { background: var(--bg-card); border: 1px solid var(--border-2); border-radius: 14px; padding: 28px; width: 400px; box-shadow: 0 12px 40px rgba(0,0,0,0.5); transform: scale(0.95); opacity: 0; transition: all 200ms var(--ease); }
    .dlg.visible { transform: scale(1); opacity: 1; }
    h2 { font-size: 1.1em; font-weight: 700; margin-bottom: 20px; }
    .field { margin-bottom: 16px; }
    .field label { display: block; font-size: 0.72em; font-weight: 600; color: var(--text-3); text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 5px; }
    .field input { width: 100%; padding: 10px 14px; border-radius: 8px; border: 1px solid var(--border-2); background: var(--bg-input); color: var(--text); font-size: 0.88em; font-family: 'JetBrains Mono', monospace; outline: none; }
    .field input:focus { border-color: var(--ac-border); box-shadow: 0 0 0 2px var(--ac-glow); }
    .err { color: var(--red); font-size: 0.82em; padding: 8px; background: rgba(248,81,73,0.08); border-radius: 6px; margin-bottom: 12px; }
    .acts { display: flex; justify-content: flex-end; gap: 10px; }
    .sec { padding: 10px 18px; border-radius: 8px; border: 1px solid var(--border-2); background: transparent; color: var(--text-2); font-size: 0.86em; font-weight: 600; cursor: pointer; font-family: inherit; }
    .pri { padding: 10px 18px; border-radius: 8px; border: none; background: var(--ac); color: white; font-size: 0.86em; font-weight: 600; cursor: pointer; font-family: inherit; }
    .pri:hover:not(:disabled) { opacity: 0.9; }
    .pri:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
