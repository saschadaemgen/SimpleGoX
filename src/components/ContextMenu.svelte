<script>
    import { contextMenu, confirmDialog, currentRoomId, roomSettingsOpen } from '../lib/stores.js';
    import { leaveRoom, setRoomTag, removeRoomTag } from '../lib/tauri.js';
    import { onMount, onDestroy } from 'svelte';

    let el;

    function close() { contextMenu.update(c => ({ ...c, visible: false })); }

    function onClick(e) { if (el && !el.contains(e.target)) close(); }

    onMount(() => { document.addEventListener('click', onClick); document.addEventListener('contextmenu', onClick); });
    onDestroy(() => { document.removeEventListener('click', onClick); document.removeEventListener('contextmenu', onClick); });

    async function toggleFav() {
        const r = $contextMenu.room;
        if (r.is_favourite) await removeRoomTag(r.room_id, 'm.favourite');
        else await setRoomTag(r.room_id, 'm.favourite', 0.5);
        close();
    }

    async function toggleMute() {
        const r = $contextMenu.room;
        if (r.is_muted) await removeRoomTag(r.room_id, 'm.lowpriority');
        else await setRoomTag(r.room_id, 'm.lowpriority', null);
        close();
    }

    function openSettings() {
        const r = $contextMenu.room;
        currentRoomId.set(r.room_id);
        roomSettingsOpen.set(true);
        close();
    }

    function leave() {
        const r = $contextMenu.room;
        close();
        confirmDialog.set({ visible: true, title: 'Leave Room', message: `Leave "${r.name}"?`, confirmText: 'Leave', danger: true, onConfirm: () => leaveRoom(r.room_id) });
    }
</script>

{#if $contextMenu.visible}
    <div class="ctx" bind:this={el} style="left:{$contextMenu.x}px;top:{$contextMenu.y}px">
        <button class="item" on:click={toggleFav}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>
            {$contextMenu.room?.is_favourite ? 'Remove Favourite' : 'Favourite'}
        </button>
        <button class="item" on:click={toggleMute}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/></svg>
            {$contextMenu.room?.is_muted ? 'Unmute' : 'Mute'}
        </button>
        <button class="item" on:click={openSettings}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
            Settings
        </button>
        <div class="sep"></div>
        <button class="item danger" on:click={leave}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>
            Leave Room
        </button>
    </div>
{/if}

<style>
    .ctx { position: fixed; z-index: 200; background: var(--bg-card); border: 1px solid var(--border-2); border-radius: 10px; padding: 6px; min-width: 180px; box-shadow: 0 8px 24px rgba(0,0,0,0.4); animation: ctxIn 120ms var(--ease); }
    @keyframes ctxIn { from { opacity: 0; transform: scale(0.95); } to { opacity: 1; transform: scale(1); } }
    .item { display: flex; align-items: center; gap: 10px; width: 100%; padding: 8px 12px; border: none; background: transparent; color: var(--text); font-size: 0.86em; font-family: inherit; cursor: pointer; border-radius: 6px; text-align: left; }
    .item:hover { background: var(--bg-hover); }
    .item.danger { color: var(--red); }
    .item.danger:hover { background: rgba(248,81,73,0.1); }
    .sep { height: 1px; background: var(--border); margin: 4px 8px; }
</style>
