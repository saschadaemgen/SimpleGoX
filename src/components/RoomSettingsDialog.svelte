<script>
    import { onMount } from 'svelte';
    import { currentRoomId, roomSettingsOpen } from '../lib/stores.js';
    import { getRoomSettings, getRoomMembers, setRoomName, setRoomTopic, setJoinRule, setHistoryVisibility, leaveRoom, loadRooms, pickAndUploadRoomAvatar } from '../lib/tauri.js';
    import Avatar from './Avatar.svelte';

    async function handleSettingsAvatarUpload() {
        if (!$currentRoomId) return;
        try { await pickAndUploadRoomAvatar($currentRoomId); settings = await getRoomSettings($currentRoomId); }
        catch (e) { console.error('Room avatar:', e); }
    }

    let settings = null;
    let members = [];
    let tab = 'general';
    let visible = false;
    let editName = '', editTopic = '', nameChanged = false, topicChanged = false, saving = false;

    const tabs = [
        { id: 'general', label: 'General' },
        { id: 'security', label: 'Security & Privacy' },
        { id: 'notifications', label: 'Notifications' },
        { id: 'advanced', label: 'Advanced' },
    ];

    onMount(async () => {
        if ($currentRoomId) {
            settings = await getRoomSettings($currentRoomId);
            members = await getRoomMembers($currentRoomId);
            if (settings) { editName = settings.name || ''; editTopic = settings.topic || ''; }
        }
        requestAnimationFrame(() => visible = true);
    });

    function close() { visible = false; setTimeout(() => roomSettingsOpen.set(false), 250); }
    function backdrop(e) { if (e.target === e.currentTarget) close(); }

    async function saveName() {
        if (!nameChanged || !$currentRoomId) return;
        saving = true;
        try { await setRoomName($currentRoomId, editName.trim()); settings = { ...settings, name: editName.trim() }; nameChanged = false; await loadRooms(); }
        catch (e) { console.error('Save name:', e); }
        saving = false;
    }

    async function saveTopic() {
        if (!topicChanged || !$currentRoomId) return;
        saving = true;
        try { await setRoomTopic($currentRoomId, editTopic.trim()); settings = { ...settings, topic: editTopic.trim() }; topicChanged = false; }
        catch (e) { console.error('Save topic:', e); }
        saving = false;
    }

    async function changeJoinRule(rule) { if ($currentRoomId) { try { await setJoinRule($currentRoomId, rule); settings = { ...settings, join_rule: rule }; } catch (e) { console.error(e); } } }
    async function changeHistory(vis) { if ($currentRoomId) { try { await setHistoryVisibility($currentRoomId, vis); settings = { ...settings, history_visibility: vis }; } catch (e) { console.error(e); } } }
    async function doLeave() { if ($currentRoomId && confirm(`Leave "${settings?.name || 'this room'}"?`)) { await leaveRoom($currentRoomId); close(); } }
</script>

<svelte:window on:keydown={e => e.key === 'Escape' && close()} />
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="bg" class:visible on:click={backdrop}>
    <div class="dlg" class:visible>
        <div class="hdr">
            <h2>Room Settings{settings?.name ? ` - ${settings.name}` : ''}</h2>
            <button class="x" on:click={close} title="Close"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg></button>
        </div>
        <div class="main">
            <nav class="nav">
                {#each tabs as t}
                    <button class="nav-btn" class:active={tab === t.id} on:click={() => tab = t.id}>{t.label}</button>
                {/each}
            </nav>
            <div class="content">
                {#if !settings}
                    <div class="loading">Loading...</div>
                {:else if tab === 'general'}
                    <h3>General</h3>
                    <div class="row2">
                        <div class="fields">
                            <div class="fld"><label>Room Name</label><input bind:value={editName} on:input={() => nameChanged = true} /></div>
                            <div class="fld"><label>Room Topic</label><textarea bind:value={editTopic} on:input={() => topicChanged = true} rows="3"></textarea></div>
                            {#if nameChanged || topicChanged}
                                <div class="save-row">
                                    <button class="btn-cancel" on:click={() => { editName = settings.name || ''; editTopic = settings.topic || ''; nameChanged = false; topicChanged = false; }}>Cancel</button>
                                    <button class="btn-save" on:click={() => { saveName(); saveTopic(); }} disabled={saving}>{saving ? 'Saving...' : 'Save'}</button>
                                </div>
                            {/if}
                        </div>
                        <div class="av-edit"><Avatar mxcUri={settings.avatar_url} name={settings.name} size={80} borderRadius={20} editable={true} onUpload={handleSettingsAvatarUpload} /></div>
                    </div>
                    <div class="div"></div>
                    <h3>Room Addresses</h3>
                    {#if settings.canonical_alias}
                        <div class="alias">{settings.canonical_alias}</div>
                    {:else}
                        <p class="desc">No published address.</p>
                    {/if}
                    <div class="div"></div>
                    <button class="btn-danger" on:click={doLeave}>Leave room</button>

                {:else if tab === 'security'}
                    <h3>Security & Privacy</h3>
                    <h4>Encryption</h4>
                    <div class="enc-row"><div class="sw-display" class:on={settings.is_encrypted}><div class="knob"></div></div><span>Encrypted</span></div>
                    <p class="desc">Once enabled, encryption cannot be disabled.</p>
                    <div class="div"></div>
                    <h4>Access</h4>
                    <label class="radio"><input type="radio" name="jr" value="invite" checked={settings.join_rule === 'invite'} on:change={() => changeJoinRule('invite')} /><div><strong>Invite only</strong><span class="rdesc">Only invited people can join.</span></div></label>
                    <label class="radio"><input type="radio" name="jr" value="public" checked={settings.join_rule === 'public'} on:change={() => changeJoinRule('public')} /><div><strong>Anyone</strong><span class="rdesc">Anyone can join.</span></div></label>
                    <div class="div"></div>
                    <h4>Who can read history?</h4>
                    <label class="radio"><input type="radio" name="hv" value="shared" checked={settings.history_visibility === 'shared' || settings.history_visibility === 'Shared'} on:change={() => changeHistory('shared')} /><div><strong>Members (full history)</strong></div></label>
                    <label class="radio"><input type="radio" name="hv" value="world_readable" checked={settings.history_visibility === 'world_readable' || settings.history_visibility === 'WorldReadable'} on:change={() => changeHistory('world_readable')} /><div><strong>Anyone (public history)</strong></div></label>

                {:else if tab === 'notifications'}
                    <h3>Notifications</h3>
                    <label class="radio"><input type="radio" name="nf" value="default" checked /><div><strong>Default</strong><span class="rdesc">As set in your settings.</span></div></label>
                    <label class="radio"><input type="radio" name="nf" value="all" /><div><strong>All messages</strong></div></label>
                    <label class="radio"><input type="radio" name="nf" value="mentions" /><div><strong>@mentions & keywords</strong></div></label>
                    <label class="radio"><input type="radio" name="nf" value="off" /><div><strong>Off</strong></div></label>

                {:else if tab === 'advanced'}
                    <h3>Advanced</h3>
                    <div class="info-fld"><label>Internal room ID</label><div class="copyable"><code>{settings.room_id}</code><button class="cp" on:click={() => navigator.clipboard.writeText(settings.room_id)} title="Copy"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg></button></div></div>
                    <div class="info-fld"><label>Room version</label><p>{settings.room_version}</p></div>
                    <div class="info-fld"><label>Members</label><p>{settings.member_count}</p></div>
                    <div class="info-fld"><label>Direct message</label><p>{settings.is_direct ? 'Yes' : 'No'}</p></div>
                {/if}
            </div>
        </div>
    </div>
</div>

<style>
    .bg { position: fixed; inset: 0; background: rgba(0,0,0,0); z-index: 200; display: flex; align-items: center; justify-content: center; transition: background 250ms; }
    .bg.visible { background: rgba(0,0,0,0.6); }
    .dlg { background: var(--bg); border: 1px solid var(--border-2); border-radius: 16px; width: 820px; max-width: 92vw; height: 600px; max-height: 85vh; display: flex; flex-direction: column; box-shadow: 0 20px 60px rgba(0,0,0,0.5); transform: scale(0.95); opacity: 0; transition: all 250ms var(--ease); overflow: hidden; }
    .dlg.visible { transform: scale(1); opacity: 1; }
    .hdr { display: flex; align-items: center; justify-content: space-between; padding: 20px 24px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
    .hdr h2 { font-size: 1em; font-weight: 600; }
    .x { width: 32px; height: 32px; border: none; background: transparent; color: var(--text-3); cursor: pointer; border-radius: 8px; display: flex; align-items: center; justify-content: center; }
    .x:hover { background: var(--bg-hover); color: var(--text); }
    .main { display: flex; flex: 1; overflow: hidden; }
    .nav { width: 200px; padding: 12px 8px; border-right: 1px solid var(--border); flex-shrink: 0; overflow-y: auto; }
    .nav-btn { display: flex; align-items: center; gap: 10px; width: 100%; padding: 10px 14px; border: none; background: transparent; color: var(--text-2); font-size: 0.86em; font-family: inherit; cursor: pointer; border-radius: 8px; text-align: left; transition: all 100ms; }
    .nav-btn:hover { background: var(--bg-hover); color: var(--text); }
    .nav-btn.active { background: var(--ac-bg); color: var(--ac); }
    .content { flex: 1; padding: 24px; overflow-y: auto; }
    .content h3 { font-size: 1.1em; font-weight: 700; margin: 0 0 16px; }
    .content h4 { font-size: 0.95em; font-weight: 700; margin: 0 0 8px; }
    .desc { font-size: 0.86em; color: var(--text-3); margin: 0 0 12px; line-height: 1.4; }
    .div { height: 1px; background: var(--border); margin: 24px 0; }
    .row2 { display: flex; gap: 24px; }
    .fields { flex: 1; }
    .av-edit { flex-shrink: 0; }
    .fld { margin-bottom: 12px; }
    .fld label { display: block; font-size: 0.72em; color: var(--text-3); margin-bottom: 4px; font-weight: 500; }
    .fld input, .fld textarea { width: 100%; padding: 10px 14px; border: 1px solid var(--border-2); background: var(--bg-card); color: var(--text); border-radius: 8px; font-size: 0.92em; font-family: inherit; outline: none; resize: vertical; }
    .fld input:focus, .fld textarea:focus { border-color: var(--ac-border); }
    .save-row { display: flex; gap: 8px; margin-top: 8px; }
    .btn-cancel { padding: 6px 14px; border-radius: 6px; border: 1px solid var(--border-2); background: transparent; color: var(--text-2); font-size: 0.82em; font-weight: 600; cursor: pointer; font-family: inherit; }
    .btn-save { padding: 6px 14px; border-radius: 6px; border: none; background: var(--ac); color: white; font-size: 0.82em; font-weight: 600; cursor: pointer; font-family: inherit; }
    .btn-save:disabled { opacity: 0.5; }
    .alias { padding: 10px 14px; border: 1px solid var(--border-2); background: var(--bg-card); border-radius: 8px; font-family: 'JetBrains Mono', monospace; font-size: 0.86em; }
    .btn-danger { padding: 10px 20px; border-radius: 8px; border: none; background: var(--red); color: white; font-size: 0.92em; font-weight: 600; cursor: pointer; font-family: inherit; }
    .btn-danger:hover { opacity: 0.9; }

    .enc-row { display: flex; align-items: center; gap: 10px; font-size: 0.92em; margin-bottom: 8px; }
    .sw-display { width: 44px; height: 24px; border-radius: 12px; background: var(--bg-raised); border: 1px solid var(--border-2); position: relative; flex-shrink: 0; }
    .sw-display.on { background: var(--ac); border-color: var(--ac); }
    .knob { width: 18px; height: 18px; border-radius: 50%; background: white; position: absolute; top: 2px; left: 2px; transition: transform 200ms; }
    .sw-display.on .knob { transform: translateX(20px); }

    .radio { display: flex; align-items: flex-start; gap: 12px; padding: 10px 0; cursor: pointer; }
    .radio input[type="radio"] { margin-top: 4px; accent-color: var(--ac); }
    .radio strong { display: block; font-size: 0.92em; }
    .rdesc { display: block; font-size: 0.78em; color: var(--text-3); margin-top: 2px; }

    .info-fld { margin-bottom: 16px; }
    .info-fld label { display: block; font-size: 0.78em; color: var(--text-3); margin-bottom: 4px; font-weight: 500; }
    .info-fld p { font-size: 0.92em; margin: 0; }
    .copyable { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border: 1px solid var(--border-2); background: var(--bg-card); border-radius: 8px; }
    .copyable code { font-family: 'JetBrains Mono', monospace; font-size: 0.78em; flex: 1; word-break: break-all; }
    .cp { border: none; background: transparent; color: var(--text-3); cursor: pointer; padding: 4px; border-radius: 4px; display: flex; }
    .cp:hover { color: var(--ac); background: var(--bg-hover); }
    .loading { text-align: center; color: var(--text-3); padding: 40px; font-size: 0.92em; }
</style>
