<script>
    import { currentRoomId, sendTypingNotices, replyingTo, editingMessage } from '../lib/stores.js';
    import { sendMessage, sendTyping, sendReply, editMessage } from '../lib/tauri.js';
    import { displayName } from '../lib/utils.js';
    import { get } from 'svelte/store';
    import EmojiPicker from './EmojiPicker.svelte';

    let text = '';
    let focused = false;
    let typingTimer = null;
    let showInputEmoji = false;
    let emojiBtnEl;

    function insertEmoji(emoji) { text += emoji; showInputEmoji = false; }

    // When entering edit mode, populate the input
    $: if ($editingMessage) { text = $editingMessage.body; }

    function send() {
        const body = text.trim();
        if (!body) return;
        const rid = get(currentRoomId);
        if (!rid) return;

        if ($editingMessage) {
            editMessage(rid, $editingMessage.eventId, body);
            editingMessage.set(null);
        } else if ($replyingTo) {
            sendReply(rid, body, $replyingTo.eventId);
            replyingTo.set(null);
        } else {
            sendMessage(rid, body);
        }

        text = '';
        if (get(sendTypingNotices)) { sendTyping(rid, false); clearTimeout(typingTimer); }
    }

    function cancelReply() { replyingTo.set(null); }
    function cancelEdit() { editingMessage.set(null); text = ''; }

    function onKey(e) { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); } if (e.key === 'Escape') { cancelReply(); cancelEdit(); } }

    function onInput() {
        const rid = get(currentRoomId);
        if (!rid || !get(sendTypingNotices)) return;
        sendTyping(rid, true);
        clearTimeout(typingTimer);
        typingTimer = setTimeout(() => sendTyping(rid, false), 4000);
    }
</script>

<div class="input-area">
    {#if $replyingTo}
        <div class="preview reply-preview">
            <div class="pbar"></div>
            <div class="pcontent">
                <span class="psender">{$replyingTo.senderDisplayName || displayName($replyingTo.sender)}</span>
                <span class="pbody">{$replyingTo.body}</span>
            </div>
            <button class="pclose" on:click={cancelReply} title="Cancel reply">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
            </button>
        </div>
    {/if}

    {#if $editingMessage}
        <div class="preview edit-preview">
            <svg viewBox="0 0 24 24" fill="none" stroke="var(--ac)" stroke-width="2" width="14" height="14"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
            <span class="plabel">Editing message</span>
            <button class="pclose" on:click={cancelEdit} title="Cancel edit">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
            </button>
        </div>
    {/if}

    <div class="bar">
        <button class="emoji-btn" bind:this={emojiBtnEl} on:click={() => showInputEmoji = !showInputEmoji} title="Emoji">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18"><circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/></svg>
        </button>
        <div class="wrap" class:focused>
            <input
                bind:value={text}
                on:keydown={onKey}
                on:input={onInput}
                on:focus={() => focused = true}
                on:blur={() => focused = false}
                placeholder="Type a message..."
            />
        </div>
        <button class="send" on:click={send} disabled={!text.trim()} title="Send">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>
        </button>
    </div>
    {#if showInputEmoji}
        <EmojiPicker
            position={{ x: emojiBtnEl?.getBoundingClientRect?.()?.left || 100, y: (emojiBtnEl?.getBoundingClientRect?.()?.top || 400) - 390 }}
            onSelect={insertEmoji}
            onClose={() => showInputEmoji = false}
        />
    {/if}
</div>

<style>
    .input-area { display: flex; flex-direction: column; }

    .preview {
        display: flex; align-items: center; gap: 8px;
        padding: 8px 20px; background: var(--bg-card); border-top: 1px solid var(--border);
    }

    .pbar { width: 3px; height: 28px; background: var(--ac); border-radius: 2px; flex-shrink: 0; }
    .pcontent { flex: 1; min-width: 0; }
    .psender { display: block; font-size: 0.78em; font-weight: 600; color: var(--ac); }
    .pbody { display: block; font-size: 0.82em; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .plabel { font-size: 0.82em; color: var(--ac); font-weight: 500; flex: 1; }
    .pclose { width: 24px; height: 24px; border: none; background: transparent; color: var(--text-3); cursor: pointer; border-radius: 4px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
    .pclose:hover { background: var(--bg-hover); color: var(--text); }

    .bar { padding: 11px 18px; display: flex; align-items: center; gap: 9px; border-top: 1px solid var(--border, rgba(240,246,252,0.04)); }

    .wrap { flex: 1; border-radius: 10px; border: 1px solid var(--border-2); transition: border-color 180ms, box-shadow 180ms; }
    .wrap.focused { border-color: var(--ac-border); box-shadow: 0 0 0 2px var(--ac-glow); }

    input { width: 100%; padding: 10px 14px; background: var(--bg-input); border: none; border-radius: 10px; color: var(--text); font-family: 'DM Sans', sans-serif; font-size: 0.88em; outline: none; }
    input::placeholder { color: var(--text-3); }

    .send { width: 38px; height: 38px; border-radius: 10px; border: none; background: var(--ac); color: white; cursor: pointer; display: flex; align-items: center; justify-content: center; transition: all 150ms var(--ease-b); }
    .send:hover:not(:disabled) { transform: scale(1.05); box-shadow: 0 0 12px var(--ac-glow); }
    .send:active:not(:disabled) { transform: scale(0.92); }
    .send:disabled { opacity: 0.4; cursor: not-allowed; }

    .emoji-btn { width: 38px; height: 38px; border-radius: 10px; border: none; background: transparent; color: #484f58; cursor: pointer; display: flex; align-items: center; justify-content: center; flex-shrink: 0; transition: color 150ms; }
    .emoji-btn:hover { color: var(--ac); }
</style>
