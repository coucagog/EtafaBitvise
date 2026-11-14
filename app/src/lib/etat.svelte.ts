import { Channel, invoke } from '@tauri-apps/api/core'
import { session } from './onglets.svelte'
import { sftp } from './sftp.svelte'
import { type Regle, tunnels } from './tunnels.svelte'

export const NOM_APP = 'MLS Connect'

export type Methode = 'mot-de-passe' | 'cle' | 'clavier' | 'agent'
export type GenreProxy = 'aucun' | 'socks5' | 'http'
export type NiveauAgent = 'aucun' | 'lecture' | 'exploitation' | 'encadre' | 'complet'
export interface AccesAgent { niveau: NiveauAgent; commandes: string[]; dossiers: string[] }

export interface Profil {
  id: string
  nom: string
  groupe: string
  hote: string
  port: number
  utilisateur: string
  methode: Methode
  cheminCle: string
  delaiConnexion: number
  keepalive: boolean
  intervalleKeepalive: number
  notes: string
  derniereConnexion: string | null
  tunnels: Regle[]
  proxy: { genre: GenreProxy; hote: string; port: number }
  rebond: string
  memoriser: boolean
  agent: AccesAgent
}

export interface Empreinte { algorithme: string; empreinte: string }

export type EtatEtape = 'en-cours' | 'fait' | 'attente' | 'erreur' | 'a-venir'

type Evenement =
  | { type: 'etape'; index: number; etat: Exclude<EtatEtape, 'a-venir'>; detail: string }
  | { type: 'cle-inconnue'; hote: string; port: number; presentee: Empreinte }
  | { type: 'cle-modifiee'; hote: string; port: number; presentee: Empreinte; enregistrees: Empreinte[] }
  | { type: 'session-perdue'; raison: string }
  | QuestionClavier

export interface QuestionClavier { type: 'question-clavier'; nom: string; instructions: string; invites: { texte: string; masque: boolean }[] }

export interface ErreurConnexion { code: string; titre: string; texte: string; detail: string | null; conseil: string | null }
export interface ErreurProfils { message: string; detail: string | null }

export type Question =
  | { type: 'cle-inconnue'; hote: string; port: number; presentee: Empreinte }
  | { type: 'cle-modifiee'; hote: string; port: number; presentee: Empreinte; enregistrees: Empreinte[] }

export type Decision = 'annuler' | 'accepter-une-fois' | 'enregistrer-et-accepter' | 'remplacer'

export interface Etape { libelle: string; etat: EtatEtape; detail: string; duree: string }

export interface LigneJournal { heure: string; niveau: 'INFO' | 'AVERT' | 'ERREUR'; source: string; message: string }

export type Section = 'connexion' | 'auth' | 'options' | 'notes'

const LIBELLES = ['Résolution du nom', 'Connexion TCP', 'Négociation des algorithmes', "Vérification de la clé d'hôte", 'Authentification', 'Ouverture de la session']
const SOURCES = ['ssh', 'ssh', 'kex', 'hostkey', 'auth', 'session']

function profilVide(): Profil {
  return {
    id: crypto.randomUUID(), nom: 'nouveau-profil', groupe: '', hote: '', port: 22, utilisateur: '', methode: 'cle', cheminCle: '',
    delaiConnexion: 15, keepalive: true, intervalleKeepalive: 30, notes: '', derniereConnexion: null, tunnels: [], proxy: { genre: 'aucun', hote: '', port: 1080 }, rebond: '', memoriser: false,
    agent: { niveau: 'aucun', commandes: [], dossiers: [] },
  }
}

function etapesInitiales(): Etape[] {
  return LIBELLES.map((libelle) => ({ libelle, etat: 'a-venir', detail: '', duree: '' }))
}

function heure(): string {
  const d = new Date()
  const n = (v: number, l = 2) => String(v).padStart(l, '0')
  return `${n(d.getHours())}:${n(d.getMinutes())}:${n(d.getSeconds())}.${n(d.getMilliseconds(), 3)}`
}

class EtatApp {
  chargement = $state<'chargement' | 'pret' | 'erreur'>('chargement')
  erreurChargement = $state<ErreurProfils | null>(null)
  chemin = $state('')
  profils = $state<Profil[]>([])
  selection = $state<string | null>(null)
  modifies = $state<Record<string, boolean>>({})
  recherche = $state('')
  section = $state<Section>('connexion')
  ecran = $state<'profils' | 'cles' | 'hotes'>('profils')
  secret = $state('')
  secretRebond = $state('')
  capacites = $state({ mobile: false, agent: false, trousseau: false })
  tactile = $state(false)
  etroit = $state(false)
  clavierOuvert = $state(false)
  /** Sur écran étroit, le détail du profil remplace la liste. */
  detail = $state(false)
  enTrousseau = $state<Record<string, boolean>>({})
  questionClavier = $state<QuestionClavier | null>(null)
  messageEnregistrement = $state<string | null>(null)

  vue = $state<'formulaire' | 'suivi'>('formulaire')
  phase = $state<'repos' | 'connexion' | 'connecte' | 'erreur'>('repos')
  etapes = $state<Etape[]>(etapesInitiales())
  erreur = $state<ErreurConnexion | null>(null)
  question = $state<Question | null>(null)
  sessionProfil = $state<string | null>(null)
  etatSession = $state<'ok' | 'perdue' | 'reconnexion'>('ok')
  raisonPerte = $state('')
  reconnecte = $state(false)
  journal = $state<LigneJournal[]>([])
  journalOuvert = $state(true)
  confirmerSortie = $state(false)
  suppression = $state(false)

  #debutEtape = new Map<number, number>()
  // Reste référencé : ce canal signale aussi la perte de la session, après la connexion.
  #canal: Channel<Evenement> | null = null

  courant = $derived(this.profils.find((p) => p.id === this.selection) ?? null)
  erreurHote = $derived(this.courant ? !this.courant.hote.trim() : false)
  erreurPort = $derived(this.courant ? !(this.courant.port >= 1 && this.courant.port <= 65535) : false)
  champsInvalides = $derived(Number(this.erreurHote) + Number(this.erreurPort))
  peutConnecter = $derived(
    !!this.courant && this.champsInvalides === 0 && this.phase !== 'connexion' && this.phase !== 'connecte' &&
    !!this.courant.utilisateur.trim() &&
    (this.courant.methode === 'clavier' || this.courant.methode === 'agent' || (this.courant.methode === 'mot-de-passe' ? this.secret.length > 0 || this.secretMemorise(this.courant) : !!this.courant.cheminCle.trim())) &&
    (this.courant.proxy.genre === 'aucun' || (!!this.courant.proxy.hote.trim() && this.courant.proxy.port >= 1)),
  )

  secretMemorise(p: Profil) {
    return p.memoriser && !!this.enTrousseau[p.id]
  }

  async #relireTrousseau() {
    if (!this.capacites.trousseau) return
    const ids = this.profils.filter((p) => p.memoriser).map((p) => p.id)
    const presents = await invoke<string[]>('trousseau_presents', { ids }).catch(() => [] as string[])
    this.enTrousseau = Object.fromEntries(presents.map((id) => [id, true]))
  }

  /** Décoché : le secret est effacé tout de suite, sans attendre l'enregistrement du profil. */
  async changerMemorisation(p: Profil, valeur: boolean) {
    p.memoriser = valeur
    this.marquerModifie()
    if (!valeur) {
      await invoke('trousseau_oublier', { id: p.id }).catch(() => {})
      delete this.enTrousseau[p.id]
    }
  }

  async charger() {
    this.chargement = 'chargement'
    try {
      this.capacites = await invoke<{ mobile: boolean; agent: boolean; trousseau: boolean }>('capacites')
      const r = await invoke<{ profils: Profil[]; chemin: string }>('profils_charger')
      this.profils = r.profils
      this.chemin = r.chemin
      this.selection = r.profils[0]?.id ?? null
      await this.#relireTrousseau()
      this.chargement = 'pret'
    } catch (e) {
      this.erreurChargement = e as ErreurProfils
      this.chargement = 'erreur'
    }
  }

  selectionner(id: string) {
    if (this.phase === 'connexion') return
    this.selection = id
    this.detail = true
    this.secret = ''
    this.secretRebond = ''
    if (this.phase !== 'connecte') this.fermerSuivi()
  }

  nouveau() {
    const p = profilVide()
    this.profils.push(p)
    this.modifies[p.id] = true
    this.selection = p.id
    this.detail = true
    this.section = 'connexion'
    this.secret = ''
    this.vue = 'formulaire'
  }

  marquerModifie() {
    if (this.selection) this.modifies[this.selection] = true
    this.messageEnregistrement = null
  }

  async enregistrer() {
    if (this.champsInvalides > 0) return
    // Effacé avant l'écriture : une modification faite pendant celle-ci reste signalée.
    const envoyes = this.modifies
    this.modifies = {}
    try {
      await invoke('profils_enregistrer', { profils: $state.snapshot(this.profils) })
      this.messageEnregistrement = null
    } catch (e) {
      this.modifies = { ...envoyes, ...this.modifies }
      const erreur = e as ErreurProfils
      this.messageEnregistrement = erreur.detail ?? erreur.message
    }
  }

  async supprimerCourant() {
    const i = this.profils.findIndex((p) => p.id === this.selection)
    if (i < 0) return
    const [retire] = this.profils.splice(i, 1)
    if (retire.memoriser) void invoke('trousseau_oublier', { id: retire.id }).catch(() => {})
    delete this.enTrousseau[retire.id]
    this.selection = this.profils[Math.min(i, this.profils.length - 1)]?.id ?? null
    this.suppression = false
    await this.enregistrer()
  }

  #journaliser(niveau: LigneJournal['niveau'], source: string, message: string) {
    this.journal.push({ heure: heure(), niveau, source, message })
  }

  /** Évènement venu du terminal, du SFTP ou d'un tunnel. Ne jamais y placer de secret. */
  consigner(niveau: LigneJournal['niveau'], source: string, message: string) {
    this.#journaliser(niveau, source, message)
  }

  #nouveauCanal() {
    const canal = new Channel<Evenement>()
    canal.onmessage = (e) => this.#recevoir(e)
    this.#canal = canal
    return canal
  }

  #perdreSession(raison: string) {
    if (this.phase !== 'connecte' || this.etatSession !== 'ok') return
    this.etatSession = 'perdue'
    this.raisonPerte = raison
    session.marquerTousFermes()
    tunnels.suspendre()
    this.#journaliser('ERREUR', 'session', raison)
  }

  #recevoir(e: Evenement) {
    if (e.type === 'session-perdue') {
      this.#perdreSession(e.raison)
      return
    }
    if (e.type === 'question-clavier') {
      this.questionClavier = e
      return
    }
    if (e.type !== 'etape') {
      this.question = e
      return
    }
    const etape = this.etapes[e.index]
    if (e.etat === 'en-cours') this.#debutEtape.set(e.index, performance.now())
    etape.etat = e.etat
    if (e.detail) etape.detail = e.detail
    if (e.etat === 'fait') {
      const debut = this.#debutEtape.get(e.index)
      etape.duree = debut === undefined ? '' : `${((performance.now() - debut) / 1000).toFixed(2).replace('.', ',')} s`
      this.#journaliser('INFO', SOURCES[e.index], etape.detail ? `${etape.libelle} — ${etape.detail}` : etape.libelle)
    } else if (e.etat === 'attente') {
      etape.duree = 'en attente'
      this.#journaliser('AVERT', SOURCES[e.index], `${etape.libelle} — ${e.detail}`)
    } else if (e.etat === 'erreur') {
      etape.duree = 'échec'
      this.#journaliser('ERREUR', SOURCES[e.index], `${etape.libelle} — ${e.detail}`)
    } else {
      etape.duree = 'en cours'
    }
  }

  async connecter() {
    const profil = this.courant
    if (!profil || !this.peutConnecter) return
    this.etapes = etapesInitiales()
    this.#debutEtape.clear()
    this.erreur = null
    this.question = null
    this.journal = []
    this.vue = 'suivi'
    this.phase = 'connexion'
    this.journalOuvert = true
    try {
      const avertissement = await invoke<string | null>('connecter', { profil: $state.snapshot(profil), secret: this.secret || null, secretRebond: this.secretRebond || null, canal: this.#nouveauCanal() })
      this.secret = ''
      this.secretRebond = ''
      if (avertissement) this.#journaliser('AVERT', 'trousseau', avertissement)
      void this.#relireTrousseau()
      this.etatSession = 'ok'
      this.phase = 'connecte'
      this.sessionProfil = profil.id
      session.demarrer()
      void tunnels.demarrer(profil)
      this.#journaliser('INFO', 'session', 'Session ouverte')
      profil.derniereConnexion = new Date().toISOString()
      const etaitModifie = this.modifies[profil.id]
      if (!etaitModifie) await this.enregistrer()
    } catch (e) {
      this.question = null
      this.questionClavier = null
      const erreur = e as ErreurConnexion
      if (erreur.code === 'annulee') {
        this.#journaliser('AVERT', 'ssh', 'Connexion annulée')
        this.phase = 'repos'
        this.fermerSuivi()
      } else {
        this.erreur = erreur
        this.phase = 'erreur'
      }
    }
  }

  decider(decision: Decision) {
    this.question = null
    void invoke('decider', { decision })
  }

  /** null abandonne la connexion en cours : une réponse partielle n'a pas de sens. */
  repondreClavier(reponses: string[] | null) {
    this.questionClavier = null
    void invoke('repondre_clavier', { reponses })
  }

  annuler() {
    this.question = null
    this.questionClavier = null
    void invoke('annuler_connexion')
  }

  fermerSuivi() {
    if (this.phase === 'connexion') return
    this.vue = 'formulaire'
    if (this.phase === 'erreur') this.phase = 'repos'
  }

  /** Le secret reste côté Rust, en mémoire, pour la durée de la session. */
  async reconnecter() {
    if (this.phase !== 'connecte' || this.etatSession !== 'perdue') return
    this.etatSession = 'reconnexion'
    this.etapes = etapesInitiales()
    try {
      await invoke('reconnecter', { canal: this.#nouveauCanal() })
      this.etatSession = 'ok'
      this.reconnecte = true
      setTimeout(() => (this.reconnecte = false), 3500)
      this.#journaliser('INFO', 'session', 'Session rétablie')
      await Promise.all([session.rouvrirFermes(), tunnels.rouvrir()])
    } catch (e) {
      const erreur = e as ErreurConnexion
      this.question = null
      this.questionClavier = null
      this.etatSession = 'perdue'
      this.raisonPerte = erreur.detail ? `${erreur.titre} : ${erreur.detail}.` : `${erreur.titre}.`
      this.#journaliser('ERREUR', 'session', this.raisonPerte)
    }
  }

  async deconnecter() {
    await invoke('deconnecter')
    session.vider()
    sftp.reinitialiser()
    tunnels.vider()
    this.phase = 'repos'
    this.etatSession = 'ok'
    this.sessionProfil = null
    this.vue = 'formulaire'
    this.#journaliser('INFO', 'session', "Session fermée par l'utilisateur")
  }

  /** Ctrl+Shift+F7 et l'entrée de la barre des tâches. */
  async basculerConnexion() {
    if (this.phase === 'connecte') {
      await this.deconnecter()
    } else if (this.phase !== 'connexion') {
      await invoke('afficher_fenetre')
      await this.connecter()
    }
  }

  async quitter() {
    await invoke('quitter')
  }
}

export const app = new EtatApp()
