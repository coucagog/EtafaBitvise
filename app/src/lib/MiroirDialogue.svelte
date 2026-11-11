<script lang="ts">
  import { untrack } from 'svelte'
  import { ArrowDown, ArrowUp, FolderSync, RefreshCw, ShieldAlert } from '@lucide/svelte'
  import Boite from './Boite.svelte'
  import { Annule, appliquer, comparer, type Plan, suppressionPermise } from './miroir.svelte'
  import { type Cote, sftp, taille } from './sftp.svelte'

  let { surFermer }: { surFermer: () => void } = $props()

  let depuis = $state<Cote>(untrack(() => sftp.actif))
  let plan = $state<Plan | null>(null)
  let erreur = $state('')
  let avancement = $state(0)
  let enCours = $state(false)
  let supprimer = $state(false)
  let generation = 0

  const source = $derived(sftp.panneau(depuis))
  const cible = $derived(sftp.panneau(source.autre))
  const ajouts = $derived(plan?.copies.filter((c) => c.nouveau).length ?? 0)
  const modifies = $derived(plan?.copies.filter((c) => !c.nouveau).length ?? 0)
  const volume = $derived(plan?.copies.reduce((s, c) => s + c.taille, 0) ?? 0)
  const permise = $derived(plan ? suppressionPermise(plan) : false)
  const suppressions = $derived(supprimer && permise ? (plan?.suppressions.length ?? 0) : 0)
  const vide = $derived(!!plan && plan.copies.length === 0 && plan.dossiers.length === 0 && suppressions === 0)

  async function lancer() {
    const g = ++generation
    plan = null
    erreur = ''
    avancement = 0
    enCours = true
    supprimer = false
    try {
      const p = await comparer(depuis, source.chemin, cible.chemin, () => g !== generation, (n) => { if (g === generation) avancement = n })
      if (g === generation) plan = p
    } catch (e) {
      if (g === generation && !(e instanceof Annule)) erreur = String(e)
    }
    if (g === generation) enCours = false
  }

  $effect(() => {
    void depuis
    untrack(() => void lancer())
  })

  function fermer() {
    generation++
    surFermer()
  }

  function valider() {
    if (!plan) return
    void appliquer(plan, supprimer && permise)
    fermer()
  }
</script>

<Boite largeur={620} bordure={suppressions ? 'var(--danger-line)' : undefined} etiquette="Miroir de dossier" surAnnuler={fermer}>
  <div class="boite-entete"><FolderSync size={18} color="var(--accent-blue)" /><h3>Miroir de dossier</h3></div>
  <div class="boite-corps corps">
    <div class="segment" role="radiogroup" aria-label="Sens du miroir">
      <button role="radio" aria-checked={depuis === 'local'} onclick={() => (depuis = 'local')}><ArrowUp size={13} />Vers le serveur</button>
      <button role="radio" aria-checked={depuis === 'distant'} onclick={() => (depuis = 'distant')}><ArrowDown size={13} />Vers ce poste</button>
    </div>
    <dl class="mono selectionnable">
      <dt>Source</dt><dd>{source.chemin}</dd>
      <dt>Destination</dt><dd>{cible.chemin}</dd>
    </dl>

    {#if enCours}
      <p class="attente" role="status"><span class="tourne"><RefreshCw size={14} /></span>Comparaison… {avancement} éléments lus</p>
    {:else if erreur}
      <p class="message-erreur selectionnable" role="alert">Comparaison impossible, rien ne sera modifié : {erreur}</p>
    {:else if plan}
      <p class="bilan" role="status">
        <strong>{ajouts}</strong> nouveau{ajouts > 1 ? 'x' : ''} · <strong>{modifies}</strong> modifié{modifies > 1 ? 's' : ''} · <strong>{plan.identiques}</strong> identique{plan.identiques > 1 ? 's' : ''}
        {#if plan.copies.length} · {taille(volume)} à copier{/if}
      </p>
      {#if plan.copies.length || plan.ignores.length || suppressions}
        <ul class="apercu mono selectionnable" aria-label="Aperçu des changements">
          {#each plan.copies.slice(0, 200) as c}<li><span class="ton {c.nouveau ? 'nouveau' : 'modifie'}">{c.nouveau ? 'Nouveau' : 'Modifié'}</span>{c.relatif}</li>{/each}
          {#if suppressions}{#each plan.suppressions.slice(0, 200) as s}<li><span class="ton supprime">Supprimé</span>{s.relatif}{s.dossier ? '/' : ''}</li>{/each}{/if}
          {#each plan.ignores.slice(0, 50) as i}<li><span class="ton">Ignoré</span>{i}</li>{/each}
          {#if plan.copies.length > 200}<li>et {plan.copies.length - 200} autres copies</li>{/if}
        </ul>
      {/if}
      {#if plan.suppressions.length}
        {#if permise}
          <label class="destructif"><input type="checkbox" class="coche" bind:checked={supprimer} /><ShieldAlert size={14} color="var(--danger-text)" /><span>Supprimer les {plan.suppressions.length} éléments absents de la source, définitivement et sans corbeille</span></label>
        {:else}
          <p class="message-erreur">Suppression désactivée : la destination est la racine du système.</p>
        {/if}
      {/if}
    {/if}
  </div>
  <div class="boite-pied">
    <button class="btn btn-discret" data-focus-initial onclick={fermer}>Annuler</button>
    <button class="btn" class:btn-primaire={!suppressions} class:btn-danger={suppressions > 0} disabled={!plan || vide} onclick={valider}>
      {vide ? 'Déjà à jour' : suppressions ? `Appliquer et supprimer ${suppressions}` : 'Appliquer'}
    </button>
  </div>
</Boite>

<style>
  .corps { display: flex; flex-direction: column; gap: 12px; }
  .segment button { display: inline-flex; align-items: center; gap: 6px; }
  dl { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 2px 12px; margin: 0; font-size: 12px; }
  dt { color: var(--text-muted); }
  dd { margin: 0; overflow-wrap: anywhere; color: var(--text-primary); }
  .attente { display: flex; align-items: center; gap: 8px; margin: 0; font-size: 12px; color: var(--text-muted); }
  .bilan { margin: 0; font-size: 13px; color: var(--text-secondary); }
  .bilan strong { color: var(--text-primary); }
  .apercu { max-height: 220px; overflow: auto; margin: 0; padding: 6px 8px; list-style: none; background: var(--surface-terminal); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); font-size: 12px; }
  .apercu li { display: flex; gap: 10px; padding: 2px 0; overflow-wrap: anywhere; }
  .ton { flex: 0 0 64px; color: var(--text-muted); }
  .ton.nouveau { color: var(--ok-500); }
  .ton.modifie { color: var(--warn-500); }
  .ton.supprime { color: var(--danger-text); font-weight: 600; }
  .destructif { display: flex; align-items: flex-start; gap: 8px; padding: 8px 10px; border: 1px solid var(--danger-line); border-radius: var(--radius-sm); background: var(--danger-tint); font-size: 12px; color: var(--text-primary); }
  .tourne { display: inline-flex; animation: rotation 900ms linear infinite; }
  @keyframes rotation { to { transform: rotate(360deg); } }
</style>
