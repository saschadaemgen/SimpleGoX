<script>
    import { rooms, currentRoomId, iotPanelOpen, telegramChats, simplexContacts, simplexContactStates } from '../lib/stores.js';
    import { loadRooms, loadIotDevices } from '../lib/tauri.js';
    import { onMount, onDestroy } from 'svelte';
    import RoomItem from './RoomItem.svelte';

    let interval;
    onMount(() => { interval = setInterval(loadRooms, 10000); });
    onDestroy(() => clearInterval(interval));

    // Combine Matrix rooms + Telegram chats + SimpleX contacts, sorted by recent activity
    $: combinedRooms = buildCombinedList($rooms, $telegramChats, $simplexContacts, $simplexContactStates);

    function buildCombinedList(matrixRooms, tgChats, sxContacts, sxStates) {
        const mxItems = (matrixRooms || []).map(room => ({
            ...room,
            _key: 'mx:' + room.room_id,
            _id: room.room_id,
            backend: 'matrix',
            sort_time: room.last_activity || 0,
        }));

        const tgItems = (tgChats || []).map(chat => ({
            _key: 'tg:' + chat.id,
            _id: 'tg:' + chat.id,
            room_id: 'tg:' + chat.id,
            name: chat.title,
            backend: chat.backend || 'telegram',
            badge_label: chat.badge_label || 'TG',
            badge_color: chat.badge_color || '#61afef',
            is_encrypted: chat.is_encrypted,
            unread_count: chat.unread_count,
            last_message_body: chat.last_message_body,
            avatar_url: chat.avatar_url,
            sort_time: chat.last_message_time || 0,
            is_muted: chat.is_muted,
            is_pinned: chat.is_pinned,
            chat_type: chat.chat_type,
            tg_id: chat.id,
        }));

        const sxItems = (sxContacts || []).map(contact => ({
            _key: 'sx:' + contact.contact_id,
            _id: 'sx:' + contact.contact_id,
            room_id: 'sx:' + contact.contact_id,
            name: contact.display_name || 'SimpleX contact',
            backend: 'simplex',
            badge_label: 'SX',
            badge_color: '#c678dd',
            is_encrypted: true,
            unread_count: 0,
            last_message_body: contact.last_message_body || '',
            avatar_url: '',
            sort_time: contact.last_message_time || contact.established_at || 0,
            is_muted: false,
            is_pinned: false,
            chat_type: 'private',
            sx_contact_id: contact.contact_id,
            // Briefing 044 W6: pass the lifecycle state (if any) so
            // RoomItem can render a yellow / red dot. Absent = healthy.
            sx_conn_state: (sxStates && sxStates[contact.contact_id]) || null,
        }));

        const combined = [...mxItems, ...tgItems, ...sxItems];
        combined.sort((a, b) => (b.sort_time || 0) - (a.sort_time || 0));
        return combined;
    }

    function select(room) {
        currentRoomId.set(room._id || room.room_id);
        if (room.backend === 'matrix') {
            loadIotDevices(room.room_id);
            if (room.name && room.name.toLowerCase().includes('iot')) {
                iotPanelOpen.set(true);
            }
        }
    }
</script>

<div class="rooms">
    {#each combinedRooms as room, i (room._key)}
        <div style="animation-delay:{Math.min(i * 30, 300)}ms" class="rm-wrap">
            <RoomItem {room} active={$currentRoomId === (room._id || room.room_id)} onclick={() => select(room)} />
        </div>
    {/each}
    {#if combinedRooms.length === 0}
        <p class="empty">No rooms yet</p>
    {/if}
</div>

<style>
    .rooms { flex: 1; overflow-y: auto; padding: 6px; }
    .rm-wrap { opacity: 0; animation: rmFade 0.25s ease forwards; }
    @keyframes rmFade { to { opacity: 1; } }
    .empty { color: var(--text-3); font-size: 0.82em; text-align: center; padding: 20px; }
</style>
