<script>
    import { createEventDispatcher } from 'svelte';
    import WelcomeStep from './WelcomeStep.svelte';
    import ChooseMessengersStep from './ChooseMessengersStep.svelte';
    import MatrixSetupStep from './MatrixSetupStep.svelte';
    import TelegramSetupStep from './TelegramSetupStep.svelte';
    import ReadyStep from './ReadyStep.svelte';
    import AnimatedBackground from '../AnimatedBackground.svelte';
    const dispatch = createEventDispatcher();

    let phaseIndex = 0;
    let slideKey = 0;
    let selected = { matrix: true, telegram: false };
    let telegramName = '';

    // Dynamic phase list based on selection
    $: phases = buildPhases(selected);

    function buildPhases(sel) {
        let p = ['welcome', 'choose'];
        if (sel.matrix) p.push('matrix');
        if (sel.telegram) p.push('telegram');
        p.push('ready');
        return p;
    }

    $: currentPhase = phases[phaseIndex] || 'welcome';

    function next() {
        if (phaseIndex < phases.length - 1) {
            phaseIndex++;
            slideKey++;
        }
    }

    function back() {
        if (phaseIndex > 0) {
            phaseIndex--;
            slideKey++;
        }
    }

    function skipTelegram() {
        // Jump to ready (last phase)
        phaseIndex = phases.length - 1;
        slideKey++;
    }

    // After choose step, recalculate phases and jump to index 2
    function afterChoose() {
        phases = buildPhases(selected);
        phaseIndex = 2; // first setup phase after choose
        slideKey++;
    }
</script>

<div class="wizard">
    <AnimatedBackground />

    {#if phaseIndex > 0}
        <div class="progress">
            {#each phases as _, i}
                <div class="dot"
                     class:active={phaseIndex === i}
                     class:done={phaseIndex > i}>
                    {#if phaseIndex > i}
                        <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12"/></svg>
                    {/if}
                </div>
                {#if i < phases.length - 1}
                    <div class="line" class:filled={phaseIndex > i}></div>
                {/if}
            {/each}
        </div>
    {/if}

    {#key slideKey}
        <div class="phase">
            {#if currentPhase === 'welcome'}
                <WelcomeStep on:next={next} />
            {:else if currentPhase === 'choose'}
                <ChooseMessengersStep bind:selected on:next={afterChoose} on:back={back} />
            {:else if currentPhase === 'matrix'}
                <MatrixSetupStep on:next={next} on:back={back} />
            {:else if currentPhase === 'telegram'}
                <TelegramSetupStep bind:telegramName on:next={next} on:skip={skipTelegram} />
            {:else if currentPhase === 'ready'}
                <ReadyStep {telegramName} on:complete={() => dispatch('complete')} />
            {/if}
        </div>
    {/key}
</div>

<style>
    .wizard {
        height: 100vh; width: 100vw; background: #0e1117;
        display: flex; flex-direction: column; overflow: hidden;
        position: relative;
    }

    .progress {
        position: relative; z-index: 1;
        display: flex; align-items: center; justify-content: center;
        gap: 0; padding: 28px 0 0; flex-shrink: 0;
    }
    .dot {
        width: 12px; height: 12px; border-radius: 50%;
        background: rgba(255,255,255,0.08);
        display: flex; align-items: center; justify-content: center;
        transition: all 0.3s ease; color: #0e1117;
    }
    .dot.active {
        background: var(--ac, #58a6ff);
        box-shadow: 0 0 12px rgba(88,166,255,0.35);
        transform: scale(1.2);
    }
    .dot.done { background: var(--ac, #58a6ff); }
    .line {
        width: 36px; height: 2px; background: rgba(255,255,255,0.08);
        transition: background 0.3s ease;
    }
    .line.filled { background: var(--ac, #58a6ff); }

    .phase {
        flex: 1; display: flex; flex-direction: column; justify-content: center;
        position: relative; z-index: 1;
        animation: slideIn 0.4s ease-out;
    }
    @keyframes slideIn {
        from { opacity: 0; transform: translateX(40px); }
        to { opacity: 1; transform: translateX(0); }
    }
</style>
