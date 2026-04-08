<script>
    import { rooms, currentRoomId, iotPanelOpen } from '../lib/stores.js';
    import { loadRooms, loadIotDevices, markAsRead } from '../lib/tauri.js';
    import { onMount, onDestroy } from 'svelte';
    import RoomItem from './RoomItem.svelte';

    let interval;
    onMount(() => { interval = setInterval(loadRooms, 10000); });
    onDestroy(() => clearInterval(interval));

    function select(room) {
        currentRoomId.set(room.room_id);
        loadIotDevices(room.room_id);
        if (room.name.toLowerCase().includes('iot')) {
            iotPanelOpen.set(true);
        }
    }
</script>

<div class="rooms">
    {#each $rooms as room (room.room_id)}
        <RoomItem {room} active={$currentRoomId === room.room_id} onclick={() => select(room)} />
    {/each}
    {#if $rooms.length === 0}
        <p class="empty">No rooms yet</p>
    {/if}
</div>

<style>
    .rooms { flex: 1; overflow-y: auto; padding: 6px; }
    .empty { color: var(--text-3); font-size: 0.82em; text-align: center; padding: 20px; }
</style>
