<script>
    import { invoke } from '@tauri-apps/api/core';
    import { simplexAddContactDialogOpen } from '../lib/stores.js';

    let linkUrl = '';
    let submitting = false;
    let statusMessage = '';
    let isError = false;

    function close() {
        simplexAddContactDialogOpen.set(false);
        linkUrl = '';
        statusMessage = '';
        isError = false;
    }

    async function submit() {
        const trimmed = linkUrl.trim();
        if (!trimmed.startsWith('simplex:/') && !trimmed.startsWith('https://simplex.chat/')) {
            statusMessage = 'Not a SimpleX link. Expected simplex:/ or https://simplex.chat/ prefix.';
            isError = true;
            return;
        }
        submitting = true;
        statusMessage = 'Submitting invitation to sidecar...';
        isError = false;
        try {
            await invoke('sx_submit_invitation', { code: trimmed });
            statusMessage = 'Invitation sent. Accept it on the other device; the contact will appear in the sidebar.';
            isError = false;
            linkUrl = '';
        } catch (e) {
            statusMessage = 'Error: ' + (e?.toString?.() ?? e);
            isError = true;
        } finally {
            submitting = false;
        }
    }

    function onKeydown(e) {
        if (e.key === 'Escape') close();
    }
</script>

<svelte:window on:keydown={onKeydown} />

{#if $simplexAddContactDialogOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="overlay" on:click={close}>
        <div class="dialog" on:click|stopPropagation role="dialog" aria-modal="true" aria-labelledby="sx-add-title">
            <h2 id="sx-add-title">Add SimpleX Contact</h2>
            <p class="hint">
                Paste a SimpleX contact address or one-time invitation URL.
                Both <code>simplex:/</code> and <code>https://simplex.chat/</code> links work.
            </p>

            <textarea
                bind:value={linkUrl}
                placeholder="simplex:/contact#/... or https://simplex.chat/contact#/..."
                rows="4"
                disabled={submitting}
            ></textarea>

            {#if statusMessage}
                <p class="status" class:error={isError}>{statusMessage}</p>
            {/if}

            <div class="actions">
                <button class="btn-secondary" on:click={close} disabled={submitting}>Close</button>
                <button
                    class="btn-primary"
                    on:click={submit}
                    disabled={submitting || !linkUrl.trim()}
                >
                    {submitting ? 'Submitting...' : 'Submit'}
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
        width: 520px; max-width: 92vw;
        background: var(--bg-card, #161b22);
        border: 1px solid var(--ac-border);
        border-radius: 14px;
        padding: 24px;
        display: flex; flex-direction: column; gap: 14px;
    }
    h2 { margin: 0; font-size: 1.05em; }
    .hint { margin: 0; color: var(--text-3, #8b949e); font-size: 0.85em; }
    .hint code {
        background: var(--bg, #0e1117); padding: 1px 5px;
        border-radius: 4px; font-size: 0.92em;
    }
    textarea {
        width: 100%;
        padding: 10px 12px;
        border-radius: 8px;
        border: 1px solid var(--ac-border);
        background: var(--bg, #0e1117);
        color: var(--text, #c9d1d9);
        font-family: monospace;
        font-size: 0.82em;
        resize: vertical;
        outline: none;
        min-height: 90px;
    }
    textarea:focus { border-color: var(--ac); }

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
