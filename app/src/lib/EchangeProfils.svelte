<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { open, save } from '@tauri-apps/plugin-dialog'
  import { FileDown, FileUp, Lock, TriangleAlert } from '@lucide/svelte'
  import Boite from './Boite.svelte'
  import { cles } from './cles.svelte'
  import { app } from './etat.svelte'

  interface ErreurEchange { code: string; message: string }
  interface BilanExport { profils: number; cles: number; clesPrivees: number; hotes: number; clesIllisibles: string[] }
  interface Apercu { profils: { id: string; nom: string; serveur: string; existe: boolean }[]; cles: number; clesPrivees: boolean; hotes: number }
  interface BilanImport {
    ajoutes: number; remplaces: number; clesImportees: number; clesPresentes: number; clesAChoisir: string[]
    hotesAjoutes: number; hotesPresents: number; hotesDifferents: string[]
  }

  const PHRASE_MIN = 8
  const FILTRES = [{ name: 'Profils MLS Connect', extensions: ['mcprofils'] }]

  let mode = $state<'export' | 'import' | null>(null)
  let enCours = $state(false)
  let erreur = $state('')

  // Export
  let choisis = $state<Record<string, boolean>>({})
  let avecCles = $state(false)
  let phrase = $state('')
  let confirmation = $state('')
  let bilanExport = $state<BilanExport | null>(null)
  const nbChoisis = $derived(app.profils.filter((p) => choisis[p.id]).length)
  const phraseValide = $derived(!avecCles || (phrase.length >= PHRASE_MIN && phrase === confirmation))
  const nonEnregistres = $derived(Object.keys(app.modifies).length > 0)

  // Import
  let source = $state('')
  let apercu = $state<Apercu | null>(null)
  let doublon = $state<'copier' | 'remplacer'>('copier')
  let phraseImport = $state('')
  let bilanImport = $state<BilanImport | null>(null)
  const doublons = $derived(apercu?.profils.filter((p) => p.existe).length ?? 0)

  const pluriel = (n: number, un: string, plusieurs: string) => `${n} ${n > 1 ? plusieurs : un}`

  function fermer() {
    if (enCours) return
    mode = null
    phrase = ''
    confirmation = ''
    phraseImport = ''
    erreur = ''
  }

  function ouvrirExport() {
    choisis = Object.fromEntries(app.profils.map((p) => [p.id, true]))
    avecCles = false
    bilanExport = null
    erreur = ''
    mode = 'export'
  }

  async function exporter() {
    if (!nbChoisis || !phraseValide || enCours) return
    erreur = ''
    const destination = await save({ title: 'Exporter les profils', defaultPath: 'profils-mls-connect.mcprofils', filters: app.capacites.mobile ? undefined : FILTRES })
    if (!destination) return
    enCours = true
    try {
      const ids = app.profils.filter((p) => choisis[p.id]).map((p) => p.id)
      bilanExport = await invoke<BilanExport>('profils_exporter', { ids, destination, phrase: avecCles ? phrase : null })
      phrase = ''
      confirmation = ''
    } catch (e) {
      erreur = (e as ErreurEchange).message ?? String(e)
    } finally {
      enCours = false
    }
  }

  async function ouvrirImport() {
    erreur = ''
    const chemin = await open({ multiple: false, directory: false, title: 'Importer des profils', filters: app.capacites.mobile ? undefined : FILTRES })
    if (typeof chemin !== 'string') return
    try {
      apercu = await invoke<Apercu>('profils_apercu', { source: chemin })
      source = chemin
      doublon = 'copier'
      phraseImport = ''
      bilanImport = null
      mode = 'import'
    } catch (e) {
      apercu = null
      erreur = (e as ErreurEchange).message ?? String(e)
      bilanImport = null
      mode = 'import'
    }
  }

  async function importer() {
    if (!apercu || enCours) return
    erreur = ''
    enCours = true
    try {
      bilanImport = await invoke<BilanImport>('profils_importer', {
        source,
        phrase: apercu.clesPrivees && phraseImport ? phraseImport : null,
        doublon,
      })
      phraseImport = ''
      await app.charger()
      await cles.charger()
    } catch (e) {
      erreur = (e as ErreurEchange).message ?? String(e)
    } finally {
      enCours = false
    }
  }
</script>

<button class="btn-icone" aria-label="Importer des profils" title="Importer des profils" disabled={nonEnregistres || app.phase === 'connexion'} onclick={ouvrirImport}>
  <FileUp size={14} />
</button>
<button class="btn-icone" aria-label="Exporter des profils" title="Exporter des profils" disabled={nonEnregistres || app.profils.length === 0} onclick={ouvrirExport}>
  <FileDown size={14} />
</button>

{#if mode === 'export'}
  <Boite largeur={480} etiquette="Exporter des profils" surAnnuler={fermer}>
    {#if bilanExport}
      <div class="boite-entete"><h3>Profils exportés</h3></div>
      <div class="boite-corps">
        <p>
          {pluriel(bilanExport.profils, 'profil', 'profils')}, {pluriel(bilanExport.cles, 'clé', 'clés')}
          {#if bilanExport.clesPrivees}dont {pluriel(bilanExport.clesPrivees, 'clé privée chiffrée', 'clés privées chiffrées')}{:else}(partie publique seulement){/if},
          {pluriel(bilanExport.hotes, "empreinte d'hôte", "empreintes d'hôtes")}.
        </p>
        {#if bilanExport.clesIllisibles.length}
          <p class="note">Clé illisible, exportée sans clé : {bilanExport.clesIllisibles.join(', ')}.</p>
        {/if}
        {#if bilanExport.clesPrivees}<p>Transmettez la phrase par un autre moyen que le fichier.</p>{/if}
      </div>
      <div class="boite-pied"><button class="btn btn-primaire" data-focus-initial onclick={fermer}>Fermer</button></div>
    {:else}
      <form onsubmit={(e) => { e.preventDefault(); void exporter() }}>
        <div class="boite-entete"><h3>Exporter des profils</h3></div>
        <div class="boite-corps">
          <div class="choix-tete">
            <span>{nbChoisis} sur {app.profils.length}</span>
            <button type="button" class="btn btn-discret btn-compact" onclick={() => (choisis = Object.fromEntries(app.profils.map((p) => [p.id, true])))}>Tous</button>
            <button type="button" class="btn btn-discret btn-compact" onclick={() => (choisis = {})}>Aucun</button>
          </div>
          <ul class="choix" aria-label="Profils à exporter">
            {#each app.profils as p (p.id)}
              <li><label><input type="checkbox" bind:checked={choisis[p.id]} /><span class="nom">{p.nom}</span><span class="adresse mono">{p.utilisateur}@{p.hote}</span></label></li>
            {/each}
          </ul>
          <small class="aide">Un profil de rebond est ajouté avec le profil qui l'utilise. Les mots de passe et passphrases mémorisés ne sont jamais exportés.</small>
          <label class="case"><input type="checkbox" bind:checked={avecCles} /><Lock size={13} /><span>Inclure les clés privées, chiffrées par une phrase</span></label>
          {#if avecCles}
            <label class="champ-vertical"><span>Phrase de l'export ({PHRASE_MIN} caractères au moins)</span>
              <input class="champ" type="password" bind:value={phrase} autocomplete="new-password" />
            </label>
            <label class="champ-vertical"><span>Confirmation</span>
              <input class="champ" type="password" class:invalide={!!confirmation && phrase !== confirmation} bind:value={confirmation} autocomplete="new-password" />
            </label>
            {#if confirmation && phrase !== confirmation}<span class="message-erreur">Les deux phrases sont différentes.</span>{/if}
            <small class="aide">Sans cette phrase, les clés privées du fichier sont illisibles. Une clé déjà protégée par sa propre passphrase la garde.</small>
          {/if}
          {#if erreur}<span class="message-erreur" role="alert">{erreur}</span>{/if}
        </div>
        <div class="boite-pied">
          <button type="button" class="btn btn-discret" disabled={enCours} onclick={fermer}>Annuler</button>
          <button type="submit" class="btn btn-primaire" data-focus-initial disabled={!nbChoisis || !phraseValide || enCours}>{enCours ? 'Export…' : 'Exporter…'}</button>
        </div>
      </form>
    {/if}
  </Boite>
{/if}

{#if mode === 'import'}
  <Boite largeur={480} etiquette="Importer des profils" surAnnuler={fermer}>
    {#if bilanImport}
      <div class="boite-entete"><h3>Profils importés</h3></div>
      <div class="boite-corps">
        <p>
          {pluriel(bilanImport.ajoutes, 'profil ajouté', 'profils ajoutés')}{#if bilanImport.remplaces}, {pluriel(bilanImport.remplaces, 'remplacé', 'remplacés')}{/if}.
          {#if bilanImport.clesImportees}{pluriel(bilanImport.clesImportees, 'clé importée', 'clés importées')}.{/if}
          {#if bilanImport.clesPresentes}{pluriel(bilanImport.clesPresentes, 'clé déjà présente', 'clés déjà présentes')}.{/if}
          {pluriel(bilanImport.hotesAjoutes, "empreinte d'hôte ajoutée", "empreintes d'hôtes ajoutées")}.
        </p>
        {#if bilanImport.clesAChoisir.length}
          <p class="note">Clé à choisir dans Authentification : {bilanImport.clesAChoisir.join(', ')}.</p>
        {/if}
        {#if bilanImport.hotesDifferents.length}
          <p class="note"><TriangleAlert size={12} /> Clé d'hôte différente de celle déjà enregistrée, gardée telle quelle : {bilanImport.hotesDifferents.join(', ')}. Vérifiez-la avec l'administrateur du serveur.</p>
        {/if}
        <p>Mots de passe et passphrases mémorisés : à saisir à la première connexion.</p>
      </div>
      <div class="boite-pied"><button class="btn btn-primaire" data-focus-initial onclick={fermer}>Fermer</button></div>
    {:else if apercu}
      <form onsubmit={(e) => { e.preventDefault(); void importer() }}>
        <div class="boite-entete"><h3>Importer des profils</h3></div>
        <div class="boite-corps">
          <p>{pluriel(apercu.profils.length, 'profil', 'profils')}, {pluriel(apercu.cles, 'clé', 'clés')}, {pluriel(apercu.hotes, "empreinte d'hôte", "empreintes d'hôtes")}.</p>
          <ul class="choix" aria-label="Profils du fichier">
            {#each apercu.profils as p (p.id)}
              <li><span class="ligne"><span class="nom">{p.nom}</span><span class="adresse mono">{p.serveur}</span>{#if p.existe}<span class="etiquette">déjà présent</span>{/if}</span></li>
            {/each}
          </ul>
          {#if doublons}
            <div class="champ-vertical"><span>{pluriel(doublons, 'profil existe', 'profils existent')} déjà sur cette machine</span>
              <div class="segment" role="radiogroup" aria-label="Profils déjà présents">
                <button type="button" role="radio" aria-checked={doublon === 'copier'} onclick={() => (doublon = 'copier')}>Ajouter une copie</button>
                <button type="button" role="radio" aria-checked={doublon === 'remplacer'} onclick={() => (doublon = 'remplacer')}>Remplacer</button>
              </div>
            </div>
          {/if}
          {#if apercu.clesPrivees}
            <label class="champ-vertical"><span>Phrase de l'export, pour les clés privées</span>
              <input class="champ" type="password" data-focus-initial bind:value={phraseImport} autocomplete="off" />
            </label>
            <small class="aide">Laissez vide pour importer les profils sans les clés privées.</small>
          {/if}
          {#if erreur}<span class="message-erreur" role="alert">{erreur}</span>{/if}
        </div>
        <div class="boite-pied">
          <button type="button" class="btn btn-discret" disabled={enCours} onclick={fermer}>Annuler</button>
          <button type="submit" class="btn btn-primaire" disabled={enCours}>{enCours ? 'Import…' : 'Importer'}</button>
        </div>
      </form>
    {:else}
      <div class="boite-entete"><h3>Import impossible</h3></div>
      <div class="boite-corps"><span class="message-erreur" role="alert">{erreur}</span></div>
      <div class="boite-pied"><button class="btn btn-primaire" data-focus-initial onclick={fermer}>Fermer</button></div>
    {/if}
  </Boite>
{/if}

<style>
  .btn-icone:disabled { color: var(--text-disabled); cursor: not-allowed; background: transparent; }
  .choix-tete { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-secondary); }
  .choix-tete span { flex: 1; }
  .choix { list-style: none; margin: 0; padding: 0; max-height: 220px; overflow: auto; border: 1px solid var(--border-default); border-radius: var(--radius-md); }
  .choix li + li { border-top: 1px solid var(--border-subtle); }
  .choix label, .ligne { display: flex; align-items: center; gap: 8px; padding: 6px 10px; font-size: 12px; color: var(--text-primary); }
  .choix label { cursor: pointer; }
  .nom { flex-shrink: 0; }
  .adresse { flex: 1; min-width: 0; font-size: 11px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .etiquette { height: 18px; padding: 0 6px; display: inline-flex; align-items: center; border-radius: var(--radius-pill); background: var(--surface-2); border: 1px solid var(--border-default); font-size: 10px; color: var(--text-secondary); }
  .case { display: inline-flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-primary); cursor: pointer; }
  .champ-vertical { display: flex; flex-direction: column; gap: 4px; }
  .champ-vertical > span { color: var(--text-secondary); }
  .segment { align-self: flex-start; }
  .aide { font-size: 11px; line-height: 1.45; color: var(--text-muted); }
  .note { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--warn-500); }
</style>
