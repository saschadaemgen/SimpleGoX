<script>
    import { createEventDispatcher } from 'svelte';
    import { settingsOpen, iotPanelOpen, roomInfoOpen, createRoomDialogOpen, joinRoomDialogOpen, createDmDialogOpen, confirmDialog, roomSettingsOpen, telegramAuthOpen, telegramChats, telegramConnected, telegramMessages, currentRoomId } from '../lib/stores.js';
    const dispatch = createEventDispatcher();
    import { tgConnect, tgGetAuthState, tgListChats, tgSubscribeUpdates } from '../lib/tauri.js';
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

    let unlisteners = [];

    onMount(async () => {
        // Auto-connect to Telegram sidecar (started by Tauri setup)
        await tryTelegramAutoConnect();

        // Also listen for tg-ready event from sidecar auto-start
        unlisteners.push(await listen('tg-ready', async () => {
            console.log('tg-ready event received');
            await tryTelegramAutoConnect();
        }));
    });

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
<ContextMenu />

<style>
    .app { display: flex; height: 100vh; }
</style>
