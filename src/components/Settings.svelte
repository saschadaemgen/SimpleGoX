<script>
    import AccountsTab from './settings/AccountsTab.svelte';
    import AppearanceTab from './settings/AppearanceTab.svelte';
    import PrivacyTab from './settings/PrivacyTab.svelte';
    import NotificationsTab from './settings/NotificationsTab.svelte';
    import AboutTab from './settings/AboutTab.svelte';

    export let visible = false;
    export let onClose = null;

    let activeTab = 'accounts';

    const tabs = [
        { id: 'accounts', label: 'Accounts' },
        { id: 'appearance', label: 'Appearance' },
        { id: 'privacy', label: 'Privacy' },
        { id: 'notifications', label: 'Notifications' },
        { id: 'about', label: 'About' },
    ];

    function handleKey(e) { if (e.key === 'Escape') onClose?.(); }
    function handleOverlay(e) { if (e.target === e.currentTarget) onClose?.(); }
</script>

<svelte:window on:keydown={handleKey} />

{#if visible}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" on:click={handleOverlay}>
    <div class="panel">
        <button class="close" on:click={() => onClose?.()}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>

        <div class="layout">
            <nav class="tabs">
                {#each tabs as tab}
                    <button class="tab" class:active={activeTab === tab.id} on:click={() => activeTab = tab.id}>
                        <span class="tab-icon">
                            {#if tab.id === 'accounts'}
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>
                            {:else if tab.id === 'appearance'}
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="13.5" cy="6.5" r="2.5"/><circle cx="17.5" cy="10.5" r="2.5"/><circle cx="8.5" cy="7.5" r="2.5"/><circle cx="6.5" cy="12.5" r="2.5"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.9 0 1.7-.8 1.7-1.7 0-.4-.2-.8-.4-1.1-.2-.3-.4-.7-.4-1.1 0-.9.8-1.7 1.7-1.7H17c2.8 0 5-2.2 5-5 0-4.4-4.5-8-10-8z"/></svg>
                            {:else if tab.id === 'privacy'}
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                            {:else if tab.id === 'notifications'}
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.73 21a2 2 0 0 1-3.46 0"/></svg>
                            {:else if tab.id === 'about'}
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
                            {/if}
                        </span>
                        <span class="tab-label">{tab.label}</span>
                    </button>
                {/each}
            </nav>

            <div class="content">
                {#if activeTab === 'accounts'}
                    <AccountsTab />
                {:else if activeTab === 'appearance'}
                    <AppearanceTab />
                {:else if activeTab === 'privacy'}
                    <PrivacyTab />
                {:else if activeTab === 'notifications'}
                    <NotificationsTab />
                {:else if activeTab === 'about'}
                    <AboutTab />
                {/if}
            </div>
        </div>
    </div>
</div>
{/if}

<style>
    .overlay {
        position: fixed; inset: 0;
        background: rgba(0, 0, 0, 0.6);
        backdrop-filter: blur(8px);
        display: flex; align-items: center; justify-content: center;
        z-index: 300;
        animation: overlayIn 0.25s ease-out;
    }
    @keyframes overlayIn { from { opacity: 0; } to { opacity: 1; } }

    .panel {
        width: 720px; max-width: 90vw;
        height: 520px; max-height: 85vh;
        background: #161b22;
        border: 1px solid rgba(255, 255, 255, 0.06);
        border-radius: 16px;
        position: relative; overflow: hidden;
        box-shadow: 0 24px 48px rgba(0, 0, 0, 0.3);
        animation: panelIn 0.25s ease-out;
    }
    @keyframes panelIn {
        from { opacity: 0; transform: scale(0.95) translateY(20px); }
        to { opacity: 1; transform: scale(1) translateY(0); }
    }

    .close {
        position: absolute; top: 16px; right: 16px;
        width: 32px; height: 32px; border: none; background: transparent;
        color: #8b949e; cursor: pointer;
        display: flex; align-items: center; justify-content: center;
        border-radius: 8px; z-index: 10; transition: all 0.15s;
    }
    .close:hover { background: rgba(255, 255, 255, 0.06); color: #e6edf3; }

    .layout { display: flex; height: 100%; }

    .tabs {
        width: 160px; min-width: 160px;
        background: rgba(0, 0, 0, 0.15);
        border-right: 1px solid rgba(255, 255, 255, 0.06);
        padding: 24px 0;
        display: flex; flex-direction: column; gap: 2px;
    }

    .tab {
        display: flex; align-items: center; gap: 10px;
        padding: 10px 16px; border: none; background: transparent;
        color: #8b949e; font-size: 0.82em; font-family: inherit;
        cursor: pointer; transition: all 0.15s; text-align: left;
        border-left: 3px solid transparent;
    }
    .tab:hover { background: rgba(255, 255, 255, 0.04); color: #c9d1d9; }
    .tab.active {
        border-left-color: var(--ac, #3fb9a8);
        background: var(--ac-bg);
        color: var(--ac, #3fb9a8);
        font-weight: 600;
    }
    .tab-icon { display: flex; align-items: center; flex-shrink: 0; }

    .content {
        flex: 1; padding: 28px 32px; overflow-y: auto;
    }
</style>
