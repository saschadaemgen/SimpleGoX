<script>
    import { torRouting } from '../../lib/stores.js';
    import { invoke } from '@tauri-apps/api/core';

    let bootstrapping = '';
    let showWarning = false;
    let warnProto = '';
    let warnMode = '';

    function request(protocol, mode) {
        if ((protocol === 'telegram' || protocol === 'whatsapp') && mode === 'i2p') return;
        if ((protocol === 'telegram' || protocol === 'whatsapp') && mode === 'tor') {
            warnProto = protocol; warnMode = mode; showWarning = true; return;
        }
        apply(protocol, mode);
    }

    async function apply(protocol, mode) {
        const old = $torRouting[protocol];
        $torRouting[protocol] = mode;
        $torRouting = $torRouting;
        bootstrapping = mode === 'tor' ? 'tor' : mode === 'i2p' ? 'i2p' : '';
        try {
            await invoke('tor_set_protocol', { protocol, mode, onionAddress: null });
            // Backend auto-saves to tor-routing.json (single source of truth)
        } catch (e) {
            console.error('Routing error:', e);
            // Don't rollback for I2P/Tor - keep UI showing the target mode
            // so user sees the error state and can retry
            if (mode === 'direct') {
                $torRouting[protocol] = old;
                $torRouting = $torRouting;
            }
        }
        bootstrapping = '';
    }
</script>

<div class="rt">
    <h3 class="title">Protocol Routing</h3>
    <p class="desc">Choose how each protocol connects. Direct is fastest. Tor hides your IP. I2P keeps traffic inside the invisible network.</p>

    {#if bootstrapping}
        <div class="boot" class:tor={bootstrapping === 'tor'} class:i2p={bootstrapping === 'i2p'}>
            <span class="spin"></span>
            {bootstrapping === 'tor' ? 'Connecting to Tor (10-30s)...' : 'Connecting to I2P (may take minutes on first run)...'}
        </div>
    {/if}

    <!-- Matrix: Direct / Tor / I2P -->
    <div class="card">
        <div class="hdr"><span class="b mx">MX</span><span class="nm">Matrix</span>
            <span class="st" class:tor={$torRouting.matrix === 'tor'} class:i2p={$torRouting.matrix === 'i2p'}>{$torRouting.matrix.toUpperCase()}</span>
        </div>
        <div class="btns">
            <button class="rb" class:on={$torRouting.matrix === 'direct'} on:click={() => request('matrix', 'direct')}>Direct</button>
            <button class="rb tor" class:on={$torRouting.matrix === 'tor'} on:click={() => request('matrix', 'tor')}>Tor</button>
            <button class="rb i2p" class:on={$torRouting.matrix === 'i2p'} on:click={() => request('matrix', 'i2p')}>I2P</button>
        </div>
        {#if $torRouting.matrix === 'tor'}<p class="info">Traffic routed through 3 Tor relays. Server sees exit node IP.</p>{/if}
        {#if $torRouting.matrix === 'i2p'}<p class="info g">Traffic stays inside I2P. No exit nodes. First connect takes 10-15 min. Connects to .b32.i2p hidden service.</p>{/if}
    </div>

    <!-- Telegram: Direct / Tor -->
    <div class="card">
        <div class="hdr"><span class="b tg">TG</span><span class="nm">Telegram</span>
            <span class="exp">EXPERIMENTAL</span>
            <span class="st" class:tor={$torRouting.telegram === 'tor'}>{$torRouting.telegram.toUpperCase()}</span>
        </div>
        <div class="btns">
            <button class="rb" class:on={$torRouting.telegram === 'direct'} on:click={() => request('telegram', 'direct')}>Direct</button>
            <button class="rb tor" class:on={$torRouting.telegram === 'tor'} on:click={() => request('telegram', 'tor')}>Tor</button>
        </div>
        {#if $torRouting.telegram === 'tor'}<p class="info w">Warning: Telegram may freeze accounts using Tor exit nodes.</p>{/if}
    </div>

    <!-- SimpleX: disabled -->
    <div class="card dis">
        <div class="hdr"><span class="b sx">SX</span><span class="nm">SimpleX</span><span class="soon">coming soon</span></div>
        <div class="btns">
            <button class="rb on" disabled>Direct</button>
            <button class="rb tor" disabled>Tor</button>
            <button class="rb i2p" disabled>I2P</button>
        </div>
    </div>

    <!-- WhatsApp: disabled -->
    <div class="card dis">
        <div class="hdr"><span class="b wa">WA</span><span class="nm">WhatsApp</span><span class="soon">coming soon</span></div>
        <div class="btns">
            <button class="rb on" disabled>Direct</button>
            <button class="rb tor" disabled>Tor</button>
        </div>
    </div>
</div>

<!-- Warning Dialog -->
{#if showWarning}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="wo" on:click={() => showWarning = false}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="wd" on:click|stopPropagation>
        <div class="wi">!</div>
        <h3>Experimental Feature</h3>
        <p>Routing {warnProto === 'telegram' ? 'Telegram' : 'WhatsApp'} through Tor is experimental:</p>
        <ul>
            <li>Account may be frozen or banned</li>
            <li>Messages may not deliver reliably</li>
            <li>{warnProto === 'telegram' ? 'Telegram blocks known Tor exit IPs' : 'Calls will not work (TCP only)'}</li>
        </ul>
        <p class="wr">Use at your own risk. Recommended only with Matrix and SimpleX.</p>
        <div class="wbtns">
            <button class="wb sec" on:click={() => showWarning = false}>Cancel</button>
            <button class="wb dan" on:click={() => { showWarning = false; apply(warnProto, warnMode); }}>I understand, enable</button>
        </div>
    </div>
</div>
{/if}

<style>
    .rt { display: flex; flex-direction: column; gap: 12px; }
    .title { font-size: 1.1em; font-weight: 600; margin: 0; }
    .desc { font-size: 0.82em; color: #8b949e; line-height: 1.5; margin: 0; }

    .boot { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-radius: 8px; font-size: 0.82em; font-weight: 500; }
    .boot.tor { border: 1px solid rgba(123,104,238,0.3); color: #7B68EE; }
    .boot.i2p { border: 1px solid rgba(152,195,121,0.3); color: #98c379; }
    .spin { width: 14px; height: 14px; border: 2px solid transparent; border-top-color: currentColor; border-radius: 50%; animation: sp 0.8s linear infinite; }
    @keyframes sp { to { transform: rotate(360deg); } }

    .card { border: 1px solid rgba(255,255,255,0.06); border-radius: 10px; padding: 14px; }
    .card.dis { opacity: 0.3; pointer-events: none; }
    .hdr { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
    .b { padding: 2px 6px; border-radius: 4px; font-size: 0.65em; font-weight: 700; letter-spacing: 0.5px; }
    .mx { background: rgba(88,166,255,0.15); color: #58a6ff; }
    .tg { background: rgba(97,175,239,0.15); color: #61afef; }
    .sx { background: rgba(198,120,221,0.15); color: #c678dd; }
    .wa { background: rgba(152,195,121,0.15); color: #98c379; }
    .nm { font-size: 0.92em; font-weight: 600; }
    .st { margin-left: auto; font-size: 0.65em; font-weight: 600; padding: 2px 7px; border-radius: 4px; color: #484f58; }
    .st.tor { color: #7B68EE; border: 1px solid rgba(123,104,238,0.3); }
    .st.i2p { color: #98c379; border: 1px solid rgba(152,195,121,0.3); }
    .exp { font-size: 0.55em; padding: 1px 5px; border-radius: 3px; background: rgba(248,81,73,0.15); color: #f85149; font-weight: 600; }
    .soon { font-size: 0.65em; color: #484f58; font-style: italic; }

    .btns { display: flex; gap: 6px; }
    .rb {
        flex: 1; padding: 7px; border-radius: 7px; border: 1px solid rgba(255,255,255,0.06);
        background: transparent; color: #8b949e; font-size: 0.82em; font-weight: 500;
        font-family: inherit; cursor: pointer; transition: all 0.15s;
    }
    .rb:hover:not(:disabled) { border-color: rgba(255,255,255,0.15); }
    .rb.on { background: var(--ac-bg); color: var(--ac, #58a6ff); border-color: var(--ac-border); }
    .rb.tor.on { background: rgba(123,104,238,0.1); color: #7B68EE; border-color: rgba(123,104,238,0.3); }
    .rb.i2p.on { background: rgba(152,195,121,0.1); color: #98c379; border-color: rgba(152,195,121,0.3); }
    .rb:disabled { opacity: 0.3; cursor: default; }

    .info { font-size: 0.75em; color: #8b949e; margin: 8px 0 0; line-height: 1.4; }
    .info.w { color: #d29922; }
    .info.g { color: rgba(152,195,121,0.8); }

    .wo { position: fixed; inset: 0; background: rgba(0,0,0,0.7); display: flex; align-items: center; justify-content: center; z-index: 9999; }
    .wd { background: #161b22; border: 1px solid rgba(255,255,255,0.1); border-radius: 14px; padding: 24px; max-width: 400px; text-align: center; }
    .wi { width: 44px; height: 44px; border-radius: 50%; background: rgba(248,81,73,0.15); color: #f85149; font-size: 22px; font-weight: 700; display: flex; align-items: center; justify-content: center; margin: 0 auto 14px; }
    .wd h3 { color: #f85149; margin-bottom: 10px; font-size: 1em; }
    .wd p { color: #8b949e; font-size: 0.82em; line-height: 1.5; margin-bottom: 6px; }
    .wd ul { text-align: left; color: #8b949e; font-size: 0.75em; line-height: 1.6; margin: 6px 0 10px 18px; }
    .wr { color: #d29922; font-weight: 600; font-size: 0.78em; }
    .wbtns { display: flex; gap: 8px; margin-top: 14px; justify-content: center; }
    .wb { padding: 7px 18px; border-radius: 8px; font-size: 0.82em; font-family: inherit; cursor: pointer; border: none; }
    .wb.sec { background: rgba(255,255,255,0.06); color: #8b949e; }
    .wb.dan { background: rgba(248,81,73,0.1); color: #f85149; border: 1px solid rgba(248,81,73,0.3); }
</style>
