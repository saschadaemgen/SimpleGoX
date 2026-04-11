<script>
    import { createEventDispatcher, onMount } from 'svelte';
    import { currentUserId, telegramConnected } from '../../lib/stores.js';
    const dispatch = createEventDispatcher();

    export let telegramName = '';

    onMount(() => {
        console.log('=== ReadyStep mounted');
        const timer = setTimeout(() => {
            console.log('=== ReadyStep: auto-completing after 4s');
            dispatch('complete');
        }, 4000);
        return () => clearTimeout(timer);
    });
</script>

<div class="ready">
    <!-- CSS confetti -->
    <div class="confetti">
        {#each Array(12) as _, i}
            <div class="particle" style="--x:{Math.random()*100}%;--d:{0.2+Math.random()*0.8}s;--c:{['#3fb9a8','#61afef','#c678dd','#98c379','#e5c07b','#e06c75'][i%6]}"></div>
        {/each}
    </div>

    <div class="content">
        <div class="logo" style="--d:0"><span class="w">Simple</span><span class="ac">Go</span><span class="w">X</span></div>
        <h2 class="title" style="--d:1">You're all set!</h2>
        <p class="sub" style="--d:2">Your connected messengers:</p>

        <div class="accounts" style="--d:3">
            <div class="account">
                <span class="badge mx">MX</span>
                <span class="name">Matrix</span>
                <span class="detail">{$currentUserId || ''}</span>
                <span class="ok">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
                </span>
            </div>
            {#if $telegramConnected}
                <div class="account">
                    <span class="badge tg">TG</span>
                    <span class="name">Telegram</span>
                    <span class="detail">{telegramName || 'Connected'}</span>
                    <span class="ok">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
                    </span>
                </div>
            {/if}
        </div>

        <p class="hint" style="--d:4">You can manage your accounts anytime in Settings.</p>

        <button class="cta" style="--d:5" on:click={() => dispatch('complete')}>Start chatting</button>
    </div>
</div>

<style>
    .ready { display: flex; align-items: center; justify-content: center; height: 100%; text-align: center; position: relative; overflow: hidden; }

    /* Confetti */
    .confetti { position: absolute; inset: 0; pointer-events: none; }
    .particle {
        position: absolute; width: 6px; height: 6px; border-radius: 50%;
        background: var(--c); left: var(--x); bottom: -10px;
        opacity: 0; animation: float 2.5s ease-out var(--d) forwards;
    }
    @keyframes float {
        0% { opacity: 0; transform: translateY(0) rotate(0deg); }
        20% { opacity: 0.8; }
        100% { opacity: 0; transform: translateY(-100vh) rotate(720deg); }
    }

    .content { max-width: 420px; padding: 0 32px; position: relative; z-index: 1; }

    .logo, .title, .sub, .accounts, .hint, .cta {
        opacity: 0; animation: fadeUp 0.5s ease-out forwards;
        animation-delay: calc(var(--d) * 120ms + 200ms);
    }
    @keyframes fadeUp {
        from { opacity: 0; transform: translateY(12px); }
        to { opacity: 1; transform: translateY(0); }
    }

    .logo { font-size: 1.8em; font-weight: 700; letter-spacing: -0.5px; margin-bottom: 8px; }
    .w { color: #e6edf3; }
    .ac { color: var(--ac, #58a6ff); }

    .title { font-size: 1.3em; font-weight: 600; margin: 0 0 8px; }
    .sub { font-size: 0.88em; color: #8b949e; margin: 0 0 20px; }

    .accounts { display: flex; flex-direction: column; gap: 8px; margin-bottom: 20px; text-align: left; }
    .account {
        display: flex; align-items: center; gap: 10px; padding: 12px 16px;
        border-radius: 10px; background: rgba(255,255,255,0.03);
        border: 1px solid rgba(255,255,255,0.06);
    }
    .badge {
        padding: 3px 8px; border-radius: 6px; font-size: 0.65em; font-weight: 700; letter-spacing: 0.5px;
    }
    .mx { background: rgba(63,185,168,0.15); color: #3fb9a8; }
    .tg { background: rgba(97,175,239,0.15); color: #61afef; }
    .name { font-size: 0.88em; font-weight: 600; }
    .detail { flex: 1; font-size: 0.75em; color: #8b949e; font-family: 'JetBrains Mono', monospace; text-align: right; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .ok { color: #98c379; flex-shrink: 0; display: flex; }

    .hint { font-size: 0.78em; color: #8b949e; margin: 0 0 28px; }

    .cta {
        padding: 14px 48px; border: none; border-radius: 12px;
        background: var(--ac, #3fb9a8); color: #0e1117;
        font-size: 1em; font-weight: 600; font-family: inherit;
        cursor: pointer; transition: all 0.15s;
        animation: pulse 2s ease-in-out infinite;
    }
    @keyframes pulse {
        0%, 100% { box-shadow: 0 0 0 0 rgba(63,185,168,0.3); }
        50% { box-shadow: 0 0 0 8px rgba(63,185,168,0); }
    }
    .cta:hover { transform: translateY(-2px); }
</style>
