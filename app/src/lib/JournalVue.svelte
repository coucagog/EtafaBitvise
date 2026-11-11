<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { writeText } from '@tauri-apps/plugin-clipboard-manager'
  import { save } from '@tauri-apps/plugin-dialog'
  import { Check, CircleX, Copy, Download, LogOut, Search, TriangleAlert } from '@lucide/svelte'
  import { app, type LigneJournal, type Profil } from './etat.svelte'

  let { p }: { p: Profil } = $props()

  type FiltreNiveau = 'tous' | 'alertes' | 'erreurs'
  const NIVEAUX: [FiltreNiveau, string][] = [['tous', 'Tout'], ['alertes', 'Alertes'], ['erreurs', 'Erreurs']]

  let niveau = $state<FiltreNiveau>('tous')
  let source = $state('')
  let requete = $state('')
  let message = $state('')
  let erreur = $state(false)
  let liste = $state<HTMLDivElement>()
  let suivre = true
  let minuteur: ReturnType<typeof setTimeout> | undefined

  const sources = $derived([...new Set(app.journal.map((l) => l.source))].sort())
  const lignes = $derived.by(() => {
    const q = requete.trim().toLowerCase()
    return app.journal.filter((l) => {
      if (niveau === 'erreurs' && l.niveau !== 'ERREUR') return false
      if (niveau === 'alertes' && l.niveau === 'INFO') return false
      if (source && l.source !== source) return false
      return !q || l.message.toLowerCase().includes(q) || l.source.toLowerCase().includes(q)
    })
  })

  // La liste suit les nouvelles entrées, sauf si l'on est remonté pour lire.
  $effect(() => {
    void lignes.length
    if (liste && suivre) liste.scrollTop = liste.scrollHeight
  })

  function defilement() {
    if (liste) suivre = liste.scrollHeight - liste.scrollTop - liste.clientHeight < 32
  }

  function annoncer(texte: string, enErreur = false) {
    message = texte
    erreur = enErreur
    clearTimeout(minuteur)
    minuteur = setTimeout(() => (message = ''), 2500)
  }

  const enTexte = (l: LigneJournal) => `${l.heure}  ${l.niveau.padEnd(6)}  ${l.source.padEnd(10)}  ${l.message}`

  async function copier() {
    try {
      await writeText(lignes.map(enTexte).join('\n'))
      annoncer('Copié')
    } catch (e) {
      annoncer(String(e), true)
    }
  }

  async function exporter() {
    const d = new Date()
    const date = `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, '0')}${String(d.getDate()).padStart(2, '0')}`
    const nom = p.nom.replace(/[^\p{L}\p{N}._-]+/gu, '-') || 'session'
    const destination = await save({
      title: 'Exporter le journal',
      defaultPath: `mls-connect-${nom}-${date}.log`,
      filters: app.capacites.mobile ? undefined : [{ name: 'Journal', extensions: ['log', 'txt'] }],
    })
    if (!destination) return
    const entete = `MLS Connect, journal de session\n${p.nom} : ${p.utilisateur}@${p.hote}:${p.port}\n${d.toLocaleString('fr-FR')}\n\n`
    try {
      await invoke('journal_exporter', { destination, contenu: entete + lignes.map(enTexte).join('\n') + '\n' })
      annoncer('Journal exporté')
    } catch (e) {
      annoncer(String(e), true)
    }
  }
</script>

<div class="vue-journal">
  <div class="commandes" role="toolbar" aria-label="Journal de session">
    <div class="segment" role="radiogroup" aria-label="Niveau">
      {#each NIVEAUX as [id, libelle]}
        <button role="radio" aria-checked={niveau === id} onclick={() => (niveau = id)}>{libelle}</button>
      {/each}
    </div>
    <select class="champ choix" aria-label="Source" bind:value={source}>
      <option value="">Toutes les sources</option>
      {#each sources as s}<option value={s}>{s}</option>{/each}
    </select>
    <label class="cherche">
      <Search size={13} color="var(--text-muted)" />
      <input bind:value={requete} aria-label="Rechercher dans le journal" placeholder="Rechercher" />
    </label>
    <span class="espace"></span>
    {#if message}<span class="annonce" class:erreur role="status">{#if !erreur}<Check size={13} />{/if}{message}</span>{/if}
    <button class="btn btn-secondaire btn-compact" disabled={lignes.length === 0} onclick={copier}><Copy size={13} />Copier</button>
    <button class="btn btn-secondaire btn-compact" disabled={lignes.length === 0} onclick={exporter}><Download size={13} />Exporter</button>
    {#if app.etroit}
      <button class="btn btn-secondaire btn-compact quitter" onclick={() => app.deconnecter()}><LogOut size={13} />Se déconnecter</button>
    {/if}
  </div>

  <div class="liste mono selectionnable" role="log" aria-label="Évènements de la session" bind:this={liste} onscroll={defilement}>
    {#if lignes.length === 0}
      <div class="rien">{app.journal.length === 0 ? 'Aucune entrée pour cette session.' : 'Aucune entrée pour ce filtre.'}</div>
    {/if}
    {#each lignes as l}
      <div class="evt {l.niveau}">
        <span class="heure">{l.heure}</span>
        <span class="niveau">{#if l.niveau === 'ERREUR'}<CircleX size={11} />{:else if l.niveau === 'AVERT'}<TriangleAlert size={11} />{/if}{l.niveau}</span>
        <span class="source">{l.source}</span>
        <span class="texte">{l.message}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .vue-journal { position: absolute; inset: 0; display: flex; flex-direction: column; background: var(--surface-terminal); }
  .commandes { flex: none; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 6px 8px; background: rgba(255, 255, 255, 0.05); border-bottom: 1px solid var(--border-subtle); }
  .choix { width: 170px; height: var(--control-h-sm); }
  .cherche { width: 220px; height: var(--control-h-sm); padding: 0 8px; display: flex; align-items: center; gap: 6px; background: var(--surface-field); border: 1px solid var(--border-default); border-radius: var(--radius-sm); }
  .cherche:focus-within { border-color: var(--accent-blue); box-shadow: 0 0 0 2px var(--accent-blue-line); }
  .cherche input { flex: 1; min-width: 0; height: 100%; border: none; background: transparent; color: var(--text-primary); font-size: 12px; outline: none; }
  .espace { flex: 1; }
  .annonce { display: inline-flex; align-items: center; gap: 5px; font-size: 12px; color: var(--ok-500); }
  .annonce.erreur { color: var(--danger-text); }
  .quitter { color: var(--danger-text); }
  .liste { flex: 1; min-height: 0; overflow: auto; padding: 4px 0; font-size: 12px; }
  .rien { padding: 48px 16px; text-align: center; font-family: var(--font-sans); color: var(--text-muted); }
  .evt { display: grid; grid-template-columns: 96px 72px 76px minmax(0, 1fr); gap: 8px; padding: 3px 12px; color: var(--text-secondary); }
  .evt.ERREUR { background: rgba(239, 68, 68, 0.07); }
  .evt.AVERT { background: rgba(251, 191, 36, 0.06); }
  .heure, .source { color: var(--text-muted); }
  .niveau { display: inline-flex; align-items: center; gap: 4px; color: var(--text-muted); }
  .ERREUR .niveau { color: var(--danger-text); font-weight: 600; }
  .AVERT .niveau { color: var(--warn-500); font-weight: 600; }
  .texte { color: var(--text-primary); white-space: pre-wrap; overflow-wrap: anywhere; }
  :global(html.etroit) .choix, :global(html.etroit) .cherche { width: auto; flex: 1 1 140px; }
  :global(html.etroit) .espace { display: none; }
  :global(html.etroit) .evt { grid-template-columns: 96px minmax(0, 1fr); gap: 2px 8px; padding: 6px 12px; }
  :global(html.etroit) .source { text-align: right; }
  :global(html.etroit) .niveau { display: none; }
  :global(html.etroit) .texte { grid-column: 1 / -1; }
</style>
