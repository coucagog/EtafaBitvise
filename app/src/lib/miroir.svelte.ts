import { invoke } from '@tauri-apps/api/core'
import { app } from './etat.svelte'
import { type Cote, type Entree, sftp } from './sftp.svelte'

export interface Copie { relatif: string; nom: string; source: string; dossier: string; taille: number; nouveau: boolean }
export interface Suppression { relatif: string; chemin: string; dossier: boolean }
export interface Plan {
  depuis: Cote
  racineSource: string
  racineCible: string
  dossiers: string[]
  copies: Copie[]
  suppressions: Suppression[]
  identiques: number
  ignores: string[]
}

const MAX_ENTREES = 50000
const TOLERANCE_DATE = 2
const prefixe = (c: Cote) => (c === 'local' ? 'local' : 'sftp')
const autre = (c: Cote): Cote => (c === 'local' ? 'distant' : 'local')
const lister = (c: Cote, chemin: string) => invoke<Entree[]>(`${prefixe(c)}_lister`, { chemin })

function joindre(base: string, nom: string) {
  const separateur = base.includes('\\') ? '\\' : '/'
  return base.endsWith(separateur) ? `${base}${nom}` : `${base}${separateur}${nom}`
}

/** Un serveur ne doit pas pouvoir faire sortir une écriture du dossier choisi. */
const nomSur = (nom: string) => !!nom && nom !== '.' && nom !== '..' && !/[/\\]/.test(nom)
const estRacine = (c: string) => c === '' || c === '/' || /^[A-Za-z]:[\\/]?$/.test(c)

export class Annule extends Error {}

/**
 * Compare deux arborescences. Toute lecture en échec arrête la comparaison : un dossier
 * illisible ne doit jamais faire passer ses fichiers pour absents, donc à supprimer.
 */
export async function comparer(depuis: Cote, racineSource: string, racineCible: string, annule: () => boolean, progres: (n: number) => void): Promise<Plan> {
  const cible = autre(depuis)
  const plan: Plan = { depuis, racineSource, racineCible, dossiers: [], copies: [], suppressions: [], identiques: 0, ignores: [] }
  // Sous Windows, « Foo » et « foo » désignent le même fichier.
  const insensible = racineSource.includes('\\') || racineCible.includes('\\')
  const cle = (nom: string) => (insensible ? nom.toLowerCase() : nom)
  let vus = 0

  const pile: { source: string; cible: string; relatif: string; existe: boolean }[] = [{ source: racineSource, cible: racineCible, relatif: '', existe: true }]
  while (pile.length) {
    if (annule()) throw new Annule()
    const d = pile.pop()!
    const sources = await lister(depuis, d.source)
    const cibles = d.existe ? await lister(cible, d.cible) : []
    vus += sources.length + cibles.length
    progres(vus)
    if (vus > MAX_ENTREES) throw new Error(`Plus de ${MAX_ENTREES} éléments : choisissez un dossier plus petit.`)

    const parNom = new Map(cibles.map((e) => [cle(e.nom), e]))
    const presents = new Set<string>()
    const enfants: typeof pile = []
    for (const e of sources) {
      const relatif = d.relatif ? `${d.relatif}/${e.nom}` : e.nom
      presents.add(cle(e.nom))
      if (!nomSur(e.nom)) {
        plan.ignores.push(`${relatif} : nom refusé`)
        continue
      }
      if (e.genre !== 'dossier' && e.genre !== 'fichier') {
        plan.ignores.push(`${relatif} : lien ou fichier spécial`)
        continue
      }
      const existant = parNom.get(cle(e.nom))
      if (existant && existant.genre !== e.genre) {
        plan.ignores.push(`${relatif} : type différent de chaque côté`)
        continue
      }
      if (e.genre === 'dossier') {
        const chemin = joindre(d.cible, e.nom)
        if (!existant) plan.dossiers.push(chemin)
        enfants.push({ source: e.chemin, cible: chemin, relatif, existe: !!existant })
      } else if (!existant) {
        plan.copies.push({ relatif, nom: e.nom, source: e.chemin, dossier: d.cible, taille: e.taille, nouveau: true })
      } else if (e.taille !== existant.taille || (e.modifie !== null && existant.modifie !== null && e.modifie > existant.modifie + TOLERANCE_DATE)) {
        plan.copies.push({ relatif, nom: e.nom, source: e.chemin, dossier: d.cible, taille: e.taille, nouveau: false })
      } else {
        plan.identiques++
      }
    }
    for (const e of cibles) {
      if (presents.has(cle(e.nom))) continue
      const relatif = d.relatif ? `${d.relatif}/${e.nom}` : e.nom
      if (e.genre === 'dossier' || e.genre === 'fichier') plan.suppressions.push({ relatif, chemin: e.chemin, dossier: e.genre === 'dossier' })
      else plan.ignores.push(`${relatif} : lien ou fichier spécial conservé`)
    }
    // Ordre de parcours stable : un parent est toujours créé avant ses enfants.
    pile.push(...enfants.reverse())
  }
  return plan
}

export const suppressionPermise = (plan: Plan) => !estRacine(plan.racineCible)

const TERMINE = ['fini', 'ignore', 'erreur', 'annule']

/** Les suppressions n'ont lieu qu'une fois toutes les copies réussies. */
export async function appliquer(plan: Plan, supprimer: boolean) {
  const cible = autre(plan.depuis)
  sftp.noter(`Miroir : ${plan.copies.length} copie${plan.copies.length > 1 ? 's' : ''}${supprimer ? `, ${plan.suppressions.length} suppression${plan.suppressions.length > 1 ? 's' : ''}` : ''} vers ${plan.racineCible}`)
  for (const dossier of plan.dossiers) {
    try {
      await invoke(`${prefixe(cible)}_creer_dossier`, { chemin: dossier })
    } catch (e) {
      sftp.noter(`Miroir arrêté, dossier non créé (${dossier}) : ${String(e)}`, true)
      return
    }
  }
  const lignes = sftp.ajouterCopies(plan.depuis === 'local', plan.copies)
  while (!lignes.every((l) => TERMINE.includes(l.etat))) {
    if (app.etatSession !== 'ok' || lignes.some((l) => !sftp.lignes.includes(l))) {
      sftp.noter('Miroir interrompu : aucune suppression effectuée', true)
      return
    }
    await new Promise((r) => setTimeout(r, 250))
  }
  const echecs = lignes.filter((l) => l.etat === 'erreur' || l.etat === 'annule').length
  if (echecs) {
    sftp.noter(`Miroir : ${echecs} copie${echecs > 1 ? 's' : ''} en échec, aucune suppression effectuée`, true)
  } else if (supprimer && suppressionPermise(plan)) {
    for (const s of plan.suppressions) {
      try {
        await invoke(`${prefixe(cible)}_supprimer`, { chemin: s.chemin })
        sftp.noter(`Miroir : ${s.relatif} supprimé`)
      } catch (e) {
        sftp.noter(`Miroir : ${s.relatif} non supprimé : ${String(e)}`, true)
      }
    }
  }
  sftp.noter('Miroir terminé')
  await sftp.panneau(cible).actualiser()
}
