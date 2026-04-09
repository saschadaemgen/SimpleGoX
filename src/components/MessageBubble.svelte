<script>
    import { formatTime, displayName } from '../lib/utils.js';
    import { currentMessages, currentRoomId } from '../lib/stores.js';
    import { sendReaction, redactEvent } from '../lib/tauri.js';
    import { get } from 'svelte/store';
    import Avatar from './Avatar.svelte';

    export let msg;
    export let isOwn = false;
    export let showTail = false;
    export let onReplyClick = null;
    export let onReactClick = null;
    export let onEditClick = null;
    export let onRedactClick = null;

    let showActions = false;

    $: replyInfo = (() => {
        if (!msg.reply_to_event_id) return null;
        const orig = $currentMessages.find(m => m.event_id === msg.reply_to_event_id);
        if (orig) return { sender: orig.sender_display_name || displayName(orig.sender), body: orig.body };
        return { sender: '...', body: 'Original message' };
    })();

    $: senderName = msg.sender_display_name || displayName(msg.sender);
    $: hasReactions = msg.reactions && msg.reactions.length > 0;
    $: hasQuote = !!replyInfo;
    $: hasSignAbove = hasReactions || hasQuote;

    function handleReact(e) { e.stopPropagation(); onReactClick?.(msg, e.currentTarget); }

    function toggleReaction(reaction) {
        const rid = get(currentRoomId);
        if (!rid) return;
        if (reaction.includes_own && reaction.event_ids?.length > 0) {
            redactEvent(rid, reaction.event_ids[reaction.event_ids.length - 1], null);
        } else {
            sendReaction(rid, msg.event_id, reaction.key);
        }
    }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="msg-wrap" class:has-av={showTail} on:mouseenter={() => showActions = true} on:mouseleave={() => showActions = false}>

    <!-- 1. Reactions sign (top, dynamic width) -->
    {#if hasReactions}
        <div class="rx-row" class:own={isOwn}>
            <div class="rx-sign" class:own={isOwn}>
                {#each msg.reactions as rx, i}
                    {#if i > 0}<span class="rx-div" class:own={isOwn}></span>{/if}
                    <button class="rx" on:click={() => toggleReaction(rx)}>{rx.key} <span class="rx-n">{rx.count}</span></button>
                {/each}
            </div>
        </div>
    {/if}

    <!-- Separator between reactions and reply -->
    {#if hasReactions && hasQuote}
        <div class="sign-sep" class:own={isOwn}></div>
    {/if}

    <!-- 2. Reply quote -->
    {#if hasQuote}
        <div class="quote" class:own={isOwn} class:below-reactions={hasReactions}>
            <div class="q-bar"></div>
            <div class="q-content">
                <div class="q-who">{replyInfo.sender}</div>
                <div class="q-txt">{replyInfo.body}</div>
            </div>
        </div>
    {/if}

    <!-- 3. Bubble group -->
    <div class="bubble-group" class:own={isOwn} class:incoming={!isOwn} class:has-sign-above={hasSignAbove}>
        <div class="bub" class:own={isOwn}>
            {#if msg.is_redacted}
                <span class="redacted">Message deleted</span>
            {:else}
                {msg.body}
                {#if msg.is_edited}<span class="edited-inline">(edited)</span>{/if}
            {/if}
        </div>
        <div class="split"></div>
        <div class="bar" class:own={isOwn}>
            <div class="bar-inner" class:own={isOwn}>
                <span class="bar-name" class:own={isOwn}>{senderName}</span>
                <span class="bar-dot" class:own={isOwn}></span>
                <span class="spacer"></span>
                <div class="meta">
                    {#if msg.is_edited}<span class="ed">edited</span>{/if}
                    {#if isOwn}
                        <span class="ck"><svg viewBox="0 0 16 16"><path d="M1.5 8.5l3 3 5-6"/><path d="M5.5 8.5l3 3 5-6" opacity="0.5"/></svg></span>
                    {/if}
                    <span class="tm">{formatTime(msg.timestamp)}</span>
                </div>
            </div>
        </div>
    </div>

    <!-- Avatar (only on last message in group) -->
    {#if showTail}
        <div class="av" class:own={isOwn}>
            <Avatar mxcUri={msg.sender_avatar_url} name={senderName} size={50} borderRadius="50%" />
        </div>
    {/if}

    <!-- Hover actions -->
    {#if showActions && !msg.is_redacted && !msg.event_id.startsWith('local-')}
        <div class="actions" class:own={isOwn}>
            <button class="act-btn" on:click={() => onReplyClick?.(msg)} title="Reply">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><polyline points="9 17 4 12 9 7"/><path d="M20 18v-2a4 4 0 0 0-4-4H4"/></svg>
            </button>
            <button class="act-btn" on:click={handleReact} title="React">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/></svg>
            </button>
            {#if isOwn}
                <button class="act-btn" on:click={() => onEditClick?.(msg)} title="Edit">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                </button>
            {/if}
            <button class="act-btn danger" on:click={() => onRedactClick?.(msg)} title="Delete">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
            </button>
        </div>
    {/if}
</div>

<style>
    .msg-wrap { position: relative; max-width: 360px; min-width: 160px; padding-bottom: 4px; }
    .msg-wrap.has-av { padding-bottom: 36px; }

    /* Fix 4: Reactions row for alignment, sign is inline-flex (dynamic width) */
    .rx-row { display: flex; }
    .rx-row.own { justify-content: flex-end; }
    /* Fix 1: padding 4px 6px (less left) */
    .rx-sign { display: inline-flex; align-items: center; gap: 0; padding: 4px 6px; border-radius: 8px 8px 0 0; background: #1a1f27; border: 1px solid #2a2f38; border-bottom: none; }
    .rx-sign.own { background: color-mix(in srgb, var(--ac, #3fb9a8) 70%, #000); border: none; }
    .rx { display: inline-flex; align-items: center; gap: 2px; padding: 0 6px; font-size: 12px; cursor: pointer; height: 18px; border: none; background: transparent; color: inherit; font-family: inherit; }
    .rx:hover { opacity: 0.7; }
    .rx-n { font-size: 9px; font-weight: 600; }
    .rx-div { width: 1px; height: 12px; flex-shrink: 0; background: rgba(255,255,255,0.1); }
    .rx-sign.own .rx-div { background: rgba(255,255,255,0.25); }

    /* Fix 3: Soft separator between reactions and reply */
    .sign-sep { height: 1px; background: rgba(255, 255, 255, 0.04); }
    .sign-sep.own { background: rgba(255, 255, 255, 0.08); }

    /* Reply quote */
    .quote { display: flex; gap: 6px; padding: 5px 10px; font-size: 12px; border-radius: 8px 8px 0 0; width: 87%; background: #1a1f27; border: 1px solid #2a2f38; border-bottom: none; }
    .quote.own { background: color-mix(in srgb, var(--ac, #3fb9a8) 70%, #000); border: none; margin-left: auto; }
    /* Fix 2: When reactions above - bubble-side corner is sharp */
    .quote.below-reactions:not(.own) { border-radius: 0 8px 0 0; border-top: none; }
    .quote.below-reactions.own { border-radius: 8px 0 0 0; }
    .q-bar { width: 2px; border-radius: 1px; background: var(--ac, #3fb9a8); flex-shrink: 0; }
    .quote.own .q-bar { background: rgba(255,255,255,0.4); }
    .q-content { min-width: 0; }
    .q-who { font-size: 11px; font-weight: 600; color: var(--ac, #3fb9a8); }
    .quote.own .q-who { color: rgba(255,255,255,0.85); }
    .q-txt { font-size: 12px; color: #8b949e; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .quote.own .q-txt { color: rgba(255,255,255,0.55); }

    /* Bubble group */
    .bubble-group { overflow: hidden; }
    .bubble-group.incoming { border: 1px solid #2a2f38; border-radius: 12px 12px 12px 0; }
    .bubble-group:not(.incoming) { border-radius: 12px 12px 0 12px; }
    /* Fix 6: has-sign-above adjusts top corners */
    .bubble-group.incoming.has-sign-above { border-radius: 0 12px 12px 0; }
    .bubble-group:not(.incoming).has-sign-above { border-radius: 12px 0 0 12px; }

    .bub { padding: 10px 14px; font-size: 14px; line-height: 1.45; word-wrap: break-word; background: #161b22; }
    .bub.own { background: var(--ac, #3fb9a8); color: #fff; }
    .redacted { font-style: italic; color: #8b949e; }
    .edited-inline { font-size: 10px; opacity: 0.5; margin-left: 4px; }

    /* Split line - accent on incoming (2px), bg on own (3px) */
    .bubble-group.incoming .split { height: 2px; background: var(--ac, #3fb9a8); }
    .bubble-group:not(.incoming) .split { height: 2.5px; background: #0e1117; }

    .bar { display: flex; align-items: center; font-size: 11px; height: 24px; padding: 0 12px; background: #161b22; color: #8b949e; padding-left: 38px; }
    .bar.own { background: var(--ac, #3fb9a8); color: rgba(255,255,255,0.7); padding-left: 12px; padding-right: 38px; flex-direction: row-reverse; }
    .bar-inner { display: flex; align-items: center; width: 100%; gap: 6px; }
    .bar-inner.own { flex-direction: row-reverse; }
    .bar-name { font-size: 11px; font-weight: 600; white-space: nowrap; flex-shrink: 0; color: var(--ac, #3fb9a8); }
    .bar-name.own { color: rgba(255,255,255,0.85); }
    .bar-dot { width: 3px; height: 3px; border-radius: 50%; flex-shrink: 0; background: #8b949e; opacity: 0.3; }
    .bar-dot.own { background: rgba(255,255,255,0.5); }
    .spacer { flex: 1; }

    .meta { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
    .ed { font-size: 9px; opacity: 0.6; }
    .ck svg { width: 13px; height: 13px; display: block; }
    .ck path { stroke: currentColor; stroke-width: 2.5; fill: none; stroke-linecap: round; stroke-linejoin: round; }
    .tm { font-size: 10px; white-space: nowrap; }

    .av { position: absolute; width: 56px; height: 56px; border-radius: 50%; display: flex; align-items: center; justify-content: center; border: 3px solid #0e1117; z-index: 2; left: -28px; bottom: -5px; overflow: hidden; }
    .av.own { left: auto; right: -28px; }

    .actions { position: absolute; top: -16px; right: 0; display: flex; gap: 2px; background: #161b22; border: 1px solid #2a2f38; border-radius: 8px; padding: 3px; box-shadow: 0 2px 8px rgba(0,0,0,0.3); z-index: 10; animation: actIn 0.1s ease; }
    .actions.own { right: auto; left: 0; }
    @keyframes actIn { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: translateY(0); } }
    .act-btn { width: 28px; height: 28px; border: none; background: transparent; color: #8b949e; cursor: pointer; border-radius: 6px; display: flex; align-items: center; justify-content: center; transition: background 0.1s ease, color 0.1s ease; }
    .act-btn:hover { background: rgba(255,255,255,0.06); color: #e6edf3; }
    .act-btn.danger:hover { background: rgba(224,108,117,0.15); color: #e06c75; }
</style>
