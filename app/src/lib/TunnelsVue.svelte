<script lang="ts">
  import { ArrowDown, ArrowRight, ArrowUp, Cable, CircleAlert, Globe, Monitor, Pencil, Plus, RotateCcw, Server, Trash2, TriangleAlert, Unplug, X } from '@lucide/svelte'
  import Boite from './Boite.svelte'
  import PasserelleFtp from './PasserelleFtp.svelte'
  import { app } from './etat.svelte'
  import { taille } from './sftp.svelte'
  import { estExpose, type EtatTunnel, type GenreTunnel, type LigneTunnel, type Regle, tunnels } from './tunnels.svelte'

  type Famille = 'local' | 'distant' | 'dynamique'
  type Filtre = Famille | 'tous'
  interface Brouillon {
    id: string | null; nom: string; famille: Famille; protocole: 'socks' | 'http'; ecouteHote: string; ecoutePort: string; cibleHote: string; ciblePort: string; auto: boolean
  }

  const famille = (g: GenreTunnel): Famille => (g === 'socks' || g === 'http' ? 'dynamique' : g)
  const FAMILLES: { id: Famille; code: string; titre: string; court: string; aide: string }[] = [
    { id: 'local', code: 'L', titre: 'Client vers serveur', court: 'Local', aide: 'Un port de ce poste mène à une destination joignable depuis le serveur.' },
    { id: 'distant', code: 'R', titre: 'Serveur vers client', court: 'Distant', aide: 'Un port du serveur mène à une destination joignable depuis ce poste.' },
    { id: 'dynamique', code: 'D', titre: 'Proxy dynamique', court: 'Dynamique', aide: "Les applications de ce poste choisissent leur destination, et sortent par le serveur." },
  ]
  const ETATS: Record<EtatTunnel, { texte: string; c: string; fond: string }> = {
    ferme: { texte: 'Inactif', c: 'var(--text-muted)', fond: 'var(--surface-2)' },
    ouverture: { texte: 'Ouverture…', c: 'var(--warn-500)', fond: 'var(--warn-tint)' },
    actif: { texte: 'Actif', c: 'var(--ok-500)', fond: 'var(--ok-tint)' },
    erreur: { texte: 'Échec', c: 'var(--danger-text)', fond: 'var(--danger-tint)' },
    suspendu: { texte: 'En attente', c: 'var(--warn-500)', fond: 'var(--warn-tint)' },
  }
  const CODES_ERREUR = { 'port-occupe': 'Port déjà utilisé', refuse: 'Refusé par le serveur', invalide: 'Règle invalide', session: 'Session fermée' }

  let filtre = $state<Filtre>('tous')
  let brouillon = $state<Brouillon | null>(null)
  let tente = $state(false)
  let aSupprimer = $state<LigneTunnel | null>(null)
  let champNom = $state<HTMLInputElement>()

  const lignes = $derived(filtre === 'tous' ? tunnels.lignes : tunnels.lignes.filter((l) => famille(l.regle.genre) === filtre))
  const compte = (f: Filtre) => (f === 'tous' ? tunnels.lignes.length : tunnels.lignes.filter((l) => famille(l.regle.genre) === f).length)
  const actifs = $derived(tunnels.lignes.filter((l) => l.etat === 'actif').length)
  const ecoute = (r: Regle) => `${r.ecouteHote.trim() || '127.0.0.1'}:${r.ecoutePort}`
  const destination = (r: Regle) => (r.genre === 'socks' ? 'SOCKS5, au choix du client' : r.genre === 'http' ? 'HTTP CONNECT, au choix du client' : `${r.cibleHote}:${r.ciblePort}`)
  const debit = (o: number) => `${taille(Math.round(o))}/s`
  const expose = $derived(tunnels.exposees)

  $effect(() => {
    if (brouillon && champNom && brouillon.id === null) champNom.focus()
  })

  function nouveau(f: Famille) {
    tente = false
    brouillon = { id: null, nom: '', famille: f, protocole: 'socks', ecouteHote: '127.0.0.1', ecoutePort: f === 'dynamique' ? '1080' : '', cibleHote: '', ciblePort: '', auto: false }
  }

  function modifier(l: LigneTunnel) {
    const r = l.regle
    tente = false
    brouillon = {
      id: r.id, nom: r.nom, famille: famille(r.genre), protocole: r.genre === 'http' ? 'http' : 'socks', ecouteHote: r.ecouteHote || '127.0.0.1',
      ecoutePort: String(r.ecoutePort), cibleHote: r.cibleHote, ciblePort: r.ciblePort ? String(r.ciblePort) : '', auto: r.auto,
    }
  }

  const portValide = (s: string) => /^\d+$/.test(s.trim()) && Number(s) >= 1 && Number(s) <= 65535

  /** Deux règles du même côté ne peuvent pas écouter sur le même port. */
  const erreurPort = $derived.by(() => {
    const b = brouillon
    if (!b) return null
    if (!portValide(b.ecoutePort)) return 'Indiquez un port entre 1 et 65535.'
    const memeCote = (g: GenreTunnel) => (g === 'distant') === (b.famille === 'distant')
    const doublon = tunnels.lignes.find((l) => l.regle.id !== b.id && memeCote(l.regle.genre) && l.regle.ecoutePort === Number(b.ecoutePort))
    return doublon ? `Ce port est déjà pris par « ${doublon.regle.nom} ».` : null
  })
  const erreurHote = $derived(brouillon && brouillon.famille !== 'dynamique' && !brouillon.cibleHote.trim() ? 'Indiquez un hôte de destination.' : null)
  const erreurCible = $derived(brouillon && brouillon.famille !== 'dynamique' && !portValide(brouillon.ciblePort) ? 'Indiquez un port entre 1 et 65535.' : null)

  async function appliquer(ev: Event) {
    ev.preventDefault()
    tente = true
    const b = brouillon
    if (!b || erreurPort || erreurHote || erreurCible) return
    const genre: GenreTunnel = b.famille === 'dynamique' ? b.protocole : b.famille
    const dynamique = b.famille === 'dynamique'
    brouillon = null
    await tunnels.appliquer({
      id: b.id ?? crypto.randomUUID(), nom: b.nom.trim() || `${genre}-${b.ecoutePort.trim()}`, genre, ecouteHote: b.ecouteHote.trim() || '127.0.0.1', ecoutePort: Number(b.ecoutePort),
      cibleHote: dynamique ? '' : b.cibleHote.trim(), ciblePort: dynamique ? 0 : Number(b.ciblePort), auto: b.auto,
    })
  }

  function supprimer(l: LigneTunnel) {
    aSupprimer = null
    void tunnels.supprimer(l)
  }

  async function limiter() {
    for (const l of expose) await tunnels.appliquer({ ...$state.snapshot(l.regle) as Regle, ecouteHote: '127.0.0.1' })
  }

  const coteDistant = $derived(brouillon?.famille === 'distant')
  const INTERFACES = $derived([
    { ip: '127.0.0.1', texte: coteDistant ? 'Serveur uniquement' : 'Ce poste uniquement' },
    { ip: '0.0.0.0', texte: 'Toutes les interfaces' },
  ])
  const aideFamille = $derived(FAMILLES.find((f) => f.id === brouillon?.famille))
  const brouillonExpose = $derived(!!brouillon && estExpose({ ecouteHote: brouillon.ecouteHote } as Regle))
</script>

<div class="vue">
  {#if app.etatSession === 'perdue'}
    <div class="alerte danger" role="alert"><Unplug size={14} /><span><strong>Session déconnectée.</strong> Les tunnels sont fermés et rouverts à la reconnexion.</span></div>
  {:else if app.etatSession === 'reconnexion'}
    <div class="alerte attention" role="status"><span><strong>Reconnexion en cours.</strong> Les tunnels sont en attente.</span></div>
  {/if}
  {#if expose.length}
    <div class="alerte attention" role="alert">
      <TriangleAlert size={14} />
      <span><strong>{expose.length > 1 ? `${expose.length} tunnels écoutent` : `« ${expose[0].regle.nom} » écoute`} sur le réseau.</strong> Toute machine qui atteint ce port traverse la session SSH, sans authentification.</span>
      <button class="btn btn-secondaire btn-compact" onclick={limiter}>Limiter à 127.0.0.1</button>
    </div>
  {/if}

  <div class="outils">
    <div class="nouveaux" role="group" aria-label="Nouveau tunnel">
      {#each FAMILLES as f}
        <button class="btn btn-secondaire btn-compact" title={f.aide} onclick={() => nouveau(f.id)}><Plus size={13} /><span class="code">{f.code}</span>{f.court}</button>
      {/each}
    </div>
    <div class="segment" role="radiogroup" aria-label="Filtrer les tunnels">
      <button role="radio" aria-checked={filtre === 'tous'} onclick={() => (filtre = 'tous')}>Tous<span>{compte('tous')}</span></button>
      {#each FAMILLES as f}
        <button role="radio" aria-checked={filtre === f.id} onclick={() => (filtre = f.id)}>{f.code}<span>{compte(f.id)}</span></button>
      {/each}
    </div>
    <span class="vide"></span>
    <span class="resume">{actifs} actif{actifs > 1 ? 's' : ''}{tunnels.enErreur ? ` · ${tunnels.enErreur} en échec` : ''}</span>
  </div>

  {#if !app.capacites.mobile}<PasserelleFtp />{/if}

  <div class="corps">
    <div class="liste" role="grid" aria-label="Tunnels">
      <div class="grille entetes" role="row">
        <span>Type</span><span>Nom</span><span>Écoute</span><span>Destination</span><span>État</span><span title="Connexions actives">Conn.</span><span>Débit</span><span>Activé</span><span></span>
      </div>
      {#each lignes as l (l.regle.id)}
        {@const f = FAMILLES.find((x) => x.id === famille(l.regle.genre))!}
        {@const e = ETATS[l.etat]}
        <div class="rangee" role="row">
          <div class="grille">
            <span class="type" title={f.titre}><span class="code">{f.code}</span></span>
            <span class="coupe nom" title={l.regle.nom}>{l.regle.nom}</span>
            <span class="mono coupe ecoute">
              {#if l.regle.genre === 'distant'}<Server size={12} />{:else}<Monitor size={12} />{/if}
              <span class="coupe" title={ecoute(l.regle)}>{ecoute(l.regle)}</span>
              {#if estExpose(l.regle)}<span title="Écoute sur le réseau"><Globe size={12} color="var(--warn-500)" /></span>{/if}
            </span>
            <span class="mono coupe cible"><ArrowRight size={12} /><span class="coupe" title={destination(l.regle)}>{destination(l.regle)}</span></span>
            <span class="pilule" style:color={e.c} style:background={e.fond}>{e.texte}</span>
            <span class="mono">{l.etat === 'actif' ? l.actives : '—'}</span>
            <span class="mono debit">{#if l.etat === 'actif'}<span><ArrowDown size={10} />{debit(l.debitDescendu)}</span><span><ArrowUp size={10} />{debit(l.debitMonte)}</span>{:else}—{/if}</span>
            <button class="interrupteur" role="switch" aria-checked={l.activee} aria-label={`Activer ${l.regle.nom}`} disabled={app.etatSession !== 'ok'} onclick={() => tunnels.basculer(l)}></button>
            <span class="actions">
              <button class="btn-icone" aria-label={`Modifier ${l.regle.nom}`} title="Modifier" onclick={() => modifier(l)}><Pencil size={13} /></button>
              <button class="btn-icone" aria-label={`Supprimer ${l.regle.nom}`} title="Supprimer" onclick={() => (aSupprimer = l)}><Trash2 size={13} /></button>
            </span>
          </div>
          {#if l.etat === 'erreur' && l.erreur}
            <div class="detail erreur" role="alert">
              <CircleAlert size={13} />
              <span class="selectionnable"><strong>{CODES_ERREUR[l.erreur.code]}.</strong> {l.erreur.message}</span>
              <button class="btn btn-secondaire btn-compact" onclick={() => modifier(l)}>Modifier</button>
              <button class="btn btn-secondaire btn-compact" onclick={() => tunnels.ouvrir(l)}><RotateCcw size={12} />Réessayer</button>
            </div>
          {:else if l.etat === 'actif' && l.avis}
            <div class="detail avis selectionnable"><TriangleAlert size={13} /><span>{l.avis}</span></div>
          {/if}
        </div>
      {/each}
      {#if tunnels.lignes.length === 0}
        <div class="message">
          <Cable size={26} color="var(--text-muted)" />
          <strong>Aucun tunnel pour ce profil</strong>
          <span>Un tunnel relaie un port entre ce poste et le serveur. Choisissez un type pour commencer.</span>
          <div class="nouveaux">{#each FAMILLES as f}<button class="btn btn-secondaire btn-compact" onclick={() => nouveau(f.id)}>{f.titre}</button>{/each}</div>
        </div>
      {:else if lignes.length === 0}
        <div class="message"><span>Aucun tunnel de ce type</span><button class="btn btn-discret btn-compact" onclick={() => (filtre = 'tous')}>Afficher tous les tunnels</button></div>
      {/if}
    </div>

    {#if brouillon}
      {@const b = brouillon}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <form class="editeur" aria-label={b.id ? 'Modifier le tunnel' : 'Nouveau tunnel'} onsubmit={appliquer} onkeydown={(e) => { if (e.key === 'Escape') { e.stopPropagation(); brouillon = null } }}>
        <div class="editeur-tete"><h3>{b.id ? 'Modifier le tunnel' : 'Nouveau tunnel'}</h3><button type="button" class="btn-icone" aria-label="Fermer" onclick={() => (brouillon = null)}><X size={14} /></button></div>
        <div class="editeur-corps">
          <div class="champ-groupe">
            <span class="etiquette">Type</span>
            <div class="segment plein" role="radiogroup" aria-label="Type de tunnel">
              {#each FAMILLES as f}<button type="button" role="radio" aria-checked={b.famille === f.id} onclick={() => { b.famille = f.id; if (f.id === 'dynamique' && !b.ecoutePort) b.ecoutePort = '1080' }}>{f.court}</button>{/each}
            </div>
            <span class="aide">{aideFamille?.aide}</span>
          </div>
          <label class="champ-groupe"><span class="etiquette">Nom</span><input class="champ" bind:this={champNom} bind:value={b.nom} placeholder="ex. pg-prod" spellcheck="false" /></label>
          <div class="champ-groupe">
            <span class="etiquette">{coteDistant ? 'Écoute sur le serveur' : 'Écoute sur ce poste'}</span>
            <div class="interfaces" role="radiogroup" aria-label="Interface d'écoute">
              {#each INTERFACES as i}
                <button type="button" role="radio" aria-checked={b.ecouteHote === i.ip} onclick={() => (b.ecouteHote = i.ip)}><span class="mono">{i.ip}</span><span>{i.texte}</span></button>
              {/each}
            </div>
            {#if brouillonExpose}
              <div class="alerte-champ"><TriangleAlert size={13} /><span>Écoute ouverte au réseau, sans authentification.</span><button type="button" class="lien" onclick={() => (b.ecouteHote = '127.0.0.1')}>Limiter à 127.0.0.1</button></div>
            {/if}
            <label class="champ-groupe"><span class="etiquette">Port d'écoute</span><input class="champ mono" bind:value={b.ecoutePort} inputmode="numeric" placeholder="1–65535" aria-invalid={tente && !!erreurPort} /></label>
            {#if tente && erreurPort}<span class="message-erreur">{erreurPort}</span>{/if}
          </div>
          {#if b.famille === 'dynamique'}
            <div class="champ-groupe">
              <span class="etiquette">Protocole</span>
              <div class="segment plein" role="radiogroup" aria-label="Protocole du proxy">
                <button type="button" role="radio" aria-checked={b.protocole === 'socks'} onclick={() => (b.protocole = 'socks')}>SOCKS5</button>
                <button type="button" role="radio" aria-checked={b.protocole === 'http'} onclick={() => (b.protocole = 'http')}>HTTP CONNECT</button>
              </div>
            </div>
          {:else}
            <div class="champ-groupe">
              <span class="etiquette">{coteDistant ? 'Destination, depuis ce poste' : 'Destination, depuis le serveur'}</span>
              <div class="deux">
                <label class="champ-groupe"><span class="etiquette">Hôte</span><input class="champ mono" bind:value={b.cibleHote} placeholder="nom ou IP" spellcheck="false" aria-invalid={tente && !!erreurHote} /></label>
                <label class="champ-groupe"><span class="etiquette">Port</span><input class="champ mono" bind:value={b.ciblePort} inputmode="numeric" placeholder="1–65535" aria-invalid={tente && !!erreurCible} /></label>
              </div>
              {#if tente && (erreurHote || erreurCible)}<span class="message-erreur">{erreurHote ?? erreurCible}</span>{/if}
            </div>
          {/if}
          <label class="option"><input type="checkbox" class="coche" bind:checked={b.auto} />Ouvrir à chaque connexion de ce profil</label>
        </div>
        <div class="editeur-pied">
          <button type="button" class="btn btn-discret btn-compact" onclick={() => (brouillon = null)}>Annuler</button>
          <button type="submit" class="btn btn-primaire btn-compact" disabled={app.etatSession !== 'ok'}>{b.id ? 'Appliquer' : 'Créer et ouvrir'}</button>
        </div>
      </form>
    {/if}
  </div>
</div>

{#if aSupprimer}
  {@const l = aSupprimer}
  <Boite largeur={420} bordure="var(--danger-line)" etiquette="Supprimer le tunnel" surAnnuler={() => (aSupprimer = null)}>
    <div class="boite-entete"><Trash2 size={18} color="var(--danger-text)" /><h3>Supprimer « {l.regle.nom} » ?</h3></div>
    <div class="boite-corps"><p>La règle est retirée du profil{l.actives ? `, et ses ${l.actives} connexions en cours sont coupées` : ''}.</p></div>
    <div class="boite-pied">
      <button class="btn btn-discret" data-focus-initial onclick={() => (aSupprimer = null)}>Annuler</button>
      <button class="btn btn-danger" onclick={() => supprimer(l)}>Supprimer le tunnel</button>
    </div>
  </Boite>
{/if}

<style>
  .vue { position: absolute; inset: 0; display: flex; flex-direction: column; background: var(--surface-window-opaque); font-size: 12px; }
  .alerte { min-height: 36px; flex: 0 0 auto; display: flex; align-items: center; gap: 8px; padding: 4px 12px; border-bottom: 1px solid var(--border-subtle); color: var(--text-secondary); }
  .alerte strong { font-weight: 600; color: var(--text-primary); }
  .alerte > span { flex: 1; min-width: 0; }
  .alerte.danger { background: var(--danger-tint); }
  .alerte.danger :global(svg) { color: var(--danger-text); }
  .alerte.attention { background: var(--warn-tint); }
  .alerte.attention :global(svg) { color: var(--warn-500); }
  .outils { height: 44px; flex: 0 0 44px; display: flex; align-items: center; gap: 12px; padding: 0 12px; border-bottom: 1px solid var(--border-default); }
  .nouveaux { display: flex; gap: 6px; }
  .code { display: inline-flex; align-items: center; justify-content: center; width: 16px; height: 16px; border-radius: var(--radius-sm); background: var(--surface-3); font-family: var(--font-mono); font-size: 10px; font-weight: 700; color: var(--text-primary); }
  .segment { display: inline-flex; gap: 2px; padding: 2px; background: var(--surface-sunken); border: 1px solid var(--border-default); border-radius: var(--radius-sm); }
  .segment button { height: 22px; padding: 0 8px; display: inline-flex; align-items: center; justify-content: center; gap: 5px; border: none; border-radius: var(--radius-sm); background: transparent; color: var(--text-secondary); font-size: 12px; }
  .segment button span { color: var(--text-muted); font-size: 11px; }
  .segment button[aria-checked="true"] { background: var(--accent-blue-tint); color: #fff; }
  .segment.plein { display: flex; }
  .segment.plein button { flex: 1; height: 26px; }
  .vide { flex: 1; }
  .resume { color: var(--text-muted); white-space: nowrap; }
  .corps { flex: 1; min-height: 0; display: flex; }
  .liste { flex: 1; min-width: 0; overflow: auto; }
  .grille { display: grid; grid-template-columns: 40px minmax(90px, 1fr) minmax(140px, 1.2fr) minmax(140px, 1.4fr) 92px 44px 92px 44px 62px; gap: 10px; align-items: center; padding: 0 12px; min-width: 860px; }
  .entetes { height: 26px; position: sticky; top: 0; z-index: 1; background: var(--surface-sunken); border-bottom: 1px solid var(--border-subtle); font-size: 11px; color: var(--text-muted); }
  .rangee { border-bottom: 1px solid var(--border-subtle); min-width: 860px; }
  .rangee .grille { min-height: 40px; }
  .rangee:hover { background: var(--surface-1); }
  .coupe { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .nom { font-weight: 600; }
  .mono { font-family: var(--font-mono); font-size: 11px; }
  .ecoute, .cible { display: flex; align-items: center; gap: 6px; color: var(--text-secondary); }
  .ecoute :global(svg), .cible :global(svg) { flex: none; color: var(--text-muted); }
  .pilule { width: fit-content; height: 20px; padding: 0 8px; display: inline-flex; align-items: center; border-radius: var(--radius-pill); font-size: 11px; white-space: nowrap; }
  .debit { display: flex; flex-direction: column; color: var(--text-muted); }
  .debit span { display: inline-flex; align-items: center; gap: 3px; white-space: nowrap; }
  .actions { display: flex; justify-content: flex-end; gap: 2px; }
  .detail { display: flex; align-items: center; gap: 8px; padding: 0 12px 8px 62px; color: var(--text-secondary); }
  .detail > span { flex: 1; min-width: 0; }
  .detail strong { font-weight: 600; }
  .detail.erreur :global(svg:first-child) { color: var(--danger-text); }
  .detail.erreur strong { color: var(--danger-text); }
  .detail.avis :global(svg) { color: var(--warn-500); }
  .message { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 72px 24px; text-align: center; color: var(--text-muted); }
  .message strong { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .editeur { width: 340px; flex: none; display: flex; flex-direction: column; border-left: 1px solid var(--border-default); background: var(--surface-1); }
  .editeur-tete { height: 40px; flex: none; display: flex; align-items: center; justify-content: space-between; padding: 0 8px 0 14px; border-bottom: 1px solid var(--border-subtle); }
  .editeur-tete h3 { margin: 0; font-size: 13px; font-weight: 600; }
  .editeur-corps { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; padding: 12px 14px; }
  .editeur-pied { height: 48px; flex: none; display: flex; align-items: center; justify-content: flex-end; gap: 8px; padding: 0 12px; border-top: 1px solid var(--border-subtle); }
  .champ-groupe { display: flex; flex-direction: column; gap: 5px; }
  .etiquette { font-size: 11px; color: var(--text-muted); }
  .aide { font-size: 11px; line-height: 1.45; color: var(--text-muted); }
  .interfaces { display: flex; flex-direction: column; gap: 4px; margin-bottom: 4px; }
  .interfaces button { height: 28px; padding: 0 8px; display: flex; align-items: center; justify-content: space-between; gap: 8px; border: 1px solid var(--border-default); border-radius: var(--radius-sm); background: transparent; color: var(--text-secondary); font-size: 11px; }
  .interfaces button[aria-checked="true"] { border-color: var(--accent-blue); background: var(--accent-blue-tint); color: #fff; }
  .alerte-champ { display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--warn-500); }
  .alerte-champ span { flex: 1; }
  .lien { padding: 0; border: none; background: transparent; color: var(--accent-blue); font-size: 11px; white-space: nowrap; }
  .deux { display: grid; grid-template-columns: 1fr 88px; gap: 8px; }
  .option { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); }
  .coche { width: 14px; height: 14px; margin: 0; accent-color: var(--accent-blue); }
  .interrupteur:disabled { opacity: 0.45; }
</style>
