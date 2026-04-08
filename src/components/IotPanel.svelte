<script>
    import { currentIotDevices, iotPanelCollapsed, iotPanelOpen } from '../lib/stores.js';
    import IotDevice from './IotDevice.svelte';

    function close() { iotPanelOpen.set(false); }
    function toggleCollapse() { iotPanelCollapsed.update(v => !v); }
</script>

<aside class="iot" class:collapsed={$iotPanelCollapsed}>
    <div class="top">
        <div class="t"><div class="dot"></div><span>Devices</span></div>
        <button class="ic" on:click={close} title="Close">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
    </div>
    <div class="body">
        {#if $currentIotDevices.length > 0}
            {#each $currentIotDevices as device (device.device_id)}
                <IotDevice {device} />
            {/each}
        {:else}
            <p class="empty">No IoT devices</p>
        {/if}
    </div>
    <div class="col-row">
        <button class="ic" on:click={toggleCollapse} title="Collapse">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="chev"><polyline points="9 18 15 12 9 6"/></svg>
        </button>
    </div>
</aside>

<style>
    .iot {
        width: var(--iot-w); min-width: var(--iot-w); background: var(--bg-card);
        border-left: 1px solid var(--ac-border); display: flex; flex-direction: column;
        transition: width 280ms var(--ease), min-width 280ms var(--ease);
        overflow: hidden;
    }
    .iot.collapsed { width: var(--sidebar-collapsed); min-width: var(--sidebar-collapsed); }

    .top {
        padding: 14px; display: flex; justify-content: space-between; align-items: center;
        border-bottom: 1px solid var(--border); min-height: 50px;
    }
    .t {
        font-size: 0.76em; font-weight: 600; color: var(--text-2);
        display: flex; align-items: center; gap: 7px; letter-spacing: 0.3px;
        text-transform: uppercase; white-space: nowrap; overflow: hidden;
    }
    .iot.collapsed .t span { opacity: 0; }
    .dot {
        width: 5px; height: 5px; border-radius: 50%; background: var(--ac);
        box-shadow: 0 0 5px var(--ac-glow); animation: ip 2s infinite; flex-shrink: 0;
    }
    @keyframes ip { 0%,100% { opacity: 0.5; } 50% { opacity: 1; } }

    .ic {
        width: 30px; height: 30px; border-radius: 8px; border: none;
        background: transparent; color: var(--text-3); cursor: pointer;
        display: flex; align-items: center; justify-content: center;
        transition: all 120ms ease; flex-shrink: 0;
    }
    .ic:hover { background: var(--bg-hover); color: var(--text-2); }

    .body { flex: 1; overflow-y: auto; padding: 5px 6px; }
    .empty { color: var(--text-3); font-size: 0.82em; text-align: center; padding: 20px; }

    .col-row { padding: 10px; border-top: 1px solid var(--border); display: flex; justify-content: flex-start; }
    .iot.collapsed .col-row { justify-content: center; }
    .chev { transition: transform 250ms var(--ease); }
    .iot.collapsed .chev { transform: rotate(180deg); }
</style>
