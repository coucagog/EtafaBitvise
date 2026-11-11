<script lang="ts">
  import type { Snippet } from 'svelte'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { Minus, Square, X } from '@lucide/svelte'
  import { app, NOM_APP } from './etat.svelte'

  let { sousTitre = '', children }: { sousTitre?: string; children?: Snippet } = $props()
  const fenetre = getCurrentWindow()
</script>

<header class="barre">
  <div class="titre" data-tauri-drag-region>
    {#if children}
      {@render children()}
      <span class="nom-app" data-tauri-drag-region>{NOM_APP}</span>
    {:else}
      <span data-tauri-drag-region>{NOM_APP}</span>
      <span class="sous" data-tauri-drag-region>— {sousTitre}</span>
    {/if}
  </div>
  {#if !app.capacites.mobile}
  <div class="boutons">
    <button aria-label="Réduire" onclick={() => fenetre.minimize()}><Minus size={14} /></button>
    <button aria-label="Agrandir" onclick={() => fenetre.toggleMaximize()}><Square size={12} /></button>
    <button aria-label="Fermer" class="fermer" onclick={() => fenetre.close()}><X size={14} /></button>
  </div>
  {/if}
</header>

<style>
  .barre { height: var(--titlebar-h); flex: 0 0 var(--titlebar-h); display: flex; align-items: stretch; border-bottom: 1px solid rgba(255, 255, 255, 0.04); }
  .titre { flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; padding-left: 12px; font-size: 12px; color: var(--text-secondary); }
  .sous { color: var(--text-muted); }
  .nom-app { margin-left: auto; padding-right: 12px; font-size: 11px; color: var(--text-muted); }
  .boutons { display: flex; }
  button { width: 46px; height: 32px; display: inline-flex; align-items: center; justify-content: center; background: transparent; border: none; color: var(--text-secondary); }
  button:hover { background: rgba(255, 255, 255, 0.06); }
  button:focus-visible { outline-offset: -2px; }
  .fermer:hover { background: var(--close-red); color: #fff; }
</style>
