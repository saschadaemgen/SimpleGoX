<script>
    import { createEventDispatcher, onMount } from 'svelte';
    import WelcomeStep from './WelcomeStep.svelte';
    import ChooseMessengersStep from './ChooseMessengersStep.svelte';
    import MatrixSetupStep from './MatrixSetupStep.svelte';
    import TelegramSetupStep from './TelegramSetupStep.svelte';
    import ReadyStep from './ReadyStep.svelte';
    const dispatch = createEventDispatcher();

    let bgCanvas;

    onMount(() => {
        const ctx = bgCanvas.getContext('2d');
        const w = bgCanvas.width = bgCanvas.offsetWidth * 2;
        const h = bgCanvas.height = bgCanvas.offsetHeight * 2;

        // Aurora blobs
        const blobs = [
            {x:0.3, y:0.4, r:0.35, color:[88,166,255], speed:0.003, phase:0},
            {x:0.7, y:0.6, r:0.3, color:[104,158,213], speed:0.002, phase:2},
            {x:0.5, y:0.3, r:0.25, color:[63,185,168], speed:0.004, phase:4},
        ];

        // Floating particles
        const pts = [];
        for (let i = 0; i < 60; i++) {
            pts.push({
                x: Math.random() * w,
                y: Math.random() * h,
                vx: (Math.random() - 0.5) * 0.8,
                vy: (Math.random() - 0.5) * 0.8,
                r: Math.random() * 2 + 1
            });
        }

        let t = 0;
        let animId;

        function draw() {
            t += 0.01;
            ctx.clearRect(0, 0, w, h);

            // Layer 1: Aurora gradient blobs
            for (let b of blobs) {
                let cx = (b.x + Math.sin(t * b.speed * 100 + b.phase) * 0.15) * w;
                let cy = (b.y + Math.cos(t * b.speed * 80 + b.phase) * 0.1) * h;
                let r = b.r * w;
                let grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, r);
                grad.addColorStop(0, 'rgba(' + b.color.join(',') + ',0.12)');
                grad.addColorStop(0.5, 'rgba(' + b.color.join(',') + ',0.04)');
                grad.addColorStop(1, 'rgba(' + b.color.join(',') + ',0)');
                ctx.fillStyle = grad;
                ctx.fillRect(0, 0, w, h);
            }

            // Layer 2: Floating connected particles
            for (let i = 0; i < pts.length; i++) {
                let p = pts[i];
                p.x += p.vx; p.y += p.vy;
                if (p.x < 0) p.x = w;
                if (p.x > w) p.x = 0;
                if (p.y < 0) p.y = h;
                if (p.y > h) p.y = 0;

                ctx.beginPath();
                ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
                ctx.fillStyle = 'rgba(88,166,255,0.4)';
                ctx.fill();

                for (let j = i + 1; j < pts.length; j++) {
                    let q = pts[j];
                    let dx = p.x - q.x, dy = p.y - q.y;
                    let d = Math.sqrt(dx * dx + dy * dy);
                    if (d < 200) {
                        ctx.beginPath();
                        ctx.moveTo(p.x, p.y);
                        ctx.lineTo(q.x, q.y);
                        ctx.strokeStyle = 'rgba(88,166,255,' + (0.15 * (1 - d / 200)) + ')';
                        ctx.lineWidth = 0.5;
                        ctx.stroke();
                    }
                }
            }

            animId = requestAnimationFrame(draw);
        }

        draw();
        return () => cancelAnimationFrame(animId);
    });

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
    <canvas class="wizard-bg" bind:this={bgCanvas}></canvas>

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

    .wizard-bg {
        position: absolute; inset: 0; width: 100%; height: 100%;
        z-index: 0; pointer-events: none;
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
