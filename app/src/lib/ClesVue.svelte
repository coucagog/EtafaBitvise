<script lang="ts">
  import { open, save } from '@tauri-apps/plugin-dialog'
  import { writeText } from '@tauri-apps/plugin-clipboard-manager'
  import { Check, Copy, Download, KeyRound, Lock, Plus, TriangleAlert, Trash2, Upload } from '@lucide/svelte'
  import Boite from './Boite.svelte'
  import { cles, type ErreurCle, type GenreCle, type InfoCle } from './cles.svelte'
  import { app } from './etat.svelte'

  const GENRES: [GenreCle, string][] = [['ed25519', 'Ed25519'], ['rsa', 'RSA 4096']]

  let generation = $state(false)
  let nom = $state('')
  let genre = $state<GenreCle>('ed25519')
  let commentaire = $state('')
  let phrase = $state('')
  let confirmation = $state('')
  let enCours = $state(false)
  let erreurBoite = $state('')

  let aSupprimer = $state<InfoCle | null>(null)
  let aDeverrouiller = $state<string | null>(null)
  let phraseImport = $state('')
  let message = $state('')
  let copiee = $state('')

  const phrasesDifferentes = $derived(phrase !== confirmation)
  const peutGenerer = $derived(!!nom.trim() && !phrasesDifferentes && !enCours)
  const utilisations = $derived(aSupprimer ? app.profils.filter((p) => p.cheminCle === aSupprimer!.chemin).length : 0)

  function ouvrirGeneration() {
    nom = ''
    genre = 'ed25519'
    commentaire = ''
    phrase = ''
    confirmation = ''
    erreurBoite = ''
    generation = true
  }

  async function generer(e: Event) {
    e.preventDefault()
    if (!peutGenerer) return
    enCours = true
    erreurBoite = ''
    try {
      await cles.generer(nom.trim(), genre, commentaire, phrase)
      generation = false
      phrase = ''
      confirmation = ''
    } catch (err) {
      erreurBoite = (err as ErreurCle).message
    } finally {
      enCours = false
    }
  }

  async function importer() {
    message = ''
    const chemin = await open({ multiple: false, directory: false, title: 'Importer une clé privée' })
    if (typeof chemin === 'string') await tenterImport(chemin, null)
  }

  async function tenterImport(chemin: string, passphrase: string | null) {
    try {
      await cles.importer(chemin, passphrase)
      aDeverrouiller = null
      phraseImport = ''
    } catch (err) {
      const erreur = err as ErreurCle
      if (erreur.code === 'phrase-requise') {
        aDeverrouiller = chemin
        erreurBoite = ''
      } else if (aDeverrouiller) {
        erreurBoite = erreur.message
      } else {
        message = erreur.message
      }
    }
  }

  function deverrouiller(e: Event) {
    e.preventDefault()
    if (aDeverrouiller && phraseImport) void tenterImport(aDeverrouiller, phraseImport)
  }

  async function copier(c: InfoCle) {
    await writeText(c.publique)
    copiee = c.nom
    setTimeout(() => {
      if (copiee === c.nom) copiee = ''
    }, 1500)
  }

  async function exporter(c: InfoCle) {
    message = ''
    const destination = await save({ title: 'Exporter la clé publique', defaultPath: `${c.nom}.pub` })
    if (!destination) return
    try {
      await cles.exporter(c.nom, destination)
    } catch (err) {
      message = (err as ErreurCle).message
    }
  }

  async function supprimer(c: InfoCle) {
    aSupprimer = null
    try {
      await cles.supprimer(c.nom)
    } catch (err) {
      message = (err as ErreurCle).message
    }
  }
</script>

<div class="vue-liste">
  <header>
    <div>
      <h2>Clés client</h2>
      <p>Clés privées de l'application, utilisables dans vos profils.</p>
    </div>
    <div class="actions">
      <button class="btn btn-secondaire" onclick={importer}><Upload size={14} />Importer…</button>
      <button class="btn btn-primaire" onclick={ouvrirGeneration}><Plus size={14} />Générer une clé</button>
    </div>
  </header>

  {#if message || cles.erreur}<p class="message-erreur alerte" role="alert">{message || cles.erreur}</p>{/if}

  {#if cles.liste.length === 0}
    <div class="vide">
      <KeyRound size={28} color="var(--text-muted)" />
      <h3>Aucune clé</h3>
      <p>Générez une clé, ou importez une clé OpenSSH, PEM, PKCS#8 ou PuTTY.</p>
    </div>
  {:else}
    <ul>
      {#each cles.liste as c (c.nom)}
        <li>
          <KeyRound size={16} color="var(--accent-blue)" />
          <div class="infos">
            <div class="titre">
              <strong>{c.nom}</strong>
              <span class="etiquette">{c.algorithme}</span>
              {#if c.protegee}
                <span class="etiquette ok"><Lock size={10} />Protégée</span>
              {:else}
                <span class="etiquette alerte-douce">Sans passphrase</span>
              {/if}
            </div>
            <code class="mono selectionnable">{c.empreinte}</code>
            {#if c.commentaire}<small>{c.commentaire}</small>{/if}
          </div>
          <div class="outils">
            <button class="btn btn-discret btn-compact" onclick={() => copier(c)}>
              {#if copiee === c.nom}<Check size={13} />Copiée{:else}<Copy size={13} />Copier la publique{/if}
            </button>
            <button class="btn-icone" aria-label={`Exporter la clé publique ${c.nom}`} title="Exporter la clé publique" onclick={() => exporter(c)}><Download size={14} /></button>
            <button class="btn-icone danger" aria-label={`Supprimer ${c.nom}`} title="Supprimer" onclick={() => (aSupprimer = c)}><Trash2 size={14} /></button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

{#if generation}
  <Boite largeur={460} etiquette="Générer une clé" surAnnuler={() => { if (!enCours) generation = false }}>
    <form onsubmit={generer}>
      <div class="boite-entete"><h3>Générer une clé</h3></div>
      <div class="boite-corps">
        <label class="champ-vertical"><span>Nom</span>
          <input class="champ mono" data-focus-initial bind:value={nom} maxlength="64" autocomplete="off" spellcheck="false" placeholder="ex. id_travail" />
        </label>
        <div class="champ-vertical"><span>Type</span>
          <div class="segment" role="radiogroup" aria-label="Type de clé">
            {#each GENRES as [id, libelle]}
              <button type="button" role="radio" aria-checked={genre === id} onclick={() => (genre = id)}>{libelle}</button>
            {/each}
          </div>
        </div>
        <label class="champ-vertical"><span>Commentaire</span>
          <input class="champ" bind:value={commentaire} maxlength="128" autocomplete="off" placeholder="ex. moi@poste" />
        </label>
        <label class="champ-vertical"><span>Passphrase</span>
          <input class="champ" type="password" bind:value={phrase} autocomplete="new-password" />
        </label>
        <label class="champ-vertical"><span>Confirmation</span>
          <input class="champ" type="password" class:invalide={phrasesDifferentes} bind:value={confirmation} autocomplete="new-password" />
        </label>
        {#if phrasesDifferentes && confirmation}<span class="message-erreur">Les deux passphrases sont différentes.</span>{/if}
        {#if !phrase}<span class="note">Sans passphrase, toute personne qui obtient ce fichier peut utiliser la clé.</span>{/if}
        {#if erreurBoite}<span class="message-erreur" role="alert">{erreurBoite}</span>{/if}
      </div>
      <div class="boite-pied">
        <button type="button" class="btn btn-discret" disabled={enCours} onclick={() => (generation = false)}>Annuler</button>
        <button type="submit" class="btn btn-primaire" disabled={!peutGenerer}>{enCours ? 'Génération…' : 'Générer'}</button>
      </div>
    </form>
  </Boite>
{/if}

{#if aDeverrouiller}
  <Boite largeur={420} etiquette="Passphrase de la clé" surAnnuler={() => (aDeverrouiller = null)}>
    <form onsubmit={deverrouiller}>
      <div class="boite-entete"><h3>Passphrase de la clé</h3></div>
      <div class="boite-corps">
        <p>Ce format doit être converti, ce qui exige la passphrase. La clé restera protégée par elle.</p>
        <input class="champ" type="password" data-focus-initial aria-label="Passphrase" bind:value={phraseImport} autocomplete="off" />
        {#if erreurBoite}<span class="message-erreur" role="alert">{erreurBoite}</span>{/if}
      </div>
      <div class="boite-pied">
        <button type="button" class="btn btn-discret" onclick={() => { aDeverrouiller = null; phraseImport = '' }}>Annuler</button>
        <button type="submit" class="btn btn-primaire" disabled={!phraseImport}>Importer</button>
      </div>
    </form>
  </Boite>
{/if}

{#if aSupprimer}
  {@const c = aSupprimer}
  <Boite largeur={440} bordure="var(--danger-line)" etiquette="Supprimer la clé" surAnnuler={() => (aSupprimer = null)}>
    <div class="boite-entete"><TriangleAlert size={18} color="var(--danger-text)" /><h3>Supprimer la clé « {c.nom} » ?</h3></div>
    <div class="boite-corps">
      <p>Cette action est définitive. Sans copie de cette clé, vous perdrez l'accès aux serveurs qui n'acceptent qu'elle.</p>
      {#if utilisations}<p class="fort">{utilisations} profil{utilisations > 1 ? 's utilisent' : ' utilise'} cette clé.</p>{/if}
    </div>
    <div class="boite-pied">
      <button class="btn btn-discret" data-focus-initial onclick={() => (aSupprimer = null)}>Annuler</button>
      <button class="btn btn-danger" onclick={() => supprimer(c)}>Supprimer la clé</button>
    </div>
  </Boite>
{/if}

<style>
  .vue-liste { flex: 1; min-height: 0; overflow: auto; padding: 18px 20px; display: flex; flex-direction: column; gap: 14px; }
  header { display: flex; align-items: flex-start; justify-content: space-between; flex-wrap: wrap; gap: 12px; }
  h2 { margin: 0; font-size: 15px; font-weight: 600; }
  header p { margin: 3px 0 0; font-size: 12px; color: var(--text-muted); }
  .actions { display: flex; gap: 8px; }
  .alerte { margin: 0; }
  .vide { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; text-align: center; padding: 24px; }
  .vide h3 { margin: 4px 0 0; font-size: 14px; }
  .vide p { margin: 0; font-size: 12px; color: var(--text-muted); }
  ul { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border-default); border-radius: var(--radius-md); overflow: hidden; }
  li { display: flex; align-items: center; gap: 12px; padding: 10px 12px; background: var(--surface-1); }
  li + li { border-top: 1px solid var(--border-subtle); }
  .infos { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .titre { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; font-size: 13px; }
  .etiquette { display: inline-flex; align-items: center; gap: 4px; height: 18px; padding: 0 6px; border-radius: var(--radius-pill); background: var(--surface-2); border: 1px solid var(--border-default); font-size: 10px; color: var(--text-secondary); }
  .etiquette.ok { color: var(--success-text, #4ade80); }
  .etiquette.alerte-douce { color: var(--warn-500); }
  code { font-size: 11px; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  small { font-size: 11px; color: var(--text-muted); }
  .outils { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
  :global(html.etroit) li { flex-wrap: wrap; }
  .champ-vertical { display: flex; flex-direction: column; gap: 4px; }
  .champ-vertical > span { color: var(--text-secondary); }
  .segment { align-self: flex-start; }
  .note { font-size: 11px; color: var(--warn-500); }
  .fort { color: var(--text-primary); }
</style>
