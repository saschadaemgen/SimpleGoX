// SimpleGoX Desktop - Frontend Logic
// No tokens, keys, or matrix-sdk types ever appear here.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

let currentRoomId = null;
let currentUserId = null;
let rooms = [];
const messageBuffers = {};

// Privacy settings (default on)
let sendReadReceipts = true;
let sendTypingNotices = true;

// Sender color palette
const SENDER_COLORS = [
    '#e91e63', '#9c27b0', '#673ab7', '#3f51b5',
    '#2196f3', '#00bcd4', '#009688', '#4caf50',
    '#ff9800', '#ff5722', '#795548', '#607d8b'
];

// ---------------------------------------------------------------------------
// DOM refs
// ---------------------------------------------------------------------------

const loginScreen = document.getElementById('login-screen');
const chatScreen = document.getElementById('chat-screen');
const settingsScreen = document.getElementById('settings-screen');
const loginForm = document.getElementById('login-form');
const loginBtn = document.getElementById('login-btn');
const loginError = document.getElementById('login-error');
const roomListEl = document.getElementById('room-list');
const messagesEl = document.getElementById('messages');
const chatRoomName = document.getElementById('chat-room-name');
const chatRoomEncrypted = document.getElementById('chat-room-encrypted');
const sendForm = document.getElementById('send-form');
const messageInput = document.getElementById('message-input');
const userInfoEl = document.getElementById('user-info');
const typingIndicator = document.getElementById('typing-indicator');
const typingText = typingIndicator.querySelector('.typing-text');

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function escapeHtml(str) {
    const el = document.createElement('span');
    el.textContent = str || '';
    return el.innerHTML;
}

function formatTime(ts) {
    if (!ts) return '';
    const d = new Date(ts);
    const h = String(d.getHours()).padStart(2, '0');
    const m = String(d.getMinutes()).padStart(2, '0');
    return `${h}:${m}`;
}

function formatDate(ts) {
    const d = new Date(ts);
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const msgDay = new Date(d.getFullYear(), d.getMonth(), d.getDate());
    const diff = (today - msgDay) / 86400000;
    if (diff === 0) return 'Today';
    if (diff === 1) return 'Yesterday';
    return d.toLocaleDateString(undefined, { weekday: 'long', month: 'short', day: 'numeric' });
}

function senderColor(userId) {
    let hash = 0;
    for (let i = 0; i < userId.length; i++) {
        hash = userId.charCodeAt(i) + ((hash << 5) - hash);
    }
    return SENDER_COLORS[Math.abs(hash) % SENDER_COLORS.length];
}

function formatUsername(userId) {
    const match = userId.match(/^@([^:]+)/);
    return match ? match[1] : userId;
}

function shouldShowSender(msg, prevMsg) {
    if (!prevMsg) return true;
    if (msg.sender !== prevMsg.sender) return true;
    if (msg.timestamp - prevMsg.timestamp > 5 * 60 * 1000) return true;
    return false;
}

function isSameDay(ts1, ts2) {
    const d1 = new Date(ts1);
    const d2 = new Date(ts2);
    return d1.getFullYear() === d2.getFullYear()
        && d1.getMonth() === d2.getMonth()
        && d1.getDate() === d2.getDate();
}

// Status SVGs
const STATUS_SENT = `<svg viewBox="0 0 16 12"><path d="M1 6l4 4L15 2" stroke="currentColor" stroke-width="2" fill="none"/></svg>`;

// ---------------------------------------------------------------------------
// Screen transitions
// ---------------------------------------------------------------------------

function showScreen(screen) {
    document.querySelectorAll('.screen').forEach(s => s.classList.remove('active'));
    screen.classList.add('active');
}

// ---------------------------------------------------------------------------
// Login
// ---------------------------------------------------------------------------

loginForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    const homeserver = document.getElementById('homeserver').value.trim();
    const username = document.getElementById('username').value.trim();
    const password = document.getElementById('password').value;

    if (!homeserver || !username || !password) return;

    loginBtn.disabled = true;
    loginBtn.querySelector('.btn-text').classList.add('hidden');
    loginBtn.querySelector('.btn-loading').classList.remove('hidden');
    loginError.classList.add('hidden');

    try {
        const result = await invoke('login', { homeserver, username, password });
        currentUserId = result.user_id;
        userInfoEl.textContent = currentUserId;
        showScreen(chatScreen);
        await loadRooms();
    } catch (err) {
        loginError.textContent = String(err);
        loginError.classList.remove('hidden');
    } finally {
        loginBtn.disabled = false;
        loginBtn.querySelector('.btn-text').classList.remove('hidden');
        loginBtn.querySelector('.btn-loading').classList.add('hidden');
    }
});

// ---------------------------------------------------------------------------
// Rooms
// ---------------------------------------------------------------------------

async function loadRooms() {
    try {
        rooms = await invoke('get_rooms');
        renderRoomList();
    } catch (err) {
        console.error('Failed to load rooms:', err);
        setTimeout(loadRooms, 2000);
    }
}

function renderRoomList() {
    if (rooms.length === 0) {
        roomListEl.innerHTML = '<p class="room-list-empty">No rooms yet</p>';
        return;
    }

    roomListEl.innerHTML = rooms.map(room => {
        const initial = (room.name || '?')[0].toUpperCase();
        const isActive = room.room_id === currentRoomId;
        const preview = getLastMessage(room.room_id);
        const avatarBg = senderColor(room.room_id);
        const unread = room.unread_count || 0;
        return `
            <div class="room-item${isActive ? ' active' : ''}"
                 data-room-id="${escapeHtml(room.room_id)}"
                 data-room-name="${escapeHtml(room.name)}"
                 data-encrypted="${room.is_encrypted}">
                <div class="room-avatar" style="background:${avatarBg}">${escapeHtml(initial)}</div>
                <div class="room-meta">
                    <div class="room-name">${escapeHtml(room.name)}</div>
                    <div class="room-preview">${escapeHtml(preview)}</div>
                </div>
                ${unread > 0 ? `<span class="unread-badge">${unread}</span>` : ''}
                ${room.is_encrypted ? '<span class="room-encrypted-icon">&#128274;</span>' : ''}
            </div>`;
    }).join('');

    roomListEl.querySelectorAll('.room-item').forEach(el => {
        el.addEventListener('click', () => selectRoom(
            el.dataset.roomId,
            el.dataset.roomName,
            el.dataset.encrypted === 'true'
        ));
    });
}

function getLastMessage(roomId) {
    const buf = messageBuffers[roomId];
    if (!buf || buf.length === 0) return '';
    const last = buf[buf.length - 1];
    return last.body.length > 40 ? last.body.slice(0, 40) + '...' : last.body;
}

function selectRoom(roomId, roomName, isEncrypted) {
    currentRoomId = roomId;

    chatRoomName.textContent = roomName;
    if (isEncrypted) {
        chatRoomEncrypted.classList.remove('hidden');
    } else {
        chatRoomEncrypted.classList.add('hidden');
    }
    sendForm.classList.remove('hidden');
    hideTypingIndicator();

    roomListEl.querySelectorAll('.room-item').forEach(el => {
        el.classList.toggle('active', el.dataset.roomId === roomId);
    });

    renderMessages();
    messageInput.focus();

    // Send read receipt for last message
    if (sendReadReceipts) {
        const buf = messageBuffers[roomId];
        if (buf && buf.length > 0) {
            const last = buf[buf.length - 1];
            if (last.event_id && last.sender !== currentUserId) {
                invoke('mark_as_read', { roomId, eventId: last.event_id }).catch(() => {});
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

function renderMessages() {
    const buf = messageBuffers[currentRoomId] || [];
    if (buf.length === 0) {
        messagesEl.innerHTML = '<p class="messages-empty">No messages yet</p>';
        return;
    }

    let html = '';
    for (let i = 0; i < buf.length; i++) {
        const msg = buf[i];
        const prev = i > 0 ? buf[i - 1] : null;

        // Date separator
        if (!prev || !isSameDay(prev.timestamp, msg.timestamp)) {
            html += `<div class="date-separator">${formatDate(msg.timestamp)}</div>`;
        }

        html += messageHtml(msg, prev);
    }

    messagesEl.innerHTML = html;
    scrollToBottom();
}

function messageHtml(msg, prevMsg) {
    const isSelf = msg.sender === currentUserId;
    const cls = isSelf ? 'message-self' : 'message-other';
    const showSender = !isSelf && shouldShowSender(msg, prevMsg);
    const grouped = !shouldShowSender(msg, prevMsg) ? ' message-grouped' : '';
    const time = formatTime(msg.timestamp);
    const color = senderColor(msg.sender);

    let status = '';
    if (isSelf) {
        status = `<span class="message-status status-sent">${STATUS_SENT}</span>`;
    }

    return `
        <div class="message ${cls}${grouped}" data-event-id="${escapeHtml(msg.event_id || '')}">
            ${showSender ? `<div class="message-sender" style="color:${color}">${escapeHtml(formatUsername(msg.sender))}</div>` : ''}
            <div class="message-body">${escapeHtml(msg.body)}</div>
            <div class="message-footer">
                <span class="message-time">${time}</span>
                ${status}
            </div>
        </div>`;
}

function appendMessage(msg) {
    if (!messageBuffers[msg.room_id]) {
        messageBuffers[msg.room_id] = [];
    }
    messageBuffers[msg.room_id].push(msg);

    if (msg.room_id === currentRoomId) {
        const emptyEl = messagesEl.querySelector('.messages-empty');
        if (emptyEl) emptyEl.remove();

        const buf = messageBuffers[msg.room_id];
        const prev = buf.length > 1 ? buf[buf.length - 2] : null;

        // Date separator if needed
        if (!prev || !isSameDay(prev.timestamp, msg.timestamp)) {
            messagesEl.insertAdjacentHTML('beforeend',
                `<div class="date-separator">${formatDate(msg.timestamp)}</div>`);
        }

        messagesEl.insertAdjacentHTML('beforeend', messageHtml(msg, prev));
        scrollToBottom();

        // Auto read receipt
        if (sendReadReceipts && msg.sender !== currentUserId && msg.event_id) {
            invoke('mark_as_read', { roomId: msg.room_id, eventId: msg.event_id }).catch(() => {});
        }
    }

    renderRoomList();
}

function scrollToBottom() {
    requestAnimationFrame(() => {
        messagesEl.scrollTop = messagesEl.scrollHeight;
    });
}

// ---------------------------------------------------------------------------
// Send
// ---------------------------------------------------------------------------

sendForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    const message = messageInput.value.trim();
    if (!message || !currentRoomId) return;

    messageInput.value = '';

    // Stop typing notice
    if (sendTypingNotices) {
        clearTimeout(typingTimeout);
        invoke('send_typing', { roomId: currentRoomId, typing: false }).catch(() => {});
    }

    try {
        await invoke('send_message', { roomId: currentRoomId, message });
    } catch (err) {
        console.error('Send failed:', err);
        messageInput.value = message;
    }
});

// ---------------------------------------------------------------------------
// Typing indicators
// ---------------------------------------------------------------------------

let typingTimeout = null;

messageInput.addEventListener('input', () => {
    if (!currentRoomId || !sendTypingNotices) return;

    invoke('send_typing', { roomId: currentRoomId, typing: true }).catch(() => {});

    clearTimeout(typingTimeout);
    typingTimeout = setTimeout(() => {
        invoke('send_typing', { roomId: currentRoomId, typing: false }).catch(() => {});
    }, 5000);
});

function showTypingIndicator(text) {
    typingText.textContent = text;
    typingIndicator.classList.remove('hidden');
}

function hideTypingIndicator() {
    typingIndicator.classList.add('hidden');
}

listen('typing', (event) => {
    const { room_id, user_ids } = event.payload;
    if (room_id !== currentRoomId) return;

    const others = user_ids.filter(id => id !== currentUserId);
    if (others.length === 0) {
        hideTypingIndicator();
    } else if (others.length === 1) {
        showTypingIndicator(formatUsername(others[0]) + ' is typing...');
    } else {
        showTypingIndicator(others.length + ' people are typing...');
    }
});

// ---------------------------------------------------------------------------
// Live message listener
// ---------------------------------------------------------------------------

listen('new-message', (event) => {
    appendMessage(event.payload);
});

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

document.getElementById('settings-btn').addEventListener('click', async () => {
    try {
        const s = await invoke('get_settings');
        document.getElementById('settings-user-id').textContent = s.user_id;
        document.getElementById('settings-homeserver').textContent = s.homeserver;
        document.getElementById('settings-device-id').textContent = s.device_id;
    } catch (err) {
        console.error('Failed to load settings:', err);
    }

    document.getElementById('setting-read-receipts').checked = sendReadReceipts;
    document.getElementById('setting-typing').checked = sendTypingNotices;

    showScreen(settingsScreen);
});

document.getElementById('settings-back').addEventListener('click', () => {
    showScreen(chatScreen);
});

document.getElementById('setting-read-receipts').addEventListener('change', (e) => {
    sendReadReceipts = e.target.checked;
});

document.getElementById('setting-typing').addEventListener('change', (e) => {
    sendTypingNotices = e.target.checked;
});

// Logout from settings
document.getElementById('settings-logout').addEventListener('click', async () => {
    if (!confirm('Log out and remove local data?')) return;
    try {
        await invoke('logout');
    } catch (err) {
        console.error('Logout error:', err);
    }
    currentRoomId = null;
    currentUserId = null;
    rooms = [];
    Object.keys(messageBuffers).forEach(k => delete messageBuffers[k]);
    showScreen(loginScreen);
});

// ---------------------------------------------------------------------------
// Periodic room refresh
// ---------------------------------------------------------------------------

setInterval(() => {
    if (chatScreen.classList.contains('active')) {
        loadRooms();
    }
}, 10000);
