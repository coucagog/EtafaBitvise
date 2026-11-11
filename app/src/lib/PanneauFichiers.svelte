<script lang="ts">
  import { app } from './etat.svelte'
  import {
    ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Bookmark, BookmarkMinus, BookmarkPlus, CircleX, File, Folder, FolderOpen, FolderPlus, House, Link, Monitor, RefreshCw,
    RotateCcw, Search, SearchX, Server, X,
  } from '@lucide/svelte'
  import type { Component } from 'svelte'
  import { droits, type Colonne, type Entree, estDossier, type Panneau, sftp, taille, typeDe } from './sftp.svelte'

  let { panneau, libelle, surMenu, surNouveauDossier, surTouche }: {
    panneau: Panneau
    libelle: string
    surMenu: (e: MouseEvent, entree: Entree | null) => void
    surNouveauDossier: () => void
    surTouche: (e: KeyboardEvent) => void
  } = $props()

  const p = $derived(panneau)
  let signetsOuverts = $state(false)
  const COLONNES = $derived<[Colonne, string][]>([
    ['nom', 'Nom'], ['taille', 'Taille'], ['type', 'Type'], ['modifie', 'Modifié'], ...(p.local ? [] : [['mode', 'Permissions'] as [Colonne, string]]),
  ])
  const gabarit = $derived(p.local ? 'minmax(0, 1fr) 76px 70px 124px' : 'minmax(0, 1fr) 76px 70px 124px 86px')
  const statut = $derived.by(() => {
    if (p.etat === 'erreur') return ''
    if (p.selection.length) {
      const octets = p.selectionnees.reduce((s, e) => s + e.taille, 0)
      return `${p.selection.length} sélectionné${p.selection.length > 1 ? 's' : ''} · ${taille(octets)}`
    }
    return `${p.visibles.length} élément${p.visibles.length > 1 ? 's' : ''}`
  })

  function icone(e: Entree): Component<{ size?: number; color?: string }> {
    if (e.genre === 'dossier') return Folder as never
    return (e.genre === 'lien' || e.genre === 'lien-dossier' ? Link : File) as never
  }

  // Au doigt : un appui ouvre un dossier, ou coche un fichier ; un appui long ouvre le menu.
  function toucher(e: Entree, ev: MouseEvent) {
    if (!app.tactile) p.cliquer(e, ev.ctrlKey || ev.metaKey, ev.shiftKey)
    else if (estDossier(e) && p.selection.length === 0) void p.aller(e.chemin)
    else p.cliquer(e, true, false)
  }

  function ouvrir(e: Entree) {
    if (estDossier(e)) void p.aller(e.chemin)
    else void sftp.transferer(p.cote, [e])
  }

  function date(s: number | null) {
    if (!s) return ''
    return new Date(s * 1000).toLocaleString('fr-FR', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' })
  }

  function glisser(ev: DragEvent, e: Entree) {
    if (!p.selection.includes(e.chemin)) p.selection = [e.chemin]
    sftp.glisse = p.cote
    ev.dataTransfer?.setData('text/plain', 'mls-connect')
    if (ev.dataTransfer) ev.dataTransfer.effectAllowed = 'copy'
  }

  function survol(ev: DragEvent) {
    if (sftp.glisse !== p.autre) return
    ev.preventDefault()
    p.depot = true
  }

  function deposer(ev: DragEvent) {
    p.depot = false
    if (sftp.glisse !== p.autre) return
    ev.preventDefault()
    const origine = sftp.panneau(p.autre)
    sftp.glisse = null
    void sftp.transferer(origine.cote, origine.selectionnees)
  }

  function validerChemin(ev: KeyboardEvent) {
    if (ev.key === 'Enter') void p.aller(p.saisie.trim())
    else if (ev.key === 'Escape') p.saisie = p.chemin
  }
</script>

<svelte:window onclick={() => (signetsOuverts = false)} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="panneau" class:actif={sftp.actif === p.cote} ondragover={survol} ondragleave={() => (p.depot = false)} ondrop={deposer} onfocusin={() => (sftp.actif = p.cote)} onmousedown={() => (sftp.actif = p.cote)}>
  <div class="tete">
    {#if p.local}<Monitor size={14} color="var(--text-muted)" />{:else}<Server size={14} color="var(--text-muted)" />{/if}
    <span class="titre">{p.local ? 'Fichiers locaux' : 'Fichiers distants'}</span>
    <span class="attenue mono coupe">{libelle}</span>
    <span class="vide"></span>
    <div class="filtre">
      <Search size={13} color="var(--text-muted)" />
      <input bind:value={p.filtre} placeholder="Filtrer…" aria-label={p.local ? 'Filtrer les fichiers locaux' : 'Filtrer les fichiers distants'} />
      {#if p.filtre}<button aria-label="Effacer le filtre" onclick={() => (p.filtre = '')}><X size={12} /></button>{/if}
    </div>
  </div>

  <div class="nav">
    <button class="btn-icone" aria-label="Précédent" title="Précédent" disabled={!p.peutReculer} onclick={() => p.reculer()}><ArrowLeft size={14} /></button>
    <button class="btn-icone" aria-label="Suivant" title="Suivant" disabled={!p.peutAvancer} onclick={() => p.avancer()}><ArrowRight size={14} /></button>
    <button class="btn-icone" aria-label="Dossier parent" title="Dossier parent (Retour arrière)" disabled={p.parent === null} onclick={() => p.monter()}><ArrowUp size={14} /></button>
    <button class="btn-icone" aria-label="Actualiser" title="Actualiser (F5)" onclick={() => p.actualiser()}><RefreshCw size={14} /></button>
    <button class="btn-icone" aria-label="Dossier personnel" title="Dossier personnel" onclick={() => p.aller(p.accueil)}><House size={14} /></button>
    <button class="btn-icone" aria-label="Signets" title="Signets" aria-haspopup="menu" aria-expanded={signetsOuverts} onclick={(e) => { e.stopPropagation(); signetsOuverts = !signetsOuverts }}><Bookmark size={14} /></button>
    <input class="chemin mono" bind:value={p.saisie} onkeydown={validerChemin} aria-label={p.local ? 'Chemin local' : 'Chemin distant'} spellcheck="false" title="Modifiez le chemin puis Entrée" placeholder={p.local ? 'Lecteurs' : ''} />
    {#if signetsOuverts}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="signets" role="menu" tabindex="-1" aria-label="Signets" onclick={(e) => e.stopPropagation()}>
        {#if p.chemin}
          <button role="menuitem" class="ajout" onclick={() => { p.basculerSignet(p.chemin); signetsOuverts = false }}>
            {#if p.signets.includes(p.chemin)}<BookmarkMinus size={14} />Retirer ce dossier{:else}<BookmarkPlus size={14} />Ajouter ce dossier{/if}
          </button>
        {/if}
        {#if p.signets.length === 0}<div class="aucun-signet">Aucun signet.</div>{/if}
        {#each p.signets as s (s)}
          <div class="signet">
            <button role="menuitem" class="mono aller" title={s} onclick={() => { signetsOuverts = false; void p.aller(s) }}><Bookmark size={13} /><span class="coupe">{s}</span></button>
            <button class="retirer" aria-label={`Retirer le signet ${s}`} onclick={() => p.basculerSignet(s)}><X size={12} /></button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="entetes" role="row" style:grid-template-columns={gabarit}>
    {#each COLONNES as [id, nom]}
      <button role="columnheader" class:droite={id === 'taille'} aria-sort={p.colonne === id ? (p.croissant ? 'ascending' : 'descending') : 'none'} onclick={() => p.trier(id)}>
        {nom}{#if p.colonne === id}{#if p.croissant}<ArrowUp size={11} />{:else}<ArrowDown size={11} />{/if}{/if}
      </button>
    {/each}
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="grille" role="grid" aria-label={p.local ? 'Fichiers locaux' : 'Fichiers distants'} aria-multiselectable="true" tabindex="0" onkeydown={surTouche} oncontextmenu={(e) => surMenu(e, null)}>
    {#if p.etat === 'chargement' && !p.entrees.length}
      <div class="message" role="status"><span class="tourne"><RefreshCw size={16} /></span>Lecture du dossier…</div>
    {:else if p.etat === 'erreur'}
      <div class="message" role="alert">
        <CircleX size={24} color="var(--danger-text)" />
        <strong>Dossier inaccessible</strong>
        <span class="mono selectionnable">{p.chemin}</span>
        <code class="selectionnable">{p.erreur}</code>
        <div class="actions">
          <button class="btn btn-secondaire btn-compact" onclick={() => p.actualiser()}><RotateCcw size={13} />Réessayer</button>
          {#if p.peutReculer}<button class="btn btn-discret btn-compact" onclick={() => p.reculer()}>Revenir</button>{/if}
        </div>
      </div>
    {:else if p.entrees.length === 0}
      <div class="message">
        <FolderOpen size={24} color="var(--text-muted)" />
        <strong>Dossier vide</strong>
        <span>Glissez des fichiers {p.local ? 'distants ici pour les télécharger' : 'locaux ici pour les envoyer'}, ou créez un dossier.</span>
        <button class="btn btn-secondaire btn-compact" onclick={surNouveauDossier}><FolderPlus size={13} />Nouveau dossier</button>
      </div>
    {:else if p.visibles.length === 0}
      <div class="message">
        <SearchX size={24} color="var(--text-muted)" />
        <span>Aucun élément ne correspond à « {p.filtre} »</span>
        <button class="btn btn-discret btn-compact" onclick={() => (p.filtre = '')}>Effacer le filtre</button>
      </div>
    {:else}
      {#each p.visibles as e (e.chemin)}
        {@const Icone = icone(e)}
        <div
          class="ligne" role="row" tabindex="-1" draggable={!app.tactile} aria-selected={p.selection.includes(e.chemin)} style:grid-template-columns={gabarit}
          onclick={(ev) => toucher(e, ev)}
          ondblclick={() => ouvrir(e)}
          ondragstart={(ev) => glisser(ev, e)}
          ondragend={() => (sftp.glisse = null)}
          oncontextmenu={(ev) => { ev.stopPropagation(); if (!p.selection.includes(e.chemin)) p.selection = [e.chemin]; surMenu(ev, e) }}
          onkeydown={() => {}}
        >
          <span class="nom"><Icone size={14} color={estDossier(e) ? 'var(--accent-blue)' : 'var(--text-muted)'} /><span class="coupe">{e.nom}</span></span>
          <span class="mono droite">{estDossier(e) ? '' : taille(e.taille)}</span>
          <span class="attenue coupe">{typeDe(e)}</span>
          <span class="attenue mono">{date(e.modifie)}</span>
          {#if !p.local}<span class="attenue mono">{droits(e.mode)}</span>{/if}
        </div>
      {/each}
    {/if}
  </div>

  <div class="statut" role="status">{statut}</div>
  {#if p.depot}
    <div class="depot">{#if p.local}<ArrowDown size={22} />{:else}<ArrowUp size={22} />{/if}<strong>{p.local ? 'Télécharger ici' : 'Envoyer ici'}</strong><span class="mono">{p.chemin}</span></div>
  {/if}
</div>

<style>
  .panneau { position: relative; min-width: 0; min-height: 0; display: flex; flex-direction: column; }
  .tete { height: 34px; flex: 0 0 34px; display: flex; align-items: center; gap: 8px; padding: 0 8px 0 12px; }
  .titre { font-size: 12px; font-weight: 600; white-space: nowrap; }
  .panneau.actif .titre { color: var(--accent-blue); }
  .attenue { color: var(--text-muted); }
  .coupe { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vide { flex: 1; }
  .filtre { position: relative; width: 180px; min-width: 100px; height: 26px; padding: 0 3px 0 7px; display: flex; align-items: center; gap: 5px; background: var(--surface-field); border: 1px solid var(--border-default); border-radius: var(--radius-sm); }
  .filtre:focus-within { border-color: var(--accent-blue); }
  .filtre input { flex: 1; min-width: 0; border: none; outline: none; background: transparent; color: var(--text-primary); font-size: 12px; }
  .filtre input::placeholder { color: var(--text-muted); }
  .filtre button { width: 20px; height: 20px; padding: 0; display: inline-flex; align-items: center; justify-content: center; border: none; background: transparent; color: var(--text-secondary); }
  .nav { position: relative; height: 36px; flex: 0 0 36px; display: flex; align-items: center; gap: 2px; padding: 0 8px; border-bottom: 1px solid var(--border-subtle); }
  .signets { position: absolute; top: 100%; left: 8px; z-index: 30; width: 340px; max-width: calc(100% - 16px); max-height: 280px; overflow-y: auto; padding: 4px; background: var(--surface-popover); backdrop-filter: var(--blur-popover); border: 1px solid var(--border-default); border-radius: var(--radius-md); box-shadow: var(--shadow-popover); }
  .signets button { min-height: 30px; border: none; background: transparent; color: var(--text-primary); font-size: 12px; text-align: left; }
  .signets button:hover { background: var(--surface-2); }
  .ajout { width: 100%; padding: 0 8px; display: flex; align-items: center; gap: 8px; border-radius: var(--radius-sm); }
  .signet { display: flex; align-items: center; }
  .aller { flex: 1; min-width: 0; padding: 0 8px; display: flex; align-items: center; gap: 8px; border-radius: var(--radius-sm); }
  .retirer { flex: none; width: 30px; display: inline-flex; align-items: center; justify-content: center; border-radius: var(--radius-sm); color: var(--text-muted) !important; }
  .aucun-signet { padding: 8px; font-size: 12px; color: var(--text-muted); }
  .nav .btn-icone:disabled { color: var(--text-disabled); background: transparent; cursor: not-allowed; }
  .chemin { flex: 1; min-width: 0; height: 26px; margin-left: 4px; padding: 0 8px; background: var(--surface-field); border: 1px solid var(--border-default); border-radius: var(--radius-sm); color: var(--text-primary); font-size: 12px; outline: none; }
  .chemin:focus { border-color: var(--accent-blue); }
  .entetes { height: 26px; flex: 0 0 26px; display: grid; gap: 8px; padding: 0 8px 0 12px; align-items: center; background: var(--surface-sunken); border-bottom: 1px solid var(--border-subtle); }
  .entetes button { height: 26px; min-width: 0; padding: 0; display: inline-flex; align-items: center; gap: 4px; border: none; background: transparent; color: var(--text-muted); font-size: 11px; }
  .entetes button:hover { color: var(--text-primary); }
  .entetes .droite { justify-content: flex-end; }
  .grille { flex: 1; min-height: 0; overflow-y: auto; outline: none; }
  .grille:focus-visible { box-shadow: inset 0 0 0 1px var(--accent-blue-line); }
  .ligne { height: 28px; display: grid; gap: 8px; align-items: center; padding: 0 8px 0 12px; font-size: 12px; cursor: default; }
  .ligne:hover { background: var(--surface-1); }
  .ligne[aria-selected="true"] { background: var(--accent-blue-tint); }
  .ligne > span { min-width: 0; }
  .nom { display: flex; align-items: center; gap: 8px; }
  .droite { text-align: right; }
  .mono { font-size: 11px; }
  .message { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 56px 24px; font-size: 12px; text-align: center; color: var(--text-muted); }
  .message strong { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .message code { max-width: 420px; font-size: 11px; color: var(--danger-text); }
  .actions { display: flex; gap: 8px; }
  .tourne { display: inline-flex; animation: rotation 900ms linear infinite; }
  @keyframes rotation { to { transform: rotate(360deg); } }
  .statut { height: 24px; flex: 0 0 24px; display: flex; align-items: center; padding: 0 12px; border-top: 1px solid var(--border-subtle); font-size: 11px; color: var(--text-muted); }
  .depot { position: absolute; inset: 100px 6px 30px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; border: 1px dashed var(--accent-blue); border-radius: var(--radius-md); background: rgba(96, 165, 250, 0.1); color: var(--accent-blue); pointer-events: none; font-size: 12px; }
  .depot strong { font-size: 13px; }
</style>
