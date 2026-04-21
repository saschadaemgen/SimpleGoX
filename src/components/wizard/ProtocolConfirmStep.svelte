<script>
    // Briefing 042 v2 W4: per-protocol confirmation screen.
    //
    // One reusable component. Parent passes { protocol, nextProtocol } plus a
    // continue handler. Renders a centered card with an accent-colored check
    // circle, the protocol code in monospace, a title "X configured", and a
    // "Next: Y" hint (empty line if no next protocol). Animations play once
    // on mount (check line-draw, text fade-up), no loops.
    //
    // This component is the cure for the "Matrix configured" false-positive
    // bug: the protocol name is always read from the `protocol` prop passed
    // by the wizard, never hardcoded.

    import { createEventDispatcher } from 'svelte';
    const dispatch = createEventDispatcher();

    export let protocol = 'simplex'; // 'simplex' | 'matrix' | 'telegram'
    export let nextProtocol = null;  // same set or null

    const LABELS = {
        simplex:  { code: 'SX', name: 'SimpleX'  },
        matrix:   { code: 'MX', name: 'Matrix'   },
        telegram: { code: 'TG', name: 'Telegram' },
    };

    $: current = LABELS[protocol] ?? { code: '??', name: protocol };
    $: next = nextProtocol ? (LABELS[nextProtocol] ?? null) : null;
</script>

<div class="confirm">
    <div class="stage">
        <div class="circle" aria-hidden="true">
            <svg viewBox="0 0 20 20" width="20" height="20">
                <polyline class="check-stroke" points="4 11 8 15 16 6" />
            </svg>
        </div>

        <p class="code mono fade-1">{current.code}</p>
        <h3 class="title fade-2">{current.name} configured</h3>
        <p class="next fade-3">{next ? `Next: ${next.name}` : '\u00A0'}</p>

        <button class="continue-btn" on:click={() => dispatch('continue')}>
            Continue
        </button>
    </div>
</div>

<style>
    .confirm {
        display: flex; align-items: center; justify-content: center;
        height: 100%;
    }
    .stage {
        max-width: 280px;
        display: flex; flex-direction: column; align-items: center;
        gap: 8px;
    }

    /* Accent-bordered circle with hand-drawn check */
    .circle {
        width: 44px; height: 44px; border-radius: 50%;
        border: 1px solid var(--ac, #3fb9a8);
        display: flex; align-items: center; justify-content: center;
        margin-bottom: 4px;
    }
    .check-stroke {
        fill: none;
        stroke: var(--ac, #3fb9a8);
        stroke-width: 2;
        stroke-linecap: round;
        stroke-linejoin: round;
        stroke-dasharray: 60;
        stroke-dashoffset: 60;
        animation: checkDraw 0.6s ease-out 0.3s forwards;
    }
    @keyframes checkDraw {
        to { stroke-dashoffset: 0; }
    }

    /* Text */
    .code {
        font-size: 10px;
        letter-spacing: 1px;
        color: #707379;
        margin: 0;
    }
    .title {
        font-size: 14px;
        font-weight: 500;
        color: #d4d7dd;
        margin: 0;
    }
    .next {
        font-size: 11px;
        color: #8b8f97;
        margin: 0 0 14px;
        min-height: 1.4em; /* reserve space so layout doesn't shift when empty */
    }
    .mono {
        font-family: 'JetBrains Mono', ui-monospace, Menlo, Consolas, monospace;
    }

    /* One-shot fade-up per line */
    .fade-1, .fade-2, .fade-3 {
        opacity: 0; transform: translateY(6px);
        animation: fadeUp 0.5s ease-out forwards;
    }
    .fade-1 { animation-delay: 0.5s; }
    .fade-2 { animation-delay: 0.7s; }
    .fade-3 { animation-delay: 0.9s; }
    @keyframes fadeUp {
        to { opacity: 1; transform: translateY(0); }
    }

    .continue-btn {
        margin-top: 4px;
        padding: 8px 28px; border-radius: 10px; border: none;
        background: var(--ac, #3fb9a8); color: #0e1117;
        font-size: 12px; font-weight: 600; font-family: inherit;
        cursor: pointer; transition: filter 0.15s;
        opacity: 0;
        animation: fadeUp 0.4s ease-out 1.1s forwards;
    }
    .continue-btn:hover { filter: brightness(1.08); }
</style>
