<script>
    import { createRoomDialogOpen } from '../lib/stores.js';
    import { createRoom } from '../lib/tauri.js';
    import { onMount } from 'svelte';

    let name = '', topic = '', roomType = 'private', enableEncryption = true;
    let roomAddress = '', showAdvanced = false;
    let loading = false, error = null, visible = false;

    onMount(() => requestAnimationFrame(() => visible = true));

    function close() { visible = false; setTimeout(() => { createRoomDialogOpen.set(false); name = ''; topic = ''; error = null; roomType = 'private'; enableEncryption = true; roomAddress = ''; showAdvanced = false; }, 200); }

    $: isPublic = roomType === 'public';
    $: title = isPublic ? 'Create a public room' : 'Create a private room';

    async function submit() {
        if (!name.trim()) return;
        loading = true; error = null;
        try {
            await createRoom(name.trim(), !isPublic && enableEncryption, isPublic, topic.trim() || null, null);
            close();
        } catch (e) { error = String(e); }
        finally { loading = false; }
    }

    function onKey(e) { if (e.key === 'Escape') close(); }
    function backdrop(e) { if (e.target === e.currentTarget) close(); }
</script>

<svelte:window on:keydown={onKey} />
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="bg" class:visible on:click={backdrop}>
    <div class="dlg" class:visible>
        <div class="head">
            <h2>{title}</h2>
            <button class="x" on:click={close} title="Close"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg></button>
        </div>
        <div class="body">
            <div class="field"><input bind:value={name} placeholder="Name" /></div>
            <div class="field"><input bind:value={topic} placeholder="Topic (optional)" /></div>

            <div class="field">
                <div class="sel-wrap">
                    <div class="sel-icon">
                        {#if isPublic}
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>
                        {:else}
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
                        {/if}
                    </div>
                    <select bind:value={roomType}>
                        <option value="private">Private room (invite only)</option>
                        <option value="public">Public room</option>
                    </select>
                    <div class="sel-chev"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><polyline points="6 9 12 15 18 9"/></svg></div>
                </div>
                <p class="hint">{isPublic ? 'Anyone can find and join this room.' : 'Only invited people can join.'}</p>
            </div>

            {#if isPublic}
                <div class="field">
                    <div class="addr"><span class="pre">#</span><input bind:value={roomAddress} placeholder="room-name" /><span class="suf">:simplego.dev</span></div>
                </div>
            {/if}

            {#if !isPublic}
                <div class="toggle-sec">
                    <!-- svelte-ignore a11y_no_static_element_interactions -->
                    <div class="trow">
                        <div class="sw" class:on={enableEncryption} on:click={() => enableEncryption = !enableEncryption} role="switch" aria-checked={enableEncryption} tabindex="0" on:keydown={e => e.key === 'Enter' && (enableEncryption = !enableEncryption)}><div class="knob"></div></div>
                        <div><span class="tlbl">Enable end-to-end encryption</span><span class="thint">You can't disable this later.</span></div>
                    </div>
                </div>
            {/if}

            <button class="link" on:click={() => showAdvanced = !showAdvanced}>{showAdvanced ? 'Hide advanced' : 'Show advanced'}</button>

            {#if showAdvanced}
                <div class="adv"><p class="hint">No additional options available yet.</p></div>
            {/if}

            {#if error}<div class="err">{error}</div>{/if}
        </div>
        <div class="foot">
            <button class="sec" on:click={close}>Cancel</button>
            <button class="pri" on:click={submit} disabled={!name.trim() || loading}>{loading ? 'Creating...' : 'Create room'}</button>
        </div>
    </div>
</div>

<style>
    .bg { position: fixed; inset: 0; background: rgba(0,0,0,0); backdrop-filter: blur(0); z-index: 150; display: flex; align-items: center; justify-content: center; transition: all 200ms; }
    .bg.visible { background: rgba(0,0,0,0.6); backdrop-filter: blur(4px); }
    .dlg { background: var(--bg-card); border: 1px solid var(--border-2); border-radius: 14px; width: 480px; max-width: 90vw; max-height: 85vh; overflow-y: auto; box-shadow: 0 16px 48px rgba(0,0,0,0.5); transform: scale(0.95); opacity: 0; transition: all 200ms var(--ease); }
    .dlg.visible { transform: scale(1); opacity: 1; }
    .head { display: flex; align-items: center; justify-content: space-between; padding: 24px 24px 0; }
    .head h2 { font-size: 1.2em; font-weight: 700; }
    .x { width: 32px; height: 32px; border: none; background: transparent; color: var(--text-3); cursor: pointer; border-radius: 8px; display: flex; align-items: center; justify-content: center; }
    .x:hover { background: var(--bg-hover); color: var(--text); }
    .body { padding: 20px 24px; }
    .field { margin-bottom: 16px; }
    .field > input { width: 100%; padding: 12px 16px; border-radius: 8px; border: 1px solid var(--border-2); background: var(--bg-input); color: var(--text); font-size: 0.92em; font-family: inherit; outline: none; }
    .field > input:focus { border-color: var(--ac-border); box-shadow: 0 0 0 2px var(--ac-glow); }
    .hint { font-size: 0.78em; color: var(--text-3); margin-top: 8px; line-height: 1.4; }

    .sel-wrap { position: relative; display: flex; align-items: center; }
    .sel-icon { position: absolute; left: 14px; color: var(--text-2); display: flex; pointer-events: none; z-index: 1; }
    select { width: 100%; padding: 12px 40px; border-radius: 8px; border: 1px solid var(--border-2); background: var(--bg-input); color: var(--text); font-size: 0.92em; font-family: inherit; appearance: none; cursor: pointer; }
    select:focus { outline: none; border-color: var(--ac-border); }
    .sel-chev { position: absolute; right: 14px; color: var(--text-3); pointer-events: none; display: flex; }

    .addr { display: flex; align-items: center; border: 1px solid var(--border-2); border-radius: 8px; background: var(--bg-input); overflow: hidden; }
    .pre { padding: 12px 0 12px 14px; color: var(--text-3); font-family: 'JetBrains Mono', monospace; font-size: 0.92em; }
    .addr input { flex: 1; padding: 12px 4px; border: none; background: transparent; color: var(--text); font-family: 'JetBrains Mono', monospace; font-size: 0.92em; outline: none; min-width: 0; }
    .suf { padding: 12px 14px 12px 0; color: var(--text-3); font-family: 'JetBrains Mono', monospace; font-size: 0.92em; }

    .toggle-sec { margin-bottom: 16px; }
    .trow { display: flex; gap: 12px; align-items: flex-start; }
    .sw { width: 44px; height: 24px; border-radius: 12px; background: var(--bg-raised); border: 1px solid var(--border-2); position: relative; cursor: pointer; flex-shrink: 0; transition: all 200ms; margin-top: 2px; }
    .sw.on { background: var(--ac); border-color: var(--ac); }
    .knob { width: 18px; height: 18px; border-radius: 50%; background: white; position: absolute; top: 2px; left: 2px; transition: transform 200ms var(--ease-b); }
    .sw.on .knob { transform: translateX(20px); }
    .tlbl { display: block; font-size: 0.92em; font-weight: 600; }
    .thint { display: block; font-size: 0.78em; color: var(--text-3); margin-top: 2px; }

    .link { border: none; background: transparent; color: var(--ac); font-size: 0.86em; font-weight: 600; font-family: inherit; cursor: pointer; padding: 4px 0; text-decoration: underline; margin-bottom: 12px; }
    .link:hover { opacity: 0.8; }
    .adv { padding: 12px 0; border-top: 1px solid var(--border); margin-top: 4px; }
    .err { color: var(--red); font-size: 0.82em; padding: 8px; background: rgba(248,81,73,0.08); border-radius: 6px; margin-top: 8px; }
    .foot { display: flex; justify-content: flex-end; gap: 10px; padding: 0 24px 24px; }
    .sec { padding: 10px 24px; border-radius: 8px; border: 1px solid var(--border-2); background: transparent; color: var(--text); font-size: 0.92em; font-weight: 600; cursor: pointer; font-family: inherit; }
    .sec:hover { background: var(--bg-hover); }
    .pri { padding: 10px 24px; border-radius: 8px; border: none; background: var(--ac); color: white; font-size: 0.92em; font-weight: 600; cursor: pointer; font-family: inherit; }
    .pri:hover:not(:disabled) { opacity: 0.9; }
    .pri:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
