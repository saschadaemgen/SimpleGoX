<script>
    import { settingsOpen, currentUserId, currentDeviceId, homeserver, recoveryKey, sendReadReceipts, sendTypingNotices } from '../lib/stores.js';
    import { loadSettings, loadRecoveryKey, doLogout, getOwnProfile, pickAndUploadAvatar, removeAvatar } from '../lib/tauri.js';
    import ColorPicker from './ColorPicker.svelte';
    import Avatar from './Avatar.svelte';
    import { onMount } from 'svelte';

    let visible = false;
    let rkVisible = false;
    let profile = null;

    async function handleAvatarUpload() {
        try {
            const mxc = await pickAndUploadAvatar();
            if (mxc) profile = await getOwnProfile();
        } catch (e) { console.error('Avatar upload:', e); }
    }

    async function handleRemoveAvatar() {
        try { await removeAvatar(); profile = await getOwnProfile(); }
        catch (e) { console.error('Remove avatar:', e); }
    }

    onMount(async () => {
        await loadSettings();
        await loadRecoveryKey();
        profile = await getOwnProfile();
        requestAnimationFrame(() => visible = true);
    });

    function close() { visible = false; setTimeout(() => settingsOpen.set(false), 250); }
    function backdrop(e) { if (e.target === e.currentTarget) close(); }
    function logout() { doLogout(); close(); }
    function toggleRk() { rkVisible = !rkVisible; }
    function copyRk() {
        if ($recoveryKey) navigator.clipboard.writeText($recoveryKey);
    }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" class:visible on:click={backdrop}>
    <div class="panel" class:visible>
        <div class="head">
            <h3>Settings</h3>
            <button class="x" on:click={close} title="Close">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
            </button>
        </div>

        <div class="body">
            <section>
                <h4>Profile</h4>
                <div class="profile-row">
                    <div class="profile-av-wrap">
                        <Avatar mxcUri={profile?.avatar_url} name={profile?.display_name || $currentUserId} size={56} borderRadius={14} editable={true} onUpload={handleAvatarUpload} />
                        {#if profile?.avatar_url}
                            <button class="rm-av" on:click={handleRemoveAvatar} title="Remove avatar"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="10" height="10"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg></button>
                        {/if}
                    </div>
                    <div class="profile-info">
                        <div class="profile-name">{profile?.display_name || 'No display name'}</div>
                        <div class="profile-uid">{$currentUserId || ''}</div>
                    </div>
                </div>
            </section>

            <section>
                <h4>Accent Color</h4>
                <ColorPicker />
            </section>

            <section>
                <h4>Privacy</h4>
                <label class="row">
                    <div>
                        <div class="lbl">Read Receipts</div>
                        <div class="desc">Let others know you read their messages</div>
                    </div>
                    <label class="stg"><input type="checkbox" bind:checked={$sendReadReceipts}><span class="stg-k"></span></label>
                </label>
                <label class="row">
                    <div>
                        <div class="lbl">Typing Notices</div>
                        <div class="desc">Show when you are typing</div>
                    </div>
                    <label class="stg"><input type="checkbox" bind:checked={$sendTypingNotices}><span class="stg-k"></span></label>
                </label>
            </section>

            <section>
                <h4>Security</h4>
                <div class="row">
                    <div class="lbl">End-to-End Encryption</div>
                    <span class="tag">Active</span>
                </div>
                <button class="row clickable" on:click={toggleRk}>
                    <div class="lbl">Recovery Key</div>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color:var(--text-3)"><polyline points="9 18 15 12 9 6"/></svg>
                </button>
                {#if rkVisible}
                    <div class="rk">
                        <code>{$recoveryKey || 'Not available'}</code>
                        <button class="cp-btn" on:click={copyRk}>Copy</button>
                    </div>
                {/if}
            </section>

            <section>
                <h4>Account</h4>
                <div class="row"><div class="lbl">User ID</div><div class="val mono">{$currentUserId || '-'}</div></div>
                <div class="row"><div class="lbl">Homeserver</div><div class="val mono">{$homeserver || '-'}</div></div>
                <div class="row"><div class="lbl">Device ID</div><div class="val mono">{$currentDeviceId || '-'}</div></div>
            </section>

            <section>
                <button class="logout" on:click={logout}>Log Out</button>
            </section>

            <div class="footer">SimpleGoX v0.1.0</div>
        </div>
    </div>
</div>

<style>
    .overlay {
        position: fixed; inset: 0; background: rgba(0, 0, 0, 0);
        backdrop-filter: blur(0px); z-index: 100; display: flex;
        align-items: center; justify-content: center;
        transition: background 250ms, backdrop-filter 250ms;
    }
    .overlay.visible { background: rgba(0, 0, 0, 0.6); backdrop-filter: blur(4px); }

    .panel {
        background: var(--bg-card); border: 1px solid var(--ac-border);
        border-radius: 14px; width: 380px; max-height: 80vh; overflow-y: auto;
        transform: scale(0.95) translateY(10px); opacity: 0;
        transition: all 300ms var(--ease);
    }
    .panel.visible { transform: scale(1) translateY(0); opacity: 1; }

    .head {
        padding: 16px 18px; display: flex; justify-content: space-between;
        align-items: center; border-bottom: 1px solid var(--border);
    }
    .head h3 { font-size: 0.95em; font-weight: 600; }
    .x {
        width: 30px; height: 30px; border-radius: 8px; border: none;
        background: transparent; color: var(--text-3); cursor: pointer;
        display: flex; align-items: center; justify-content: center;
    }
    .x:hover { background: var(--bg-hover); color: var(--text-2); }

    .body { padding: 0; }
    section { padding: 14px 18px; border-bottom: 1px solid var(--border); }
    section:last-of-type { border-bottom: none; }
    h4 {
        font-size: 0.72em; font-weight: 600; color: var(--text-3);
        text-transform: uppercase; letter-spacing: 1px; margin-bottom: 10px;
    }

    .row {
        display: flex; align-items: center; justify-content: space-between;
        padding: 8px 0; min-height: 36px; width: 100%; border: none;
        background: none; color: inherit; font-family: inherit; text-align: left;
    }
    .clickable { cursor: pointer; }
    .clickable:hover { opacity: 0.8; }

    .lbl { font-size: 0.86em; font-weight: 500; }
    .desc { font-size: 0.72em; color: var(--text-3); margin-top: 1px; }
    .val { font-size: 0.72em; color: var(--text-3); max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .mono { font-family: 'JetBrains Mono', monospace; }

    .tag {
        padding: 2px 8px; border-radius: 5px; font-size: 0.65em; font-weight: 600;
        background: var(--ac-bg); color: var(--ac); border: 1px solid var(--ac-border);
    }

    /* Toggle */
    .stg {
        width: 40px; height: 22px; border-radius: 11px; background: var(--bg);
        border: 1px solid var(--border-2); position: relative; cursor: pointer;
        transition: all 280ms var(--ease); flex-shrink: 0; display: block;
    }
    .stg input { position: absolute; width: 0; height: 0; opacity: 0; pointer-events: none; }
    .stg-k {
        position: absolute; width: 16px; height: 16px; border-radius: 50%;
        top: 2px; left: 2px; background: var(--text-3); pointer-events: none;
        transition: all 280ms var(--ease-b);
    }
    .stg input:checked ~ .stg-k { transform: translateX(18px); background: var(--ac); box-shadow: 0 0 5px rgba(63, 185, 168, 0.2); }
    .stg:has(input:checked) { border-color: var(--ac-border); }

    .rk {
        padding: 10px 14px; background: var(--bg); border: 1px solid var(--border-2);
        border-radius: 8px; margin-top: 8px; display: flex; align-items: center; gap: 10px;
    }
    .rk code { flex: 1; word-break: break-all; font-size: 0.78em; color: var(--ac); font-family: 'JetBrains Mono', monospace; }
    .cp-btn {
        padding: 5px 12px; border-radius: 6px; border: 1px solid var(--ac-border);
        background: var(--ac-bg); color: var(--ac); font-size: 0.78em; font-weight: 500;
        cursor: pointer; font-family: 'DM Sans', sans-serif;
    }
    .cp-btn:hover { background: var(--ac); color: var(--bg); }

    .logout {
        width: 100%; padding: 10px; border: 1px solid rgba(248, 81, 73, 0.2);
        border-radius: 9px; background: rgba(248, 81, 73, 0.06); color: var(--red);
        font-family: 'DM Sans', sans-serif; font-size: 0.88em; font-weight: 500;
        cursor: pointer; transition: all 150ms var(--ease);
    }
    .logout:hover { background: rgba(248, 81, 73, 0.12); }

    .profile-row { display: flex; gap: 14px; align-items: center; }
    .profile-av-wrap { position: relative; flex-shrink: 0; }
    .rm-av { position: absolute; top: -3px; right: -3px; width: 18px; height: 18px; border-radius: 50%; border: 2px solid var(--bg-card); background: var(--red); color: white; cursor: pointer; display: flex; align-items: center; justify-content: center; z-index: 1; }
    .rm-av:hover { transform: scale(1.15); }
    .profile-info { flex: 1; min-width: 0; }
    .profile-name { font-size: 0.95em; font-weight: 600; }
    .profile-uid { font-size: 0.72em; color: var(--text-3); font-family: 'JetBrains Mono', monospace; margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

    .footer { text-align: center; padding: 12px 0; font-size: 0.68em; color: var(--text-3); }
</style>
