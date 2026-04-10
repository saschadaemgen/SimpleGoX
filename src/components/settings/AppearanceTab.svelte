<script>
    import VisualColorPicker from './VisualColorPicker.svelte';
    import Tooltip from '../ui/Tooltip.svelte';
</script>

<h3 class="tab-title">Appearance</h3>

<section>
    <h4 class="sec-title">Accent Color<Tooltip text="Choose your preferred accent color. Used for buttons, active elements, and highlights." /></h4>
    <VisualColorPicker />
</section>

<section>
    <h4 class="sec-title">Theme<Tooltip text="Switch between dark and light themes. Currently only dark mode is available." /></h4>
    <div class="theme-grid">
        <button class="theme-opt active">
            <div class="theme-preview dark"></div>
            <span>Dark</span>
        </button>
        <button class="theme-opt" disabled>
            <div class="theme-preview light"></div>
            <span>Light</span>
            <span class="soon">Soon</span>
        </button>
        <button class="theme-opt" disabled>
            <div class="theme-preview system"></div>
            <span>System</span>
            <span class="soon">Soon</span>
        </button>
    </div>
</section>

<style>
    .tab-title { font-size: 1.1em; font-weight: 600; margin: 0 0 20px; }
    section { margin-bottom: 24px; }
    .sec-title { font-size: 0.72em; font-weight: 600; color: #8b949e; text-transform: uppercase; letter-spacing: 1px; margin-bottom: 12px; }

    .theme-grid { display: flex; gap: 10px; }
    .theme-opt {
        flex: 1; padding: 10px; border-radius: 10px; border: 1px solid rgba(255,255,255,0.06);
        background: transparent; color: inherit; font-family: inherit; cursor: pointer;
        display: flex; flex-direction: column; align-items: center; gap: 8px;
        font-size: 0.82em; transition: all 0.15s; position: relative;
    }
    .theme-opt:hover:not(:disabled) { border-color: rgba(255,255,255,0.12); }
    .theme-opt.active { border-color: var(--ac, #3fb9a8); background: var(--ac-bg); }
    .theme-opt:disabled { opacity: 0.35; cursor: default; }

    .theme-preview { width: 100%; height: 32px; border-radius: 6px; }
    .theme-preview.dark { background: linear-gradient(135deg, #0e1117 50%, #161b22 50%); }
    .theme-preview.light { background: linear-gradient(135deg, #f0f0f0 50%, #ffffff 50%); }
    .theme-preview.system { background: linear-gradient(135deg, #0e1117 50%, #f0f0f0 50%); }

    .soon { font-size: 0.7em; color: #8b949e; font-style: italic; }
</style>
