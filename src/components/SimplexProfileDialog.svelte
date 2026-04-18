<script>
    import { invoke } from '@tauri-apps/api/core';
    import { onMount } from 'svelte';
    import { simplexProfileDialogOpen, simplexProfile } from '../lib/stores.js';

    let displayName = '';
    let fullName = '';
    let bio = '';
    let saving = false;
    let statusMessage = '';
    let isError = false;

    async function loadProfile() {
        try {
            const p = await invoke('sx_get_profile');
            if (p && p.has_profile) {
                displayName = p.display_name || '';
                fullName = p.full_name || '';
                bio = p.bio || '';
            }
        } catch (e) {
            console.warn('sx_get_profile failed:', e);
        }
    }

    // Re-load whenever the dialog opens so the latest persisted values show.
    $: if ($simplexProfileDialogOpen) loadProfile();

    function close() {
        simplexProfileDialogOpen.set(false);
        statusMessage = '';
        isError = false;
    }

    async function save() {
        if (!displayName.trim()) {
            statusMessage = 'Display name cannot be empty.';
            isError = true;
            return;
        }
        saving = true;
        statusMessage = 'Saving...';
        isError = false;
        try {
            await invoke('sx_set_profile', {
                displayName: displayName.trim(),
                fullName: fullName.trim(),
                bio: bio.trim(),
            });
            simplexProfile.set({
                display_name: displayName.trim(),
                full_name: fullName.trim(),
                bio: bio.trim(),
            });
            statusMessage = 'Saved.';
            isError = false;
        } catch (e) {
            statusMessage = 'Error: ' + (e?.toString?.() ?? e);
            isError = true;
        } finally {
            saving = false;
        }
    }

    function onKeydown(e) {
        if (e.key === 'Escape') close();
    }
</script>

<svelte:window on:keydown={onKeydown} />

{#if $simplexProfileDialogOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="overlay" on:click={close}>
        <div class="dialog" on:click|stopPropagation role="dialog" aria-modal="true" aria-labelledby="sx-profile-title">
            <h2 id="sx-profile-title">SimpleX Profile</h2>
            <p class="hint">
                This is the profile peers see when you accept their contact requests.
                Stored locally in the sidecar; never sent to any central server.
            </p>

            <label>
                <span>Display name</span>
                <input type="text" bind:value={displayName} disabled={saving} maxlength="64" />
            </label>

            <label>
                <span>Full name (optional)</span>
                <input type="text" bind:value={fullName} disabled={saving} maxlength="128" />
            </label>

            <label>
                <span>Bio (optional)</span>
                <textarea bind:value={bio} disabled={saving} maxlength="512" rows="3"></textarea>
            </label>

            {#if statusMessage}
                <p class="status" class:error={isError}>{statusMessage}</p>
            {/if}

            <div class="actions">
                <button class="btn-secondary" on:click={close} disabled={saving}>Close</button>
                <button class="btn-primary" on:click={save} disabled={saving || !displayName.trim()}>
                    {saving ? 'Saving...' : 'Save'}
                </button>
            </div>
        </div>
    </div>
{/if}

<style>
    .overlay {
        position: fixed; inset: 0;
        background: rgba(0,0,0,0.55);
        display: flex; align-items: center; justify-content: center;
        z-index: 1000;
    }
    .dialog {
        width: 480px; max-width: 92vw;
        background: var(--bg-card, #161b22);
        border: 1px solid var(--ac-border);
        border-radius: 14px;
        padding: 24px;
        display: flex; flex-direction: column; gap: 14px;
    }
    h2 { margin: 0; font-size: 1.05em; }
    .hint { margin: 0; color: var(--text-3, #8b949e); font-size: 0.85em; }

    label {
        display: flex; flex-direction: column; gap: 5px;
    }
    label > span {
        font-size: 0.8em; color: var(--text-3, #8b949e);
    }
    input, textarea {
        padding: 9px 12px;
        border-radius: 8px;
        border: 1px solid var(--ac-border);
        background: var(--bg, #0e1117);
        color: var(--text, #c9d1d9);
        font-family: inherit;
        font-size: 0.88em;
        outline: none;
    }
    input:focus, textarea:focus { border-color: var(--ac); }
    textarea { resize: vertical; }

    .status {
        margin: 0; font-size: 0.82em;
        padding: 8px 10px;
        border-radius: 6px;
        background: rgba(63,185,168,0.08);
        color: var(--text-2, #adbac7);
    }
    .status.error {
        background: rgba(255,99,99,0.12);
        color: #ff8080;
    }

    .actions {
        display: flex; justify-content: flex-end; gap: 10px;
        margin-top: 6px;
    }
    button {
        padding: 8px 16px; border-radius: 8px;
        border: none; cursor: pointer;
        font-size: 0.88em; font-family: inherit;
        transition: opacity 120ms;
    }
    button:disabled { opacity: 0.5; cursor: not-allowed; }
    .btn-primary {
        background: var(--ac, #58a6ff); color: var(--bg, #0e1117);
        font-weight: 600;
    }
    .btn-primary:hover:not(:disabled) { opacity: 0.9; }
    .btn-secondary {
        background: transparent; color: var(--text-2, #adbac7);
        border: 1px solid var(--ac-border);
    }
    .btn-secondary:hover:not(:disabled) { background: var(--bg-hover, rgba(240,246,252,0.06)); }
</style>
