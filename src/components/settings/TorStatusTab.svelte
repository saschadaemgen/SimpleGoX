<script>
    import { invoke } from '@tauri-apps/api/core';
    import { listen } from '@tauri-apps/api/event';
    import { onMount, onDestroy } from 'svelte';
    import Tooltip from '../ui/Tooltip.svelte';

    let torState = 'disconnected'; // disconnected, bootstrapping, connected
    let exitIp = '---';
    let checkingIp = false;
    let ipError = '';
    let uptime = 0;
    let activeConns = 0;
    let totalConns = 0;
    let bytesIn = 0;
    let bytesOut = 0;

    let throughputHistory = [];
    let latencyHistory = [];
    const MAX_POINTS = 60;

    let canvasTP;
    let canvasLat;
    let animId;
    let unlisteners = [];

    onMount(async () => {
        unlisteners.push(await listen('tor-state', (ev) => {
            torState = ev.payload;
        }));

        unlisteners.push(await listen('tor-exit-ip', (ev) => {
            exitIp = ev.payload;
        }));

        unlisteners.push(await listen('tor-stats', (ev) => {
            const d = ev.payload;
            uptime = d.uptime_secs || 0;
            activeConns = d.active_connections || 0;
            totalConns = d.total_connections || 0;
            bytesIn = d.bytes_in_total || 0;
            bytesOut = d.bytes_out_total || 0;

            throughputHistory = [...throughputHistory.slice(-(MAX_POINTS - 1)), { in: d.throughput_in || 0, out: d.throughput_out || 0 }];

            if (d.latency_ms !== null && d.latency_ms !== undefined) {
                latencyHistory = [...latencyHistory.slice(-(MAX_POINTS - 1)), d.latency_ms];
            }
        }));

        try {
            const status = await invoke('tor_get_status');
            if (status) torState = 'connected';
        } catch (_) {}

        try { await invoke('tor_start_stats'); } catch (_) {}

        drawLoop();
    });

    onDestroy(() => {
        for (const u of unlisteners) u();
        if (animId) cancelAnimationFrame(animId);
    });

    function drawLoop() {
        drawGraph(canvasTP, throughputHistory.map(d => d.in), throughputHistory.map(d => d.out), 'rgba(88,166,255,', 'rgba(123,104,238,');
        drawGraph(canvasLat, latencyHistory, null, 'rgba(123,104,238,', null);
        animId = requestAnimationFrame(drawLoop);
    }

    function drawGraph(canvas, data1, data2, c1, c2) {
        if (!canvas) return;
        const ctx = canvas.getContext('2d');
        const w = canvas.width = canvas.offsetWidth * 2;
        const h = canvas.height = canvas.offsetHeight * 2;
        ctx.clearRect(0, 0, w, h);

        // Grid
        ctx.strokeStyle = 'rgba(255,255,255,0.04)';
        ctx.lineWidth = 1;
        for (let i = 1; i < 4; i++) { ctx.beginPath(); ctx.moveTo(0, h*i/4); ctx.lineTo(w, h*i/4); ctx.stroke(); }

        const drawLine = (data, color) => {
            if (!data || data.length < 2) return;
            const max = Math.max(1, ...data);
            const step = w / (MAX_POINTS - 1);
            const off = MAX_POINTS - data.length;

            ctx.beginPath();
            ctx.moveTo(off * step, h);
            for (let i = 0; i < data.length; i++) {
                ctx.lineTo((off + i) * step, h - (data[i] / max) * h * 0.85);
            }
            ctx.lineTo((off + data.length - 1) * step, h);
            ctx.closePath();
            ctx.fillStyle = color + '0.1)';
            ctx.fill();

            ctx.beginPath();
            for (let i = 0; i < data.length; i++) {
                const x = (off + i) * step;
                const y = h - (data[i] / max) * h * 0.85;
                i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
            }
            ctx.strokeStyle = color + '0.7)';
            ctx.lineWidth = 2;
            ctx.stroke();
        };

        drawLine(data1, c1);
        if (data2) drawLine(data2, c2);
    }

    function fmtBytes(b) {
        if (b < 1024) return b + ' B';
        if (b < 1048576) return (b / 1024).toFixed(1) + ' KB';
        return (b / 1048576).toFixed(1) + ' MB';
    }

    function fmtUptime(s) {
        const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = s % 60;
        if (h > 0) return `${h}h ${m}m`;
        if (m > 0) return `${m}m ${sec}s`;
        return `${sec}s`;
    }

    async function checkIp() {
        checkingIp = true; ipError = '';
        try {
            exitIp = await invoke('tor_check_ip');
        } catch (e) {
            ipError = String(e);
            exitIp = 'Error';
            console.error('IP check failed:', e);
        }
        checkingIp = false;
    }
</script>

<div class="status-panel">
    <div class="banner" class:connected={torState === 'connected'} class:bootstrapping={torState === 'bootstrapping'}>
        <span class="bdot" class:pulse={torState === 'bootstrapping'}></span>
        {#if torState === 'connected'}Connected to Tor
        {:else if torState === 'bootstrapping'}Bootstrapping...
        {:else}Disconnected{/if}
    </div>

    <div class="stats">
        <div class="stat">
            <div class="sl">EXIT IP<Tooltip text="Your apparent IP as seen by destination servers through Tor." /></div>
            <div class="sv mono">{exitIp}</div>
            <button class="sbtn" on:click={checkIp} disabled={checkingIp || torState !== 'connected'}>
                {checkingIp ? 'Checking...' : 'Check IP'}
            </button>
            {#if ipError}<div class="serr">{ipError}</div>{/if}
        </div>
        <div class="stat">
            <div class="sl">UPTIME</div>
            <div class="sv">{torState === 'connected' ? fmtUptime(uptime) : '---'}</div>
        </div>
        <div class="stat">
            <div class="sl">CONNECTIONS</div>
            <div class="sv">{torState === 'connected' ? activeConns : '---'}</div>
            {#if torState === 'connected'}<div class="ssub">total: {totalConns}</div>{/if}
        </div>
    </div>

    <div class="graph-box">
        <div class="gh"><span class="gt">Throughput</span><div class="gl"><span class="gd" style="background:rgba(88,166,255,0.7)"></span>In <span class="gd" style="background:rgba(123,104,238,0.7)"></span>Out</div></div>
        <canvas class="gc" bind:this={canvasTP}></canvas>
        <div class="gf">Total: {fmtBytes(bytesIn)} in / {fmtBytes(bytesOut)} out</div>
    </div>

    <div class="graph-box">
        <div class="gh"><span class="gt">Latency</span><div class="gl"><span class="gd" style="background:rgba(123,104,238,0.7)"></span>ms</div></div>
        <canvas class="gc lat" bind:this={canvasLat}></canvas>
    </div>
</div>

<style>
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

    .stats { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 10px; }
    .stat {
        display: flex; flex-direction: column; align-items: center; gap: 4px;
        padding: 14px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.04);
        background: rgba(255,255,255,0.02);
    }
    .sl { font-size: 0.65em; font-weight: 600; color: #8b949e; letter-spacing: 0.5px; }
    .sv { font-size: 1.2em; font-weight: 700; font-family: inherit; }
    .sv.mono { font-family: 'JetBrains Mono', monospace; font-size: 0.85em; word-break: break-all; }
    .ssub { font-size: 0.68em; color: #484f58; }
    .sbtn {
        padding: 3px 10px; border-radius: 5px; border: 1px solid rgba(255,255,255,0.08);
        background: transparent; color: #8b949e; font-size: 0.68em; font-family: inherit;
        cursor: pointer; transition: all 0.15s;
    }
    .sbtn:hover:not(:disabled) { border-color: var(--ac); color: var(--ac); }
    .sbtn:disabled { opacity: 0.3; cursor: default; }
    .serr { font-size: 0.65em; color: #f85149; text-align: center; }

    .graph-box {
        padding: 12px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.04);
        background: rgba(255,255,255,0.02);
    }
    .gh { display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px; }
    .gt { font-size: 0.78em; font-weight: 600; color: #8b949e; }
    .gl { display: flex; align-items: center; gap: 8px; font-size: 0.65em; color: #484f58; }
    .gd { width: 8px; height: 8px; border-radius: 50%; display: inline-block; }
    .gc { width: 100%; height: 80px; }
    .gc.lat { height: 60px; }
    .gf { font-size: 0.65em; color: #484f58; margin-top: 4px; font-family: 'JetBrains Mono', monospace; }
</style>
