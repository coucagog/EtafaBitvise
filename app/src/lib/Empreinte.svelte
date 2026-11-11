<script lang="ts">
  let { valeur, taille = 16 }: { valeur: string; taille?: number } = $props()
  const groupes = $derived((valeur.replace(/^SHA256:/, '').match(/.{1,4}/g) ?? []).join(' '))
</script>

<div class="empreinte mono selectionnable" style:font-size={`${taille}px`}>{groupes}</div>

<style>
  .empreinte { line-height: 1.7; letter-spacing: 0.5px; word-spacing: 6px; color: var(--text-primary); }
</style>
