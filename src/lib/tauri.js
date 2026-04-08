import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { get } from 'svelte/store';
import {
    isLoggedIn, currentUserId, currentDeviceId, homeserver, recoveryKey,
    rooms, messages, typingUsers, iotDevices, loginError, loginLoading,
    currentRoomId, roomInfoData, roomMembers,
} from './stores.js';

let unlisteners = [];

// Auth

export async function doLogin(hs, username, password) {
    loginLoading.set(true);
    loginError.set(null);
    try {
        const r = await invoke('login', { homeserver: hs, username, password });
        currentUserId.set(r.user_id);
        currentDeviceId.set(r.device_id);
        homeserver.set(hs);
        isLoggedIn.set(true);
        startListening();
        await loadRooms();
    } catch (e) {
        loginError.set(String(e));
    } finally {
        loginLoading.set(false);
    }
}

export async function tryRestore() {
    try {
        const r = await invoke('try_restore_session');
        if (r) {
            currentUserId.set(r.user_id);
            currentDeviceId.set(r.device_id);
            homeserver.set(r.homeserver);
            isLoggedIn.set(true);
            startListening();
            await loadRooms();
        }
    } catch (e) {
        console.log('No session:', e);
    }
}

export async function doLogout() {
    try { await invoke('logout'); } catch (e) { console.error('Logout:', e); }
    isLoggedIn.set(false);
    currentUserId.set(null);
    currentDeviceId.set(null);
    rooms.set([]);
    messages.set({});
    typingUsers.set({});
    iotDevices.set({});
}

// Rooms

export async function loadRooms() {
    try { rooms.set(await invoke('get_rooms')); }
    catch (e) { console.error('Rooms:', e); }
}

// Messages

export async function sendMessage(roomId, message) {
    // Local echo - show immediately
    const tempId = `local-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
    const userId = get(currentUserId);
    messages.update(cur => {
        const arr = cur[roomId] || [];
        return { ...cur, [roomId]: [...arr, {
            event_id: tempId, room_id: roomId, sender: userId,
            sender_display_name: null, sender_avatar_url: null,
            body: message, timestamp: Date.now(), is_own: true,
            reply_to_event_id: null, is_edited: false, is_redacted: false,
        }] };
    });
    try {
        await invoke('send_message', { roomId, message });
    } catch (e) {
        // Remove local echo on failure
        messages.update(cur => {
            const arr = cur[roomId] || [];
            return { ...cur, [roomId]: arr.filter(m => m.event_id !== tempId) };
        });
        throw e;
    }
}

export async function getRoomMessages(roomId, limit) {
    try { return await invoke('get_room_messages', { roomId, limit: limit || 50 }); }
    catch (e) { console.error('getRoomMessages:', e); return []; }
}

export async function sendReply(roomId, body, replyToEventId) {
    const tempId = `local-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
    const userId = get(currentUserId);
    messages.update(cur => {
        const arr = cur[roomId] || [];
        return { ...cur, [roomId]: [...arr, {
            event_id: tempId, room_id: roomId, sender: userId,
            sender_display_name: null, sender_avatar_url: null,
            body, timestamp: Date.now(), is_own: true,
            reply_to_event_id: replyToEventId, is_edited: false, is_redacted: false,
        }] };
    });
    try { await invoke('send_reply', { roomId, body, replyToEventId }); }
    catch (e) {
        messages.update(cur => ({ ...cur, [roomId]: (cur[roomId] || []).filter(m => m.event_id !== tempId) }));
        throw e;
    }
}

export async function sendReaction(roomId, eventId, emoji) {
    await invoke('send_reaction', { roomId, eventId, emoji });
}

export async function editMessage(roomId, eventId, newBody) {
    await invoke('edit_message', { roomId, eventId, newBody });
    messages.update(cur => {
        const arr = cur[roomId] || [];
        return { ...cur, [roomId]: arr.map(m => m.event_id === eventId ? { ...m, body: newBody, is_edited: true } : m) };
    });
}

// Typing

export async function sendTyping(roomId, typing) {
    try { await invoke('send_typing', { roomId, typing }); } catch (_) {}
}

// Receipts

export async function markAsRead(roomId, eventId) {
    try { await invoke('mark_as_read', { roomId, eventId }); } catch (_) {}
}

// IoT

export async function sendIotCommand(roomId, deviceId, action, value) {
    await invoke('send_iot_command', { roomId, deviceId, action, value });
}

export async function loadIotDevices(roomId) {
    try {
        const devs = await invoke('get_iot_devices', { roomId });
        iotDevices.update(cur => ({ ...cur, [roomId]: devs }));
    } catch (_) {}
}

// Settings

export async function loadSettings() {
    try {
        const s = await invoke('get_settings');
        currentUserId.set(s.user_id);
        currentDeviceId.set(s.device_id);
        homeserver.set(s.homeserver);
    } catch (_) {}
}

export async function loadRecoveryKey() {
    try { recoveryKey.set(await invoke('get_recovery_key')); } catch (_) { recoveryKey.set(null); }
}

// Room management

export async function createRoom(name, isEncrypted, isPublic, topic, inviteUserIds) {
    const roomId = await invoke('create_room', { name, isEncrypted, isPublic, topic: topic || null, inviteUserIds: inviteUserIds || null });
    await loadRooms();
    currentRoomId.set(roomId);
    return roomId;
}

export async function createDm(userId) {
    const roomId = await invoke('create_dm', { userId });
    await loadRooms();
    currentRoomId.set(roomId);
    return roomId;
}

export async function joinRoom(roomIdOrAlias) {
    const roomId = await invoke('join_room', { roomIdOrAlias });
    await loadRooms();
    currentRoomId.set(roomId);
    return roomId;
}

export async function leaveRoom(roomId) {
    await invoke('leave_room', { roomId });
    if (get(currentRoomId) === roomId) currentRoomId.set(null);
    await loadRooms();
}

export async function inviteUser(roomId, userId) {
    await invoke('invite_user', { roomId, userId });
}

export async function kickUser(roomId, userId, reason) {
    await invoke('kick_user', { roomId, userId, reason: reason || null });
}

export async function banUser(roomId, userId, reason) {
    await invoke('ban_user', { roomId, userId, reason: reason || null });
}

export async function unbanUser(roomId, userId) {
    await invoke('unban_user', { roomId, userId });
}

export async function getRoomMembers(roomId) {
    try { return await invoke('get_room_members', { roomId }); }
    catch (_) { return []; }
}

export async function getRoomInfo(roomId) {
    try { return await invoke('get_room_info', { roomId }); }
    catch (_) { return null; }
}

export async function setRoomName(roomId, name) {
    await invoke('set_room_name', { roomId, name });
    await loadRooms();
}

export async function setRoomTopic(roomId, topic) {
    await invoke('set_room_topic', { roomId, topic });
}

export async function setRoomTag(roomId, tag, order) {
    await invoke('set_room_tag', { roomId, tag, order: order || null });
    await loadRooms();
}

export async function removeRoomTag(roomId, tag) {
    await invoke('remove_room_tag', { roomId, tag });
    await loadRooms();
}

export async function redactEvent(roomId, eventId, reason) {
    await invoke('redact_event', { roomId, eventId, reason: reason || null });
    messages.update(cur => {
        const arr = cur[roomId] || [];
        return { ...cur, [roomId]: arr.map(m => m.event_id === eventId ? { ...m, is_redacted: true, body: '' } : m) };
    });
}

export async function loadRoomInfo(roomId) {
    const info = await getRoomInfo(roomId);
    if (info) roomInfoData.set(info);
    const members = await getRoomMembers(roomId);
    roomMembers.set(members);
}

// Room settings

export async function getRoomSettings(roomId) {
    try { return await invoke('get_room_settings', { roomId }); }
    catch (_) { return null; }
}

export async function setJoinRule(roomId, joinRule) {
    await invoke('set_join_rule', { roomId, joinRule });
}

export async function setHistoryVisibility(roomId, visibility) {
    await invoke('set_history_visibility', { roomId, visibility });
}

// Avatar / Profile

const avatarCache = new Map();

export function clearAvatarCache() { avatarCache.clear(); }

export async function resolveMxcUrl(mxcUri, width, height) {
    if (!mxcUri) return null;
    const w = width || 96;
    const h = height || 96;
    const key = `${mxcUri}_${w}_${h}`;
    if (avatarCache.has(key)) return avatarCache.get(key);
    try {
        const dataUrl = await invoke('get_avatar_base64', { mxcUri, width: w, height: h });
        if (dataUrl) avatarCache.set(key, dataUrl);
        return dataUrl;
    } catch (e) {
        console.error('resolveMxcUrl failed:', mxcUri, e);
        return null;
    }
}

export async function getOwnProfile() {
    try { return await invoke('get_own_profile'); }
    catch (_) { return null; }
}

export async function setDisplayName(name) {
    await invoke('set_display_name', { name });
}

export async function uploadAvatar(data, contentType) {
    return await invoke('upload_avatar', { data: Array.from(data), contentType });
}

export async function removeAvatar() {
    await invoke('remove_avatar');
}

export async function setRoomAvatar(roomId, data, contentType) {
    await invoke('set_room_avatar', { roomId, data: Array.from(data), contentType });
    await loadRooms();
}

export async function removeRoomAvatar(roomId) {
    await invoke('remove_room_avatar', { roomId });
    await loadRooms();
}

// File-dialog-based avatar upload

function extractPath(result) {
    if (!result) return null;
    if (typeof result === 'string') return result;
    if (Array.isArray(result)) {
        const first = result[0];
        return typeof first === 'string' ? first : first?.path || first?.filePath || null;
    }
    return result.path || result.filePath || result.file || null;
}

export async function pickAndUploadAvatar() {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const result = await open({
        title: 'Select Avatar Image',
        filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }],
        multiple: false, directory: false,
    });
    console.log('File dialog result:', result, typeof result);
    const filePath = extractPath(result);
    if (!filePath) { console.log('File dialog cancelled or no path'); return null; }
    console.log('Uploading avatar from:', filePath);
    const mxc = await invoke('upload_avatar_from_path', { filePath });
    console.log('Avatar uploaded:', mxc);
    clearAvatarCache();
    return mxc;
}

export async function pickAndUploadRoomAvatar(roomId) {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const result = await open({
        title: 'Select Room Avatar',
        filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }],
        multiple: false, directory: false,
    });
    console.log('File dialog result:', result, typeof result);
    const filePath = extractPath(result);
    if (!filePath) { console.log('File dialog cancelled or no path'); return; }
    console.log('Uploading room avatar from:', filePath);
    await invoke('upload_room_avatar_from_path', { roomId, filePath });
    console.log('Room avatar uploaded');
    clearAvatarCache();
    await loadRooms();
}

// Event listeners

async function startListening() {
    for (const u of unlisteners) u();
    unlisteners = [];

    unlisteners.push(await listen('new-message', ev => {
        const msg = ev.payload;
        messages.update(cur => {
            const arr = cur[msg.room_id] || [];
            // Skip exact duplicates
            if (arr.some(m => m.event_id === msg.event_id)) return cur;
            // Replace local echo with server version
            if (msg.is_own) {
                const localIdx = arr.findIndex(m => m.event_id.startsWith('local-') && m.body === msg.body && m.is_own);
                if (localIdx >= 0) {
                    const updated = [...arr];
                    updated[localIdx] = msg;
                    return { ...cur, [msg.room_id]: updated };
                }
            }
            return { ...cur, [msg.room_id]: [...arr, msg] };
        });
        loadRooms();
    }));

    unlisteners.push(await listen('typing', ev => {
        const { room_id, user_ids } = ev.payload;
        typingUsers.update(cur => ({ ...cur, [room_id]: user_ids }));
    }));

    unlisteners.push(await listen('iot-status', ev => {
        const s = ev.payload;
        iotDevices.update(cur => {
            const devs = cur[s.room_id] || [];
            return {
                ...cur,
                [s.room_id]: devs.map(d => d.device_id === s.device_id
                    ? { ...d, state: s.state, value: s.value, unit: s.unit }
                    : d
                ),
            };
        });
    }));

    // Reaction events
    unlisteners.push(await listen('new-reaction', ev => {
        const rx = ev.payload;
        messages.update(cur => {
            const arr = cur[rx.room_id] || [];
            return {
                ...cur,
                [rx.room_id]: arr.map(msg => {
                    if (msg.event_id !== rx.target_event_id) return msg;
                    const reactions = [...(msg.reactions || [])];
                    const idx = reactions.findIndex(r => r.key === rx.key);
                    if (idx >= 0) {
                        reactions[idx] = {
                            ...reactions[idx],
                            count: reactions[idx].count + 1,
                            includes_own: reactions[idx].includes_own || rx.is_own,
                            event_ids: [...(reactions[idx].event_ids || []), rx.event_id],
                        };
                    } else {
                        reactions.push({ key: rx.key, count: 1, includes_own: rx.is_own, event_ids: [rx.event_id] });
                    }
                    return { ...msg, reactions };
                }),
            };
        });
    }));
}
