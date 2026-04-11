<script>
    import { createEventDispatcher } from 'svelte';
    const dispatch = createEventDispatcher();
</script>

<div class="welcome">
    <div class="content">
        <div class="logo" style="--d:0"><span class="w">Simple</span><span class="ac">Go</span><span class="w">X</span></div>
        <div class="claim" style="--d:1">One inbox. Four protocols. Your keys.</div>
        <p class="desc" style="--d:2">
            SimpleGoX connects Matrix, Telegram, SimpleX and WhatsApp
            in one place. Every protocol runs isolated on your device.
            Your keys never leave your machine.
        </p>
        <button class="cta" style="--d:3" on:click={() => dispatch('next')}>Get Started</button>
        <div class="ver" style="--d:4">v0.0.1-pre-alpha</div>
    </div>
</div>

<style>
    .welcome {
        display: flex; align-items: center; justify-content: center;
        height: 100%; text-align: center;
        background:
            radial-gradient(ellipse at 30% 40%, rgba(63,185,168,0.07) 0%, transparent 60%),
            radial-gradient(ellipse at 70% 80%, rgba(97,175,239,0.04) 0%, transparent 50%);
    }

    .content { max-width: 420px; padding: 0 32px; }

    .logo, .claim, .desc, .cta, .ver {
        opacity: 0; animation: fadeUp 0.5s ease-out forwards;
        animation-delay: calc(var(--d) * 150ms + 100ms);
    }
    @keyframes fadeUp {
        from { opacity: 0; transform: translateY(16px); }
        to { opacity: 1; transform: translateY(0); }
    }

    .logo { font-size: 2.6em; font-weight: 700; letter-spacing: -1px; margin-bottom: 12px; }
    .w { color: #e6edf3; }
    .ac { color: var(--ac, #58a6ff); }

    .claim { font-size: 1.15em; font-weight: 600; color: #b1bac4; margin-bottom: 20px; }

    .desc { font-size: 0.88em; color: #8b949e; line-height: 1.65; margin: 0 0 32px; }

    .cta {
        padding: 14px 48px; border: none; border-radius: 12px;
        background: var(--ac, #3fb9a8); color: #0e1117;
        font-size: 1em; font-weight: 600; font-family: inherit;
        cursor: pointer; transition: all 0.15s;
    }
    .cta:hover { transform: translateY(-2px); box-shadow: 0 8px 24px rgba(63,185,168,0.2); }
    .cta:active { transform: scale(0.98); }

    .ver { font-size: 0.72em; color: #484f58; margin-top: 32px; }
</style>
