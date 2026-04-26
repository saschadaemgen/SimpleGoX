<script>
    // Briefing 042 v2 W1: state machine rewrite.
    //
    // Replaces the 042 v1 `phaseIndex + buildPhases(selected)` logic which
    // had two observable bugs:
    //   1. ReadyStep hardcoded "Matrix configured" even when only SimpleX
    //      was set up (bug lived in ReadyStep.svelte, now replaced by
    //      FinalStep.svelte).
    //   2. No per-protocol confirmation screens — user couldn't tell when
    //      one protocol's setup ended and the next began.
    //
    // Model: pure state machine driven by `queue + current + configured`.
    //   step ∈ { welcome | choose | protocol_setup | protocol_confirm | final }
    //   selected   — the set of protocols the user checked in Choose step
    //   queue      — remaining protocols to process (FIFO, fixed order sx→mx→tg)
    //   current    — protocol whose setup OR confirm is currently on screen
    //   configured — protocols that finished setup successfully (not skipped)
    //
    // Flow:
    //   welcome → choose → [for each selected p in order sx→mx→tg:
    //                         protocol_setup(p) → protocol_confirm(p)
    //                         OR protocol_setup(p) → skip → next in queue]
    //                    → final (auto-advances to app after 2s)
    //
    // Skip path bypasses the confirmation screen (no false-positive).
    // AnimatedBackground preserved per Prinz's explicit request.

    import { createEventDispatcher } from 'svelte';
    import WelcomeStep from './WelcomeStep.svelte';
    import ChooseMessengersStep from './ChooseMessengersStep.svelte';
    import SimpleXSetupStep from './SimpleXSetupStep.svelte';
    import MatrixSetupStep from './MatrixSetupStep.svelte';
    import TelegramSetupStep from './TelegramSetupStep.svelte';
    import ProtocolConfirmStep from './ProtocolConfirmStep.svelte';
    import FinalStep from './FinalStep.svelte';
    import AnimatedBackground from '../AnimatedBackground.svelte';
    const dispatch = createEventDispatcher();

    // --- state ---
    const PROTOCOL_ORDER = ['simplex', 'matrix', 'telegram'];

    let step = 'welcome';
    // Preserved from v1 for compatibility with ChooseMessengersStep which
    // binds to this object.
    let selected = { simplex: true, matrix: true, telegram: false };
    let queue = [];
    let current = null;
    let configured = new Set();
    let telegramName = '';
    let slideKey = 0;

    // --- progress bar state ---
    // Total dots = welcome(implicit) + choose + N_selected * 2 (setup + confirm) + final
    // Hidden on welcome (phaseIndex > 0 guard in v1). Keep same behaviour:
    // show dots starting from `choose` onwards.
    $: selectedCount = Object.values(selected).filter(Boolean).length;
    $: progressTotal = 1 /* choose */ + selectedCount * 2 /* setup+confirm */ + 1 /* final */;
    $: progressIndex = computeProgressIndex(step, current, queue, configured);

    function computeProgressIndex(stp, cur, q, cfg) {
        if (stp === 'welcome') return -1;
        if (stp === 'choose') return 0;
        if (stp === 'final') return progressTotal - 1;
        // protocol_setup / protocol_confirm: count completed protocols * 2
        // plus offset for setup (0) or confirm (1).
        const pos = PROTOCOL_ORDER.filter(p => selected[p]).indexOf(cur);
        if (pos < 0) return 0;
        const base = 1 /* choose */ + pos * 2;
        return base + (stp === 'protocol_confirm' ? 1 : 0);
    }

    // --- transitions ---

    function buildQueue() {
        return PROTOCOL_ORDER.filter(p => selected[p]);
    }

    function toChoose() {
        step = 'choose';
        slideKey++;
    }

    function afterChoose() {
        queue = buildQueue();
        if (queue.length === 0) {
            // At-least-one guard should have prevented this, but be defensive.
            step = 'final';
        } else {
            current = queue[0];
            step = 'protocol_setup';
        }
        slideKey++;
    }

    // Setup of `current` completed successfully — show the confirmation
    // screen for the same protocol.
    function afterSetup() {
        if (current) configured.add(current);
        configured = configured; // nudge reactivity
        step = 'protocol_confirm';
        slideKey++;
    }

    // Skip button on a setup step — drop `current` from the queue WITHOUT
    // showing a confirmation for it (no false-positive), move to next.
    function afterSkip() {
        advanceQueue();
    }

    // User clicked Continue on a confirmation screen — move to the next
    // protocol, or if the queue is drained, transition to final (>=2
    // configured) or directly complete (<=1 configured, per Briefing 042a
    // Fix A: the confirmation screen already IS the completion beat when
    // only one protocol was set up).
    function afterConfirm() {
        advanceQueue();
    }

    function advanceQueue() {
        const rest = queue.slice(1);
        queue = rest;
        if (rest.length === 0) {
            current = null;
            // Briefing 042a Fix A: skip the FinalStep when 0 or 1
            // protocols were actually configured. The ProtocolConfirmStep
            // the user just clicked Continue on was already the "setup
            // complete" beat. A second identical screen feels bureaucratic.
            if (configured.size <= 1) {
                dispatch('complete');
                return;
            }
            step = 'final';
        } else {
            current = rest[0];
            step = 'protocol_setup';
        }
        slideKey++;
    }

    function back() {
        // Pragmatic back: from any protocol step return to the Choose screen
        // with selection preserved. More granular back navigation (step-by-step
        // reverse) is a future enhancement; users rarely want to unwind a
        // multi-step configuration, but DO want to change which protocols
        // they picked after starting setup.
        if (step === 'choose') {
            step = 'welcome';
        } else if (step === 'protocol_setup' || step === 'protocol_confirm') {
            queue = [];
            current = null;
            configured = new Set();
            step = 'choose';
        }
        slideKey++;
    }

    function handleWelcomeNext() {
        toChoose();
    }

    // --- derived per-confirm "next protocol" label ---
    $: nextProtocol = (step === 'protocol_confirm' && queue.length > 1) ? queue[1] : null;

    // --- final: always advance to app when FinalStep auto-completes ---
    function onFinalComplete() {
        dispatch('complete');
    }
</script>

<div class="wizard">
    <AnimatedBackground />

    {#if step !== 'welcome'}
        <div class="progress" aria-hidden="true">
            {#each Array(progressTotal) as _, i}
                <div class="dot"
                     class:active={progressIndex === i}
                     class:done={progressIndex > i}>
                    {#if progressIndex > i}
                        <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12"/></svg>
                    {/if}
                </div>
                {#if i < progressTotal - 1}
                    <div class="line" class:filled={progressIndex > i}></div>
                {/if}
            {/each}
        </div>
    {/if}

    {#key slideKey}
        <div class="phase">
            {#if step === 'welcome'}
                <WelcomeStep on:next={handleWelcomeNext} />
            {:else if step === 'choose'}
                <ChooseMessengersStep bind:selected on:next={afterChoose} on:back={back} />
            {:else if step === 'protocol_setup' && current === 'simplex'}
                <SimpleXSetupStep
                    {selected}
                    on:next={afterSetup}
                    on:back={back}
                    on:skip={afterSkip}
                />
            {:else if step === 'protocol_setup' && current === 'matrix'}
                <MatrixSetupStep on:next={afterSetup} on:back={back} />
            {:else if step === 'protocol_setup' && current === 'telegram'}
                <TelegramSetupStep bind:telegramName on:next={afterSetup} on:skip={afterSkip} />
            {:else if step === 'protocol_confirm'}
                <ProtocolConfirmStep
                    protocol={current}
                    {nextProtocol}
                    on:continue={afterConfirm}
                />
            {:else if step === 'final'}
                <FinalStep on:complete={onFinalComplete} />
            {/if}
        </div>
    {/key}
</div>

<style>
    .wizard {
        height: 100vh; width: 100vw;
        background: #0d0f13;
        display: flex; flex-direction: column; overflow: hidden;
        position: relative;
    }

    .progress {
        position: relative; z-index: 1;
        display: flex; align-items: center; justify-content: center;
        gap: 0; padding: 22px 0 0; flex-shrink: 0;
    }
    .dot {
        width: 10px; height: 10px; border-radius: 50%;
        background: #23272e;
        display: flex; align-items: center; justify-content: center;
        transition: all 0.3s ease; color: #0d0f13;
    }
    .dot.active {
        background: var(--ac, #3fb9a8);
        transform: scale(1.2);
    }
    .dot.done { background: var(--ac, #3fb9a8); }
    .line {
        width: 26px; height: 1px;
        background: #23272e;
        transition: background 0.3s ease;
    }
    .line.filled { background: var(--ac, #3fb9a8); }

    .phase {
        flex: 1; display: flex; flex-direction: column; justify-content: center;
        position: relative; z-index: 1;
        animation: slideIn 0.4s ease-out;
    }
    @keyframes slideIn {
        from { opacity: 0; transform: translateX(20px); }
        to { opacity: 1; transform: translateX(0); }
    }
</style>
