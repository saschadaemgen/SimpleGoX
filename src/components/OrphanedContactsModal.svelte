<script>
    // Briefing 045a: top-level orphan-cleanup modal. Rendered by
    // App.svelte when sx_get_profile reports no profile but
    // sx_list_contacts returns a non-zero row count. Those contact
    // rows are bound to cryptographic keys from a previous (now
    // missing) profile and cannot exchange messages with anyone, so
    // there is no recovery path - they must be wiped before
    // SimpleGoX continues. Hence: single button, no escape hatch.

    export let count = 0;
    export let onConfirm = () => {};

    let busy = false;
    async function handleConfirm() {
        if (busy) return;
        busy = true;
        try {
            await onConfirm();
        } finally {
            // App.svelte unmounts the modal on success; on failure it
            // routes to the error startup screen. Either way `busy`
            // does not need a fall-back reset, but keep it defensive.
            busy = false;
        }
    }
</script>

<div class="overlay" role="dialog" aria-modal="true" aria-labelledby="orphaned-title">
    <div class="modal">
        <h2 id="orphaned-title">{count} orphaned SimpleX {count === 1 ? 'contact' : 'contacts'} found</h2>
        <p>
            Your SimpleX profile is missing, but {count}
            {count === 1 ? 'contact is' : 'contacts are'} still on disk.
            Without your previous profile keys, {count === 1 ? 'it' : 'they'}
            cannot exchange messages and cannot be recovered.
        </p>
        <p class="emph">
            {count === 1 ? 'It' : 'They'} will be permanently removed before
            SimpleGoX continues.
        </p>
        <div class="actions">
            <button class="btn btn-destructive" on:click={handleConfirm} disabled={busy}>
                {busy ? 'Wiping...' : 'Wipe orphaned contacts and continue'}
            </button>
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
    p {
        font-size: 13px;
        line-height: 1.5;
        color: #b1bac4;
        margin: 0 0 14px;
    }
    p.emph {
        color: #d4d7dd;
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
    .btn-destructive {
        background: #e06c75;
        color: #0e1117;
    }
    .btn-destructive:hover:not(:disabled) {
        filter: brightness(1.08);
    }
    .btn-destructive:disabled {
        opacity: 0.6;
        cursor: progress;
    }
</style>
