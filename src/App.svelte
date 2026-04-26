<script>
    import { isLoggedIn, telegramConnected, simplexProfile, accentColor, settingsOpen, rooms, messages, currentRoomId, telegramChats, telegramMessages, simplexContacts, simplexMessages, startupStatus } from './lib/stores.js';
    import { tryRestore } from './lib/tauri.js';
    import { invoke } from '@tauri-apps/api/core';
    import { listen } from '@tauri-apps/api/event';
    import ChatLayout from './components/ChatLayout.svelte';
    import SetupWizard from './components/wizard/SetupWizard.svelte';
    import OrphanedContactsModal from './components/OrphanedContactsModal.svelte';
    import SplashScreen from './components/SplashScreen.svelte';
    import StartupScreen from './components/StartupScreen.svelte';
    import StatusBanner from './components/StatusBanner.svelte';
    import { onMount } from 'svelte';

    // Briefing 045 W4: wizardActive is now computed from the startup
    // sequence below, not driven by an initial-true + tryRestore race.
    // Starts null so the template only shows StartupScreen until the
    // sequence explicitly sets it true or false.
    let wizardActive = null;
    let showSplash = false;
    // Briefing 045a: top-level orphan-contact gate. Set in runStartup when
    // sx_get_profile reports no profile but sx_list_contacts returns rows.
    // The render gate shows OrphanedContactsModal above wizard / ChatLayout.
    let orphanedContactsCount = 0;

    $: anyMessengerConfigured = $isLoggedIn || $telegramConnected || $simplexProfile !== null;

    // Briefing 045 W3.3: one-time migration that removes the legacy
    // localStorage keys we used to hold contact / TG chat caches in.
    // The backend DB is now the single source of truth; any leftover
    // key is dead weight and - in the SimpleX case - the mechanism
    // behind the "ghost contacts" bug (see Briefing 045 Bug 1). Runs
    // once on every mount; after the first clean start the loop is a
    // no-op.
    function cleanupLegacyLocalStorage() {
        const legacyKeys = [
            'sgx-sx-contacts',
            // sgx-tg-chats stays for now - TG side has not yet been
            // refactored to backend-as-source-of-truth. See briefing's
            // "Out of Scope" section.
        ];
        for (const k of legacyKeys) {
            if (localStorage.getItem(k) !== null) {
                localStorage.removeItem(k);
                console.info(`[migration] removed legacy localStorage key: ${k}`);
            }
        }
    }

    // Wait for the sx-ready event the Tauri backend emits after the
    // SimpleX sidecar gRPC connection is up. 15s timeout matches
    // briefing R1/R6. Can be configured by setting
    // SGX_STARTUP_TIMEOUT_MS in the environment (read on the Rust
    // side; the frontend value here is a hard fallback).
    async function waitForSxReady(timeoutMs = 15000) {
        return new Promise((resolve, reject) => {
            let done = false;
            let unlistenFn = null;
            const timer = setTimeout(() => {
                if (done) return;
                done = true;
                if (unlistenFn) unlistenFn();
                reject(new Error('sidecar ready timeout (' + timeoutMs + 'ms)'));
            }, timeoutMs);
            listen('sx-ready', () => {
                if (done) return;
                done = true;
                clearTimeout(timer);
                if (unlistenFn) unlistenFn();
                resolve();
            }).then(fn => {
                unlistenFn = fn;
                if (done) fn();
            });
        });
    }

    async function runStartup() {
        cleanupLegacyLocalStorage();

        startupStatus.set({
            phase: 'sidecar_waiting',
            message: 'Waiting for secure sidecar...',
            detail: 'gRPC on port 50053',
            error: null,
        });

        try {
            await waitForSxReady();
        } catch (e) {
            startupStatus.set({
                phase: 'error',
                message: 'Sidecar did not come up in time',
                detail: '',
                error: String(e),
            });
            return;
        }

        // Matrix session restore: unchanged from pre-045. Runs in
        // parallel with the SimpleX profile fetch below because they
        // touch independent state (Matrix vs SimpleX sidecar).
        const matrixRestore = (async () => {
            try { await tryRestore(); } catch (_) {}
        })();

        startupStatus.set({
            phase: 'loading_profile',
            message: 'Loading your profile...',
            detail: '',
            error: null,
        });

        // Briefing 045a: strict error distinction. A failed sx_get_profile
        // RPC must not be silently treated as "no profile" - that would
        // send a returning user through wizard setup on a transient IPC
        // glitch. Any error sets phase='error' and blocks on the retry
        // screen instead.
        let profile;
        try {
            profile = await invoke('sx_get_profile');
        } catch (e) {
            console.error('[App] sx_get_profile failed:', e);
            startupStatus.set({
                phase: 'error',
                message: 'Startup failed',
                detail: '',
                error: `Failed to load SimpleX profile: ${e}`,
                source: 'profile',
            });
            return;
        }

        if (profile.has_profile) {
            // Returning user with a SimpleX profile: hydrate the profile
            // store and load the contact list normally. Contact-list
            // failure is non-fatal here (sidebar will just render empty
            // until a refresh() succeeds later).
            simplexProfile.set({
                display_name: profile.display_name,
                full_name: profile.full_name,
                bio: profile.bio,
            });

            startupStatus.set({
                phase: 'loading_contacts',
                message: 'Loading your contacts...',
                detail: '',
                error: null,
            });

            let contactCount = 0;
            try {
                const contacts = await simplexContacts.loadFromBackend();
                contactCount = contacts?.length ?? 0;
            } catch (e) {
                console.error('sx_list_contacts failed during startup:', e);
            }

            // Readable "Restoring N contacts..." beat so the phase line
            // is not a blur of three updates in 50ms on typical hardware.
            if (contactCount > 0) {
                startupStatus.set({
                    phase: 'loading_contacts',
                    message: `Restoring ${contactCount} contact${contactCount === 1 ? '' : 's'}...`,
                    detail: '',
                    error: null,
                });
                await new Promise(r => setTimeout(r, 250));
            }
        } else {
            // Briefing 045a: no profile. Check for orphaned contacts
            // BEFORE the wizard / ChatLayout decision. If any rows
            // remain in the sidecar DB, they belong to a previous
            // (now-missing) profile and are cryptographically dead
            // against any new profile. They must be wiped before the
            // user proceeds. Critically, do NOT push them into
            // simplexContacts: the orphan modal renders above the
            // ChatLayout branch precisely so the sidebar never has
            // a chance to display them as ghost rows.
            startupStatus.set({
                phase: 'loading_contacts',
                message: 'Checking SimpleX state...',
                detail: '',
                error: null,
            });

            let contacts = [];
            try {
                contacts = await invoke('sx_list_contacts');
            } catch (e) {
                console.warn('[App] sx_list_contacts during orphan check failed:', e);
                contacts = [];
            }

            if (Array.isArray(contacts) && contacts.length > 0) {
                orphanedContactsCount = contacts.length;
                console.info(`[App] orphaned contacts detected: ${orphanedContactsCount}`);
            }
        }

        // Matrix / TG path: also finish restore before unblocking.
        await matrixRestore;

        // Decide wizard vs chat. A messenger is considered configured
        // if ANY of Matrix (isLoggedIn), Telegram (cached handle), or
        // SimpleX (profile set) is present.
        wizardActive = !anyMessengerConfigured;

        // Returning user with at least one messenger: keep the Splash
        // transition for the familiar look. Fresh install / no config:
        // go straight to the Wizard with no transition. Orphan-modal
        // path: skip splash entirely - a destructive prompt should not
        // be preceded by a flash behind it.
        showSplash = !wizardActive && orphanedContactsCount === 0;

        startupStatus.set({
            phase: 'ready',
            message: '',
            detail: '',
            error: null,
        });
    }

    onMount(() => {
        runStartup();
    });

    // Reactive guard: if every messenger becomes disconnected while the
    // ChatLayout is up (e.g. user logs out from Settings), re-enter the
    // Wizard. Only fires when wizard is NOT already active.
    $: {
        if ($startupStatus.phase === 'ready' && wizardActive === false && !anyMessengerConfigured) {
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
        // Briefing 045 W3 / W6: drop all contact references. The store's
        // clear() path logs a line so the reset is auditable.
        simplexContacts.clear();
        simplexMessages.set({});
    }

    function onWizardComplete() {
        console.log('=== Wizard complete, isLoggedIn:', $isLoggedIn, 'tgConnected:', $telegramConnected);
        wizardActive = false;
        showSplash = false;
        settingsOpen.set(false); // NEVER open settings after wizard
        // Briefing 045 W5: after the wizard creates a profile, fetch
        // the contact list (likely empty on fresh setup; on a "keep
        // and continue" path it would already have been wiped by the
        // orphan modal).
        simplexContacts.loadFromBackend().catch(e => {
            console.warn('post-wizard loadFromBackend failed:', e);
        });
    }

    function onRunWizard() {
        resetToDefaults();
        wizardActive = true;
    }

    function onSplashDone() {
        showSplash = false;
    }

    function onStartupRetry() {
        // Best-effort: reset the status and re-run the sequence. The
        // sidecar may or may not recover; if not, the error state is
        // just re-entered with a fresh error string.
        // Briefing 045a: clear the orphan-count gate too so a stale
        // value from the previous attempt does not survive the retry.
        orphanedContactsCount = 0;
        startupStatus.set({
            phase: 'booting',
            message: 'Retrying startup...',
            detail: '',
            error: null,
        });
        runStartup();
    }

    // Briefing 045a: top-level orphan-cleanup confirmation. Triggered
    // by the OrphanedContactsModal's single "Wipe and continue" button.
    // After the wipe RPC succeeds, recompute wizardActive from the
    // current store state (simplexProfile is still null, so the gate
    // reflects only Matrix / Telegram). Matrix logged in -> ChatLayout;
    // nothing else configured -> wizard.
    async function onOrphansWiped() {
        try {
            await invoke('sx_wipe_all_contacts', { wizardIntent: true });
        } catch (e) {
            console.error('[App] orphan wipe failed:', e);
            startupStatus.set({
                phase: 'error',
                message: 'Startup failed',
                detail: '',
                error: `Failed to wipe orphaned contacts: ${e}`,
                source: 'orphan_wipe',
            });
            return;
        }
        console.info('[App] orphaned contacts wiped, re-evaluating wizard state');
        orphanedContactsCount = 0;
        wizardActive = !anyMessengerConfigured;
    }

    // Briefing 045 W6: clean-shutdown hook from the Tauri backend.
    // Listener registered at module init (onMount via top-level listen
    // would race the backend; we hook after the import resolves).
    listen('app-shutting-down', () => {
        console.info('[shutdown] app-shutting-down received, clearing stores');
        simplexContacts.clear();
        simplexMessages.set({});
    }).catch(e => console.warn('app-shutting-down listener registration failed:', e));
</script>

<StatusBanner />

{#if showSplash}
    <SplashScreen on:done={onSplashDone} />
{/if}

{#if $startupStatus.phase !== 'ready'}
    <StartupScreen onretry={onStartupRetry} />
{:else if orphanedContactsCount > 0}
    <OrphanedContactsModal count={orphanedContactsCount} onConfirm={onOrphansWiped} />
{:else if wizardActive}
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
