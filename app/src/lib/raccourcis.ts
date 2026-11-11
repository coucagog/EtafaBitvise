import catalogue from './raccourcis.json'

export type Portee = 'profils' | 'terminal'

/** Compare un évènement clavier à une combinaison du catalogue, comme « Ctrl+Shift+P » ou « Alt+C ». */
export function correspond(e: KeyboardEvent, touches: string): boolean {
  const parties = touches.split('+')
  const touche = parties[parties.length - 1]
  // Lettres, chiffres et touches F : position physique (sur AZERTY, la touche 0 produit « à »). Signes : caractère.
  const memeTouche = /^[A-Z]$/.test(touche)
    ? e.code === `Key${touche}`
    : /^\d$/.test(touche)
      ? e.code === `Digit${touche}` || e.code === `Numpad${touche}`
      : /^F\d+$/.test(touche)
        ? e.code === touche
        : e.key === touche
  return (
    memeTouche &&
    e.ctrlKey === parties.includes('Ctrl') &&
    e.shiftKey === parties.includes('Shift') &&
    e.altKey === parties.includes('Alt') &&
    !e.metaKey
  )
}

/** Renvoie l'identifiant du raccourci de fenêtre correspondant, dans cette portée. */
export function raccourciDeFenetre(e: KeyboardEvent, portee: Portee): string | null {
  return catalogue.fenetre.find((r) => r.portee === portee && correspond(e, r.touches))?.id ?? null
}

export function touches(id: string): string {
  return catalogue.fenetre.find((r) => r.id === id)?.touches ?? ''
}
