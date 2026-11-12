pragma Singleton
import QtQuick

// Substituto do I18n para o qmltestrunner (sem Quickshell): formata em pt-BR.
QtObject {
    function t(key, vars) { return key === "format.decimal" ? "," : key }
    function plural(key, n, vars) { return key }
    function num(value, digits) { return Number(value).toFixed(digits || 0).replace(".", ",") }
}
