<script>
    import Avatar from './Avatar.svelte';
    import { formatTime, displayName } from '../lib/utils.js';
    import { currentMessages, currentRoomId } from '../lib/stores.js';
    import { sendReaction, redactEvent } from '../lib/tauri.js';
    import { get } from 'svelte/store';
    import { onMount, onDestroy } from 'svelte';

    export let group;
    export let onReplyClick = null;
    export let onReactClick = null;
    export let onEditClick = null;
    export let onRedactClick = null;

    $: isOwn = group.messages[0]?.is_own || false;
    $: senderName = group.messages[0]?.sender_display_name || displayName(group.sender);
    $: lastMsg = group.messages[group.messages.length - 1];

    $: firstReply = (() => {
        const m = group.messages[0];
        if (!m.reply_to_event_id) return null;
        if (m.reply_to_body) return { sender: m.reply_to_sender_display_name || displayName(m.reply_to_sender) || '...', body: m.reply_to_body };
        const orig = $currentMessages.find(o => o.event_id === m.reply_to_event_id);
        if (orig) return { sender: orig.sender_display_name || displayName(orig.sender), body: orig.body };
        return { sender: '...', body: 'Original message' };
    })();

    $: groupReactions = (() => {
        const merged = {};
        for (const msg of group.messages) {
            if (!msg.reactions) continue;
            for (const r of msg.reactions) {
                if (merged[r.key]) {
                    merged[r.key].count += r.count;
                    merged[r.key].includes_own = merged[r.key].includes_own || r.includes_own;
                    merged[r.key].event_ids = [...merged[r.key].event_ids, ...(r.event_ids || [])];
                } else {
                    merged[r.key] = { ...r, event_ids: [...(r.event_ids || [])] };
                }
            }
        }
        return Object.values(merged);
    })();

    $: hasReactions = groupReactions.length > 0;
    $: hasSignAbove = hasReactions || !!firstReply;

    function toggleReaction(rx) {
        const rid = get(currentRoomId);
        if (!rid) return;
        if (rx.includes_own && rx.event_ids?.length > 0) {
            redactEvent(rid, rx.event_ids[rx.event_ids.length - 1], null);
        } else {
            sendReaction(rid, lastMsg.event_id, rx.key);
        }
    }

    let hoveredIdx = -1;
    let touchIdx = -1;
    $: menuIdx = hoveredIdx >= 0 ? hoveredIdx : touchIdx;

    function toggleTouch(i) { touchIdx = touchIdx === i ? -1 : i; }
    function closeTouch(e) { if (!e.target.closest('.stacked-msg')) touchIdx = -1; }

    onMount(() => document.addEventListener('click', closeTouch));
    onDestroy(() => document.removeEventListener('click', closeTouch));

    // Reply expand/collapse
    let replyExpanded = false;
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="grp" class:own={isOwn}>

    {#if hasReactions}
        <div class="rx-row" class:own={isOwn}>
            <div class="rx-sign" class:own={isOwn}>
                {#each groupReactions as rx, i}
                    {#if i > 0}<span class="rx-sep" class:own={isOwn}></span>{/if}
                    <button class="rx-btn" on:click={() => toggleReaction(rx)}>{rx.key} <span class="rx-cnt">{rx.count}</span></button>
                {/each}
            </div>
        </div>
    {/if}

    {#if firstReply}
        <div class="reply" class:own={isOwn} class:below-rx={hasReactions} class:expanded={replyExpanded}>
            <div class="reply-accent"></div>
            <div class="reply-body">
                <div class="reply-top">
                    <span class="reply-who">{firstReply.sender}</span>
                    <span class="reply-txt">{firstReply.body}</span>
                </div>
                {#if replyExpanded}
                    <div class="reply-full">{firstReply.body}</div>
                {/if}
            </div>
            <button class="reply-toggle" on:click|stopPropagation={() => replyExpanded = !replyExpanded}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="12" height="12" class:flipped={replyExpanded}>
                    <polyline points="6 9 12 15 18 9"/>
                </svg>
            </button>
        </div>
    {/if}

    <div class="block" class:inc={!isOwn} class:has-top={hasSignAbove}>

        {#each group.messages as msg, i (msg.event_id || msg.timestamp)}
            {#if i > 0}
                <div class="sep" class:inc={!isOwn}></div>
            {/if}

            <div class="stacked-msg"
                 on:click={() => toggleTouch(i)}
                 on:mouseenter={() => { hoveredIdx = i; touchIdx = -1; }}
                 on:mouseleave={() => hoveredIdx = -1}>

                <div class="bub" class:own={isOwn}>
                    {#if msg.is_redacted}
                        <span class="del-text">Message deleted</span>
                    {:else}
                        {msg.body}
                        {#if msg.is_edited}<span class="edit-tag">(edited)</span>{/if}
                    {/if}
                </div>

                {#if !msg.is_redacted && !msg.event_id?.startsWith('local-')}
                    <div class="menu" class:own={isOwn} class:open={menuIdx === i}>
                        <button class="m-btn" on:click|stopPropagation={() => { touchIdx = -1; onReplyClick?.(msg); }} title="Reply">
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13"><polyline points="9 17 4 12 9 7"/><path d="M20 18v-2a4 4 0 0 0-4-4H4"/></svg>
                        </button>
                        <span class="m-sep" class:own={isOwn}></span>
                        <button class="m-btn" on:click|stopPropagation={(e) => { touchIdx = -1; onReactClick?.(msg, e.currentTarget); }} title="React">
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13"><circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/></svg>
                        </button>
                        {#if isOwn}
                            <span class="m-sep" class:own={isOwn}></span>
                            <button class="m-btn" on:click|stopPropagation={() => { touchIdx = -1; onEditClick?.(msg); }} title="Edit">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                            </button>
                        {/if}
                        <span class="m-sep" class:own={isOwn}></span>
                        <button class="m-btn m-danger" on:click|stopPropagation={() => { touchIdx = -1; onRedactClick?.(msg); }} title="Delete">
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="13" height="13"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
                        </button>
                    </div>
                {/if}
            </div>
        {/each}

        <div class="bar-sep" class:inc={!isOwn}></div>

        <div class="bar" class:own={isOwn}>
            <span class="bar-name" class:own={isOwn}>{senderName}</span>
            <span class="dot" class:own={isOwn}></span>
            <span class="fill"></span>
            <span class="info">
                {#if lastMsg.is_edited}<span class="ed">edited</span>{/if}
                {#if isOwn}<span class="ck"><svg viewBox="0 0 16 16"><path d="M1.5 8.5l3 3 5-6"/><path d="M5.5 8.5l3 3 5-6" opacity="0.5"/></svg></span>{/if}
                <span class="time">{formatTime(lastMsg.timestamp)}</span>
            </span>
        </div>
    </div>

    <div class="avatar" class:own={isOwn}>
        <Avatar mxcUri={group.messages[0]?.sender_avatar_url} name={senderName} size={50} borderRadius="50%" />
    </div>
</div>

<style>
    /* ===== GROUP ===== */
    .grp {
        position: relative;
        max-width: 380px;
        min-width: 160px;
        padding-bottom: 36px;
        margin-bottom: 4px;
        margin-left: 10px;
        margin-right: 10px;
    }
    .grp.own { margin-left: auto; margin-right: 10px; }

    /* ===== REACTIONS ===== */
    .rx-row { display: flex; }
    .rx-row.own { justify-content: flex-end; }
    .rx-sign { display: inline-flex; align-items: center; padding: 4px 6px; border-radius: 8px 8px 0 0; background: #161b22; border: 1px solid #2a2f38; border-bottom: none; }
    .rx-sign.own { background: var(--accent, #3fb9a8); border: none; }
    .rx-btn { display: inline-flex; align-items: center; gap: 2px; padding: 0 6px; height: 18px; font-size: 12px; border: none; background: none; color: inherit; cursor: pointer; font-family: inherit; }
    .rx-btn:hover { opacity: 0.7; }
    .rx-cnt { font-size: 9px; font-weight: 600; }
    .rx-sep { width: 1px; height: 12px; background: rgba(255,255,255,0.1); flex-shrink: 0; }
    .rx-sep.own { background: rgba(255,255,255,0.25); }

    /* No separator - reactions sit directly on next element */

    /* ===== REPLY ===== */
    .reply { display: flex; gap: 6px; padding: 7px 12px; width: 87%; border-radius: 8px 8px 0 0; background: #1a1f27; border: 1px solid #2a2f38; border-bottom: none; align-items: flex-start; }
    .reply.own { background: #2d8a7d; border: none; margin-left: auto; }
    .reply.below-rx:not(.own) { border-radius: 0 8px 0 0; border-top: none; }
    .reply.below-rx.own { border-radius: 8px 0 0 0; }
    .reply-accent { width: 2px; min-height: 100%; border-radius: 1px; background: var(--accent, #3fb9a8); flex-shrink: 0; }
    .reply.own .reply-accent { background: rgba(255,255,255,0.4); }
    .reply-body { flex: 1; min-width: 0; }
    .reply-top { display: flex; align-items: center; gap: 6px; position: relative; top: 2px; }
    .reply-who { font-size: 11px; font-weight: 600; color: var(--accent, #3fb9a8); flex-shrink: 0; }
    .reply.own .reply-who { color: rgba(255,255,255,0.85); }
    .reply-txt { font-size: 12px; color: #8b949e; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; min-width: 0; }
    .reply.own .reply-txt { color: rgba(255,255,255,0.55); }
    .reply-full { font-size: 12px; color: #8b949e; margin-top: 4px; line-height: 1.4; white-space: pre-wrap; word-wrap: break-word; }
    .reply.own .reply-full { color: rgba(255,255,255,0.55); }
    .reply.expanded .reply-txt { display: none; }

    .reply-toggle { width: 20px; height: 20px; border: none; background: none; color: #8b949e; cursor: pointer; display: flex; align-items: center; justify-content: center; flex-shrink: 0; border-radius: 4px; transition: background 0.1s; }
    .reply-toggle:hover { background: rgba(255,255,255,0.08); }
    .reply.own .reply-toggle { color: rgba(255,255,255,0.5); }
    .reply-toggle svg { transition: transform 0.15s ease; }
    .reply-toggle svg.flipped { transform: rotate(180deg); }

    /* ===== BLOCK ===== */
    .block { overflow: hidden; }
    .block.inc { border: 1px solid #2a2f38; border-radius: 12px 12px 12px 0; }
    .block:not(.inc) { border-radius: 12px 12px 0 12px; }
    .block.inc.has-top { border-radius: 0 12px 12px 0; }
    .block:not(.inc).has-top { border-radius: 12px 0 0 12px; }

    /* ===== MESSAGES ===== */
    .stacked-msg { position: relative; }
    .bub { padding: 10px 14px; font-size: 14px; line-height: 1.45; word-wrap: break-word; background: #161b22; }
    .bub.own { background: var(--accent, #3fb9a8); color: #fff; }
    .del-text { font-style: italic; color: #8b949e; }
    .edit-tag { font-size: 10px; opacity: 0.5; margin-left: 4px; }

    /* ===== SEPARATORS ===== */
    .sep { height: 2px; }
    .sep.inc { background: var(--accent, #3fb9a8); }
    .sep:not(.inc) { background: #0e1117; }
    .bar-sep { height: 2px; }
    .bar-sep.inc { background: var(--accent, #3fb9a8); }
    .bar-sep:not(.inc) { background: #0e1117; }

    /* ===== INFO BAR ===== */
    .bar { display: flex; align-items: center; height: 24px; padding: 0 12px 0 38px; font-size: 11px; background: #161b22; color: #8b949e; }
    .bar.own { background: var(--accent, #3fb9a8); color: rgba(255,255,255,0.7); padding: 0 38px 0 12px; flex-direction: row-reverse; }
    .bar-name { font-size: 11px; font-weight: 600; white-space: nowrap; color: var(--accent, #3fb9a8); flex-shrink: 0; position: relative; top: -2px; }
    .bar-name.own { color: rgba(255,255,255,0.85); }
    .dot { width: 3px; height: 3px; border-radius: 50%; background: #8b949e; opacity: 0.3; flex-shrink: 0; margin: 0 6px; }
    .dot.own { background: rgba(255,255,255,0.5); }
    .fill { flex: 1; }
    .info { display: flex; align-items: center; gap: 6px; flex-shrink: 0; position: relative; top: -1px; }
    .ed { font-size: 9px; opacity: 0.6; }
    .ck svg { width: 13px; height: 13px; display: block; }
    .ck path { stroke: currentColor; stroke-width: 2.5; fill: none; stroke-linecap: round; stroke-linejoin: round; }
    .time { font-size: 10px; white-space: nowrap; }

    /* ===== AVATAR ===== */
    .avatar { position: absolute; width: 56px; height: 56px; border-radius: 50%; border: none; z-index: 2; overflow: hidden; left: -28px; bottom: 18px; background: #0e1117; }
    .avatar.own { left: auto; right: -28px; background: #0e1117; }

    /* ===== INLINE MENU ===== */
    .menu { display: flex; align-items: center; justify-content: center; overflow: hidden; height: 0; opacity: 0; background: #1c2129; transition: height 0.15s ease, opacity 0.15s ease; }
    .menu.open { height: 28px; opacity: 1; }
    .menu.own { background: #2d8a7d; }

    /* Close animation via max-height trick - menu collapses when removed from DOM with transition */

    .m-btn { height: 28px; padding: 0 12px; border: none; background: none; cursor: pointer; display: flex; align-items: center; justify-content: center; color: #8b949e; transition: color 0.1s, background 0.1s; }
    .menu.own .m-btn { color: rgba(255,255,255,0.6); }
    .m-btn:hover { color: #e6edf3; background: rgba(255,255,255,0.06); }
    .menu.own .m-btn:hover { color: #fff; background: rgba(255,255,255,0.12); }
    .m-danger:hover { color: #e06c75 !important; background: rgba(224,108,117,0.12) !important; }
    .m-sep { width: 1px; height: 14px; background: rgba(255,255,255,0.06); flex-shrink: 0; }
    .m-sep.own { background: rgba(255,255,255,0.18); }
</style>
