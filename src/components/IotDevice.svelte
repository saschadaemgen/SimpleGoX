<script>
    import { currentRoomId } from '../lib/stores.js';
    import { sendIotCommand } from '../lib/tauri.js';
    import { get } from 'svelte/store';

    export let device;

    $: on = device.state === true;
    $: dead = !device.online;

    function toggle() {
        const rid = get(currentRoomId);
        if (rid) sendIotCommand(rid, device.device_id, 'set', !on);
    }

    function press() {
        const rid = get(currentRoomId);
        if (rid) sendIotCommand(rid, device.device_id, 'set', true);
    }

    function slide(e) {
        const rid = get(currentRoomId);
        if (rid) sendIotCommand(rid, device.device_id, 'set', parseInt(e.target.value, 10));
    }
</script>

<div class="dv" class:on class:dead>
    <div class="left">
        <div class="ic">
            {#if device.device_type === 'switch'}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 18h6M10 22h4M12 2a7 7 0 0 0-4 12.7V17h8v-2.3A7 7 0 0 0 12 2z"/></svg>
            {:else if device.device_type === 'sensor'}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 14.76V3.5a2.5 2.5 0 0 0-5 0v11.26a4.5 4.5 0 1 0 5 0z"/></svg>
            {:else if device.device_type === 'button'}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.73 21a2 2 0 0 1-3.46 0"/></svg>
            {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/></svg>
            {/if}
        </div>
        <div class="inf">
            <span class="nm">{device.label}</span>
            <span class="sub">{device.device_id}</span>
        </div>
    </div>
    <div class="right">
        {#if device.device_type === 'switch'}
            <button class="sw" class:on on:click={toggle} title="Toggle"><div class="sw-k"></div></button>
        {:else if device.device_type === 'sensor'}
            <span class="val">{device.value ?? '--'}<span class="unit">{device.unit ? ' ' + device.unit : ''}</span></span>
        {:else if device.device_type === 'button'}
            <button class="btn" on:click={press}>Press</button>
        {:else if device.device_type === 'dimmer'}
            <div class="sl-wrap">
                <input type="range" class="sl" min="0" max="100" value={device.value ?? 50} on:change={slide} />
                <span class="sl-v">{device.value ?? 50}%</span>
            </div>
        {/if}
    </div>
</div>

<style>
    .dv {
        padding: 10px; display: flex; justify-content: space-between; align-items: center;
        border-radius: 9px; transition: all 150ms var(--ease);
        animation: dvIn 250ms var(--ease) both;
    }
    @keyframes dvIn { from { opacity: 0; transform: translateX(8px); } to { opacity: 1; transform: translateX(0); } }
    .dv:hover { background: var(--bg-hover); }
    .dv.dead { opacity: 0.18; pointer-events: none; }

    .left { display: flex; align-items: center; gap: 9px; overflow: hidden; }

    .ic {
        width: 34px; height: 34px; border-radius: 9px; background: var(--bg-raised);
        border: 1px solid var(--border-2); display: flex; align-items: center;
        justify-content: center; transition: all 250ms var(--ease); flex-shrink: 0;
    }
    .ic :global(svg) { color: var(--text-3); width: 16px; height: 16px; transition: color 250ms; }
    .dv.on .ic { border-color: var(--ac-border); box-shadow: 0 0 8px var(--ac-glow); }
    .dv.on .ic :global(svg) { color: var(--ac); }

    .inf { display: flex; flex-direction: column; overflow: hidden; }
    .nm { font-size: 0.82em; font-weight: 500; white-space: nowrap; }
    .sub { font-size: 0.66em; color: var(--text-3); font-family: 'JetBrains Mono', monospace; }

    .sw {
        width: 40px; height: 22px; border-radius: 11px; background: var(--bg);
        border: 1px solid var(--border-2); position: relative; cursor: pointer;
        transition: all 280ms var(--ease);
    }
    .sw.on { border-color: var(--ac-border); box-shadow: 0 0 8px rgba(63, 185, 168, 0.05); }
    .sw-k {
        position: absolute; width: 16px; height: 16px; border-radius: 50%;
        top: 2px; left: 2px; background: var(--text-3);
        transition: all 280ms var(--ease-b);
    }
    .sw.on .sw-k { transform: translateX(18px); background: var(--ac); box-shadow: 0 0 5px rgba(63, 185, 168, 0.2); }

    .val { font-family: 'JetBrains Mono', monospace; font-size: 0.9em; font-weight: 500; color: var(--text-2); }
    .dv.on .val { color: var(--text); }
    .unit { font-size: 0.7em; color: var(--text-3); }

    .btn {
        padding: 4px 12px; border-radius: 7px; border: 1px solid var(--border-2);
        background: transparent; color: var(--text-2); font-family: 'DM Sans', sans-serif;
        font-size: 0.78em; font-weight: 500; cursor: pointer; transition: all 180ms var(--ease);
    }
    .btn:hover { border-color: var(--ac-border); color: var(--ac); }
    .btn:active { transform: scale(0.93); }

    .sl-wrap { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; }
    .sl { width: 76px; height: 3px; -webkit-appearance: none; appearance: none; border-radius: 2px; outline: none; background: var(--bg); }
    .sl::-webkit-slider-thumb { -webkit-appearance: none; width: 13px; height: 13px; border-radius: 50%; background: var(--text-2); cursor: pointer; border: 2px solid var(--bg-card); }
    .sl-v { font-family: 'JetBrains Mono', monospace; font-size: 0.66em; color: var(--text-3); }
</style>
