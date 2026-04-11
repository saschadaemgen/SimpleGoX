<script>
    import { onMount, createEventDispatcher } from 'svelte';
    import AnimatedBackground from './AnimatedBackground.svelte';
    const dispatch = createEventDispatcher();

    let fadeOut = false;

    onMount(() => {
        setTimeout(() => fadeOut = true, 2000);
        setTimeout(() => dispatch('done'), 2400);
    });
</script>

<div class="splash" class:out={fadeOut}>
    <AnimatedBackground />
    <div class="content">
        <div class="logo"><span class="w">Simple</span><span class="ac">Go</span><span class="w">X</span></div>
        <div class="tag">One inbox. Four protocols. Your keys.</div>
    </div>
</div>

<style>
    .splash {
        position: fixed; inset: 0; background: #0e1117;
        display: flex; align-items: center; justify-content: center;
        z-index: 9999;
    }
    .splash.out { opacity: 0; transition: opacity 0.3s ease; }

    .content { position: relative; z-index: 1; text-align: center; }

    .logo { font-size: 48px; font-weight: 300; letter-spacing: -1px; }
    .w { color: #e6edf3; }
    .ac { color: var(--ac, #58a6ff); }

    .tag { font-size: 14px; color: #8b949e; margin-top: 2px; }
</style>
