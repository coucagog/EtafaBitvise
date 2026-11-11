import { mount } from 'svelte'
import './styles/tokens.css'
import './styles/base.css'
import App from './App.svelte'
import './styles/tactile.css'

const agent = navigator.userAgent
if (agent.includes('Android') || agent.includes('Linux')) {
  document.documentElement.dataset.fallback = 'on'
}
// macOS : fenêtre opaque et barre de titre native superposée (tauri.macos.conf.json).
if (agent.includes('Macintosh')) {
  document.documentElement.dataset.fallback = 'on'
  document.documentElement.classList.add('mac')
}

mount(App, { target: document.getElementById('app')! })

// WebKitGTK peut laisser incomplet le premier dessin d'une fenêtre transparente (titre, onglets, pied absents) :
// un changement passager du fond de la page en provoque un second, complet.
if (agent.includes('Linux') && !agent.includes('Android')) {
  const racine = document.documentElement
  const redessiner = () => {
    racine.style.background = 'rgba(0, 0, 0, 0.01)'
    requestAnimationFrame(() => requestAnimationFrame(() => racine.style.removeProperty('background')))
  }
  addEventListener('load', () => [300, 1500].forEach((delai) => setTimeout(redessiner, delai)))
}
