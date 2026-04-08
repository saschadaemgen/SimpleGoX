<script>
    import { kickUser, banUser, createDm } from '../lib/tauri.js';
    import { confirmDialog } from '../lib/stores.js';
    import { displayName } from '../lib/utils.js';
    import Avatar from './Avatar.svelte';

    export let member;
    export let currentUserId = '';
    export let canKick = false;
    export let canBan = false;
    export let roomId = '';

    let showActions = false;
    $: isMe = member.user_id === currentUserId;
    $: name = member.display_name || displayName(member.user_id);
    $: powerLabel = member.power_level >= 100 ? 'Admin' : member.power_level >= 50 ? 'Mod' : null;

    function dm() { createDm(member.user_id); showActions = false; }

    function kick() {
        showActions = false;
        confirmDialog.set({ visible: true, title: 'Kick User', message: `Remove ${name} from this room?`, confirmText: 'Kick', danger: true, onConfirm: () => kickUser(roomId, member.user_id, null) });
    }

    function ban() {
        showActions = false;
        confirmDialog.set({ visible: true, title: 'Ban User', message: `Ban ${name} from this room?`, confirmText: 'Ban', danger: true, onConfirm: () => banUser(roomId, member.user_id, null) });
    }
</script>

<div class="member" class:me={isMe}>
    <Avatar mxcUri={member.avatar_url} name={name} size={32} borderRadius={8} />
    <div class="info">
        <span class="name">{name}{#if isMe}<span class="you">you</span>{/if}</span>
        <span class="uid">{member.user_id}</span>
    </div>
    {#if powerLabel}<span class="pl">{powerLabel}</span>{/if}
    {#if !isMe}
        <button class="more" on:click={() => showActions = !showActions} title="Actions">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><circle cx="12" cy="5" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="12" cy="19" r="1"/></svg>
        </button>
        {#if showActions}
            <div class="acts">
                <button on:click={dm}>Message</button>
                {#if canKick}<button class="danger" on:click={kick}>Kick</button>{/if}
                {#if canBan}<button class="danger" on:click={ban}>Ban</button>{/if}
            </div>
        {/if}
    {/if}
</div>

<style>
    .member { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 8px; position: relative; transition: background 100ms; }
    .member:hover { background: var(--bg-hover); }
    .info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
    .name { font-size: 0.86em; font-weight: 600; display: flex; align-items: center; gap: 6px; }
    .you { font-size: 0.72em; font-weight: 500; color: var(--ac); background: var(--ac-bg); padding: 1px 6px; border-radius: 4px; }
    .uid { font-size: 0.68em; color: var(--text-3); font-family: 'JetBrains Mono', monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .pl { font-size: 0.65em; font-weight: 600; color: var(--ac); background: var(--ac-bg); padding: 2px 8px; border-radius: 4px; flex-shrink: 0; }
    .more { width: 24px; height: 24px; border: none; background: transparent; color: var(--text-3); cursor: pointer; border-radius: 4px; display: flex; align-items: center; justify-content: center; opacity: 0; transition: opacity 100ms; }
    .member:hover .more { opacity: 1; }
    .more:hover { background: var(--bg-hover); color: var(--text); }
    .acts { position: absolute; right: 8px; top: 100%; z-index: 10; background: var(--bg-card); border: 1px solid var(--border-2); border-radius: 8px; padding: 4px; box-shadow: 0 4px 12px rgba(0,0,0,0.3); min-width: 100px; }
    .acts button { padding: 6px 12px; border: none; background: transparent; color: var(--text); font-size: 0.78em; font-family: inherit; cursor: pointer; border-radius: 4px; text-align: left; width: 100%; }
    .acts button:hover { background: var(--bg-hover); }
    .acts button.danger { color: var(--red); }
    .acts button.danger:hover { background: rgba(248,81,73,0.1); }
</style>
