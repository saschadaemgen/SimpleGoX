<script>
    import { rooms, currentRoomId, iotPanelOpen, telegramChats, simplexContactsList } from '../lib/stores.js';
    import { loadRooms, loadIotDevices } from '../lib/tauri.js';
    import { onMount, onDestroy } from 'svelte';
    import RoomItem from './RoomItem.svelte';

    let interval;
    onMount(() => { interval = setInterval(loadRooms, 10000); });
    onDestroy(() => clearInterval(interval));

    // Briefing 045 W3: sx_conn_state is now a field on each contact
    // rather than looked up from a side-indexed state map. Subscribing
    // to the derived `simplexContactsList` gives the flat array view.
    $: combinedRooms = buildCombinedList($rooms, $telegramChats, $simplexContactsList);

    function buildCombinedList(matrixRooms, tgChats, sxContacts) {
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

        const sxItems = (sxContacts || []).map(contact => {
            // Briefing 045 W3: contacts carry their own conn_state as
            // a string field ('connected' | 'reconnecting' | 'dead');
            // RoomItem already expects `sx_conn_state` to be null for
            // healthy sessions and an object for reconnecting / dead.
            // Map the flat string to the object shape RoomItem reads.
            const connState = (() => {
                switch (contact.conn_state) {
                    case 'reconnecting':
                        return {
                            state: 'reconnecting',
                            attempt: contact.reconnect_attempt,
                            maxAttempts: contact.reconnect_max_attempts,
                        };
                    case 'dead':
                        return { state: 'dead' };
                    default:
                        // 'connected' or missing -> null = healthy, no dot
                        return null;
                }
            })();
            return {
                _key: 'sx:' + contact.contact_id,
                _id: 'sx:' + contact.contact_id,
                room_id: 'sx:' + contact.contact_id,
                name: contact.display_name || 'SimpleX contact',
                backend: 'simplex',
                badge_label: 'SX',
                badge_color: '#c678dd',
                is_encrypted: true,
                unread_count: contact.unread_count || 0,
                // Backend no longer ships last_message_body; that is Tier 3
                // message plaintext, never leaves the sidecar. Sidebar shows
                // the last-activity timestamp only.
                last_message_body: '',
                avatar_url: '',
                sort_time:
                    contact.last_message_at_unix ||
                    contact.established_at_unix ||
                    0,
                is_muted: false,
                is_pinned: false,
                chat_type: 'private',
                sx_contact_id: contact.contact_id,
                sx_conn_state: connState,
            };
        });

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
