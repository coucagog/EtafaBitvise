<script lang="ts">
  import { tick } from 'svelte'
  import { Ban, CircleAlert, CircleCheck, Clock, Crosshair, Inbox, ListChecks, Loader, Pause, Play, RotateCcw, SkipForward, X } from '@lucide/svelte'
  import { duree, type Ligne, sftp, taille } from './sftp.svelte'

  let { envoi }: { envoi: boolean } = $props()
  const lignes = $derived(envoi ? sftp.envois : sftp.telechargements)
  const actives = $derived(lignes.filter((l) => l.etat === 'actif' || l.etat === 'pause').length)
  const enAttente = $derived(lignes.filter((l) => l.etat === 'attente').length)
  const terminees = $derived(lignes.filter((l) => l.etat === 'fini' || l.etat === 'ignore').length)
  const erreurs = $derived(lignes.filter((l) => l.etat === 'erreur').length)
  const resume = $derived(`${actives} en cours · ${enAttente} en attente · ${terminees} terminé${terminees > 1 ? 's' : ''}${erreurs ? ` · ${erreurs} en erreur` : ''}`)
  let liste = $state<HTMLDivElement>()

  const ETATS = {
    attente: { texte: 'En attente', couleur: 'var(--text-secondary)', fond: 'var(--surface-2)', Icone: Clock },
    actif: { texte: 'En cours', couleur: 'var(--accent-blue)', fond: 'var(--accent-blue-tint)', Icone: Loader },
    pause: { texte: 'En pause', couleur: 'var(--warn-500)', fond: 'var(--warn-tint)', Icone: Pause },
    fini: { texte: 'Terminé', couleur: 'var(--ok-500)', fond: 'var(--ok-tint)', Icone: CircleCheck },
    erreur: { texte: 'Erreur', couleur: 'var(--danger-text)', fond: 'var(--danger-tint)', Icone: CircleAlert },
    annule: { texte: 'Annulé', couleur: 'var(--text-muted)', fond: 'var(--surface-1)', Icone: Ban },
    ignore: { texte: 'Ignoré', couleur: 'var(--text-muted)', fond: 'var(--surface-1)', Icone: SkipForward },
  } as const

  const barre = (l: Ligne) => (l.etat === 'erreur' ? 'var(--rec-500)' : l.etat === 'fini' ? 'var(--ok-500)' : l.etat === 'pause' ? 'var(--warn-500)' : 'var(--accent-blue)')

  const idActif = $derived(lignes.find((l) => l.etat === 'actif')?.id)
  $effect(() => {
    if (!sftp.suivre || idActif === undefined) return
    void tick().then(() => liste?.querySelector('[data-actif="true"]')?.scrollIntoView({ block: 'nearest' }))
  })
</script>

<div class="file">
  <div class="tete">
    <div class="titres">
      <strong>{envoi ? "File d'envoi" : 'File de téléchargement'}</strong>
      <span>{resume}</span>
    </div>
    <span class="vide"></span>
    <button class="btn btn-compact" class:btn-secondaire={!sftp.suivre} class:suivi={sftp.suivre} aria-pressed={sftp.suivre} title="Garder le transfert actif visible" onclick={() => (sftp.suivre = !sftp.suivre)}><Crosshair size={13} />Suivre le transfert actif</button>
    <span class="trait"></span>
    <button class="btn btn-secondaire btn-compact" onclick={() => sftp.demarrerTout(envoi)}><Play size={13} />Tout démarrer</button>
    <button class="btn btn-secondaire btn-compact" onclick={() => sftp.suspendreTout(envoi)}><Pause size={13} />Tout suspendre</button>
    <button class="btn btn-discret btn-compact" disabled={!terminees} title="Retirer les terminés" onclick={() => sftp.retirerTermines(envoi)}><ListChecks size={13} />Terminés</button>
  </div>
  <div class="colonnes grille-ligne"><span>Fichier</span><span>Progression</span><span>Débit</span><span>Restant</span><span>État</span><span>Actions</span></div>
  <div class="liste" role="grid" aria-label={envoi ? "File d'envoi" : 'File de téléchargement'} bind:this={liste}>
    {#if lignes.length === 0}
      <div class="message"><Inbox size={24} color="var(--text-muted)" /><strong>File vide</strong><span>Sélectionnez des fichiers dans Parcourir, puis lancez le transfert.</span></div>
    {/if}
    {#each lignes as l (l.id)}
      {@const e = ETATS[l.etat]}
      <div class="grille-ligne ligne" role="row" data-actif={l.etat === 'actif'}>
        <div class="fichier">
          <span class="nom coupe" title={l.source}>{l.nom}</span>
          {#if l.message}<span class="sous erreur coupe selectionnable" title={l.message}>{l.message}</span>{:else}<span class="sous coupe mono" title={l.dossier}>{l.etat === 'actif' && l.fichier && l.fichier !== l.nom ? l.fichier : `→ ${l.dossier}`}</span>{/if}
        </div>
        <div class="progres">
          <div class="piste" role="progressbar" aria-valuenow={Math.round(l.pourcent)} aria-valuemin="0" aria-valuemax="100" aria-label="Progression"><div style:width={`${l.pourcent}%`} style:background={barre(l)}></div></div>
          <div class="chiffres mono"><span>{taille(l.fait)}{l.total ? ` / ${taille(l.total)}` : ''}</span><span>{Math.round(l.pourcent)} %</span></div>
        </div>
        <span class="mono">{l.etat === 'actif' && l.debit ? `${taille(l.debit)}/s` : '—'}</span>
        <span class="mono">{l.etat === 'actif' ? duree(l.restant) : '—'}</span>
        <span class="badge" style:color={e.couleur} style:background={e.fond}><e.Icone size={11} />{e.texte}</span>
        <div class="actions">
          {#if l.etat === 'actif'}
            <button class="btn-icone" aria-label="Mettre en pause" title="Mettre en pause" onclick={() => sftp.pauseLigne(l, true)}><Pause size={13} /></button>
          {:else if l.etat === 'pause'}
            <button class="btn-icone" aria-label="Reprendre" title="Reprendre" onclick={() => sftp.pauseLigne(l, false)}><Play size={13} /></button>
          {:else if l.etat === 'erreur' || l.etat === 'annule'}
            <button class="btn-icone" aria-label="Réessayer" title="Réessayer" onclick={() => sftp.demarrerLigne(l)}><RotateCcw size={13} /></button>
          {:else if l.etat === 'attente'}
            <button class="btn-icone" aria-label="Démarrer" title="Démarrer" onclick={() => sftp.demarrerLigne(l)}><Play size={13} /></button>
          {/if}
          <button class="btn-icone" aria-label={l.etat === 'actif' || l.etat === 'pause' ? 'Annuler le transfert' : 'Retirer de la file'} title={l.etat === 'actif' || l.etat === 'pause' ? 'Annuler le transfert' : 'Retirer de la file'} onclick={() => sftp.retirer(l)}><X size={13} /></button>
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .file { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .tete { height: 48px; flex: 0 0 48px; display: flex; align-items: center; gap: 8px; padding: 0 12px 0 16px; border-bottom: 1px solid var(--border-subtle); }
  .titres { display: flex; flex-direction: column; gap: 2px; font-size: 12px; color: var(--text-muted); }
  .titres strong { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .vide { flex: 1; }
  .trait { width: 1px; height: 20px; background: var(--border-default); }
  .suivi { background: var(--accent-blue-tint); border-color: var(--accent-blue-line); color: #fff; }
  .btn:disabled { color: var(--text-disabled); background: transparent; cursor: not-allowed; }
  .grille-ligne { display: grid; grid-template-columns: minmax(0, 1fr) 200px 84px 72px 112px 76px; gap: 10px; align-items: center; padding: 0 12px 0 16px; }
  .colonnes { height: 26px; flex: 0 0 26px; background: var(--surface-sunken); border-bottom: 1px solid var(--border-subtle); font-size: 11px; color: var(--text-muted); }
  .liste { flex: 1; min-height: 0; overflow-y: auto; }
  .ligne { min-height: 44px; padding-top: 4px; padding-bottom: 4px; border-bottom: 1px solid var(--border-subtle); font-size: 12px; }
  .ligne[data-actif="true"] { background: rgba(96, 165, 250, 0.05); }
  .fichier { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .coupe { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sous { font-size: 11px; color: var(--text-muted); }
  .sous.erreur { color: var(--danger-text); }
  .mono { font-family: var(--font-mono); font-size: 11px; }
  .progres { display: flex; flex-direction: column; gap: 4px; }
  .piste { height: 4px; background: var(--surface-3); border-radius: 2px; overflow: hidden; }
  .piste div { height: 100%; }
  .chiffres { display: flex; justify-content: space-between; gap: 8px; color: var(--text-muted); }
  .badge { width: fit-content; height: 20px; padding: 0 8px; display: inline-flex; align-items: center; gap: 5px; border-radius: var(--radius-pill); font-size: 11px; white-space: nowrap; }
  .actions { display: flex; gap: 2px; justify-content: flex-end; }
  .message { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 72px 24px; font-size: 12px; color: var(--text-muted); text-align: center; }
  .message strong { font-size: 13px; font-weight: 600; color: var(--text-primary); }
</style>
