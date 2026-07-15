# Inicializa el submódulo vendorizado de FlyByWire en un clone limpio del repo.
# Clone eficiente: sin blobs hasta que se necesitan (--filter=blob:none) y
# sparse-checkout solo de los árboles de sistemas (Rust). El A380 es necesario
# porque el Cargo.toml raíz del vendor lo lista como workspace member.
#
# Uso:  .\scripts\bootstrap-vendor.ps1   (desde la raíz del repo)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$vendor = Join-Path $repoRoot "core-rs\vendor\aircraft"

git -C $repoRoot config submodule."core-rs/vendor/aircraft".update checkout

# Init sin checkout para poder configurar partial clone + sparse antes de traer archivos
git -C $repoRoot submodule init
if (-not (Test-Path (Join-Path $vendor ".git"))) {
    $url = git -C $repoRoot config --file .gitmodules submodule."core-rs/vendor/aircraft".url
    git clone --filter=blob:none --sparse --no-checkout $url $vendor
    git -C $repoRoot submodule absorbgitdirs
}

git -C $vendor sparse-checkout set fbw-common/src/wasm fbw-a32nx/src/wasm fbw-a380x/src/wasm

# Checkout exacto del pin registrado en el índice del superproyecto
$pin = (git -C $repoRoot ls-tree HEAD core-rs/vendor/aircraft) -split '\s+' | Select-Object -Index 2
if (-not $pin) {
    # Repo aún sin commit del submódulo: usa el pin staged
    $pin = (git -C $repoRoot ls-files -s core-rs/vendor/aircraft) -split '\s+' | Select-Object -Index 1
}
git -C $vendor fetch --filter=blob:none origin $pin
git -C $vendor checkout --quiet $pin

Write-Host "Vendor listo en $vendor @ $pin"
