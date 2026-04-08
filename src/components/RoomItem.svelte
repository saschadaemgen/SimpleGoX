<script>
    import { messages, sidebarCollapsed, contextMenu } from '../lib/stores.js';
    import Avatar from './Avatar.svelte';

    export let room;
    export let active = false;
    export let onclick = () => {};

    function onCtx(e) {
        e.preventDefault();
        e.stopPropagation();
        contextMenu.set({ visible: true, x: e.clientX, y: e.clientY, room });
    }

    $: preview = (() => {
        const buf = $messages[room.room_id] || [];
        if (!buf.length) return '';
        const t = buf[buf.length - 1].body;
        return t.length > 35 ? t.slice(0, 35) + '...' : t;
    })();

    $: isIot = room.name.toLowerCase().includes('iot');
    $: unread = room.unread_count || 0;
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<button class="rm" class:sel={active} on:click={onclick} on:contextmenu={onCtx}>
    <Avatar mxcUri={room.avatar_url} name={room.name} size={36} borderRadius={10} />
    <div class="body">
        <div class="nm">{room.name}</div>
        <div class="last">{preview}</div>
    </div>
    <div class="side">
        {#if unread > 0}
            <div class="badge">{unread}</div>
        {/if}
        {#if room.is_encrypted}
            <span class="e2e">E2EE</span>
        {/if}
    </div>
</button>

<style>
    .rm {
        display: flex; align-items: center; gap: 10px; padding: 8px 10px;
        border-radius: 9px; cursor: pointer; position: relative; width: 100%;
        border: none; background: transparent; text-align: left; color: inherit;
        font-family: inherit;
        transition: all 120ms ease;
        animation: rmIn 250ms var(--ease) both;
    }
    @keyframes rmIn { from { opacity: 0; transform: translateX(-5px); } to { opacity: 1; transform: translateX(0); } }

    .rm:hover { background: var(--bg-hover); }
    .rm.sel { background: var(--ac-bg); }

    .rm.sel::before {
        content: ''; position: absolute; left: 0; top: 10px; bottom: 10px;
        width: 2px; border-radius: 0 2px 2px 0; background: var(--ac);
        animation: barIn 200ms var(--ease);
    }
    @keyframes barIn { from { transform: scaleY(0); } to { transform: scaleY(1); } }

    .body { flex: 1; min-width: 0; overflow: hidden; transition: opacity 200ms, width 200ms; }
    :global(.sidebar.collapsed) .body { opacity: 0; width: 0; }

    .nm { font-size: 0.86em; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .last { font-size: 0.73em; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }

    .side { display: flex; flex-direction: column; align-items: flex-end; gap: 4px; flex-shrink: 0; }
    :global(.sidebar.collapsed) .side { display: none; }

    .badge {
        min-width: 17px; height: 17px; border-radius: 9px; background: var(--ac);
        color: var(--bg); font-size: 0.62em; font-weight: 700;
        display: flex; align-items: center; justify-content: center; padding: 0 4px;
        animation: pulse 2s ease infinite;
    }
    @keyframes pulse {
        0%, 100% { box-shadow: 0 0 0 0 rgba(63, 185, 168, 0.3); }
        50% { box-shadow: 0 0 0 5px rgba(63, 185, 168, 0); }
    }

    .e2e {
        font-size: 0.55em; font-weight: 700; letter-spacing: 0.5px;
        padding: 1px 5px; border-radius: 4px;
        background: var(--ac-bg); color: var(--ac); border: 1px solid var(--ac-border);
    }

    /* Collapsed sidebar: glow on active icon */
    :global(.sidebar.collapsed) .rm.sel::before { display: none; }
    :global(.sidebar.collapsed) .rm.sel :global(.av) { border-color: var(--ac-border); box-shadow: 0 0 8px var(--ac-glow); }
</style>
