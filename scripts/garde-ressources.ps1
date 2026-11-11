# Garde-fou obligatoire avant toute compilation lourde (voir Doc-Ref/03-ressources-compilation.md).
# Sans -Commande : verifie seulement et affiche le verdict.
# Avec -Commande : execute la commande si les conditions sont reunies, a priorite basse et sur 1 job.
# Code de sortie 2 = refus.
param(
    [string]$Commande = "",
    [double]$RamMinGo = 4,
    [double]$DisqueMinGo = 20,
    [int]$CpuMaxPct = 60
)

$refus = @()

$builds = Get-CimInstance Win32_Process -Filter "Name='cargo.exe' OR Name='rustc.exe' OR Name='cl.exe' OR Name='clang.exe' OR Name='gradle.exe'"
foreach ($p in $builds) {
    $ligne = $p.CommandLine
    if ($ligne.Length -gt 90) { $ligne = $ligne.Substring(0, 90) }
    $refus += "Compilation deja en cours : pid $($p.ProcessId) $ligne"
}

$sdk = Get-CimInstance Win32_Process -Filter "Name='java.exe'" | Where-Object { $_.CommandLine -match 'sdkmanager|SdkManagerCli' }
foreach ($p in $sdk) { $refus += "Installation Android en cours : pid $($p.ProcessId)" }

$os = Get-CimInstance Win32_OperatingSystem
$ramGo = [math]::Round($os.FreePhysicalMemory / 1MB, 1)
if ($ramGo -lt $RamMinGo) { $refus += "RAM libre insuffisante : $ramGo Go (minimum $RamMinGo Go)" }

$disqueGo = [math]::Round((Get-PSDrive C).Free / 1GB, 1)
if ($disqueGo -lt $DisqueMinGo) { $refus += "Disque C: insuffisant : $disqueGo Go (minimum $DisqueMinGo Go)" }

$cpu = (Get-CimInstance Win32_Processor | Measure-Object LoadPercentage -Average).Average
if ($cpu -gt $CpuMaxPct) { $refus += "CPU trop charge : $cpu % (maximum $CpuMaxPct %)" }

"RAM libre : $ramGo Go | Disque C: $disqueGo Go | CPU : $cpu %"

if ($refus.Count -gt 0) {
    "VERDICT : REFUS"
    $refus | ForEach-Object { " - $_" }
    exit 2
}

"VERDICT : AUTORISE"
if ($Commande -eq "") { exit 0 }

# Les processus enfants heritent de la priorite basse.
(Get-Process -Id $PID).PriorityClass = [System.Diagnostics.ProcessPriorityClass]::BelowNormal
$env:CARGO_BUILD_JOBS = "1"
# NDK_HOME limite a ce processus et a ses enfants : aucune variable systeme ou utilisateur n'est modifiee.
$ndk = Get-ChildItem "$env:ANDROID_HOME\ndk" -Directory -ErrorAction SilentlyContinue |
    Sort-Object { [version]$_.Name } | Select-Object -Last 1
if ($ndk) {
    $env:NDK_HOME = $ndk.FullName
    $env:ANDROID_NDK_HOME = $ndk.FullName
    $env:ANDROID_NDK_ROOT = $ndk.FullName
    # Outils de la cible ARM64 (API 24 minimum), propres a cette cible et a ce processus.
    $tc = Join-Path $ndk.FullName "toolchains\llvm\prebuilt\windows-x86_64\bin"
    $env:PATH = "$tc;$env:PATH"
    $env:CC_aarch64_linux_android = "$tc\aarch64-linux-android24-clang.cmd"
    $env:CXX_aarch64_linux_android = "$tc\aarch64-linux-android24-clang++.cmd"
    $env:AR_aarch64_linux_android = "$tc\llvm-ar.exe"
    $env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$tc\aarch64-linux-android24-clang.cmd"
}
Invoke-Expression $Commande
exit $LASTEXITCODE
