<script>
    import { accentColor } from '../lib/stores.js';

    const presets = [
        '#3fb9a8', '#58a6ff', '#7c6aef', '#e0628d', '#f0883e',
        '#3fb950', '#d29922', '#db6d6d', '#79c0ff', '#b392f0',
    ];

    let hex = '';

    function pick(c) { accentColor.set(c); }
    function apply() {
        const v = hex.startsWith('#') ? hex : '#' + hex;
        if (/^#[0-9a-fA-F]{6}$/.test(v)) accentColor.set(v);
    }
</script>

<div class="cp">
    <div class="grid">
        {#each presets as c}
            <button class="dot" class:active={$accentColor === c} style="background:{c}" on:click={() => pick(c)} title={c}></button>
        {/each}
    </div>
    <div class="row">
        <span class="hash">#</span>
        <input class="hex" bind:value={hex} placeholder="hex code" maxlength="6"
               on:keydown={e => e.key === 'Enter' && apply()} />
        <button class="go" on:click={apply}>Apply</button>
    </div>
</div>

<style>
    .grid { display: grid; grid-template-columns: repeat(5, 1fr); gap: 8px; margin-bottom: 12px; }

    .dot {
        width: 100%; aspect-ratio: 1; border-radius: 10px; cursor: pointer;
        border: 2px solid transparent; transition: all 180ms var(--ease);
    }
    .dot:hover { transform: scale(1.1); }
    .dot.active { border-color: white; box-shadow: 0 0 10px var(--ac-glow); }
    .dot.active::after {
        content: ''; position: absolute; inset: 4px; border-radius: 6px;
        border: 1px solid rgba(255, 255, 255, 0.3);
    }

    .row { display: flex; align-items: center; gap: 8px; }
    .hash { font-size: 0.8em; color: var(--text-3); }
    .hex {
        flex: 1; padding: 7px 10px; background: var(--bg); border: 1px solid var(--border-2);
        border-radius: 7px; color: var(--text); font-family: 'JetBrains Mono', monospace;
        font-size: 0.82em; outline: none;
    }
    .hex:focus { border-color: var(--ac-border); }

    .go {
        padding: 7px 14px; border-radius: 7px; border: 1px solid var(--ac-border);
        background: var(--ac-bg); color: var(--ac); font-family: 'DM Sans', sans-serif;
        font-size: 0.8em; font-weight: 500; cursor: pointer;
        transition: all 150ms var(--ease);
    }
    .go:hover { background: var(--ac); color: var(--bg); }
</style>
