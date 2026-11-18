#!/usr/bin/env bash
# Installa Mia Agent su Linux: compila il binario release, lo mette in
# ~/.local/bin/mia e aggiunge "Mia Agent" al menu applicazioni.
# Uso: ./packaging/install.sh  (dalla root del repo)
# Dati: ~/mia-data. Disinstalla con:
#   rm ~/.local/bin/mia ~/.local/share/applications/mia.desktop ~/.local/share/icons/mia.svg
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null 2>&1; then
    echo "Serve Rust (cargo). Installalo con:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo "poi riavvia il terminale e rilancia questo script."
    exit 1
fi

echo "Compilo Mia (release, qualche minuto)..."
cargo build --release -p mia-ui --bin mia

mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons"
cp target/release/mia "$HOME/.local/bin/mia"
cp packaging/mia.desktop "$HOME/.local/share/applications/mia.desktop"
cp packaging/mia.svg "$HOME/.local/share/icons/mia.svg"

# Percorsi assoluti nel lanciatore (niente dipendenze dal PATH).
sed -i "s|^Exec=.*|Exec=$HOME/.local/bin/mia|" \
    "$HOME/.local/share/applications/mia.desktop"
sed -i "s|^Icon=.*|Icon=$HOME/.local/share/icons/mia.svg|" \
    "$HOME/.local/share/applications/mia.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$HOME/.local/share/applications" || true
fi

echo "Mia installata: cercala nel menu applicazioni come \"Mia Agent\"."
echo "Al primo avvio ti guida lei nel setup del modello, dentro l'app."
echo "I dati stanno in ~/mia-data."
