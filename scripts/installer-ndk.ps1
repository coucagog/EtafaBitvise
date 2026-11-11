# Installe la derniere version stable du NDK dans le SDK Android de l'utilisateur.
# Attend la fin du banc de tests puis l'autorisation du garde-fou. N'accepte aucune licence.
param([int]$MaxHeures = 12, [int]$PauseSec = 60)

$racine = Split-Path $PSScriptRoot -Parent
$garde = Join-Path $PSScriptRoot "garde-ressources.ps1"
$suivi = Join-Path $racine "journal-dev\installation-ndk.log"
$sdk = $env:ANDROID_HOME
$limite = (Get-Date).AddHours($MaxHeures)

function Note($texte) {
    $ligne = "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') $texte"
    Add-Content -Path $suivi -Value $ligne -Encoding UTF8
    $ligne
}

if (-not (Test-Path "$sdk\licenses\android-sdk-license")) { Note "ARRET : licence SDK non acceptee dans $sdk"; exit 4 }
$mgr = "$sdk\cmdline-tools\latest\bin\sdkmanager.bat"
if (-not (Test-Path $mgr)) { $mgr = (Get-ChildItem "$sdk\cmdline-tools" -Recurse -Filter sdkmanager.bat | Select-Object -First 1).FullName }
if (-not $mgr) { Note "ARRET : sdkmanager introuvable"; exit 4 }

Note "Attente de la fin du banc de tests et du garde-fou"
while ($true) {
    if ((Get-Date) -ge $limite) { Note "Abandon : pas d'autorisation en $MaxHeures h"; exit 3 }
    $banc = Get-CimInstance Win32_Process -Filter "Name='powershell.exe'" | Where-Object { $_.CommandLine -match 'attendre-et-tester' }
    if (-not $banc) {
        $sortie = & $garde
        if ($LASTEXITCODE -eq 0) { Note "Garde-fou AUTORISE : $($sortie[0])"; break }
    }
    Start-Sleep -Seconds $PauseSec
}

(Get-Process -Id $PID).PriorityClass = [System.Diagnostics.ProcessPriorityClass]::BelowNormal

$liste = & $mgr "--sdk_root=$sdk" --list 2>$null
$ndk = $liste | Where-Object { $_ -match '^\s*ndk;\d' -and $_ -notmatch 'rc|beta|alpha' } |
    ForEach-Object { ($_ -split '\|')[0].Trim() } | Sort-Object -Unique |
    Sort-Object { [version]($_ -replace 'ndk;', '') } | Select-Object -Last 1
if (-not $ndk) { Note "ARRET : aucune version stable trouvee"; exit 5 }

Note "Installation de $ndk dans $sdk"
$retour = & $mgr "--sdk_root=$sdk" --install $ndk 2>&1 | Select-Object -Last 6 | Out-String
Note $retour

$dossier = Join-Path "$sdk\ndk" ($ndk -replace 'ndk;', '')
if ((Test-Path "$dossier\source.properties") -and (Test-Path "$dossier\ndk-build.cmd")) {
    Note "OK : $dossier"
    exit 0
}
Note "ECHEC : installation incomplete dans $dossier"
exit 6
