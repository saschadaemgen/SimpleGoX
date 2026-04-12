<script>
    import { listen } from '@tauri-apps/api/event';
    import { invoke } from '@tauri-apps/api/core';
    import { onMount, onDestroy } from 'svelte';

    let state = 'disconnected';
    let uptime = 0;
    let unlisteners = [];

    onMount(async () => {
        unlisteners.push(await listen('i2p-state', (ev) => { state = ev.payload; }));
    });

    onDestroy(() => { for (const u of unlisteners) u(); });

    function fmtUptime(s) {
        const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
        if (h > 0) return `${h}h ${m}m`;
        if (m > 0) return `${m}m ${s % 60}s`;
        return `${s}s`;
    }
</script>

<h3 class="tab-title">I2P Network</h3>

<div class="status-panel">
    <div class="banner" class:connected={state === 'connected'} class:bootstrapping={state === 'bootstrapping'}>
        <span class="bdot" class:pulse={state === 'bootstrapping'}></span>
        {#if state === 'connected'}Connected to I2P
        {:else if state === 'bootstrapping'}Bootstrapping I2P (10-15 min first run)...
        {:else}Disconnected{/if}
    </div>

    <div class="info-section">
        <h4 class="sec-title">About I2P</h4>
        <p class="desc">
            I2P (Invisible Internet Project) is a fully encrypted overlay network.
            Unlike Tor, I2P has no exit nodes - traffic never leaves the network.
            All communication stays inside I2P using garlic routing with 12 hops.
        </p>

        <div class="props">
            <div class="prop"><span class="pl">Latency</span><span class="pv">1-3 seconds</span></div>
            <div class="prop"><span class="pl">Hops</span><span class="pv">12 (6 per direction)</span></div>
            <div class="prop"><span class="pl">Routing</span><span class="pv">Garlic (bundled messages)</span></div>
            <div class="prop"><span class="pl">Exit nodes</span><span class="pv">None (closed network)</span></div>
            <div class="prop"><span class="pl">First boot</span><span class="pv">10-15 minutes</span></div>
            <div class="prop"><span class="pl">Subsequent</span><span class="pv">3-5 minutes</span></div>
            <div class="prop"><span class="pl">Protocols</span><span class="pv">Matrix, SimpleX only</span></div>
            <div class="prop"><span class="pl">Engine</span><span class="pv">emissary-core (Rust)</span></div>
        </div>
    </div>

    <div class="info-section">
        <h4 class="sec-title">SimpleGoX Homeserver on I2P</h4>
        <div class="addr-box">
            <code>aho2me4wz2wbayiviw5tax77iftuh4xy54qckzfm6s3oxcngpulq.b32.i2p</code>
        </div>
        <p class="desc">Port 8448. Enable I2P routing for Matrix to connect via this address.</p>
    </div>
</div>

<style>
    .tab-title { font-size: 1.1em; font-weight: 600; margin: 0 0 16px; }
    .status-panel { display: flex; flex-direction: column; gap: 14px; }

    .banner {
        display: flex; align-items: center; gap: 10px; padding: 12px 16px;
        border-radius: 8px; border: 1px solid rgba(255,255,255,0.06);
        font-weight: 600; font-size: 0.92em; color: #484f58;
    }
    .banner.connected { border-color: rgba(63,185,80,0.3); color: #3fb950; }
    .banner.bootstrapping { border-color: rgba(210,153,34,0.3); color: #d29922; }
    .bdot { width: 10px; height: 10px; border-radius: 50%; background: #484f58; flex-shrink: 0; }
    .banner.connected .bdot { background: #3fb950; }
    .banner.bootstrapping .bdot { background: #d29922; }
    .bdot.pulse { animation: pulse 1.5s ease infinite; }
    @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.3; } }

    .info-section {
        padding: 14px; border-radius: 8px;
        background: rgba(255,255,255,0.02); border: 1px solid rgba(255,255,255,0.04);
    }
    .sec-title { font-size: 0.72em; font-weight: 600; color: #8b949e; text-transform: uppercase; letter-spacing: 1px; margin-bottom: 8px; }
    .desc { font-size: 0.82em; color: #8b949e; line-height: 1.5; margin: 0 0 10px; }

    .props { display: grid; grid-template-columns: 1fr 1fr; gap: 4px; }
    .prop { display: flex; justify-content: space-between; padding: 4px 0; font-size: 0.78em; }
    .pl { color: #8b949e; }
    .pv { color: #c9d1d9; font-family: 'JetBrains Mono', monospace; }

    .addr-box {
        padding: 10px; border-radius: 6px; background: #0e1117;
        border: 1px solid rgba(255,255,255,0.06); margin-bottom: 8px;
    }
    .addr-box code { font-size: 0.7em; color: #98c379; word-break: break-all; }
</style>
