<script>
    import { settingsOpen, iotPanelOpen, roomInfoOpen, createRoomDialogOpen, joinRoomDialogOpen, createDmDialogOpen, confirmDialog, roomSettingsOpen, telegramAuthOpen, telegramChats, telegramConnected } from '../lib/stores.js';
    import { tgGetAuthState, tgListChats } from '../lib/tauri.js';
    import { onMount } from 'svelte';
    import Sidebar from './Sidebar.svelte';
    import ChatView from './ChatView.svelte';
    import IotPanel from './IotPanel.svelte';
    import RoomInfoPanel from './RoomInfoPanel.svelte';
    import SettingsOverlay from './SettingsOverlay.svelte';
    import CreateRoomDialog from './CreateRoomDialog.svelte';
    import JoinRoomDialog from './JoinRoomDialog.svelte';
    import CreateDmDialog from './CreateDmDialog.svelte';
    import ConfirmDialog from './ConfirmDialog.svelte';
    import ContextMenu from './ContextMenu.svelte';
    import RoomSettingsDialog from './RoomSettingsDialog.svelte';
    import TelegramAuth from './TelegramAuth.svelte';

    onMount(async () => {
        // Try loading Telegram chats if sidecar is already connected and authenticated
        try {
            const state = await tgGetAuthState();
            if (state === 'ready') {
                const chats = await tgListChats(50);
                telegramChats.set(chats);
                telegramConnected.set(true);
                console.log(`Loaded ${chats.length} Telegram chats`);
            }
        } catch (e) {
            console.log('Telegram not connected yet:', e);
        }
    });
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
{#if $settingsOpen}<SettingsOverlay />{/if}
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
