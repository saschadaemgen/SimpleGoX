<script>
    import { listen } from '@tauri-apps/api/event';
    import { onMount, onDestroy, afterUpdate } from 'svelte';

    let logs = [];
    let filter = 'all';
    let autoScroll = true;
    let logEl;
    let unlistener;
    const MAX = 500;

    onMount(async () => {
        unlistener = await listen('tor-log', (ev) => {
            logs = [...logs.slice(-(MAX - 1)), ev.payload];
        });
    });

    onDestroy(() => { if (unlistener) unlistener(); });

    afterUpdate(() => {
        if (autoScroll && logEl) logEl.scrollTop = logEl.scrollHeight;
    });

    $: filtered = filter === 'all' ? logs : logs.filter(l => l.level && l.level.toLowerCase() === filter);
</script>

<div class="log-panel">
    <div class="toolbar">
        <div class="filters">
            {#each ['all', 'info', 'warn', 'error'] as lvl}
                <button class="fbtn" class:sel={filter === lvl} on:click={() => filter = lvl}>{lvl.toUpperCase()}</button>
            {/each}
        </div>
        <div class="actions">
            <label class="ascroll"><input type="checkbox" bind:checked={autoScroll} /> Auto-scroll</label>
            <button class="cbtn" on:click={() => logs = []}>Clear</button>
        </div>
    </div>

    <div class="log-container" bind:this={logEl}>
        {#if filtered.length === 0}
            <div class="empty">
                {logs.length === 0 ? 'No Tor log entries yet. Enable Tor routing to see activity.' : `No entries match "${filter}".`}
            </div>
        {:else}
            {#each filtered as entry}
                <div class="entry" class:w={entry.level === 'WARN'} class:e={entry.level === 'ERROR'}>
                    <span class="time">{entry.time || ''}</span>
                    <span class="lvl">{entry.level || 'INFO'}</span>
                    <span class="msg">{entry.message}</span>
                </div>
            {/each}
        {/if}
    </div>
</div>

<style>
    .log-panel { display: flex; flex-direction: column; height: 100%; }

    .toolbar {
        display: flex; justify-content: space-between; align-items: center;
        padding-bottom: 8px; border-bottom: 1px solid rgba(255,255,255,0.04);
        margin-bottom: 6px; flex-shrink: 0;
    }
    .filters { display: flex; gap: 3px; }
    .fbtn {
        padding: 3px 10px; border-radius: 5px; border: 1px solid rgba(255,255,255,0.04);
        background: rgba(255,255,255,0.02); color: #8b949e; font-size: 0.7em;
        font-family: inherit; cursor: pointer; transition: all 0.15s;
    }
    .fbtn:hover { background: rgba(255,255,255,0.04); }
    .fbtn.sel { background: var(--ac-bg); color: var(--ac, #58a6ff); border-color: var(--ac-border); }

    .actions { display: flex; align-items: center; gap: 8px; }
    .ascroll { font-size: 0.68em; color: #8b949e; display: flex; align-items: center; gap: 4px; cursor: pointer; }
    .ascroll input { accent-color: var(--ac); }
    .cbtn {
        padding: 2px 8px; border-radius: 4px; border: 1px solid rgba(255,255,255,0.06);
        background: transparent; color: #8b949e; font-size: 0.68em; font-family: inherit; cursor: pointer;
    }

    .log-container {
        flex: 1; overflow-y: auto; font-family: 'JetBrains Mono', monospace;
        font-size: 0.68em; line-height: 1.6; min-height: 200px;
    }
    .empty { color: #484f58; text-align: center; padding: 32px; font-family: inherit; font-size: 1.3em; }

    .entry { display: flex; gap: 6px; padding: 1px 0; border-bottom: 1px solid rgba(255,255,255,0.02); }
    .time { color: #484f58; flex-shrink: 0; }
    .lvl { width: 38px; flex-shrink: 0; font-weight: 600; color: #58a6ff; }
    .entry.w .lvl { color: #d29922; }
    .entry.e .lvl { color: #f85149; }
    .msg { color: #8b949e; word-break: break-all; }
    .entry.e .msg { color: #f85149; }
</style>
