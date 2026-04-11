import { writable, derived } from 'svelte/store';

// Auth
export const isLoggedIn = writable(false);
export const currentUserId = writable(null);
export const currentDeviceId = writable(null);
export const homeserver = writable(null);
export const recoveryKey = writable(null);

// Rooms
export const rooms = writable([]);
export const currentRoomId = writable(null);

// Messages: { roomId: [...] }
export const messages = writable({});

// Typing: { roomId: [userId, ...] }
export const typingUsers = writable({});

// IoT: { roomId: [device, ...] }
export const iotDevices = writable({});

// UI
export const sidebarCollapsed = writable(false);
export const iotPanelOpen = writable(false);
export const iotPanelCollapsed = writable(false);
export const settingsOpen = writable(false);
export const loginError = writable(null);
export const loginLoading = writable(false);

// Dialogs
export const createRoomDialogOpen = writable(false);
export const joinRoomDialogOpen = writable(false);
export const createDmDialogOpen = writable(false);
export const roomInfoOpen = writable(false);
export const roomInfoData = writable(null);
export const roomMembers = writable([]);
export const contextMenu = writable({ visible: false, x: 0, y: 0, room: null });
export const confirmDialog = writable({ visible: false, title: '', message: '', confirmText: 'Confirm', danger: false, onConfirm: null });
export const roomSettingsOpen = writable(false);
export const replyingTo = writable(null); // { eventId, sender, senderDisplayName, body }
export const editingMessage = writable(null); // { eventId, body }

// Telegram / Multi-Messenger
export const telegramAuthOpen = writable(false);
export const telegramAuthState = writable('disconnected'); // disconnected, wait_phone, wait_code, wait_password, ready
// Load TG chats from cache for instant display on startup
const cachedTgChats = (() => {
    try {
        const raw = localStorage.getItem('sgx-tg-chats');
        return raw ? JSON.parse(raw) : [];
    } catch (_) { return []; }
})();
export const telegramChats = writable(cachedTgChats);
export const telegramConnected = writable(cachedTgChats.length > 0);
export const telegramMessages = writable({}); // { chatId: [messages] }

// Settings
export const accentColor = writable(localStorage.getItem('sgx-accent') || '#58a6ff');
export const desktopNotifications = writable(localStorage.getItem('sgx-notif') !== 'false');
export const notificationSound = writable(localStorage.getItem('sgx-sound') !== 'false');
export const sendReadReceipts = writable(true);
export const sendTypingNotices = writable(true);

// Derived
export const currentMessages = derived(
    [messages, currentRoomId],
    ([$m, $rid]) => $rid ? ($m[$rid] || []) : []
);

export const currentRoom = derived(
    [rooms, currentRoomId],
    ([$rooms, $rid]) => $rooms.find(r => r.room_id === $rid) || null
);

export const currentTyping = derived(
    [typingUsers, currentRoomId, currentUserId],
    ([$t, $rid, $uid]) => {
        const users = $rid ? ($t[$rid] || []) : [];
        return users.filter(u => u !== $uid);
    }
);

export const currentIotDevices = derived(
    [iotDevices, currentRoomId],
    ([$iot, $rid]) => $rid ? ($iot[$rid] || []) : []
);

// Persist accent color
accentColor.subscribe(color => {
    localStorage.setItem('sgx-accent', color);
    const r = document.documentElement;
    r.style.setProperty('--ac', color);
    const c = color.replace('#', '');
    const rv = parseInt(c.substring(0, 2), 16);
    const gv = parseInt(c.substring(2, 4), 16);
    const bv = parseInt(c.substring(4, 6), 16);
    r.style.setProperty('--ac-bg', `rgba(${rv},${gv},${bv},0.07)`);
    r.style.setProperty('--ac-border', `rgba(${rv},${gv},${bv},0.14)`);
    r.style.setProperty('--ac-glow', `rgba(${rv},${gv},${bv},0.08)`);
    r.style.setProperty('--ac-line', `rgba(${rv},${gv},${bv},0.22)`);
});
desktopNotifications.subscribe(v => localStorage.setItem('sgx-notif', v));
notificationSound.subscribe(v => localStorage.setItem('sgx-sound', v));

// Cache TG chats for instant startup display
telegramChats.subscribe(chats => {
    if (chats.length > 0) {
        localStorage.setItem('sgx-tg-chats', JSON.stringify(chats));
    }
});
