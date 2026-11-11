<script lang="ts">
  import { untrack } from 'svelte'
  import { ArrowDown, ArrowUp, Download, FilePen, Folder, FolderPlus, FolderSync, History, EllipsisVertical, KeyRound, Pause, Play, ShieldAlert, Trash2, TriangleAlert, Upload } from '@lucide/svelte'
  import Boite from './Boite.svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { open, save } from '@tauri-apps/plugin-dialog'
  import FileTransferts from './FileTransferts.svelte'
  import MiroirDialogue from './MiroirDialogue.svelte'
  import PanneauFichiers from './PanneauFichiers.svelte'
  import { app, type Profil } from './etat.svelte'
  import { type Conflit, type Cote, droits, type Entree, estDossier, type Panneau, REGLES, sftp, type Vue } from './sftp.svelte'

  let { p }: { p: Profil } = $props()

  const compte = (n: number) => n
  const enCours = (envoi: boolean) => sftp.lignes.filter((l) => l.envoi === envoi && ['attente', 'actif', 'pause'].includes(l.etat)).length
  const VUES = $derived<[Vue, string, typeof ArrowUp, number | null][]>([
    ['parcourir', 'Parcourir', Folder as never, null],
    ['envoi', "File d'envoi", ArrowUp, compte(enCours(true))],
    ['telechargement', 'File de téléchargement', ArrowDown, compte(enCours(false))],
    ['journal', 'Journal', History as never, null],
  ])

  // Premier affichage, puis après chaque rétablissement de la session.
  $effect(() => {
    if (app.etatSession === 'ok') untrack(() => void sftp.demarrer())
  })

  $effect(() => {
    sftp.local.chargerSignets(p.id)
    sftp.distant.chargerSignets(p.id)
  })

  let miroir = $state(false)
  const miroirPossible = $derived(!!sftp.local.chemin && !!sftp.distant.chemin && sftp.local.etat === 'pret' && sftp.distant.etat === 'pret')
  let partLocale = $state(50)
  let zone = $state<HTMLDivElement>()

  function deplacer(e: MouseEvent) {
    const r = zone?.getBoundingClientRect()
    if (r) partLocale = Math.max(25, Math.min(75, ((e.clientX - r.left) / r.width) * 100))
  }

  function debutSeparateur(e: MouseEvent) {
    e.preventDefault()
    const fin = () => { removeEventListener('mousemove', deplacer); removeEventListener('mouseup', fin) }
    addEventListener('mousemove', deplacer)
    addEventListener('mouseup', fin)
  }

  function clavierSeparateur(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft') partLocale = Math.max(25, partLocale - 2)
    else if (e.key === 'ArrowRight') partLocale = Math.min(75, partLocale + 2)
  }

  // Menu contextuel
  let menu = $state<{ x: number; y: number; cote: Cote; entree: Entree | null } | null>(null)

  function ouvrirMenu(cote: Cote, e: MouseEvent, entree: Entree | null) {
    e.preventDefault()
    sftp.actif = cote
    menu = { x: Math.min(e.clientX, innerWidth - 220), y: Math.max(8, Math.min(e.clientY, innerHeight - (app.tactile ? 360 : 240))), cote, entree }
  }

  // Au doigt, ces actions restent accessibles sans dépendre de l'appui long.
  function menuSelection(e: MouseEvent) {
    e.stopPropagation()
    const entree = actif.selectionnees[0]
    if (entree) menu = { x: Math.max(8, innerWidth - 216), y: Math.max(8, innerHeight - 400), cote: sftp.actif, entree }
  }

  // Dialogues
  let saisie = $state<{ mode: 'dossier' | 'renommer'; cote: Cote; entree: Entree | null; valeur: string; erreur: string } | null>(null)
  let suppression = $state<{ cote: Cote; entrees: Entree[]; erreur: string } | null>(null)
  let permissions = $state<{ entree: Entree; mode: number; erreur: string } | null>(null)
  let tousConflits = $state(false)
  let champSaisie = $state<HTMLInputElement>()

  $effect(() => {
    if (saisie && champSaisie) {
      champSaisie.focus()
      const i = saisie.valeur.lastIndexOf('.')
      champSaisie.setSelectionRange(0, i > 0 ? i : saisie.valeur.length)
    }
  })

  $effect(() => {
    if (sftp.question) tousConflits = false
  })

  const prefixe = (c: Cote) => (c === 'local' ? 'local' : 'sftp')
  const nomValide = (n: string) => !!n && n !== '.' && n !== '..' && !/[/\\]/.test(n)

  function nouveauDossier(cote: Cote) {
    saisie = { mode: 'dossier', cote, entree: null, valeur: 'nouveau-dossier', erreur: '' }
  }

  function renommer(cote: Cote, entree: Entree) {
    saisie = { mode: 'renommer', cote, entree, valeur: entree.nom, erreur: '' }
  }

  async function validerSaisie(ev: Event) {
    ev.preventDefault()
    if (!saisie) return
    const s = saisie
    const nom = s.valeur.trim()
    if (!nomValide(nom)) {
      s.erreur = 'Ce nom contient des caractères interdits.'
      return
    }
    const panneau = sftp.panneau(s.cote)
    try {
      if (s.mode === 'dossier') await invoke(`${prefixe(s.cote)}_creer_dossier`, { chemin: panneau.joindre(nom) })
      else await invoke(`${prefixe(s.cote)}_renommer`, { ancien: s.entree!.chemin, nouveau: panneau.joindre(nom) })
      saisie = null
      await panneau.actualiser()
    } catch (e) {
      s.erreur = String(e)
    }
  }

  function demanderSuppression(cote: Cote) {
    const entrees = sftp.panneau(cote).selectionnees
    if (entrees.length) suppression = { cote, entrees: [...entrees], erreur: '' }
  }

  async function supprimer() {
    if (!suppression) return
    const s = suppression
    const panneau = sftp.panneau(s.cote)
    for (const e of s.entrees) {
      try {
        await invoke(`${prefixe(s.cote)}_supprimer`, { chemin: e.chemin })
        sftp.noter(`${e.nom} : supprimé`)
      } catch (err) {
        s.erreur = `${e.nom} : ${String(err)}`
        sftp.noter(s.erreur, true)
        await panneau.actualiser()
        return
      }
    }
    suppression = null
    panneau.selection = []
    await panneau.actualiser()
  }

  async function appliquerPermissions() {
    if (!permissions) return
    const d = permissions
    try {
      await invoke('sftp_permissions', { chemin: d.entree.chemin, mode: d.mode })
      permissions = null
      await sftp.distant.actualiser()
    } catch (e) {
      d.erreur = String(e)
    }
  }

  function basculerBit(bit: number) {
    if (permissions) permissions.mode ^= bit
  }

  function surTouche(panneau: Panneau, e: KeyboardEvent) {
    const selection = panneau.selectionnees
    if (e.key === 'F5') panneau.actualiser()
    else if (e.key === 'Backspace') panneau.monter()
    else if (e.key === 'Delete') demanderSuppression(panneau.cote)
    else if (e.key === 'F2' && selection.length === 1) renommer(panneau.cote, selection[0])
    else if (e.key === 'Enter' && selection.length === 1 && estDossier(selection[0])) void panneau.aller(selection[0].chemin)
    else if (e.key === 'a' && e.ctrlKey) panneau.toutSelectionner()
    else return
    e.preventDefault()
  }

  const CHOIX: [Conflit, string][] = [['ecraser', 'Écraser'], ['renommer', 'Renommer'], ['ignorer', 'Ignorer']]
  const actif = $derived(sftp.panneau(sftp.actif))
  const peutTransferer = $derived(actif.selection.length > 0)
  const peutExporter = $derived(actif.selectionnees.length === 1 && !estDossier(actif.selectionnees[0]))

  // Android : le panneau local est le dossier privé de l'application, les échanges avec l'appareil passent par le sélecteur système.
  async function depuisAppareil() {
    const uris = await open({ multiple: true, directory: false, title: 'Importer des fichiers' }).catch(() => null)
    if (!uris || uris.length === 0) return
    try {
      const n = await invoke<number>('appareil_importer', { uris, dossier: sftp.local.chemin })
      sftp.noter(`${n} fichier${n > 1 ? 's' : ''} importé${n > 1 ? 's' : ''} de l'appareil`)
      await sftp.local.aller(sftp.local.chemin)
    } catch (e) {
      sftp.noter(`Import impossible — ${e}`, true)
    }
  }

  async function versAppareil() {
    const e = actif.selectionnees[0]
    const uri = await save({ title: 'Exporter le fichier', defaultPath: e.nom }).catch(() => null)
    if (!uri) return
    try {
      await invoke('appareil_exporter', { source: e.chemin, uri })
      sftp.noter(`${e.nom} : exporté vers l'appareil`)
    } catch (err) {
      sftp.noter(`${e.nom} — export impossible : ${err}`, true)
    }
  }
  const ClasseDroits = ['Propriétaire', 'Groupe', 'Autres']
  const BITS: [number, string][] = [[4, 'Lecture'], [2, 'Écriture'], [1, 'Exécution']]
</script>

<svelte:window onclick={() => (menu = null)} />

<div class="sftp">
  <div class="vues" role="tablist" aria-label="Vues SFTP">
    {#each VUES as [id, libelle, Icone, n]}
      <button role="tab" aria-selected={sftp.vue === id} class:choisie={sftp.vue === id} onclick={() => (sftp.vue = id)}>
        <Icone size={14} />{libelle}{#if n !== null}<span class="compteur">{n}</span>{/if}
      </button>
    {/each}
  </div>

  {#if sftp.vue === 'parcourir'}
    {#if app.etroit}
      <div class="cote" role="radiogroup" aria-label="Panneau affiché">
        <div class="segment">
          <button role="radio" aria-checked={sftp.actif === 'local'} onclick={() => (sftp.actif = 'local')}>Local</button>
          <button role="radio" aria-checked={sftp.actif === 'distant'} onclick={() => (sftp.actif = 'distant')}>Distant</button>
        </div>
      </div>
    {/if}
    <div class="panneaux" bind:this={zone} style:grid-template-columns={app.etroit ? 'minmax(0, 1fr)' : `minmax(0, ${partLocale}fr) 7px minmax(0, ${100 - partLocale}fr)`}>
      {#if !app.etroit || sftp.actif === 'local'}
      <PanneauFichiers panneau={sftp.local} libelle="Ce poste" surMenu={(e, x) => ouvrirMenu('local', e, x)} surNouveauDossier={() => nouveauDossier('local')} surTouche={(e) => surTouche(sftp.local, e)} />
      {/if}
      {#if !app.etroit}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div class="separateur" role="separator" aria-orientation="vertical" aria-label="Redimensionner les panneaux" aria-valuemin="25" aria-valuemax="75" aria-valuenow={Math.round(partLocale)} tabindex="0" title="Glisser pour redimensionner · double-clic : 50/50" onmousedown={debutSeparateur} onkeydown={clavierSeparateur} ondblclick={() => (partLocale = 50)}><span></span></div>
      {/if}
      {#if !app.etroit || sftp.actif === 'distant'}
      <PanneauFichiers panneau={sftp.distant} libelle={`${p.utilisateur}@${p.hote}`} surMenu={(e, x) => ouvrirMenu('distant', e, x)} surNouveauDossier={() => nouveauDossier('distant')} surTouche={(e) => surTouche(sftp.distant, e)} />
      {/if}
    </div>
  {:else if sftp.vue === 'envoi'}
    <FileTransferts envoi={true} />
  {:else if sftp.vue === 'telechargement'}
    <FileTransferts envoi={false} />
  {:else}
    <div class="journal" role="log" aria-label="Journal SFTP">
      {#if sftp.journal.length === 0}<div class="journal-vide">Aucun évènement pour l'instant.</div>{/if}
      {#each sftp.journal as l}
        <div class="evenement mono selectionnable" class:erreur={l.erreur}><span>{l.heure}</span>{l.message}</div>
      {/each}
    </div>
  {/if}

  <div class="bas">
    <button class="btn btn-primaire btn-compact" aria-disabled={!peutTransferer} onclick={() => peutTransferer && sftp.transferer(sftp.actif, [...actif.selectionnees])}>
      {#if sftp.actif === 'local'}<Upload size={13} />Envoyer{:else}<Download size={13} />Télécharger{/if}
      {#if actif.selection.length}({actif.selection.length}){/if}
    </button>
    {#if app.tactile && actif.selection.length}<button class="btn btn-secondaire btn-compact" onclick={menuSelection}><EllipsisVertical size={13} />Actions</button>{/if}
    {#if app.capacites.mobile && sftp.actif === 'local'}
      <button class="btn btn-secondaire btn-compact" onclick={depuisAppareil}>Importer…</button>
      <button class="btn btn-secondaire btn-compact" disabled={!peutExporter} onclick={versAppareil}>Exporter…</button>
    {/if}
    <button class="btn btn-secondaire btn-compact" disabled={!miroirPossible} title="Aligner un dossier sur l'autre, avec aperçu" onclick={() => (miroir = true)}><FolderSync size={13} />Miroir…</button>
    <label class="option"><button class="interrupteur" role="switch" aria-checked={sftp.auto} aria-label="Démarrage automatique" onclick={() => { sftp.auto = !sftp.auto; sftp.pomper() }}></button>Démarrage auto</label>
    <label class="option">Si le fichier existe
      <select bind:value={sftp.regle} aria-label="Règle si le fichier existe">{#each REGLES as [id, libelle]}<option value={id}>{libelle}</option>{/each}</select>
    </label>
    <span class="vide"></span>
    <button class="btn btn-compact" class:btn-secondaire={!sftp.pauseGlobale} class:en-pause={sftp.pauseGlobale} aria-pressed={sftp.pauseGlobale} title="Pause ou reprise des transferts (Ctrl+Shift+F8)" onclick={() => sftp.basculerPause()}>
      {#if sftp.pauseGlobale}<Play size={13} />Reprendre{:else}<Pause size={13} />Pause{/if}<span class="kbd">Ctrl+Shift+F8</span>
    </button>
  </div>

  {#if menu}
    {@const m = menu}
    {@const panneau = sftp.panneau(m.cote)}
    <div class="menu" role="menu" style:left={`${m.x}px`} style:top={`${m.y}px`}>
      {#if m.entree}
        <button role="menuitem" onclick={() => sftp.transferer(m.cote, [...panneau.selectionnees])}>{#if m.cote === 'local'}<Upload size={13} />Envoyer{:else}<Download size={13} />Télécharger{/if}</button>
        {#if panneau.selection.length === 1}
          <button role="menuitem" onclick={() => renommer(m.cote, m.entree!)}><FilePen size={13} />Renommer<small>F2</small></button>
          {#if m.cote === 'distant'}<button role="menuitem" onclick={() => (permissions = { entree: m.entree!, mode: m.entree!.mode ?? 0o644, erreur: '' })}><KeyRound size={13} />Permissions…</button>{/if}
        {/if}
        <button role="menuitem" class="danger" onclick={() => demanderSuppression(m.cote)}><Trash2 size={13} />Supprimer<small>Suppr</small></button>
        <span class="filet"></span>
      {/if}
      <button role="menuitem" onclick={() => nouveauDossier(m.cote)}><FolderPlus size={13} />Nouveau dossier</button>
    </div>
  {/if}
</div>

{#if miroir}<MiroirDialogue surFermer={() => (miroir = false)} />{/if}

{#if sftp.question}
  {@const q = sftp.question}
  <Boite largeur={480} bordure="var(--warn-line)" etiquette="Ce nom existe déjà" surAnnuler={() => sftp.repondre('annuler')}>
    <div class="boite-entete"><TriangleAlert size={18} color="var(--warn-500)" /><h3>Ce nom existe déjà</h3></div>
    <div class="boite-corps">
      <p><span class="mono">{q.nom}</span> existe déjà {q.envoi ? 'sur le serveur' : 'sur ce poste'}.</p>
      {#if q.plusieurs}<label class="option tous"><input type="checkbox" class="coche" bind:checked={tousConflits} />Appliquer à tous les éléments suivants</label>{/if}
    </div>
    <div class="boite-pied">
      <button class="btn btn-discret" data-focus-initial onclick={() => sftp.repondre('annuler')}>Annuler</button>
      {#if !q.dossier}<button class="btn btn-secondaire" onclick={() => sftp.repondre('reprendre', tousConflits)}>Reprendre</button>{/if}
      {#each CHOIX as [id, libelle]}
        <button class="btn" class:btn-secondaire={id !== 'ecraser'} class:btn-danger={id === 'ecraser'} onclick={() => sftp.repondre(id, tousConflits)}>{libelle}</button>
      {/each}
    </div>
  </Boite>
{/if}

{#if saisie}
  {@const s = saisie}
  <Boite largeur={420} etiquette={s.mode === 'dossier' ? 'Nouveau dossier' : 'Renommer'} surAnnuler={() => (saisie = null)}>
    <form onsubmit={validerSaisie}>
      <div class="boite-entete"><h3>{s.mode === 'dossier' ? 'Nouveau dossier' : `Renommer ${s.entree?.nom}`}</h3></div>
      <div class="boite-corps">
        <input class="champ mono" bind:this={champSaisie} bind:value={s.valeur} aria-label="Nom" spellcheck="false" />
        {#if s.erreur}<span class="message-erreur selectionnable">{s.erreur}</span>{/if}
      </div>
      <div class="boite-pied">
        <button type="button" class="btn btn-discret" onclick={() => (saisie = null)}>Annuler</button>
        <button type="submit" class="btn btn-primaire">{s.mode === 'dossier' ? 'Créer' : 'Renommer'}</button>
      </div>
    </form>
  </Boite>
{/if}

{#if suppression}
  {@const s = suppression}
  <Boite largeur={460} bordure="var(--danger-line)" etiquette="Confirmer la suppression" surAnnuler={() => (suppression = null)}>
    <div class="boite-entete"><ShieldAlert size={18} color="var(--danger-text)" /><h3>Supprimer définitivement ?</h3></div>
    <div class="boite-corps">
      <p>{s.entrees.length > 1 ? `${s.entrees.length} éléments seront supprimés` : 'Cet élément sera supprimé'} {s.cote === 'local' ? 'de ce poste' : 'du serveur'}, sans corbeille. Le contenu des dossiers est inclus.</p>
      <ul class="noms mono">{#each s.entrees.slice(0, 6) as e}<li>{e.nom}</li>{/each}{#if s.entrees.length > 6}<li>et {s.entrees.length - 6} autres</li>{/if}</ul>
      {#if s.erreur}<span class="message-erreur selectionnable">{s.erreur}</span>{/if}
    </div>
    <div class="boite-pied">
      <button class="btn btn-discret" data-focus-initial onclick={() => (suppression = null)}>Annuler</button>
      <button class="btn btn-danger" onclick={supprimer}>Supprimer</button>
    </div>
  </Boite>
{/if}

{#if permissions}
  {@const d = permissions}
  <Boite largeur={400} etiquette="Permissions" surAnnuler={() => (permissions = null)}>
    <div class="boite-entete"><KeyRound size={18} color="var(--accent-blue)" /><h3>Permissions de {d.entree.nom}</h3></div>
    <div class="boite-corps">
      <div class="droits">
        <span></span>{#each BITS as [, libelle]}<span class="attenue">{libelle}</span>{/each}
        {#each ClasseDroits as classe, i}
          <span>{classe}</span>
          {#each BITS as [bit, libelle]}
            {@const masque = bit << ((2 - i) * 3)}
            <input type="checkbox" class="coche" aria-label={`${classe} : ${libelle}`} checked={(d.mode & masque) !== 0} onchange={() => basculerBit(masque)} />
          {/each}
        {/each}
      </div>
      <p class="mono resume">{droits(d.mode)} · {(d.mode & 0o777).toString(8).padStart(3, '0')}</p>
      {#if d.erreur}<span class="message-erreur selectionnable">{d.erreur}</span>{/if}
    </div>
    <div class="boite-pied">
      <button class="btn btn-discret" onclick={() => (permissions = null)}>Annuler</button>
      <button class="btn btn-primaire" onclick={appliquerPermissions}>Appliquer</button>
    </div>
  </Boite>
{/if}

<style>
  .sftp { position: absolute; inset: 0; display: flex; flex-direction: column; background: var(--surface-window-opaque); }
  .vues { height: 40px; flex: 0 0 40px; display: flex; align-items: center; gap: 4px; padding: 0 8px; border-bottom: 1px solid var(--border-default); }
  .vues button { height: 28px; padding: 0 10px; display: inline-flex; align-items: center; gap: 6px; border: none; border-radius: var(--radius-sm); background: transparent; color: var(--text-secondary); font-size: 12px; }
  .vues button:hover { background: var(--surface-2); }
  .vues .choisie { background: var(--accent-blue-tint); color: #fff; }
  .compteur { min-width: 18px; height: 16px; padding: 0 5px; display: inline-flex; align-items: center; justify-content: center; border-radius: var(--radius-pill); background: var(--surface-3); font-size: 11px; }
  .panneaux { flex: 1; min-height: 0; display: grid; }
  .cote { flex: none; padding: 6px 12px; border-bottom: 1px solid var(--border-default); }
  .cote .segment { display: flex; }
  .cote .segment button { flex: 1; }
  .separateur { position: relative; cursor: col-resize; display: flex; justify-content: center; }
  .separateur span { width: 1px; height: 100%; background: var(--border-default); }
  .separateur:hover span, .separateur:focus-visible span { width: 3px; background: var(--accent-blue); }
  .journal { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 16px; }
  .journal-vide { padding: 64px; text-align: center; font-size: 12px; color: var(--text-muted); }
  .evenement { display: flex; gap: 12px; padding: 2px 0; font-size: 12px; color: var(--text-secondary); }
  .evenement span { color: var(--text-muted); }
  .evenement.erreur { color: var(--danger-text); }
  .bas { height: 44px; flex: 0 0 44px; display: flex; align-items: center; gap: 16px; padding: 0 12px; border-top: 1px solid var(--border-default); background: rgba(0, 0, 0, 0.18); }
  .option { display: inline-flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-secondary); }
  .option select { height: 26px; padding: 0 6px; background: var(--surface-field); border: 1px solid var(--border-default); border-radius: var(--radius-sm); color: var(--text-primary); font-size: 12px; color-scheme: dark; }
  .option.tous { margin-top: 10px; }
  .coche { width: 14px; height: 14px; margin: 0; accent-color: var(--accent-blue); }
  .vide { flex: 1; }
  .en-pause { background: var(--warn-tint); border-color: var(--warn-line); color: var(--warn-500); }
  .menu { position: fixed; z-index: 60; width: 208px; padding: 4px; display: flex; flex-direction: column; background: var(--surface-popover-opaque, var(--surface-popover)); border: 1px solid var(--border-default); border-radius: var(--radius-md); box-shadow: var(--shadow-popover); }
  .menu button { height: 28px; padding: 0 10px; display: flex; align-items: center; gap: 8px; border: none; border-radius: var(--radius-sm); background: transparent; color: var(--text-primary); font-size: 12px; text-align: left; }
  .menu button:hover { background: var(--surface-2); }
  .menu small { margin-left: auto; font-family: var(--font-mono); font-size: 11px; color: var(--text-muted); }
  .menu .danger { color: var(--danger-text); }
  .filet { height: 1px; margin: 4px 0; background: var(--border-default); }
  .noms { margin: 10px 0 0; padding-left: 18px; font-size: 12px; color: var(--text-primary); }
  .droits { display: grid; grid-template-columns: 1fr repeat(3, 70px); gap: 10px; align-items: center; justify-items: start; font-size: 12px; }
  .attenue { color: var(--text-muted); font-size: 11px; }
  .resume { margin: 12px 0 0; font-size: 12px; color: var(--text-secondary); }
</style>
