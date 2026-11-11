<script lang="ts">
  import {
    ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Cable, Check, ChevronDown, ChevronUp, CircleCheck, Clipboard, Clock, Copy, FolderOpen, LogOut, Plus, RefreshCw,
    ScrollText, Search, Terminal as IconeTerminal, Type, Unplug, X, ZoomIn, ZoomOut,
  } from '@lucide/svelte'
  import BarreTitre from './BarreTitre.svelte'
  import Dialogues from './Dialogues.svelte'
  import JournalVue from './JournalVue.svelte'
  import SftpVue from './SftpVue.svelte'
  import TerminalVue from './TerminalVue.svelte'
  import TunnelsVue from './TunnelsVue.svelte'
  import { tunnels } from './tunnels.svelte'
  import { app, type Profil } from './etat.svelte'
  import { raccourciDeFenetre, touches } from './raccourcis'
  import { CURSEURS, type Onglet, PALETTES, session } from './onglets.svelte'

  let { p }: { p: Profil } = $props()
  let champRecherche = $state<HTMLInputElement>()

  const TONS = {
    ok: { c: 'var(--ok-500)', fond: 'var(--ok-tint)', texte: 'Connecté' },
    reconnexion: { c: 'var(--warn-500)', fond: 'var(--warn-tint)', texte: 'Reconnexion' },
    perdue: { c: 'var(--danger-text)', fond: 'var(--danger-tint)', texte: 'Déconnecté' },
  }
  const ton = $derived(TONS[app.etatSession])
  const courant = $derived(session.courant)

  const pastille = (o: Onglet) => {
    if (app.etatSession === 'perdue') return 'var(--rec-500)'
    if (app.etatSession === 'reconnexion' || o.statut === 'ouverture') return 'var(--warn-500)'
    return o.statut === 'ok' ? 'var(--ok-500)' : 'var(--rec-500)'
  }

  function allerTerminal() {
    const t = session.onglets.find((o) => o.type === 'terminal')
    if (t) session.actif = t.cle
    else if (app.etatSession === 'ok') session.nouvelOnglet()
  }

  // L'action part à l'appui : annuler pointerdown garde le focus (et le clavier virtuel), mais peut supprimer le clic.
  function presser(e: PointerEvent, action: () => void) {
    e.preventDefault()
    action()
  }

  function touche(sequence: string) {
    if (courant?.type === 'terminal') session.ecrire(courant, session.appliquerModificateurs(sequence))
  }

  function fleche(lettre: string) {
    if (courant?.type === 'terminal') session.fleche(courant, lettre)
  }

  const RANGEE: { texte?: string; aria?: string; Icone?: typeof ArrowLeft; mod?: 'ctrl' | 'alt'; action: () => void }[] = [
    { texte: 'Échap', action: () => touche('\x1b') },
    { texte: 'Tab', action: () => touche('\t') },
    { texte: 'Ctrl', mod: 'ctrl', action: () => (session.ctrl = !session.ctrl) },
    { texte: 'Alt', mod: 'alt', action: () => (session.alt = !session.alt) },
    { aria: 'Flèche gauche', Icone: ArrowLeft, action: () => fleche('D') },
    { aria: 'Flèche haut', Icone: ArrowUp, action: () => fleche('A') },
    { aria: 'Flèche bas', Icone: ArrowDown, action: () => fleche('B') },
    { aria: 'Flèche droite', Icone: ArrowRight, action: () => fleche('C') },
  ]

  function reconnecter() {
    if (app.etatSession === 'perdue') void app.reconnecter()
    else if (app.etatSession === 'ok' && courant?.type === 'terminal' && courant.statut === 'ferme') void session.rouvrir(courant)
  }

  // Phase de capture : l'action passe avant xterm, et la touche ne part jamais vers le serveur.
  function clavier(e: KeyboardEvent) {
    if (e.key === 'Escape' && session.affichageOuvert) {
      session.affichageOuvert = false
      return
    }
    const id = raccourciDeFenetre(e, 'terminal')
    if (!id) return
    e.preventDefault()
    e.stopPropagation()
    if (id === 'reconnecter') reconnecter()
    else if (id === 'copier') void session.copier()
    else if (id === 'coller') void session.coller()
    else if (id === 'rechercher') champRecherche?.focus()
    else if (id === 'nouveau-terminal' && app.etatSession === 'ok') session.nouvelOnglet()
    else if (id === 'nouveau-sftp' && app.etatSession === 'ok') session.ouvrirUnique('sftp')
    else if (id === 'tunnels') session.ouvrirUnique('tunnels')
    else if (id === 'zoom-avant') session.zoomer(1)
    else if (id === 'zoom-arriere') session.zoomer(-1)
    else if (id === 'zoom-defaut') session.zoomDefaut()
  }

  const duree = $derived([Math.floor(session.duree / 3600), Math.floor(session.duree / 60) % 60, session.duree % 60].map((n) => String(n).padStart(2, '0')).join(':'))

  function taille(octets: number) {
    if (octets < 1024) return `${octets} o`
    if (octets < 1048576) return `${(octets / 1024).toFixed(1).replace('.', ',')} Kio`
    return `${(octets / 1048576).toFixed(2).replace('.', ',')} Mio`
  }

  function toucheRecherche(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault()
      session.chercher(e.shiftKey ? 'precedent' : 'suivant')
    } else if (e.key === 'Escape') {
      session.requete = ''
      session.chercher('suivant')
      courant?.term?.focus()
    }
  }

  const texteResultats = $derived(
    !session.requete || !session.resultats ? '' : session.resultats.total === 0 ? '0 résultat' : `${session.resultats.index + 1} / ${session.resultats.total}`,
  )
</script>

<svelte:window onkeydowncapture={clavier} />

<div class="fenetre {app.etatSession}">
  <BarreTitre>
    <span class="fort">{p.nom}</span>
    <span class="mono attenue">{p.utilisateur}@{p.hote}:{p.port}</span>
    <span class="pilule" role="status" style:background={ton.fond} style:color={ton.c}><span style:background={ton.c}></span>{ton.texte}</span>
  </BarreTitre>

  {#if app.etatSession === 'perdue'}
    <div class="banniere perdue" role="alert">
      <Unplug size={14} color="var(--danger-text)" />
      <span class="texte"><strong>Session déconnectée.</strong> {app.raisonPerte} Les terminaux restent consultables en lecture.</span>
      <button class="btn btn-primaire btn-compact" onclick={() => app.reconnecter()}><RefreshCw size={13} />Reconnecter<span class="kbd sombre">{touches('reconnecter')}</span></button>
    </div>
  {:else if app.etatSession === 'reconnexion'}
    <div class="banniere reconnexion" role="status">
      <span class="tourne"><RefreshCw size={14} color="var(--warn-500)" /></span>
      <span class="texte"><strong>Reconnexion en cours…</strong></span>
    </div>
  {:else if app.reconnecte}
    <div class="banniere reussie" role="status"><CircleCheck size={14} color="var(--ok-500)" /><span class="texte"><strong>Reconnecté.</strong></span></div>
  {/if}

  <div class="espace-travail">
    <nav aria-label="Actions de session">
      <div class="titre-groupe rubrique">Ouvrir</div>
      <button class="action" disabled={app.etatSession !== 'ok'} onclick={() => session.nouvelOnglet()}>
        <IconeTerminal size={14} /><span>Nouveau terminal</span><small>{touches('nouveau-terminal')}</small>
      </button>
      <button class="action" disabled={app.etatSession !== 'ok'} onclick={() => session.ouvrirUnique('sftp')}>
        <FolderOpen size={14} /><span>SFTP</span><small>{touches('nouveau-sftp')}</small>
      </button>
      <button class="action" onclick={() => session.ouvrirUnique('tunnels')}>
        <Cable size={14} /><span>Tunnels</span>{#if tunnels.enErreur}<span class="badge-erreur" title="Tunnels en échec">{tunnels.enErreur}</span>{/if}<small>{touches('tunnels')}</small>
      </button>
      <button class="action" onclick={() => session.ouvrirUnique('journal')}>
        <ScrollText size={14} /><span>Journal</span>
      </button>
      <div class="vide"></div>
      <button class="action danger" onclick={() => app.deconnecter()}><LogOut size={14} /><span>Se déconnecter</span></button>
    </nav>

    <div class="zone">
      <div class="barre-onglets" role="tablist" aria-label="Onglets de la session">
        {#each session.onglets as o (o.cle)}
          {@const actif = o.cle === session.actif}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div class="onglet-session" class:actif role="tab" tabindex="0" aria-selected={actif} title={o.titre} onclick={() => (session.actif = o.cle)}>
            <span class="point" style:background={pastille(o)}></span>
            {#if o.type === 'sftp'}<FolderOpen size={13} />{:else if o.type === 'tunnels'}<Cable size={13} />{:else if o.type === 'journal'}<ScrollText size={13} />{:else}<IconeTerminal size={13} />{/if}
            <span class="nom">{o.titre}</span>
            <button aria-label="Fermer l'onglet" onclick={(e) => { e.stopPropagation(); session.fermerOnglet(o) }}><X size={12} /></button>
          </div>
        {/each}
        <button class="plus" aria-label="Nouveau terminal" title={`Nouveau terminal (${touches('nouveau-terminal')})`} disabled={app.etatSession !== 'ok'} onclick={() => session.nouvelOnglet()}><Plus size={14} /></button>
      </div>

      {#if courant?.type === 'terminal'}
        <div class="outils" role="toolbar" aria-label="Terminal">
          <div class="recherche">
            <Search size={13} color="var(--text-muted)" />
            <input bind:this={champRecherche} bind:value={session.requete} oninput={() => session.chercher('suivant')} onkeydown={toucheRecherche} aria-label="Rechercher dans l'historique" placeholder={`Rechercher dans l'historique   ${touches('rechercher')}`} />
            <span class="mono compte">{texteResultats}</span>
            <button aria-label="Occurrence précédente" onclick={() => session.chercher('precedent')}><ChevronUp size={13} /></button>
            <button aria-label="Occurrence suivante" onclick={() => session.chercher('suivant')}><ChevronDown size={13} /></button>
          </div>
          <span class="vide"></span>
          {#if session.flash}<span class="flash" role="status"><Check size={13} />{session.flash}</span>{/if}
          <button class="outil" aria-label="Copier" title={`Copier (${touches('copier')})`} onclick={() => session.copier()}><Copy size={14} /></button>
          <button class="outil" aria-label="Coller" title={`Coller (${touches('coller')})`} onclick={() => session.coller()}><Clipboard size={14} /></button>
          <span class="trait"></span>
          <button class="outil" aria-label="Réduire la police" title={`Zoom arrière (${touches('zoom-arriere')})`} onclick={() => session.zoomer(-1)}><ZoomOut size={14} /></button>
          <button class="outil taille mono" aria-label="Taille par défaut" title={`Taille par défaut (${touches('zoom-defaut')})`} onclick={() => session.zoomDefaut()}>{session.reglages.taille} px</button>
          <button class="outil" aria-label="Agrandir la police" title={`Zoom avant (${touches('zoom-avant')})`} onclick={() => session.zoomer(1)}><ZoomIn size={14} /></button>
          <span class="trait"></span>
          <button class="affichage" class:ouvert={session.affichageOuvert} aria-pressed={session.affichageOuvert} onclick={() => (session.affichageOuvert = !session.affichageOuvert)}><Type size={14} />Affichage</button>
        </div>
      {/if}

      <div class="terminaux" role="log" aria-label="Sortie du terminal">
        {#each session.onglets as o (o.cle)}
          {#if o.type === 'terminal'}<TerminalVue onglet={o} visible={o.cle === session.actif} />{/if}
        {/each}
        {#if courant?.type === 'sftp'}<SftpVue {p} />{/if}
        {#if courant?.type === 'tunnels'}<TunnelsVue />{/if}
        {#if courant?.type === 'journal'}<JournalVue {p} />{/if}
        {#if courant?.statut === 'ouverture'}
          <div class="ouverture" role="status"><span class="tourne"><RefreshCw size={14} /></span>Ouverture du terminal…</div>
        {/if}
        {#if courant?.type === 'terminal' && courant.statut === 'ferme' && app.etatSession === 'ok'}
          <div class="carte-ferme" role="alert">
            <Unplug size={18} color="var(--danger-text)" />
            <div class="carte-corps">
              <strong>Ce terminal est déconnecté</strong>
              <span>Le serveur a fermé le canal à {courant.heureFin}{courant.code !== null ? ` (code ${courant.code})` : ''}. La session SSH et les autres onglets restent ouverts.</span>
              <div class="carte-actions">
                <button class="btn btn-primaire" onclick={() => courant && session.rouvrir(courant)}><RefreshCw size={14} />Reconnecter<span class="kbd sombre">{touches('reconnecter')}</span></button>
                <button class="btn btn-secondaire" onclick={() => courant && session.fermerOnglet(courant)}>Fermer l'onglet</button>
              </div>
            </div>
          </div>
        {/if}
        {#if session.onglets.length === 0}
          <div class="aucun">
            <strong>Aucun onglet ouvert</strong>
            <span>La session reste active. Ouvrez un terminal pour continuer.</span>
            <button class="btn btn-primaire" disabled={app.etatSession !== 'ok'} onclick={() => session.nouvelOnglet()}><IconeTerminal size={14} />Nouveau terminal</button>
          </div>
        {/if}
      </div>

      {#if app.tactile && courant?.type === 'terminal'}
        <div class="touches" role="toolbar" aria-label="Touches du terminal">
          {#each RANGEE as k}
            {@const engage = k.mod ? session[k.mod] : false}
            <button class:engage aria-label={k.aria} aria-pressed={k.mod ? engage : undefined} onpointerdown={(e) => presser(e, k.action)} onclick={(e) => { if (e.detail === 0) k.action() }}>
              {#if k.Icone}<k.Icone size={18} />{:else}{k.texte}{/if}
            </button>
          {/each}
        </div>
      {/if}

      {#if session.affichageOuvert}
        <div class="popover" role="dialog" aria-label="Affichage du terminal">
          <div class="titre-groupe tete-popover">Affichage du terminal<button aria-label="Fermer" onclick={() => (session.affichageOuvert = false)}><X size={13} /></button></div>
          <div class="reglage">
            <span>Palette</span>
            <div class="palettes" role="radiogroup" aria-label="Palette">
              {#each Object.entries(PALETTES) as [id, palette]}
                <button role="radio" aria-checked={session.reglages.palette === id} class:coche={session.reglages.palette === id} onclick={() => { session.reglages.palette = id; session.memoriser() }}>
                  <span class="apercu" style:background={palette.theme.background}>{#each palette.apercu as c}<span style:background={c}></span>{/each}</span>{palette.libelle}
                </button>
              {/each}
            </div>
          </div>
          <div class="reglage ligne">
            <span>Curseur</span>
            <div class="segment" role="radiogroup" aria-label="Curseur">
              {#each CURSEURS as [id, libelle]}
                <button role="radio" aria-checked={session.reglages.curseur === id} onclick={() => { session.reglages.curseur = id; session.memoriser() }}>{libelle}</button>
              {/each}
            </div>
          </div>
        </div>
      {/if}
    </div>
  </div>

  {#if app.etroit && !app.clavierOuvert}
    <nav class="basse" aria-label="Navigation de la session">
      <button class:choisi={courant?.type === 'terminal'} onclick={allerTerminal}><IconeTerminal size={20} />Terminal</button>
      <button class:choisi={courant?.type === 'sftp'} disabled={app.etatSession !== 'ok'} onclick={() => session.ouvrirUnique('sftp')}><FolderOpen size={20} />Fichiers</button>
      <button class:choisi={courant?.type === 'tunnels'} onclick={() => session.ouvrirUnique('tunnels')}><Cable size={20} />Tunnels{#if tunnels.enErreur}<span class="badge-erreur">{tunnels.enErreur}</span>{/if}</button>
      <button class:choisi={courant?.type === 'journal'} onclick={() => session.ouvrirUnique('journal')}><ScrollText size={20} />Journal</button>
    </nav>
  {/if}

  <footer>
    <span style:color={ton.c}><span class="etat" style:background={ton.c}></span>{ton.texte}</span>
    <span title="Durée de session"><Clock size={12} /><span class="mono">{duree}</span></span>
    <span title="Octets envoyés par les terminaux"><ArrowUp size={12} /><span class="mono">{taille(session.envoyes)}</span></span>
    <span title="Octets reçus par les terminaux"><ArrowDown size={12} /><span class="mono">{taille(session.recus)}</span></span>
    <span class="vide"></span>
    {#if courant?.type === 'terminal'}<span class="mono">{courant.colonnes} × {courant.lignes} · UTF-8</span>{/if}
  </footer>

  <Dialogues />
</div>

<style>
  .fenetre {
    height: 100%; display: flex; flex-direction: column; position: relative; overflow: hidden; border-radius: var(--radius-lg);
    background: var(--surface-window); backdrop-filter: var(--blur-window); -webkit-backdrop-filter: var(--blur-window);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }
  .fenetre.ok { border-color: rgba(74, 222, 128, 0.42); }
  .fenetre.reconnexion { border-color: rgba(251, 191, 36, 0.42); }
  .fort { color: var(--text-primary); font-weight: 600; }
  .attenue { color: var(--text-muted); }
  .pilule { height: 20px; padding: 0 8px; display: inline-flex; align-items: center; gap: 6px; border-radius: var(--radius-pill); font-size: 11px; }
  .pilule span { width: 6px; height: 6px; border-radius: 50%; }
  .banniere { flex: 0 0 auto; display: flex; align-items: center; gap: 10px; min-height: 34px; padding: 4px 8px 4px 12px; font-size: 12px; }
  .banniere .texte { flex: 1; min-width: 0; color: var(--text-primary); }
  .banniere strong { font-weight: 600; }
  .banniere.perdue { background: var(--danger-tint); border-bottom: 1px solid var(--danger-line); }
  .banniere.perdue strong { color: var(--danger-text); }
  .banniere.reconnexion { background: var(--warn-tint); border-bottom: 1px solid var(--warn-line); }
  .banniere.reconnexion strong { color: var(--warn-500); }
  .banniere.reussie { background: var(--ok-tint); border-bottom: 1px solid var(--ok-line); }
  .banniere.reussie strong { color: var(--ok-500); }
  .kbd.sombre { color: var(--accent-ink); background: rgba(0, 0, 0, 0.12); border-color: rgba(0, 0, 0, 0.2); }
  .tourne { display: inline-flex; animation: rotation 900ms linear infinite; }
  @keyframes rotation { to { transform: rotate(360deg); } }

  .espace-travail { flex: 1; min-height: 0; display: grid; grid-template-columns: 264px minmax(0, 1fr); }
  nav { display: flex; flex-direction: column; gap: 2px; padding: 12px 8px 10px; border-right: 1px solid var(--border-subtle); background: rgba(0, 0, 0, 0.12); }
  .rubrique { padding: 0 8px 6px; }
  .vide { flex: 1; }
  .action { height: 32px; width: 100%; padding: 0 8px; display: flex; align-items: center; gap: 10px; border: none; border-radius: var(--radius-sm); background: transparent; color: var(--text-primary); font-size: 12px; text-align: left; }
  .action:hover:not(:disabled) { background: var(--surface-2); }
  .action:disabled { color: var(--text-disabled); cursor: not-allowed; }
  .action span { flex: 1; }
  .action .badge-erreur { flex: none; min-width: 16px; height: 16px; padding: 0 4px; display: inline-flex; align-items: center; justify-content: center; border-radius: var(--radius-pill); background: var(--danger-tint); color: var(--danger-text); font-size: 11px; }
  .action small { font-size: 11px; font-family: var(--font-mono); color: var(--text-muted); }
  .action.danger { color: var(--danger-text); }

  .zone { position: relative; min-width: 0; display: flex; flex-direction: column; }
  .barre-onglets { height: 38px; flex: 0 0 38px; display: flex; align-items: flex-end; gap: 2px; padding: 0 8px; background: var(--surface-sunken); border-bottom: 1px solid var(--border-default); overflow: auto hidden; scrollbar-width: none; }
  .onglet-session { position: relative; top: 1px; height: 31px; width: 176px; min-width: 112px; padding: 0 4px 0 10px; display: flex; align-items: center; gap: 7px; flex-shrink: 1; border: 1px solid var(--border-subtle); border-bottom: none; border-radius: var(--radius-md) var(--radius-md) 0 0; background: var(--surface-1); color: var(--text-secondary); font-size: 12px; cursor: pointer; }
  .onglet-session.actif { background: rgba(255, 255, 255, 0.09); border-color: var(--border-default); box-shadow: inset 0 2px 0 var(--accent-blue); color: var(--text-primary); font-weight: 600; }
  .onglet-session:focus-visible { outline-offset: -2px; }
  .point { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .nom { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .onglet-session button { width: 20px; height: 20px; flex-shrink: 0; padding: 0; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--radius-xs); background: transparent; color: var(--text-muted); }
  .onglet-session button:hover { background: var(--surface-3); color: var(--text-primary); }
  .plus { width: 30px; height: 30px; flex-shrink: 0; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--radius-sm); background: transparent; color: var(--text-secondary); }
  .plus:hover:not(:disabled) { background: var(--surface-2); }

  .outils { height: 40px; flex: 0 0 40px; display: flex; align-items: center; gap: 6px; padding: 0 8px; background: rgba(255, 255, 255, 0.05); border-bottom: 1px solid var(--border-subtle); }
  .recherche { height: 28px; width: 340px; padding: 0 2px 0 8px; display: flex; align-items: center; gap: 6px; background: var(--surface-field); border: 1px solid var(--border-default); border-radius: var(--radius-sm); }
  .recherche:focus-within { border-color: var(--accent-blue); box-shadow: 0 0 0 2px var(--accent-blue-line); }
  .recherche input { flex: 1; min-width: 0; height: 24px; border: none; background: transparent; color: var(--text-primary); font-size: 12px; outline: none; }
  .recherche input::placeholder { color: var(--text-muted); }
  .compte { font-size: 11px; color: var(--text-muted); white-space: nowrap; }
  .recherche button { width: 22px; height: 22px; padding: 0; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--radius-xs); background: transparent; color: var(--text-secondary); }
  .recherche button:hover { background: var(--surface-2); }
  .flash { display: inline-flex; align-items: center; gap: 5px; font-size: 12px; color: var(--ok-500); }
  .outil { width: 28px; height: 28px; padding: 0; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--radius-sm); background: transparent; color: var(--text-secondary); }
  .outil:hover, .affichage:hover { background: var(--surface-2); color: var(--text-primary); }
  .outil.taille { width: auto; min-width: 48px; padding: 0 6px; font-size: 11px; }
  .trait { width: 1px; height: 18px; background: var(--border-default); }
  .affichage { height: 28px; padding: 0 8px; display: inline-flex; align-items: center; gap: 6px; border: none; border-radius: var(--radius-sm); background: transparent; color: var(--text-secondary); font-size: 12px; }
  .affichage.ouvert { background: var(--accent-blue-tint); box-shadow: inset 0 0 0 1px var(--accent-blue-line); color: #fff; }

  .terminaux { position: relative; flex: 1; min-height: 0; background: var(--surface-terminal); }
  .ouverture { position: absolute; left: 14px; bottom: 12px; display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-muted); }
  .carte-ferme { position: absolute; left: 50%; bottom: 24px; transform: translateX(-50%); width: 560px; max-width: calc(100% - 32px); padding: 14px 14px 14px 16px; display: flex; gap: 12px; background: var(--surface-popover); border: 1px solid var(--danger-line); border-radius: var(--radius-md); box-shadow: var(--shadow-dialog); }
  .carte-corps { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; font-size: 12px; line-height: 1.5; color: var(--text-secondary); }
  .carte-corps strong { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .carte-actions { display: flex; gap: 8px; margin-top: 8px; }
  .aucun { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; padding: 24px; font-size: 12px; color: var(--text-muted); }
  .aucun strong { font-size: 14px; font-weight: 600; color: var(--text-primary); }

  .popover { position: absolute; top: 84px; right: 8px; z-index: 50; width: 304px; padding: 10px 14px 14px; display: flex; flex-direction: column; gap: 12px; background: var(--surface-popover); backdrop-filter: var(--blur-popover); border: 1px solid var(--border-default); border-radius: var(--radius-md); box-shadow: var(--shadow-popover); }
  .tete-popover { display: flex; align-items: center; justify-content: space-between; padding-bottom: 6px; border-bottom: 1px solid rgba(255, 255, 255, 0.06); }
  .tete-popover button { width: 22px; height: 22px; padding: 0; display: inline-flex; align-items: center; justify-content: center; border: none; background: transparent; color: var(--text-secondary); }
  .reglage { display: flex; flex-direction: column; gap: 6px; font-size: 12px; color: var(--text-secondary); }
  .reglage.ligne { flex-direction: row; align-items: center; justify-content: space-between; }
  .palettes { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  .palettes button { padding: 7px 8px; display: flex; align-items: center; gap: 6px; border: 1px solid var(--border-default); border-radius: var(--radius-sm); background: transparent; color: var(--text-secondary); font-size: 12px; text-align: left; }
  .palettes button.coche { background: var(--accent-blue-tint); border-color: var(--accent-blue-line); color: #fff; }
  .apercu { display: inline-flex; gap: 2px; padding: 3px; border-radius: var(--radius-xs); flex-shrink: 0; }
  .apercu span { width: 12px; height: 12px; border-radius: 2px; }

  footer { height: var(--footer-h); flex: 0 0 var(--footer-h); display: flex; align-items: center; gap: 16px; padding: 0 12px; background: rgba(0, 0, 0, 0.3); border-top: 1px solid rgba(255, 255, 255, 0.04); font-size: 11px; color: var(--text-muted); }
  footer > span { display: inline-flex; align-items: center; gap: 6px; }
  .etat { width: 7px; height: 7px; border-radius: 50%; }
  .touches { flex: none; display: flex; gap: 4px; padding: 4px; background: var(--surface-sunken); border-top: 1px solid var(--border-default); scrollbar-width: none; }
  .touches button { flex: 1 1 0; min-width: 0; height: 48px; padding: 0; display: inline-flex; align-items: center; justify-content: center; border: 1px solid var(--border-default); border-radius: var(--radius-sm); background: var(--surface-1); color: var(--text-primary); font-size: 13px; }
  .touches .engage { background: var(--accent-blue-tint); border-color: var(--accent-blue-line); color: #fff; }
  .basse { flex: none; display: grid; grid-template-columns: repeat(4, 1fr); padding: 0; flex-direction: row; gap: 0; border-top: 1px solid var(--border-default); border-right: 0; background: var(--surface-sunken); }
  .basse button { position: relative; height: 56px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px; border: none; background: transparent; color: var(--text-secondary); font-size: 11px; }
  .basse .choisi { color: var(--accent-blue); }
  .basse button:disabled { color: var(--text-disabled); }
  .basse .badge-erreur { position: absolute; top: 4px; right: calc(50% - 22px); min-width: 16px; height: 16px; padding: 0 4px; border-radius: var(--radius-pill); background: var(--danger-tint); color: var(--danger-text); font-size: 10px; line-height: 16px; }
</style>
