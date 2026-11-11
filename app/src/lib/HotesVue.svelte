<script lang="ts">
  import { Search, ShieldCheck, TriangleAlert, Trash2 } from '@lucide/svelte'
  import Boite from './Boite.svelte'
  import { cles, type ErreurCle, type HoteConnu } from './cles.svelte'

  let recherche = $state('')
  let aOublier = $state<HoteConnu | null>(null)
  let message = $state('')

  const visibles = $derived.by(() => {
    const motif = recherche.trim().toLowerCase()
    return cles.hotes.filter((h) => !motif || h.motif.toLowerCase().includes(motif) || h.empreinte.toLowerCase().includes(motif))
  })

  async function oublier(h: HoteConnu) {
    aOublier = null
    message = ''
    try {
      await cles.oublierHote(h)
    } catch (e) {
      message = (e as ErreurCle).message
    }
  }
</script>

<div class="vue-liste">
  <header>
    <div>
      <h2>Clés d'hôte connues</h2>
      <p>Serveurs déjà approuvés. Leur clé est vérifiée à chaque connexion.</p>
    </div>
    <div class="recherche">
      <Search size={13} color="var(--text-muted)" />
      <input class="champ" aria-label="Rechercher un hôte" bind:value={recherche} placeholder="Hôte ou empreinte" />
    </div>
  </header>

  {#if message || cles.erreur}<p class="message-erreur alerte" role="alert">{message || cles.erreur}</p>{/if}

  {#if cles.hotes.length === 0}
    <div class="vide">
      <ShieldCheck size={28} color="var(--text-muted)" />
      <h3>Aucun hôte enregistré</h3>
      <p>Choisissez « Enregistrer et accepter » à la première connexion à un serveur.</p>
    </div>
  {:else if visibles.length === 0}
    <div class="vide"><p>Aucun hôte ne correspond.</p></div>
  {:else}
    <ul>
      {#each visibles as h (h.motif + h.empreinte)}
        <li>
          <ShieldCheck size={16} color="var(--accent-blue)" />
          <div class="infos">
            <div class="titre"><strong class="mono">{h.hote}</strong><span class="port">:{h.port}</span><span class="etiquette">{h.algorithme}</span></div>
            <code class="mono selectionnable">{h.empreinte}</code>
          </div>
          <button class="btn-icone danger" aria-label={`Oublier ${h.hote}:${h.port}`} title="Oublier cette clé" onclick={() => (aOublier = h)}><Trash2 size={14} /></button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

{#if aOublier}
  {@const h = aOublier}
  <Boite largeur={440} bordure="var(--danger-line)" etiquette="Oublier cette clé d'hôte" surAnnuler={() => (aOublier = null)}>
    <div class="boite-entete"><TriangleAlert size={18} color="var(--danger-text)" /><h3>Oublier {h.hote}:{h.port} ?</h3></div>
    <div class="boite-corps">
      <p>À la prochaine connexion, ce serveur sera traité comme inconnu. Vérifiez alors son empreinte avant de l'accepter.</p>
      <code class="mono selectionnable">{h.empreinte}</code>
    </div>
    <div class="boite-pied">
      <button class="btn btn-discret" data-focus-initial onclick={() => (aOublier = null)}>Annuler</button>
      <button class="btn btn-danger" onclick={() => oublier(h)}>Oublier la clé</button>
    </div>
  </Boite>
{/if}

<style>
  .vue-liste { flex: 1; min-height: 0; overflow: auto; padding: 18px 20px; display: flex; flex-direction: column; gap: 14px; }
  header { display: flex; align-items: flex-start; justify-content: space-between; flex-wrap: wrap; gap: 12px; }
  h2 { margin: 0; font-size: 15px; font-weight: 600; }
  header p { margin: 3px 0 0; font-size: 12px; color: var(--text-muted); }
  .recherche { position: relative; display: flex; align-items: center; width: 260px; max-width: 100%; }
  .recherche :global(svg) { position: absolute; left: 9px; }
  .recherche input { width: 100%; padding-left: 28px; }
  .alerte { margin: 0; }
  .vide { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; text-align: center; padding: 24px; }
  .vide h3 { margin: 4px 0 0; font-size: 14px; }
  .vide p { margin: 0; font-size: 12px; color: var(--text-muted); }
  ul { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border-default); border-radius: var(--radius-md); overflow: hidden; }
  li { display: flex; align-items: center; gap: 12px; padding: 10px 12px; background: var(--surface-1); }
  li + li { border-top: 1px solid var(--border-subtle); }
  .infos { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .titre { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; font-size: 13px; }
  .port { margin-left: -6px; color: var(--text-muted); }
  .etiquette { height: 18px; padding: 0 6px; display: inline-flex; align-items: center; border-radius: var(--radius-pill); background: var(--surface-2); border: 1px solid var(--border-default); font-size: 10px; color: var(--text-secondary); }
  code { font-size: 11px; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
