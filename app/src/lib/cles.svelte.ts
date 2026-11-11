import { invoke } from '@tauri-apps/api/core'

export interface InfoCle {
  nom: string
  chemin: string
  algorithme: string
  empreinte: string
  commentaire: string
  protegee: boolean
  publique: string
}

export interface HoteConnu {
  motif: string
  hote: string
  port: number
  algorithme: string
  empreinte: string
}

export type GenreCle = 'ed25519' | 'rsa'
export interface ErreurCle { code: string; message: string }

class Cles {
  liste = $state<InfoCle[]>([])
  hotes = $state<HoteConnu[]>([])
  erreur = $state<string | null>(null)

  async charger() {
    try {
      this.liste = await invoke<InfoCle[]>('cles_lister')
      this.hotes = await invoke<HoteConnu[]>('hotes_lister')
      this.erreur = null
    } catch (e) {
      this.erreur = (e as ErreurCle).message
    }
  }

  async generer(nom: string, genre: GenreCle, commentaire: string, phrase: string) {
    await invoke('cles_generer', { nom, genre, commentaire, phrase: phrase || null })
    await this.charger()
  }

  async importer(chemin: string, phrase: string | null) {
    await invoke('cles_importer', { chemin, phrase })
    await this.charger()
  }

  async supprimer(nom: string) {
    await invoke('cles_supprimer', { nom })
    await this.charger()
  }

  exporter(nom: string, destination: string) {
    return invoke<void>('cle_exporter_publique', { nom, destination })
  }

  async oublierHote(h: HoteConnu) {
    await invoke('hotes_supprimer', { motif: h.motif, empreinte: h.empreinte })
    await this.charger()
  }
}

export const cles = new Cles()
