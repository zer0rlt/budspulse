# Build BudsPulse for Windows (NSIS setup, MSI and a portable exe) into release/Windows.
#   pwsh scripts/build-windows.ps1
# Needs: Rust (MSVC toolchain), Node.js 20+, WebView2 (part of Windows 11).
#
# Release binaries must not carry the builder's home folder: rustc embeds source paths in panic
# messages and the linker stores the full .pdb path. They are remapped / shortened here, and the
# paths are computed at build time, so nothing machine-specific is committed.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot\..").Path
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$out = Join-Path $root 'release\Windows'

# the last matching prefix wins, so the most specific folders go last
$flags = @(
    "--remap-path-prefix=$env:USERPROFILE=~",
    "--remap-path-prefix=$cargoHome=/cargo",
    "--remap-path-prefix=$root=/budspulse",
    '-Clink-arg=/PDBALTPATH:%_PDB%'
)
$env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]0x1f

Push-Location $root
try {
    npm ci --no-audit --no-fund --loglevel=error
    if ($LASTEXITCODE) { throw 'npm ci failed' }
    npx tauri build --bundles nsis,msi
    if ($LASTEXITCODE) { throw 'tauri build failed' }
} finally {
    Pop-Location
    Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
}

$b = Join-Path $root 'src-tauri\target\release'
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item "$b\bundle\nsis\*-setup.exe", "$b\bundle\msi\*.msi" $out -Force
Copy-Item "$b\budspulse.exe" (Join-Path $out 'BudsPulse-portable.exe') -Force

# self-check: the home folder must not appear in the shipped exe
$bytes = [IO.File]::ReadAllBytes((Join-Path $out 'BudsPulse-portable.exe'))
$text = [Text.Encoding]::GetEncoding(28591).GetString($bytes)
if ($text.IndexOf($env:USERPROFILE, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
    throw "the exe still contains $env:USERPROFILE"
}
Get-ChildItem $out | Format-Table Name, Length
