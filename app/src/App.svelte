<script lang="ts">
  import { listen } from '@tauri-apps/api/event'
  import { ArrowLeft, File, KeyRound, RotateCcw, Server, ShieldCheck, CircleX } from '@lucide/svelte'
  import ApprobationAgent from './lib/ApprobationAgent.svelte'
  import BarreTitre from './lib/BarreTitre.svelte'
  import ClesVue from './lib/ClesVue.svelte'
  import HotesVue from './lib/HotesVue.svelte'
  import Dialogues from './lib/Dialogues.svelte'
  import Formulaire from './lib/Formulaire.svelte'
  import Journal from './lib/Journal.svelte'
  import ListeProfils from './lib/ListeProfils.svelte'
  import Session from './lib/Session.svelte'
  import Suivi from './lib/Suivi.svelte'
  import { cles } from './lib/cles.svelte'
  import { app } from './lib/etat.svelte'
  import { raccourciDeFenetre } from './lib/raccourcis'
  import { sftp } from './lib/sftp.svelte'

  // Android : la notification de session maintient l'application en vie en arrière-plan.
  const pontAndroid = (window as unknown as { McAndroid?: { ouverte(libelle: string): void; fermee(): void } }).McAndroid
  $effect(() => {
    if (!pontAndroid) return
    if (app.phase !== 'connecte') {
      pontAndroid.fermee()
      return
    }
    const profil = app.profils.find((p) => p.id === app.sessionProfil)
    const cible = profil ? `${profil.utilisateur}@${profil.hote}` : 'serveur'
    pontAndroid.ouverte(app.etatSession === 'ok' ? `Connecté à ${cible}` : `Connexion perdue : ${cible}`)
  })
  $effect(() => {
    if (!pontAndroid) return
    const deconnecter = () => void app.deconnecter()
    window.addEventListener('mc-deconnecter', deconnecter)
    return () => window.removeEventListener('mc-deconnecter', deconnecter)
  })

  $effect(() => {
    void app.charger()
    void cles.charger()
    const ecoutes = [
      listen<string>('mc://raccourci', (e) => {
        if (e.payload === 'connexion') void app.basculerConnexion()
        else if (e.payload === 'pause') sftp.basculerPause()
      }),
      listen('mc://quitter', () => (app.confirmerSortie = true)),
    ]
    return () => ecoutes.forEach((p) => p.then((fin) => fin()))
  })

  // Classes de mise en page, et hauteur réellement visible quand le clavier virtuel est ouvert.
  $effect(() => {
    const racine = document.documentElement
    const pointeur = matchMedia('(pointer: coarse)')
    const largeur = matchMedia('(max-width: 720px)')
    const vue = window.visualViewport
    let plafond = 0
    let largeurVue = 0
    const actualiser = () => {
      app.tactile = app.capacites.mobile || pointeur.matches
      app.etroit = largeur.matches
      racine.classList.toggle('tactile', app.tactile)
      racine.classList.toggle('etroit', app.etroit)
      racine.classList.toggle('android', app.capacites.mobile)
      if (!vue || !app.tactile) return
      if (vue.width !== largeurVue) {
        largeurVue = vue.width
        plafond = 0
      }
      plafond = Math.max(plafond, vue.height)
      racine.style.setProperty('--hauteur-visible', `${vue.height}px`)
      app.clavierOuvert = vue.height < plafond * 0.8
    }
    void app.capacites.mobile
    actualiser()
    pointeur.addEventListener('change', actualiser)
    largeur.addEventListener('change', actualiser)
    vue?.addEventListener('resize', actualiser)
    return () => {
      pointeur.removeEventListener('change', actualiser)
      largeur.removeEventListener('change', actualiser)
      vue?.removeEventListener('resize', actualiser)
    }
  })

  function ouvrirEcran(ecran: typeof app.ecran) {
    app.ecran = ecran
    if (ecran !== 'profils') void cles.charger()
  }

  function clavier(e: KeyboardEvent) {
    // En session, Ctrl+N et Ctrl+S appartiennent au shell distant.
    if (app.phase === 'connecte') return
    if (e.key === 'Escape' && app.phase === 'connexion' && !app.question) {
      app.annuler()
      return
    }
    if (app.ecran !== 'profils') return
    const id = raccourciDeFenetre(e, 'profils')
    if (id === 'nouveau') {
      e.preventDefault()
      app.nouveau()
    } else if (id === 'enregistrer') {
      e.preventDefault()
      void app.enregistrer()
    }
  }

  const nombre = $derived(`${app.profils.length} profil${app.profils.length > 1 ? 's' : ''}`)
</script>

<svelte:window onkeydown={clavier} />

{#if app.phase === 'connecte' && app.courant}
  <Session p={app.courant} />
{:else}
<div class="fenetre">
  <BarreTitre sousTitre={app.ecran === 'profils' ? 'Profils' : app.ecran === 'cles' ? 'Clés' : 'Hôtes'} />

  <div class="onglets principale" role="tablist" aria-label="Navigation principale">
    <button class="onglet" role="tab" aria-selected={app.ecran === 'profils'} onclick={() => ouvrirEcran('profils')}>
      <Server size={14} color="var(--accent-blue)" />Profils<span class="compteur">{app.profils.length}</span>
    </button>
    <button class="onglet" role="tab" aria-selected={app.ecran === 'cles'} onclick={() => ouvrirEcran('cles')}>
      <KeyRound size={14} />Clés
    </button>
    <button class="onglet" role="tab" aria-selected={app.ecran === 'hotes'} onclick={() => ouvrirEcran('hotes')}>
      <ShieldCheck size={14} />Hôtes
    </button>
  </div>

  {#if app.ecran === 'cles'}
    <section class="plein"><ClesVue /></section>
  {:else if app.ecran === 'hotes'}
    <section class="plein"><HotesVue /></section>
  {:else}
  <div class="corps">
    {#if !app.etroit || !app.detail}<ListeProfils />{/if}
    {#if !app.etroit || app.detail}
    <section>
      {#if app.chargement === 'erreur'}
        <div class="centre">
          <CircleX size={28} color="var(--danger-text)" />
          <h2>Impossible de charger les profils</h2>
          <p>{app.erreurChargement?.message}</p>
          {#if app.erreurChargement?.detail}<code class="mono selectionnable">{app.erreurChargement.detail}</code>{/if}
          <button class="btn btn-primaire" onclick={() => app.charger()}><RotateCcw size={14} />Réessayer</button>
        </div>
      {:else if app.chargement === 'pret' && !app.courant}
        <div class="centre">
          <Server size={28} color="var(--text-muted)" />
          <h2>Aucun profil</h2>
          <p>Créez un profil pour enregistrer un serveur.</p>
          <button class="btn btn-primaire" onclick={() => app.nouveau()}>Nouveau profil</button>
        </div>
      {:else if app.courant}
        {@const p = app.courant}
        <div class="entete-profil">
          {#if app.etroit}<button class="btn-icone" aria-label="Retour aux profils" onclick={() => (app.detail = false)}><ArrowLeft size={18} /></button>{/if}
          <div class="identite">
            <div class="ligne-nom">
              <h2>{p.nom}</h2>
              {#if app.modifies[p.id]}<span class="pilule"><span></span>Non enregistré</span>{/if}
            </div>
            <span class="route mono">{p.utilisateur}@{p.hote}:{p.port}</span>
          </div>
        </div>
        {#if app.vue === 'suivi'}
          <Suivi {p} />
        {:else}
          <Formulaire {p} />
        {/if}
        <Journal />
      {/if}
    </section>
    {/if}
  </div>
  {/if}

  <footer>
    <span>{nombre}</span>
    {#if app.chemin}<span class="mono chemin"><File size={12} />{app.chemin}</span>{/if}
  </footer>

  <Dialogues />
</div>
{/if}

<!-- Hors de la fenêtre des profils : une demande d'agent peut arriver pendant une session. -->
{#if !app.capacites.mobile}<ApprobationAgent />{/if}

<style>
  .fenetre {
    height: 100%; display: flex; flex-direction: column; position: relative; overflow: hidden;
    border-radius: var(--radius-lg); background: var(--surface-window);
    backdrop-filter: var(--blur-window); -webkit-backdrop-filter: var(--blur-window);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }
  .principale { height: 33px; padding: 0 8px; border-bottom: 1px solid var(--border-default); }
  .corps { flex: 1; min-height: 0; display: grid; grid-template-columns: 280px minmax(0, 1fr); }
  section { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .plein { flex: 1; }
  :global(html.etroit) .corps { grid-template-columns: minmax(0, 1fr); }
  .entete-profil { flex-shrink: 0; display: flex; align-items: center; gap: 12px; padding: 14px 20px 12px; }
  .identite { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .ligne-nom { display: flex; align-items: center; gap: 8px; }
  h2 { margin: 0; font-size: 15px; font-weight: 600; }
  .pilule { height: 20px; padding: 0 8px; display: inline-flex; align-items: center; gap: 5px; border-radius: var(--radius-pill); background: var(--surface-2); border: 1px solid var(--border-default); font-size: 11px; color: var(--text-secondary); }
  .pilule span { width: 6px; height: 6px; border-radius: 50%; background: var(--text-secondary); }
  .route { font-size: 12px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .centre { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; padding: 24px; text-align: center; }
  .centre h2 { margin-top: 4px; font-size: 14px; }
  .centre p { margin: 0; max-width: 420px; font-size: 12px; line-height: 1.5; color: var(--text-muted); }
  .centre code { padding: 4px 8px; background: rgba(0, 0, 0, 0.35); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); font-size: 12px; color: var(--danger-text); }
  footer { height: var(--footer-h); flex: 0 0 var(--footer-h); display: flex; align-items: center; gap: 16px; padding: 0 12px; background: rgba(0, 0, 0, 0.3); border-top: 1px solid var(--border-subtle); font-size: 11px; color: var(--text-muted); }
  footer span { display: inline-flex; align-items: center; gap: 6px; }
  .chemin { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
</style>
