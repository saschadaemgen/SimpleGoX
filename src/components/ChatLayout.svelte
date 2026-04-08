<script>
    import { settingsOpen, iotPanelOpen, roomInfoOpen, createRoomDialogOpen, joinRoomDialogOpen, createDmDialogOpen, confirmDialog, roomSettingsOpen } from '../lib/stores.js';
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
<ContextMenu />

<style>
    .app { display: flex; height: 100vh; }
</style>
