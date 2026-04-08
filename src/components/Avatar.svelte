<script>
    import { resolveMxcUrl } from '../lib/tauri.js';

    export let mxcUri = null;
    export let name = '';
    export let size = 40;
    export let borderRadius = 10;
    export let clickable = false;
    export let editable = false;
    export let onUpload = null;

    let imgUrl = null;
    let loaded = false;
    let errored = false;
    let hovering = false;

    $: initial = (name || '?').charAt(0).toUpperCase();
    $: fontSize = Math.round(size * 0.4);
    $: brCss = typeof borderRadius === 'number' ? `${borderRadius}px` : borderRadius;
    $: iconSize = Math.round(size * 0.35);

    $: if (mxcUri) {
        resolve(mxcUri, size);
    } else {
        imgUrl = null;
        loaded = false;
        errored = false;
    }

    async function resolve(uri, sz) {
        console.log('Avatar: resolving', uri);
        loaded = false;
        errored = false;
        const url = await resolveMxcUrl(uri, sz * 2, sz * 2);
        console.log('Avatar: resolved to', url);
        if (url) imgUrl = url;
        else { console.warn('Avatar: resolve returned null for', uri); errored = true; }
    }

    function onLoad() { loaded = true; }
    function onError() { console.error('Avatar: image load failed', imgUrl); errored = true; loaded = false; }
    function handleClick() { if (editable && onUpload) onUpload(); }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
    class="av"
    class:clickable={clickable || editable}
    class:editable
    style="width:{size}px;height:{size}px;border-radius:{brCss};font-size:{fontSize}px"
    on:click={handleClick}
    on:mouseenter={() => hovering = true}
    on:mouseleave={() => hovering = false}
    role={editable ? 'button' : undefined}
    tabindex={editable ? 0 : undefined}
    on:keydown={e => editable && e.key === 'Enter' && handleClick()}
>
    {#if imgUrl && !errored}
        <img src={imgUrl} alt={name} class:loaded on:load={onLoad} on:error={onError} style="border-radius:{brCss}" />
    {/if}
    {#if !loaded || errored || !imgUrl}
        <span class="ini">{initial}</span>
    {/if}
    {#if editable && hovering}
        <div class="overlay" style="border-radius:{brCss}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width={iconSize} height={iconSize}>
                <path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"/>
                <circle cx="12" cy="13" r="4"/>
            </svg>
        </div>
    {/if}
</div>

<style>
    .av {
        position: relative; display: flex; align-items: center; justify-content: center;
        background: var(--bg-raised); border: 2px solid var(--border-2);
        color: var(--ac); font-weight: 700; flex-shrink: 0; overflow: hidden;
        transition: transform 150ms ease;
    }
    .av.clickable, .av.editable { cursor: pointer; }
    .av.clickable:hover, .av.editable:hover { transform: scale(1.05); }
    img {
        position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover;
        opacity: 0; transition: opacity 200ms ease;
    }
    img.loaded { opacity: 1; }
    .ini { user-select: none; line-height: 1; }
    .overlay {
        position: absolute; inset: 0; background: rgba(0,0,0,0.55);
        display: flex; align-items: center; justify-content: center; color: white;
        animation: fadeIn 150ms ease;
    }
    @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }
</style>
