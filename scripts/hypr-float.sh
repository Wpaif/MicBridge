#!/usr/bin/env bash
# Faz a janela Quickshell do MicBridge flutuar no Omarchy: um bloco marcado em
# ~/.config/hypr/bindings.lua dá a ela a tag floating-window, cujas regras do
# próprio Omarchy flutuam, centralizam e dimensionam a janela (875×600).
#
# Uso: hypr-float.sh add|remove
# Fora do Omarchy (sem bindings.lua com a API Lua `o.`) não faz nada. Se a
# configuração passar a ter erros depois da mudança, o arquivo anterior volta.
set -euo pipefail

APP_ID="dev.wilian.MicBridge"
BEGIN="-- micbridge: begin. Escrito pelo install.sh do MicBridge; o uninstall.sh remove o bloco inteiro."
END="-- micbridge: end."
FILE="${XDG_CONFIG_HOME:-$HOME/.config}/hypr/bindings.lua"

has_block() { grep -qxF -e "$BEGIN" "$FILE"; }

# Texto sem o bloco: da linha BEGIN até a END, inclusive, e a linha em branco
# que o `add` pôs antes dela. Adicionar e remover devolve o arquivo original.
without_block() {
  awk -v b="$BEGIN" -v e="$END" '
    held { if ($0 != b) print ""; held = 0 }
    !skip && $0 == "" { held = 1; next }
    $0 == b { skip = 1 }
    !skip { print }
    $0 == e { skip = 0 }
    END { if (held) print "" }
  ' "$FILE"
}

config_errors() { hyprctl configerrors 2>/dev/null || true; }

# Grava o novo conteúdo, recarrega e desfaz se surgirem erros novos.
apply() {
  local content="$1" what="$2" backup before after
  backup="$(mktemp)"
  cp -p "$FILE" "$backup"
  before="$(config_errors)"
  printf '%s\n' "$content" > "$FILE.micbridge.tmp"
  mv "$FILE.micbridge.tmp" "$FILE"
  if ! command -v hyprctl >/dev/null 2>&1 || [[ -z "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
    rm -f "$backup"
    echo "Janela flutuante: $what $FILE; recarregue o Hyprland dentro da sessão (hyprctl reload)."
    return
  fi
  hyprctl reload >/dev/null 2>&1 || true
  after="$(config_errors)"
  if [[ "$after" != "$before" && -n "${after//[[:space:]]/}" ]]; then
    cp -p "$backup" "$FILE"
    rm -f "$backup"
    hyprctl reload >/dev/null 2>&1 || true
    echo "Aviso: a regra de janela flutuante gerou erros no Hyprland e foi desfeita:" >&2
    echo "$after" >&2
    return
  fi
  rm -f "$backup"
  echo "Janela flutuante: $what $FILE e Hyprland recarregado."
}

[[ -f "$FILE" ]] && grep -q '^o\.' "$FILE" || exit 0

case "${1:-}" in
  add)
    has_block && exit 0
    apply "$(cat "$FILE")

$BEGIN
o.window(\"$APP_ID\", { tag = \"+floating-window\" })
$END" "bloco adicionado em"
    ;;
  remove)
    has_block || exit 0
    apply "$(without_block)" "bloco removido de"
    ;;
  *)
    echo "Uso: $0 add|remove" >&2
    exit 1
    ;;
esac
