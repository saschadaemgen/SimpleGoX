<script>
    import { currentRoomId, currentRoom, currentMessages, currentUserId, currentTyping, iotPanelOpen, roomInfoOpen, sendReadReceipts, messages, replyingTo, editingMessage, confirmDialog, telegramChats, telegramMessages } from '../lib/stores.js';
    import { markAsRead, getRoomMessages, sendReaction, redactEvent, tgGetMessages, tgSendMessage } from '../lib/tauri.js';
    import { groupMessages, needsDateSep } from '../lib/utils.js';
    import { get } from 'svelte/store';
    import MessageGroup from './MessageGroup.svelte';
    import MessageInput from './MessageInput.svelte';
    import TypingIndicator from './TypingIndicator.svelte';
    import DateSeparator from './DateSeparator.svelte';
    import EmojiPicker from './EmojiPicker.svelte';
    import { afterUpdate, tick } from 'svelte';

    let showEmojiPicker = false;
    let emojiPickerTarget = null;

    function handleReply(msg) {
        replyingTo.set({ eventId: msg.event_id, sender: msg.sender, senderDisplayName: msg.sender_display_name, body: msg.body });
    }

    function handleReact(msg, btnEl) {
        const rect = btnEl?.getBoundingClientRect?.() || { left: 200, top: 200, bottom: 230 };
        let y = rect.top - 390;
        if (y < 10) y = rect.bottom + 8;
        emojiPickerTarget = { eventId: msg.event_id, x: rect.left, y };
        showEmojiPicker = true;
    }

    async function handleEmojiSelect(emoji) {
        if (!emojiPickerTarget) return;
        const rid = get(currentRoomId);
        if (rid) await sendReaction(rid, emojiPickerTarget.eventId, emoji);
        showEmojiPicker = false;
        emojiPickerTarget = null;
    }

    function handleEdit(msg) {
        editingMessage.set({ eventId: msg.event_id, body: msg.body });
    }

    function handleRedact(msg) {
        confirmDialog.set({
            visible: true, title: 'Delete Message',
            message: 'Delete this message? This cannot be undone.',
            confirmText: 'Delete', danger: true,
            onConfirm: async () => {
                const rid = get(currentRoomId);
                if (rid) await redactEvent(rid, msg.event_id, null);
            },
        });
    }

    let container;
    let autoScroll = true;
    let prevRoom = null;
    let fadeIn = false;
    let loadingHistory = false;

    $: if ($currentRoomId !== prevRoom) {
        prevRoom = $currentRoomId;
        fadeIn = false;
        loadingHistory = true;
        loadHistory($currentRoomId).then(() => {
            tick().then(() => { fadeIn = true; autoScroll = true; loadingHistory = false; });
        });
    }

    async function loadHistory(roomId) {
        if (!roomId) return;
        const existing = $messages[roomId];
        if (existing && existing.length > 0) { loadingHistory = false; return; }
        const history = await getRoomMessages(roomId, 50);
        if (history.length > 0) {
            messages.update(cur => ({ ...cur, [roomId]: history }));
        }
    }

    $: groups = groupMessages($currentMessages);

    // Read receipt on room change
    $: if ($currentRoomId && $currentMessages.length > 0 && $sendReadReceipts) {
        const last = $currentMessages[$currentMessages.length - 1];
        if (last.event_id && last.sender !== $currentUserId) {
            markAsRead($currentRoomId, last.event_id);
        }
    }

    afterUpdate(() => {
        if (autoScroll && container) {
            container.scrollTo({ top: container.scrollHeight, behavior: 'smooth' });
        }
    });

    function onScroll() {
        if (!container) return;
        const { scrollTop, scrollHeight, clientHeight } = container;
        autoScroll = scrollHeight - scrollTop - clientHeight < 100;
    }

    function toggleIot() { iotPanelOpen.update(v => !v); }

    // Telegram chat detection
    $: isTelegramChat = $currentRoomId?.startsWith('tg:');
    $: telegramChatId = isTelegramChat ? $currentRoomId.slice(3) : null;
    $: tgChatInfo = isTelegramChat ? $telegramChats.find(c => 'tg:' + c.id === $currentRoomId) : null;
    $: chatTitle = isTelegramChat ? (tgChatInfo?.title || 'Telegram Chat') : ($currentRoom?.name || 'Select a room');

    // Load Telegram messages when a TG chat is selected
    let tgLoading = false;
    $: if (telegramChatId) { loadTelegramMessages(telegramChatId); }

    async function loadTelegramMessages(chatId) {
        const existing = $telegramMessages[chatId];
        if (existing && existing.length > 0) return;
        tgLoading = true;
        try {
            const msgs = await tgGetMessages(chatId, 50);
            telegramMessages.update(cur => ({ ...cur, [chatId]: msgs }));
        } catch (e) {
            console.error('Failed to load TG messages:', e);
        } finally {
            tgLoading = false;
        }
    }

    // Current Telegram messages and groups
    $: currentTgMessages = isTelegramChat ? ($telegramMessages[telegramChatId] || []) : [];
    $: tgGroups = isTelegramChat ? groupMessages(currentTgMessages) : [];

    // Telegram message sending
    let tgInputText = '';
    async function sendTgMessage() {
        if (!tgInputText.trim() || !telegramChatId) return;
        const text = tgInputText.trim();
        tgInputText = '';
        try {
            await tgSendMessage(telegramChatId, text);
            // Reload messages to show the new one
            const msgs = await tgGetMessages(telegramChatId, 50);
            telegramMessages.update(cur => ({ ...cur, [telegramChatId]: msgs }));
        } catch (e) {
            console.error('Failed to send TG message:', e);
        }
    }
</script>

<div class="chat">
    <div class="head">
        <div class="left">
            <span class="title">{chatTitle}</span>
            {#if isTelegramChat}
                <span class="tag-tg">TG</span>
            {:else}
                {#if $currentRoom?.is_encrypted}
                    <span class="tag-e2e">E2E</span>
                {/if}
                {#if $currentRoom?.name?.toLowerCase().includes('iot')}
                    <button class="tag-iot" class:on={$iotPanelOpen} on:click={toggleIot}>
                        <span class="pdot"></span>IoT
                    </button>
                {/if}
            {/if}
        </div>
        {#if $currentRoomId && !isTelegramChat}
            <button class="info-btn" on:click={() => roomInfoOpen.update(v => !v)} title="Room Info">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
            </button>
        {/if}
    </div>

    {#if isTelegramChat}
        <div class="msgs show" bind:this={container} on:scroll={onScroll}>
            {#if tgLoading}
                <div class="loading-msgs">Loading Telegram messages...</div>
            {/if}
            {#each tgGroups as group, gi (group.messages[0].event_id || gi)}
                {#if gi === 0 || needsDateSep(group.messages[0], tgGroups[gi - 1].messages[tgGroups[gi - 1].messages.length - 1])}
                    <DateSeparator timestamp={group.messages[0].timestamp} />
                {/if}
                <MessageGroup {group} onReplyClick={null} onReactClick={null} onEditClick={null} onRedactClick={null} />
            {/each}
            {#if tgGroups.length === 0 && !tgLoading}
                <div class="empty"><p>No messages yet</p></div>
            {/if}
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="tg-input">
            <input
                type="text"
                placeholder="Telegram message..."
                bind:value={tgInputText}
                on:keydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) sendTgMessage(); }}
            />
            <button class="tg-send" on:click={sendTgMessage} disabled={!tgInputText.trim()}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>
            </button>
        </div>
    {:else if $currentRoomId}
        <div class="msgs" class:show={fadeIn} bind:this={container} on:scroll={onScroll}>
            {#if loadingHistory}
                <div class="loading-msgs">Loading messages...</div>
            {/if}
            {#each groups as group, gi (group.messages[0].event_id || group.messages[0].timestamp)}
                {#if gi === 0 || needsDateSep(group.messages[0], groups[gi - 1].messages[groups[gi - 1].messages.length - 1])}
                    <DateSeparator timestamp={group.messages[0].timestamp} />
                {/if}
                <MessageGroup {group} onReplyClick={handleReply} onReactClick={handleReact} onEditClick={handleEdit} onRedactClick={handleRedact} />
            {/each}
            <TypingIndicator users={$currentTyping} />
        </div>
        <MessageInput />
    {:else}
        <div class="empty"><p>Select a room to start chatting</p></div>
    {/if}
    {#if showEmojiPicker && emojiPickerTarget}
        <EmojiPicker
            position={{ x: emojiPickerTarget.x, y: emojiPickerTarget.y }}
            onSelect={handleEmojiSelect}
            onClose={() => { showEmojiPicker = false; emojiPickerTarget = null; }}
        />
    {/if}
</div>

<style>
    .chat { flex: 1; display: flex; flex-direction: column; background: var(--bg); min-width: 0; }

    .head {
        padding: 13px 22px; display: flex; align-items: center; justify-content: space-between;
        border-bottom: 1px solid var(--ac-border);
        background: rgba(14, 17, 23, 0.85); backdrop-filter: blur(10px); z-index: 2;
    }
    .left { display: flex; align-items: center; gap: 9px; flex: 1; }

    .info-btn { width: 30px; height: 30px; border-radius: 8px; border: none; background: transparent; color: var(--text-3); cursor: pointer; display: flex; align-items: center; justify-content: center; flex-shrink: 0; transition: all 120ms; }
    .info-btn:hover { background: var(--bg-hover); color: var(--text-2); }
    .title { font-size: 0.95em; font-weight: 600; }

    .tag-e2e {
        padding: 2px 8px; border-radius: 5px; font-size: 0.65em; font-weight: 600;
        background: var(--ac-bg); color: var(--ac); border: 1px solid var(--ac-border);
    }
    .tag-iot {
        padding: 2px 8px; border-radius: 5px; font-size: 0.65em; font-weight: 600;
        background: var(--ac-bg); color: var(--ac); border: 1px solid var(--ac-border);
        cursor: pointer; display: flex; align-items: center; gap: 4px;
        transition: all 180ms var(--ease); font-family: inherit;
    }
    .tag-iot:hover { background: rgba(63, 185, 168, 0.12); }
    .tag-iot.on { background: rgba(63, 185, 168, 0.14); border-color: var(--ac-line); }
    .pdot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; animation: pd 2s infinite; }
    @keyframes pd { 0%,100% { opacity: 0.3; } 50% { opacity: 1; } }

    .msgs {
        flex: 1; overflow-y: auto; padding: 14px 24px;
        display: flex; flex-direction: column; gap: 5px;
        opacity: 0; transition: opacity 150ms ease;
    }
    .msgs.show { opacity: 1; }

    .empty { flex: 1; display: flex; align-items: center; justify-content: center; color: var(--text-3); font-size: 0.88em; }
    .loading-msgs { text-align: center; color: var(--text-3); font-size: 0.82em; padding: 16px; }

    .tag-tg {
        padding: 2px 8px; border-radius: 5px; font-size: 0.65em; font-weight: 700;
        background: rgba(97,175,239,0.15); color: #61afef; letter-spacing: 0.5px;
    }

    .tg-input {
        display: flex; align-items: center; gap: 8px;
        padding: 10px 16px;
        border-top: 1px solid var(--border, rgba(240,246,252,0.04));
        background: var(--bg-card, #161b22);
    }
    .tg-input input {
        flex: 1; padding: 10px 14px; border-radius: 10px;
        border: 1px solid var(--border, rgba(240,246,252,0.06));
        background: var(--bg, #0e1117); color: var(--text, #c9d1d9);
        font-size: 0.88em; font-family: inherit; outline: none;
        transition: border-color 150ms;
    }
    .tg-input input:focus { border-color: #61afef; }
    .tg-send {
        width: 36px; height: 36px; border-radius: 10px; border: none;
        background: #61afef; color: #0e1117; cursor: pointer;
        display: flex; align-items: center; justify-content: center;
        transition: all 120ms; flex-shrink: 0;
    }
    .tg-send:hover:not(:disabled) { background: #7bc0f5; }
    .tg-send:disabled { opacity: 0.4; cursor: default; }
</style>
