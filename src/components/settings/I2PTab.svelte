<script>
    import { invoke } from '@tauri-apps/api/core';
    import { onMount, onDestroy } from 'svelte';

    let stats = null;
    let error = '';
    let interval = null;

    onMount(() => {
        fetchStats();
        interval = setInterval(fetchStats, 5000);
    });

    onDestroy(() => {
        if (interval) clearInterval(interval);
    });

    async function fetchStats() {
        try {
            stats = await invoke('get_i2p_stats');
            error = '';
        } catch (e) {
            stats = null;
            error = String(e);
        }
    }
</script>

<h3 class="tab-title">I2P Network</h3>

{#if stats && stats.connected}
<div class="dashboard">
    <!-- Status -->
    <div class="banner on">
        <span class="dot green"></span>
        <span>{stats.network_status || 'Connected'}</span>
        <span class="up">{stats.uptime}</span>
    </div>

    <div class="engine">Engine: i2pd {stats.version} (C++)</div>

    <!-- Bandwidth -->
    <div class="section">
        <h4 class="sec-title">Bandwidth</h4>
        <div class="grid">
            <div class="cell"><span class="lbl">Inbound</span><span class="val">{stats.bw_inbound || '-'}</span></div>
            <div class="cell"><span class="lbl">Outbound</span><span class="val">{stats.bw_outbound || '-'}</span></div>
            <div class="cell"><span class="lbl">Received</span><span class="val">{stats.received || '-'}</span></div>
            <div class="cell"><span class="lbl">Sent</span><span class="val">{stats.sent || '-'}</span></div>
            <div class="cell"><span class="lbl">Transit</span><span class="val">{stats.transit_bw || '-'}</span></div>
            <div class="cell"><span class="lbl">Tunnel Success</span><span class="val">{stats.success_rate || '-'}</span></div>
        </div>
    </div>

    <!-- Network -->
    <div class="section">
        <h4 class="sec-title">Network</h4>
        <div class="grid">
            <div class="cell"><span class="lbl">Routers</span><span class="val">{stats.routers || '-'}</span></div>
            <div class="cell"><span class="lbl">Floodfills</span><span class="val">{stats.floodfills || '-'}</span></div>
            <div class="cell"><span class="lbl">LeaseSets</span><span class="val">{stats.lease_sets || '-'}</span></div>
        </div>
    </div>

    <!-- Tunnels -->
    <div class="section">
        <h4 class="sec-title">Tunnels</h4>
        <div class="grid">
            <div class="cell"><span class="lbl">Client</span><span class="val">{stats.client_tunnels || '-'}</span></div>
            <div class="cell"><span class="lbl">Transit</span><span class="val">{stats.transit || '-'}</span></div>
        </div>
    </div>

    <!-- Services -->
    <div class="section">
        <h4 class="sec-title">Services</h4>
        <div class="grid">
            <div class="cell"><span class="lbl">SOCKS Proxy</span><span class="val svc-on">Port 4447</span></div>
            <div class="cell"><span class="lbl">Webconsole</span><span class="val mono">127.0.0.1:7070</span></div>
        </div>
    </div>

    <!-- Homeserver -->
    <div class="section">
        <h4 class="sec-title">Homeserver on I2P</h4>
        <div class="addr-box">
            <code>aho2me4wz2wbayiviw5tax77iftuh4xy54qckzfm6s3oxcngpulq.b32.i2p</code>
        </div>
        <span class="desc">Port 8448</span>
    </div>
</div>

{:else}
<div class="dashboard">
    <div class="banner off">
        <span class="dot grey"></span>
        <span>Disconnected</span>
    </div>
    <p class="hint">Start I2P via Settings > Routing > Matrix > I2P</p>
    {#if error}
        <p class="err">{error}</p>
    {/if}
</div>
{/if}

<style>
    .tab-title { font-size: 1.1em; font-weight: 600; margin: 0 0 16px; }
    .dashboard { display: flex; flex-direction: column; gap: 10px; }

    .banner {
        display: flex; align-items: center; gap: 10px; padding: 10px 14px;
        border-radius: 8px; font-weight: 600; font-size: 0.88em;
        border: 1px solid rgba(255,255,255,0.06);
    }
    .banner.on { border-color: rgba(152,195,121,0.3); color: #98c379; }
    .banner.off { color: #484f58; }
    .up { margin-left: auto; font-size: 0.8em; opacity: 0.7; font-weight: 400; }
    .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
    .dot.green { background: #98c379; }
    .dot.grey { background: #484f58; }

    .engine { font-size: 0.72em; color: #8b949e; padding: 0 2px; }

    .section {
        padding: 10px 14px; border-radius: 8px;
        background: rgba(255,255,255,0.02); border: 1px solid rgba(255,255,255,0.04);
    }
    .sec-title {
        font-size: 0.65em; font-weight: 600; color: #8b949e;
        text-transform: uppercase; letter-spacing: 1px; margin: 0 0 8px;
    }
    .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
    .cell { display: flex; justify-content: space-between; font-size: 0.78em; }
    .lbl { color: #8b949e; }
    .val { color: #c9d1d9; font-family: 'JetBrains Mono', monospace; }
    .val.svc-on { color: #98c379; }
    .val.mono { font-family: 'JetBrains Mono', monospace; font-size: 0.9em; }

    .addr-box {
        padding: 8px 10px; border-radius: 6px; background: #0e1117;
        border: 1px solid rgba(255,255,255,0.06); margin-bottom: 4px;
    }
    .addr-box code { font-size: 0.68em; color: #98c379; word-break: break-all; }
    .desc { font-size: 0.72em; color: #8b949e; }
    .hint { font-size: 0.82em; color: #8b949e; font-style: italic; }
    .err { font-size: 0.72em; color: #f85149; }
</style>
