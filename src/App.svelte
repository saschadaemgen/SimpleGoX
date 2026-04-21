<script>
    import { isLoggedIn, telegramConnected, simplexProfile, accentColor, settingsOpen, rooms, messages, currentRoomId, telegramChats, telegramMessages } from './lib/stores.js';
    import { tryRestore } from './lib/tauri.js';
    import ChatLayout from './components/ChatLayout.svelte';
    import SetupWizard from './components/wizard/SetupWizard.svelte';
    import SplashScreen from './components/SplashScreen.svelte';
    import StatusBanner from './components/StatusBanner.svelte';
    import { onMount } from 'svelte';

    let ready = false;
    let showWizard = false;
    let showSplash = false;
    let splashDone = false;

    // Briefing 042a Fix B: a messenger counts as "configured" when ANY of
    // Matrix login, Telegram session, or SimpleX profile is present. The
    // SimpleX branch was missing before, which caused two bugs:
    // (1) main-app condition below fell through when only SimpleX was set
    //     up - nothing rendered.
    // (2) reactive watcher flipped back to the wizard after the 2s
    //     wizardCompleted window, looping the user back to Welcome.
    $: anyMessengerConfigured = $isLoggedIn || $telegramConnected || $simplexProfile !== null;

    onMount(async () => {
        try {
            await tryRestore();
        } catch (_) {}
        // Note: at this point simplexProfile has not been hydrated yet
        // (ChatLayout.trySimplexAutoConnect does that after sx-ready).
        // Using $isLoggedIn alone here mirrors the original behaviour for
        // session restoration; the reactive watcher below handles the
        // SimpleX case once the store populates.
        if (!$isLoggedIn) {
            showWizard = true;
        } else {
            // Existing session - show splash while things load
            showSplash = true;
        }
        ready = true;
    });

    // Reactive: if all accounts disconnected, show wizard.
    // Briefing 042a Fix B: anyMessengerConfigured now includes SimpleX so
    // a freshly-finished SimpleX-only wizard does not loop back to Welcome.
    $: {
        if (ready && !anyMessengerConfigured && !showWizard && !wizardCompleted) {
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

<StatusBanner />

{#if showSplash}
    <SplashScreen on:done={onSplashDone} />
{/if}

{#if !ready}
    <!-- Briefing 042b Fix D: fill the ~1.5s tryRestore gap with a minimal
         branded backdrop instead of a black screen. No timers, no dispatch:
         unmounts cleanly when `ready` flips. -->
    <div class="startup-gate">
        <div class="startup-logo">
            <span class="w">Simple</span><span class="ac">Go</span><span class="w">X</span>
        </div>
    </div>
{:else if showWizard}
    <SetupWizard on:complete={onWizardComplete} />
{:else if anyMessengerConfigured}
    <div class="app-wrap" class:visible={!showSplash}>
        <ChatLayout on:run-wizard={onRunWizard} />
    </div>
{/if}

<style>
    .app-wrap { opacity: 0; transition: opacity 0.4s ease; }
    .app-wrap.visible { opacity: 1; }

    /* Briefing 042b Fix D: startup gate - hides the black screen while
       tryRestore() runs. Matches the SplashScreen logo styling but without
       timers or animated background. Unmounts as soon as `ready` flips. */
    .startup-gate {
        position: fixed; inset: 0;
        background: #0e1117;
        display: flex; align-items: center; justify-content: center;
        z-index: 9998;
    }
    .startup-logo {
        font-size: 48px; font-weight: 300; letter-spacing: -1px;
    }
    .startup-logo .w { color: #e6edf3; }
    .startup-logo .ac { color: var(--ac, #58a6ff); }
</style>
