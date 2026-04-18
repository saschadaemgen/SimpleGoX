<script>
    import { createEventDispatcher } from 'svelte';
    import { settingsOpen, iotPanelOpen, roomInfoOpen, createRoomDialogOpen, joinRoomDialogOpen, createDmDialogOpen, confirmDialog, roomSettingsOpen, telegramAuthOpen, telegramChats, telegramConnected, telegramMessages, currentRoomId, torRouting, simplexContacts, simplexMessages, simplexReady, simplexProfile } from '../lib/stores.js';
    const dispatch = createEventDispatcher();
    import { tgConnect, tgGetAuthState, tgListChats, tgSubscribeUpdates } from '../lib/tauri.js';
    import { invoke } from '@tauri-apps/api/core';
    import { listen } from '@tauri-apps/api/event';
    import { onMount, onDestroy } from 'svelte';
    import Sidebar from './Sidebar.svelte';
    import ChatView from './ChatView.svelte';
    import IotPanel from './IotPanel.svelte';
    import RoomInfoPanel from './RoomInfoPanel.svelte';
    import Settings from './Settings.svelte';
    import CreateRoomDialog from './CreateRoomDialog.svelte';
    import JoinRoomDialog from './JoinRoomDialog.svelte';
    import CreateDmDialog from './CreateDmDialog.svelte';
    import ConfirmDialog from './ConfirmDialog.svelte';
    import ContextMenu from './ContextMenu.svelte';
    import RoomSettingsDialog from './RoomSettingsDialog.svelte';
    import TelegramAuth from './TelegramAuth.svelte';
    import SimplexProfileDialog from './SimplexProfileDialog.svelte';
    import Toast from './Toast.svelte';

    let unlisteners = [];

    onMount(async () => {
        // Re-apply saved network routing (Tor/I2P) after login
        await restoreSavedRouting();

        // Auto-connect to Telegram sidecar (started by Tauri setup)
        await tryTelegramAutoConnect();

        // Also listen for tg-ready event from sidecar auto-start
        unlisteners.push(await listen('tg-ready', async () => {
            console.log('tg-ready event received');
            // Clear TG avatar cache on sidecar reconnect (file IDs may change)
            if (window.__tgAvatarCache) window.__tgAvatarCache.clear();
            await tryTelegramAutoConnect();
        }));

        // SimpleX sidecar integration
        unlisteners.push(await listen('sx-ready', async () => {
            console.log('sx-ready event received');
            await trySimplexAutoConnect();
        }));

        // If the sidecar already emitted sx-ready before we registered (race
        // on fast startup), probe once immediately. Safe: sx_subscribe_updates
        // is idempotent in practice and a no-op error is caught.
        trySimplexAutoConnect().catch(e => console.log('SX initial probe:', e));
    });

    async function restoreSavedRouting() {
        try {
            // Load routing from backend JSON (single source of truth)
            const routing = await invoke('tor_get_saved_routing');
            if (routing) {
                torRouting.set(routing);
                // Re-apply non-direct routing so Matrix client gets rebuilt with proxy
                for (const [proto, mode] of Object.entries(routing)) {
                    if (mode && mode !== 'direct') {
                        console.log(`=== Restoring ${proto} routing to ${mode}`);
                        await invoke('tor_set_protocol', { protocol: proto, mode, onionAddress: null });
                    }
                }
            }
        } catch (e) {
            console.log('Routing restore skipped:', e);
        }
    }

    onDestroy(() => {
        for (const u of unlisteners) u();
        unlisteners = [];
    });

    async function tryTelegramAutoConnect() {
        try {
            await tgConnect(50051);
            const authState = await tgGetAuthState();
            console.log('=== TG auto-connect: auth state =', JSON.stringify(authState));
            if (authState.state === 'ready') {
                telegramConnected.set(true);
                await loadTelegramChatsRetry();
                await subscribeTgUpdatesRetry();
                await setupTgListeners();
            }
        } catch (e) {
            console.log('Telegram auto-connect not available:', e);
        }
    }

    async function loadTelegramChatsRetry() {
        for (let attempt = 1; attempt <= 5; attempt++) {
            await new Promise(r => setTimeout(r, 2000));
            const chats = await tgListChats(50);
            console.log('=== TG chat load attempt', attempt, ':', chats.length, 'chats');
            if (chats.length > 0) {
                telegramChats.set(chats);
                return;
            }
        }
        console.warn('=== TG: no chats after 5 attempts');
    }

    async function subscribeTgUpdatesRetry() {
        for (let attempt = 1; attempt <= 3; attempt++) {
            try {
                await tgSubscribeUpdates();
                console.log('=== TG subscribe success on attempt', attempt);
                return;
            } catch (e) {
                console.warn('=== TG subscribe attempt', attempt, 'failed:', e);
                await new Promise(r => setTimeout(r, 2000));
            }
        }
    }

    async function trySimplexAutoConnect() {
        try {
            // Load the persisted profile (may be null if first run)
            const profile = await invoke('sx_get_profile');
            if (profile && profile.has_profile) {
                simplexProfile.set({
                    display_name: profile.display_name,
                    full_name: profile.full_name,
                    bio: profile.bio,
                });
            }

            await invoke('sx_subscribe_updates');
            simplexReady.set(true);
            await setupSxListeners();
            console.log('SX real-time listeners active');
        } catch (e) {
            console.log('SimpleX auto-connect not ready:', e);
        }
    }

    async function setupSxListeners() {
        // Peer identity established (after X3DH + ratchet decode of peer profile)
        unlisteners.push(await listen('sx-contact-established', (ev) => {
            const c = ev.payload;
            console.log('sx-contact-established', c);
            simplexContacts.update(list => {
                const idx = list.findIndex(x => x.contact_id === c.contact_id);
                const entry = {
                    contact_id: c.contact_id,
                    display_name: c.display_name || 'SimpleX contact',
                    full_name: c.full_name || '',
                    bio: c.bio || '',
                    established_at: c.established_at || 0,
                    last_message_body: '',
                    last_message_time: c.established_at || 0,
                };
                if (idx >= 0) {
                    const next = list.slice();
                    next[idx] = { ...list[idx], ...entry };
                    return next;
                }
                return [...list, entry];
            });
        }));

        // Incoming (and later outgoing echo) chat messages.
        unlisteners.push(await listen('sx-new-message', (ev) => {
            const m = ev.payload;
            console.log('sx-new-message', m);

            // Try to decode the x.msg.new JSON wrapper; fall back to raw body
            // text when the peer uses a shape we do not parse yet.
            let displayBody = m.body || '';
            try {
                const parsed = JSON.parse(m.body);
                const text = parsed?.params?.content?.text;
                if (typeof text === 'string' && text.length > 0) {
                    displayBody = text;
                }
            } catch (_) {
                // Not JSON, keep raw string.
            }

            simplexMessages.update(cur => {
                const list = cur[m.contact_id] || [];
                const exists = list.some(x => x.msg_id === m.msg_id && x.is_own === !!m.is_own);
                if (exists) return cur;
                return {
                    ...cur,
                    [m.contact_id]: [...list, {
                        msg_id: m.msg_id,
                        timestamp: m.timestamp,
                        body: displayBody,
                        raw_body: m.body,
                        is_own: !!m.is_own,
                    }],
                };
            });

            simplexContacts.update(list => list.map(c => (
                c.contact_id === m.contact_id
                    ? { ...c, last_message_body: displayBody, last_message_time: m.timestamp || c.last_message_time }
                    : c
            )));
        }));

        unlisteners.push(await listen('sx-contact-updated', (ev) => {
            const u = ev.payload;
            simplexContacts.update(list => list.map(c => (
                c.contact_id === u.contact_id
                    ? { ...c, display_name: u.display_name, full_name: u.full_name, bio: u.bio }
                    : c
            )));
        }));
    }

    async function setupTgListeners() {
        // New Telegram message
        unlisteners.push(await listen('tg-new-message', (ev) => {
            const msg = ev.payload;
            const tgRoomId = 'tg:' + msg.chat_id;

            telegramMessages.update(cur => {
                const existing = cur[msg.chat_id];
                if (!existing) return cur;
                if (existing.some(m => m.event_id === msg.event_id)) return cur;
                return {
                    ...cur,
                    [msg.chat_id]: [...existing, {
                        event_id: msg.event_id,
                        sender: msg.sender,
                        sender_display_name: msg.sender_display_name,
                        sender_avatar_url: msg.sender_avatar_url || null,
                        body: msg.body,
                        timestamp: msg.timestamp,
                        is_own: msg.is_own,
                        is_edited: false,
                        is_redacted: false,
                        reply_to_event_id: null,
                        reactions: [],
                        backend: 'telegram',
                    }],
                };
            });

            telegramChats.update(chats => chats.map(c => {
                if (c.id !== msg.chat_id) return c;
                return {
                    ...c,
                    last_message_body: msg.body,
                    last_message_time: msg.timestamp / 1000,
                    unread_count: ($currentRoomId === tgRoomId) ? c.unread_count : c.unread_count + 1,
                };
            }));
        }));

        // Chat updated (unread count, last message)
        unlisteners.push(await listen('tg-chat-updated', (ev) => {
            const data = ev.payload;
            telegramChats.update(chats => chats.map(c => {
                if (c.id !== data.chat_id) return c;
                return {
                    ...c,
                    unread_count: data.unread_count || c.unread_count,
                    last_message_body: data.last_message_body || c.last_message_body,
                    last_message_time: data.last_message_time ? data.last_message_time / 1000 : c.last_message_time,
                };
            }));
        }));

        console.log('TG real-time listeners active');
    }
</script>

<div class="app">
    <Sidebar />
    <ChatView />
    {#if $roomInfoOpen}
        <RoomInfoPanel />
    {/if}
    {#if $iotPanelOpen}
        <IotPanel />
    {/if}
</div>
<Settings visible={$settingsOpen} onClose={() => settingsOpen.set(false)} on:run-wizard={() => dispatch('run-wizard')} />
{#if $createRoomDialogOpen}<CreateRoomDialog />{/if}
{#if $joinRoomDialogOpen}<JoinRoomDialog />{/if}
{#if $createDmDialogOpen}<CreateDmDialog />{/if}
{#if $confirmDialog.visible}<ConfirmDialog />{/if}
{#if $roomSettingsOpen}<RoomSettingsDialog />{/if}
{#if $telegramAuthOpen}<TelegramAuth />{/if}
<SimplexProfileDialog />
<Toast />
<ContextMenu />

<style>
    .app { display: flex; height: calc(100vh - var(--banner-h, 0px)); margin-top: var(--banner-h, 0px); }
</style>
