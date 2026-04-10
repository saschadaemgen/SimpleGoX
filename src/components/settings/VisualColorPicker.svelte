<script>
    import { accentColor } from '../../lib/stores.js';

    const presets = [
        '#3fb9a8', '#58a6ff', '#7c6aef', '#e0628d', '#f0883e',
        '#3fb950', '#d29922', '#db6d6d', '#79c0ff', '#b392f0',
    ];

    let hue = 0;
    let sat = 100;
    let lit = 50;
    let hex = '';
    let pickerEl;
    let dragging = false;

    // Init from current accent color
    $: parseHex($accentColor);

    function parseHex(h) {
        if (!h || h.length < 7) return;
        const r = parseInt(h.slice(1, 3), 16) / 255;
        const g = parseInt(h.slice(3, 5), 16) / 255;
        const b = parseInt(h.slice(5, 7), 16) / 255;
        const max = Math.max(r, g, b), min = Math.min(r, g, b);
        let hh = 0, ss = 0, ll = (max + min) / 2;
        if (max !== min) {
            const d = max - min;
            ss = ll > 0.5 ? d / (2 - max - min) : d / (max + min);
            if (max === r) hh = ((g - b) / d + (g < b ? 6 : 0)) / 6;
            else if (max === g) hh = ((b - r) / d + 2) / 6;
            else hh = ((r - g) / d + 4) / 6;
        }
        hue = Math.round(hh * 360);
        sat = Math.round(ss * 100);
        lit = Math.round(ll * 100);
    }

    function hslToHex(h, s, l) {
        s /= 100; l /= 100;
        const a = s * Math.min(l, 1 - l);
        const f = n => {
            const k = (n + h / 30) % 12;
            const color = l - a * Math.max(Math.min(k - 3, 9 - k, 1), -1);
            return Math.round(255 * color).toString(16).padStart(2, '0');
        };
        return `#${f(0)}${f(8)}${f(4)}`;
    }

    $: computedHex = hslToHex(hue, sat, lit);

    function apply(color) { accentColor.set(color); }
    function pick(c) { accentColor.set(c); }

    function handleField(e) {
        if (!pickerEl) return;
        const rect = pickerEl.getBoundingClientRect();
        sat = Math.round(Math.max(0, Math.min(100, (e.clientX - rect.left) / rect.width * 100)));
        lit = Math.round(Math.max(0, Math.min(100, 100 - (e.clientY - rect.top) / rect.height * 100)));
        apply(hslToHex(hue, sat, lit));
    }

    function onFieldDown(e) { dragging = true; handleField(e); }
    function onFieldMove(e) { if (dragging) handleField(e); }
    function onFieldUp() { dragging = false; }

    function handleHue(e) {
        hue = parseInt(e.target.value);
        apply(hslToHex(hue, sat, lit));
    }

    function applyHex() {
        const v = hex.startsWith('#') ? hex : '#' + hex;
        if (/^#[0-9a-fA-F]{6}$/.test(v)) apply(v);
    }
</script>

<svelte:window on:mousemove={onFieldMove} on:mouseup={onFieldUp} />

<div class="presets">
    {#each presets as c}
        <button class="dot" class:active={$accentColor === c} style="background:{c}" on:click={() => pick(c)} title={c}></button>
    {/each}
</div>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="field-wrap">
    <div class="sat-field"
         bind:this={pickerEl}
         on:mousedown={onFieldDown}
         style="background: linear-gradient(to right, #fff, hsl({hue}, 100%, 50%));">
        <div class="sat-dark"></div>
        <div class="cursor" style="left:{sat}%;top:{100-lit}%"></div>
    </div>
    <input type="range" class="hue-slider" min="0" max="360" bind:value={hue} on:input={handleHue} />
</div>

<div class="footer">
    <div class="preview" style="background:{computedHex}"></div>
    <div class="hex-group">
        <span class="hash">#</span>
        <input class="hex" bind:value={hex} placeholder={computedHex.slice(1)} maxlength="6"
               on:keydown={e => e.key === 'Enter' && applyHex()} />
    </div>
    <button class="go" on:click={applyHex}>Apply</button>
</div>

<style>
    .presets { display: grid; grid-template-columns: repeat(5, 1fr); gap: 8px; margin-bottom: 16px; }
    .dot {
        width: 100%; aspect-ratio: 1; border-radius: 10px; cursor: pointer;
        border: 2px solid transparent; transition: all 180ms ease;
    }
    .dot:hover { transform: scale(1.1); }
    .dot.active { border-color: white; box-shadow: 0 0 10px var(--ac-glow); }

    .field-wrap { margin-bottom: 12px; }

    .sat-field {
        width: 100%; height: 140px; border-radius: 8px; cursor: crosshair;
        position: relative; border: 1px solid rgba(255,255,255,0.06);
        user-select: none;
    }
    .sat-dark {
        position: absolute; inset: 0; border-radius: 7px;
        background: linear-gradient(to top, #000, transparent);
    }
    .cursor {
        width: 14px; height: 14px; border-radius: 50%;
        border: 2px solid white; box-shadow: 0 0 4px rgba(0,0,0,0.6);
        position: absolute; transform: translate(-50%, -50%);
        pointer-events: none; z-index: 1;
    }

    .hue-slider {
        width: 100%; height: 14px; margin: 10px 0 0; -webkit-appearance: none;
        border-radius: 7px; cursor: pointer;
        background: linear-gradient(to right,
            hsl(0,100%,50%), hsl(60,100%,50%), hsl(120,100%,50%),
            hsl(180,100%,50%), hsl(240,100%,50%), hsl(300,100%,50%), hsl(360,100%,50%));
    }
    .hue-slider::-webkit-slider-thumb {
        -webkit-appearance: none; width: 18px; height: 18px; border-radius: 50%;
        background: white; border: 2px solid rgba(0,0,0,0.3); cursor: pointer;
        box-shadow: 0 2px 4px rgba(0,0,0,0.2);
    }

    .footer { display: flex; align-items: center; gap: 10px; }
    .preview { width: 32px; height: 32px; border-radius: 50%; border: 2px solid rgba(255,255,255,0.1); flex-shrink: 0; }
    .hex-group {
        display: flex; align-items: center; background: #0e1117;
        border: 1px solid rgba(255,255,255,0.06); border-radius: 8px; padding: 0 10px; flex: 1;
    }
    .hash { color: #8b949e; font-size: 0.82em; }
    .hex {
        background: transparent; border: none; color: #e6edf3; font-size: 0.82em;
        font-family: 'JetBrains Mono', monospace; width: 100%; padding: 8px 4px; outline: none;
    }
    .go {
        padding: 8px 14px; border-radius: 8px; border: 1px solid var(--ac-border);
        background: var(--ac-bg); color: var(--ac); font-family: inherit;
        font-size: 0.8em; font-weight: 500; cursor: pointer; transition: all 150ms;
    }
    .go:hover { background: var(--ac); color: #0e1117; }
</style>
