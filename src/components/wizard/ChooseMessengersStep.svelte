<script>
    import { createEventDispatcher } from 'svelte';
    const dispatch = createEventDispatcher();

    export let selected = { matrix: true, telegram: false };

    function toggle(proto) {
        selected[proto] = !selected[proto];
        // At least one must stay selected
        if (!selected.matrix && !selected.telegram) {
            selected[proto] = true;
        }
        selected = selected;
    }

    $: canContinue = selected.matrix || selected.telegram;
</script>

<div class="step">
    <h2 class="title">Choose your messengers</h2>
    <p class="sub">Select the protocols you want to connect. You can always add more later in Settings.</p>

    <div class="grid">
        <button class="card" class:sel={selected.matrix} on:click={() => toggle('matrix')}>
            <span class="badge mx">MX</span>
            <div class="name">Matrix</div>
            <div class="desc">Federated, encrypted, open protocol</div>
            <div class="check" class:on={selected.matrix}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12"/></svg>
            </div>
        </button>

        <button class="card" class:sel={selected.telegram} on:click={() => toggle('telegram')}>
            <span class="badge tg">TG</span>
            <div class="name">Telegram</div>
            <div class="desc">Fast, popular, feature-rich messaging</div>
            <div class="check" class:on={selected.telegram}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12"/></svg>
            </div>
        </button>

        <div class="card disabled">
            <span class="badge sx">SX</span>
            <div class="name">SimpleX</div>
            <div class="desc">Maximum privacy, no metadata</div>
            <span class="soon">Coming Soon</span>
        </div>

        <div class="card disabled">
            <span class="badge wa">WA</span>
            <div class="name">WhatsApp</div>
            <div class="desc">EU DMA interoperability</div>
            <span class="soon">Coming Soon</span>
        </div>
    </div>

    <div class="actions">
        <button class="btn sec" on:click={() => dispatch('back')}>Back</button>
        <button class="btn pri" disabled={!canContinue} on:click={() => dispatch('next')}>Continue</button>
    </div>
</div>

<style>
    .step { max-width: 480px; margin: 0 auto; padding: 0 32px; }
    .title { font-size: 1.2em; font-weight: 600; margin: 0 0 8px; }
    .sub { font-size: 0.84em; color: #8b949e; margin: 0 0 24px; line-height: 1.5; }

    .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; margin-bottom: 28px; }

    .card {
        padding: 20px; border-radius: 14px; border: 1px solid rgba(255,255,255,0.06);
        background: rgba(255,255,255,0.02); cursor: pointer; text-align: left;
        font-family: inherit; color: inherit; transition: all 0.2s; position: relative;
    }
    .card:hover:not(.disabled) { border-color: rgba(255,255,255,0.12); background: rgba(255,255,255,0.04); }
    .card.sel { border-color: var(--ac, #3fb9a8); background: var(--ac-bg); }
    .card.disabled { opacity: 0.35; cursor: default; pointer-events: none; }

    .badge {
        display: inline-block; padding: 4px 10px; border-radius: 6px;
        font-size: 0.68em; font-weight: 700; letter-spacing: 0.5px; margin-bottom: 10px;
    }
    .mx { background: rgba(63,185,168,0.15); color: #3fb9a8; }
    .tg { background: rgba(97,175,239,0.15); color: #61afef; }
    .sx { background: rgba(198,120,221,0.15); color: #c678dd; }
    .wa { background: rgba(152,195,121,0.15); color: #98c379; }

    .name { font-size: 0.92em; font-weight: 600; margin-bottom: 4px; }
    .desc { font-size: 0.75em; color: #8b949e; line-height: 1.4; }

    .check {
        position: absolute; top: 12px; right: 12px;
        width: 22px; height: 22px; border-radius: 50%;
        border: 2px solid rgba(255,255,255,0.1);
        display: flex; align-items: center; justify-content: center;
        transition: all 0.2s; color: transparent;
    }
    .check.on {
        background: var(--ac, #3fb9a8); border-color: var(--ac, #3fb9a8); color: #0e1117;
        animation: pop 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
    }
    @keyframes pop { 0% { transform: scale(0); } 100% { transform: scale(1); } }

    .soon { position: absolute; bottom: 8px; right: 12px; font-size: 0.6em; color: #8b949e; font-style: italic; }

    .actions { display: flex; justify-content: space-between; }
    .btn {
        padding: 11px 28px; border-radius: 10px; border: none;
        font-size: 0.88em; font-weight: 600; font-family: inherit; cursor: pointer; transition: all 0.15s;
    }
    .btn.pri { background: var(--ac, #3fb9a8); color: #0e1117; }
    .btn.pri:hover:not(:disabled) { filter: brightness(1.1); }
    .btn.pri:disabled { opacity: 0.4; cursor: default; }
    .btn.sec { background: rgba(255,255,255,0.06); color: #c9d1d9; }
    .btn.sec:hover { background: rgba(255,255,255,0.1); }
</style>
