<script>
    import { currentRoomId, currentRoom, currentMessages, currentUserId, currentTyping, iotPanelOpen, roomInfoOpen, sendReadReceipts, messages, replyingTo, editingMessage, confirmDialog, telegramChats, telegramMessages, simplexContactsList, simplexMessages, showToast } from '../lib/stores.js';
    import { markAsRead, getRoomMessages, sendReaction, redactEvent, tgGetMessages, tgSendMessage } from '../lib/tauri.js';
    import { invoke } from '@tauri-apps/api/core';
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
        if (!roomId || roomId.startsWith('tg:')) return; // TG rooms use their own loader
        const existing = $messages[roomId];
        if (existing && existing.length > 0) { loadingHistory = false; return; }
        try {
            const history = await getRoomMessages(roomId, 50);
            if (history.length > 0) {
                messages.update(cur => ({ ...cur, [roomId]: history }));
            }
        } catch (e) {
            console.error('Failed to load history:', e);
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

    // SimpleX chat detection
    $: isSimplexChat = $currentRoomId?.startsWith('sx:');
    $: simplexContactId = isSimplexChat ? $currentRoomId.slice(3) : null;
    $: sxContactInfo = isSimplexChat ? $simplexContactsList.find(c => c.contact_id === simplexContactId) : null;
    $: sxChatMessages = isSimplexChat ? ($simplexMessages[simplexContactId] || []) : [];
    // Briefing 045 W3: conn_state lives on each contact now. 'connected'
    // / absent / missing sxContactInfo -> no dot rendered.
    $: sxHeaderConnState = sxContactInfo?.conn_state || 'connected';
    $: sxHeaderDotClass =
        sxHeaderConnState === 'dead'
            ? 'dead'
            : sxHeaderConnState === 'reconnecting'
              ? 'reconnecting'
              : '';
    $: sxHeaderTitle = (() => {
        if (!sxContactInfo) return '';
        if (sxHeaderConnState === 'dead') return 'Connection lost - restart app to retry';
        if (sxHeaderConnState === 'reconnecting') {
            const a = sxContactInfo.reconnect_attempt;
            const m = sxContactInfo.reconnect_max_attempts;
            if (a) return `Reconnecting (attempt ${a}/${m ?? '?'})...`;
            return 'Reconnecting...';
        }
        return '';
    })();

    $: chatTitle = isTelegramChat
        ? (tgChatInfo?.title || 'Telegram Chat')
        : (isSimplexChat
            ? (sxContactInfo?.display_name || 'SimpleX contact')
            : ($currentRoom?.name || 'Select a room'));

    // Load Telegram messages when a TG chat is selected (on-demand, serialized)
    let tgLoading = false;
    let tgLoadingChat = null;
    $: if (telegramChatId && telegramChatId !== tgLoadingChat) { loadTelegramMessages(telegramChatId); }

    async function loadTelegramMessages(chatId) {
        if (tgLoading) return; // Prevent concurrent loads (h2 fix)
        tgLoading = true;
        tgLoadingChat = chatId;
        try {
            console.log(`Loading TG messages for chat ${chatId}...`);
            const msgs = await tgGetMessages(chatId, 50);
            console.log(`Got ${msgs.length} TG messages`);
            msgs.reverse();
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
            // Wait for TDLib to process the sent message
            await new Promise(r => setTimeout(r, 500));
            // Reload all messages
            const msgs = await tgGetMessages(telegramChatId, 50);
            msgs.reverse();
            telegramMessages.update(cur => ({ ...cur, [telegramChatId]: msgs }));
        } catch (e) {
            console.error('Failed to send TG message:', e);
        }
    }

    // Real-time updates come via tg-new-message events from ChatLayout.
    // No polling needed.

    // SimpleX message sending (Briefing 041b Phase 6).
    // The sidecar emits sx-new-message with is_own=true on success; the
    // existing simplexMessages store subscriber in ChatLayout handles the
    // own-bubble render, so no optimistic UI is needed here.
    let sxInputText = '';
    let sxSending = false;

    async function sendSxMessage() {
        const text = sxInputText.trim();
        if (!text || !simplexContactId || sxSending) return;
        sxSending = true;
        try {
            await invoke('sx_send_message', { contactId: simplexContactId, body: text });
            sxInputText = '';
        } catch (e) {
            const msg = (e?.toString?.() ?? String(e)).replace(/^Error: /, '');
            console.error('sx_send_message failed:', msg);
            showToast('Failed to send: ' + msg, 'error');
        } finally {
            sxSending = false;
        }
    }
</script>

<div class="chat">
    <div class="head">
        <div class="left">
            {#if sxHeaderDotClass}
                <!-- Briefing 044 W6 + 045 W3: SimpleX connection lifecycle
                     dot for the currently-selected contact. Rendered only
                     when conn_state is reconnecting or dead (empty class
                     -> no dot on healthy sessions). -->
                <span class="hd-conn-dot {sxHeaderDotClass}" title={sxHeaderTitle} aria-label={sxHeaderTitle}></span>
            {/if}
            <span class="title">{chatTitle}</span>
            {#if isTelegramChat}
                <span class="tag-tg">TG</span>
            {:else if isSimplexChat}
                <span class="tag-sx">SX</span>
                <span class="tag-e2e">E2E</span>
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
        {#if $currentRoomId && !isTelegramChat && !isSimplexChat}
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
    {:else if isSimplexChat}
        <div class="msgs show" bind:this={container} on:scroll={onScroll}>
            {#each sxChatMessages as msg (msg.msg_id + '-' + (msg.is_own ? 'o' : 'p'))}
                <div class="sx-msg" class:own={msg.is_own}>
                    <div class="sx-body">{msg.body}</div>
                    {#if msg.timestamp}
                        <div class="sx-time">{new Date(msg.timestamp * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</div>
                    {/if}
                </div>
            {/each}
            {#if sxChatMessages.length === 0}
                <div class="empty"><p>No messages yet. Send something from the peer to see it here.</p></div>
            {/if}
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="sx-input active">
            <input
                type="text"
                placeholder="Encrypted SimpleX message..."
                bind:value={sxInputText}
                disabled={sxSending}
                on:keydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendSxMessage(); } }}
            />
            <button
                class="sx-send"
                on:click={sendSxMessage}
                disabled={!sxInputText.trim() || sxSending}
                title={sxSending ? 'Sending...' : 'Send (Enter)'}
            >
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

    /* Briefing 044 W6: SimpleX connection-state dot in the chat header. */
    .hd-conn-dot {
        display: inline-block;
        width: 9px; height: 9px; border-radius: 50%;
        margin-right: 6px; vertical-align: middle; flex-shrink: 0;
    }
    .hd-conn-dot.reconnecting {
        background: #f2c94c;
        box-shadow: 0 0 0 0 rgba(242, 201, 76, 0.55);
        animation: hdReconnectPulse 1.4s ease-in-out infinite;
    }
    .hd-conn-dot.dead {
        background: #e06c75;
        box-shadow: 0 0 8px rgba(224, 108, 117, 0.5);
    }
    @keyframes hdReconnectPulse {
        0%, 100% { box-shadow: 0 0 0 0 rgba(242, 201, 76, 0.55); }
        50%      { box-shadow: 0 0 0 6px rgba(242, 201, 76, 0); }
    }

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

    .tag-sx {
        padding: 2px 8px; border-radius: 5px; font-size: 0.65em; font-weight: 700;
        background: rgba(198,120,221,0.15); color: #c678dd; letter-spacing: 0.5px;
    }

    .sx-msg {
        max-width: 65%;
        padding: 8px 12px;
        border-radius: 12px;
        background: var(--bg-card, #161b22);
        align-self: flex-start;
        display: flex; flex-direction: column; gap: 2px;
    }
    .sx-msg.own {
        align-self: flex-end;
        background: rgba(198,120,221,0.18);
    }
    .sx-body { font-size: 0.92em; color: var(--text, #c9d1d9); word-break: break-word; }
    .sx-time { font-size: 0.7em; color: var(--text-3, #6e7681); align-self: flex-end; }

    .sx-input {
        display: flex; align-items: center; gap: 8px;
        padding: 10px 16px;
        border-top: 1px solid var(--border, rgba(240,246,252,0.04));
        background: var(--bg-card, #161b22);
        opacity: 0.55;
    }
    .sx-input.active { opacity: 1; }
    .sx-input input {
        flex: 1; padding: 10px 14px; border-radius: 10px;
        border: 1px solid var(--border, rgba(240,246,252,0.06));
        background: var(--bg, #0e1117); color: var(--text-3, #6e7681);
        font-size: 0.88em; font-family: inherit; outline: none;
        cursor: not-allowed;
    }
    .sx-input.active input {
        color: var(--text, #c9d1d9);
        cursor: text;
        transition: border-color 150ms;
    }
    .sx-input.active input:focus { border-color: #c678dd; }
    .sx-input.active input:disabled { opacity: 0.6; cursor: progress; }
    .sx-send {
        width: 36px; height: 36px; border-radius: 10px; border: none;
        background: #c678dd; color: #0e1117;
        display: flex; align-items: center; justify-content: center;
        flex-shrink: 0; cursor: not-allowed;
    }
    .sx-input.active .sx-send { cursor: pointer; transition: all 120ms; }
    .sx-input.active .sx-send:hover:not(:disabled) { background: #d68fe6; }
    .sx-send:disabled { background: rgba(198,120,221,0.35); }

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
