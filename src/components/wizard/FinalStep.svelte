<script>
    // Briefing 042 v2 W6: terminal wizard screen.
    //
    // Design decisions approved by Prinz:
    // - Grayscale base, accent-only color for ring/check/cursor.
    // - Ring draws itself closed in 1.2s (stroke-dashoffset).
    // - Check draws inside the ring after the ring completes (0.6s, delay 0.8s).
    // - No protocol enumeration, no "congratulations", no exclamation marks.
    // - Two lines of copy: "Setup complete" + monospace "Opening app_" with
    //   blinking cursor.
    // - Auto-advance EXACTLY 2s after mount; no cancel, no button.
    //
    // Replaces the old ReadyStep.svelte which had confetti + a hardcoded
    // "Matrix" account row that produced the wrong-protocol-name bug.

    import { createEventDispatcher, onMount } from 'svelte';
    const dispatch = createEventDispatcher();

    onMount(() => {
        const timer = setTimeout(() => dispatch('complete'), 2000);
        return () => clearTimeout(timer);
    });
</script>

<div class="final">
    <div class="stage">
        <svg class="ring" viewBox="0 0 60 60" width="60" height="60" aria-hidden="true">
            <circle class="ring-circle" cx="30" cy="30" r="28" />
            <polyline class="ring-check" points="20 31 27 38 40 24" />
        </svg>

        <h2 class="title">Setup complete</h2>
        <p class="sub"><span class="mono">Opening app</span><span class="cursor">_</span></p>
    </div>
</div>

<style>
    .final {
        display: flex; align-items: center; justify-content: center;
        height: 100%; text-align: center;
    }
    .stage {
        max-width: 320px;
        display: flex; flex-direction: column; align-items: center;
        gap: 14px;
    }

    /* --- Ring + Check --- */
    .ring { display: block; }
    .ring-circle {
        fill: none;
        stroke: var(--ac, #3fb9a8);
        stroke-width: 1.5;
        stroke-linecap: round;
        /* circumference of r=28 is ~176 */
        stroke-dasharray: 176;
        stroke-dashoffset: 176;
        transform: rotate(-90deg);
        transform-origin: 30px 30px;
        animation: ringDraw 1.2s ease-out 0.1s forwards;
    }
    .ring-check {
        fill: none;
        stroke: var(--ac, #3fb9a8);
        stroke-width: 2;
        stroke-linecap: round;
        stroke-linejoin: round;
        stroke-dasharray: 40;
        stroke-dashoffset: 40;
        animation: checkDraw 0.6s ease-out 0.9s forwards;
    }
    @keyframes ringDraw {
        to { stroke-dashoffset: 0; }
    }
    @keyframes checkDraw {
        to { stroke-dashoffset: 0; }
    }

    /* --- Text --- */
    .title {
        font-size: 15px; font-weight: 500;
        margin: 0;
        color: #d4d7dd;
        opacity: 0; transform: translateY(4px);
        animation: fadeUp 0.5s ease-out 1.1s forwards;
    }
    .sub {
        margin: 0;
        font-size: 11px;
        color: #707379;
        opacity: 0; transform: translateY(4px);
        animation: fadeUp 0.5s ease-out 1.35s forwards;
    }
    @keyframes fadeUp {
        to { opacity: 1; transform: translateY(0); }
    }

    .mono {
        font-family: 'JetBrains Mono', ui-monospace, Menlo, Consolas, monospace;
    }
    .cursor {
        display: inline-block;
        width: 0.6ch;
        color: var(--ac, #3fb9a8);
        font-family: 'JetBrains Mono', ui-monospace, Menlo, Consolas, monospace;
        animation: blink 1s step-end infinite;
    }
    @keyframes blink {
        50% { opacity: 0; }
    }
</style>
