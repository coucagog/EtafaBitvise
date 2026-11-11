import { Channel, invoke } from '@tauri-apps/api/core'
import { app } from './etat.svelte'

export type Genre = 'dossier' | 'fichier' | 'lien-dossier' | 'lien' | 'autre'
export interface Entree { nom: string; chemin: string; genre: Genre; taille: number; modifie: number | null; mode: number | null }
export type Cote = 'local' | 'distant'
export type Conflit = 'ecraser' | 'reprendre' | 'ignorer' | 'renommer'
export type Regle = Conflit | 'demander'
export type Colonne = 'nom' | 'taille' | 'type' | 'modifie' | 'mode'
export type Vue = 'parcourir' | 'envoi' | 'telechargement' | 'journal'
type EtatLigne = 'attente' | 'actif' | 'pause' | 'fini' | 'erreur' | 'annule' | 'ignore'

export const REGLES: [Regle, string][] = [['demander', 'Demander'], ['ecraser', 'Écraser'], ['reprendre', 'Reprendre'], ['ignorer', 'Ignorer'], ['renommer', 'Renommer']]

export const estDossier = (e: Entree) => e.genre === 'dossier' || e.genre === 'lien-dossier'

export function typeDe(e: Entree) {
  if (e.genre === 'dossier') return 'Dossier'
  if (e.genre === 'lien-dossier' || e.genre === 'lien') return 'Lien'
  if (e.genre === 'autre') return 'Spécial'
  const i = e.nom.lastIndexOf('.')
  return i > 0 ? e.nom.slice(i + 1).toUpperCase() : 'Fichier'
}

export function taille(octets: number) {
  const virgule = (n: number, d: number) => n.toFixed(d).replace('.', ',')
  if (octets < 1024) return `${octets} o`
  if (octets < 1048576) return `${virgule(octets / 1024, 1)} Kio`
  if (octets < 1073741824) return `${virgule(octets / 1048576, 1)} Mio`
  return `${virgule(octets / 1073741824, 2)} Gio`
}

export function droits(mode: number | null) {
  if (mode === null) return ''
  return [6, 3, 0].map((d) => { const b = (mode >> d) & 7; return `${b & 4 ? 'r' : '-'}${b & 2 ? 'w' : '-'}${b & 1 ? 'x' : '-'}` }).join('')
}

export function duree(secondes: number) {
  if (!Number.isFinite(secondes)) return '—'
  const s = Math.ceil(secondes)
  if (s < 60) return `${s} s`
  if (s < 3600) return `${Math.floor(s / 60)} min ${s % 60} s`
  return `${Math.floor(s / 3600)} h ${Math.floor(s / 60) % 60} min`
}

function heure() {
  return new Date().toLocaleTimeString('fr-FR')
}

const comparateur = new Intl.Collator('fr', { numeric: true, sensitivity: 'base' })

export class Panneau {
  chemin = $state('')
  saisie = $state('')
  entrees = $state<Entree[]>([])
  etat = $state<'chargement' | 'pret' | 'erreur'>('chargement')
  erreur = $state('')
  filtre = $state('')
  selection = $state<string[]>([])
  colonne = $state<Colonne>('nom')
  croissant = $state(true)
  depot = $state(false)
  #precedents = $state<string[]>([])
  #suivants = $state<string[]>([])
  #ancre: string | null = null
  #requete = 0
  #cleSignets = ''
  accueil = ''
  signets = $state<string[]>([])

  constructor(readonly cote: Cote) {}

  /** Signets du poste, communs à tous les profils ; signets distants, propres à chaque profil. */
  chargerSignets(profil: string) {
    this.#cleSignets = this.local ? 'mc.signets.local' : `mc.signets.distant.${profil}`
    try {
      const v = JSON.parse(localStorage.getItem(this.#cleSignets) ?? '[]')
      this.signets = Array.isArray(v) ? v.filter((s) => typeof s === 'string') : []
    } catch {
      this.signets = []
    }
  }

  basculerSignet(chemin: string) {
    this.signets = this.signets.includes(chemin) ? this.signets.filter((s) => s !== chemin) : [...this.signets, chemin]
    try {
      localStorage.setItem(this.#cleSignets, JSON.stringify($state.snapshot(this.signets)))
    } catch {
      // Sans stockage, les signets durent le temps de la session.
    }
  }

  get local() { return this.cote === 'local' }
  get autre(): Cote { return this.local ? 'distant' : 'local' }
  peutReculer = $derived(this.#precedents.length > 0)
  peutAvancer = $derived(this.#suivants.length > 0)

  visibles = $derived.by(() => {
    const f = this.filtre.toLowerCase()
    const sens = this.croissant ? 1 : -1
    const valeur = (e: Entree) => {
      if (this.colonne === 'taille') return e.taille
      if (this.colonne === 'modifie') return e.modifie ?? 0
      if (this.colonne === 'mode') return e.mode ?? 0
      return 0
    }
    return this.entrees
      .filter((e) => !f || e.nom.toLowerCase().includes(f))
      .sort((a, b) => {
        if (estDossier(a) !== estDossier(b)) return estDossier(a) ? -1 : 1
        const c = this.colonne === 'nom' ? comparateur.compare(a.nom, b.nom) : this.colonne === 'type' ? comparateur.compare(typeDe(a), typeDe(b)) : valeur(a) - valeur(b)
        return (c || comparateur.compare(a.nom, b.nom)) * sens
      })
  })
  selectionnees = $derived(this.entrees.filter((e) => this.selection.includes(e.chemin)))

  get parent(): string | null {
    const c = this.chemin
    if (this.local && /^[A-Za-z]:\\?$/.test(c)) return ''
    if (c === '' || c === '/') return null
    const separateur = this.local && c.includes('\\') ? '\\' : '/'
    const i = c.replace(/[\\/]+$/, '').lastIndexOf(separateur)
    if (i < 0) return null
    if (i === 0) return '/'
    const parent = c.slice(0, i)
    return /^[A-Za-z]:$/.test(parent) ? `${parent}\\` : parent
  }

  joindre(nom: string) {
    const c = this.chemin
    if (this.local && c.includes('\\')) return c.endsWith('\\') ? `${c}${nom}` : `${c}\\${nom}`
    return c.endsWith('/') ? `${c}${nom}` : `${c}/${nom}`
  }

  async #charger(chemin: string) {
    const requete = ++this.#requete
    this.etat = 'chargement'
    try {
      const entrees = await invoke<Entree[]>(`${this.local ? 'local' : 'sftp'}_lister`, { chemin })
      if (requete !== this.#requete) return false
      this.entrees = entrees
      this.chemin = chemin
      this.saisie = chemin
      this.etat = 'pret'
      this.selection = this.selection.filter((s) => entrees.some((e) => e.chemin === s))
      return true
    } catch (e) {
      if (requete !== this.#requete) return false
      this.erreur = String(e)
      this.chemin = chemin
      this.saisie = chemin
      this.etat = 'erreur'
      return false
    }
  }

  async aller(chemin: string) {
    const avant = this.chemin
    if (chemin !== avant && avant !== '') {
      this.#precedents.push(avant)
      this.#suivants = []
    }
    this.selection = []
    this.filtre = ''
    await this.#charger(chemin)
  }

  actualiser() { return this.#charger(this.chemin) }

  async reculer() {
    const cible = this.#precedents.pop()
    if (cible === undefined) return
    this.#suivants.push(this.chemin)
    this.selection = []
    await this.#charger(cible)
  }

  async avancer() {
    const cible = this.#suivants.pop()
    if (cible === undefined) return
    this.#precedents.push(this.chemin)
    this.selection = []
    await this.#charger(cible)
  }

  monter() {
    const p = this.parent
    if (p !== null) void this.aller(p)
  }

  trier(colonne: Colonne) {
    if (this.colonne === colonne) this.croissant = !this.croissant
    else { this.colonne = colonne; this.croissant = true }
  }

  cliquer(e: Entree, ctrl: boolean, maj: boolean) {
    if (maj && this.#ancre) {
      const noms = this.visibles.map((v) => v.chemin)
      const [a, b] = [noms.indexOf(this.#ancre), noms.indexOf(e.chemin)].sort((x, y) => x - y)
      this.selection = noms.slice(a, b + 1)
      return
    }
    this.#ancre = e.chemin
    if (ctrl) this.selection = this.selection.includes(e.chemin) ? this.selection.filter((s) => s !== e.chemin) : [...this.selection, e.chemin]
    else this.selection = [e.chemin]
  }

  toutSelectionner() { this.selection = this.visibles.map((v) => v.chemin) }

  reinitialiser() {
    this.chemin = ''; this.saisie = ''; this.entrees = []; this.selection = []; this.filtre = ''
    this.#precedents = []; this.#suivants = []; this.etat = 'chargement'
  }
}

export class Ligne {
  etat = $state<EtatLigne>('attente')
  fait = $state(0)
  total = $state(0)
  debit = $state(0)
  fichier = $state('')
  message = $state('')
  #dernier = { t: 0, o: 0 }

  constructor(readonly id: number, readonly envoi: boolean, readonly nom: string, readonly source: string, readonly dossier: string, public conflit: Conflit) {}

  restant = $derived(this.debit > 0 ? (this.total - this.fait) / this.debit : Infinity)
  pourcent = $derived(this.total > 0 ? Math.min(100, (this.fait / this.total) * 100) : this.etat === 'fini' ? 100 : 0)

  mesurer(fait: number, total: number) {
    const t = performance.now()
    if (this.#dernier.t && t > this.#dernier.t && fait >= this.#dernier.o) {
      const instantane = ((fait - this.#dernier.o) * 1000) / (t - this.#dernier.t)
      this.debit = this.debit ? this.debit * 0.7 + instantane * 0.3 : instantane
    }
    this.#dernier = { t, o: fait }
    this.fait = fait
    this.total = total
  }

  repartir() {
    this.etat = 'attente'; this.message = ''; this.fait = 0; this.debit = 0; this.#dernier = { t: 0, o: 0 }
  }
}

export interface QuestionConflit { nom: string; envoi: boolean; dossier: boolean; plusieurs: boolean }
interface ReponseConflit { choix: Conflit | 'annuler'; tous: boolean }
interface Bilan { fichiers: number; ignores: number; erreurs: string[] }
export interface LigneJournalSftp { heure: string; erreur: boolean; message: string }

let prochainId = 1

class EtatSftp {
  local = new Panneau('local')
  distant = new Panneau('distant')
  actif = $state<Cote>('distant')
  vue = $state<Vue>('parcourir')
  lignes = $state<Ligne[]>([])
  regle = $state<Regle>('demander')
  auto = $state(true)
  pauseGlobale = $state(false)
  suivre = $state(true)
  journal = $state<LigneJournalSftp[]>([])
  question = $state<QuestionConflit | null>(null)
  #repondre: ((r: ReponseConflit) => void) | null = null
  glisse: Cote | null = null

  envois = $derived(this.lignes.filter((l) => l.envoi))
  telechargements = $derived(this.lignes.filter((l) => !l.envoi))

  panneau(cote: Cote) { return cote === 'local' ? this.local : this.distant }

  noter(message: string, erreur = false) {
    this.journal.push({ heure: heure(), erreur, message })
    app.consigner(erreur ? 'ERREUR' : 'INFO', 'sftp', message)
  }

  async demarrer() {
    try {
      if (!this.distant.chemin) {
        const a = await invoke<{ local: string; distant: string }>('sftp_accueils')
        this.local.accueil = a.local
        this.distant.accueil = a.distant
        await Promise.all([this.local.aller(a.local), this.distant.aller(a.distant)])
      } else {
        await Promise.all([this.local.actualiser(), this.distant.actualiser()])
      }
    } catch (e) {
      this.distant.etat = 'erreur'
      this.distant.erreur = String(e)
    }
  }

  reinitialiser() {
    this.lignes = []
    this.journal = []
    this.vue = 'parcourir'
    this.pauseGlobale = false
    this.question = null
    this.local.reinitialiser()
    this.distant.reinitialiser()
  }

  repondre(choix: Conflit | 'annuler', tous = false) {
    this.question = null
    this.#repondre?.({ choix, tous })
    this.#repondre = null
  }

  #demander(q: QuestionConflit) {
    this.question = q
    return new Promise<ReponseConflit>((r) => (this.#repondre = r))
  }

  /** Met les éléments en file. La règle « Demander » se résout ici, avant le départ du transfert. */
  async transferer(depuis: Cote, entrees: Entree[]) {
    const source = this.panneau(depuis)
    const cible = this.panneau(source.autre)
    // « Demander » ne doit jamais écraser en silence un fichier apparu depuis le dernier affichage.
    if (this.regle === 'demander' && !(await cible.actualiser())) {
      this.noter(`Transfert non lancé : dossier de destination illisible (${cible.chemin})`, true)
      return
    }
    const noms = new Set(cible.entrees.map((e) => e.nom))
    let reponseCommune: Conflit | null = null
    for (const [i, e] of entrees.entries()) {
      let conflit: Conflit = this.regle === 'demander' ? 'ecraser' : this.regle
      if (noms.has(e.nom) && this.regle === 'demander') {
        if (reponseCommune) conflit = reponseCommune
        else {
          const r = await this.#demander({ nom: e.nom, envoi: depuis === 'local', dossier: estDossier(e), plusieurs: entrees.length - i > 1 })
          if (r.choix === 'annuler') return
          conflit = r.choix
          if (r.tous) reponseCommune = r.choix
        }
      }
      this.lignes.push(new Ligne(prochainId++, depuis === 'local', e.nom, e.chemin, cible.chemin, conflit))
    }
    this.pomper()
  }

  /** Copies déjà confirmées par l'aperçu du miroir : elles remplacent le fichier existant. */
  ajouterCopies(envoi: boolean, copies: { nom: string; source: string; dossier: string }[]) {
    const lignes = copies.map((c) => new Ligne(prochainId++, envoi, c.nom, c.source, c.dossier, 'ecraser'))
    this.lignes.push(...lignes)
    this.pomper()
    return this.lignes.filter((l) => lignes.some((n) => n.id === l.id))
  }

  pomper() {
    if (this.pauseGlobale || !this.auto) return
    for (const envoi of [true, false]) this.#lancerSuivant(envoi)
  }

  #occupe(envoi: boolean) {
    return this.lignes.some((l) => l.envoi === envoi && (l.etat === 'actif' || l.etat === 'pause'))
  }

  #lancerSuivant(envoi: boolean) {
    if (this.#occupe(envoi)) return
    const ligne = this.lignes.find((l) => l.envoi === envoi && l.etat === 'attente')
    if (ligne) void this.#executer(ligne)
  }

  async #executer(l: Ligne) {
    l.etat = 'actif'
    l.message = ''
    const canal = new Channel<{ fait: number; total: number; fichier: string }>()
    canal.onmessage = (p) => { l.mesurer(p.fait, p.total); l.fichier = p.fichier }
    try {
      const b = await invoke<Bilan>('sftp_transferer', { id: l.id, envoi: l.envoi, source: l.source, dossier: l.dossier, conflit: l.conflit, canal })
      if (b.erreurs.length) {
        l.etat = 'erreur'
        l.message = b.erreurs.length > 1 ? `${b.erreurs[0]} (et ${b.erreurs.length - 1} autres)` : b.erreurs[0]
        for (const e of b.erreurs) this.noter(`${l.nom} — ${e}`, true)
      } else if (b.fichiers === 0 && b.ignores > 0) {
        l.etat = 'ignore'
        this.noter(`${l.nom} : ignoré`)
      } else {
        l.etat = 'fini'
        l.fait = l.total
        this.noter(`${l.nom} : ${l.envoi ? 'envoyé' : 'téléchargé'}, ${b.fichiers} fichier${b.fichiers > 1 ? 's' : ''}`)
      }
    } catch (e) {
      const message = String(e)
      if (message === 'Annulé') {
        l.etat = 'annule'
        this.noter(`${l.nom} : annulé`)
      } else {
        l.etat = 'erreur'
        l.message = message
        this.noter(`${l.nom} — ${message}`, true)
      }
    }
    l.debit = 0
    const destination = this.panneau(l.envoi ? 'distant' : 'local')
    if (destination.chemin === l.dossier && destination.etat === 'pret') void destination.actualiser()
    this.pomper()
  }

  async pauseLigne(l: Ligne, active: boolean) {
    await invoke('transfert_pause', { id: l.id, active })
    l.etat = active ? 'pause' : 'actif'
    l.debit = 0
  }

  demarrerLigne(l: Ligne) {
    if (l.etat === 'erreur' || l.etat === 'annule') {
      // Après un échec, reprendre évite de recopier ce qui est déjà arrivé.
      if (l.conflit === 'ecraser') l.conflit = 'reprendre'
      l.repartir()
    }
    this.pomper()
    if (l.etat === 'attente' && !this.#occupe(l.envoi) && !this.pauseGlobale) void this.#executer(l)
  }

  retirer(l: Ligne) {
    if (l.etat === 'actif' || l.etat === 'pause') {
      void invoke('transfert_annuler', { id: l.id })
      return
    }
    this.lignes.splice(this.lignes.indexOf(l), 1)
  }

  demarrerTout(envoi: boolean) {
    for (const l of this.lignes) if (l.envoi === envoi && l.etat === 'pause') void this.pauseLigne(l, false)
    this.pauseGlobale = false
    for (const l of this.lignes) if (l.envoi === envoi && (l.etat === 'erreur' || l.etat === 'annule')) l.repartir()
    if (!this.#occupe(envoi)) this.#lancerSuivant(envoi)
  }

  suspendreTout(envoi: boolean) {
    for (const l of this.lignes) if (l.envoi === envoi && l.etat === 'actif') void this.pauseLigne(l, true)
  }

  retirerTermines(envoi: boolean) {
    this.lignes = this.lignes.filter((l) => l.envoi !== envoi || !['fini', 'ignore'].includes(l.etat))
  }

  /** Ctrl+Shift+F8 et le bouton Pause. */
  basculerPause() {
    this.pauseGlobale = !this.pauseGlobale
    for (const l of this.lignes) {
      if (this.pauseGlobale && l.etat === 'actif') void this.pauseLigne(l, true)
      else if (!this.pauseGlobale && l.etat === 'pause') void this.pauseLigne(l, false)
    }
    this.noter(this.pauseGlobale ? 'Transferts en pause' : 'Transferts repris')
    this.pomper()
  }
}

export const sftp = new EtatSftp()
