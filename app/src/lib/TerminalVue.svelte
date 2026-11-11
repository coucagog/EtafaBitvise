<script lang="ts">
  import { untrack } from 'svelte'
  import { FitAddon } from '@xterm/addon-fit'
  import { SearchAddon } from '@xterm/addon-search'
  import { Terminal } from '@xterm/xterm'
  import '@xterm/xterm/css/xterm.css'
  import { app } from './etat.svelte'
  import { raccourciDeFenetre } from './raccourcis'
  import { type Onglet, PALETTES, POLICE_TERMINAL, session } from './onglets.svelte'

  let { onglet, visible }: { onglet: Onglet; visible: boolean } = $props()
  let hote: HTMLDivElement

  function ajuster() {
    if (!visible || !hote.offsetWidth) return
    onglet.fit?.fit()
  }

  $effect(() => {
    // Hors dépendances : un changement de réglage ne doit pas recréer le terminal.
    const r = untrack(() => ({ ...session.reglages }))
    const term = new Terminal({
      allowProposedApi: true,
      fontFamily: POLICE_TERMINAL,
      fontSize: r.taille,
      cursorStyle: r.curseur,
      cursorBlink: true,
      scrollback: 10000,
      theme: PALETTES[r.palette].theme,
    })
    const fit = new FitAddon()
    const recherche = new SearchAddon()
    term.loadAddon(fit)
    term.loadAddon(recherche)
    term.open(hote)
    // Sur Android, xterm compare le contenu de sa zone de saisie après chaque touche (code 229) :
    // des touches rapprochées réutilisent une comparaison périmée et dupliquent des caractères.
    // Hors composition, la saisie est donc lue directement dans l'évènement `input`.
    if (app.capacites.mobile) {
      hote.addEventListener('keydown', (e) => {
        if (e.keyCode === 229 && !e.isComposing) e.stopPropagation()
      }, true)
      hote.addEventListener('input', (e) => {
        const saisie = e as InputEvent
        if (saisie.isComposing) return
        const texte = saisie.inputType === 'insertText' ? saisie.data : saisie.inputType === 'deleteContentBackward' ? '\x7f' : saisie.inputType === 'insertLineBreak' ? '\r' : null
        if (!texte) return
        e.stopPropagation()
        term.input(texte, true)
      }, true)
    }
    onglet.term = term
    onglet.fit = fit
    onglet.recherche = recherche
    fit.fit()
    onglet.colonnes = term.cols
    onglet.lignes = term.rows

    // Ces touches sont traitées par Session.svelte et ne partent pas vers le serveur.
    term.attachCustomKeyEventHandler((e) => {
      if (raccourciDeFenetre(e, 'terminal')) return false
      if (onglet.statut === 'ferme' && e.key === 'Enter' && e.type === 'keydown' && app.etatSession === 'ok') {
        void session.rouvrir(onglet)
        return false
      }
      return true
    })
    term.onData((d) => session.ecrire(onglet, session.appliquerModificateurs(d)))
    term.onResize(({ cols, rows }) => session.redimensionner(onglet, cols, rows))
    term.onTitleChange((t) => { if (t) onglet.titre = t })
    recherche.onDidChangeResults(({ resultIndex, resultCount }) => {
      if (onglet.cle === session.actif) session.resultats = { index: resultIndex, total: resultCount }
    })
    untrack(() => void session.ouvrir(onglet))
    term.focus()

    const observation = new ResizeObserver(() => ajuster())
    observation.observe(hote)
    return () => observation.disconnect()
  })

  $effect(() => {
    const { taille, palette, curseur } = session.reglages
    const term = onglet.term
    if (!term) return
    term.options.fontSize = taille
    term.options.cursorStyle = curseur
    term.options.theme = PALETTES[palette].theme
    ajuster()
  })

  $effect(() => {
    if (visible) {
      ajuster()
      onglet.term?.focus()
    }
  })
</script>

<div class="hote" class:cache={!visible} style:background={PALETTES[session.reglages.palette].theme.background} bind:this={hote}></div>

<style>
  .hote { position: absolute; inset: 0; padding: 10px 0 12px 14px; }
  .cache { display: none; }
</style>
