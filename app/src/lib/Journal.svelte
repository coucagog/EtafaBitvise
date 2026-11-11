<script lang="ts">
  import { ChevronDown, ChevronRight, CircleX, TriangleAlert } from '@lucide/svelte'
  import { app } from './etat.svelte'

  const resume = $derived.by(() => {
    const n = app.journal.length
    if (!n) return 'vide'
    const erreurs = app.journal.filter((l) => l.niveau === 'ERREUR').length
    const avertissements = app.journal.filter((l) => l.niveau === 'AVERT').length
    return [
      `${n} entrée${n > 1 ? 's' : ''}`,
      erreurs ? `${erreurs} erreur${erreurs > 1 ? 's' : ''}` : '',
      avertissements ? `${avertissements} avertissement${avertissements > 1 ? 's' : ''}` : '',
    ].filter(Boolean).join(' · ')
  })
</script>

<div class="journal">
  <button class="entete" aria-expanded={app.journalOuvert} onclick={() => (app.journalOuvert = !app.journalOuvert)}>
    {#if app.journalOuvert}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
    <strong>Journal</strong><span>{resume}</span>
  </button>
  {#if app.journalOuvert}
    <div class="lignes mono selectionnable" role="log">
      {#if app.journal.length === 0}
        <div class="vide">Aucune entrée pour cette session.</div>
      {/if}
      {#each app.journal as l}
        <div class="ligne {l.niveau}">
          <span class="heure">{l.heure}</span>
          <span class="niveau">
            {#if l.niveau === 'ERREUR'}<CircleX size={11} />{:else if l.niveau === 'AVERT'}<TriangleAlert size={11} />{/if}{l.niveau}
          </span>
          <span class="source">{l.source}</span>
          <span class="message">{l.message}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .journal { flex-shrink: 0; border-top: 1px solid var(--border-default); background: var(--surface-terminal); }
  .entete { width: 100%; height: 32px; display: flex; align-items: center; gap: 8px; padding: 0 12px; border: none; background: var(--surface-sunken); color: var(--text-secondary); font-size: 12px; text-align: left; }
  .entete:hover { color: var(--text-primary); }
  .entete:focus-visible { outline-offset: -2px; }
  .entete strong { font-weight: 600; }
  .entete span { font-size: 11px; color: var(--text-muted); }
  .lignes { height: 136px; overflow: auto; padding: 4px 0; font-size: 12px; }
  .vide { padding: 8px 12px; font-family: var(--font-sans); color: var(--text-muted); }
  .ligne { display: grid; grid-template-columns: 96px 72px 64px minmax(0, 1fr); gap: 8px; height: 24px; align-items: center; padding: 0 12px; color: var(--text-secondary); }
  .ligne.ERREUR { background: rgba(239, 68, 68, 0.07); }
  .ligne.AVERT { background: rgba(251, 191, 36, 0.06); }
  .heure { color: var(--text-muted); }
  .niveau { display: inline-flex; align-items: center; gap: 4px; color: var(--text-muted); }
  .ERREUR .niveau { color: var(--danger-text); font-weight: 600; }
  .AVERT .niveau { color: var(--warn-500); font-weight: 600; }
  .source { color: var(--text-muted); }
  .message { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-primary); }
</style>
