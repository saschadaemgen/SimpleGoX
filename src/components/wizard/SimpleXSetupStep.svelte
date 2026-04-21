<script>
    // Briefing 042 v2 W3: compact SimpleX setup step.
    //
    // Complete redesign vs 042 v1 (which overflowed the viewport). Size
    // constraints from the approved mockup:
    //   - card padding 20px 20px 18px
    //   - inputs: height 30px, font 12px
    //   - rows: 7px vertical spacing
    //   - Tor/I2P info line: 10px mono, divider above
    //   - fits within ~520px vertical even with all fields visible + Continue
    //
    // Calls `sx_set_profile` on the sidecar. Stores the chosen SMP server in
    // localStorage key `sgx-sx-smp-server` as a stopgap (no backend consumer
    // exists yet per Briefing 042 v2 W5 Option B decision).
    //
    // Uses Svelte 4 syntax for parity with neighbouring wizard steps; the
    // briefing's suggestion to use runes is not adopted because Matrix/Telegram
    // steps still use `export let` + `createEventDispatcher` + `$:`.

    import { createEventDispatcher } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { simplexProfile, showToast } from '../../lib/stores.js';
    const dispatch = createEventDispatcher();

    export let selected = { matrix: false, telegram: false, simplex: true };

    const BIO_MAX = 160;

    let displayName = '';
    let fullName = '';
    let bio = '';
    let bioFocused = false;
    let serverChoice = localStorage.getItem('sgx-sx-smp-server') || 'smp.simplego.dev';
    // Custom stores a separate text field; not mixed with the three known
    // server ids to avoid losing the default when the user clicks Custom.
    let customServer = '';
    if (serverChoice !== 'smp.simplego.dev' && serverChoice !== 'smp8.simplex.im') {
        // Previously-saved custom value: surface it in the custom input.
        customServer = serverChoice;
        serverChoice = 'custom';
    }
    let saving = false;
    let error = '';

    $: bioOver = bio.length > BIO_MAX;
    $: trimmedCustom = customServer.trim();
    $: customValid = serverChoice !== 'custom'
        || (trimmedCustom.includes('.') && !/\s/.test(trimmedCustom) && trimmedCustom.length >= 3);
    $: effectiveServer = serverChoice === 'custom' ? trimmedCustom : serverChoice;
    $: canContinue = displayName.trim().length > 0 && !bioOver && customValid && !saving;

    // Skip is safe only if another messenger is already selected — otherwise
    // skipping leaves zero configured protocols, breaking the at-least-one
    // invariant.
    $: canSkip = (selected.matrix || selected.telegram) && !saving;

    async function handleContinue() {
        if (!canContinue) return;
        saving = true;
        error = '';
        try {
            await invoke('sx_set_profile', {
                displayName: displayName.trim(),
                fullName: fullName.trim(),
                bio: bio.trim(),
            });
            simplexProfile.set({
                display_name: displayName.trim(),
                full_name: fullName.trim(),
                bio: bio.trim(),
            });
            // W5 Option B: persist server choice to localStorage. No backend
            // RPC consumes this yet (see 042 v2 W5 decision). Future briefing
            // will add `sx_set_default_smp_server` and migrate readers.
            try {
                localStorage.setItem('sgx-sx-smp-server', effectiveServer);
            } catch (_) {}
            dispatch('next');
        } catch (e) {
            error = (e && e.toString) ? e.toString() : String(e);
            saving = false;
            showToast('SimpleX setup failed: ' + error, 'error');
        }
    }

    function onKey(e) {
        if (e.key === 'Enter' && canContinue) handleContinue();
    }
</script>

<div class="step">
    <h3 class="title">SimpleX</h3>
    <p class="intro">
        Local profile. No phone number, no account. Shared only when you accept a contact.
    </p>

    <input
        class="fld"
        type="text"
        bind:value={displayName}
        on:keydown={onKey}
        disabled={saving}
        maxlength="64"
        placeholder="Display name (required)"
        autofocus
    />

    <input
        class="fld"
        type="text"
        bind:value={fullName}
        on:keydown={onKey}
        disabled={saving}
        maxlength="128"
        placeholder="Full name (optional)"
    />

    <div class="bio-row">
        <input
            class="fld bio"
            type="text"
            bind:value={bio}
            on:focus={() => (bioFocused = true)}
            on:blur={() => (bioFocused = false)}
            on:keydown={onKey}
            disabled={saving}
            maxlength={BIO_MAX + 20}
            placeholder="Bio (optional, max {BIO_MAX})"
        />
        {#if bioFocused || bio.length > 0}
            <span class="bio-counter" class:over={bioOver}>
                {bio.length}/{BIO_MAX}
            </span>
        {/if}
    </div>

    <select class="fld" bind:value={serverChoice} disabled={saving}>
        <option value="smp.simplego.dev">smp.simplego.dev  (recommended)</option>
        <option value="smp8.simplex.im">smp8.simplex.im</option>
        <option value="custom">Custom</option>
    </select>

    {#if serverChoice === 'custom'}
        <input
            class="fld custom"
            type="text"
            bind:value={customServer}
            on:keydown={onKey}
            disabled={saving}
            placeholder="smp.example.com"
        />
        {#if customServer.trim().length > 0 && !customValid}
            <div class="inline-err">Enter a hostname with a dot, no spaces.</div>
        {/if}
    {/if}

    <hr class="div" />
    <p class="routing-info mono">Tor and I2P available in Settings → Routing</p>

    {#if error}
        <div class="err">{error}</div>
    {/if}

    <div class="actions">
        <button
            class="lnk skip"
            on:click={() => dispatch('skip')}
            disabled={!canSkip}
            title={canSkip ? '' : 'At least one messenger must be configured'}
        >
            Skip
        </button>
        <div class="right">
            <button class="btn sec" on:click={() => dispatch('back')} disabled={saving}>
                Back
            </button>
            <button class="btn pri" on:click={handleContinue} disabled={!canContinue}>
                {saving ? 'Saving...' : 'Continue'}
            </button>
        </div>
    </div>
</div>

<style>
    .step {
        max-width: 360px;
        margin: 0 auto;
        padding: 20px 20px 18px;
        color: #d4d7dd;
    }

    .title {
        font-size: 14px;
        font-weight: 500;
        margin: 0 0 4px;
        color: #d4d7dd;
    }
    .intro {
        font-size: 11px;
        line-height: 1.45;
        color: #8b8f97;
        margin: 0 0 14px;
    }

    /* --- form fields --- */
    .fld {
        width: 100%;
        box-sizing: border-box;
        height: 30px;
        padding: 0 10px;
        border-radius: 6px;
        border: 1px solid #23272e;
        background: #14171c;
        color: #d4d7dd;
        font-size: 12px;
        font-family: inherit;
        outline: none;
        margin-bottom: 7px;
        transition: border-color 150ms;
    }
    .fld:focus {
        border-color: var(--ac, #3fb9a8);
    }
    .fld::placeholder {
        color: #707379;
    }
    .fld:disabled {
        opacity: 0.6;
        cursor: progress;
    }
    select.fld {
        appearance: none;
        background-image: linear-gradient(45deg, transparent 50%, #707379 50%),
                          linear-gradient(135deg, #707379 50%, transparent 50%);
        background-position:
            calc(100% - 14px) 13px,
            calc(100% - 10px) 13px;
        background-size: 4px 4px, 4px 4px;
        background-repeat: no-repeat;
        padding-right: 24px;
    }

    .bio-row { position: relative; }
    .bio-counter {
        position: absolute;
        right: 10px; top: 50%;
        transform: translateY(-50%);
        font-size: 9px;
        color: #707379;
        pointer-events: none;
        background: #14171c;
        padding: 0 2px;
        font-family: 'JetBrains Mono', ui-monospace, Menlo, Consolas, monospace;
    }
    .bio-counter.over { color: #e06c75; }
    .bio { padding-right: 48px; }

    .custom { margin-top: 1px; }
    .inline-err {
        font-size: 10px;
        color: #e06c75;
        margin: -4px 0 7px;
    }

    /* --- divider + Tor/I2P hint --- */
    .div {
        border: none;
        border-top: 1px solid #23272e;
        margin: 4px 0 6px;
    }
    .routing-info {
        font-size: 10px;
        color: #707379;
        margin: 0 0 11px;
        padding: 6px 0 0;
    }
    .mono {
        font-family: 'JetBrains Mono', ui-monospace, Menlo, Consolas, monospace;
    }

    /* --- error --- */
    .err {
        background: rgba(224, 108, 117, 0.08);
        border: 1px solid rgba(224, 108, 117, 0.2);
        border-radius: 6px;
        padding: 6px 8px;
        margin: 0 0 8px;
        font-size: 11px;
        color: #e06c75;
    }

    /* --- actions row --- */
    .actions {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 6px;
        margin-top: 2px;
    }
    .right { display: flex; gap: 6px; }

    .btn {
        padding: 5px 14px;
        border-radius: 8px;
        border: none;
        font-size: 11px;
        font-weight: 600;
        font-family: inherit;
        cursor: pointer;
        transition: filter 0.15s, background 0.15s;
    }
    .btn.pri { background: var(--ac, #3fb9a8); color: #0e1117; }
    .btn.pri:hover:not(:disabled) { filter: brightness(1.08); }
    .btn.pri:disabled { opacity: 0.4; cursor: default; }
    .btn.sec { background: #23272e; color: #d4d7dd; }
    .btn.sec:hover:not(:disabled) { background: #2a2e35; }
    .btn.sec:disabled { opacity: 0.4; cursor: default; }

    .lnk.skip {
        background: transparent;
        border: none;
        color: #8b8f97;
        font-size: 11px;
        font-family: inherit;
        padding: 5px 2px;
        cursor: pointer;
        text-decoration: underline;
        text-underline-offset: 2px;
    }
    .lnk.skip:hover:not(:disabled) { color: #d4d7dd; }
    .lnk.skip:disabled {
        opacity: 0.35;
        cursor: default;
        text-decoration: none;
    }
</style>
