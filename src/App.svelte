<script>
    import { isLoggedIn, telegramConnected, simplexProfile, accentColor, settingsOpen, rooms, messages, currentRoomId, telegramChats, telegramMessages } from './lib/stores.js';
    import { tryRestore } from './lib/tauri.js';
    import ChatLayout from './components/ChatLayout.svelte';
    import SetupWizard from './components/wizard/SetupWizard.svelte';
    import SplashScreen from './components/SplashScreen.svelte';
    import StatusBanner from './components/StatusBanner.svelte';
    import { onMount } from 'svelte';

    // Briefing 042c: wizard visibility is now driven by an explicit flag
    // instead of the `!ready` startup-gate + `showWizard` pair. Initial
    // value `true` so the WelcomeStep paints on the first frame, with no
    // black screen and no separate logo gate. The flag is flipped to
    // `false` only by:
    //   (a) onMount, when a returning user already has a configured
    //       protocol (Matrix session restored, or TG cache present).
    //   (b) onWizardComplete, after the wizard's own state machine
    //       reaches FinalStep / single-protocol auto-complete.
    // This decoupling preserves the per-protocol ProtocolConfirmStep +
    // FinalStep flow that breaks if Wizard visibility is bound directly
    // to `anyMessengerConfigured` (the store gets set MID-wizard by
    // doLogin / sx_set_profile, which would unmount the wizard before
    // the confirm screens run).
    let wizardActive = true;
    let showSplash = false;

    // A messenger counts as "configured" when ANY of Matrix login,
    // Telegram session, or SimpleX profile is present. Used both for
    // the returning-user shortcut and for the all-accounts-gone reactive
    // guard below.
    $: anyMessengerConfigured = $isLoggedIn || $telegramConnected || $simplexProfile !== null;

    onMount(async () => {
        // Returning-user fast path 1: TG session is detected via the
        // localStorage cache that stores.js seeds telegramConnected from.
        // If we already know we are configured before tryRestore even runs,
        // skip the wizard immediately.
        if (anyMessengerConfigured) {
            wizardActive = false;
            showSplash = true;
        }
        try {
            await tryRestore();
        } catch (_) {}
        // Returning-user fast path 2: tryRestore restored a Matrix session.
        // simplexProfile is still null at this point (ChatLayout's
        // trySimplexAutoConnect hydrates it after sx-ready), but Matrix or
        // TG-cache flips anyMessengerConfigured here.
        if (anyMessengerConfigured && wizardActive) {
            wizardActive = false;
            showSplash = true;
        }
        // Otherwise wizardActive stays true and the wizard remains on
        // screen waiting for the user to configure something.
    });

    // Account disconnect from inside ChatLayout: bring the wizard back.
    // Only fires when the wizard is NOT already active, so an in-progress
    // wizard run (e.g. user has not yet clicked through ProtocolConfirm)
    // is unaffected by stores being briefly cleared.
    $: {
        if (!anyMessengerConfigured && !wizardActive) {
            console.log('=== ALL ACCOUNTS GONE -> showing wizard');
            showSplash = false;
            resetToDefaults();
            wizardActive = true;
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

    function onWizardComplete() {
        console.log('=== Wizard complete, isLoggedIn:', $isLoggedIn, 'tgConnected:', $telegramConnected);
        wizardActive = false;
        showSplash = false;
        settingsOpen.set(false); // NEVER open settings after wizard
    }

    function onRunWizard() {
        resetToDefaults();
        wizardActive = true;
    }

    function onSplashDone() {
        showSplash = false;
    }
</script>

<StatusBanner />

{#if showSplash}
    <SplashScreen on:done={onSplashDone} />
{/if}

{#if wizardActive}
    <SetupWizard on:complete={onWizardComplete} />
{:else if anyMessengerConfigured}
    <div class="app-wrap" class:visible={!showSplash}>
        <ChatLayout on:run-wizard={onRunWizard} />
    </div>
{/if}

<style>
    .app-wrap { opacity: 0; transition: opacity 0.4s ease; }
    .app-wrap.visible { opacity: 1; }
</style>
