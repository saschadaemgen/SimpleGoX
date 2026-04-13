<script>
    import { listen } from '@tauri-apps/api/event';
    import { onMount, onDestroy } from 'svelte';

    let status = '';
    let network = '';
    let currentDetail = '';
    let visible = false;
    let expanded = false;
    let elapsed = 0;
    let timer = null;
    let hideTimeout = null;
    let logs = [];
    let unlisteners = [];
    let logContainer;

    // Min display time queue
    let detailQueue = [];
    let detailTimer = null;
    let lastDetailChange = 0;
    const MIN_DISPLAY_MS = 2000;

    function setBannerHeight(h) {
        document.documentElement.style.setProperty('--banner-h', h);
    }

    function updateBannerHeight() {
        if (!visible) {
            setBannerHeight('0px');
        } else if (expanded) {
            setBannerHeight('236px');
        } else {
            setBannerHeight('36px');
        }
    }

    onMount(async () => {
        setBannerHeight('0px');

        // Structured events (state + detail)
        unlisteners.push(await listen('i2p-status', (e) => {
            handleStatus('I2P', e.payload.state, e.payload.detail);
        }));
        unlisteners.push(await listen('tor-status', (e) => {
            handleStatus('Tor', e.payload.state, e.payload.detail);
        }));

        // Legacy simple string events (backwards compatible)
        unlisteners.push(await listen('tor-state', (e) => {
            const p = e.payload;
            if (typeof p === 'object' && p.state) {
                handleStatus('Tor', p.state, p.detail || '');
            } else {
                handleStatus('Tor', p, '');
            }
        }));
        unlisteners.push(await listen('i2p-state', (e) => {
            const p = e.payload;
            if (typeof p === 'object' && p.state) {
                handleStatus('I2P', p.state, p.detail || '');
            } else {
                handleStatus('I2P', p, '');
            }
        }));
    });

    onDestroy(() => {
        for (const u of unlisteners) u();
        if (timer) clearInterval(timer);
        if (hideTimeout) clearTimeout(hideTimeout);
        if (detailTimer) clearTimeout(detailTimer);
        setBannerHeight('0px');
    });

    function addLog(net, text, state) {
        if (!text) return;
        const time = new Date().toLocaleTimeString('de-DE', {
            hour: '2-digit', minute: '2-digit', second: '2-digit'
        });
        logs = [...logs.slice(-49), { time, net, text, state }];
        // Auto-scroll
        setTimeout(() => {
            if (logContainer) logContainer.scrollTop = logContainer.scrollHeight;
        }, 10);
    }

    function setDetail(text) {
        if (!text) return;
        const now = Date.now();
        const elapsed = now - lastDetailChange;

        if (elapsed >= MIN_DISPLAY_MS || !currentDetail) {
            currentDetail = text;
            lastDetailChange = now;
            // Drain queue if pending
            if (detailQueue.length > 0 && !detailTimer) {
                detailTimer = setTimeout(drainQueue, MIN_DISPLAY_MS);
            }
        } else {
            // Queue it - only keep latest
            detailQueue = [text];
            if (!detailTimer) {
                detailTimer = setTimeout(drainQueue, MIN_DISPLAY_MS - elapsed);
            }
        }
    }

    function drainQueue() {
        detailTimer = null;
        if (detailQueue.length > 0) {
            currentDetail = detailQueue.pop();
            detailQueue = [];
            lastDetailChange = Date.now();
        }
    }

    function handleStatus(net, state, detail) {
        console.log('StatusBanner:', net, state, detail);
        if (hideTimeout) { clearTimeout(hideTimeout); hideTimeout = null; }

        addLog(net, detail, state);

        if (state === 'bootstrapping' || state === 'reconnecting') {
            network = net;
            status = state;
            setDetail(detail || (state === 'reconnecting' ? 'Reconnecting...' : 'Starting...'));
            visible = true;
            if (!timer) {
                elapsed = 0;
                timer = setInterval(() => { elapsed = elapsed + 1; }, 1000);
            }
            updateBannerHeight();
        } else if (state === 'connected') {
            network = net;
            status = 'connected';
            currentDetail = detail || 'Connected';
            if (timer) { clearInterval(timer); timer = null; }
            updateBannerHeight();
            hideTimeout = setTimeout(() => {
                visible = false;
                expanded = false;
                updateBannerHeight();
            }, 3000);
        } else if (state === 'error') {
            network = net;
            status = 'error';
            currentDetail = detail || 'Connection failed';
            visible = true;
            if (timer) { clearInterval(timer); timer = null; }
            updateBannerHeight();
        } else if (state === 'disconnected' || state === 'direct') {
            visible = false;
            expanded = false;
            if (timer) { clearInterval(timer); timer = null; }
            updateBannerHeight();
        }
    }

    function toggleExpanded() {
        expanded = !expanded;
        updateBannerHeight();
    }

    function formatTime(secs) {
        const m = Math.floor(secs / 60);
        const s = secs % 60;
        return m > 0 ? `${m}m ${s}s` : `${s}s`;
    }
</script>

<div class="banner-wrapper" class:visible class:expanded>
    <div class="status-banner"
         class:bootstrapping={status === 'bootstrapping'}
         class:reconnecting={status === 'reconnecting'}
         class:connected={status === 'connected'}
         class:error={status === 'error'}>
        <div class="banner-main">
            {#if status === 'bootstrapping' || status === 'reconnecting'}
                <div class="spinner"></div>
            {:else if status === 'connected'}
                <svg class="icon" viewBox="0 0 16 16" fill="none">
                    <path d="M3 8.5L6.5 12L13 4" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
            {:else if status === 'error'}
                <svg class="icon" viewBox="0 0 16 16" fill="none">
                    <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="1.5"/>
                    <path d="M8 5v4M8 11v0.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                </svg>
            {/if}

            <span class="msg">{currentDetail}</span>

            {#if status === 'bootstrapping' || status === 'reconnecting'}
                <span class="timer">({formatTime(elapsed)})</span>
                <span class="hint">First connect takes 2-5 min</span>
            {/if}

            {#if logs.length > 0 && status !== 'connected'}
                <button class="chevron-btn" on:click|stopPropagation={toggleExpanded}
                        title={expanded ? 'Hide log' : 'Show log'}>
                    <svg viewBox="0 0 12 12" fill="none">
                        <path d={expanded ? 'M2 8L6 4L10 8' : 'M2 4L6 8L10 4'}
                              stroke="currentColor" stroke-width="1.5"
                              stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                </button>
            {/if}
        </div>
    </div>

    {#if expanded && logs.length > 0}
        <div class="log-panel" bind:this={logContainer}>
            {#each logs as entry}
                <div class="log-line"
                     class:log-err={entry.state === 'error'}
                     class:log-ok={entry.state === 'connected'}>
                    <span class="log-time">{entry.time}</span>
                    <span class="log-net">[{entry.net}]</span>
                    <span class="log-text">{entry.text}</span>
                </div>
            {/each}
        </div>
    {/if}
</div>

<style>
    .banner-wrapper {
        position: fixed;
        top: 0;
        left: 0;
        right: 0;
        z-index: 500;
        overflow: hidden;
        max-height: 0;
        opacity: 0;
        transition: max-height 0.3s ease, opacity 0.3s ease;
    }
    .banner-wrapper.visible {
        max-height: 40px;
        opacity: 1;
    }
    .banner-wrapper.visible.expanded {
        max-height: 240px;
    }

    .status-banner {
        font-size: 13px;
        font-weight: 500;
        font-family: 'Inter', -apple-system, sans-serif;
    }
    .banner-main {
        height: 36px;
        padding: 0 16px;
        display: flex;
        align-items: center;
        gap: 10px;
    }

    .bootstrapping, .reconnecting {
        background: color-mix(in srgb, var(--ac, #58a6ff) 12%, #1a1b26);
        color: var(--ac, #58a6ff);
        border-bottom: 1px solid color-mix(in srgb, var(--ac, #58a6ff) 30%, transparent);
    }
    .connected {
        background: rgba(152, 195, 121, 0.15);
        color: #98c379;
        border-bottom: 1px solid rgba(152, 195, 121, 0.3);
    }
    .error {
        background: rgba(248, 81, 73, 0.15);
        color: #f85149;
        border-bottom: 1px solid rgba(248, 81, 73, 0.3);
    }

    .spinner {
        width: 14px; height: 14px;
        border: 2px solid transparent;
        border-top-color: currentColor;
        border-radius: 50%;
        animation: spin 1s linear infinite;
        flex-shrink: 0;
    }
    @keyframes spin { to { transform: rotate(360deg); } }

    .icon { width: 16px; height: 16px; flex-shrink: 0; }
    .msg {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }
    .timer { flex-shrink: 0; opacity: 0.7; }
    .hint {
        margin-left: auto;
        font-size: 11px;
        opacity: 0.5;
        white-space: nowrap;
    }

    .chevron-btn {
        flex-shrink: 0;
        width: 24px; height: 24px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: none;
        border: 1px solid rgba(255,255,255,0.1);
        border-radius: 4px;
        cursor: pointer;
        color: inherit;
        opacity: 0.6;
        padding: 0;
        margin-left: 4px;
    }
    .chevron-btn:hover { opacity: 1; background: rgba(255,255,255,0.05); }
    .chevron-btn svg { width: 12px; height: 12px; }

    .log-panel {
        max-height: 200px;
        overflow-y: auto;
        background: rgba(0,0,0,0.3);
        border-top: 1px solid rgba(255,255,255,0.06);
        padding: 6px 0;
        font-family: 'JetBrains Mono', 'Cascadia Code', 'Consolas', monospace;
        font-size: 11px;
    }
    .log-line {
        padding: 2px 16px;
        display: flex;
        gap: 8px;
        color: rgba(255,255,255,0.5);
    }
    .log-line:hover { background: rgba(255,255,255,0.03); }
    .log-time { color: rgba(255,255,255,0.3); flex-shrink: 0; }
    .log-net { color: rgba(255,255,255,0.4); flex-shrink: 0; min-width: 30px; }
    .log-text { color: rgba(255,255,255,0.6); }
    .log-err .log-text { color: #f85149; }
    .log-ok .log-text { color: #98c379; }
</style>
