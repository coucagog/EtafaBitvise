# Attend l'autorisation du garde-fou, puis verifie et execute le banc de faisabilite.
# Chaque etape repasse par le garde-fou. Abandon apres -MaxHeures sans autorisation.
param([int]$MaxHeures = 12, [int]$PauseSec = 60, [switch]$DiagCanaux, [switch]$Android, [switch]$Apk, [switch]$Gradle, [switch]$Compiler, [switch]$App, [switch]$Exe, [switch]$Reel, [switch]$ApkApp, [switch]$GradleApp, [string]$Filtre = "")

$racine = Split-Path $PSScriptRoot -Parent
$garde = Join-Path $PSScriptRoot "garde-ressources.ps1"
$projet = Join-Path $racine "poc\etafa-core"
$journal = Join-Path $racine "journal-dev"
$suivi = Join-Path $journal "execution-tests.log"
$limite = (Get-Date).AddHours($MaxHeures)

function Note($texte) {
    $ligne = "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') $texte"
    Add-Content -Path $suivi -Value $ligne -Encoding UTF8
    $ligne
}

function Attendre($etape) {
    $motif = ""
    while ((Get-Date) -lt $limite) {
        $sortie = & $garde
        if ($LASTEXITCODE -eq 0) { Note "[$etape] garde-fou AUTORISE : $($sortie[0])"; return $true }
        $nouveau = (($sortie | Select-Object -Skip 2) -join " | ")
        if ($nouveau -ne $motif) { Note "[$etape] attente : $($sortie[0]) :: $nouveau"; $motif = $nouveau }
        Start-Sleep -Seconds $PauseSec
    }
    Note "[$etape] abandon : pas d'autorisation en $MaxHeures h"
    return $false
}

Set-Location $projet
$etapes = @(
    @{ Nom = "check"; Log = "$journal\cargo-check.log"; Cmd = "cargo check --bin feasibility --message-format short" },
    @{ Nom = "tests"; Log = "$journal\cargo-run.log"; Cmd = "cargo run --release --bin feasibility -- $journal\resultats-feasibility.json" }
)
if ($Android) {
    $etapes = @(
        @{ Nom = "android"; Log = "$journal\cargo-android.log"; Cmd = "cargo build --release --target aarch64-linux-android --bin feasibility" }
    )
}

if ($Apk) {
    $app = Join-Path $racine "poc\etafa-tablette"
    $etapes = @(
        @{ Nom = "apk"; Log = "$journal\cargo-apk.log"; Cmd = "(cd /d $app && npm install && cargo tauri icon app-icon.png && cargo tauri android init --ci && cargo tauri android build --debug --target aarch64 --apk)" }
    )
}

if ($Gradle) {
    # Windows refuse les liens symboliques hors mode developpeur : la bibliotheque est copiee dans jniLibs.
    $tauriDir = Join-Path $racine "poc\etafa-tablette\src-tauri"
    $so = "$tauriDir\target\aarch64-linux-android\debug\libetafa_test_lib.so"
    $gradleArgs = "assembleArm64Debug -x rustBuildArm64Debug -PabiList=arm64-v8a -ParchList=arm64 -PtargetList=aarch64 --no-daemon --max-workers=1 --console=plain"
    $etapes = @(
        @{ Nom = "gradle"; Log = "$journal\gradle-apk.log"; Cmd = "(cd /d $tauriDir\gen\android && copy /Y $so app\src\main\jniLibs\arm64-v8a\ && $tauriDir\gen\android\gradlew.bat $gradleArgs)" }
    )
}

if ($DiagCanaux) {
    $etapes = @(
        @{ Nom = "diag"; Log = "$journal\cargo-diag.log"; Cmd = "cargo run --release --bin feasibility -- $journal\diag-canaux.json --diag-canaux" }
    )
}

if ($Compiler) {
    $etapes = @(
        @{ Nom = "compilation"; Log = "$journal\cargo-build.log"; Cmd = "cargo build --release --bin feasibility" }
    )
}

if ($App) {
    # npm --ignore-scripts : aucun script d'installation de paquet n'est execute.
    $front = Join-Path $racine "app"
    $etapes = @(
        @{ Nom = "app-npm"; Log = "$journal\app-npm.log"; Cmd = "(cd /d $front && npm install --ignore-scripts --no-fund --no-audit)" },
        @{ Nom = "app-front"; Log = "$journal\app-front.log"; Cmd = "(cd /d $front && npm run check && npm run build)" },
        @{ Nom = "app-core"; Log = "$journal\app-core-tests.log"; Cmd = "(cd /d $racine && cargo test -p mc-core)" },
        @{ Nom = "app-tauri"; Log = "$journal\app-tauri-check.log"; Cmd = "(cd /d $racine && cargo check -p mc-connect --message-format short)" }
    )
}

if ($Exe) {
    $front = Join-Path $racine "app"
    $etapes = @(
        @{ Nom = "app-exe"; Log = "$journal\app-exe.log"; Cmd = "(cd /d $front && cargo tauri build --debug --no-bundle)" }
    )
}

if ($ApkApp) {
    # Sous Windows sans mode developpeur, le lien vers la bibliotheque peut echouer en fin d'etape : voir -GradleApp.
    $front = Join-Path $racine "app"
    $etapes = @(
        @{ Nom = "app-android-init"; Log = "$journal\app-android-init.log"; Cmd = "(cd /d $front && if not exist src-tauri\gen\android\ (cargo tauri android init --ci))" },
        @{ Nom = "app-android-check"; Log = "$journal\app-android-check.log"; Cmd = "(cd /d $racine && cargo check -p mc-connect --target aarch64-linux-android --message-format short)" },
        @{ Nom = "app-android-build"; Log = "$journal\app-android-build.log"; Cmd = "(cd /d $front && cargo tauri android build --debug --target aarch64 --apk)" }
    )
}

if ($GradleApp) {
    $genAndroid = Join-Path $racine "app\src-tauri\gen\android"
    $so = Join-Path $racine "target\aarch64-linux-android\debug\libmc_connect_lib.so"
    $jni = "app\src\main\jniLibs\arm64-v8a"
    $gradleArgs = "assembleArm64Debug -x rustBuildArm64Debug -PabiList=arm64-v8a -ParchList=arm64 -PtargetList=aarch64 --no-daemon --max-workers=1 --console=plain"
    $etapes = @(
        @{ Nom = "app-gradle"; Log = "$journal\app-gradle.log"; Cmd = "(cd /d $genAndroid && (if not exist $jni\ mkdir $jni) && copy /Y $so $jni\ && $genAndroid\gradlew.bat $gradleArgs)" }
    )
}

if ($Reel) {
    # Variables MC_REEL_* transmises par l'environnement de l'appelant.
    $etapes = @(
        @{ Nom = "app-reel"; Log = "$journal\app-openssh-reel.log"; Cmd = "(cd /d $racine && cargo test -p mc-core --test openssh_reel $Filtre -- --ignored --nocapture --test-threads=1)" }
    )
}

foreach ($e in $etapes) {
    if (-not (Attendre $e.Nom)) { exit 3 }
    Note "[$($e.Nom)] debut : $($e.Cmd)"
    # Le chemin du projet ne contient pas d'espace : pas de guillemets imbriques pour cmd.
    $cmd = "cmd /c `"$($e.Cmd) > $($e.Log) 2>&1`""
    & $garde -Commande $cmd | Out-Null
    $code = $LASTEXITCODE
    Note "[$($e.Nom)] fin, code $code"
    if ($code -ne 0) { exit $code }
}
Note "Termine."
exit 0
