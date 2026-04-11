<script>
    import { resolveMxcUrl } from '../lib/tauri.js';
    import { invoke } from '@tauri-apps/api/core';

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

    // In-memory cache for TG avatars
    const tgCache = new Map();

    $: initial = (name || '?').charAt(0).toUpperCase();
    $: fontSize = Math.round(size * 0.4);
    $: brCss = typeof borderRadius === 'number' ? `${borderRadius}px` : borderRadius;
    $: iconSize = Math.round(size * 0.35);

    let lastUri = null;
    $: if (mxcUri && mxcUri !== lastUri) {
        lastUri = mxcUri;
        resolve(mxcUri, size);
    } else if (!mxcUri && lastUri) {
        lastUri = null;
        imgUrl = null;
        loaded = false;
        errored = false;
    }

    async function resolve(uri, sz) {
        console.log('=== Avatar resolve called with:', uri);
        loaded = false;
        errored = false;

        // Telegram avatar
        if (uri && uri.startsWith('tg-file:')) {
            const fileId = parseInt(uri.replace('tg-file:', ''));
            console.log('=== Avatar: TG file detected, fileId:', fileId);
            if (isNaN(fileId) || fileId <= 0) {
                console.log('=== Avatar: TG fileId invalid, skipping');
                errored = true;
                return;
            }
            if (tgCache.has(fileId)) {
                console.log('=== Avatar: TG cache hit for', fileId);
                imgUrl = tgCache.get(fileId);
                return;
            }
            try {
                console.log('=== Avatar: TG downloading fileId', fileId);
                const dataUrl = await invoke('tg_download_avatar', { fileId });
                console.log('=== Avatar: TG download result:', dataUrl ? 'got data (' + dataUrl.length + ' chars)' : 'null');
                if (dataUrl) { tgCache.set(fileId, dataUrl); imgUrl = dataUrl; }
                else errored = true;
            } catch (e) {
                console.error('=== Avatar: TG download FAILED:', e);
                errored = true;
            }
            return;
        }

        // Matrix avatar
        const url = await resolveMxcUrl(uri, sz * 2, sz * 2);
        if (url) imgUrl = url;
        else errored = true;
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
        opacity: 0; transition: opacity 200ms ease; margin: 0; padding: 0;
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
