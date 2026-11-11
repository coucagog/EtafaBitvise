import { invoke } from '@tauri-apps/api/core'
import { app, type Profil } from './etat.svelte'

export type GenreTunnel = 'local' | 'distant' | 'socks' | 'http'
export interface Regle {
  id: string; nom: string; genre: GenreTunnel; ecouteHote: string; ecoutePort: number; cibleHote: string; ciblePort: number; auto: boolean
}
interface Mesure { actives: number; descendu: number; monte: number; derniereErreur: string | null }
export interface ErreurTunnel { code: 'invalide' | 'port-occupe' | 'refuse' | 'session'; message: string }
export type EtatTunnel = 'ferme' | 'ouverture' | 'actif' | 'erreur' | 'suspendu'

const LOCALES = ['127.0.0.1', 'localhost', '::1']
export const estExpose = (r: Regle) => !LOCALES.includes(r.ecouteHote.trim() || '127.0.0.1')

export class LigneTunnel {
  regle = $state<Regle>() as Regle
  etat = $state<EtatTunnel>('ferme')
  activee = $state(false)
  erreur = $state<ErreurTunnel | null>(null)
  avis = $state<string | null>(null)
  actives = $state(0)
  descendu = $state(0)
  monte = $state(0)
  debitDescendu = $state(0)
  debitMonte = $state(0)

  constructor(regle: Regle) {
    this.regle = regle
  }

  razMesures() {
    this.actives = 0; this.debitDescendu = 0; this.debitMonte = 0
  }
}

const INTERVALLE_MESURES = 1000

class EtatTunnels {
  lignes = $state<LigneTunnel[]>([])
  #profil: Profil | null = null
  #minuteur: ReturnType<typeof setInterval> | undefined

  enErreur = $derived(this.lignes.filter((l) => l.etat === 'erreur').length)
  exposees = $derived(this.lignes.filter((l) => l.activee && l.etat === 'actif' && estExpose(l.regle)))

  async demarrer(profil: Profil) {
    this.vider()
    this.#profil = profil
    this.lignes = (profil.tunnels ?? []).map((r) => new LigneTunnel({ ...r }))
    this.#minuteur = setInterval(() => void this.#mesurer(), INTERVALLE_MESURES)
    await Promise.all(this.lignes.filter((l) => l.regle.auto).map((l) => this.ouvrir(l)))
  }

  vider() {
    clearInterval(this.#minuteur)
    this.lignes = []
    this.#profil = null
  }

  async #mesurer() {
    if (!this.lignes.some((l) => l.etat === 'actif')) return
    const mesures = await invoke<Record<string, Mesure>>('tunnel_mesures').catch(() => null)
    if (!mesures) return
    for (const l of this.lignes) {
      const m = mesures[l.regle.id]
      if (!m) continue
      l.debitDescendu = (Math.max(0, m.descendu - l.descendu) * 1000) / INTERVALLE_MESURES
      l.debitMonte = (Math.max(0, m.monte - l.monte) * 1000) / INTERVALLE_MESURES
      l.actives = m.actives
      l.descendu = m.descendu
      l.monte = m.monte
      l.avis = m.derniereErreur
    }
  }

  async ouvrir(l: LigneTunnel) {
    l.activee = true
    l.etat = 'ouverture'
    l.erreur = null
    l.avis = null
    l.descendu = 0
    l.monte = 0
    l.razMesures()
    try {
      await invoke('tunnel_ouvrir', { regle: $state.snapshot(l.regle) })
      l.etat = 'actif'
      app.consigner('INFO', 'tunnel', `${l.regle.nom} ouvert`)
    } catch (e) {
      l.erreur = e as ErreurTunnel
      l.etat = 'erreur'
      app.consigner('ERREUR', 'tunnel', `${l.regle.nom} : ${l.erreur.message}`)
    }
  }

  async fermer(l: LigneTunnel) {
    l.activee = false
    l.etat = 'ferme'
    l.erreur = null
    l.razMesures()
    app.consigner('INFO', 'tunnel', `${l.regle.nom} fermé`)
    await invoke('tunnel_fermer', { id: l.regle.id })
  }

  basculer(l: LigneTunnel) {
    return l.activee ? this.fermer(l) : this.ouvrir(l)
  }

  /** La session est tombée : les écoutes ne servent plus, les règles restent. */
  suspendre() {
    for (const l of this.lignes) {
      if (l.activee) l.etat = 'suspendu'
      l.razMesures()
    }
  }

  async rouvrir() {
    await Promise.all(this.lignes.filter((l) => l.activee).map((l) => this.ouvrir(l)))
  }

  async appliquer(regle: Regle) {
    let l = this.lignes.find((x) => x.regle.id === regle.id)
    if (l) l.regle = regle
    else {
      l = new LigneTunnel(regle)
      this.lignes.push(l)
    }
    await this.#memoriser()
    await this.ouvrir(l)
  }

  async supprimer(l: LigneTunnel) {
    await this.fermer(l)
    this.lignes.splice(this.lignes.indexOf(l), 1)
    await this.#memoriser()
  }

  async #memoriser() {
    if (!this.#profil) return
    const regles = this.lignes.map((l) => $state.snapshot(l.regle) as Regle)
    this.#profil.tunnels = regles
    // Un profil encore jamais enregistré garde ses règles en mémoire, avec le reste du brouillon.
    await invoke('profil_tunnels', { id: this.#profil.id, regles }).catch(() => false)
  }
}

export const tunnels = new EtatTunnels()
