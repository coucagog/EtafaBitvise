<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog'
  import { CircleX, FolderOpen, Plug } from '@lucide/svelte'
  import { cles } from './cles.svelte'
  import { app, type Methode, type NiveauAgent, type Profil, type Section } from './etat.svelte'
  import { touches } from './raccourcis'

  let { p }: { p: Profil } = $props()

  const SECTIONS: [Section, string][] = [['connexion', 'Connexion'], ['auth', 'Authentification'], ['options', 'Options'], ['notes', 'Notes']]
  const TOUTES_METHODES: [Methode, string][] = [['mot-de-passe', 'Mot de passe'], ['cle', 'Clé'], ['clavier', 'Saisie interactive'], ['agent', 'Agent SSH']]
  const METHODES = $derived(TOUTES_METHODES.filter(([id]) => id !== 'agent' || app.capacites.agent))
  type Route = 'direct' | 'socks5' | 'http' | 'rebond'
  const ROUTES: [Route, string][] = [['direct', 'Directe'], ['socks5', 'SOCKS5'], ['http', 'HTTP'], ['rebond', 'Rebond']]

  const route = $derived<Route>(p.rebond ? 'rebond' : p.proxy.genre === 'aucun' ? 'direct' : p.proxy.genre)
  const autresProfils = $derived(app.profils.filter((x) => x.id !== p.id && !app.modifies[x.id]))
  const profilRebond = $derived(app.profils.find((x) => x.id === p.rebond))

  // Sans profil à proposer, la route reste « Directe » : un rebond affiché mais vide tromperait.
  let sansRebond = $state(false)
  $effect(() => {
    void p.id
    sansRebond = false
  })

  function choisirRoute(r: Route) {
    sansRebond = r === 'rebond' && !autresProfils.length
    if (sansRebond) return
    p.rebond = r === 'rebond' ? autresProfils[0].id : ''
    p.proxy.genre = r === 'socks5' || r === 'http' ? r : 'aucun'
    if (p.proxy.genre !== 'aucun' && !p.proxy.port) p.proxy.port = 1080
    app.secretRebond = ''
    changer()
  }

  const changer = () => app.marquerModifie()

  const NIVEAUX: [NiveauAgent, string, string][] = [
    ['aucun', 'Aucun', 'Profil invisible pour les agents.'],
    ['lecture', '0 · Lecture', 'Commandes de consultation (état, services, journaux) et lecture des fichiers dans les dossiers ci-dessous.'],
    ['exploitation', '1 · Exploitation', 'Comme la lecture, plus les commandes ajoutées ci-dessous, avec leurs arguments.'],
    ['encadre', '2 · Shell encadré', "Toute commande, mais chacune s'affiche ici et attend votre accord : MLS Connect doit être ouvert. Fichiers : dossiers ci-dessous seulement."],
    ['complet', '3 · Accès complet', "Toute commande, tout fichier, sans approbation. Réservé à vos serveurs d'essai : les droits du compte sur le serveur sont la seule limite."],
  ]
  const lignes = (texte: string) => texte.split('\n').map((l) => l.trim()).filter(Boolean)

  function entier(valeur: string, maximum: number): number {
    const n = Number(valeur)
    return Number.isInteger(n) && n >= 0 && n <= maximum ? n : 0
  }

  async function parcourir() {
    const chemin = await open({ multiple: false, directory: false, title: 'Choisir une clé privée' })
    if (typeof chemin === 'string') {
      p.cheminCle = chemin
      changer()
    }
  }

  const raisonBlocage = $derived.by(() => {
    if (app.champsInvalides > 0) return ''
    if (!p.utilisateur.trim()) return 'Utilisateur requis pour se connecter.'
    if (p.methode === 'mot-de-passe' && !app.secret) return 'Saisissez le mot de passe dans Authentification.'
    if (p.methode === 'cle' && !p.cheminCle.trim()) return 'Choisissez une clé privée dans Authentification.'
    if (p.proxy.genre !== 'aucun' && !p.proxy.hote.trim()) return 'Indiquez le proxy dans Options.'
    return ''
  })
</script>

<div class="onglets sections" role="tablist" aria-label="Sections du profil">
  {#each SECTIONS as [id, libelle]}
    <button class="onglet" role="tab" aria-selected={app.section === id} onclick={() => (app.section = id)}>
      {libelle}
      {#if id === 'connexion' && app.champsInvalides > 0}<span class="point-erreur" aria-label="Erreur"></span>{/if}
    </button>
  {/each}
</div>

<div class="contenu">
  {#if app.section === 'connexion'}
    <div class="pile">
      <div class="ligne-champ">
        <label for="f-nom">Nom du profil</label>
        <input id="f-nom" class="champ limite" bind:value={p.nom} oninput={changer} />
      </div>
      <div class="ligne-champ">
        <label for="f-groupe">Groupe</label>
        <input id="f-groupe" class="champ limite" bind:value={p.groupe} oninput={changer} placeholder="ex. Production" />
      </div>
      <div class="separateur"></div>
      <div class="ligne-champ haut">
        <label for="f-hote">Hôte</label>
        <div class="colonne">
          <div class="rangee">
            <input id="f-hote" class="champ mono" class:invalide={app.erreurHote} aria-invalid={app.erreurHote} bind:value={p.hote} oninput={changer} placeholder="Nom d'hôte ou adresse IP" />
            <label for="f-port" class="etiquette">Port</label>
            <input id="f-port" class="champ mono nombre port" class:invalide={app.erreurPort} aria-invalid={app.erreurPort} inputmode="numeric" value={p.port || ''} oninput={(e) => { p.port = entier(e.currentTarget.value, 65535); changer() }} />
          </div>
          {#if app.erreurHote}<span class="message-erreur"><CircleX size={12} />Hôte requis pour se connecter.</span>{/if}
          {#if app.erreurPort}<span class="message-erreur"><CircleX size={12} />Le port doit être un nombre entre 1 et 65535.</span>{/if}
        </div>
      </div>
      <div class="ligne-champ">
        <label for="f-utilisateur">Utilisateur</label>
        <input id="f-utilisateur" class="champ mono limite" bind:value={p.utilisateur} oninput={changer} placeholder="ex. root" />
      </div>
    </div>
  {:else if app.section === 'auth'}
    <div class="pile">
      <div class="ligne-champ">
        <span class="etiquette">Méthode</span>
        <div class="segment debut" role="radiogroup" aria-label="Méthode d'authentification">
          {#each METHODES as [id, libelle]}
            <button role="radio" aria-checked={p.methode === id} onclick={() => { p.methode = id; app.secret = ''; changer() }}>{libelle}</button>
          {/each}
        </div>
      </div>
      {#if p.methode === 'mot-de-passe'}
        <div class="ligne-champ">
          <label for="f-mdp">Mot de passe</label>
          <input id="f-mdp" class="champ limite" type="password" autocomplete="off" bind:value={app.secret} placeholder={app.secretMemorise(p) ? 'Enregistré dans le trousseau' : 'Requis à chaque connexion'} />
        </div>
        {@render memorisation()}
      {:else if p.methode === 'agent'}
        <p class="aide">Utilise les clés chargées dans l'agent OpenSSH ou Pageant. Aucune clé ni passphrase ne passe par MLS Connect.</p>
      {:else if p.methode === 'clavier'}
        <p class="aide">Le serveur pose ses questions à la connexion, par exemple un code à usage unique. Les réponses ne sont pas conservées : elles seront redemandées à chaque reconnexion.</p>
      {:else}
        {#if cles.liste.length}
          <div class="ligne-champ">
            <label for="f-mes-cles">Mes clés</label>
            <select id="f-mes-cles" class="champ limite" value={p.cheminCle} onchange={(e) => { p.cheminCle = e.currentTarget.value; changer() }}>
              <option value="" disabled>Choisir une clé…</option>
              {#each cles.liste as c (c.nom)}<option value={c.chemin}>{c.nom} · {c.algorithme}</option>{/each}
            </select>
          </div>
        {/if}
        <div class="ligne-champ">
          <label for="f-cle">Clé privée</label>
          <div class="rangee">
            <input id="f-cle" class="champ mono" bind:value={p.cheminCle} oninput={changer} placeholder="Chemin du fichier de clé" />
            {#if !app.capacites.mobile}<button class="btn btn-secondaire" onclick={parcourir}><FolderOpen size={14} />Parcourir…</button>{/if}
          </div>
        </div>
        <div class="ligne-champ">
          <label for="f-phrase">Passphrase</label>
          <input id="f-phrase" class="champ limite" type="password" autocomplete="off" bind:value={app.secret} placeholder={app.secretMemorise(p) ? 'Enregistrée dans le trousseau' : 'Si la clé est protégée'} />
        </div>
        {@render memorisation()}
      {/if}
    </div>
  {:else if app.section === 'options'}
    <div class="pile etroite">
      <div class="titre-groupe">Route réseau</div>
      <div class="route">
        <div class="segment debut" role="radiogroup" aria-label="Route réseau">
          {#each ROUTES as [id, libelle]}
            <button role="radio" aria-checked={route === id} onclick={() => choisirRoute(id)}>{libelle}</button>
          {/each}
        </div>
        {#if sansRebond}
          <small class="aide">Enregistrez d'abord un autre profil pour vous en servir de rebond.</small>
        {/if}
        {#if route === 'socks5' || route === 'http'}
          <div class="rangee">
            <input class="champ mono" aria-label="Hôte du proxy" bind:value={p.proxy.hote} oninput={changer} placeholder="Hôte du proxy" />
            <input class="champ mono nombre port" aria-label="Port du proxy" inputmode="numeric" value={p.proxy.port || ''} oninput={(e) => { p.proxy.port = entier(e.currentTarget.value, 65535); changer() }} />
          </div>
          <small class="aide">Proxy sans authentification. Le nom du serveur est résolu par le proxy.</small>
        {:else if route === 'rebond'}
          {#if autresProfils.length}
            <select class="champ" aria-label="Profil de rebond" bind:value={p.rebond} onchange={() => { app.secretRebond = ''; changer() }}>
              {#each autresProfils as x (x.id)}<option value={x.id}>{x.nom} · {x.utilisateur}@{x.hote}:{x.port}</option>{/each}
            </select>
            {#if profilRebond?.methode !== 'clavier' && profilRebond?.methode !== 'agent' && !(profilRebond && app.secretMemorise(profilRebond))}
              <input class="champ" type="password" autocomplete="off" aria-label="Secret du rebond" bind:value={app.secretRebond} placeholder={profilRebond?.methode === 'cle' ? 'Passphrase de la clé du rebond, si elle est protégée' : 'Mot de passe du rebond'} />
            {/if}
            <small class="aide">La clé d'hôte du rebond est vérifiée, puis celle du serveur visé.</small>
          {:else}
            <small class="aide">Le profil de rebond a des modifications non enregistrées.</small>
          {/if}
        {/if}
      </div>
      <div class="titre-groupe espace">Délais</div>
      <div class="option bordee">
        <div class="texte"><span>Délai de connexion</span><small>Abandon si le serveur ne répond pas.</small></div>
        <label class="unite"><input class="champ mono nombre petit" inputmode="numeric" aria-label="Délai de connexion en secondes" value={p.delaiConnexion || ''} oninput={(e) => { p.delaiConnexion = entier(e.currentTarget.value, 600); changer() }} />s</label>
      </div>
      <div class="option">
        <div class="texte"><span>Keep-alive</span><small>Paquet de maintien pour éviter la coupure des sessions inactives.</small></div>
        {#if p.keepalive}
          <label class="unite">toutes les<input class="champ mono nombre petit" inputmode="numeric" aria-label="Intervalle keep-alive en secondes" value={p.intervalleKeepalive || ''} oninput={(e) => { p.intervalleKeepalive = entier(e.currentTarget.value, 3600); changer() }} />s</label>
        {/if}
        <button class="interrupteur" role="switch" aria-checked={p.keepalive} aria-label="Keep-alive" onclick={() => { p.keepalive = !p.keepalive; changer() }}></button>
      </div>
      {#if !app.capacites.mobile}
      <div class="titre-groupe espace">Accès d'un agent</div>
      <div class="colonne agent">
        <select class="champ limite" aria-label="Niveau d'accès d'un agent" bind:value={p.agent.niveau} onchange={changer}>
          {#each NIVEAUX as [id, libelle]}<option value={id}>{libelle}</option>{/each}
        </select>
        <small class="aide" class:danger={p.agent.niveau === 'complet'}>{NIVEAUX.find(([id]) => id === p.agent.niveau)?.[2] ?? ''}</small>
        {#if p.agent.niveau === 'lecture' || p.agent.niveau === 'exploitation' || p.agent.niveau === 'encadre'}
          <label class="etiquette-agent" for="f-agent-dossiers">Dossiers lisibles, un par ligne</label>
          <textarea id="f-agent-dossiers" class="champ mono" rows="3" placeholder="/var/log" value={p.agent.dossiers.join('\n')} oninput={(e) => { p.agent.dossiers = lignes(e.currentTarget.value); changer() }}></textarea>
        {/if}
        {#if p.agent.niveau === 'exploitation'}
          <label class="etiquette-agent" for="f-agent-commandes">Commandes ajoutées, une par ligne</label>
          <textarea id="f-agent-commandes" class="champ mono" rows="3" placeholder="systemctl restart nginx" value={p.agent.commandes.join('\n')} oninput={(e) => { p.agent.commandes = lignes(e.currentTarget.value); changer() }}></textarea>
          <small class="aide">Une commande ajoutée admet des arguments de plus. Pour les refuser, terminez la ligne par un signe égal : <span class="mono">systemctl restart nginx =</span></small>
        {/if}
        {#if p.agent.niveau !== 'aucun'}
          <small class="aide">L'agent passe par le programme mc-mcp. La clé d'hôte doit avoir été enregistrée ici, et le secret mémorisé dans le trousseau. Chaque demande est consignée dans agent.log.</small>
        {/if}
      </div>
      {/if}
    </div>
  {:else}
    <div class="pile">
      <label for="f-notes" class="etiquette-notes">Notes (non transmises au serveur)</label>
      <textarea id="f-notes" class="champ" rows="9" bind:value={p.notes} oninput={changer}></textarea>
    </div>
  {/if}
</div>

<div class="actions">
  {#if app.champsInvalides > 0}
    <span class="message-erreur"><CircleX size={12} />{app.champsInvalides === 2 ? '2 champs à corriger' : '1 champ à corriger'}</span>
  {/if}
  <span class="statut">
    {app.messageEnregistrement ?? raisonBlocage ?? ''}
    {#if !app.messageEnregistrement && !raisonBlocage}{app.modifies[p.id] ? 'Modifications non enregistrées' : 'Enregistré dans profiles.json'}{/if}
  </span>
  <button class="btn btn-secondaire" onclick={() => app.enregistrer()} aria-disabled={app.champsInvalides > 0}>Enregistrer<span class="kbd">{touches('enregistrer')}</span></button>
  {#if app.phase === 'connecte' && app.sessionProfil === p.id}
    <button class="btn btn-danger" onclick={() => app.deconnecter()}>Se déconnecter</button>
  {:else}
    <button class="btn btn-primaire" aria-disabled={!app.peutConnecter} onclick={() => app.connecter()}><Plug size={14} />Se connecter</button>
  {/if}
</div>

{#snippet memorisation()}
  {#if app.capacites.trousseau}
    <label class="memoriser">
      <input type="checkbox" checked={p.memoriser} onchange={(e) => app.changerMemorisation(p, e.currentTarget.checked)} />
      <span>Mémoriser dans le trousseau du système</span>
    </label>
    {#if p.memoriser}<small class="aide">Enregistré après la prochaine connexion réussie. Décocher l'efface immédiatement.</small>{/if}
  {/if}
{/snippet}

<style>
  .sections { height: 33px; padding: 0 14px; border-bottom: 1px solid var(--border-default); overflow-x: auto; }
  .onglet { padding: 0 10px; }
  .point-erreur { width: 6px; height: 6px; border-radius: 50%; background: var(--rec-500); }
  .contenu { flex: 1; min-height: 0; overflow: auto; padding: 20px 24px; }
  .pile { display: flex; flex-direction: column; gap: 14px; max-width: 680px; }
  .pile.etroite { max-width: 640px; gap: 4px; }
  .colonne { display: flex; flex-direction: column; gap: 6px; }
  .rangee { display: flex; gap: 12px; align-items: center; }
  .rangee .etiquette { font-size: 12px; color: var(--text-secondary); }
  .limite { max-width: 320px; }
  .port { width: 76px; flex: 0 0 76px; }
  .petit { width: 56px; height: 28px; padding: 0 8px; }
  .separateur { height: 1px; background: var(--border-subtle); }
  .debut { justify-self: start; }
  .route { display: flex; flex-direction: column; align-items: flex-start; gap: 8px; padding: 8px 0 14px; }
  .route .rangee, .route select { width: 100%; max-width: 480px; }
  .espace { margin-top: 6px; }
  .memoriser { display: inline-flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-secondary); cursor: pointer; }
  .agent { padding: 4px 0 8px; }
  .agent textarea { max-width: 480px; resize: vertical; }
  .etiquette-agent { margin-top: 6px; font-size: 12px; color: var(--text-secondary); }
  .aide.danger { color: var(--rec-500); }
  .aide { margin: 0; font-size: 11px; line-height: 1.45; color: var(--text-muted); }
  .option { display: flex; align-items: center; gap: 12px; padding: 8px 0; }
  .option.bordee { border-bottom: 1px solid var(--border-subtle); }
  .texte { flex: 1; display: flex; flex-direction: column; gap: 2px; font-size: 12px; color: var(--text-primary); }
  .texte small { font-size: 11px; color: var(--text-muted); }
  .unite { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-muted); }
  .etiquette-notes { font-size: 12px; color: var(--text-secondary); }
  .actions { flex-shrink: 0; display: flex; align-items: center; gap: 8px; padding: 10px 20px; border-top: 1px solid var(--border-default); background: rgba(0, 0, 0, 0.2); }
  .statut { flex: 1; font-size: 11px; color: var(--text-muted); }
</style>
