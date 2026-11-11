import { Channel, invoke } from '@tauri-apps/api/core'
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager'
import type { FitAddon } from '@xterm/addon-fit'
import type { SearchAddon } from '@xterm/addon-search'
import type { ITheme, Terminal } from '@xterm/xterm'
import { app } from './etat.svelte'

export const PALETTES: Record<string, { libelle: string; theme: ITheme; apercu: string[] }> = {
  graphite: theme('Graphite', '#0c0c0c', '#d4d4d4', '#8c8c8c', '#4ade80', '#60a5fa', '#fbbf24', '#f87171', '#67e8f9'),
  contraste: theme('Contraste élevé', '#000000', '#ffffff', '#bdbdbd', '#74ffa6', '#93c9ff', '#ffd75e', '#ff8a8a', '#86f5ff'),
  ambre: theme('Ambre', '#110d06', '#f2cf8a', '#a88c58', '#f6dc9e', '#ffd99a', '#ffe6b3', '#ffa274', '#f2cf8a'),
  clair: theme('Clair', '#f5f5f2', '#1f1f1f', '#5c5c5c', '#15803d', '#1d5fbf', '#8a5a00', '#b42318', '#0e6f80'),
}

function theme(libelle: string, fond: string, texte: string, attenue: string, vert: string, bleu: string, jaune: string, rouge: string, cyan: string) {
  return {
    libelle,
    apercu: [vert, bleu, jaune, rouge],
    theme: {
      background: fond, foreground: texte, cursor: texte, cursorAccent: fond, selectionBackground: 'rgba(96, 165, 250, 0.35)',
      black: fond, brightBlack: attenue, white: texte, brightWhite: texte,
      red: rouge, brightRed: rouge, green: vert, brightGreen: vert, yellow: jaune, brightYellow: jaune,
      blue: bleu, brightBlue: bleu, magenta: cyan, brightMagenta: cyan, cyan, brightCyan: cyan,
    } satisfies ITheme,
  }
}

export const POLICE_TERMINAL = '"Cascadia Mono", Consolas, ui-monospace, monospace'
export type Curseur = 'block' | 'bar' | 'underline'
export const CURSEURS: [Curseur, string][] = [['block', 'Bloc'], ['bar', 'Barre'], ['underline', 'Souligné']]

interface Reglages { taille: number; palette: string; curseur: Curseur }
const REGLAGES_DEFAUT: Reglages = { taille: 13, palette: 'graphite', curseur: 'block' }
const CLE_REGLAGES = 'mc.terminal'

function lireReglages(): Reglages {
  try {
    return { ...REGLAGES_DEFAUT, ...JSON.parse(localStorage.getItem(CLE_REGLAGES) ?? '{}') }
  } catch {
    return { ...REGLAGES_DEFAUT }
  }
}

export type StatutOnglet = 'ouverture' | 'ok' | 'ferme'

export class Onglet {
  readonly cle = crypto.randomUUID()
  titre = $state('bash')
  statut = $state<StatutOnglet>('ouverture')
  code = $state<number | null>(null)
  heureFin = $state('')
  colonnes = $state(80)
  lignes = $state(24)
  idRust: number | null = null
  generation = 0
  term?: Terminal
  fit?: FitAddon
  recherche?: SearchAddon

  constructor(readonly type: 'terminal' | 'sftp' | 'tunnels' | 'journal' = 'terminal') {
    if (type === 'sftp') this.titre = 'SFTP'
    else if (type === 'tunnels') this.titre = 'Tunnels'
    else if (type === 'journal') this.titre = 'Journal'
    if (type !== 'terminal') this.statut = 'ok'
  }
}

const encodeur = new TextEncoder()

class EtatSession {
  onglets = $state<Onglet[]>([])
  actif = $state<string | null>(null)
  envoyes = $state(0)
  recus = $state(0)
  duree = $state(0)
  flash = $state('')
  requete = $state('')
  resultats = $state<{ index: number; total: number } | null>(null)
  reglages = $state<Reglages>(lireReglages())
  affichageOuvert = $state(false)

  courant = $derived(this.onglets.find((o) => o.cle === this.actif) ?? null)

  #minuteur: ReturnType<typeof setInterval> | undefined
  #minuteurFlash: ReturnType<typeof setTimeout> | undefined

  demarrer() {
    this.vider()
    this.#minuteur = setInterval(() => this.duree++, 1000)
    this.nouvelOnglet()
  }

  vider() {
    clearInterval(this.#minuteur)
    for (const o of this.onglets) {
      o.generation++
      o.term?.dispose()
    }
    this.onglets = []
    this.actif = null
    this.envoyes = 0
    this.recus = 0
    this.duree = 0
    this.resultats = null
    this.requete = ''
  }

  nouvelOnglet() {
    const o = new Onglet()
    this.onglets.push(o)
    this.actif = o.cle
  }

  /** Un seul onglet SFTP, Tunnels ou Journal par session. */
  ouvrirUnique(type: 'sftp' | 'tunnels' | 'journal') {
    let o = this.onglets.find((x) => x.type === type)
    if (!o) {
      o = new Onglet(type)
      this.onglets.push(o)
    }
    this.actif = o.cle
  }

  async ouvrir(o: Onglet) {
    if (o.type !== 'terminal') {
      o.statut = 'ok'
      return
    }
    const generation = ++o.generation
    o.statut = 'ouverture'
    o.code = null
    const sortie = new Channel<ArrayBuffer>()
    sortie.onmessage = (m) => {
      if (generation !== o.generation) return
      const octets = new Uint8Array(m)
      this.recus += octets.length
      o.term?.write(octets)
    }
    const fin = new Channel<number | null>()
    // La fin du canal peut arriver avant la réponse à l'ouverture.
    let finRecue = false
    fin.onmessage = (code) => {
      finRecue = true
      if (generation === o.generation) this.marquerFerme(o, code)
    }
    try {
      const id = await invoke<number>('terminal_ouvrir', { terme: 'xterm-256color', cols: o.colonnes, rows: o.lignes, sortie, fin })
      // Onglet fermé pendant l'ouverture : le canal qui vient de s'ouvrir n'a plus de lecteur.
      if (generation !== o.generation || finRecue) {
        void invoke('terminal_fermer', { id })
        return
      }
      o.idRust = id
      o.statut = 'ok'
      app.consigner('INFO', 'terminal', `Terminal ouvert (${o.colonnes} × ${o.lignes})`)
    } catch (e) {
      if (generation !== o.generation) return
      o.term?.writeln(`\r\n${String(e)}`)
      this.marquerFerme(o, null)
    }
  }

  marquerFerme(o: Onglet, code: number | null) {
    if (o.statut === 'ferme') return
    o.statut = 'ferme'
    o.code = code
    o.heureFin = new Date().toLocaleTimeString('fr-FR')
    if (o.type === 'terminal') app.consigner('INFO', 'terminal', `Terminal fermé${code !== null ? ` (code ${code})` : ''}`)
    if (o.idRust !== null) void invoke('terminal_fermer', { id: o.idRust })
    o.idRust = null
  }

  marquerTousFermes() {
    for (const o of this.onglets) this.marquerFerme(o, null)
  }

  async rouvrirFermes() {
    await Promise.all(this.onglets.filter((o) => o.statut === 'ferme').map((o) => this.rouvrir(o)))
  }

  async rouvrir(o: Onglet) {
    o.term?.write('\r\n')
    await this.ouvrir(o)
    if (o.cle === this.actif) o.term?.focus()
  }

  fermerOnglet(o: Onglet) {
    if (o.idRust !== null) void invoke('terminal_fermer', { id: o.idRust })
    o.generation++
    o.term?.dispose()
    const i = this.onglets.indexOf(o)
    this.onglets.splice(i, 1)
    if (this.actif === o.cle) this.actif = this.onglets[Math.min(i, this.onglets.length - 1)]?.cle ?? null
  }

  // Rangée tactile : Ctrl et Alt valent pour la touche suivante seulement.
  ctrl = $state(false)
  alt = $state(false)

  appliquerModificateurs(d: string) {
    let sortie = d
    if (this.ctrl && d.length === 1) {
      const c = d.toUpperCase().charCodeAt(0)
      if (c >= 64 && c <= 95) sortie = String.fromCharCode(c - 64)
      else if (d === ' ') sortie = '\0'
      else if (d === '?') sortie = '\x7f'
    }
    if (this.alt) sortie = `\x1b${sortie}`
    this.ctrl = false
    this.alt = false
    return sortie
  }

  fleche(o: Onglet, lettre: string) {
    const modif = 1 + (this.alt ? 2 : 0) + (this.ctrl ? 4 : 0)
    const applicatif = o.term?.modes.applicationCursorKeysMode
    this.ctrl = false
    this.alt = false
    this.ecrire(o, modif > 1 ? `\x1b[1;${modif}${lettre}` : applicatif ? `\x1bO${lettre}` : `\x1b[${lettre}`)
  }

  ecrire(o: Onglet, texte: string) {
    if (o.idRust === null) return
    const octets = encodeur.encode(texte)
    this.envoyes += octets.length
    void invoke('terminal_ecrire', { id: o.idRust, donnees: Array.from(octets) })
  }

  redimensionner(o: Onglet, colonnes: number, lignes: number) {
    o.colonnes = colonnes
    o.lignes = lignes
    if (o.idRust !== null) void invoke('terminal_redimensionner', { id: o.idRust, cols: colonnes, rows: lignes })
  }

  annoncer(message: string) {
    clearTimeout(this.#minuteurFlash)
    this.flash = message
    this.#minuteurFlash = setTimeout(() => (this.flash = ''), 1800)
  }

  async copier() {
    const selection = this.courant?.term?.getSelection()
    if (!selection) return
    await writeText(selection)
    this.annoncer('Copié')
  }

  async coller() {
    const o = this.courant
    if (!o || o.statut !== 'ok') return
    const texte = await readText().catch(() => '')
    // term.paste applique le mode « bracketed paste » quand le shell le demande.
    if (texte) o.term?.paste(texte)
  }

  zoomer(ecart: number) {
    this.reglages.taille = Math.max(11, Math.min(22, this.reglages.taille + ecart))
    this.memoriser()
  }

  zoomDefaut() {
    this.reglages.taille = REGLAGES_DEFAUT.taille
    this.memoriser()
  }

  memoriser() {
    try {
      localStorage.setItem(CLE_REGLAGES, JSON.stringify($state.snapshot(this.reglages)))
    } catch {
      // Préférence d'affichage : sans stockage, les valeurs par défaut s'appliquent au prochain lancement.
    }
  }

  chercher(sens: 'suivant' | 'precedent') {
    const o = this.courant
    if (!o?.recherche) return
    if (!this.requete) {
      o.recherche.clearDecorations()
      this.resultats = null
      return
    }
    const options = {
      decorations: {
        matchBackground: '#fbbf2444', activeMatchBackground: '#fbbf24',
        matchOverviewRuler: '#fbbf24', activeMatchColorOverviewRuler: '#fbbf24',
      },
    }
    if (sens === 'suivant') o.recherche.findNext(this.requete, options)
    else o.recherche.findPrevious(this.requete, options)
  }
}

export const session = new EtatSession()
