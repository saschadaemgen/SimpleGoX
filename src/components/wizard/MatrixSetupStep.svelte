<script>
    import { createEventDispatcher } from 'svelte';
    import { doLogin, loadRecoveryKey } from '../../lib/tauri.js';
    import { currentUserId, recoveryKey, loginError, loginLoading } from '../../lib/stores.js';
    const dispatch = createEventDispatcher();

    let sub = 'homeserver'; // homeserver, mode, credentials, done
    let homeserver = 'https://matrix.simplego.dev';
    let hsOption = 'simplego'; // simplego, matrixorg, custom
    let customHs = '';
    let username = '';
    let password = '';
    let keySaved = false;

    $: if (hsOption === 'simplego') homeserver = 'https://matrix.simplego.dev';
    $: if (hsOption === 'matrixorg') homeserver = 'https://matrix.org';
    $: if (hsOption === 'custom') homeserver = customHs;

    function goMode() { sub = 'mode'; }
    function goCredentials() { sub = 'credentials'; }

    async function submit() {
        if (!username || !password) return;
        loginError.set(null);
        await doLogin(homeserver, username, password);
        // doLogin sets isLoggedIn=true on success, but we want to stay in wizard
        // Check if login succeeded by checking currentUserId
        if ($currentUserId) {
            await loadRecoveryKey();
            sub = 'done';
        }
    }

    function copyKey() {
        if ($recoveryKey) navigator.clipboard.writeText($recoveryKey);
    }

    function onKey(e) { if (e.key === 'Enter') submit(); }
</script>

<div class="step">
    {#if sub === 'homeserver'}
        <h2 class="title">Choose your homeserver</h2>
        <p class="sub">A homeserver stores your messages and handles encryption. You can use ours or connect to any Matrix server.</p>

        <div class="options">
            <label class="opt" class:sel={hsOption === 'simplego'}>
                <input type="radio" bind:group={hsOption} value="simplego" />
                <div class="opt-body">
                    <div class="opt-name">matrix.simplego.dev <span class="rec">Recommended</span></div>
                    <div class="opt-desc">Hosted by the SimpleGoX team</div>
                </div>
            </label>
            <label class="opt" class:sel={hsOption === 'matrixorg'}>
                <input type="radio" bind:group={hsOption} value="matrixorg" />
                <div class="opt-body">
                    <div class="opt-name">matrix.org</div>
                    <div class="opt-desc">Largest public Matrix server</div>
                </div>
            </label>
            <label class="opt" class:sel={hsOption === 'custom'}>
                <input type="radio" bind:group={hsOption} value="custom" />
                <div class="opt-body">
                    <div class="opt-name">Custom homeserver</div>
                    {#if hsOption === 'custom'}
                        <input class="hs-input" type="text" bind:value={customHs} placeholder="https://matrix.example.com" />
                    {:else}
                        <div class="opt-desc">Enter your own server URL</div>
                    {/if}
                </div>
            </label>
        </div>

        <div class="actions">
            <button class="btn sec" on:click={() => dispatch('back')}>Back</button>
            <button class="btn pri" on:click={goMode} disabled={!homeserver}>Continue</button>
        </div>

    {:else if sub === 'mode'}
        <h2 class="title">Do you have an account on</h2>
        <p class="sub hs-label">{homeserver.replace('https://', '')}?</p>

        <div class="mode-cards">
            <button class="mode-card" on:click={goCredentials}>
                <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M16 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="8.5" cy="7" r="4"/><line x1="20" y1="8" x2="20" y2="14"/><line x1="23" y1="11" x2="17" y2="11"/></svg>
                <div class="mode-name">I'm new here</div>
                <div class="mode-desc">Create a new account</div>
                <span class="mode-soon">Registration coming soon</span>
            </button>
            <button class="mode-card" on:click={goCredentials}>
                <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><polyline points="10 17 15 12 10 7"/><line x1="15" y1="12" x2="3" y2="12"/></svg>
                <div class="mode-name">I have an account</div>
                <div class="mode-desc">Sign in with existing credentials</div>
            </button>
        </div>

        <div class="actions">
            <button class="btn sec" on:click={() => sub = 'homeserver'}>Back</button>
        </div>

    {:else if sub === 'credentials'}
        <h2 class="title">Sign in to Matrix</h2>
        <p class="sub hs-label">{homeserver.replace('https://', '')}</p>

        <div class="field">
            <label>USERNAME</label>
            <input type="text" bind:value={username} on:keydown={onKey} placeholder="username" autocomplete="username" autofocus />
        </div>
        <div class="field">
            <label>PASSWORD</label>
            <input type="password" bind:value={password} on:keydown={onKey} placeholder="password" autocomplete="current-password" />
        </div>

        {#if $loginError}
            <div class="err">{$loginError}</div>
        {/if}

        <div class="actions">
            <button class="btn sec" on:click={() => sub = 'mode'}>Back</button>
            <button class="btn pri" on:click={submit} disabled={$loginLoading || !username || !password}>
                {$loginLoading ? 'Connecting...' : 'Sign In'}
            </button>
        </div>

    {:else if sub === 'done'}
        <div class="done">
            <div class="check-circle">
                <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
            </div>
            <h2 class="title">Matrix Connected!</h2>
            <p class="sub">You're signed in as <strong>{$currentUserId}</strong></p>

            {#if $recoveryKey}
                <div class="rk-section">
                    <p class="rk-hint">Save your recovery key in a safe place:</p>
                    <div class="rk-box">
                        <code>{$recoveryKey}</code>
                        <button class="rk-copy" on:click={copyKey}>Copy</button>
                    </div>
                    <label class="rk-check">
                        <input type="checkbox" bind:checked={keySaved} />
                        <span>I have saved my recovery key</span>
                    </label>
                </div>
            {/if}

            <div class="actions center">
                <button class="btn pri" disabled={$recoveryKey && !keySaved} on:click={() => dispatch('next')}>Continue</button>
            </div>
        </div>
    {/if}
</div>

<style>
    .step { max-width: 440px; margin: 0 auto; padding: 0 32px; }
    .title { font-size: 1.15em; font-weight: 600; margin: 0 0 8px; }
    .sub { font-size: 0.84em; color: #8b949e; margin: 0 0 24px; line-height: 1.5; }
    .sub strong { color: var(--ac, #3fb9a8); }
    .hs-label { font-family: 'JetBrains Mono', monospace; color: #61afef; font-size: 0.78em; }

    .options { display: flex; flex-direction: column; gap: 8px; margin-bottom: 24px; }
    .opt {
        display: flex; align-items: flex-start; gap: 12px; padding: 14px 16px;
        border-radius: 12px; border: 1px solid rgba(255,255,255,0.06);
        cursor: pointer; transition: all 0.15s;
    }
    .opt:hover { border-color: rgba(255,255,255,0.12); }
    .opt.sel { border-color: var(--ac, #3fb9a8); background: var(--ac-bg); }
    .opt input[type="radio"] { margin-top: 3px; accent-color: var(--ac, #3fb9a8); }
    .opt-body { flex: 1; }
    .opt-name { font-size: 0.88em; font-weight: 600; }
    .opt-desc { font-size: 0.75em; color: #8b949e; margin-top: 2px; }
    .rec { font-size: 0.7em; color: var(--ac, #3fb9a8); font-weight: 600; margin-left: 6px; }
    .hs-input {
        width: 100%; margin-top: 6px; padding: 8px 12px; border-radius: 8px;
        border: 1px solid rgba(255,255,255,0.08); background: #0e1117; color: #c9d1d9;
        font-size: 0.82em; font-family: 'JetBrains Mono', monospace; outline: none;
    }
    .hs-input:focus { border-color: var(--ac, #3fb9a8); }

    .mode-cards { display: flex; flex-direction: column; gap: 10px; margin-bottom: 20px; }
    .mode-card {
        display: flex; flex-direction: column; align-items: center; gap: 6px;
        padding: 20px; border-radius: 12px; border: 1px solid rgba(255,255,255,0.06);
        background: rgba(255,255,255,0.02); cursor: pointer; text-align: center;
        font-family: inherit; color: inherit; transition: all 0.15s;
    }
    .mode-card:hover { border-color: var(--ac, #58a6ff); background: rgba(255,255,255,0.04); }
    .mode-card svg { color: #8b949e; margin-bottom: 4px; }
    .mode-card:hover svg { color: var(--ac, #58a6ff); }
    .mode-name { font-size: 0.92em; font-weight: 600; }
    .mode-desc { font-size: 0.78em; color: #8b949e; }
    .mode-soon { font-size: 0.68em; color: #e5c07b; font-style: italic; margin-top: 4px; }

    .field { margin-bottom: 16px; }
    .field label { display: block; font-size: 0.7em; font-weight: 600; color: #8b949e; text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 6px; }
    .field input {
        width: 100%; padding: 10px 14px; border-radius: 10px;
        border: 1px solid rgba(255,255,255,0.08); background: #0e1117;
        color: #c9d1d9; font-size: 0.9em; font-family: inherit; outline: none;
    }
    .field input:focus { border-color: var(--ac, #3fb9a8); }

    .err {
        background: rgba(248,81,73,0.08); border: 1px solid rgba(248,81,73,0.15);
        border-radius: 10px; padding: 10px 14px; margin-bottom: 16px;
        font-size: 0.8em; color: #f85149;
    }

    .done { text-align: center; }
    .check-circle {
        width: 56px; height: 56px; border-radius: 50%;
        background: rgba(152,195,121,0.15); color: #98c379;
        display: flex; align-items: center; justify-content: center;
        margin: 0 auto 16px;
        animation: pop 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
    }
    @keyframes pop { 0% { transform: scale(0); } 100% { transform: scale(1); } }

    .rk-section { text-align: left; margin: 20px 0; }
    .rk-hint { font-size: 0.82em; color: #8b949e; margin: 0 0 8px; }
    .rk-box {
        display: flex; align-items: center; gap: 10px; padding: 10px 14px;
        background: #0e1117; border: 1px solid rgba(255,255,255,0.06);
        border-radius: 8px; margin-bottom: 12px;
    }
    .rk-box code { flex: 1; font-size: 0.75em; color: var(--ac, #3fb9a8); word-break: break-all; font-family: 'JetBrains Mono', monospace; }
    .rk-copy {
        padding: 5px 12px; border-radius: 6px; border: 1px solid var(--ac-border);
        background: var(--ac-bg); color: var(--ac); font-size: 0.75em; font-weight: 500;
        cursor: pointer; font-family: inherit;
    }
    .rk-copy:hover { background: var(--ac); color: #0e1117; }

    .rk-check { display: flex; align-items: center; gap: 8px; font-size: 0.82em; color: #b1bac4; cursor: pointer; }
    .rk-check input { accent-color: var(--ac, #3fb9a8); }

    .actions { display: flex; justify-content: space-between; margin-top: 8px; }
    .actions.center { justify-content: center; }
    .btn {
        padding: 11px 28px; border-radius: 10px; border: none;
        font-size: 0.88em; font-weight: 600; font-family: inherit; cursor: pointer; transition: all 0.15s;
    }
    .btn.pri { background: var(--ac, #3fb9a8); color: #0e1117; }
    .btn.pri:hover:not(:disabled) { filter: brightness(1.1); }
    .btn.pri:disabled { opacity: 0.4; cursor: default; }
    .btn.sec { background: rgba(255,255,255,0.06); color: #c9d1d9; }
    .btn.sec:hover { background: rgba(255,255,255,0.1); }
</style>
