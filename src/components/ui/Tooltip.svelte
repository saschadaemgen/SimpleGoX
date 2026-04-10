<script>
    export let text = '';

    let visible = false;
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<span class="tip-wrap"
      on:mouseenter={() => visible = true}
      on:mouseleave={() => visible = false}>
    <span class="tip-icon">?</span>
    {#if visible}
        <div class="tip">
            {text}
            <div class="arrow"></div>
        </div>
    {/if}
</span>

<style>
    .tip-wrap { display: inline-flex; position: relative; vertical-align: middle; }

    .tip-icon {
        width: 16px; height: 16px; border-radius: 50%;
        background: rgba(255,255,255,0.06); color: #8b949e;
        font-size: 10px; font-weight: 700;
        display: inline-flex; align-items: center; justify-content: center;
        cursor: help; margin-left: 6px; transition: all 0.15s;
    }
    .tip-icon:hover { background: rgba(255,255,255,0.1); color: #c9d1d9; }

    .tip {
        position: absolute; bottom: calc(100% + 8px); left: 50%; transform: translateX(-50%);
        background: #1c2128; color: #c9d1d9; font-size: 12px; line-height: 1.5;
        padding: 8px 12px; border-radius: 8px;
        border: 1px solid rgba(255,255,255,0.08);
        box-shadow: 0 8px 24px rgba(0,0,0,0.4);
        width: max-content; max-width: 240px; z-index: 500;
        pointer-events: none; animation: tipIn 0.15s ease;
        font-weight: 400; text-transform: none; letter-spacing: normal;
    }
    @keyframes tipIn { from { opacity: 0; transform: translateX(-50%) translateY(4px); } to { opacity: 1; transform: translateX(-50%) translateY(0); } }

    .arrow {
        position: absolute; width: 8px; height: 8px;
        background: #1c2128; border: 1px solid rgba(255,255,255,0.08);
        border-top: none; border-right: none;
        transform: rotate(-45deg); bottom: -5px; left: calc(50% - 4px);
    }
</style>
