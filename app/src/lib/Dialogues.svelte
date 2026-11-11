<script lang="ts">
  import { Check, Copy, ShieldAlert, ShieldX, Trash2, TriangleAlert } from '@lucide/svelte'
  import Boite from './Boite.svelte'
  import Empreinte from './Empreinte.svelte'
  import { app, NOM_APP, type Profil } from './etat.svelte'

  const FICHIERS: Record<string, string> = { 'ssh-ed25519': 'ed25519', 'ecdsa-sha2-nistp256': 'ecdsa', 'ecdsa-sha2-nistp384': 'ecdsa', 'ecdsa-sha2-nistp521': 'ecdsa', 'ssh-rsa': 'rsa' }
  const fichierCle = (algorithme: string) => `/etc/ssh/ssh_host_${FICHIERS[algorithme] ?? 'ed25519'}_key.pub`

  let verifie = $state(false)
  let copie = $state(false)
  let saisieSuppression = $state('')
  let reponses = $state<string[]>([])

  async function copier(texte: string) {
    await navigator.clipboard.writeText(texte)
    copie = true
    setTimeout(() => (copie = false), 1600)
  }

  $effect(() => {
    if (app.question) {
      verifie = false
      copie = false
    }
  })

  $effect(() => {
    if (app.suppression) saisieSuppression = ''
  })

  $effect(() => {
    reponses = (app.questionClavier?.invites ?? []).map(() => '')
  })

  function validerClavier(e: Event) {
    e.preventDefault()
    const envoi = [...reponses]
    reponses = []
    app.repondreClavier(envoi)
  }

  const adresse = (hote: string, port: number) => `${hote}:${port}`
  const nom = $derived((app.courant as Profil | null)?.nom ?? '')
</script>

{#if app.questionClavier}
  {@const q = app.questionClavier}
  <Boite largeur={440} etiquette="Le serveur demande une saisie" surAnnuler={() => app.repondreClavier(null)}>
    <form onsubmit={validerClavier}>
      <div class="boite-entete"><h3>{q.nom || 'Le serveur demande une saisie'}</h3></div>
      <div class="boite-corps">
        {#if q.instructions}<p class="consigne">{q.instructions}</p>{/if}
        {#each q.invites as invite, i}
          <label class="invite">
            <span>{invite.texte}</span>
            <input class="champ mono" type={invite.masque ? 'password' : 'text'} autocomplete="off" spellcheck="false" data-focus-initial={i === 0 ? '' : undefined} bind:value={reponses[i]} />
          </label>
        {/each}
      </div>
      <div class="boite-pied">
        <button type="button" class="btn btn-discret" onclick={() => app.repondreClavier(null)}>Annuler</button>
        <button type="submit" class="btn btn-primaire">Valider</button>
      </div>
    </form>
  </Boite>
{/if}

{#if app.question?.type === 'cle-inconnue'}
  {@const q = app.question}
  <Boite largeur={520} bordure="var(--warn-line)" etiquette="Clé d'hôte inconnue" surAnnuler={() => app.decider('annuler')}>
    <div class="boite-entete"><ShieldAlert size={18} color="var(--warn-500)" /><h3>Clé d'hôte inconnue</h3></div>
    <div class="boite-corps">
      <p>Première connexion à <span class="fort">{nom}</span> (<span class="mono">{adresse(q.hote, q.port)}</span>). Comparez l'empreinte ci-dessous avec celle communiquée par l'administrateur du serveur avant d'accepter.</p>
      <div class="cadre">
        <div class="cadre-tete">
          <span class="titre-groupe">Empreinte SHA-256 · {q.presentee.algorithme}</span>
          <span class="espace"></span>
          <button class="copier" onclick={() => copier(q.presentee.empreinte)}>
            {#if copie}<Check size={12} />Copiée{:else}<Copy size={12} />Copier{/if}
          </button>
        </div>
        <Empreinte valeur={q.presentee.empreinte} />
      </div>
      <div class="commande">
        <span>Pour l'afficher sur le serveur :</span>
        <code class="mono selectionnable">ssh-keygen -lf {fichierCle(q.presentee.algorithme)}</code>
      </div>
    </div>
    <div class="boite-pied">
      <!-- Focus initial sur Annuler : Entrée ne doit pas faire confiance à une clé non lue. -->
      <button class="btn btn-discret" data-focus-initial onclick={() => app.decider('annuler')}>Annuler</button>
      <button class="btn btn-secondaire" onclick={() => app.decider('accepter-une-fois')}>Accepter une fois</button>
      <button class="btn btn-primaire" onclick={() => app.decider('enregistrer-et-accepter')}>Enregistrer et accepter</button>
    </div>
  </Boite>
{:else if app.question?.type === 'cle-modifiee'}
  {@const q = app.question}
  <Boite largeur={600} bordure="var(--rec-500)" ombre="0 0 0 4px rgba(239, 68, 68, 0.15)" etiquette="La clé d'hôte a changé" surAnnuler={() => app.decider('annuler')}>
    <div class="alerte-tete">
      <ShieldX size={20} color="var(--danger-text)" />
      <div class="alerte-titres">
        <h3>La clé d'hôte de {nom} a changé</h3>
        <span>Connexion bloquée · aucune donnée envoyée</span>
      </div>
    </div>
    <div class="boite-corps haut">
      <p>La clé présentée par <span class="mono fort">{adresse(q.hote, q.port)}</span> ne correspond pas à celle enregistrée. Cela peut venir d'une réinstallation du serveur, mais aussi d'une interception de la connexion. Ne continuez pas sans avoir vérifié la nouvelle empreinte par un autre canal.</p>
      <div class="comparaison">
        {#each q.enregistrees as ancienne}
          <div class="cote ancien"><strong>Enregistrée</strong><small>{ancienne.algorithme}</small></div>
          <div class="valeur ancien"><Empreinte valeur={ancienne.empreinte} taille={14} /></div>
        {/each}
        <div class="cote nouveau"><strong>Présentée maintenant</strong><small>{q.presentee.algorithme}</small></div>
        <div class="valeur nouveau"><Empreinte valeur={q.presentee.empreinte} taille={14} /></div>
      </div>
      <button class="case" role="checkbox" aria-checked={verifie} onclick={() => (verifie = !verifie)}>
        <span>{#if verifie}<Check size={12} />{/if}</span>
        <span>J'ai vérifié la nouvelle empreinte auprès de l'administrateur du serveur.</span>
      </button>
    </div>
    <div class="boite-pied">
      <button class="btn btn-discret" onclick={() => copier([...q.enregistrees.map((e) => `Enregistrée (${e.algorithme}) : ${e.empreinte}`), `Présentée (${q.presentee.algorithme}) : ${q.presentee.empreinte}`].join('\n'))}>
        {#if copie}<Check size={14} />Copiées{:else}<Copy size={14} />Copier les empreintes{/if}
      </button>
      <span class="espace"></span>
      <button class="btn btn-secondaire" data-focus-initial onclick={() => app.decider('annuler')}>Annuler la connexion</button>
      {#if verifie}
        <button class="btn btn-danger" onclick={() => app.decider('remplacer')}>Remplacer la clé et se connecter</button>
      {:else}
        <button class="btn" aria-disabled="true" title="Cochez d'abord la vérification">Remplacer la clé et se connecter</button>
      {/if}
    </div>
  </Boite>
{/if}

{#if app.suppression}
  <Boite largeur={440} bordure="var(--danger-line)" etiquette="Supprimer le profil" surAnnuler={() => (app.suppression = false)}>
    <div class="boite-entete"><TriangleAlert size={18} color="var(--danger-text)" /><h3>Supprimer le profil « {nom} » ?</h3></div>
    <div class="boite-corps">
      <p>Le profil sera supprimé. Les clés client et la clé d'hôte enregistrée sont conservées.</p>
      <label class="confirmation">
        <span>Saisissez <span class="mono fort">{nom}</span> pour confirmer</span>
        <input class="champ mono" data-focus-initial bind:value={saisieSuppression} />
      </label>
    </div>
    <div class="boite-pied">
      <button class="btn btn-secondaire" onclick={() => (app.suppression = false)}>Annuler</button>
      {#if saisieSuppression === nom}
        <button class="btn btn-danger" onclick={() => app.supprimerCourant()}><Trash2 size={14} />Supprimer le profil</button>
      {:else}
        <button class="btn" aria-disabled="true"><Trash2 size={14} />Supprimer le profil</button>
      {/if}
    </div>
  </Boite>
{/if}

{#if app.confirmerSortie}
  <Boite largeur={440} bordure="var(--warn-line)" etiquette="Quitter {NOM_APP}" surAnnuler={() => (app.confirmerSortie = false)}>
    <div class="boite-entete"><TriangleAlert size={18} color="var(--warn-500)" /><h3>Quitter {NOM_APP} ?</h3></div>
    <div class="boite-corps"><p>Une session SSH est ouverte. Elle sera fermée.</p></div>
    <div class="boite-pied">
      <button class="btn btn-secondaire" data-focus-initial onclick={() => (app.confirmerSortie = false)}>Annuler</button>
      <button class="btn btn-danger" onclick={() => app.quitter()}>Se déconnecter et quitter</button>
    </div>
  </Boite>
{/if}

<style>
  .fort { color: var(--text-primary); }
  .consigne { margin: 0; white-space: pre-line; }
  .invite { display: flex; flex-direction: column; gap: 4px; }
  .espace { flex: 1; }
  .cadre { display: flex; flex-direction: column; gap: 6px; padding: 12px; background: rgba(0, 0, 0, 0.35); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); }
  .cadre-tete { display: flex; align-items: center; gap: 8px; }
  .copier { height: 24px; padding: 0 8px; display: inline-flex; align-items: center; gap: 4px; border: 1px solid transparent; border-radius: var(--radius-xs); background: transparent; color: var(--text-secondary); font-size: 11px; }
  .copier:hover { background: var(--surface-2); color: var(--text-primary); }
  .commande { display: flex; flex-direction: column; gap: 4px; }
  .commande > span { color: var(--text-muted); }
  code { padding: 6px 8px; background: var(--surface-terminal); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); font-size: 12px; color: var(--text-primary); }
  .alerte-tete { padding: 14px 16px; display: flex; align-items: center; gap: 10px; background: rgba(239, 68, 68, 0.18); border-bottom: 1px solid var(--danger-line); }
  .alerte-titres { display: flex; flex-direction: column; gap: 2px; }
  .alerte-titres h3 { margin: 0; font-size: 14px; font-weight: 600; color: #fff; }
  .alerte-titres span { font-size: 12px; font-weight: 600; color: var(--danger-text); }
  .haut { padding-top: 14px; }
  .comparaison { display: grid; grid-template-columns: 150px minmax(0, 1fr); border: 1px solid var(--border-default); border-radius: var(--radius-sm); overflow: hidden; }
  .cote { padding: 10px 12px; display: flex; flex-direction: column; gap: 2px; }
  .cote strong { font-weight: 600; color: var(--text-primary); }
  .cote small { font-size: 11px; color: var(--text-secondary); }
  .valeur { padding: 10px 12px; }
  .ancien { background: rgba(0, 0, 0, 0.3); }
  .nouveau { background: var(--danger-tint); border-top: 1px solid var(--danger-line); }
  .cote.nouveau strong { color: var(--danger-text); }
  .confirmation { display: flex; flex-direction: column; gap: 6px; }
</style>
