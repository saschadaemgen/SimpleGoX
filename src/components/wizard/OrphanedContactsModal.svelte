<script>
    // Briefing 045 W5: destructive-confirmation modal for the Wizard's
    // orphan-cleanup path. Shown when the sidebar DB has contact rows
    // but no SimpleX profile is about to be created (or a new profile
    // replaces an existing one).
    //
    // Intentionally minimal: two buttons, explicit language about what
    // "delete" means, preview of the first few contacts so the user
    // can spot any surprise. No "keep them" option because the
    // contacts are cryptographically dead against a new profile -
    // keeping them would produce exactly the ghost-contact symptom
    // this briefing is undoing.
    import { createEventDispatcher } from 'svelte';
    const dispatch = createEventDispatcher();

    export let contacts = []; // ContactSummaryDto[]

    $: count = contacts?.length ?? 0;
    $: previewContacts = (contacts ?? []).slice(0, 5);
    $: hasMore = count > previewContacts.length;

    function cancel() {
        dispatch('cancel');
    }

    function confirm() {
        dispatch('confirm');
    }
</script>

<div class="overlay" role="dialog" aria-modal="true" aria-labelledby="orphaned-title">
    <div class="modal">
        <h2 id="orphaned-title">Previous Contacts Found</h2>
        <p class="lead">
            We found <strong>{count} {count === 1 ? 'contact' : 'contacts'}</strong>
            from a previous profile on this device.
        </p>
        <p>
            These contacts are bound to cryptographic keys from the previous
            profile and cannot be used with your new profile. They will be
            permanently deleted if you continue.
        </p>
        {#if previewContacts.length > 0}
            <ul class="preview">
                {#each previewContacts as c (c.contact_id)}
                    <li class="pvrow">
                        <span class="pvname">
                            {c.display_name || c.contact_id.slice(0, 16)}
                        </span>
                        <span class="pvid">
                            {c.contact_id.slice(0, 16)}
                        </span>
                    </li>
                {/each}
                {#if hasMore}
                    <li class="pvrow more">and {count - previewContacts.length} more</li>
                {/if}
            </ul>
        {/if}
        <div class="actions">
            <button class="btn btn-cancel" on:click={cancel}>Cancel Setup</button>
            <button class="btn btn-destructive" on:click={confirm}>Delete and Continue</button>
        </div>
    </div>
</div>

<style>
    .overlay {
        position: fixed;
        inset: 0;
        background: rgba(10, 12, 16, 0.72);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 10001;
        padding: 24px;
    }
    .modal {
        max-width: 460px;
        width: 100%;
        background: #14171c;
        border: 1px solid #23272e;
        border-radius: 12px;
        padding: 24px;
        color: #d4d7dd;
        box-shadow: 0 10px 40px rgba(0, 0, 0, 0.5);
    }
    h2 {
        margin: 0 0 12px;
        font-size: 18px;
        font-weight: 600;
        color: #e6edf3;
    }
    .lead {
        font-size: 14px;
        margin: 0 0 10px;
    }
    p {
        font-size: 13px;
        line-height: 1.5;
        color: #b1bac4;
        margin: 0 0 14px;
    }
    .preview {
        list-style: none;
        padding: 0;
        margin: 0 0 18px;
        background: #0e1117;
        border: 1px solid #23272e;
        border-radius: 8px;
        overflow: hidden;
    }
    .pvrow {
        display: flex;
        justify-content: space-between;
        gap: 10px;
        padding: 8px 12px;
        font-size: 12px;
        border-bottom: 1px solid #1a1e24;
    }
    .pvrow:last-child { border-bottom: none; }
    .pvrow.more {
        color: #707379;
        font-style: italic;
        justify-content: center;
    }
    .pvname {
        color: #d4d7dd;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }
    .pvid {
        color: #707379;
        font-family: 'JetBrains Mono', ui-monospace, Menlo, Consolas, monospace;
        font-size: 10px;
        flex-shrink: 0;
    }
    .actions {
        display: flex;
        justify-content: flex-end;
        gap: 10px;
        margin-top: 6px;
    }
    .btn {
        padding: 8px 18px;
        border: none;
        border-radius: 8px;
        font-size: 13px;
        font-weight: 600;
        font-family: inherit;
        cursor: pointer;
        transition: filter 0.15s, background 0.15s;
    }
    .btn-cancel {
        background: #23272e;
        color: #d4d7dd;
    }
    .btn-cancel:hover { background: #2a2e35; }
    .btn-destructive {
        background: #e06c75;
        color: #0e1117;
    }
    .btn-destructive:hover { filter: brightness(1.08); }
</style>
