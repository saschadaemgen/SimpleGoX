<script>
    import TorStatusTab from './TorStatusTab.svelte';
    import TorRoutingTab from './TorRoutingTab.svelte';
    import TorNetworkTab from './TorNetworkTab.svelte';
    import TorLogTab from './TorLogTab.svelte';

    let activeTab = 'status';

    const tabs = [
        { id: 'status', label: 'Status' },
        { id: 'routing', label: 'Routing' },
        { id: 'network', label: 'Network' },
        { id: 'log', label: 'Log' },
    ];
</script>

<h3 class="tab-title">Tor Network</h3>

<div class="sub-tabs">
    {#each tabs as tab}
        <button class="sub-tab" class:active={activeTab === tab.id} on:click={() => activeTab = tab.id}>
            {tab.label}
        </button>
    {/each}
</div>

<div class="sub-content">
    {#if activeTab === 'status'}
        <TorStatusTab />
    {:else if activeTab === 'routing'}
        <TorRoutingTab />
    {:else if activeTab === 'network'}
        <TorNetworkTab />
    {:else if activeTab === 'log'}
        <TorLogTab />
    {/if}
</div>

<style>
    .tab-title { font-size: 1.1em; font-weight: 600; margin: 0 0 16px; }

    .sub-tabs {
        display: flex; gap: 2px; padding: 3px; border-radius: 10px;
        background: rgba(255,255,255,0.03); margin-bottom: 18px;
    }
    .sub-tab {
        flex: 1; padding: 7px 0; border: none; border-radius: 8px;
        background: transparent; color: #8b949e; font-size: 0.82em;
        font-weight: 500; font-family: inherit; cursor: pointer;
        transition: all 0.15s; text-align: center;
    }
    .sub-tab:hover { color: #c9d1d9; }
    .sub-tab.active { background: rgba(255,255,255,0.06); color: #e6edf3; font-weight: 600; }

    .sub-content { flex: 1; }
</style>
