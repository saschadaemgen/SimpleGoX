<script>
    import { sidebarCollapsed, settingsOpen, currentUserId, createRoomDialogOpen, joinRoomDialogOpen, createDmDialogOpen } from '../lib/stores.js';
    import RoomList from './RoomList.svelte';

    function toggle() { sidebarCollapsed.update(v => !v); }
    function openSettings() { settingsOpen.set(true); }
</script>

<aside class="sidebar" class:collapsed={$sidebarCollapsed}>
    <div class="top">
        <span class="logo">Simple<span>GoX</span></span>
        <button class="ic" on:click={openSettings} title="Settings">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
        </button>
    </div>
    <div class="sep"></div>
    <div class="actions">
        <button class="ic" on:click={() => createRoomDialogOpen.set(true)} title="Create Room">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
        </button>
        <button class="ic" on:click={() => joinRoomDialogOpen.set(true)} title="Join Room">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><polyline points="10 17 15 12 10 7"/><line x1="15" y1="12" x2="3" y2="12"/></svg>
        </button>
        <button class="ic" on:click={() => createDmDialogOpen.set(true)} title="Direct Message">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
        </button>
    </div>
    <RoomList />
    <div class="toggle-row">
        <button class="ic" on:click={toggle} title="Collapse">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="chevron"><polyline points="15 18 9 12 15 6"/></svg>
        </button>
    </div>
    <div class="foot">
        <div class="u-av">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>
            <div class="dot"></div>
        </div>
        <span class="uid">{$currentUserId || ''}</span>
    </div>
</aside>

<style>
    .sidebar {
        width: var(--sidebar-w); min-width: var(--sidebar-w);
        background: var(--bg-card); display: flex; flex-direction: column;
        border-right: 1px solid var(--ac-border); overflow: hidden;
        transition: width 280ms var(--ease), min-width 280ms var(--ease);
    }
    .sidebar.collapsed { width: var(--sidebar-collapsed); min-width: var(--sidebar-collapsed); }

    .top {
        padding: 14px; display: flex; justify-content: space-between;
        align-items: center; min-height: 52px;
    }
    .logo {
        font-size: 1.05em; font-weight: 700; letter-spacing: -0.5px;
        white-space: nowrap; overflow: hidden; transition: opacity 200ms;
    }
    .logo span { color: var(--ac); }
    .sidebar.collapsed .logo { opacity: 0; width: 0; }

    .ic {
        width: 30px; height: 30px; border-radius: 8px; border: none;
        background: transparent; color: var(--text-3); cursor: pointer;
        display: flex; align-items: center; justify-content: center;
        transition: all 120ms ease; flex-shrink: 0;
    }
    .ic:hover { background: var(--bg-hover); color: var(--text-2); }

    .actions { display: flex; gap: 4px; padding: 8px 10px 4px; }
    .sidebar.collapsed .actions { flex-direction: column; padding: 4px 6px; }

    .sep { height: 1px; background: var(--border); margin: 0 14px; }
    .sidebar.collapsed .sep { margin: 0 8px; }

    .toggle-row {
        padding: 10px 14px; border-top: 1px solid var(--border);
        display: flex; justify-content: flex-end;
    }
    .sidebar.collapsed .toggle-row { justify-content: center; }
    .chevron { transition: transform 250ms var(--ease); }
    .sidebar.collapsed .chevron { transform: rotate(180deg); }

    .foot {
        padding: 10px 12px; border-top: 1px solid var(--border);
        display: flex; align-items: center; gap: 8px; overflow: hidden;
    }
    .u-av {
        width: 28px; height: 28px; border-radius: 8px; background: var(--bg-raised);
        border: 1px solid var(--border-2); display: flex; align-items: center;
        justify-content: center; position: relative; flex-shrink: 0;
    }
    .dot {
        position: absolute; bottom: -2px; right: -2px; width: 8px; height: 8px;
        border-radius: 50%; background: var(--green); border: 2px solid var(--bg-card);
    }
    .uid {
        font-family: 'JetBrains Mono', monospace; font-size: 0.7em; color: var(--text-3);
        white-space: nowrap; overflow: hidden; transition: opacity 200ms;
    }
    .sidebar.collapsed .uid { opacity: 0; }
</style>
