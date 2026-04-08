<script>
    import { roomInfoOpen, roomInfoData, roomMembers, currentRoomId, currentUserId } from '../lib/stores.js';
    import { loadRoomInfo, inviteUser, setRoomName, setRoomTopic, pickAndUploadRoomAvatar, removeRoomAvatar, getRoomInfo } from '../lib/tauri.js';
    import MemberItem from './MemberItem.svelte';
    import Avatar from './Avatar.svelte';

    async function handleRoomAvatarUpload() {
        if (!$currentRoomId) return;
        try { await pickAndUploadRoomAvatar($currentRoomId); const info = await getRoomInfo($currentRoomId); if (info) roomInfoData.set(info); }
        catch (e) { console.error('Room avatar upload:', e); }
    }

    let inviteInput = '', inviteLoading = false, inviteError = null;
    let editingName = false, editingTopic = false, nameInput = '', topicInput = '';

    $: if ($roomInfoOpen && $currentRoomId) loadRoomInfo($currentRoomId);

    function close() { roomInfoOpen.set(false); }

    async function doInvite() {
        if (!inviteInput.trim() || !$currentRoomId) return;
        inviteLoading = true; inviteError = null;
        try { await inviteUser($currentRoomId, inviteInput.trim()); inviteInput = ''; await loadRoomInfo($currentRoomId); }
        catch (e) { inviteError = String(e); }
        finally { inviteLoading = false; }
    }

    function startEditName() { nameInput = $roomInfoData?.name || ''; editingName = true; }
    async function saveName() { if (nameInput.trim() && $currentRoomId) { await setRoomName($currentRoomId, nameInput.trim()); editingName = false; await loadRoomInfo($currentRoomId); } }

    function startEditTopic() { topicInput = $roomInfoData?.topic || ''; editingTopic = true; }
    async function saveTopic() { if ($currentRoomId) { await setRoomTopic($currentRoomId, topicInput.trim()); editingTopic = false; await loadRoomInfo($currentRoomId); } }
</script>

{#if $roomInfoOpen}
    <aside class="panel">
        <div class="head">
            <h3>Room Info</h3>
            <button class="x" on:click={close} title="Close">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
            </button>
        </div>

        {#if $roomInfoData}
            <div class="body">
                <div class="av-lg-wrap">
                    <!-- TODO: editable should check power levels -->
                    <Avatar mxcUri={$roomInfoData?.avatar_url} name={$roomInfoData?.name} size={64} borderRadius={16} editable={true} onUpload={handleRoomAvatarUpload} />
                </div>

                <section>
                    {#if editingName}
                        <input class="edit-input" bind:value={nameInput} on:keydown={e => e.key === 'Enter' && saveName()} on:blur={saveName} />
                    {:else}
                        <div class="name-row">
                            <h2>{$roomInfoData.name || 'Unnamed Room'}</h2>
                            <button class="edit-btn" on:click={startEditName} title="Edit name">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                            </button>
                        </div>
                    {/if}
                </section>

                <section>
                    <div class="label">Topic</div>
                    {#if editingTopic}
                        <textarea class="edit-input" bind:value={topicInput} on:keydown={e => e.key === 'Enter' && !e.shiftKey && saveTopic()} on:blur={saveTopic} rows="3"></textarea>
                    {:else}
                        <p class="topic">{$roomInfoData.topic || 'No topic set'}</p>
                        <button class="edit-btn-sm" on:click={startEditTopic}>Edit</button>
                    {/if}
                </section>

                <section class="badges">
                    {#if $roomInfoData.is_encrypted}<span class="badge ac"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="12" height="12"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg> Encrypted</span>{/if}
                    {#if $roomInfoData.is_direct}<span class="badge">DM</span>{/if}
                    <span class="badge">{$roomInfoData.member_count} members</span>
                </section>

                <section>
                    <div class="label">Invite</div>
                    <div class="inv-row">
                        <input bind:value={inviteInput} placeholder="@user:server.com" on:keydown={e => e.key === 'Enter' && doInvite()} />
                        <button class="inv-btn" on:click={doInvite} disabled={!inviteInput.trim() || inviteLoading}>{inviteLoading ? '...' : 'Invite'}</button>
                    </div>
                    {#if inviteError}<div class="err">{inviteError}</div>{/if}
                </section>

                <section>
                    <div class="label">Members ({$roomMembers.length})</div>
                    <div class="members">
                        {#each $roomMembers as member (member.user_id)}
                            <MemberItem {member} currentUserId={$currentUserId} canKick={true} canBan={true} roomId={$currentRoomId} />
                        {/each}
                    </div>
                </section>
            </div>
        {:else}
            <div class="loading">Loading...</div>
        {/if}
    </aside>
{/if}

<style>
    .panel { width: 300px; background: var(--bg-card); border-left: 1px solid var(--ac-border); overflow-y: auto; flex-shrink: 0; animation: slideIn 250ms var(--ease); }
    @keyframes slideIn { from { transform: translateX(100%); } to { transform: translateX(0); } }
    .head { display: flex; align-items: center; justify-content: space-between; padding: 16px; border-bottom: 1px solid var(--border); }
    .head h3 { font-size: 0.9em; font-weight: 600; }
    .x { width: 28px; height: 28px; border: none; background: transparent; color: var(--text-3); cursor: pointer; border-radius: 6px; display: flex; align-items: center; justify-content: center; }
    .x:hover { background: var(--bg-hover); }
    .body { padding: 20px 16px; }
    .av-lg-wrap { display: flex; justify-content: center; margin-bottom: 16px; }
    section { margin-bottom: 20px; }
    .label { font-size: 0.68em; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; color: var(--text-3); margin-bottom: 8px; }
    .name-row { display: flex; align-items: center; justify-content: center; gap: 8px; }
    .name-row h2 { font-size: 1.1em; font-weight: 700; text-align: center; }
    .edit-btn { border: none; background: transparent; color: var(--text-3); cursor: pointer; padding: 4px; border-radius: 4px; }
    .edit-btn:hover { color: var(--ac); background: var(--bg-hover); }
    .edit-btn-sm { border: none; background: transparent; color: var(--text-3); cursor: pointer; font-size: 0.72em; padding: 2px 6px; border-radius: 4px; margin-top: 4px; }
    .edit-btn-sm:hover { color: var(--ac); }
    .topic { font-size: 0.86em; color: var(--text-2); line-height: 1.4; }
    .edit-input { width: 100%; padding: 8px 10px; border: 1px solid var(--ac-border); background: var(--bg-input); color: var(--text); border-radius: 6px; font-size: 0.86em; font-family: inherit; outline: none; resize: vertical; }
    .badges { display: flex; flex-wrap: wrap; gap: 6px; }
    .badge { font-size: 0.72em; padding: 4px 8px; border-radius: 6px; background: var(--bg-raised); color: var(--text-2); display: flex; align-items: center; gap: 4px; }
    .badge.ac { color: var(--ac); }
    .inv-row { display: flex; gap: 6px; }
    .inv-row input { flex: 1; padding: 8px 10px; border: 1px solid var(--border-2); background: var(--bg-input); color: var(--text); border-radius: 6px; font-size: 0.78em; font-family: 'JetBrains Mono', monospace; outline: none; }
    .inv-row input:focus { border-color: var(--ac-border); }
    .inv-btn { padding: 8px 12px; border-radius: 6px; border: none; background: var(--ac); color: white; font-size: 0.78em; font-weight: 600; cursor: pointer; white-space: nowrap; font-family: inherit; }
    .inv-btn:disabled { opacity: 0.5; cursor: not-allowed; }
    .err { color: var(--red); font-size: 0.72em; margin-top: 4px; }
    .members { display: flex; flex-direction: column; gap: 2px; }
    .loading { text-align: center; color: var(--text-3); padding: 40px 16px; font-size: 0.86em; }
</style>
