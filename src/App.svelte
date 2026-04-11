<script>
    import { isLoggedIn, telegramConnected, accentColor, settingsOpen, rooms, messages, currentRoomId, telegramChats, telegramMessages } from './lib/stores.js';
    import { tryRestore } from './lib/tauri.js';
    import ChatLayout from './components/ChatLayout.svelte';
    import SetupWizard from './components/wizard/SetupWizard.svelte';
    import SplashScreen from './components/SplashScreen.svelte';
    import { onMount } from 'svelte';

    let ready = false;
    let showWizard = false;
    let showSplash = false;
    let splashDone = false;

    onMount(async () => {
        try {
            await tryRestore();
        } catch (_) {}
        if (!$isLoggedIn) {
            showWizard = true;
        } else {
            // Existing session - show splash while things load
            showSplash = true;
        }
        ready = true;
    });

    // Reactive: if all accounts disconnected, show wizard
    $: {
        if (ready && !$isLoggedIn && !$telegramConnected && !showWizard && !wizardCompleted) {
            console.log('=== ALL ACCOUNTS GONE -> showing wizard');
            showSplash = false;
            resetToDefaults();
            showWizard = true;
        }
    }

    function resetToDefaults() {
        accentColor.set('#58a6ff');
        settingsOpen.set(false);
        rooms.set([]);
        messages.set({});
        currentRoomId.set(null);
        telegramChats.set([]);
        telegramMessages.set({});
        localStorage.removeItem('sgx-tg-chats');
    }

    let wizardCompleted = false;

    function onWizardComplete() {
        console.log('=== Wizard complete, isLoggedIn:', $isLoggedIn, 'tgConnected:', $telegramConnected);
        wizardCompleted = true;
        showWizard = false;
        showSplash = false;
        settingsOpen.set(false); // NEVER open settings after wizard
        setTimeout(() => { wizardCompleted = false; }, 2000);
    }

    function onRunWizard() {
        wizardCompleted = false;
        resetToDefaults();
        showWizard = true;
    }

    function onSplashDone() {
        showSplash = false;
        splashDone = true;
    }
</script>

{#if showSplash}
    <SplashScreen on:done={onSplashDone} />
{/if}

{#if !ready}
    <!-- Session check in progress -->
{:else if showWizard}
    <SetupWizard on:complete={onWizardComplete} />
{:else if $isLoggedIn || $telegramConnected}
    <div class="app-wrap" class:visible={!showSplash}>
        <ChatLayout on:run-wizard={onRunWizard} />
    </div>
{/if}

<style>
    .app-wrap { opacity: 0; transition: opacity 0.4s ease; }
    .app-wrap.visible { opacity: 1; }
</style>
