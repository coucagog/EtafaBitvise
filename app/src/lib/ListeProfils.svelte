<script lang="ts">
  import { ChevronDown, CircleX, Plus, Search, Server, Trash2 } from '@lucide/svelte'
  import { app, type Profil } from './etat.svelte'
  import { touches } from './raccourcis'

  const groupes = $derived.by(() => {
    const q = app.recherche.trim().toLowerCase()
    const carte = new Map<string, Profil[]>()
    for (const p of app.profils) {
      if (q && !(p.nom + p.hote + p.utilisateur).toLowerCase().includes(q)) continue
      const nom = p.groupe.trim() || 'Sans groupe'
      carte.set(nom, [...(carte.get(nom) ?? []), p])
    }
    return [...carte.entries()]
  })

  const adresse = (p: Profil) => `${p.utilisateur}@${p.hote}${p.port !== 22 ? `:${p.port}` : ''}`

  function derniere(p: Profil) {
    if (!p.derniereConnexion) return 'jamais'
    return new Date(p.derniereConnexion).toLocaleDateString('fr-FR')
  }

  function touche(e: KeyboardEvent) {
    if (e.key === 'Delete' && app.courant) {
      e.preventDefault()
      app.suppression = true
    }
  }
</script>

<aside aria-label="Profils">
  <div class="outils">
    <button class="btn btn-secondaire btn-compact" title={`Nouveau profil (${touches('nouveau')})`} onclick={() => app.nouveau()}>
      <Plus size={13} />Nouveau
    </button>
    <span class="espace"></span>
    <button class="btn-icone danger" aria-label="Supprimer le profil" title="Supprimer (Suppr)" disabled={!app.courant} onclick={() => (app.suppression = true)}>
      <Trash2 size={14} />
    </button>
  </div>
  <div class="recherche">
    <span class="loupe"><Search size={13} /></span>
    <input class="champ" bind:value={app.recherche} aria-label="Rechercher un profil" placeholder="Nom, hôte ou utilisateur" />
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="liste" role="listbox" aria-label="Liste des profils" tabindex="-1" onkeydown={touche}>
    {#if app.chargement === 'chargement'}
      <div class="squelette" aria-busy="true">
        {#each [62, 48, 70, 40, 56, 66] as largeur}
          <div class="ligne-squelette"><span class="carre"></span><span class="barres"><span style:width={`${largeur}%`}></span><span style:width={`${largeur / 2}%`}></span></span></div>
        {/each}
      </div>
    {:else if app.chargement === 'erreur'}
      <div class="alerte" role="alert"><CircleX size={14} color="var(--danger-text)" />Liste indisponible</div>
    {:else if app.profils.length === 0}
      <div class="message">Aucun profil enregistré.</div>
    {:else if groupes.length === 0}
      <div class="message">Aucun profil ne correspond à « {app.recherche} ».</div>
    {:else}
      {#each groupes as [nom, profils]}
        <div class="groupe" role="group" aria-label={nom}>
          <div class="titre-groupe entete"><ChevronDown size={12} />{nom}<span class="compteur">{profils.length}</span></div>
          {#each profils as p (p.id)}
            {@const actif = p.id === app.selection}
            <button class="profil" class:actif role="option" aria-selected={actif} onclick={() => app.selectionner(p.id)}>
              <Server size={14} color={actif ? 'var(--accent-blue)' : 'var(--text-muted)'} />
              <span class="infos">
                <span class="nom">
                  {p.nom}
                  {#if app.modifies[p.id]}<span class="point" title="Modifications non enregistrées" aria-label="Modifié"></span>{/if}
                </span>
                <span class="adresse mono">{adresse(p)}</span>
              </span>
              <span class="date">{derniere(p)}</span>
            </button>
          {/each}
        </div>
      {/each}
    {/if}
  </div>
</aside>

<style>
  aside { display: flex; flex-direction: column; min-height: 0; background: var(--surface-sunken); border-right: 1px solid var(--border-default); }
  .outils { display: flex; align-items: center; gap: 2px; padding: 8px; }
  .espace { flex: 1; }
  .btn-icone:disabled { color: var(--text-disabled); cursor: not-allowed; background: transparent; }
  .recherche { padding: 0 8px 8px; position: relative; }
  .loupe { position: absolute; left: 17px; top: 7px; color: var(--text-muted); pointer-events: none; display: inline-flex; }
  .recherche .champ { height: 28px; padding: 0 8px 0 28px; }
  .liste { flex: 1; min-height: 0; overflow: auto; padding: 0 6px 8px; }
  .groupe { display: flex; flex-direction: column; gap: 1px; margin-bottom: 6px; }
  .entete { display: flex; align-items: center; gap: 6px; height: 28px; padding: 0 6px; }
  .profil { width: 100%; height: 44px; display: flex; align-items: center; gap: 10px; padding: 0 10px; border: none; border-radius: var(--radius-sm); background: transparent; text-align: left; color: var(--text-primary); }
  .profil:hover { background: var(--surface-2); }
  .profil.actif { background: var(--accent-blue-tint); box-shadow: inset 0 0 0 1px var(--accent-blue-line); }
  .infos { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .nom { display: flex; align-items: center; gap: 6px; font-size: 12px; }
  .actif .nom { font-weight: 600; }
  .point { width: 6px; height: 6px; border-radius: 50%; background: var(--text-secondary); }
  .adresse { font-size: 11px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .date { font-size: 11px; color: var(--text-muted); white-space: nowrap; }
  .message { padding: 24px 10px; font-size: 12px; color: var(--text-muted); text-align: center; }
  .alerte { margin: 4px 2px; padding: 8px 10px; display: flex; gap: 8px; background: var(--danger-tint); border: 1px solid var(--danger-line); border-radius: var(--radius-sm); font-size: 12px; }
  .squelette { display: flex; flex-direction: column; gap: 4px; padding: 4px; }
  .ligne-squelette { height: 44px; display: flex; align-items: center; gap: 10px; padding: 0 6px; }
  .carre { width: 14px; height: 14px; border-radius: 3px; background: var(--surface-2); }
  .barres { flex: 1; display: flex; flex-direction: column; gap: 6px; }
  .barres span { height: 9px; border-radius: 3px; background: var(--surface-3); }
  .barres span:last-child { height: 8px; background: var(--surface-2); }
</style>
