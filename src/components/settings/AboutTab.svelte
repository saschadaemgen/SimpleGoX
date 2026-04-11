<script>
    import { createEventDispatcher } from 'svelte';
    const dispatch = createEventDispatcher();
</script>

<div class="about">
    <div class="logo-area">
        <span class="logo-text"><span class="lw">Simple</span><span class="ac">Go</span><span class="lw">X</span></span>
        <span class="version">Desktop v0.0.1-pre-alpha</span>
        <span class="season">Season 2</span>
    </div>

    <p class="desc">
        An open-source multi-messenger client built with Rust, Tauri v2, and Svelte.
    </p>

    <div class="tagline">
        <span class="proto-tag mx">Matrix</span>
        <span class="proto-tag tg">Telegram</span>
        <span class="proto-tag sx">SimpleX</span>
        <span class="proto-tag wa">WhatsApp</span>
    </div>
    <p class="motto">Four protocols. One inbox. Your keys.</p>

    <div class="divider"></div>

    <div class="links">
        <div class="link-row">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 0 0-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0 0 20 4.77 5.07 5.07 0 0 0 19.91 1S18.73.65 16 2.48a13.38 13.38 0 0 0-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 0 0 5 4.77a5.44 5.44 0 0 0-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 0 0 9 18.13V22"/></svg>
            <span>github.com/saschadaemgen/SimpleGoX</span>
        </div>
        <div class="link-row">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>
            <span>simplego.dev</span>
        </div>
        <div class="link-row">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
            <span>#simplego:simplego.dev</span>
        </div>
    </div>

    <div class="divider"></div>

    <div class="legal">
        <p>License: Apache-2.0</p>
        <p>Copyright 2026 IT and More Systems, Recklinghausen</p>
    </div>

    <div class="tech">
        <span class="tech-tag">Tauri v2</span>
        <span class="tech-tag">Svelte 5</span>
        <span class="tech-tag">matrix-rust-sdk</span>
        <span class="tech-tag">tdlib-rs</span>
    </div>

    <div class="divider"></div>

    <div class="setup-section">
        <p class="setup-hint">Run the setup wizard to reconfigure your messenger connections.</p>
        <button class="setup-btn" on:click={() => dispatch('run-wizard')}>Run Setup Wizard</button>
    </div>
</div>

<style>
    .about { text-align: center; }

    .logo-area { margin-bottom: 16px; }
    .logo-text { font-size: 1.6em; font-weight: 700; letter-spacing: -0.5px; display: block; }
    .lw { color: #e6edf3; }
    .ac { color: var(--ac, #58a6ff); }
    .version { font-size: 0.82em; color: #8b949e; display: block; margin-top: 4px; }
    .season { font-size: 0.72em; color: #8b949e; display: block; margin-top: 2px; font-style: italic; }

    .desc { font-size: 0.85em; color: #b1bac4; line-height: 1.5; margin: 16px 0; }

    .tagline { display: flex; justify-content: center; gap: 8px; margin: 16px 0 8px; flex-wrap: wrap; }
    .proto-tag {
        padding: 3px 10px; border-radius: 6px; font-size: 0.72em;
        font-weight: 600; letter-spacing: 0.3px;
    }
    .proto-tag.mx { background: rgba(63,185,168,0.15); color: var(--ac, #58a6ff); }
    .proto-tag.tg { background: rgba(97,175,239,0.15); color: #61afef; }
    .proto-tag.sx { background: rgba(198,120,221,0.15); color: #c678dd; }
    .proto-tag.wa { background: rgba(152,195,121,0.15); color: #98c379; }

    .motto { font-size: 0.8em; color: #8b949e; font-style: italic; margin: 8px 0 0; }

    .divider { height: 1px; background: rgba(255,255,255,0.06); margin: 20px 0; }

    .links { text-align: left; }
    .link-row {
        display: flex; align-items: center; gap: 10px; padding: 6px 0;
        font-size: 0.82em; color: #8b949e;
    }
    .link-row svg { color: #8b949e; flex-shrink: 0; }

    .legal { font-size: 0.72em; color: #8b949e; }
    .legal p { margin: 4px 0; }

    .tech { display: flex; justify-content: center; gap: 8px; margin-top: 16px; flex-wrap: wrap; }
    .tech-tag {
        padding: 3px 10px; border-radius: 6px; font-size: 0.7em;
        background: rgba(255,255,255,0.04); color: #8b949e;
        font-family: 'JetBrains Mono', monospace;
    }

    .setup-section { margin-top: 8px; }
    .setup-hint { font-size: 0.78em; color: #8b949e; margin-bottom: 10px; }
    .setup-btn {
        padding: 8px 20px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.08);
        background: rgba(255,255,255,0.04); color: #c9d1d9;
        font-size: 0.82em; font-weight: 500; font-family: inherit; cursor: pointer;
        transition: all 0.15s;
    }
    .setup-btn:hover { background: rgba(255,255,255,0.08); }
</style>
