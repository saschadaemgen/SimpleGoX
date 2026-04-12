<script>
    import { invoke } from '@tauri-apps/api/core';
    import { onMount, onDestroy } from 'svelte';

    let connections = [];
    let pollInterval;

    onMount(() => {
        pollInterval = setInterval(async () => {
            try {
                connections = await invoke('tor_get_connections');
            } catch (_) {
                connections = [];
            }
        }, 3000);
    });

    onDestroy(() => { if (pollInterval) clearInterval(pollInterval); });

    function fmtBytes(b) {
        if (b < 1024) return b + ' B';
        if (b < 1048576) return (b / 1024).toFixed(1) + ' KB';
        return (b / 1048576).toFixed(1) + ' MB';
    }
</script>

<div class="network-panel">
    <h4 class="sec-title">Active Connections</h4>

    {#if connections.length === 0}
        <div class="empty">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="10"/><path d="M12 8v4l3 3"/></svg>
            <p>No active Tor connections. Enable Tor routing for a protocol to see traffic.</p>
        </div>
    {:else}
        {#each connections as conn (conn.id)}
            <div class="conn">
                <div class="conn-head">
                    <span class="conn-proto" class:mx={conn.protocol === 'matrix'} class:tg={conn.protocol === 'telegram'}>{conn.protocol.toUpperCase()}</span>
                    <span class="conn-dest">{conn.destination}</span>
                </div>
                <div class="conn-stats">{fmtBytes(conn.bytes_in)} in / {fmtBytes(conn.bytes_out)} out</div>
            </div>
        {/each}
    {/if}

    <h4 class="sec-title" style="margin-top:20px">Consensus</h4>
    <div class="info-row">
        <span class="info-l">Status</span>
        <span class="info-v">Managed by Arti (auto-refresh)</span>
    </div>
</div>

<style>
    .network-panel { display: flex; flex-direction: column; gap: 8px; }
    .sec-title { font-size: 0.72em; font-weight: 600; color: #8b949e; text-transform: uppercase; letter-spacing: 1px; margin-bottom: 6px; }

    .empty {
        display: flex; flex-direction: column; align-items: center; gap: 8px;
        padding: 28px; color: #484f58; text-align: center;
        border-radius: 8px; border: 1px dashed rgba(255,255,255,0.06);
    }
    .empty p { font-size: 0.82em; max-width: 280px; line-height: 1.5; margin: 0; }

    .conn {
        padding: 10px 12px; border-radius: 8px;
        background: rgba(255,255,255,0.02); border: 1px solid rgba(255,255,255,0.04);
    }
    .conn-head { display: flex; align-items: center; gap: 8px; margin-bottom: 4px; }
    .conn-proto {
        padding: 1px 6px; border-radius: 4px; font-size: 0.65em; font-weight: 700;
        background: rgba(255,255,255,0.06); color: #8b949e;
    }
    .conn-proto.mx { background: rgba(88,166,255,0.15); color: #58a6ff; }
    .conn-proto.tg { background: rgba(97,175,239,0.15); color: #61afef; }
    .conn-dest { font-size: 0.78em; font-family: 'JetBrains Mono', monospace; color: #8b949e; }
    .conn-stats { font-size: 0.72em; color: #484f58; font-family: 'JetBrains Mono', monospace; }

    .info-row { display: flex; justify-content: space-between; padding: 6px 0; border-bottom: 1px solid rgba(255,255,255,0.04); }
    .info-l { font-size: 0.82em; color: #8b949e; }
    .info-v { font-size: 0.82em; color: #c9d1d9; }
</style>
