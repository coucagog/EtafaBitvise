<script lang="ts">
  import { Circle, CircleCheck, CircleX, LoaderCircle, RotateCcw, ShieldAlert } from '@lucide/svelte'
  import { app, type EtatEtape, type Profil } from './etat.svelte'

  let { p }: { p: Profil } = $props()

  const faites = $derived(app.etapes.filter((e) => e.etat === 'fait').length)
  const indexCourant = $derived(Math.min(app.etapes.findIndex((e) => e.etat !== 'fait') < 0 ? 5 : app.etapes.findIndex((e) => e.etat !== 'fait'), 5))
  const pourcentage = $derived(app.phase === 'connecte' ? 100 : Math.round(((faites + 0.4) / 6) * 100))
  const titre = $derived(app.phase === 'connecte' ? `Connecté à ${p.nom}` : app.phase === 'erreur' ? `Échec de la connexion à ${p.nom}` : `Connexion à ${p.nom}…`)
  const couleurBarre = $derived(app.phase === 'erreur' ? 'var(--rec-500)' : app.phase === 'connecte' ? 'var(--ok-500)' : 'var(--warn-500)')

  const COULEURS: Record<EtatEtape, string> = { 'en-cours': 'var(--warn-500)', attente: 'var(--warn-500)', fait: 'var(--ok-500)', erreur: 'var(--danger-text)', 'a-venir': 'var(--text-faint)' }
</script>

<div class="suivi">
  <div class="tete">
    <h3>{titre}</h3>
    <span class="mono compte">{app.phase === 'connecte' ? '6 étapes' : `Étape ${indexCourant + 1} sur 6`}</span>
  </div>
  <div class="barre" role="progressbar" aria-valuenow={pourcentage} aria-valuemin="0" aria-valuemax="100">
    <div style:width={`${pourcentage}%`} style:background={couleurBarre}></div>
  </div>
  <ol aria-label="Étapes de connexion">
    {#each app.etapes as e}
      <li class={e.etat}>
        <span class="icone">
          {#if e.etat === 'en-cours'}<span class="tourne"><LoaderCircle size={14} color={COULEURS[e.etat]} aria-label="En cours" /></span>
          {:else if e.etat === 'fait'}<CircleCheck size={14} color={COULEURS[e.etat]} />
          {:else if e.etat === 'attente'}<ShieldAlert size={14} color={COULEURS[e.etat]} />
          {:else if e.etat === 'erreur'}<CircleX size={14} color={COULEURS[e.etat]} />
          {:else}<Circle size={14} color={COULEURS[e.etat]} />{/if}
        </span>
        <span class="libelles">
          <span class="libelle">{e.libelle}</span>
          {#if e.detail}<span class="detail mono">{e.detail}</span>{/if}
        </span>
        <span class="duree mono">{e.duree}</span>
      </li>
    {/each}
  </ol>

  {#if app.phase === 'erreur' && app.erreur}
    <div class="alerte" role="alert">
      <CircleX size={16} color="var(--danger-text)" />
      <div class="alerte-corps">
        <span class="alerte-titre">{app.erreur.titre}</span>
        <span>{app.erreur.texte}</span>
        {#if app.erreur.detail}<span class="mono secondaire selectionnable">{app.erreur.detail}</span>{/if}
        {#if app.erreur.conseil}<span class="secondaire">{app.erreur.conseil}</span>{/if}
        <div class="boutons">
          {#if app.erreur.code === 'authentification' || app.erreur.code === 'cle-locale'}
            <button class="btn btn-primaire" onclick={() => { app.section = 'auth'; app.fermerSuivi() }}>Modifier l'authentification</button>
          {/if}
          <button class="btn btn-secondaire" aria-disabled={!app.peutConnecter} onclick={() => app.connecter()}><RotateCcw size={14} />Réessayer</button>
          <button class="btn btn-discret" onclick={() => app.fermerSuivi()}>Fermer</button>
        </div>
      </div>
    </div>
  {:else if app.phase === 'connecte'}
    <div class="succes" role="status">
      <CircleCheck size={16} color="var(--ok-500)" />
      <span class="texte-succes"><strong>Connecté.</strong> Session prête.</span>
      <button class="btn btn-secondaire btn-compact" onclick={() => app.fermerSuivi()}>Retour au profil</button>
      <button class="btn btn-danger btn-compact" onclick={() => app.deconnecter()}>Se déconnecter</button>
    </div>
  {:else if app.phase === 'connexion'}
    <div class="annulation">
      <button class="btn btn-secondaire" onclick={() => app.annuler()}>Annuler la connexion</button>
      <span>Échap pour annuler</span>
    </div>
  {/if}
</div>

<style>
  .suivi { flex: 1; min-height: 0; overflow: auto; padding: 16px 24px 20px; display: flex; flex-direction: column; gap: 14px; border-top: 1px solid var(--border-default); }
  .suivi > * { max-width: 760px; width: 100%; }
  .tete { display: flex; align-items: center; gap: 12px; }
  h3 { margin: 0; flex: 1; font-size: 14px; font-weight: 600; }
  .compte { font-size: 11px; color: var(--text-muted); }
  .barre { height: 4px; flex-shrink: 0; border-radius: 2px; background: var(--surface-2); overflow: hidden; }
  .barre div { height: 100%; transition: width 300ms ease; }
  ol { margin: 0; padding: 0; list-style: none; border: 1px solid var(--border-default); border-radius: var(--radius-md); overflow: hidden; background: rgba(0, 0, 0, 0.2); }
  li { display: grid; grid-template-columns: 20px minmax(0, 1fr) auto; gap: 10px; align-items: center; min-height: 40px; padding: 6px 12px; border-bottom: 1px solid var(--border-subtle); }
  li:last-child { border-bottom: none; }
  li.erreur { background: rgba(239, 68, 68, 0.07); }
  li.attente, li.en-cours { background: rgba(251, 191, 36, 0.05); }
  .icone { display: inline-flex; align-items: center; justify-content: center; }
  .tourne { display: inline-flex; animation: rotation 900ms linear infinite; }
  @keyframes rotation { to { transform: rotate(360deg); } }
  .libelles { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .libelle { font-size: 12px; color: var(--text-primary); }
  .a-venir .libelle { color: var(--text-muted); }
  .erreur .libelle { color: var(--danger-text); }
  .detail { font-size: 11px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .duree { font-size: 11px; color: var(--text-muted); }
  .erreur .duree { color: var(--danger-text); }
  .attente .duree, .en-cours .duree { color: var(--warn-500); }
  .alerte { padding: 12px 14px; display: flex; gap: 10px; background: var(--danger-tint); border: 1px solid var(--danger-line); border-radius: var(--radius-md); }
  .alerte-corps { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; font-size: 12px; line-height: 1.5; }
  .alerte-titre { font-size: 13px; font-weight: 600; color: var(--danger-text); }
  .secondaire { color: var(--text-secondary); }
  .boutons { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 6px; }
  .succes { padding: 10px 12px; display: flex; align-items: center; gap: 10px; background: var(--ok-tint); border: 1px solid var(--ok-line); border-radius: var(--radius-md); font-size: 12px; }
  .texte-succes { flex: 1; }
  .texte-succes strong { font-weight: 600; color: var(--ok-500); }
  .annulation { display: flex; align-items: center; gap: 12px; font-size: 11px; color: var(--text-muted); }
</style>
