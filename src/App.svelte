<script>
    import { isLoggedIn, telegramConnected, accentColor } from './lib/stores.js';
    import { tryRestore } from './lib/tauri.js';
    import ChatLayout from './components/ChatLayout.svelte';
    import SetupWizard from './components/wizard/SetupWizard.svelte';
    import { onMount } from 'svelte';

    let ready = false;
    let showWizard = false;

    onMount(async () => {
        try {
            await tryRestore();
        } catch (_) {}
        if (!$isLoggedIn) {
            showWizard = true;
        }
        ready = true;
    });

    // Reactive: if all accounts disconnected, show wizard
    $: if (ready && !$isLoggedIn && !$telegramConnected && !showWizard) {
        resetToDefaults();
        showWizard = true;
    }

    function resetToDefaults() {
        accentColor.set('#58a6ff');
    }

    function onWizardComplete() {
        showWizard = false;
    }

    function onRunWizard() {
        resetToDefaults();
        showWizard = true;
    }
</script>

{#if !ready}
    <!-- Session check in progress -->
{:else if showWizard}
    <SetupWizard on:complete={onWizardComplete} />
{:else if $isLoggedIn || $telegramConnected}
    <ChatLayout on:run-wizard={onRunWizard} />
{/if}
