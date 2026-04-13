<script>
    import { torRouting } from '../../lib/stores.js';
    import { invoke } from '@tauri-apps/api/core';
    import Tooltip from '../ui/Tooltip.svelte';

    let bootstrapping = false;
    let showWarning = false;
    let warningProtocol = '';
    let pendingMode = '';

    function requestProtocol(protocol, mode) {
        // Show warning for Telegram and WhatsApp
        if ((protocol === 'telegram' || protocol === 'whatsapp') && mode !== 'direct') {
            warningProtocol = protocol;
            pendingMode = mode;
            showWarning = true;
            return;
        }
        setProtocol(protocol, mode);
    }

    function confirmWarning() {
        setProtocol(warningProtocol, pendingMode);
        showWarning = false;
    }

    async function setProtocol(protocol, mode) {
        $torRouting[protocol] = mode;
        $torRouting = $torRouting;
        bootstrapping = true;
        try {
            await invoke('tor_set_protocol', { protocol, mode, onionAddress: null });
        } catch (e) {
            console.error('Tor routing error:', e);
        }
        bootstrapping = false;
    }
</script>

<div class="routing-panel">
    <p class="hint">Route each protocol through Tor independently. First activation takes 10-30 seconds.<Tooltip text="Each protocol gets its own Tor circuit for isolation." /></p>

    {#if bootstrapping}
        <div class="tor-status bootstrapping"><span class="spinner"></span> Connecting to Tor network...</div>
    {/if}

    <!-- Matrix -->
    <div class="route-row">
        <div class="proto-info">
            <span class="badge mx">MX</span>
            <span class="proto-name">Matrix</span>
            <span class="proto-status" class:active={$torRouting.matrix !== 'direct'}>
                {#if $torRouting.matrix === 'tor'}via Tor
                {:else if $torRouting.matrix === 'i2p'}via I2P
                {:else}Direct{/if}
            </span>
        </div>
        <div class="route-opts">
            <button class="ropt" class:sel={$torRouting.matrix === 'direct'} on:click={() => requestProtocol('matrix', 'direct')}>Direct</button>
            <button class="ropt" class:sel={$torRouting.matrix === 'tor'} on:click={() => requestProtocol('matrix', 'tor')}>Tor</button>
            <button class="ropt i2p" class:sel={$torRouting.matrix === 'i2p'} on:click={() => requestProtocol('matrix', 'i2p')}>I2P</button>
        </div>
    </div>

    {#if $torRouting.matrix === 'i2p'}
        <div class="i2p-info">Traffic stays entirely inside the I2P network. No exit nodes. First connection takes 2-5 minutes. Connects to: aho2me4...b32.i2p:8448</div>
    {/if}

    <!-- Telegram -->
    <div class="route-row">
        <div class="proto-info">
            <span class="badge tg">TG</span>
            <span class="proto-name">Telegram</span>
            <span class="exp-badge">EXPERIMENTAL</span>
            <span class="proto-status" class:active={$torRouting.telegram !== 'direct'}>{$torRouting.telegram !== 'direct' ? 'via Tor' : 'Direct'}</span>
        </div>
        <div class="route-opts">
            <button class="ropt" class:sel={$torRouting.telegram === 'direct'} on:click={() => requestProtocol('telegram', 'direct')}>Direct</button>
            <button class="ropt" class:sel={$torRouting.telegram === 'tor'} on:click={() => requestProtocol('telegram', 'tor')}>Tor</button>
        </div>
    </div>

    <!-- SimpleX - DISABLED -->
    <div class="route-row disabled">
        <div class="proto-info">
            <span class="badge sx">SX</span>
            <span class="proto-name">SimpleX</span>
            <span class="coming">coming soon</span>
        </div>
        <div class="route-opts">
            <button class="ropt sel" disabled>Direct</button>
            <button class="ropt" disabled>Tor</button>
            <button class="ropt" disabled>V3 Onion</button>
        </div>
    </div>

    <!-- WhatsApp - DISABLED -->
    <div class="route-row disabled">
        <div class="proto-info">
            <span class="badge wa">WA</span>
            <span class="proto-name">WhatsApp</span>
            <span class="exp-badge">EXPERIMENTAL</span>
            <span class="coming">coming soon</span>
        </div>
        <div class="route-opts">
            <button class="ropt sel" disabled>Direct</button>
            <button class="ropt" disabled>Tor</button>
        </div>
    </div>
</div>

<!-- Experimental Warning Dialog -->
{#if showWarning}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="warn-overlay" on:click={() => showWarning = false}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="warn-dialog" on:click|stopPropagation>
        <div class="warn-icon">!</div>
        <h3>Experimental Feature</h3>
        <p>
            Routing {warningProtocol === 'telegram' ? 'Telegram' : 'WhatsApp'} through
            Tor is experimental and may cause problems:
        </p>
        <ul>
            <li>Your account may be temporarily frozen or permanently banned</li>
            <li>Messages may not be delivered reliably</li>
            <li>{warningProtocol === 'telegram'
                ? 'Telegram actively blocks known Tor exit node IPs'
                : 'WhatsApp calls will not work (Tor is TCP only, calls need UDP)'}</li>
        </ul>
        <p class="warn-risk">Use at your own risk. We recommend Tor only with Matrix and SimpleX.</p>
        <div class="warn-btns">
            <button class="wbtn sec" on:click={() => showWarning = false}>Cancel</button>
            <button class="wbtn danger" on:click={confirmWarning}>I understand, enable anyway</button>
        </div>
    </div>
</div>
{/if}

<style>
    .routing-panel { display: flex; flex-direction: column; gap: 12px; }
    .hint { font-size: 0.82em; color: #8b949e; line-height: 1.5; margin: 0 0 8px; }

    .tor-status { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-radius: 8px; font-size: 0.78em; }
    .tor-status.bootstrapping { background: rgba(229,192,123,0.08); color: #e5c07b; }
    .spinner { width: 14px; height: 14px; border: 2px solid rgba(229,192,123,0.3); border-top-color: #e5c07b; border-radius: 50%; animation: spin 0.8s linear infinite; }
    @keyframes spin { to { transform: rotate(360deg); } }

    .route-row {
        display: flex; align-items: center; justify-content: space-between;
        padding: 12px 14px; border-radius: 10px;
        background: rgba(255,255,255,0.02); border: 1px solid rgba(255,255,255,0.04);
    }
    .route-row.disabled { opacity: 0.35; pointer-events: none; }

    .proto-info { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
    .badge { padding: 2px 7px; border-radius: 5px; font-size: 0.7em; font-weight: 700; letter-spacing: 0.5px; }
    .mx { background: rgba(88,166,255,0.15); color: #58a6ff; }
    .tg { background: rgba(97,175,239,0.15); color: #61afef; }
    .sx { background: rgba(198,120,221,0.15); color: #c678dd; }
    .wa { background: rgba(152,195,121,0.15); color: #98c379; }
    .proto-name { font-size: 0.88em; font-weight: 500; }
    .proto-status { font-size: 0.72em; color: #484f58; }
    .proto-status.active { color: #7B68EE; }
    .coming { font-size: 0.65em; color: #484f58; font-style: italic; }
    .exp-badge { font-size: 0.55em; padding: 1px 5px; border-radius: 3px; background: rgba(248,81,73,0.15); color: #f85149; font-weight: 600; letter-spacing: 0.5px; }

    .route-opts { display: flex; gap: 4px; }
    .ropt {
        padding: 5px 14px; border-radius: 7px; font-size: 0.78em; font-weight: 500;
        cursor: pointer; transition: all 0.15s; border: 1px solid rgba(255,255,255,0.04);
        background: rgba(255,255,255,0.03); color: #8b949e; font-family: inherit;
    }
    .ropt:hover:not(:disabled) { background: rgba(255,255,255,0.06); }
    .ropt.sel { background: var(--ac-bg); color: var(--ac, #58a6ff); border-color: var(--ac-border); }
    .ropt.i2p.sel { background: rgba(152,195,121,0.12); color: #98c379; border-color: rgba(152,195,121,0.3); }
    .ropt:disabled { cursor: default; }

    .i2p-info { font-size: 0.75em; color: #98c379; padding: 10px 14px; background: rgba(152,195,121,0.06); border-radius: 8px; line-height: 1.4; margin-top: -4px; }

    /* Warning Dialog */
    .warn-overlay {
        position: fixed; inset: 0; background: rgba(0,0,0,0.7);
        display: flex; align-items: center; justify-content: center; z-index: 9999;
    }
    .warn-dialog {
        background: #161b22; border: 1px solid rgba(255,255,255,0.1);
        border-radius: 14px; padding: 24px; max-width: 420px; text-align: center;
    }
    .warn-icon {
        width: 48px; height: 48px; border-radius: 50%;
        background: rgba(248,81,73,0.15); color: #f85149;
        font-size: 24px; font-weight: 700;
        display: flex; align-items: center; justify-content: center;
        margin: 0 auto 16px;
    }
    .warn-dialog h3 { color: #f85149; margin-bottom: 12px; font-size: 1em; }
    .warn-dialog p { color: #8b949e; font-size: 0.82em; line-height: 1.5; margin-bottom: 8px; }
    .warn-dialog ul { text-align: left; color: #8b949e; font-size: 0.75em; line-height: 1.6; margin: 8px 0 12px 20px; }
    .warn-risk { color: #d29922; font-weight: 600; font-size: 0.78em; }
    .warn-btns { display: flex; gap: 10px; margin-top: 16px; justify-content: center; }
    .wbtn {
        padding: 8px 20px; border-radius: 8px; font-size: 0.82em; font-family: inherit;
        cursor: pointer; transition: all 0.15s; border: none;
    }
    .wbtn.sec { background: rgba(255,255,255,0.06); color: #8b949e; }
    .wbtn.sec:hover { background: rgba(255,255,255,0.1); }
    .wbtn.danger { background: rgba(248,81,73,0.1); color: #f85149; border: 1px solid rgba(248,81,73,0.3); }
    .wbtn.danger:hover { background: rgba(248,81,73,0.2); }
</style>
