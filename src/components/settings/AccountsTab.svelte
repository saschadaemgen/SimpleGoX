<script>
    import { currentUserId, homeserver, isLoggedIn, rooms, messages, telegramConnected, telegramAuthOpen, telegramChats, telegramMessages } from '../../lib/stores.js';
    import { getOwnProfile, tgRemoveAccount, tgConnect, doLogout } from '../../lib/tauri.js';
    import { invoke } from '@tauri-apps/api/core';
    import Avatar from '../Avatar.svelte';
    import Tooltip from '../ui/Tooltip.svelte';
    import { onMount } from 'svelte';

    let profile = null;
    let disconnectingTg = false;
    let disconnectingMx = false;
    let addingAccount = false;
    let showTgConfirm = false;
    let showMxConfirm = false;
    onMount(async () => {
        try { profile = await getOwnProfile(); } catch (_) {}
    });

    async function handleAddAccount() {
        addingAccount = true;
        try {
            await invoke('tg_start_sidecar', { port: 50051 });
            await tgConnect(50051);
        } catch (e) {
            try { await tgConnect(50051); } catch (_) {}
        }
        addingAccount = false;
        telegramAuthOpen.set(true);
    }

    async function confirmTgDisconnect() {
        showTgConfirm = false;
        disconnectingTg = true;
        try {
            await tgRemoveAccount();
            telegramConnected.set(false);
            telegramChats.set([]);
            telegramMessages.set({});
            localStorage.removeItem('sgx-tg-chats');
        } catch (e) {
            console.error('TG disconnect failed:', e);
        } finally {
            disconnectingTg = false;
        }
    }

    async function confirmMxDisconnect() {
        showMxConfirm = false;
        disconnectingMx = true;
        try {
            await doLogout();
            rooms.set([]);
            messages.set({});
            isLoggedIn.set(false);
            profile = null;
        } catch (e) {
            console.error('Matrix disconnect failed:', e);
        } finally {
            disconnectingMx = false;
        }
    }
</script>

<h3 class="tab-title">Accounts</h3>

<!-- Matrix Account -->
{#if $isLoggedIn}
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
        <div class="card-actions">
            <button class="act-btn danger" on:click={() => showMxConfirm = true} disabled={disconnectingMx}>
                {disconnectingMx ? 'Disconnecting...' : 'Disconnect'}
            </button>
        </div>
    </div>
{:else}
    <div class="card add">
        <div class="card-avatar placeholder"><span class="proto-badge mx">MX</span></div>
        <div class="card-info">
            <div class="card-name">Matrix<Tooltip text="Your primary Matrix account. Messages are encrypted end-to-end using vodozemac." /></div>
            <div class="card-detail">Not connected</div>
        </div>
    </div>
{/if}

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
            <button class="act-btn danger" on:click={() => showTgConfirm = true} disabled={disconnectingTg}>
                {disconnectingTg ? 'Disconnecting...' : 'Disconnect'}
            </button>
        </div>
    </div>
{:else}
    <div class="card add">
        <div class="card-avatar placeholder">
            <span class="proto-badge tg">TG</span>
        </div>
        <div class="card-info">
            <div class="card-name">Telegram<Tooltip text="Connect an existing Telegram account. Your session runs locally via TDLib - credentials never leave your device." /></div>
            <div class="card-detail">Not connected</div>
        </div>
        <div class="card-actions">
            <button class="act-btn primary" on:click={handleAddAccount} disabled={addingAccount}>
                {addingAccount ? 'Starting...' : '+ Add Account'}
            </button>
        </div>
    </div>
{/if}

<!-- Matrix Confirm Dialog -->
{#if showMxConfirm}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="confirm-overlay" on:click={() => showMxConfirm = false}>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="confirm-dialog" on:click|stopPropagation>
            <h4>Disconnect Matrix</h4>
            <p>You will be logged out and your encryption keys will be removed from this device. You can reconnect anytime with your credentials.</p>
            <div class="confirm-actions">
                <button class="act-btn secondary" on:click={() => showMxConfirm = false}>Cancel</button>
                <button class="act-btn danger" on:click={confirmMxDisconnect}>Disconnect</button>
            </div>
        </div>
    </div>
{/if}

<!-- Telegram Confirm Dialog -->
{#if showTgConfirm}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="confirm-overlay" on:click={() => showTgConfirm = false}>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="confirm-dialog" on:click|stopPropagation>
            <h4>Disconnect Telegram</h4>
            <p>Disconnect Telegram? Your chat history will be removed from SimpleGoX. You can reconnect anytime with your phone number.</p>
            <div class="confirm-actions">
                <button class="act-btn secondary" on:click={() => showTgConfirm = false}>Cancel</button>
                <button class="act-btn danger" on:click={confirmTgDisconnect}>Disconnect</button>
            </div>
        </div>
    </div>
{/if}

<!-- Future Protocols -->
<div class="card disabled">
    <div class="card-avatar placeholder"><span class="proto-badge sx">SX</span></div>
    <div class="card-info">
        <div class="card-name">SimpleX<Tooltip text="SimpleX protocol support is planned for Season 3." /></div>
        <div class="card-detail dimmed">Coming in Season 3</div>
    </div>
</div>
<div class="card disabled">
    <div class="card-avatar placeholder"><span class="proto-badge wa">WA</span></div>
    <div class="card-info">
        <div class="card-name">WhatsApp<Tooltip text="WhatsApp support via EU DMA interoperability is planned for Season 4." /></div>
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

    .card-badge, .proto-badge { padding: 3px 8px; border-radius: 6px; font-size: 0.65em; font-weight: 700; letter-spacing: 0.5px; flex-shrink: 0; }
    .mx { background: rgba(88,166,255,0.15); color: #58a6ff; }
    .tg { background: rgba(97,175,239,0.15); color: #61afef; }
    .sx { background: rgba(198,120,221,0.15); color: #c678dd; }
    .wa { background: rgba(152,195,121,0.15); color: #98c379; }

    .card-actions { flex-shrink: 0; }
    .act-btn {
        padding: 6px 14px; border-radius: 8px; border: none;
        font-size: 0.78em; font-weight: 600; font-family: inherit; cursor: pointer; transition: all 0.15s;
    }
    .act-btn.primary { background: var(--ac, #58a6ff); color: #0e1117; }
    .act-btn.primary:hover:not(:disabled) { filter: brightness(1.15); }
    .act-btn.secondary { background: rgba(255,255,255,0.06); color: #c9d1d9; }
    .act-btn.secondary:hover:not(:disabled) { background: rgba(255,255,255,0.1); }
    .act-btn.danger { background: rgba(224,108,117,0.1); color: #e06c75; }
    .act-btn.danger:hover:not(:disabled) { background: rgba(224,108,117,0.2); }
    .act-btn:disabled { opacity: 0.4; cursor: default; }

    .confirm-overlay {
        position: fixed; inset: 0; background: rgba(0,0,0,0.5);
        display: flex; align-items: center; justify-content: center;
        z-index: 500; animation: fadeIn 150ms ease;
    }
    @keyframes fadeIn { from { opacity: 0; } }

    .confirm-dialog {
        background: #161b22; border-radius: 14px; padding: 24px;
        width: 360px; max-width: 90vw;
        border: 1px solid rgba(255,255,255,0.06);
        box-shadow: 0 16px 32px rgba(0,0,0,0.4);
    }
    .confirm-dialog h4 { margin: 0 0 12px; font-size: 0.95em; font-weight: 600; }
    .confirm-dialog p { font-size: 0.82em; color: #8b949e; line-height: 1.5; margin: 0 0 18px; }
    .confirm-actions { display: flex; gap: 8px; justify-content: flex-end; }
</style>
