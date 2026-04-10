<script>
    import { currentUserId, homeserver, telegramConnected, telegramAuthOpen, telegramChats, telegramMessages } from '../../lib/stores.js';
    import { getOwnProfile, tgLogout, tgConnect } from '../../lib/tauri.js';
    import { invoke } from '@tauri-apps/api/core';
    import Avatar from '../Avatar.svelte';
    import { onMount } from 'svelte';

    let profile = null;
    let loggingOut = false;
    let addingAccount = false;
    onMount(async () => { profile = await getOwnProfile(); });

    async function handleAddAccount() {
        addingAccount = true;
        try {
            // Start sidecar if not running, then connect
            await invoke('tg_start_sidecar', { port: 50051 });
            await tgConnect(50051);
        } catch (e) {
            // Sidecar might already be running, try just connecting
            try { await tgConnect(50051); } catch (_) {}
        }
        addingAccount = false;
        telegramAuthOpen.set(true);
    }

    async function handleTgLogout() {
        loggingOut = true;
        try {
            await tgLogout();
            // Reset all Telegram state
            telegramConnected.set(false);
            telegramChats.set([]);
            telegramMessages.set({});
            console.log('Telegram logged out and state reset');
        } catch (e) {
            console.error('TG logout failed:', e);
        } finally {
            loggingOut = false;
        }
    }
</script>

<h3 class="tab-title">Accounts</h3>

<!-- Matrix Account -->
<div class="card">
    <div class="card-avatar">
        <Avatar mxcUri={profile?.avatar_url} name={profile?.display_name || $currentUserId} size={44} borderRadius={12} />
    </div>
    <div class="card-info">
        <div class="card-name">{profile?.display_name || $currentUserId || 'Matrix'}</div>
        <div class="card-detail">{$currentUserId || ''}</div>
        <div class="card-detail">{$homeserver || ''}</div>
        <div class="card-status connected"><span class="dot"></span> Connected</div>
    </div>
    <div class="card-badge mx">MX</div>
</div>

<!-- Telegram Account -->
{#if $telegramConnected}
    <div class="card">
        <div class="card-avatar placeholder">
            <span class="proto-badge tg">TG</span>
        </div>
        <div class="card-info">
            <div class="card-name">Telegram</div>
            <div class="card-status connected"><span class="dot"></span> Connected</div>
        </div>
        <div class="card-actions">
            <button class="act-btn danger" on:click={handleTgLogout} disabled={loggingOut}>
                {loggingOut ? 'Signing out...' : 'Sign Out'}
            </button>
        </div>
    </div>
{:else}
    <div class="card add">
        <div class="card-avatar placeholder">
            <span class="proto-badge tg">TG</span>
        </div>
        <div class="card-info">
            <div class="card-name">Telegram</div>
            <div class="card-detail">Not connected</div>
        </div>
        <div class="card-actions">
            <button class="act-btn primary" on:click={handleAddAccount} disabled={addingAccount}>
                {addingAccount ? 'Starting...' : '+ Add Account'}
            </button>
        </div>
    </div>
{/if}

<!-- Future Protocols -->
<div class="card disabled">
    <div class="card-avatar placeholder"><span class="proto-badge sx">SX</span></div>
    <div class="card-info">
        <div class="card-name">SimpleX</div>
        <div class="card-detail dimmed">Coming in Season 3</div>
    </div>
</div>
<div class="card disabled">
    <div class="card-avatar placeholder"><span class="proto-badge wa">WA</span></div>
    <div class="card-info">
        <div class="card-name">WhatsApp</div>
        <div class="card-detail dimmed">Coming in Season 4</div>
    </div>
</div>

<style>
    .tab-title { font-size: 1.1em; font-weight: 600; margin: 0 0 20px; }

    .card {
        display: flex; align-items: center; gap: 14px; padding: 14px 16px;
        border-radius: 12px; background: rgba(255,255,255,0.02);
        border: 1px solid rgba(255,255,255,0.04); margin-bottom: 10px;
        transition: background 0.15s;
    }
    .card:hover:not(.disabled) { background: rgba(255,255,255,0.04); }
    .card.disabled { opacity: 0.4; pointer-events: none; }

    .card-avatar { flex-shrink: 0; }
    .card-avatar.placeholder {
        width: 44px; height: 44px; border-radius: 12px;
        display: flex; align-items: center; justify-content: center;
        background: rgba(255,255,255,0.04);
    }

    .card-info { flex: 1; min-width: 0; }
    .card-name { font-size: 0.9em; font-weight: 600; }
    .card-detail { font-size: 0.75em; color: #8b949e; margin-top: 1px; font-family: 'JetBrains Mono', monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .card-detail.dimmed { font-style: italic; font-family: inherit; }

    .card-status { font-size: 0.72em; margin-top: 3px; display: flex; align-items: center; gap: 5px; }
    .dot { width: 7px; height: 7px; border-radius: 50%; }
    .connected { color: #98c379; }
    .connected .dot { background: #98c379; }

    .card-badge, .proto-badge {
        padding: 3px 8px; border-radius: 6px; font-size: 0.65em;
        font-weight: 700; letter-spacing: 0.5px; flex-shrink: 0;
    }
    .mx { background: rgba(63,185,168,0.15); color: var(--ac, #3fb9a8); }
    .tg { background: rgba(97,175,239,0.15); color: #61afef; }
    .sx { background: rgba(198,120,221,0.15); color: #c678dd; }
    .wa { background: rgba(152,195,121,0.15); color: #98c379; }

    .card-actions { flex-shrink: 0; }
    .act-btn {
        padding: 6px 14px; border-radius: 8px; border: none;
        font-size: 0.78em; font-weight: 600; font-family: inherit; cursor: pointer;
        transition: all 0.15s;
    }
    .act-btn.primary { background: var(--ac, #3fb9a8); color: #0e1117; }
    .act-btn.primary:hover { filter: brightness(1.15); }
    .act-btn.danger { background: rgba(224,108,117,0.1); color: #e06c75; }
    .act-btn.danger:hover { background: rgba(224,108,117,0.2); }
</style>
