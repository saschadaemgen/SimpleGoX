import { writable, derived } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

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

// Tor/I2P routing (per-protocol, loaded from backend JSON on mount)
export const torRouting = writable({ matrix: 'direct', telegram: 'direct', simplex: 'direct', whatsapp: 'direct' });

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

// SimpleX
// Briefing 045 W3: the contact store is now a pure in-memory structure
// hydrated from the backend at app start. localStorage is NOT the source
// of truth; the sgx-simplex sidecar DB is. Legacy sgx-sx-contacts keys
// are cleared by the migration in App.svelte onMount.
//
// Shape of $simplexContacts:
//   { status: 'idle' | 'loading' | 'loaded' | 'error',
//     contacts: ContactSummaryDto[],
//     error: string | null,
//     lastLoadedAt: number | null }
//
// Consumers that just want the contact array use the derived store
// `simplexContactsList` below.
function createSimplexContactsStore() {
    const INITIAL = {
        status: 'idle',
        contacts: [],
        error: null,
        lastLoadedAt: null,
    };
    const { subscribe, set, update } = writable(INITIAL);

    return {
        subscribe,

        /// Block-loads the contact list from the backend. Called once
        /// during the App.svelte startup sequence (Briefing 045 W4)
        /// and from logout / wizard-completion paths thereafter.
        async loadFromBackend() {
            set({ status: 'loading', contacts: [], error: null, lastLoadedAt: null });
            try {
                const contacts = await invoke('sx_list_contacts');
                set({
                    status: 'loaded',
                    contacts,
                    error: null,
                    lastLoadedAt: Date.now(),
                });
                return contacts;
            } catch (e) {
                set({
                    status: 'error',
                    contacts: [],
                    error: String(e),
                    lastLoadedAt: null,
                });
                throw e;
            }
        },

        /// Silent refetch. No loading flash, keeps the current array
        /// visible while the request is in flight. Use after
        /// sx-contact-established / sx-contact-updated events.
        async refresh() {
            try {
                const contacts = await invoke('sx_list_contacts');
                update(s => ({ ...s, contacts, lastLoadedAt: Date.now() }));
                return contacts;
            } catch (e) {
                // refresh failure does not clobber the cached list
                console.warn('[simplexContacts] refresh failed:', e);
                return null;
            }
        },

        /// Explicit teardown for logout / app-exit (Briefing 045 W6).
        /// JS strings are GC-managed so this is best-effort reference
        /// release; the Tauri backend does the real zeroize on its
        /// Vec<ContactSummaryDto> copy when it falls out of scope.
        clear() {
            console.info('[simplexContacts] clear() called - dropping all contact data');
            set(INITIAL);
        },

        /// Incremental ConnState update from the per-contact lifecycle
        /// events (sx-contact-disconnected / reconnecting / reconnected
        /// / dead). Avoids a full refresh round-trip for these high-
        /// frequency events. Falls through silently if the contact is
        /// not in the current list (edge case: event fires before the
        /// initial loadFromBackend completes).
        applyStateUpdate(contactId, newConnState, attempt, maxAttempts) {
            update(s => ({
                ...s,
                contacts: s.contacts.map(c =>
                    c.contact_id === contactId
                        ? {
                            ...c,
                            conn_state: newConnState,
                            reconnect_attempt: attempt ?? null,
                            reconnect_max_attempts: maxAttempts ?? null,
                        }
                        : c
                ),
            }));
        },

        /// Optimistic last-message update from sx-new-message events.
        /// Keeps the sidebar sort order fresh without a round-trip.
        applyNewMessage(contactId, timestampMs, isOwn) {
            update(s => ({
                ...s,
                contacts: s.contacts.map(c => {
                    if (c.contact_id !== contactId) return c;
                    return {
                        ...c,
                        last_message_at_unix: Math.floor(timestampMs / 1000),
                        unread_count: isOwn ? c.unread_count : c.unread_count + 1,
                    };
                }),
            }));
        },
    };
}

export const simplexContacts = createSimplexContactsStore();

// Flat array view for consumers that iterate / find over the contact
// list (sidebar, chat view header, etc.). One-line migration from the
// old writable: `$simplexContacts` -> `$simplexContactsList`.
export const simplexContactsList = derived(
    simplexContacts,
    $s => $s.contacts
);

export const simplexContactsReady = derived(
    simplexContacts,
    $s => $s.status === 'loaded'
);

export const simplexMessages = writable({}); // { contactId: [ { msg_id, timestamp, body, is_own } ] }
export const simplexReady = writable(false);
export const simplexProfile = writable(null); // { display_name, full_name, bio } or null while unconfigured
export const simplexProfileDialogOpen = writable(false);

// Briefing 045 W4: global startup progress state. Drives the
// StartupScreen overlay shown before the main ChatLayout or Wizard
// renders. Phases are observed in order by App.svelte onMount, and
// 'ready' is the only value that lets the main UI mount.
//
//   'booting'           - Svelte just mounted, nothing done yet
//   'sidecar_waiting'   - awaiting sx-ready event from the Tauri backend
//   'loading_profile'   - calling sx_get_profile
//   'loading_contacts'  - calling sx_list_contacts
//   'ready'             - UI may mount (ChatLayout or Wizard)
//   'error'             - unrecoverable startup error; retry button shown
export const startupStatus = writable({
    phase: 'booting',
    message: 'Starting SimpleGoX...',
    detail: '',
    error: null,
});

// Toast notifications (lightweight, used across the app)
export const toasts = writable([]);
export function showToast(message, level = 'info', duration = 4000) {
    const id = Date.now() + Math.random();
    toasts.update(list => [...list, { id, message, level }]);
    setTimeout(() => {
        toasts.update(list => list.filter(t => t.id !== id));
    }, duration);
}

// Briefing 045 W3: the former subscriber that wrote `sgx-sx-contacts`
// into localStorage on every non-empty update is REMOVED. That block
// was structurally asymmetric - it wrote on length > 0 and never
// cleared on empty updates - which meant the cache outlived every
// contact deletion and produced the "ghost contacts" seen in live
// testing. The backend DB is now the single source of truth, reachable
// via `simplexContacts.loadFromBackend()` / `refresh()`.

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
