<script>
    // Briefing 045 W4: blocking startup overlay. Renders while the
    // backend is still loading (sidecar ready check, profile fetch,
    // contact list fetch). Prevents the race where the main chat
    // layout or the wizard tries to render before we know whether
    // the user has a profile or how many contacts the backend holds.
    //
    // Visual language mirrors SplashScreen (same logo block, same
    // dark background #0e1117) but adds a status line that reflects
    // `startupStatus.phase` and a detail line with any technical info.
    // On `phase === 'error'` a retry button is shown.

    import { startupStatus } from '../lib/stores.js';

    export let onretry = () => {};

    // Friendly copy per phase. Keeps the main App.svelte free of UX
    // strings; phase identifiers stay machine-friendly.
    $: phaseLabel = (() => {
        switch ($startupStatus.phase) {
            case 'booting':
                return $startupStatus.message || 'Starting SimpleGoX...';
            case 'sidecar_waiting':
                return $startupStatus.message || 'Waiting for secure sidecar...';
            case 'loading_profile':
                return $startupStatus.message || 'Loading your profile...';
            case 'loading_contacts':
                return $startupStatus.message || 'Loading your contacts...';
            case 'error':
                return $startupStatus.message || 'Startup failed';
            default:
                return $startupStatus.message || '';
        }
    })();

    $: showRetry = $startupStatus.phase === 'error';
</script>

<div class="startup">
    <div class="content">
        <div class="logo">
            <span class="w">Simple</span><span class="ac">Go</span><span class="w">X</span>
        </div>
        <div class="phase" class:err={showRetry}>{phaseLabel}</div>
        {#if $startupStatus.detail}
            <div class="detail">{$startupStatus.detail}</div>
        {/if}
        {#if showRetry}
            {#if $startupStatus.error}
                <div class="error-text">{$startupStatus.error}</div>
            {/if}
            <button class="retry" on:click={onretry}>Retry</button>
        {/if}
    </div>
</div>

<style>
    .startup {
        position: fixed;
        inset: 0;
        background: #0e1117;
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 10000;
    }
    .content {
        text-align: center;
        max-width: 420px;
        padding: 0 24px;
    }
    .logo {
        font-size: 48px;
        font-weight: 300;
        letter-spacing: -1px;
        margin-bottom: 28px;
    }
    .logo .w {
        color: #e6edf3;
    }
    .logo .ac {
        color: var(--ac, #58a6ff);
    }
    .phase {
        font-size: 15px;
        font-weight: 500;
        color: #b1bac4;
        margin-bottom: 6px;
        min-height: 22px;
    }
    .phase.err {
        color: #e06c75;
    }
    .detail {
        font-size: 12px;
        color: #8b949e;
        margin-bottom: 16px;
        font-family: 'JetBrains Mono', ui-monospace, Menlo, Consolas, monospace;
    }
    .error-text {
        font-size: 12px;
        color: #e06c75;
        background: rgba(224, 108, 117, 0.08);
        border: 1px solid rgba(224, 108, 117, 0.2);
        border-radius: 6px;
        padding: 8px 10px;
        margin: 12px 0;
        font-family: 'JetBrains Mono', ui-monospace, Menlo, Consolas, monospace;
        max-height: 200px;
        overflow-y: auto;
        text-align: left;
    }
    .retry {
        margin-top: 8px;
        padding: 10px 28px;
        border: none;
        border-radius: 10px;
        background: var(--ac, #3fb9a8);
        color: #0e1117;
        font-size: 14px;
        font-weight: 600;
        font-family: inherit;
        cursor: pointer;
        transition: filter 0.15s;
    }
    .retry:hover {
        filter: brightness(1.1);
    }
</style>
