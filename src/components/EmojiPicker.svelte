<script>
    import { onMount, onDestroy } from 'svelte';

    export let onSelect = null;
    export let onClose = null;
    export let position = { x: 0, y: 0 };

    const categories = [
        { name: 'Frequent', emojis: ['👍','❤️','😂','🎉','😢','😮','🔥','👀','💯','✅','🙏','👏'] },
        { name: 'Smileys', emojis: ['😀','😃','😄','😁','😆','😅','🤣','😂','🙂','😊','😇','🥰','😍','🤩','😘','😗','😋','😛','😜','🤪','😝','🤑','🤗','🤭','🤫','🤔','🤐','🤨','😐','😑','😶','😏','😒','🙄','😬','😌','😔','😪','🤤','😴','😷','🤒','🤕','🤢','🤮','🥵','🥶','🥴','😵','🤯','🤠','🥳','🥸','😎','🤓','🧐','😕','😟','🙁','😮','😯','😲','😳','🥺','😦','😧','😨','😰','😥','😢','😭','😱','😖','😣','😞','😓','😩','😫','🥱','😤','😡','😠','🤬','😈','👿','💀'] },
        { name: 'Gestures', emojis: ['👋','🤚','🖐️','✋','🖖','👌','🤌','🤏','✌️','🤞','🤟','🤘','🤙','👈','👉','👆','🖕','👇','☝️','👍','👎','✊','👊','🤛','🤜','👏','🙌','👐','🤲','🤝','🙏','💪'] },
        { name: 'Hearts', emojis: ['❤️','🧡','💛','💚','💙','💜','🖤','🤍','🤎','💔','💖','💗','💓','💞','💕','💟','💝','💘'] },
        { name: 'Objects', emojis: ['🎉','🎊','🎈','🎁','🏆','🥇','🎯','🔥','⭐','🌟','✨','💫','🌈','☀️','🌙','💡','🔔','📌','📎','🔗','💻','📱','⌨️','🖥️','🔒','🔓','🛡️','⚙️','🔧','💾','📁','📊'] },
        { name: 'Symbols', emojis: ['✅','❌','⭕','❗','❓','💯','🔴','🟢','🔵','⚪','⚫','🟡','🟠','🟣','🟤','➡️','⬅️','⬆️','⬇️','↩️','🔄','ℹ️','⚠️','🚫'] },
    ];

    let activeCategory = 0;
    let searchQuery = '';
    let pickerEl;

    $: filtered = searchQuery ? categories.flatMap(c => c.emojis).filter(e => e.includes(searchQuery)) : categories[activeCategory].emojis;

    function select(emoji) { onSelect?.(emoji); onClose?.(); }
    function clickOutside(e) { if (pickerEl && !pickerEl.contains(e.target)) onClose?.(); }

    onMount(() => { setTimeout(() => document.addEventListener('click', clickOutside), 10); });
    onDestroy(() => { document.removeEventListener('click', clickOutside); });
</script>

<div class="picker" bind:this={pickerEl} style="left:{position.x}px;top:{position.y}px">
    <div class="pk-s"><input bind:value={searchQuery} placeholder="Search emoji..." /></div>
    {#if !searchQuery}
        <div class="pk-t">
            {#each categories as cat, i}
                <button class="tab" class:active={activeCategory === i} on:click={() => activeCategory = i} title={cat.name}>{cat.emojis[0]}</button>
            {/each}
        </div>
    {/if}
    <div class="pk-g">
        {#each filtered as emoji}<button class="em" on:click={() => select(emoji)}>{emoji}</button>{/each}
        {#if filtered.length === 0}<div class="none">No emoji found</div>{/if}
    </div>
</div>

<style>
    .picker { position: fixed; z-index: 300; width: 320px; max-height: 380px; background: #161b22; border: 1px solid #2a2f38; border-radius: 12px; box-shadow: 0 8px 30px rgba(0,0,0,0.4); display: flex; flex-direction: column; overflow: hidden; animation: pIn 150ms ease; }
    @keyframes pIn { from { opacity: 0; transform: scale(0.95); } to { opacity: 1; transform: scale(1); } }
    .pk-s { padding: 10px 12px 6px; }
    .pk-s input { width: 100%; padding: 8px 12px; border: 1px solid #2a2f38; background: #0e1117; color: #e6edf3; border-radius: 8px; font-size: 13px; font-family: 'DM Sans', sans-serif; box-sizing: border-box; outline: none; }
    .pk-s input:focus { border-color: var(--ac, #3fb9a8); }
    .pk-t { display: flex; gap: 2px; padding: 6px 12px; border-bottom: 1px solid #21262d; }
    .tab { width: 32px; height: 32px; border: none; background: transparent; border-radius: 6px; cursor: pointer; font-size: 16px; display: flex; align-items: center; justify-content: center; }
    .tab:hover { background: rgba(255,255,255,0.04); }
    .tab.active { background: rgba(255,255,255,0.06); }
    .pk-g { display: grid; grid-template-columns: repeat(8, 1fr); gap: 2px; padding: 8px; overflow-y: auto; flex: 1; }
    .em { width: 36px; height: 36px; border: none; background: transparent; border-radius: 6px; cursor: pointer; font-size: 20px; display: flex; align-items: center; justify-content: center; transition: background 100ms, transform 100ms; }
    .em:hover { background: rgba(255,255,255,0.06); transform: scale(1.2); }
    .none { grid-column: 1 / -1; text-align: center; color: #484f58; font-size: 13px; padding: 20px; }
</style>
